//! Enregistrement DASH dynamic live (SegmentTimeline) : refresh du MPD, capture
//! incrémentale dédupliquée par URL, finalisation concat + remux (ClearKey via
//! ffmpeg). Voir spec 2026-06-04-phase-4b2b-dash-live-design.md.
use crate::dash::engine::{resolve_key, DashSelection};
use crate::dash::mpd::{parse_mpd, Addressing, Mpd};
use crate::dash::segments::{build_segments, SegmentPlan};
use crate::dash::select::{pick_audio, video_variants, ReprCoord};
use crate::engine::{DownloadHandle, Outcome};
use crate::hls::remux::Remuxer;
use crate::http::ByteFetcher;
use crate::live::{responsive_sleep, RefreshInterval};
use crate::media_io::{fetch_to_vec, finalize_and_remux, finish_cancelled, work_dir, write_segment};
use crate::model::{DownloadStatus, Progress, RequestHeaders};
use crate::rate_limiter::RateLimiter;
use crate::CoreError;
use std::collections::HashSet;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Issue d'une passe de capture d'une piste.
enum Step { Continue, Stopped, Cancelled }

/// État d'une piste (vidéo/audio) entre rafraîchissements.
struct DashLiveTrack {
    prefix: &'static str,
    seen: HashSet<String>,
    counter: usize,
    init_done: bool,
    has_init: bool,
    has_any: bool,
}

impl DashLiveTrack {
    fn new(prefix: &'static str) -> Self {
        Self { prefix, seen: HashSet::new(), counter: 0, init_done: false, has_init: false, has_any: false }
    }

    #[allow(clippy::too_many_arguments)]
    async fn capture<F: ByteFetcher>(
        &mut self, plan: &SegmentPlan, fetcher: &F, work: &Path, headers: &RequestHeaders,
        limiter: &Arc<RateLimiter>, handle: &DownloadHandle,
        progress: &mpsc::UnboundedSender<Progress>, id: &str, total_done: &mut u32,
    ) -> Result<Step, CoreError> {
        if !self.init_done {
            if let Some(init) = &plan.init {
                let bytes = match fetch_to_vec(fetcher, &init.url, init.byte_range, headers, limiter, handle).await? {
                    Some(b) => b, None => return Ok(Step::Cancelled),
                };
                std::fs::write(work.join(format!("{}_init.seg", self.prefix)), bytes)?;
                self.has_init = true;
            }
            self.init_done = true;
        }
        for seg in &plan.media {
            if self.seen.contains(&seg.url) { continue; }
            if handle.is_cancelled() { return Ok(Step::Cancelled); }
            if handle.is_stopped() { return Ok(Step::Stopped); }
            let bytes = match fetch_to_vec(fetcher, &seg.url, seg.byte_range, headers, limiter, handle).await? {
                Some(b) => b, None => return Ok(Step::Cancelled),
            };
            write_segment(work, self.prefix, self.counter, &bytes)?;
            self.counter += 1;
            self.seen.insert(seg.url.clone());
            self.has_any = true;
            *total_done += 1;
            let _ = progress.send(Progress {
                id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
                speed_bps: 0, status: DownloadStatus::Recording,
                segments_done: Some(*total_done), segments_total: None,
            });
        }
        Ok(Step::Continue)
    }
}

/// Coordonnées + clé résolues une fois au 1ᵉʳ rafraîchissement.
struct Resolved {
    vcoord: ReprCoord,
    acoord: Option<ReprCoord>,
    key: Option<String>,
}

