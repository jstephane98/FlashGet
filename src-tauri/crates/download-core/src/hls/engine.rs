use crate::hls::crypto::{decrypt_aes128_cbc, iv_for_segment};
use crate::hls::playlist::{parse_media, KeyMethod};
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::engine::{DownloadHandle, Outcome};
use crate::CoreError;
use futures_util::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Répertoire de travail temporaire associé à un fichier de destination.
fn work_dir(dest: &Path) -> std::path::PathBuf {
    let mut s = dest.as_os_str().to_owned();
    s.push(".fghls");
    std::path::PathBuf::from(s)
}

/// Lit entièrement un flux d'octets (rate-limité, respecte pause/cancel, cumule la progression).
async fn fetch_to_vec<F: ByteFetcher>(
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

fn emit_segments(
    tx: &mpsc::UnboundedSender<Progress>,
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

/// Télécharge un flux HLS VOD vers `dest` (.mp4) via le moteur Rust + remux ffmpeg.
#[allow(clippy::too_many_arguments)]
pub async fn download_hls<F: ByteFetcher, R: Remuxer>(
    fetcher: &F,
    remux: &R,
    id: &str,
    media_url: &str,
    audio_url: Option<&str>,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    // 1. Récupère + parse la media playlist vidéo.
    let video_pl_bytes = match fetch_to_vec(fetcher, media_url, None, headers, &limiter, &handle).await? {
        Some(b) => b,
        None => return finish_cancelled(&work),
    };
    let video_pl = parse_media(&String::from_utf8_lossy(&video_pl_bytes), media_url);
    if video_pl.is_live {
        return Err(CoreError::Media("flux live non supporté (à venir)".into()));
    }
    // Refuse tout chiffrement non géré.
    for seg in &video_pl.segments {
        if let Some(k) = &seg.key {
            if k.method != KeyMethod::Aes128 {
                return Err(CoreError::Media("chiffrement non supporté (DRM/SAMPLE-AES)".into()));
            }
        }
    }

    // 2. (Optionnel) media playlist audio.
    let audio_pl = if let Some(au) = audio_url {
        let bytes = match fetch_to_vec(fetcher, au, None, headers, &limiter, &handle).await? {
            Some(b) => b,
            None => return finish_cancelled(&work),
        };
        Some(parse_media(&String::from_utf8_lossy(&bytes), au))
    } else {
        None
    };

    let total_segments = (video_pl.segments.len()
        + audio_pl.as_ref().map(|p| p.segments.len()).unwrap_or(0)) as u32;
    let mut done: u32 = 0;
    let mut key_cache: HashMap<String, [u8; 16]> = HashMap::new();

    // 3. Télécharge la piste vidéo (+ init fMP4).
    if let Some(map_uri) = &video_pl.map {
        let init = match fetch_to_vec(fetcher, map_uri, None, headers, &limiter, &handle).await? {
            Some(b) => b,
            None => return finish_cancelled(&work),
        };
        std::fs::write(work.join("v_init.seg"), init)?;
    }
    match download_track(
        fetcher, &video_pl, "v", &work, headers, &limiter, &handle, &progress, id,
        &mut done, total_segments, &mut key_cache,
    )
    .await?
    {
        Outcome::Completed => {}
        Outcome::Paused => {
            emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused);
            return Ok(Outcome::Paused); // work dir conservé pour la reprise
        }
        Outcome::Cancelled => return finish_cancelled(&work),
    }

    // 4. Télécharge la piste audio si présente.
    if let Some(ap) = &audio_pl {
        if let Some(map_uri) = &ap.map {
            let init = match fetch_to_vec(fetcher, map_uri, None, headers, &limiter, &handle).await? {
                Some(b) => b,
                None => return finish_cancelled(&work),
            };
            std::fs::write(work.join("a_init.seg"), init)?;
        }
        match download_track(
            fetcher, ap, "a", &work, headers, &limiter, &handle, &progress, id,
            &mut done, total_segments, &mut key_cache,
        )
        .await?
        {
            Outcome::Completed => {}
            Outcome::Paused => {
                emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused);
                return Ok(Outcome::Paused);
            }
            Outcome::Cancelled => return finish_cancelled(&work),
        }
    }

    // 5. Concatène chaque piste en un fichier unique, puis remux.
    let video_cat = concat_track(&work, "v", video_pl.map.is_some())?;
    let audio_cat = if let Some(ap) = &audio_pl {
        Some(concat_track(&work, "a", ap.map.is_some())?)
    } else {
        None
    };
    remux
        .remux(RemuxInputs { video: video_cat, audio: audio_cat }, dest)
        .await?;

    // 6. Nettoyage + progression finale.
    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(),
        downloaded_bytes: handle.downloaded(),
        total_bytes: None,
        speed_bps: 0,
        status: DownloadStatus::Completed,
        segments_done: Some(total_segments),
        segments_total: Some(total_segments),
    });
    Ok(Outcome::Completed)
}

