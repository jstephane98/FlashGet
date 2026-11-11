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

/// Connexions (segments) par fichier par défaut, quand aucun réglage n'est fourni.
pub const DEFAULT_SEGMENTS: u32 = 8;

/// Contrôle partagé d'un téléchargement en cours.
#[derive(Clone, Default)]
pub struct DownloadHandle {
    pause: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
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
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
    }
    pub fn is_stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    pub fn downloaded(&self) -> u64 {
        self.downloaded.load(Ordering::SeqCst)
    }
    pub fn is_paused(&self) -> bool {
        self.pause.load(Ordering::SeqCst)
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }
    pub fn add_downloaded(&self, n: u64) {
        self.downloaded.fetch_add(n, Ordering::SeqCst);
    }
    /// Remet le compteur à `n` (nouvelle tentative d'un outil qui rapporte un cumul).
    pub fn set_downloaded(&self, n: u64) {
        self.downloaded.store(n, Ordering::SeqCst);
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
/// Reprend depuis le sidecar s'il existe. Multi-segments (`segments` connexions, ramené
/// à 1 au minimum) si le serveur le supporte ; un sidecar existant garde son découpage.
/// En cas d'échec d'un segment, la progression est enregistrée dans le sidecar avant de
/// renvoyer l'erreur : une nouvelle tentative reprend là où les segments se sont arrêtés.
#[allow(clippy::too_many_arguments)]
pub async fn download<F: ByteFetcher>(
    fetcher: &F,
    id: &str,
    url: &str,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
    segments: u32,
) -> Result<Outcome, CoreError> {
    use futures_util::future::join_all;
    use futures_util::StreamExt;

    let probe = fetcher.probe(url, headers).await?;
    // Une réponse d'erreur au sondage (page 503, 403…) ne décrit pas le fichier : on
    // échoue avant de toucher au fichier ou au sidecar d'une tentative précédente.
    // 416 fait exception, comme à l'analyse du lien : le sondage demande `bytes=0-0`, qu'un
    // serveur refuse pour un fichier vide ; la taille est alors inconnue (0) et le flux
    // unique termine avec un fichier vide.
    if probe.status >= 400 && probe.status != 416 {
        return Err(if probe.status == 404 || probe.status == 410 {
            CoreError::NotFound(format!("HTTP {} pour {url}", probe.status))
        } else {
            CoreError::Http { status: probe.status, message: format!("HTTP {} pour {url}", probe.status) }
        });
    }
    let probe_total = probe.total_bytes.unwrap_or(0);

    // Construire (ou recharger) l'état des segments.
    // FIX I1 — trust a valid sidecar even when the fresh probe reports unknown size.
    // Sans support des Range, une reprise est impossible : le serveur renverrait le corps
    // entier (200) pour `bytes=done-end`, écrit à l'offset `done`. On ignore donc toute
    // progression enregistrée et on repart de 0.
    // Un sidecar n'est fiable que si le fichier existe avec exactement la taille annoncée ;
    // sinon (fichier absent, tronqué ou réalloué) on le supprime et on repart de 0.
    let loaded = if probe.supports_ranges { resume::load(dest) } else { None };
    let loaded = match loaded {
        Some(s) if std::fs::metadata(dest).map(|m| m.len() == s.total_bytes).unwrap_or(false) => Some(s),
        Some(_) => {
            resume::clear(dest);
            None
        }
        None => None,
    };
    let mut resuming = false;
    let (mut state, total) = match loaded {
        // trust sidecar when it has a real size and the probe either agrees or is unknown(0)
        Some(s) if s.total_bytes > 0 && (probe_total == 0 || probe_total == s.total_bytes) => {
            let t = s.total_bytes;
            resuming = true;
            (s, t)
        }
        _ => {
            // Nouveau départ : un fichier présent va être réalloué ou tronqué, il ne doit
            // jamais garder un sidecar qui le décrit.
            if dest.exists() {
                resume::clear(dest);
            }
            let total = probe_total;
            let segs = if probe.supports_ranges && total > 0 {
                compute_segments(total, segments.max(1))
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
    // Une reprise garde les octets existants (pas de set_len) ; un nouveau départ sur un
    // fichier déjà présent (sidecar ignoré, p. ex. serveur sans Range) le remet à `total`
    // octets, pour ne jamais laisser de queue d'un essai précédent.
    {
        let needs_prealloc = total > 0
            && (!dest.exists()
                || (!resuming && std::fs::metadata(dest).map(|m| m.len() != total).unwrap_or(true)));
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
                emit(&progress, id, &handle, total, DownloadStatus::Paused, None);
                return Ok(Outcome::Paused);
            }
            let chunk = chunk?;
            limiter.acquire(chunk.len() as u64).await;
            file.write_all(&chunk).await?;
            handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
            emit(&progress, id, &handle, total, DownloadStatus::Downloading, None);
        }
        file.flush().await?;
        resume::clear(dest);
        emit(&progress, id, &handle, total, DownloadStatus::Completed, None);
        return Ok(Outcome::Completed);
    }

    // Cas segmenté : une tâche par segment incomplet, écriture à l'offset.
    // On exécute les futures CONCURREMMENT sans `tokio::spawn` : comme elles sont
    // awaitées dans la même async fn, on peut emprunter `fetcher: &F` directement
    // (pas de borne `'static`, donc pas besoin d'`Arc<F>`).
    // Compteurs (longueur, octets faits) de TOUS les segments, pour Progress.parts.
    let all_parts: Arc<Vec<(u64, Arc<AtomicU64>)>> = Arc::new(
        state
            .segments
            .iter()
            .map(|(s, e, d)| (e - s + 1, Arc::new(AtomicU64::new(*d))))
            .collect(),
    );
    let mut seg_trackers: Vec<usize> = Vec::new();
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
        let seg_progress = all_parts[idx].1.clone();
        seg_trackers.push(idx);
        let parts_owned = all_parts.clone();

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
                let chunk = match chunk {
                    Ok(c) => c,
                    Err(e) => {
                        // Les octets déjà comptés doivent être sur le disque avant que
                        // le sidecar ne les déclare faits.
                        file.flush().await?;
                        return Err(e);
                    }
                };
                limiter.acquire(chunk.len() as u64).await;
                file.write_all(&chunk).await?;
                seg_progress.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                handle.downloaded.fetch_add(chunk.len() as u64, Ordering::SeqCst);
                emit(&progress, &id_owned, &handle, total, DownloadStatus::Downloading, Some(parts_owned.as_slice()));
            }
            file.flush().await?;
            Ok((idx, Outcome::Completed))
        };
        futs.push(fut);
    }

    // Exécuter toutes les tâches de segment concurremment.
    let results = join_all(futs).await;

    let mut outcome = Outcome::Completed;
    let mut first_err: Option<CoreError> = None;
    let mut all_errors_transient = true;
    for r in results {
        match r {
            Err(e) => {
                all_errors_transient &= e.is_transient();
                if first_err.is_none() {
                    first_err = Some(e);
                }
            }
            Ok((_, Outcome::Completed)) => {}
            Ok((_, Outcome::Paused)) => {
                if outcome == Outcome::Completed {
                    outcome = Outcome::Paused;
                }
            }
            Ok((_, Outcome::Cancelled)) => {
                outcome = Outcome::Cancelled;
            }
        }
    }

    // Mettre à jour l'état des segments avec la progression observée.
    for idx in seg_trackers {
        let done = all_parts[idx].1.load(Ordering::SeqCst);
        let (s, e, _) = state.segments[idx];
        state.segments[idx] = (s, e, done.min(e - s + 1));
    }

    // Échec d'un segment : une annulation l'emporte (rien à garder) ; sinon on enregistre la
    // progression (si c'est sûr) pour que la tentative suivante reparte de là.
    if let Some(err) = first_err {
        if outcome == Outcome::Cancelled || handle.cancel.load(Ordering::SeqCst) {
            resume::clear(dest);
            let _ = std::fs::remove_file(dest);
            return Ok(Outcome::Cancelled);
        }
        if save_sidecar_after_error(probe.supports_ranges, all_errors_transient) {
            if let Err(se) = resume::save(dest, &state) {
                eprintln!("engine: sidecar non enregistré après l'échec de {id}: {se}");
            }
        }
        return Err(err);
    }

    match outcome {
        Outcome::Completed => {
            resume::clear(dest);
            emit(&progress, id, &handle, total, DownloadStatus::Completed, Some(all_parts.as_slice()));
            Ok(Outcome::Completed)
        }
        Outcome::Paused => {
            resume::save(dest, &state)?;
            emit(&progress, id, &handle, total, DownloadStatus::Paused, Some(all_parts.as_slice()));
            Ok(Outcome::Paused)
        }
        Outcome::Cancelled => {
            resume::clear(dest);
            let _ = std::fs::remove_file(dest);
            Ok(Outcome::Cancelled)
        }
    }
}

