//! Enregistrement HLS live : boucle de rafraîchissement de la media playlist,
//! capture incrémentale par numéro de séquence absolue, finalisation par
//! concat + remux. Voir spec 2026-06-04-phase-4b2a-hls-live-design.md.
use crate::hls::crypto::{decrypt_aes128_cbc, iv_for_segment};
use crate::hls::playlist::{parse_media, KeyMethod, MediaPlaylist};
use crate::hls::remux::Remuxer;
use crate::http::ByteFetcher;
use crate::media_io::{fetch_to_vec, finish_cancelled, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::engine::{DownloadHandle, Outcome};
use crate::CoreError;
use crate::live::responsive_sleep;
use crate::live::RefreshInterval;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Issue d'une passe de capture d'une piste sur un rafraîchissement.
enum Step { Continue, Stopped, Cancelled }

/// État persistant d'une piste (vidéo ou audio) entre rafraîchissements.
struct LiveTrack {
    prefix: &'static str,
    next_seq: u64,           // prochaine séquence absolue à capturer
    init_handled: bool,      // EXT-X-MAP déjà traité
    has_init: bool,          // un init existe (pour la concat)
    has_any: bool,           // au moins un segment capturé
    key_cache: HashMap<String, [u8; 16]>,
}

impl LiveTrack {
    fn new(prefix: &'static str) -> Self {
        Self { prefix, next_seq: 0, init_handled: false, has_init: false, has_any: false, key_cache: HashMap::new() }
    }

    /// Capture les segments de `pl` dont la séquence absolue ≥ `next_seq`.
    #[allow(clippy::too_many_arguments)]
    async fn capture<F: ByteFetcher>(
        &mut self, pl: &MediaPlaylist, fetcher: &F, work: &Path, headers: &RequestHeaders,
        limiter: &Arc<RateLimiter>, handle: &DownloadHandle,
        progress: &mpsc::UnboundedSender<Progress>, id: &str, total_done: &mut u32,
    ) -> Result<Step, CoreError> {
        if !self.init_handled {
            if let Some(map_uri) = &pl.map {
                let init = match fetch_to_vec(fetcher, map_uri, None, headers, limiter, handle).await? {
                    Some(b) => b, None => return Ok(Step::Cancelled),
                };
                std::fs::write(work.join(format!("{}_init.seg", self.prefix)), init)?;
                self.has_init = true;
            }
            self.init_handled = true;
        }
        for (idx, seg) in pl.segments.iter().enumerate() {
            let abs = pl.media_sequence + idx as u64;
            if abs < self.next_seq { continue; }
            if handle.is_cancelled() { return Ok(Step::Cancelled); }
            if handle.is_stopped() { return Ok(Step::Stopped); }
            let raw = match fetch_to_vec(fetcher, &seg.uri, seg.byte_range, headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Step::Cancelled),
            };
            let bytes = if let Some(k) = &seg.key {
                let uri = k.uri.as_ref().ok_or_else(|| CoreError::Media("clé AES-128 sans URI".into()))?;
                let key = match self.key_cache.get(uri) {
                    Some(k) => *k,
                    None => {
                        let kb = match fetch_to_vec(fetcher, uri, None, headers, limiter, handle).await? {
                            Some(b) => b, None => return Ok(Step::Cancelled),
                        };
                        if kb.len() != 16 { return Err(CoreError::Media("clé AES-128 de taille invalide".into())); }
                        let mut arr = [0u8; 16];
                        arr.copy_from_slice(&kb);
                        self.key_cache.insert(uri.clone(), arr);
                        arr
                    }
                };
                let iv = iv_for_segment(k.iv, abs);
                decrypt_aes128_cbc(&raw, &key, &iv)?
            } else { raw };
            write_segment(work, self.prefix, abs as usize, &bytes)?;
            self.next_seq = abs + 1;
            self.has_any = true;
            *total_done += 1;
            let _ = progress.send(Progress {
                id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
                speed_bps: 0, status: DownloadStatus::Recording,
                segments_done: Some(*total_done), segments_total: None, parts: None,
            });
        }
        Ok(Step::Continue)
    }
}

