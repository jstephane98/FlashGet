use crate::dash::clearkey::{build_license_request, parse_jwk_response};
use crate::dash::mpd::{parse_mpd, Addressing, Mpd, ProtScheme};
use crate::dash::segments::base_media;
use crate::dash::segments::build_segments;
use crate::dash::select::{pick_audio, video_variants};
use crate::dash::sidx::parse_sidx;
use crate::engine::{DownloadHandle, Outcome};
use crate::hls::remux::{RemuxInputs, Remuxer};
use crate::http::ByteFetcher;
use crate::media_io::{concat_track, emit_segments, fetch_to_vec, finish_cancelled, seg_path, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Sélection persistée d'un download DASH.
#[derive(Debug, Clone, PartialEq)]
pub struct DashSelection {
    pub video_index: u32,
    pub audio_lang: Option<String>,
}

/// Télécharge un flux DASH VOD vers `dest` (.mp4) : segments via le moteur Rust,
/// remux (et déchiffrement CENC ClearKey) via ffmpeg.
#[allow(clippy::too_many_arguments)]
pub async fn download_dash<F: ByteFetcher, R: Remuxer>(
    fetcher: &F,
    remux: &R,
    id: &str,
    mpd_url: &str,
    selection: &DashSelection,
    dest: &Path,
    handle: DownloadHandle,
    limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>,
    headers: &RequestHeaders,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    // 1. Récupère + parse le MPD.
    let mpd_bytes = match fetch_to_vec(fetcher, mpd_url, None, headers, &limiter, &handle).await? {
        Some(b) => b,
        None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    };
    let mpd = parse_mpd(&String::from_utf8_lossy(&mpd_bytes), mpd_url)?;
    if mpd.dynamic {
        return crate::dash::live::download_dash_live(
            fetcher, remux, id, mpd_url, selection, dest,
            handle, limiter, progress, headers,
            Some(mpd), crate::live::RefreshInterval::Derived, crate::live::Clock::System,
        ).await;
    }

    // 2. Résout les Representations vidéo + audio.
    let vlist = video_variants(&mpd);
    let (vcoord, _) = vlist
        .iter()
        .find(|(_, v)| v.index == selection.video_index)
        .or_else(|| vlist.first())
        .ok_or_else(|| CoreError::Media(crate::i18n::tr("aucune variante vidéo", "No video variant").into()))?;
    let video_as = &mpd.periods[vcoord.period].adaptation_sets[vcoord.adaptation];
    let video_repr = &video_as.representations[vcoord.representation];
    let audio_coord = pick_audio(&mpd, selection.audio_lang.as_deref());

    // 3. Clé ClearKey si chiffré (vidéo ou audio).
    let key = resolve_key(fetcher, &mpd, vcoord.adaptation, headers).await?;

    // 4. Plans de segments.
    let vplan = build_segments(video_repr, &video_repr.base, mpd.duration_secs);
    let aplan = match &audio_coord {
        Some(c) => {
            let ar = &mpd.periods[c.period].adaptation_sets[c.adaptation].representations[c.representation];
            Some((ar, build_segments(ar, &ar.base, mpd.duration_secs)))
        }
        None => None,
    };

    let total_segments = (vplan.media.len()
        + aplan.as_ref().map(|(_, p)| p.media.len()).unwrap_or(0)) as u32;
    let mut done: u32 = 0;

    // 5. Télécharge la piste vidéo (résout SegmentBase via sidx si besoin).
    match download_track(fetcher, video_repr, vplan, "v", &work, headers, &limiter, &handle, &progress, id, &mut done, total_segments).await? {
        Outcome::Completed => {}
        Outcome::Paused => { emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused); return Ok(Outcome::Paused); }
        Outcome::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    }

    // 6. Piste audio.
    let has_audio = aplan.is_some();
    if let Some((ar, ap)) = aplan {
        match download_track(fetcher, ar, ap, "a", &work, headers, &limiter, &handle, &progress, id, &mut done, total_segments).await? {
            Outcome::Completed => {}
            Outcome::Paused => { emit_segments(&progress, id, &handle, done, total_segments, DownloadStatus::Paused); return Ok(Outcome::Paused); }
            Outcome::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        }
    }

    // 7. Concat (IO lourde ⇒ thread bloquant) + remux (avec clé éventuelle).
    let work_v = work.clone();
    let video_cat = tokio::task::spawn_blocking(move || concat_track(&work_v, "v", true))
        .await
        .map_err(|e| CoreError::Media(crate::trf!("tâche de concaténation interrompue: {e}", "Concatenation task interrupted: {e}")))??;
    let audio_cat = if has_audio {
        let work_a = work.clone();
        let cat = tokio::task::spawn_blocking(move || concat_track(&work_a, "a", true))
            .await
            .map_err(|e| CoreError::Media(crate::trf!("tâche de concaténation interrompue: {e}", "Concatenation task interrupted: {e}")))??;
        Some(cat)
    } else {
        None
    };
    remux.remux(RemuxInputs { video: video_cat, audio: audio_cat, decryption_key: key }, dest).await?;

    // 8. Nettoyage + progression finale.
    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_segments), segments_total: Some(total_segments), parts: None,
    });
    Ok(Outcome::Completed)
}