/// Télécharge tous les segments d'une piste vers `work/<prefix>NNNNN.seg`.
#[allow(clippy::too_many_arguments)]
async fn download_track<F: ByteFetcher>(
    fetcher: &F,
    pl: &crate::hls::playlist::MediaPlaylist,
    prefix: &str,
    work: &Path,
    headers: &RequestHeaders,
    limiter: &Arc<RateLimiter>,
    handle: &DownloadHandle,
    progress: &mpsc::UnboundedSender<Progress>,
    id: &str,
    done: &mut u32,
    total: u32,
    key_cache: &mut HashMap<String, [u8; 16]>,
) -> Result<Outcome, CoreError> {
    for (idx, seg) in pl.segments.iter().enumerate() {
        let seg_path = work.join(format!("{prefix}{idx:05}.seg"));
        if seg_path.exists() {
            *done += 1; // reprise : segment déjà fait
            continue;
        }
        if handle.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        if handle.is_paused() {
            return Ok(Outcome::Paused);
        }
        let raw = match fetch_to_vec(fetcher, &seg.uri, seg.byte_range, headers, limiter, handle).await? {
            Some(b) => b,
            None => return Ok(Outcome::Cancelled),
        };
        let bytes = if let Some(k) = &seg.key {
            let uri = k.uri.as_ref().ok_or_else(|| CoreError::Media("clé AES-128 sans URI".into()))?;
            let key = match key_cache.get(uri) {
                Some(k) => *k,
                None => {
                    let kb = match fetch_to_vec(fetcher, uri, None, headers, limiter, handle).await? {
                        Some(b) => b,
                        None => return Ok(Outcome::Cancelled),
                    };
                    if kb.len() != 16 {
                        return Err(CoreError::Media("clé AES-128 de taille invalide".into()));
                    }
                    let mut arr = [0u8; 16];
                    arr.copy_from_slice(&kb);
                    key_cache.insert(uri.clone(), arr);
                    arr
                }
            };
            let iv = iv_for_segment(k.iv, pl.media_sequence + idx as u64);
            decrypt_aes128_cbc(&raw, &key, &iv)?
        } else {
            raw
        };
        let part = work.join(format!("{prefix}{idx:05}.part"));
        std::fs::write(&part, &bytes)?;
        std::fs::rename(&part, &seg_path)?; // rename atomique
        *done += 1;
        emit_segments(progress, id, handle, *done, total, DownloadStatus::Downloading);
    }
    Ok(Outcome::Completed)
}

/// Concatène `<prefix>_init.seg` (si fMP4) + tous les `<prefix>NNNNN.seg` triés en un fichier.
fn concat_track(work: &Path, prefix: &str, has_init: bool) -> Result<std::path::PathBuf, CoreError> {
    use std::io::Write;
    let out_path = work.join(format!("{prefix}.cat"));
    let mut out = std::fs::File::create(&out_path)?;
    if has_init {
        let init = work.join(format!("{prefix}_init.seg"));
        if init.exists() {
            out.write_all(&std::fs::read(init)?)?;
        }
    }
    let mut segs: Vec<std::path::PathBuf> = std::fs::read_dir(work)?
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

fn finish_cancelled(work: &Path) -> Result<Outcome, CoreError> {
    let _ = std::fs::remove_dir_all(work);
    Ok(Outcome::Cancelled)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::http::ByteFetcher;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use std::collections::HashMap as Map;
    use tempfile::tempdir;

    /// Fetcher qui mappe URL → octets (playlists, segments, clés).
    struct MapFetcher {
        routes: Map<String, Vec<u8>>,
    }
    #[async_trait]
    impl ByteFetcher for MapFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(
            &self,
            url: &str,
            range: Option<(u64, u64)>,
            _h: &RequestHeaders,
        ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = self.routes.get(url).cloned().unwrap_or_default();
            let slice = match range {
                Some((s, e)) => data[s as usize..(e as usize + 1).min(data.len())].to_vec(),
                None => data,
            };
            Ok(stream::iter(vec![Ok(Bytes::from(slice))]).boxed())
        }
    }

    fn unlimited() -> Arc<RateLimiter> {
        Arc::new(RateLimiter::unlimited())
    }

    #[tokio::test]
    async fn downloads_clear_vod_and_remuxes() {
        let media = "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXTINF:1.0,\nseg0.ts\n#EXTINF:1.0,\nseg1.ts\n#EXT-X-ENDLIST\n";
        let mut routes = Map::new();
        routes.insert("https://h/p/index.m3u8".to_string(), media.as_bytes().to_vec());
        routes.insert("https://h/p/seg0.ts".to_string(), b"AAAA".to_vec());
        routes.insert("https://h/p/seg1.ts".to_string(), b"BBBB".to_vec());
        let fetcher = MapFetcher { routes };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download_hls(
            &fetcher, &remux, "id1", "https://h/p/index.m3u8", None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
        )
        .await
        .unwrap();

        assert_eq!(outcome, Outcome::Completed);
        // FakeRemuxer concatène les inputs : video.cat = "AAAABBBB".
        assert_eq!(std::fs::read(&dest).unwrap(), b"AAAABBBB");
        // Le work dir temporaire doit être nettoyé après remux.
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists(), "le work dir doit être nettoyé");
    }

    #[tokio::test]
    async fn rejects_live_stream() {
        let media = "#EXTM3U\n#EXTINF:1.0,\nseg0.ts\n"; // pas d'ENDLIST
        let mut routes = Map::new();
        routes.insert("https://h/p/index.m3u8".to_string(), media.as_bytes().to_vec());
        let fetcher = MapFetcher { routes };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let err = download_hls(
            &fetcher, &remux, "id2", "https://h/p/index.m3u8", None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }
}
