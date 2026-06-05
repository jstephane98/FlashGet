//! Smoke-test **hors-ligne** de l'enregistrement HLS live : pilote le vrai
//! `download_hls_live` (via `ReqwestFetcher`) contre un serveur HTTP loopback
//! servant une media playlist live qui ÉVOLUE (fenêtre glissante → ENDLIST),
//! avec un vrai `CliRemuxer` invoquant le `ffmpeg.exe` embarqué, et vérifie
//! qu'un `.mp4` lisible est produit.
//!
//! Point de correction clé : les segments TS sont des morceaux d'UN SEUL encode
//! continu (muxer HLS de ffmpeg), de sorte que la concaténation par le moteur a
//! des timestamps continus et le décodage final est propre.
//!
//! Deux cas :
//!  1. `hls_live_records_to_endlist` : la playlist glisse sur 2 fenêtres puis
//!     pose ENDLIST → finalisation par fin de flux.
//!  2. `hls_live_stop_finalizes_partial` : la playlist ne se termine JAMAIS ;
//!     le test lance la capture en tâche, laisse capturer quelques segments,
//!     puis `handle.stop()` → finalisation partielle déterministe.
//!
//! `#[ignore]` car exige le vrai ffmpeg.exe ; lancer avec :
//! ```text
//! cargo test -p download-core --test hls_live_smoke -- --ignored --nocapture
//! ```

use download_core::engine::{DownloadHandle, Outcome};
use download_core::hls::live::download_hls_live;
use download_core::live::RefreshInterval;
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::model::{Progress, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use download_core::CoreError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::tempdir;
use tiny_http::{Response, Server};

/// Chemin du vrai ffmpeg embarqué (relatif au crate download-core).
fn ffmpeg_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bin/ffmpeg-x86_64-pc-windows-msvc.exe")
}

/// Remuxer de test : invoque ffmpeg directement (`-i ... -c copy`), hors du
/// sidecar Tauri. C'est le chemin de remux normal qu'on valide ici.
struct CliRemuxer {
    ffmpeg: PathBuf,
}

#[async_trait::async_trait]
impl Remuxer for CliRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into()];
        args.push("-i".into());
        args.push(inputs.video.to_string_lossy().to_string());
        if let Some(audio) = &inputs.audio {
            args.push("-i".into());
            args.push(audio.to_string_lossy().to_string());
        }
        args.extend([
            "-c".into(),
            "copy".into(),
            "-movflags".into(),
            "+faststart".into(),
        ]);
        args.push(output.to_string_lossy().to_string());

        let out = Command::new(&self.ffmpeg)
            .args(&args)
            .output()
            .map_err(|e| CoreError::Media(format!("échec exécution ffmpeg: {e}")))?;
        if !out.status.success() {
            let stderr = String::from_utf8_lossy(&out.stderr);
            return Err(CoreError::Media(format!("ffmpeg a échoué: {stderr}")));
        }
        Ok(())
    }
}

/// Génère UN flux H.264 continu de 4 s découpé en segments HLS réels (`seg0.ts`,
/// `seg1.ts`, ...) via le muxer HLS de ffmpeg, puis lit ces segments en mémoire.
/// Comme ils proviennent d'UN SEUL encode, leur concaténation a des timestamps
/// continus et décode proprement.
fn build_continuous_segments(ffmpeg: &Path, dir: &Path) -> Vec<Vec<u8>> {
    let seg_pattern = dir.join("seg%d.ts");
    let playlist = dir.join("all.m3u8");
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc=duration=4:size=160x120:rate=10",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-g",
            "10",
            "-hls_time",
            "1",
            "-hls_list_size",
            "0",
            "-hls_segment_filename",
        ])
        .arg(&seg_pattern)
        .arg(&playlist)
        .status()
        .expect("lancer ffmpeg pour générer les segments HLS continus");
    assert!(status.success(), "ffmpeg n'a pas pu générer les segments HLS");

    // Découvre combien de segments ont réellement été produits (sans hardcoder).
    let mut segs = Vec::new();
    for i in 0.. {
        let p = dir.join(format!("seg{i}.ts"));
        match std::fs::read(&p) {
            Ok(bytes) => segs.push(bytes),
            Err(_) => break,
        }
    }
    assert!(
        segs.len() >= 3,
        "au moins 3 segments continus attendus (obtenu {})",
        segs.len()
    );
    segs
}

