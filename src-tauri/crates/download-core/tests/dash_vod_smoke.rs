//! Smoke-tests DASH VOD **sans ffmpeg** (suite normale) couvrant deux exigences
//! de bout en bout via le vrai `download_dash` + `ReqwestFetcher` + serveur HTTP
//! loopback :
//!
//!   1. `live_dash_is_rejected` — un MPD `type="dynamic"` (live) échoue proprement
//!      avec un message « flux live DASH non supporté ».
//!   2. `pause_then_resume_skips_done_segments` — une reprise après pause ne
//!      re-télécharge pas les segments média déjà présents sur le disque.
//!
//! Le déchiffrement ClearKey (qui exige le vrai ffmpeg) est couvert séparément
//! dans `dash_clearkey_smoke.rs` (`#[ignore]`).

use download_core::dash::engine::{download_dash, DashSelection};
use download_core::engine::{DownloadHandle, Outcome};
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::model::{Progress, ProbeResult, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use download_core::CoreError;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::stream::BoxStream;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tempfile::tempdir;
use tiny_http::{Response, Server};

const N_SEGS: usize = 6;

/// Remuxer minimal (pas de ffmpeg) : concatène vidéo (+audio) comme le
/// `FakeRemuxer` du crate. Suffisant pour valider la réassemblage.
struct ConcatRemuxer;
#[async_trait]
impl Remuxer for ConcatRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut buf = std::fs::read(&inputs.video)?;
        if let Some(a) = &inputs.audio {
            buf.extend_from_slice(&std::fs::read(a)?);
        }
        std::fs::write(output, buf)?;
        Ok(())
    }
}

/// Fetcher qui délègue à reqwest, enregistre chaque URL de segment média
/// récupérée, et (optionnellement) met le handle en pause après `pause_after`
/// segments média — ce qui force le moteur à renvoyer `Paused` avant le suivant.
struct InstrumentedFetcher {
    inner: ReqwestFetcher,
    handle: DownloadHandle,
    pause_after: usize, // 0 = jamais
    media_fetches: Arc<Mutex<Vec<String>>>,
}

#[async_trait]
impl download_core::http::ByteFetcher for InstrumentedFetcher {
    async fn probe(&self, url: &str, h: &RequestHeaders) -> Result<ProbeResult, CoreError> {
        self.inner.probe(url, h).await
    }
    async fn fetch_range(
        &self,
        url: &str,
        range: Option<(u64, u64)>,
        h: &RequestHeaders,
    ) -> Result<BoxStream<'static, Result<Bytes, CoreError>>, CoreError> {
        if url.contains("seg-") {
            let mut f = self.media_fetches.lock().unwrap();
            f.push(url.to_string());
            if self.pause_after != 0 && f.len() == self.pause_after {
                self.handle.pause();
            }
        }
        self.inner.fetch_range(url, range, h).await
    }
}

/// MPD VOD (SegmentList, clair) servi par le serveur local.
fn build_static_mpd(n: usize) -> String {
    let urls: String = (1..=n)
        .map(|i| format!("        <SegmentURL media=\"v_seg-{i}.m4s\"/>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="static" mediaPresentationDuration="PT6S" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011">
  <Period>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <Representation id="v0" bandwidth="200000" width="320" height="240">
        <SegmentList>
          <Initialization sourceURL="v_init.mp4"/>
{urls}        </SegmentList>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#
    )
}

const DYNAMIC_MPD: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="dynamic" minimumUpdatePeriod="PT2S" mediaPresentationDuration="PT6S">
  <Period>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <SegmentTemplate initialization="v_init.mp4" media="v_seg-$Number$.m4s" timescale="1000" duration="1000" startNumber="1"/>
      <Representation id="v0" bandwidth="200000" width="320" height="240"/>
    </AdaptationSet>
  </Period>
</MPD>"#;

/// Sert le MPD donné, un init et `N_SEGS` segments média synthétiques. Renvoie
/// le port. Les segments ont un contenu déterministe pour vérifier l'assemblage.
fn serve(mpd: Vec<u8>) -> u16 {
    let server = Server::http("127.0.0.1:0").expect("bind port éphémère");
    let port = server.server_addr().to_ip().unwrap().port();
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url().trim_start_matches('/').to_string();
            let body: Option<Vec<u8>> = if url == "manifest.mpd" {
                Some(mpd.clone())
            } else if url == "v_init.mp4" {
                Some(b"INIT".to_vec())
            } else if let Some(n) = url.strip_prefix("v_seg-").and_then(|s| s.strip_suffix(".m4s")) {
                n.parse::<usize>().ok().filter(|i| (1..=N_SEGS).contains(i)).map(seg_bytes)
            } else {
                None
            };
            let resp = match body {
                Some(b) => Response::from_data(b).with_status_code(200),
                None => Response::from_data(Vec::new()).with_status_code(404),
            };
            let _ = request.respond(resp);
        }
    });
    port
}

