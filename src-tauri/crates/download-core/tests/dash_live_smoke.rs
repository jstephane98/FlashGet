//! Smoke-test DASH dynamic live OFFLINE (#[ignore], exige le vrai ffmpeg).
//! Un serveur loopback sert un MPD dynamic SegmentTimeline qui évolue puis
//! repasse static ; download_dash_live via ReqwestFetcher + CliRemuxer → .mp4.
//!
//! Rejouer : cargo test -p download-core --test dash_live_smoke -- --ignored --nocapture

use download_core::dash::engine::DashSelection;
use download_core::dash::live::download_dash_live;
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::live::RefreshInterval;
use download_core::model::{Progress, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use download_core::engine::{DownloadHandle, Outcome};
use download_core::CoreError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tiny_http::{Response, Server};

fn ffmpeg_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bin/ffmpeg-x86_64-pc-windows-msvc.exe")
}

struct CliRemuxer { ffmpeg: PathBuf }

#[async_trait::async_trait]
impl Remuxer for CliRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into(), "-i".into(), inputs.video.to_string_lossy().into()];
        if let Some(audio) = &inputs.audio {
            args.push("-i".into());
            args.push(audio.to_string_lossy().into());
        }
        args.extend(["-c".into(), "copy".into(), "-movflags".into(), "+faststart".into(), output.to_string_lossy().into()]);
        let out = Command::new(&self.ffmpeg).args(&args).output()
            .map_err(|e| CoreError::Media(format!("ffmpeg: {e}")))?;
        if !out.status.success() {
            return Err(CoreError::Media(format!("ffmpeg a échoué: {}", String::from_utf8_lossy(&out.stderr))));
        }
        Ok(())
    }
}