/// Après un échec de segment, enregistrer la progression observée ?
/// - Serveur sans Range : non, la reprise est impossible (on repartira de 0).
/// - Une erreur non passagère (disque notamment) : non, on garde le sidecar précédent. Une
///   écriture `write_all` réussie peut n'être que mise en tampon ; son échec ne remonte que
///   plus tard, et les octets comptés ne sont alors pas fiables.
/// - Uniquement des erreurs passagères (réseau, HTTP 5xx) : oui. Le segment fautif a vidé
///   son tampon (`flush`) avant de rendre l'erreur, ses octets comptés sont sur le disque.
fn save_sidecar_after_error(supports_ranges: bool, all_errors_transient: bool) -> bool {
    supports_ranges && all_errors_transient
}

fn emit(
    tx: &mpsc::UnboundedSender<Progress>,
    id: &str,
    handle: &DownloadHandle,
    total: u64,
    status: DownloadStatus,
    parts: Option<&[(u64, Arc<AtomicU64>)]>,
) {
    let parts = parts.filter(|p| p.len() >= 2).map(|p| {
        p.iter()
            .map(|(len, done)| {
                let len = (*len).max(1);
                (done.load(Ordering::SeqCst).min(len) as f64 / len as f64) as f32
            })
            .collect()
    });
    let _ = tx.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: if total > 0 { Some(total) } else { None },
        speed_bps: 0, // la vitesse est calculée côté appelant (Task 8)
        status,
        segments_done: None,
        segments_total: None,
        parts,
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

        let outcome = download(&fetcher, "id1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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

        download(&fetcher, "id2", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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

        download(&fetcher, "id3", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();

        let mut last = 0u64;
        while let Ok(p) = rx.try_recv() {
            last = p.downloaded_bytes.max(last);
        }
        assert_eq!(last, 300);
    }

    async fn run_collect(fetcher: &FakeFetcher, dest: &std::path::Path, handle: DownloadHandle) -> Vec<Progress> {
        let (tx, mut rx) = mpsc::unbounded_channel();
        download(fetcher, "p1", "http://x/f", dest, handle, unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();
        std::iter::from_fn(|| rx.try_recv().ok()).collect()
    }

    #[tokio::test]
    async fn parts_reported_per_segment() {
        let fetcher = FakeFetcher::new(vec![7u8; 800], true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let all = run_collect(&fetcher, &dest, DownloadHandle::new()).await;
        let last = all.last().unwrap();
        assert_eq!(last.status, DownloadStatus::Completed);
        let parts = last.parts.as_ref().expect("parts attendus");
        assert_eq!(parts.len(), DEFAULT_SEGMENTS as usize);
        assert!(parts.iter().all(|f| (*f - 1.0).abs() < f32::EPSILON));
        assert!(all.iter().filter_map(|p| p.parts.as_ref()).flatten().all(|f| (0.0..=1.0).contains(f)));
    }

    #[tokio::test]
    async fn segments_count_follows_argument() {
        let fetcher = FakeFetcher::new(vec![7u8; 800], true);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let (tx, mut rx) = mpsc::unbounded_channel();
        download(&fetcher, "p4", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        let all: Vec<Progress> = std::iter::from_fn(|| rx.try_recv().ok()).collect();
        let last = all.last().unwrap();
        assert_eq!(last.status, DownloadStatus::Completed);
        assert_eq!(last.parts.as_ref().expect("parts attendus").len(), 4);
        assert_eq!(std::fs::read(&dest).unwrap(), vec![7u8; 800]);
    }

    /// Une coupure réseau en cours de route laisse un fichier de reprise à jour :
    /// la tentative suivante repart de là et termine un fichier intact.
    #[tokio::test]
    async fn network_failure_saves_resume_state_for_retry() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        let mut failing = FakeFetcher::new(data.clone(), true);
        failing.fail_at = Some(300); // segments 0 (0..199) et 1 (200..399) : 1 coupé à 300
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download(&failing, "rt", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(err.is_transient(), "coupure réseau attendue, obtenu {err:?}");

        let st = resume::load(&dest).expect("fichier de reprise attendu après l'échec");
        let done: Vec<u64> = st.segments.iter().map(|(_, _, d)| *d).collect();
        assert_eq!(done, vec![200, 100, 0, 0]);

        let fetcher = FakeFetcher::new(data.clone(), true);
        let (tx2, mut rx2) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "rt", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        let first = std::iter::from_fn(|| rx2.try_recv().ok()).next().unwrap();
        assert!(first.downloaded_bytes > 300, "la reprise doit partir des 300 octets déjà écrits");
    }

    /// Serveur sans Range (il renvoie toujours le corps entier) : pas de sidecar après une
    /// coupure, et un sidecar ancien est ignoré. La tentative suivante repart de 0 et donne
    /// un fichier d'exactement `total` octets.
    #[tokio::test]
    async fn non_range_server_retry_restarts_from_zero() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        let mut failing = FakeFetcher::new(data.clone(), false);
        failing.ignore_ranges = true;
        failing.fail_at = Some(300);
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download(&failing, "nr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(err.is_transient());
        assert!(resume::load(&dest).is_none(), "aucune reprise possible sans Range : pas de sidecar");

        // Sidecar ancien qui annonce 300 octets faits + fichier trop long (essai précédent).
        resume::save(&dest, &ResumeState { url: "http://x/f".into(), total_bytes: 800, segments: vec![(0, 799, 300)] }).unwrap();
        std::fs::write(&dest, vec![0xAAu8; 1100]).unwrap();

        let mut fetcher = FakeFetcher::new(data.clone(), false);
        fetcher.ignore_ranges = true;
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "nr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        let written = std::fs::read(&dest).unwrap();
        assert_eq!(written.len(), 800);
        assert_eq!(written, data);
    }

    /// Erreur disque (non passagère) : le sidecar précédent est conservé tel quel, les
    /// octets comptés pendant l'essai fautif ne sont pas déclarés faits.
    #[tokio::test]
    async fn io_error_keeps_previous_sidecar() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        let mut net = FakeFetcher::new(data.clone(), true);
        net.fail_at = Some(300);
        let (tx, _rx) = mpsc::unbounded_channel();
        download(&net, "io", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        let before: Vec<u64> = resume::load(&dest).unwrap().segments.iter().map(|(_, _, d)| *d).collect();
        assert_eq!(before, vec![200, 100, 0, 0]);

        let mut disk = FakeFetcher::new(data.clone(), true);
        disk.fail_at = Some(500);
        disk.fail_with_io = true;
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let err = download(&disk, "io", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Io(_)), "obtenu {err:?}");
        let after: Vec<u64> = resume::load(&dest).unwrap().segments.iter().map(|(_, _, d)| *d).collect();
        assert_eq!(after, before, "le sidecar ne doit pas être réécrit après une erreur disque");

        let fetcher = FakeFetcher::new(data.clone(), true);
        let (tx3, _rx3) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "io", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx3, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[test]
    fn sidecar_saved_after_error_only_when_safe() {
        assert!(save_sidecar_after_error(true, true));
        assert!(!save_sidecar_after_error(false, true), "sans Range");
        assert!(!save_sidecar_after_error(true, false), "erreur non passagère (disque)");
        assert!(!save_sidecar_after_error(false, false));
    }

    #[tokio::test]
    async fn no_parts_for_unknown_size() {
        // Sans Range : un seul segment.
        let fetcher = FakeFetcher::new(vec![1u8; 300], false);
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let all = run_collect(&fetcher, &dest, DownloadHandle::new()).await;
        assert!(!all.is_empty());
        assert!(all.iter().all(|p| p.parts.is_none()));

        // Taille inconnue : flux non segmenté.
        let mut fetcher = FakeFetcher::new(vec![1u8; 300], true);
        fetcher.probe_reports_size = false;
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let all = run_collect(&fetcher, &dest, DownloadHandle::new()).await;
        assert!(!all.is_empty());
        assert!(all.iter().all(|p| p.parts.is_none()));
    }

    /// Régression : un sondage sans Range suivi d'un échec ne doit pas laisser un ancien
    /// sidecar valider un fichier tronqué lors d'une reprise ultérieure.
    #[tokio::test]
    async fn stale_sidecar_not_reused_after_error_page_probe() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        let mut first = FakeFetcher::new(data.clone(), true);
        first.fail_at = Some(300);
        let (tx, _rx) = mpsc::unbounded_channel();
        download(&first, "c1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(resume::load(&dest).is_some());

        // Page d'erreur : sans Range, petite taille, flux en échec.
        let mut bad = FakeFetcher::new(vec![b'x'; 50], false);
        bad.fail_at = Some(10);
        let (tx2, _rx2) = mpsc::unbounded_channel();
        download(&bad, "c1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(resume::load(&dest).is_none(), "le sidecar d'un fichier réalloué doit disparaître");

        let good = FakeFetcher::new(data.clone(), true);
        let (tx3, _rx3) = mpsc::unbounded_channel();
        let outcome = download(&good, "c1", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx3, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn probe_503_is_transient_and_leaves_files_untouched() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let mut first = FakeFetcher::new(data.clone(), true);
        first.fail_at = Some(300);
        let (tx, _rx) = mpsc::unbounded_channel();
        download(&first, "c2", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        let file_before = std::fs::read(&dest).unwrap();
        let side_before = resume::load(&dest).unwrap();

        let mut down = FakeFetcher::new(vec![b'x'; 20], false);
        down.probe_status = 503;
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let err = download(&down, "c2", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::Http { status: 503, .. }), "obtenu {err:?}");
        assert!(err.is_transient());
        assert_eq!(std::fs::read(&dest).unwrap(), file_before);
        assert_eq!(resume::load(&dest).unwrap(), side_before);
    }

    #[tokio::test]
    async fn probe_404_is_not_found() {
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let mut f = FakeFetcher::new(vec![1u8; 20], false);
        f.probe_status = 404;
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download(&f, "c3", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap_err();
        assert!(matches!(err, CoreError::NotFound(_)), "obtenu {err:?}");
        assert!(!dest.exists());
    }

    /// Fichier vide : le serveur répond 416 au sondage `bytes=0-0` ; le téléchargement
    /// aboutit à un fichier vide au lieu d'une erreur.
    #[tokio::test]
    async fn probe_416_on_empty_file_completes() {
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        let mut f = FakeFetcher::new(Vec::new(), false);
        f.probe_status = 416;
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&f, "c6", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn sidecar_with_mismatched_file_length_is_discarded() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        resume::save(&dest, &ResumeState { url: "http://x/f".into(), total_bytes: 800, segments: vec![(0, 799, 500)] }).unwrap();
        std::fs::write(&dest, vec![0u8; 100]).unwrap();

        let f = FakeFetcher::new(data.clone(), true);
        let (tx, mut rx) = mpsc::unbounded_channel();
        download(&f, "c4", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        let first = std::iter::from_fn(|| rx.try_recv().ok()).next().unwrap();
        assert!(first.downloaded_bytes <= 200, "repart de 0, pas des 500 octets du sidecar périmé");
    }

    #[tokio::test]
    async fn orphan_sidecar_without_file_is_discarded() {
        let data: Vec<u8> = vec![9u8; 400];
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);
        resume::save(&dest, &ResumeState { url: "http://x/f".into(), total_bytes: 400, segments: vec![(0, 399, 400)] }).unwrap();
        let f = FakeFetcher::new(data.clone(), true);
        let (tx, _rx) = mpsc::unbounded_channel();
        download(&f, "c5", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), 4)
            .await
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }

    #[tokio::test]
    async fn parts_reflect_resumed_segments() {
        let data: Vec<u8> = (0..=255u8).cycle().take(800).collect();
        let dir = tempdir().unwrap();
        let dest = collect_dest(&dir);

        let handle = DownloadHandle::new();
        handle.pause();
        let fetcher = FakeFetcher::new(data.clone(), true);
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download(&fetcher, "p1", "http://x/f", &dest, handle, unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Paused);

        let mut st = resume::load(&dest).unwrap();
        let (s, e, _) = st.segments[0];
        st.segments[0] = (s, e, e - s + 1);
        resume::save(&dest, &st).unwrap();

        let all = run_collect(&fetcher, &dest, DownloadHandle::new()).await;
        let first = all.iter().find(|p| p.status == DownloadStatus::Downloading).unwrap();
        let parts = first.parts.as_ref().expect("parts attendus");
        assert!((parts[0] - 1.0).abs() < f32::EPSILON);
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
        let outcome = download(&fetcher, "id4", "http://x/f", &dest, handle, unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();
        assert_eq!(outcome, Outcome::Paused);

        // 2e run : reprise jusqu'à complétion.
        let fetcher2 = FakeFetcher::new(data.clone(), true);
        let (tx2, _rx2) = mpsc::unbounded_channel();
        let outcome2 = download(&fetcher2, "id4", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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
        let outcome = download(&fetcher, "idr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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
            download(&fetcher2, "idr", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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

        download(&fetcher, "t", "http://x/f", &dest, DownloadHandle::new(), limiter, tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();

        let slept = *clock.slept.lock().unwrap();
        assert!(slept >= 1.9, "le moteur aurait dû dormir ~2 s (simulées), got {slept}");
    }

    #[test]
    fn stop_flag_independent_from_cancel_and_pause() {
        let h = DownloadHandle::new();
        assert!(!h.is_stopped());
        h.stop();
        assert!(h.is_stopped());
        // stop n'affecte ni cancel ni pause
        assert!(!h.is_cancelled());
        assert!(!h.is_paused());
        // un clone partage le drapeau
        let h2 = h.clone();
        assert!(h2.is_stopped());
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
        let outcome = download(&fetcher, "ids", "http://x/f", &dest, handle, unlimited(), tx, &RequestHeaders::default(), DEFAULT_SEGMENTS)
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
        let outcome2 = download(&fetcher2, "ids", "http://x/f", &dest, DownloadHandle::new(), unlimited(), tx2, &RequestHeaders::default(), DEFAULT_SEGMENTS)
            .await
            .unwrap();
        assert_eq!(outcome2, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), data);
    }
}