/// Octets déterministes du i-ème segment (1-based) : "SEGi".
fn seg_bytes(i: usize) -> Vec<u8> {
    format!("SEG{i}").into_bytes()
}

#[tokio::test(flavor = "multi_thread")]
async fn live_dash_is_rejected() {
    let port = serve(DYNAMIC_MPD.as_bytes().to_vec());
    let fetcher = ReqwestFetcher::new();
    let dir = tempdir().unwrap();
    let dest = dir.path().join("live.mp4");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let sel = DashSelection { video_index: 0, audio_lang: None };
    let url = format!("http://127.0.0.1:{port}/manifest.mpd");

    let err = download_dash(
        &fetcher, &ConcatRemuxer, "live1", &url, &sel, &dest,
        DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(),
    )
    .await
    .expect_err("un MPD dynamic (live) doit être rejeté");

    match err {
        CoreError::Media(m) => assert!(
            m.contains("live") && m.contains("non supporté"),
            "message d'erreur live inattendu: {m}"
        ),
        other => panic!("attendu CoreError::Media, obtenu {other:?}"),
    }
    assert!(!dest.exists(), "aucun fichier de sortie ne doit être produit pour du live");
}

#[tokio::test(flavor = "multi_thread")]
async fn pause_then_resume_skips_done_segments() {
    let port = serve(build_static_mpd(N_SEGS).into_bytes());
    let url = format!("http://127.0.0.1:{port}/manifest.mpd");
    let dir = tempdir().unwrap();
    let dest = dir.path().join("vod.mp4");
    let sel = DashSelection { video_index: 0, audio_lang: None };
    let media_fetches = Arc::new(Mutex::new(Vec::<String>::new()));

    // --- Run 1 : pause après 3 segments média ---
    let handle1 = DownloadHandle::new();
    let fetcher1 = InstrumentedFetcher {
        inner: ReqwestFetcher::new(),
        handle: handle1.clone(),
        pause_after: 3,
        media_fetches: media_fetches.clone(),
    };
    let (tx1, _rx1) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let outcome1 = download_dash(
        &fetcher1, &ConcatRemuxer, "vod1", &url, &sel, &dest,
        handle1, Arc::new(RateLimiter::unlimited()), tx1, &RequestHeaders::default(),
    )
    .await
    .expect("run 1 ne doit pas échouer");
    assert_eq!(outcome1, Outcome::Paused, "le run 1 doit se mettre en pause");
    assert!(!dest.exists(), "pas de sortie tant que la pause n'est pas reprise");
    assert_eq!(
        media_fetches.lock().unwrap().len(),
        3,
        "exactement 3 segments média téléchargés avant la pause"
    );

    // --- Run 2 : reprise (handle neuf), doit sauter les 3 segments déjà présents ---
    let handle2 = DownloadHandle::new();
    let fetcher2 = InstrumentedFetcher {
        inner: ReqwestFetcher::new(),
        handle: handle2.clone(),
        pause_after: 0,
        media_fetches: media_fetches.clone(),
    };
    let (tx2, _rx2) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let outcome2 = download_dash(
        &fetcher2, &ConcatRemuxer, "vod1", &url, &sel, &dest,
        handle2, Arc::new(RateLimiter::unlimited()), tx2, &RequestHeaders::default(),
    )
    .await
    .expect("run 2 doit réussir");
    assert_eq!(outcome2, Outcome::Completed);
    assert!(dest.exists(), "la reprise doit produire le fichier final");

    // Chaque segment média n'a été téléchargé qu'UNE fois au total : la reprise
    // n'a pas re-téléchargé les 3 déjà faits.
    let all = media_fetches.lock().unwrap().clone();
    assert_eq!(all.len(), N_SEGS, "total des fetchs média = {N_SEGS} (aucun re-téléchargement)");
    let mut uniq = all.clone();
    uniq.sort();
    uniq.dedup();
    assert_eq!(uniq.len(), N_SEGS, "chaque segment média ne doit être récupéré qu'une fois: {all:?}");

    // Le fichier final = init + concat des 6 segments dans l'ordre.
    let mut expected = b"INIT".to_vec();
    for i in 1..=N_SEGS {
        expected.extend_from_slice(&seg_bytes(i));
    }
    assert_eq!(std::fs::read(&dest).unwrap(), expected, "réassemblage pause/reprise incorrect");
}