#[allow(clippy::too_many_arguments)]
async fn download_track<F: ByteFetcher>(
    fetcher: &F,
    repr: &crate::dash::mpd::Representation,
    mut plan: crate::dash::segments::SegmentPlan,
    prefix: &str,
    work: &Path,
    headers: &RequestHeaders,
    limiter: &Arc<RateLimiter>,
    handle: &DownloadHandle,
    progress: &mpsc::UnboundedSender<Progress>,
    id: &str,
    done: &mut u32,
    total: u32,
) -> Result<Outcome, CoreError> {
    // Init.
    if let Some(init) = &plan.init {
        let bytes = match fetch_to_vec(fetcher, &init.url, init.byte_range, headers, limiter, handle).await? {
            Some(b) => b, None => return Ok(Outcome::Cancelled),
        };
        std::fs::write(work.join(format!("{prefix}_init.seg")), bytes)?;
    }
    // SegmentBase : lire le sidx pour obtenir les médias.
    if let Addressing::Base(b) = &repr.addressing {
        if let Some((s, e)) = b.index_range {
            let idx = match fetch_to_vec(fetcher, &b.url, Some((s, e)), headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Outcome::Cancelled),
            };
            let ranges = parse_sidx(&idx, e + 1)?;
            plan.media = base_media(b, &ranges);
        }
    }

    if matches!(repr.addressing, Addressing::Base(_)) && plan.media.is_empty() {
        return Err(CoreError::Media(crate::i18n::tr("SegmentBase sans indexRange : aucun segment média", "SegmentBase without indexRange: no media segment").into()));
    }

    for (idx, seg) in plan.media.iter().enumerate() {
        if seg_path(work, prefix, idx).exists() {
            *done += 1;
            continue;
        }
        if handle.is_cancelled() { return Ok(Outcome::Cancelled); }
        if handle.is_paused() { return Ok(Outcome::Paused); }
        let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await? {
            Some(b) => b, None => return Ok(Outcome::Cancelled),
        };
        write_segment(work, prefix, idx, &bytes)?;
        *done += 1;
        emit_segments(progress, id, handle, *done, total, DownloadStatus::Downloading);
    }
    Ok(Outcome::Completed)
}