/// Construit un corps de media playlist live : fenêtre commençant à la séquence
/// `seq` listant les segments d'indices `idxs`, avec ENDLIST optionnel.
fn live_body(seq: usize, idxs: &[usize], endlist: bool) -> String {
    let mut s = format!(
        "#EXTM3U\n#EXT-X-TARGETDURATION:1\n#EXT-X-MEDIA-SEQUENCE:{seq}\n"
    );
    for &i in idxs {
        s.push_str(&format!("#EXTINF:1.0,\nseg{i}.ts\n"));
    }
    if endlist {
        s.push_str("#EXT-X-ENDLIST\n");
    }
    s
}

/// Décode `path` avec ffmpeg (`-f null -`) en jetant la sortie ;
/// renvoie (succès, stderr).
fn ffmpeg_decode_check(ffmpeg: &Path, path: &Path) -> (bool, String) {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-v", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .expect("lancer ffmpeg pour la vérification de décodage");
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

/// Vérifie qu'un mp4 produit est lisible : ffmpeg décode TOUT le flux sans
/// erreur fatale. On exige `status.success()` (le décodeur n'a pas avorté) ET
/// l'absence de marqueurs d'échec dur (« invalid data » / « Error ») — un
/// décodage propre d'un fichier corrompu déclencherait l'un ou l'autre. On
/// n'exige PAS un stderr vide : même un encode continu peut émettre des notes
/// bénignes.
fn assert_decodes(ffmpeg: &Path, dest: &Path) {
    let (ok, stderr) = ffmpeg_decode_check(ffmpeg, dest);
    eprintln!("--- ffmpeg decode stderr ---\n{stderr}\n--- fin stderr ---");
    assert!(ok, "ffmpeg n'a pas décodé le mp4 (exit != 0) — stderr:\n{stderr}");
    // ffmpeg `-v error` émet en minuscules : on teste tout sur `low` (sinon un
    // « error decoding » minuscule passerait inaperçu).
    let low = stderr.to_lowercase();
    assert!(
        !low.contains("invalid data") && !low.contains("error"),
        "le décodage signale une erreur dure — stderr:\n{stderr}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn hls_live_records_to_endlist() {
    let ffmpeg = ffmpeg_path();
    assert!(
        ffmpeg.exists(),
        "ffmpeg introuvable en {} — déposez le vrai binaire avant ce smoke-test",
        ffmpeg.display()
    );

    let dir = tempdir().unwrap();
    let segs = build_continuous_segments(&ffmpeg, dir.path());
    let n = segs.len();
    eprintln!("{n} segments continus produits");

    // Playlists successives : fenêtre de 2 qui glisse, puis ENDLIST. On vise les
    // deux PREMIERS et deux DERNIERS segments pour couvrir tout le flux ; avec
    // n>=3 on liste [0,1] → [1,2] (glissement, dedup s1) → [last-1,last] ENDLIST.
    let bodies = vec![
        live_body(0, &[0, 1], false),
        live_body(1, &[1, 2], false),
        live_body(2, &(2..n).collect::<Vec<_>>(), true),
    ];

    let server = Server::http("127.0.0.1:0").expect("bind port éphémère");
    let port = server.server_addr().to_ip().unwrap().port();

    let segs_arc = Arc::new(segs);
    let bodies_arc = Arc::new(bodies);
    let counter = Arc::new(AtomicUsize::new(0));
    {
        let segs = segs_arc.clone();
        let bodies = bodies_arc.clone();
        let counter = counter.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let url = request.url().trim_start_matches('/').to_string();
                let body: Option<Vec<u8>> = if url == "live.m3u8" {
                    let i = counter.fetch_add(1, Ordering::SeqCst);
                    let idx = i.min(bodies.len() - 1);
                    Some(bodies[idx].clone().into_bytes())
                } else if let Some(num) = url
                    .strip_prefix("seg")
                    .and_then(|s| s.strip_suffix(".ts"))
                {
                    num.parse::<usize>().ok().and_then(|k| segs.get(k).cloned())
                } else {
                    None
                };
                let response = match body {
                    Some(b) => Response::from_data(b).with_status_code(200),
                    None => Response::from_data(Vec::new()).with_status_code(404),
                };
                let _ = request.respond(response);
            }
        });
    }

    let fetcher = ReqwestFetcher::new();
    let remux = CliRemuxer { ffmpeg: ffmpeg.clone() };
    let dest = dir.path().join("out.mp4");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let media_url = format!("http://127.0.0.1:{port}/live.m3u8");

    let outcome = download_hls_live(
        &fetcher,
        &remux,
        "live1",
        &media_url,
        None,
        &dest,
        DownloadHandle::new(),
        Arc::new(RateLimiter::unlimited()),
        tx,
        &RequestHeaders::default(),
        None,
        RefreshInterval::Fixed(Duration::from_millis(10)),
    )
    .await
    .expect("l'enregistrement HLS live doit réussir");

    assert_eq!(outcome, Outcome::Completed);
    assert!(dest.exists(), "le .mp4 de sortie doit exister");
    let size = std::fs::metadata(&dest).unwrap().len();
    assert!(size > 1000, "mp4 trop petit ({size} octets)");
    assert_decodes(&ffmpeg, &dest);
    eprintln!("OK : enregistrement live → ENDLIST, mp4 lisible ({size} octets)");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn hls_live_stop_finalizes_partial() {
    let ffmpeg = ffmpeg_path();
    assert!(
        ffmpeg.exists(),
        "ffmpeg introuvable en {} — déposez le vrai binaire avant ce smoke-test",
        ffmpeg.display()
    );

    let dir = tempdir().unwrap();
    let segs = build_continuous_segments(&ffmpeg, dir.path());
    let n = segs.len();

    // Playlist live qui ne se termine JAMAIS : à chaque appel, fenêtre glissante
    // sans ENDLIST. On expose tous les segments dans l'ordre par fenêtres de 2,
    // puis on reste sur la dernière fenêtre (clamp), toujours sans ENDLIST.
    let server = Server::http("127.0.0.1:0").expect("bind port éphémère");
    let port = server.server_addr().to_ip().unwrap().port();

    // Combien de segments le moteur a-t-il servis (pour la déterminisme).
    let seg_served = Arc::new(AtomicUsize::new(0));
    let segs_arc = Arc::new(segs);
    let counter = Arc::new(Mutex::new(0usize));
    {
        let segs = segs_arc.clone();
        let seg_served = seg_served.clone();
        let counter = counter.clone();
        std::thread::spawn(move || {
            for request in server.incoming_requests() {
                let url = request.url().trim_start_matches('/').to_string();
                let body: Option<Vec<u8>> = if url == "live.m3u8" {
                    let mut c = counter.lock().unwrap();
                    // Fenêtre glissante de 2, jamais d'ENDLIST. Clamp pour rester
                    // valide après épuisement des segments.
                    let start = (*c).min(n.saturating_sub(2));
                    *c += 1;
                    let idxs: Vec<usize> = (start..(start + 2).min(n)).collect();
                    Some(live_body(start, &idxs, false).into_bytes())
                } else if let Some(num) = url
                    .strip_prefix("seg")
                    .and_then(|s| s.strip_suffix(".ts"))
                {
                    seg_served.fetch_add(1, Ordering::SeqCst);
                    num.parse::<usize>().ok().and_then(|k| segs.get(k).cloned())
                } else {
                    None
                };
                let response = match body {
                    Some(b) => Response::from_data(b).with_status_code(200),
                    None => Response::from_data(Vec::new()).with_status_code(404),
                };
                let _ = request.respond(response);
            }
        });
    }

    let fetcher = ReqwestFetcher::new();
    let remux = CliRemuxer { ffmpeg: ffmpeg.clone() };
    let dest = dir.path().join("out_stop.mp4");
    let dest_check = dest.clone(); // `dest` est déplacé dans la tâche ; on garde un clone pour les assertions.
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let media_url = format!("http://127.0.0.1:{port}/live.m3u8");
    let handle = DownloadHandle::new();

    // Lance la capture en tâche ; arrête après qu'au moins 1 segment a été servi.
    let task = {
        let handle = handle.clone();
        tokio::spawn(async move {
            download_hls_live(
                &fetcher,
                &remux,
                "live2",
                &media_url,
                None,
                &dest,
                handle,
                Arc::new(RateLimiter::unlimited()),
                tx,
                &RequestHeaders::default(),
                None,
                RefreshInterval::Fixed(Duration::from_millis(10)),
            )
            .await
        })
    };

    // Attend (borné) qu'au moins 1 segment ait été servi, puis stop.
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while seg_served.load(Ordering::SeqCst) < 1 {
        assert!(std::time::Instant::now() < deadline, "aucun segment servi à temps");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    // Laisse capturer quelques segments de plus pour un mp4 décodable.
    tokio::time::sleep(Duration::from_millis(200)).await;
    handle.stop();

    let outcome = task.await.expect("tâche live").expect("live stop doit réussir");
    assert_eq!(outcome, Outcome::Completed);
    assert!(dest_check.exists(), "le .mp4 partiel doit exister");
    let size = std::fs::metadata(&dest_check).unwrap().len();
    assert!(size > 1000, "mp4 partiel trop petit ({size} octets)");
    assert_decodes(&ffmpeg, &dest_check);
    eprintln!("OK : stop mid-recording → mp4 partiel lisible ({size} octets)");
}
