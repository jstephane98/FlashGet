//! Helpers de téléchargement/assemblage de segments partagés par HLS et DASH.
use crate::engine::DownloadHandle;
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use futures_util::StreamExt;
use std::path::{Path, PathBuf};

/// Répertoire de travail temporaire associé à un fichier de destination.
pub fn work_dir(dest: &Path) -> PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fghls");
    PathBuf::from(s)
}

/// Lit entièrement un flux d'octets (rate-limité, respecte pause/cancel, cumule la progression).
/// `None` ⇒ annulé.
pub async fn fetch_to_vec<F: ByteFetcher>(
    fetcher: &F,
    url: &str,
    range: Option<(u64, u64)>,
    headers: &RequestHeaders,
    limiter: &RateLimiter,
    handle: &DownloadHandle,
) -> Result<Option<Vec<u8>>, CoreError> {
    let mut stream = fetcher.fetch_range(url, range, headers).await?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        if handle.is_cancelled() {
            return Ok(None);
        }
        let chunk = chunk?;
        limiter.acquire(chunk.len() as u64).await;
        buf.extend_from_slice(&chunk);
        handle.add_downloaded(chunk.len() as u64);
    }
    Ok(Some(buf))
}

/// Écrit un segment via `.part` → rename atomique `<prefix>NNNNN.seg`.
pub fn write_segment(work: &Path, prefix: &str, idx: usize, bytes: &[u8]) -> Result<(), CoreError> {
    let part = work.join(format!("{prefix}{idx:05}.part"));
    let seg = work.join(format!("{prefix}{idx:05}.seg"));
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, &seg)?;
    Ok(())
}

/// Chemin final d'un segment (pour tester sa présence à la reprise).
pub fn seg_path(work: &Path, prefix: &str, idx: usize) -> PathBuf {
    work.join(format!("{prefix}{idx:05}.seg"))
}

/// Concatène `<prefix>_init.seg` (si présent) + tous les `<prefix>NNNNN.seg` triés.
pub fn concat_track(work: &Path, prefix: &str, has_init: bool) -> Result<PathBuf, CoreError> {
    use std::io::Write;
    let out_path = work.join(format!("{prefix}.cat"));
    let mut out = std::fs::File::create(&out_path)?;
    if has_init {
        let init = work.join(format!("{prefix}_init.seg"));
        if init.exists() {
            out.write_all(&std::fs::read(init)?)?;
        }
    }
    // Tri NUMÉRIQUE par numéro de séquence (suffixe entier) : robuste quelle que
    // soit la largeur (VOD padde sur 5 ; live peut dépasser 99999 segments).
    let mut segs: Vec<(u64, PathBuf)> = std::fs::read_dir(work)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_string();
            if name.contains("_init") {
                return None;
            }
            // <prefix><digits>.seg → digits (strip_prefix/suffix : pas d'arithmétique d'index).
            let digits = name.strip_prefix(prefix)?.strip_suffix(".seg")?;
            let seq: u64 = digits.parse().ok()?;
            Some((seq, p))
        })
        .collect();
    segs.sort_by_key(|(seq, _)| *seq);
    for (_, s) in segs {
        out.write_all(&std::fs::read(s)?)?;
    }
    out.flush()?;
    Ok(out_path)
}

/// Concatène la piste vidéo (et audio si présente) puis remux en `dest` via
/// ffmpeg. `audio_has_init = None` ⇒ pas de piste audio. La concat (IO lourde)
/// tourne sur un thread bloquant.
pub async fn finalize_and_remux<R: Remuxer>(
    work: &Path,
    video_has_init: bool,
    audio_has_init: Option<bool>,
    decryption_key: Option<String>,
    remux: &R,
    dest: &Path,
) -> Result<(), CoreError> {
    let work_v = work.to_path_buf();
    let video = tokio::task::spawn_blocking(move || concat_track(&work_v, "v", video_has_init))
        .await
        .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??;
    let audio = if let Some(a_has_init) = audio_has_init {
        let work_a = work.to_path_buf();
        Some(
            tokio::task::spawn_blocking(move || concat_track(&work_a, "a", a_has_init))
                .await
                .map_err(|e| CoreError::Media(format!("tâche de concaténation interrompue: {e}")))??,
        )
    } else {
        None
    };
    remux.remux(RemuxInputs { video, audio, decryption_key }, dest).await
}

/// Émet une progression par segments.
pub fn emit_segments(
    tx: &tokio::sync::mpsc::UnboundedSender<Progress>,
    id: &str,
    handle: &DownloadHandle,
    done: u32,
    total: u32,
    status: DownloadStatus,
) {
    let _ = tx.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: None,
        speed_bps: 0,
        status,
        segments_done: Some(done),
        segments_total: Some(total), parts: None,
    });
}

/// Supprime le dossier de travail (annulation).
pub fn finish_cancelled(work: &Path) {
    let _ = std::fs::remove_dir_all(work);
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn concat_orders_segments_numerically_not_lexically() {
        let dir = tempdir().unwrap();
        let w = dir.path();
        // 99999 puis 100000 : ordre numérique correct = 99999 avant 100000,
        // alors que le tri lexical placerait "v100000" avant "v99999".
        std::fs::write(w.join("v99999.seg"), b"A").unwrap();
        std::fs::write(w.join("v100000.seg"), b"B").unwrap();
        let out = concat_track(w, "v", false).unwrap();
        assert_eq!(std::fs::read(out).unwrap(), b"AB");
    }

    #[tokio::test]
    async fn finalize_concats_and_passes_key() {
        use crate::hls::remux::fake::FakeRemuxer;
        let dir = tempdir().unwrap();
        let w = dir.path();
        std::fs::write(w.join("v0.seg"), b"V0").unwrap();
        std::fs::write(w.join("v1.seg"), b"V1").unwrap();
        std::fs::write(w.join("a0.seg"), b"A0").unwrap();
        let out = w.join("out.mp4");
        let remux = FakeRemuxer::default();
        finalize_and_remux(w, false, Some(false), Some("deadbeef".into()), &remux, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"V0V1A0");
        let calls = remux.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].decryption_key.as_deref(), Some("deadbeef"));
        assert!(calls[0].audio.is_some());
    }

    #[tokio::test]
    async fn finalize_video_only_no_key() {
        use crate::hls::remux::fake::FakeRemuxer;
        let dir = tempdir().unwrap();
        let w = dir.path();
        std::fs::write(w.join("v0.seg"), b"XX").unwrap();
        let out = w.join("out.mp4");
        let remux = FakeRemuxer::default();
        finalize_and_remux(w, false, None, None, &remux, &out).await.unwrap();
        assert_eq!(std::fs::read(&out).unwrap(), b"XX");
        let calls = remux.calls.lock().unwrap();
        assert!(calls[0].audio.is_none());
        assert!(calls[0].decryption_key.is_none());
    }
}