/// Enregistre un flux DASH dynamic live (SegmentTimeline) vers `dest` (.mp4).
#[allow(clippy::too_many_arguments)]
pub async fn download_dash_live<F: ByteFetcher, R: Remuxer>(
    fetcher: &F, remux: &R, id: &str,
    mpd_url: &str, selection: &DashSelection, dest: &Path,
    handle: DownloadHandle, limiter: Arc<RateLimiter>,
    progress: mpsc::UnboundedSender<Progress>, headers: &RequestHeaders,
    initial: Option<Mpd>, refresh: RefreshInterval,
) -> Result<Outcome, CoreError> {
    let work = work_dir(dest);
    std::fs::create_dir_all(&work)?;

    let mut pending = initial;
    let mut resolved: Option<Resolved> = None;
    let mut v = DashLiveTrack::new("v");
    let mut a = DashLiveTrack::new("a");
    let mut total_done: u32 = 0;

    loop {
        let mpd = match pending.take() {
            Some(m) => m,
            None => match fetch_to_vec(fetcher, mpd_url, None, headers, &limiter, &handle).await? {
                Some(b) => parse_mpd(&String::from_utf8_lossy(&b), mpd_url)?,
                None => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            },
        };

        if resolved.is_none() {
            let vlist = video_variants(&mpd);
            let (vcoord, _) = vlist
                .iter()
                .find(|(_, vv)| vv.index == selection.video_index)
                .or_else(|| vlist.first())
                .ok_or_else(|| CoreError::Media("aucune variante vidéo".into()))?;
            let vrepr = &mpd.periods[vcoord.period].adaptation_sets[vcoord.adaptation].representations[vcoord.representation];
            match &vrepr.addressing {
                Addressing::Template(t) if t.timeline.is_some() => {}
                _ => return Err(CoreError::Media("DASH live $Number$/SegmentList non supporté (à venir)".into())),
            }
            let acoord = pick_audio(&mpd, selection.audio_lang.as_deref());
            let key = resolve_key(fetcher, &mpd, vcoord.adaptation, headers).await?;
            if let Some(k) = &key {
                std::fs::write(work.join("clearkey.hex"), k)?;
            }
            resolved = Some(Resolved { vcoord: vcoord.clone(), acoord, key });
        }
        let r = resolved.as_ref().unwrap();

        let vrepr = &mpd.periods[r.vcoord.period].adaptation_sets[r.vcoord.adaptation].representations[r.vcoord.representation];
        let vplan = build_segments(vrepr, &vrepr.base, mpd.duration_secs);
        match v.capture(&vplan, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
            Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
            Step::Stopped => break,
            Step::Continue => {}
        }

        if let Some(ac) = &r.acoord {
            let arepr = &mpd.periods[ac.period].adaptation_sets[ac.adaptation].representations[ac.representation];
            let aplan = build_segments(arepr, &arepr.base, mpd.duration_secs);
            match a.capture(&aplan, fetcher, &work, headers, &limiter, &handle, &progress, id, &mut total_done).await? {
                Step::Cancelled => { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
                Step::Stopped => break,
                Step::Continue => {}
            }
        }

        if handle.is_stopped() { break; }
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
        if !mpd.dynamic || mpd.minimum_update_period.is_none() { break; }

        let interval = refresh.compute(mpd.minimum_update_period.unwrap_or(0.0));
        responsive_sleep(interval, &handle).await;
        if handle.is_cancelled() { finish_cancelled(&work); return Ok(Outcome::Cancelled); }
    }

    let key = resolved.and_then(|r| r.key);
    finalize_and_remux(&work, v.has_init, a.has_any.then_some(a.has_init), key, remux, dest).await?;

    let _ = std::fs::remove_dir_all(&work);
    let _ = progress.send(Progress {
        id: id.to_string(), downloaded_bytes: handle.downloaded(), total_bytes: None,
        speed_bps: 0, status: DownloadStatus::Completed,
        segments_done: Some(total_done), segments_total: None,
    });
    Ok(Outcome::Completed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dash::engine::DashSelection;
    use crate::hls::remux::fake::FakeRemuxer;
    use crate::live::RefreshInterval;
    use crate::model::ProbeResult;
    use async_trait::async_trait;
    use bytes::Bytes;
    use futures_util::stream::{self, BoxStream};
    use futures_util::StreamExt;
    use std::collections::HashMap as Map;
    use std::sync::{Arc, Mutex};
    use std::time::Duration;
    use tempfile::tempdir;
    use tokio::sync::mpsc;

    /// MPD dynamic SegmentTimeline ; `media="seg-$Time$.m4s"`. `times` = valeurs t.
    fn mpd(times: &[u64], dynamic: bool) -> String {
        let s_elems: String = times.iter().enumerate().map(|(i, t)| {
            if i == 0 { format!("<S t=\"{t}\" d=\"1\"/>") } else { "<S d=\"1\"/>".to_string() }
        }).collect();
        let typ = if dynamic { "dynamic" } else { "static" };
        let mup = if dynamic { " minimumUpdatePeriod=\"PT1S\"" } else { "" };
        format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{typ}"{mup}>
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1">
      <SegmentTimeline>{s_elems}</SegmentTimeline>
    </SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#)
    }

    struct EvolvingMpd {
        mpd_url: String,
        mpds: Vec<String>,
        calls: Mutex<usize>,
        segs: Map<String, Vec<u8>>,
        handle: Option<DownloadHandle>,
        stop_after: usize,
        cancel_after: usize,
        seg_fetches: Arc<Mutex<usize>>,
        post_resp: Option<Vec<u8>>,
    }
    #[async_trait]
    impl ByteFetcher for EvolvingMpd {
        async fn probe(&self, _u: &str, _h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
            Ok(ProbeResult { total_bytes: None, supports_ranges: false, file_name: None })
        }
        async fn fetch_range(&self, url: &str, _r: Option<(u64, u64)>, _h: &RequestHeaders)
            -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
            let data = if url == self.mpd_url {
                let mut n = self.calls.lock().unwrap();
                let b = self.mpds[(*n).min(self.mpds.len() - 1)].clone();
                *n += 1;
                b.into_bytes()
            } else {
                let mut c = self.seg_fetches.lock().unwrap();
                *c += 1;
                if let Some(h) = &self.handle {
                    if self.stop_after != 0 && *c == self.stop_after { h.stop(); }
                    if self.cancel_after != 0 && *c == self.cancel_after { h.cancel(); }
                }
                self.segs.get(url).cloned().unwrap_or_default()
            };
            Ok(stream::iter(vec![Ok(Bytes::from(data))]).boxed())
        }
        async fn post(&self, _u: &str, _b: &[u8], _ct: &str, _h: &RequestHeaders) -> Result<Vec<u8>, CoreError> {
            Ok(self.post_resp.clone().unwrap_or_default())
        }
    }
    fn unlimited() -> Arc<RateLimiter> { Arc::new(RateLimiter::unlimited()) }
    fn sel() -> DashSelection { DashSelection { video_index: 0, audio_lang: None } }

    #[tokio::test]
    async fn captures_dedups_and_ends_on_static() {
        let base = "https://h/p/";
        let mpds = vec![mpd(&[0, 1], true), mpd(&[1, 2], true), mpd(&[2, 3], false)];
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB"),(2,"CC"),(3,"DD")] {
            segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec());
        }
        let seg_fetches = Arc::new(Mutex::new(0));
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: seg_fetches.clone(), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D1", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(),
            None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABBCCDD");
        assert_eq!(*seg_fetches.lock().unwrap(), 4, "dédup par URL : 4 fetchs");
        assert!(!dir.path().join("out.mp4.fghls").exists());
    }

    #[tokio::test]
    async fn manual_stop_finalizes_partial() {
        let base = "https://h/p/";
        let mpds = vec![mpd(&[0, 1], true)];
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB")] { segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: Some(h.clone()), stop_after: 2, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D2", &format!("{base}m.mpd"), &sel(), &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        assert_eq!(std::fs::read(&dest).unwrap(), b"AABB");
    }

    #[tokio::test]
    async fn cancel_discards() {
        let base = "https://h/p/";
        let mpds = vec![mpd(&[0, 1], true)];
        let mut segs = Map::new();
        for (t, b) in [(0u64,"AA"),(1,"BB")] { segs.insert(format!("{base}seg-{t}.m4s"), b.as_bytes().to_vec()); }
        let h = DownloadHandle::new();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds, calls: Mutex::new(0), segs,
            handle: Some(h.clone()), stop_after: 0, cancel_after: 1, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D3", &format!("{base}m.mpd"), &sel(), &dest,
            h, unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Cancelled);
        assert!(!dest.exists());
        assert!(!dir.path().join("out.mp4.fghls").exists());
    }

    #[tokio::test]
    async fn clearkey_resolved_persisted_and_passed() {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let base = "https://h/p/";
        let key: Vec<u8> = (0u8..16).collect();
        let k_b64 = URL_SAFE_NO_PAD.encode(&key);
        let jwk = format!(r#"{{"keys":[{{"kty":"oct","k":"{k_b64}","kid":"x"}}]}}"#);
        let xml = format!(r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013" type="static">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" cenc:default_KID="11112222-3333-4444-5555-666677778888"/>
    <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
      <dashif:laurl xmlns:dashif="https://dashif.org/CPS">{base}license</dashif:laurl>
    </ContentProtection>
    <SegmentTemplate media="seg-$Time$.m4s" timescale="1"><SegmentTimeline><S t="0" d="1"/></SegmentTimeline></SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#);
        let mut segs = Map::new();
        segs.insert(format!("{base}seg-0.m4s"), b"ENC".to_vec());
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds: vec![xml], calls: Mutex::new(0), segs,
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)),
            post_resp: Some(jwk.into_bytes()),
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let outcome = download_dash_live(
            &fetcher, &remux, "D4", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap();
        assert_eq!(outcome, Outcome::Completed);
        let calls = remux.calls.lock().unwrap();
        assert_eq!(calls[0].decryption_key.as_deref(), Some("000102030405060708090a0b0c0d0e0f"));
    }

    #[tokio::test]
    async fn rejects_number_mode_without_timeline() {
        let base = "https://h/p/";
        let xml = r#"<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT1S">
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate media="seg-$Number$.m4s" timescale="1" duration="1" startNumber="1"/>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#.to_string();
        let fetcher = EvolvingMpd {
            mpd_url: format!("{base}m.mpd"), mpds: vec![xml], calls: Mutex::new(0), segs: Map::new(),
            handle: None, stop_after: 0, cancel_after: 0, seg_fetches: Arc::new(Mutex::new(0)), post_resp: None,
        };
        let remux = FakeRemuxer::default();
        let dir = tempdir().unwrap();
        let dest = dir.path().join("out.mp4");
        let (tx, _rx) = mpsc::unbounded_channel();
        let err = download_dash_live(
            &fetcher, &remux, "D5", &format!("{base}m.mpd"), &sel(), &dest,
            DownloadHandle::new(), unlimited(), tx, &RequestHeaders::default(), None, RefreshInterval::Fixed(Duration::from_millis(0)),
        ).await.unwrap_err();
        assert!(matches!(err, CoreError::Media(_)));
    }
}