/// Génère un fMP4 fragmenté via ffmpeg, le découpe en (init, [fragments]).
/// `init` = tout jusqu'au premier `moof` ; chaque fragment = moof..next-moof.
fn make_fmp4_segments(ffmpeg: &Path, dir: &Path, n: usize) -> (Vec<u8>, Vec<Vec<u8>>) {
    let enc = dir.join("enc.mp4");
    let status = Command::new(ffmpeg).args([
        "-hide_banner", "-loglevel", "error", "-y",
        "-f", "lavfi", "-i", &format!("testsrc=duration={n}:size=160x120:rate=10"),
        "-c:v", "libx264", "-pix_fmt", "yuv420p",
        "-g", "10", "-keyint_min", "10", "-sc_threshold", "0",
        "-frag_duration", "500000",
        "-movflags", "+frag_keyframe+empty_moov+default_base_moof",
    ]).arg(&enc).status().expect("ffmpeg enc");
    assert!(status.success(), "ffmpeg n'a pas pu générer le fMP4 fragmenté");
    let data = std::fs::read(&enc).unwrap();
    let mut moof_offsets = Vec::new();
    let mut off = 0usize;
    while off + 8 <= data.len() {
        let size = u32::from_be_bytes(data[off..off+4].try_into().unwrap()) as usize;
        let typ = &data[off+4..off+8];
        let real = if size == 1 {
            u64::from_be_bytes(data[off+8..off+16].try_into().unwrap()) as usize
        } else {
            size
        };
        if real == 0 { break; }
        if typ == b"moof" { moof_offsets.push(off); }
        off += real;
    }
    assert!(!moof_offsets.is_empty(), "fMP4 sans moof : fixture invalide");
    let init = data[..moof_offsets[0]].to_vec();
    let mut segs = Vec::new();
    for (i, &s) in moof_offsets.iter().enumerate() {
        let e = moof_offsets.get(i+1).copied().unwrap_or(data.len());
        segs.push(data[s..e].to_vec());
    }
    eprintln!("make_fmp4_segments: init={} octets, {} fragments", init.len(), segs.len());
    (init, segs)
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn dash_live_records_until_static() {
    let ffmpeg = ffmpeg_path();
    assert!(ffmpeg.exists(), "ffmpeg introuvable en {} — déposez le vrai binaire avant ce smoke-test", ffmpeg.display());

    let dir = tempdir().unwrap();
    // Duration=6 + frag_duration=500000µs @ 10fps → ~20 fragments par seconde-encode,
    // 6s devrait donner ≥ 6 fragments avec des GOP de 10 frames (=1s).
    let (init, segs) = make_fmp4_segments(&ffmpeg, dir.path(), 6);
    assert!(segs.len() >= 4, "attendu ≥4 fragments, obtenu {}", segs.len());

    // On construit 3 MPD qui référencent des fenêtres glissantes de 2 segments
    // chacun, puis passent static. La timeline est en secondes (timescale=1) :
    // fenêtre 0: [seg-0, seg-1], fenêtre 1: [seg-1, seg-2], fenêtre 2: [seg-2, seg-3] static.
    // Le moteur déduplique par URL → 4 segments distincts fetchés.
    let mk = |start_t: u64, end: bool| -> String {
        let typ = if end { "static" } else { "dynamic" };
        let mup = if end { String::new() } else { " minimumUpdatePeriod=\"PT1S\"".to_string() };
        format!(r#"<?xml version="1.0"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" type="{typ}"{mup}>
  <Period><AdaptationSet contentType="video" mimeType="video/mp4">
    <SegmentTemplate initialization="init.mp4" media="seg-$Time$.m4s" timescale="1">
      <SegmentTimeline><S t="{start_t}" d="1"/><S d="1"/></SegmentTimeline>
    </SegmentTemplate>
    <Representation id="v0" bandwidth="1"/>
  </AdaptationSet></Period>
</MPD>"#)
    };

    // 3 MPD successifs : dynamic [0,1] → dynamic [1,2] → static [2,3]
    let mpds: Vec<String> = vec![mk(0, false), mk(1, false), mk(2, true)];

    let init_srv = init.clone();
    let segs_srv = segs.clone();
    let calls = Arc::new(Mutex::new(0usize));
    let mpds_srv = mpds.clone();

    let server = Server::http("127.0.0.1:0").expect("bind port éphémère");
    let port = server.server_addr().to_ip().unwrap().port();

    std::thread::spawn(move || {
        for req in server.incoming_requests() {
            let url = req.url().trim_start_matches('/').to_string();
            let body: Option<Vec<u8>> = if url == "m.mpd" {
                let mut n = calls.lock().unwrap();
                let b = mpds_srv[(*n).min(mpds_srv.len()-1)].clone();
                eprintln!("[server] MPD #{} → {}", *n, if *n < mpds_srv.len()-1 { "dynamic" } else { "static" });
                *n += 1;
                Some(b.into_bytes())
            } else if url == "init.mp4" {
                eprintln!("[server] init.mp4 ({} octets)", init_srv.len());
                Some(init_srv.clone())
            } else if let Some(t) = url.strip_prefix("seg-").and_then(|s| s.strip_suffix(".m4s")) {
                let idx = t.parse::<usize>().unwrap_or(usize::MAX);
                eprintln!("[server] seg-{t}.m4s → fragment[{idx}]");
                t.parse::<usize>().ok().and_then(|i| segs_srv.get(i).cloned())
            } else {
                eprintln!("[server] 404: {url}");
                None
            };
            let _ = req.respond(match body {
                Some(b) => Response::from_data(b).with_status_code(200),
                None => Response::from_data(Vec::new()).with_status_code(404),
            });
        }
    });

    let fetcher = ReqwestFetcher::new();
    let remux = CliRemuxer { ffmpeg: ffmpeg.clone() };
    let dest = dir.path().join("out.mp4");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let url = format!("http://127.0.0.1:{port}/m.mpd");
    let sel = DashSelection { video_index: 0, audio_lang: None };

    eprintln!("Lancement download_dash_live sur {url}");
    let outcome = download_dash_live(
        &fetcher, &remux, "smk", &url, &sel, &dest,
        DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(),
        None, RefreshInterval::Fixed(Duration::from_millis(10)),
    ).await.expect("dash live doit réussir");

    assert_eq!(outcome, Outcome::Completed);
    assert!(dest.exists(), "le .mp4 de sortie doit exister");
    let size = std::fs::metadata(&dest).unwrap().len();
    assert!(size > 1000, "mp4 trop petit ({size} octets) — concat/remux a probablement échoué silencieusement");
    eprintln!("out.mp4 produit : {size} octets");

    // Décodage final : ffmpeg -f null - doit sortir sans erreur
    let chk = Command::new(&ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(&dest)
        .args(["-f", "null", "-"])
        .output()
        .expect("lancer ffmpeg pour la vérification de décodage");
    let stderr = String::from_utf8_lossy(&chk.stderr).to_string();
    eprintln!("--- ffmpeg decode stderr ---\n{stderr}--- fin stderr ---");
    assert!(chk.status.success(), "décodage du .mp4 live échoué (exit != 0): {stderr}");
    let low = stderr.to_lowercase();
    assert!(
        !low.contains("invalid data") && !low.contains("error"),
        "le décodage signale une erreur dure — stderr:\n{stderr}"
    );
    eprintln!("OK : DASH dynamic live → static, mp4 décodé proprement ({size} octets)");
}
