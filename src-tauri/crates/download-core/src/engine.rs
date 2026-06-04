use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::resume::{self, ResumeState};
use crate::segments::compute_segments;
use crate::CoreError;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::mpsc;

pub const DEFAULT_SEGMENTS: u32 = 8;

/// Contrôle partagé d'un téléchargement en cours.
#[derive(Clone, Default)]
pub struct DownloadHandle {
    pause: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    downloaded: Arc<AtomicU64>,
}

impl DownloadHandle {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn pause(&self) {
        self.pause.store(true, Ordering::SeqCst);
    }
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::SeqCst)
    }
}

/// Issue d'un run du moteur.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    Completed,
    Paused,
    Cancelled,
}

/// Télécharge `url` vers `dest`. Émet la progression (octets cumulés) sur `progress`.
/// Reprend depuis le sidecar s'il existe. Multi-segments si le serveur le supporte.
pub async fn download<F: ByteFetcher>(
    fetcher: &F,
    id: &str,
    url: &str,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
    use futures_util::future::join_all;
    use futures_util::StreamExt;

    let probe = fetcher.probe(url, headers).await?;
    let probe_total = probe.total_bytes.unwrap_or(0);

    // Construire (ou recharger) l'état des segments.
    // FIX I1 — trust a valid sidecar even when the fresh probe reports unknown size.
    let loaded = resume::load(dest);
    let (mut state, total) = match loaded {
        // trust sidecar when it has a real size and the probe either agrees or is unknown(0)
        Some(s) if s.total_bytes > 0 && (probe_total == 0 || probe_total == s.total_bytes) => {
            let t = s.total_bytes;
            (s, t)
        }
        _ => {
            let total = probe_total;
            let segs = if probe.supports_ranges && total > 0 {
                compute_segments(total, DEFAULT_SEGMENTS)
            } else if total > 0 {
                vec![(0, total - 1)]
            } else {
                vec![] // taille inconnue : un seul flux non-segmenté
            };
            let state = ResumeState {
                url: url.to_string(),
                total_bytes: total,
                segments: segs.into_iter().map(|(s, e)| (s, e, 0u64)).collect(),
            };
            (state, total)
        }
    };

    // Pré-allouer le fichier de destination à la bonne taille (si connue).
    // Only preallocate when the file does not already exist — a partial resume
    // must keep its existing bytes and must not be truncated by set_len.
    {
        let needs_prealloc = total > 0 && !dest.exists();
        let f = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(dest)?;
        if needs_prealloc {
            f.set_len(total)?;
        }
    }

    // Amorcer le compteur cumulé avec ce qui est déjà téléchargé (reprise).
    let already: u64 = state.segments.iter().map(|(_, _, d)| *d).sum();
    handle.downloaded.store(already, Ordering::SeqCst);

    // Cas taille inconnue : un seul flux séquentiel, pas de segmentation.
    if state.segments.is_empty() {
        let mut file = tokio::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(dest)
            .await?;
        let mut stream = fetcher.fetch_range(url, None, headers).await?;
        while let Some(chunk) = stream.next().await {
            if handle.cancel.load(Ordering::SeqCst) {
                resume::clear(dest);
                let _ = std::fs::remove_file(dest);
                return Ok(Outcome::Cancelled);
            }
            if handle.pause.load(Ordering::SeqCst) {
                file.flush().await?;
                // No resume::save here: total_bytes==0 for unknown-size streams,
                // so the saved state would always be rejected on reload anyway.
                emit(&progress, id, &handle, total, DownloadStatus::Paused);
                return Ok(Outcome::Paused);
            }
            let chunk = chunk?;
            limiter.acquire(chunk.len() as u64).await;
            file.write_all(&chunk).await?;
            handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
            emit(&progress, id, &handle, total, DownloadStatus::Downloading);
        }
        file.flush().await?;
        resume::clear(dest);
        emit(&progress, id, &handle, total, DownloadStatus::Completed);
        return Ok(Outcome::Completed);
    }

    // Cas segmenté : une tâche par segment incomplet, écriture à l'offset.
    // On exécute les futures CONCURREMMENT sans `tokio::spawn` : comme elles sont
    // awaitées dans la même async fn, on peut emprunter `fetcher: &F` directement
    // (pas de borne `'static`, donc pas besoin d'`Arc<F>`).
    let mut seg_trackers: Vec<(usize, Arc<AtomicU64>)> = Vec::new();
    let mut futs = Vec::new();

    for (idx, (start, end, done)) in state.segments.iter().enumerate() {
        let seg_start = *start;
        let seg_end = *end;
        let seg_done = *done;

        if seg_start + seg_done > seg_end {
            continue; // segment déjà fini — ne pas l'ajouter aux trackers
        }

        // Progrès par segment (suivi pour mettre à jour le sidecar après le join).
        // Only track segments that will actually run.
        let seg_progress = Arc::new(AtomicU64::new(seg_done));
        seg_trackers.push((idx, seg_progress.clone()));

        let resume_from = seg_start + seg_done;

        let fetcher_ref = fetcher;
        let headers_ref = headers;
        let url_owned = url.to_string();
        let dest_owned = dest.to_path_buf();
        let handle = handle.clone();
        let progress = progress.clone();
        let id_owned = id.to_string();
        let limiter = limiter.clone();

        let fut = async move {
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .open(&dest_owned)
                .await?;
            file.seek(std::io::SeekFrom::Start(resume_from)).await?;
            let mut stream = fetcher_ref
                .fetch_range(&url_owned, Some((resume_from, seg_end)), headers_ref)
                .await?;
            while let Some(chunk) = stream.next().await {
                if handle.cancel.load(Ordering::SeqCst) {
                    return Ok::<(usize, Outcome), CoreError>((idx, Outcome::Cancelled));
                }
                if handle.pause.load(Ordering::SeqCst) {
                    file.flush().await?;
                    return Ok((idx, Outcome::Paused));
                }
                let chunk = chunk?;
                limiter.acquire(chunk.len() as u64).await;
                file.write_all(&chunk).await?;
                seg_progress.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                emit(&progress, &id_owned, &handle, total, DownloadStatus::Downloading);
            }
            file.flush().await?;
            Ok((idx, Outcome::Completed))
        };
        futs.push(fut);
    }

    // Exécuter toutes les tâches de segment concurremment.
    let results = join_all(futs).await;

    let mut outcome = Outcome::Completed;
    for r in results {
        match r? {
            (_, Outcome::Completed) => {}
            (_, Outcome::Paused) => {
                if outcome == Outcome::Completed {
                    outcome = Outcome::Paused;
                }
            }
            (_, Outcome::Cancelled) => {
                outcome = Outcome::Cancelled;
            }
        }
    }

    // Mettre à jour l'état des segments avec la progression observée.
    for (idx, tracker) in seg_trackers {
        let done = tracker.load(Ordering::SeqCst);
        let (s, e, _) = state.segments[idx];
        state.segments[idx] = (s, e, done.min(e - s + 1));
    }

    match outcome {
        Outcome::Completed => {
            resume::clear(dest);
            emit(&progress, id, &handle, total, DownloadStatus::Completed);
            Ok(Outcome::Completed)
        }
        Outcome::Paused => {
            resume::save(dest, &state)?;
            emit(&progress, id, &handle, total, DownloadStatus::Paused);
            Ok(Outcome::Paused)
        }
        Outcome::Cancelled => {
            resume::clear(dest);
            let _ = std::fs::remove_file(dest);
            Ok(Outcome::Cancelled)
        }
    }
}