/// Refuse tout chiffrement non AES-128 (garde héritée du VOD HLS).
fn reject_unsupported_crypto(pl: &MediaPlaylist) -> Result<(), CoreError> {
    for seg in &pl.segments {
        if let Some(k) = &seg.key {
            if k.method != KeyMethod::Aes128 {
                return Err(CoreError::Media("chiffrement non supporté (DRM/SAMPLE-AES)".into()));
            }
        }
    }
    Ok(())
}

/// Enregistre un flux HLS live vers `dest` (.mp4). Capture la fenêtre disponible
/// puis les nouveaux segments à chaque rafraîchissement, jusqu'à `stop()` ou la
/// fin de flux (`ENDLIST`). `cancel()` jette la capture.
#[allow(clippy::too_many_arguments)]
pub async fn download_hls_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    media_url: &str, audio_url: Option<&str>, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<MediaPlaylist>, refresh: RefreshInterval,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    let mut v = LiveTrack::new("v");
    let mut a = LiveTrack::new("a");
    let mut total_done: u32 = 0;
    let mut pending_video = initial;
    let mut first_pass = true;

    loop {
        // --- piste vidéo ---
        let vpl = match pending_video.take() {
            Some(pl) => pl,
            None => match fetch_to_vec(fetcher, media_url, None, headers, &limiter, &handle).await? {
                Some(b) => parse_media(&String::from_utf8_lossy(&b), media_url),
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            },
        };
        reject_unsupported_crypto(&vpl)?;
        if first_pass && vpl.segments.is_empty() && vpl.map.is_none() {
            finish_cancelled(&work);
            return Err(CoreError::Media("playlist HLS sans segment (master au lieu de media ?)".into()));
        }
        first_pass = false;
        match v.capture(&vpl, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
            Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            Step::Stopped => break,
            Step::Continue => {}
        }
        let video_ended = !vpl.is_live;

        // --- piste audio (optionnelle) ---
        let mut audio_ended = true;
        if let Some(au) = audio_url {
            let apl = match fetch_to_vec(fetcher, au, None, headers, &limiter, &handle).await? {
                Some(b) => parse_media(&String::from_utf8_lossy(&b), au),
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            };
            reject_unsupported_crypto(&apl)?;
            match a.capture(&apl, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
                Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
                Step::Stopped => break,
                Step::Continue => {}
            }
            audio_ended = !apl.is_live;
        }

        if handle.is_stopped() { break; }
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        if video_ended && audio_ended { break; } // fin de flux sur toutes les pistes

        let interval = refresh.compute(vpl.target_duration);
        responsive_sleep(interval, &handle).await;
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        // stop : la boucle reprend et capturera une dernière fois si la playlist a
        // bougé, puis cassera au `is_stopped()` ci-dessus — acceptable.
    }

    // --- finalisation : concat + remux (helper partagé) ---
    crate::media_io::finalize_and_remux(
        &work,
        v.has_init,
        a.has_any.then_some(a.has_init),
        None,
        remux,
        dest,
    )
    .await?;

    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_done), segments_total: None, parts: None,
    });
    Ok(Outcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::http::ByteFetcher;
    use crate::model::{ProbeResult, RequestHeaders};
    use crate::rate_limiter::RateLimiter;
    use crate::engine::Outcome;
    use crate::CoreError;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use futures_util::StreamExt;
    use std::collections::HashMap as Map;
    use std::sync::{Arc, Mutex};
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    /// Fetcher où l'URL de playlist renvoie des corps SUCCESSIFS (fenêtre qui
    /// glisse), et les segments/clés viennent d'une map fixe. Peut poser
    /// stop/cancel après un nombre donné de fetchs de segment.
    struct EvolvingFetcher {
        playlist_url: String,
        playlists: Vec<String>,
        calls: Mutex<usize>,
        segs: Map<String, Vec<u8>>,
        handle: Option<DownloadHandle>,
        stop_after_segs: usize, // 0 = jamais
        cancel_after_segs: usize,
        seg_fetches: Arc<Mutex<usize>>,
    }
    #[async_trait]
    impl ByteFetcher for EvolvingFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None, content_type: None })
        }
        async fn fetch_range(&self, url: &str, _r: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = if url == self.playlist_url {
                let mut n = self.calls.lock().unwrap();
                let body = self.playlists[(*n).min(self.playlists.len() - 1)].clone();
                *n += 1;
                body.into_bytes()
            } else {
                let d = self.segs.get(url).cloned().unwrap_or_default();
                let mut c = self.seg_fetches.lock().unwrap();
                *c += 1;
                if let Some(h) = &self.handle {
                    if self.stop_after_segs != 0 && *c == self.stop_after_segs { h.stop(); }
                    if self.cancel_after_segs != 0 && *c == self.cancel_after_segs { h.cancel(); }
                }
                d
            };
            Ok(stream::iter(vec![Ok(Bytes::from(data))]).boxed())
        }
    }

    fn pl(seq: u64, segs: &[&str], endlist: bool) -> String {
        let mut s = format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{seq}\n");
        for u in segs { s.push_str(&format!("#EXTINF:1.0,\n{u}\n")); }
        if endlist { s.push_str("#EXT-X-ENDLIST\n"); }
        s
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }

    #[tokio::test]
    async fn captures_window_dedups_and_finalizes_on_endlist() {
        let base = "https://h/p/";
        let playlists = vec![
            pl(0, &["s0.ts", "s1.ts"], false),
            pl(1, &["s1.ts", "s2.ts"], false),       // fenêtre glisse : s1 déjà capturé
            pl(2, &["s2.ts", "s3.ts"], true),         // ENDLIST
        ];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB"),("s2.ts","CC"),("s3.ts","DD")] {
            segs.insert(format!("{base}{n}"), b.as_bytes().to_vec());
        }
        let seg_fetches = Arc::new(Mutex::new(0));
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0, seg_fetches: seg_fetches.clone(),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();

        let outcome = download_hls_live(
            &fetcher, &remux, "L1", &format!("{base}live.m3u8"), None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();

        assert_eq!(outcome, Outcome::Completed);
        // s0..s3 capturés une fois chacun, dans l'ordre.
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABBCCDD");
        assert_eq!(*seg_fetches.lock().unwrap(), 4, "aucun segment re-téléchargé");
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists(), "work dir nettoyé");
    }

    #[tokio::test]
    async fn manual_stop_finalizes_partial() {
        let base = "https://h/p/";
        let playlists = vec![pl(0, &["s0.ts", "s1.ts"], false)];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB")] { segs.insert(format!("{base}{n}"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: Some(h.clone()), stop_after_segs: 2, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L2", &format!("{base}live.m3u8"), None, &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }

    #[tokio::test]
    async fn cancel_discards_work_dir() {
        let base = "https://h/p/";
        let playlists = vec![pl(0, &["s0.ts", "s1.ts"], false)];
        let mut segs = Map::new();
        for (n, b) in [("s0.ts","AA"),("s1.ts","BB")] { segs.insert(format!("{base}{n}"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists, calls: Mutex::new(0),
            segs, handle: Some(h.clone()), stop_after_segs: 0, cancel_after_segs: 1,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L3", &format!("{base}live.m3u8"), None, &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(!dest.exists());
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists());
    }

    #[tokio::test]
    async fn decrypts_aes128_live_with_key_rotation() {
        use crate::hls::crypto::iv_for_segment;
        use cbc::cipher::{block_padding::Pkcs7, BlockEncryptMut, KeyIvInit};
        type Enc = cbc::Encryptor<aes::Aes128>;
        let base = "https://h/p/";
        let key0 = [1u8; 16];
        let key1 = [2u8; 16];
        // playlist : s0 sous key0 puis s1 sous key1 (rotation), ENDLIST.
        let body = format!(
            "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"{base}k0.bin\"\n#EXTINF:1.0,\ns0.ts\n\
#EXT-X-KEY:METHOD=AES-128,URI=\"{base}k1.bin\"\n#EXTINF:1.0,\ns1.ts\n#EXT-X-ENDLIST\n");
        // Chiffre avec PKCS7 (decrypt_aes128_cbc retire le PKCS7), IV dérivé de
        // la séquence absolue comme le fait le moteur.
        let enc = |plain: &[u8], key: &[u8; 16], seq: u64| {
            let iv = iv_for_segment(None, seq);
            Enc::new(key.into(), (&iv).into()).encrypt_padded_vec_mut::<Pkcs7>(plain)
        };
        let mut segs = Map::new();
        segs.insert(format!("{base}live.m3u8"), body.into_bytes());
        segs.insert(format!("{base}k0.bin"), key0.to_vec());
        segs.insert(format!("{base}k1.bin"), key1.to_vec());
        segs.insert(format!("{base}s0.ts"), enc(b"HELLO-VIDEO-0", &key0, 0));
        segs.insert(format!("{base}s1.ts"), enc(b"HELLO-VIDEO-1", &key1, 1));
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"),
            playlists: vec![String::from_utf8(segs[&format!("{base}live.m3u8")].clone()).unwrap()],
            calls: Mutex::new(0), segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L4", &format!("{base}live.m3u8"), None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"HELLO-VIDEO-0HELLO-VIDEO-1");
    }

    #[tokio::test]
    async fn captures_separate_audio_track() {
        let base = "https://h/p/";
        let vbody = pl(0, &["v0.ts", "v1.ts"], true);
        let abody = pl(0, &["a0.ts", "a1.ts"], true);
        let mut segs = Map::new();
        segs.insert(format!("{base}audio.m3u8"), abody.into_bytes());
        for (n, b) in [("v0.ts","V0"),("v1.ts","V1"),("a0.ts","A0"),("a1.ts","A1")] {
            segs.insert(format!("{base}{n}"), b.as_bytes().to_vec());
        }
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}video.m3u8"), playlists: vec![vbody], calls: Mutex::new(0),
            segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L5", &format!("{base}video.m3u8"),
            Some(&format!("{base}audio.m3u8")), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        // FakeRemuxer concatène vidéo puis audio.
        assert_eq!(std::fs::read(&dest).unwrap(), b"V0V1A0A1");
    }

    #[tokio::test]
    async fn prepends_fmp4_init_map() {
        let base = "https://h/p/";
        let body = format!("#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:0\n\
#EXT-X-MAP:URI=\"{base}init.mp4\"\n#EXTINF:1.0,\ns0.ts\n#EXTINF:1.0,\ns1.ts\n#EXT-X-ENDLIST\n");
        let mut segs = Map::new();
        for (n, b) in [("init.mp4","INIT"),("s0.ts","AA"),("s1.ts","BB")] {
            segs.insert(format!("{base}{n}"), b.as_bytes().to_vec());
        }
        let fetcher = EvolvingFetcher {
            playlist_url: format!("{base}live.m3u8"), playlists: vec![body], calls: Mutex::new(0),
            segs, handle: None, stop_after_segs: 0, cancel_after_segs: 0,
            seg_fetches: Arc::new(Mutex::new(0)),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_hls_live(
            &fetcher, &remux, "L6", &format!("{base}live.m3u8"), None, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        // init (EXT-X-MAP) préfixé puis les 2 segments.
        assert_eq!(std::fs::read(&dest).unwrap(), b"INITAABB");
    }
}