/// Récupère la clé ClearKey (hex) si l'AdaptationSet vidéo est chiffré, sinon None.
// Hypothèse 4b-1 : un seul ContentProtection (clé ClearKey unique) couvre toutes
// les pistes ; on n'inspecte que l'AdaptationSet vidéo (cas CENC courant).
pub(crate) async fn resolve_key<F: ByteFetcher>(
    fetcher: &F,
    mpd: &Mpd,
    video_adaptation: usize,
    headers: &RequestHeaders,
) -> Result<Option<String>, CoreError> {
    let cp = match &mpd.periods[0].adaptation_sets[video_adaptation].content_protection {
        Some(cp) => cp,
        None => return Ok(None),
    };
    let kid = match &cp.default_kid {
        Some(k) => k.clone(),
        None => return Err(CoreError::Media(crate::i18n::tr("contenu chiffré sans default_KID", "Encrypted content without default_KID").into())),
    };
    match (&cp.scheme, &cp.clearkey_laurl) {
        (ProtScheme::ClearKey, Some(laurl)) | (ProtScheme::Cenc, Some(laurl)) => {
            let body = build_license_request(&kid)?;
            let resp = fetcher.post(laurl, body.as_bytes(), "application/json", headers).await?;
            Ok(Some(parse_jwk_response(&String::from_utf8_lossy(&resp))?))
        }
        _ => Err(CoreError::Media(crate::i18n::tr("DRM non supporté (ClearKey requis)", "Unsupported DRM (ClearKey required)").into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use futures_util::StreamExt;
    use std::collections::HashMap as Map;
    use tempfile::tempdir;

    struct MapFetcher { routes: Map<String, Vec<u8>>, post_resp: Option<Vec<u8>> }
    #[async_trait]
    impl ByteFetcher for MapFetcher {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None, content_type: None, status: 200 })
        }
        async fn fetch_range(&self, url: &str, range: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = self.routes.get(url).cloned().unwrap_or_default();
            let slice = match range {
                Some((s, e)) => data[s as usize..(e as usize + 1).min(data.len())].to_vec(),
                None => data,
            };
            Ok(stream::iter(vec![Ok(Bytes::from(slice))]).boxed())
        }
        async fn post(&self, _url: &str, _b: &[u8], _ct: &str, _h: &RequestHeaders) -> Result<Vec<u8>, CoreError> {
            Ok(self.post_resp.clone().unwrap_or_default())
        }
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }

    const MPD: &str = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT4S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate initialization="https://h/p/vinit.mp4" media="https://h/p/v$Number$.m4s" timescale="1000" duration="2000" startNumber="1"/>
    <Representation id="v0" bandwidth="800000" width="640" height="360"/>
  </AdaptationSet></Period>
</MPD>"#;

    #[tokio::test]
    async fn downloads_clear_dash_and_remuxes() {
        let mut routes = Map::new();
        routes.insert("https://h/p/m.mpd".into(), MPD.as_bytes().to_vec());
        routes.insert("https://h/p/vinit.mp4".into(), b"INIT".to_vec());
        routes.insert("https://h/p/v1.m4s".into(), b"AAAA".to_vec());
        routes.insert("https://h/p/v2.m4s".into(), b"BBBB".to_vec());
        let fetcher = MapFetcher { routes, post_resp: None };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let sel = DashSelection { video_index: 0, audio_lang: None };

        let outcome = download_dash(&fetcher, &remux, "id1", "https://h/p/m.mpd", &sel, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap();

        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"INITAAAABBBB");
        let work = format!("{}.fghls", dest.to_string_lossy());
        assert!(!std::path::Path::new(&work).exists());
    }

    #[tokio::test]
    async fn dynamic_mpd_is_recorded_not_rejected() {
        // type=dynamic SANS minimumUpdatePeriod => le moteur live capture la
        // fenêtre puis termine (pas de refresh attendu).
        let mpd = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" mediaPresentationDuration="PT2S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1"><SegmentTimeline><S t="0" d="1"/><S d="1"/></SegmentTimeline></SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#;
        let mut routes = Map::new();
        routes.insert("https://h/p/m.mpd".to_string(), mpd.as_bytes().to_vec());
        routes.insert("https://h/p/seg-0.m4s".to_string(), b"AA".to_vec());
        routes.insert("https://h/p/seg-1.m4s".to_string(), b"BB".to_vec());
        let fetcher = MapFetcher { routes, post_resp: None };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let sel = DashSelection { video_index: 0, audio_lang: None };
        let outcome = download_dash(&fetcher, &remux, "dyn1", "https://h/p/m.mpd", &sel, &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default()).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }
}