fn emit(
    tx: &mpsc::UnboundedSender<Progress>,
    id: &str,
    handle: &DownloadHandle,
    total: u64,
    status: DownloadStatus,
) {
    let _ = tx.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: if total > 0 { Some(total) } else { None },
        speed_bps: 0, // la vitesse est calculée côté appelant (Task 8)
        status,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RequestHeaders;
    use crate::rate_limiter::RateLimiter;
    use crate::tests::fake_fetcher::FakeFetcher;
    use tempfile::tempdir;

    fn unlimited() -> std::sync::Arc<RateLimiter> {
        std::sync::Arc::new(RateLimiter::unlimited())
    }

    fn collect_dest(dir: &tempfile::TempDir) -> std::path::PathBuf {
        dir.path().join("out.bin")
    }

    #[tokio::test]
    async fn downloads_full_file_multisegment() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let fetcher = FakeFetcher::new(data.clone(), true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download(&fetcher, "id1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();

        assert_eq!(outcome, Outcome::Completed);
        let written = std::fs::read(&dest).unwrap();
        assert_eq!(written, data);
    }

    #[tokio::test]
    async fn falls_back_to_single_stream_when_no_ranges() {
        let data: Vec<u8> = (0..200u8).collect();
        let fetcher = FakeFetcher::new(data.clone(), false);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, _rx) = mpsc::unbounded_channel();

        download(&fetcher, "id2", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();

        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn emits_progress() {
        let data: Vec<u8> = vec![1u8; 300];
        let fetcher = FakeFetcher::new(data, true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, mut rx) = mpsc::unbounded_channel();

        download(&fetcher, "id3", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();

        let mut last = 0u64;
        while let Ok(p) = rx.try_recv() {
            last = p.downloaded_bytes.max(last);
        }
        assert_eq!(last, 300);
    }

    #[tokio::test]
    async fn resume_after_pause_completes_file() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        // 1er run : on met en pause immédiatement après le démarrage.
        let fetcher = FakeFetcher::new(data.clone(), true);
        let handle = DownloadHandle::new();
        handle.pause(); // pause avant le premier chunk => crée le sidecar
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "id4", "http://x/f", &dest, handle, unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Paused);

        // 2e run : reprise jusqu'à complétion.
        let fetcher2 = FakeFetcher::new(data.clone(), true);
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome2 = download(&fetcher2, "id4", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default())
            .await
            .unwrap();
        assert_eq!(outcome2, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    /// Guard for FIX 2: a second download() call on a completed dest must not
    /// corrupt the file via set_len (idempotent re-run).
    #[tokio::test]
    async fn resume_preserves_partial_bytes() {
        let data: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        // First run: download to completion.
        let fetcher = FakeFetcher::new(data.clone(), true);
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "idr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        let first_bytes = std::fs::read(&dest).unwrap();
        assert_eq!(first_bytes, data, "first download must match data");

        // Second run: same dest already exists — set_len must NOT be called
        // (needs_prealloc == false because dest exists), so the file bytes survive.
        let fetcher2 = FakeFetcher::new(data.clone(), true);
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome2 =
            download(&fetcher2, "idr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default())
                .await
                .unwrap();
        assert_eq!(outcome2, Outcome::Completed);
        let second_bytes = std::fs::read(&dest).unwrap();
        assert_eq!(second_bytes, data, "re-run must not corrupt already-correct bytes");
    }

    #[tokio::test]
    async fn throttle_makes_engine_wait() {
        use crate::rate_limiter::Clock;
        use std::sync::Mutex;
        struct Fake { now: Mutex<f64>, slept: Mutex<f64> }
        #[async_trait::async_trait]
        impl Clock for Fake {
            fn now_secs(&self) -> f64 { *self.now.lock().unwrap() }
            async fn sleep(&self, s: f64) {
                *self.now.lock().unwrap() += s;
                *self.slept.lock().unwrap() += s;
            }
        }
        let clock = std::sync::Arc::new(Fake { now: Mutex::new(0.0), slept: Mutex::new(0.0) });
        let limiter = std::sync::Arc::new(RateLimiter::with_clock(1000, clock.clone()));

        let data: Vec<u8> = vec![7u8; 2000]; // 2000 octets à 1000 o/s => ~2 s simulées
        let fetcher = FakeFetcher::new(data, true);
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.bin");
        let (tx, _rx) = mpsc::unbounded_channel();

        download(&fetcher, "t", "http://x/f", &dest, DownloadHandle::new(), limiter, tx, &RequestHeaders::default())
            .await
            .unwrap();

        let slept = *clock.slept.lock().unwrap();
        assert!(slept >= 1.9, "le moteur aurait dû dormir ~2 s (simulées), got {slept}");
    }

    /// Guard for FIX I1: a valid sidecar (total>0) must be trusted on resume even
    /// when the fresh probe can't report the size (total_bytes None).
    #[tokio::test]
    async fn resume_trusts_sidecar_when_probe_size_unknown() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        // 1er run : probe révèle la taille, on met en pause avant le premier chunk
        // => le sidecar est écrit avec total>0 et progression 0.
        let fetcher = FakeFetcher::new(data.clone(), true);
        let handle = DownloadHandle::new();
        handle.pause();
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "ids", "http://x/f", &dest, handle, unlimited(), tx, &RequestHeaders::default())
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Paused);
        assert!(
            resume::load(&dest).is_some(),
            "a sidecar must exist after the paused first run"
        );

        // 2e run : probe NE révèle PAS la taille (None). Le moteur doit faire
        // confiance au sidecar et terminer correctement.
        let mut fetcher2 = FakeFetcher::new(data.clone(), true);
        fetcher2.probe_reports_size = false;
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome2 = download(&fetcher2, "ids", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default())
            .await
            .unwrap();
        assert_eq!(outcome2, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }
}
