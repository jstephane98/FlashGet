//! Helpers de téléchargement/assemblage de segments partagés par HLS et DASH.
use crate::engine::DownloadHandle;
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
    // Tri lexical == ordre numérique car les indices sont zéro-paddés sur 5 chiffres
    // (limite : 99999 segments, ≈ plusieurs jours de VOD — hors périmètre).
    let mut segs: Vec<PathBuf> = std::fs::read_dir(work)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.starts_with(prefix) && n.ends_with(".seg") && !n.contains("_init"))
                .unwrap_or(false)
        })
        .collect();
    segs.sort();
    for s in segs {
        out.write_all(&std::fs::read(s)?)?;
    }
    out.flush()?;
    Ok(out_path)
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
        segments_total: Some(total),
    });
}

/// Supprime le dossier de travail (annulation).
pub fn finish_cancelled(work: &Path) {
    let _ = std::fs::remove_dir_all(work);
}
