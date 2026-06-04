//! Smoke-test ClearKey / CENC **hors-ligne** : valide le déchiffrement de bout
//! en bout d'un flux DASH chiffré AES-CTR (CENC) via le vrai pipeline de
//! production (`download_dash` + `ReqwestFetcher` + flux de licence W3C
//! ClearKey), avec un vrai `ffmpeg.exe` faisant le `-decryption_key`.
//!
//! Aucun flux ClearKey public n'étant garanti stable, ce test **fabrique sa
//! propre fixture** : ffmpeg génère un fMP4 fragmenté chiffré CENC, on le
//! découpe en init + N segments média, un mini serveur HTTP loopback sert le
//! MPD / les segments / la licence (JWK), et le moteur télécharge puis
//! déchiffre. Tout est local : aucune dépendance réseau externe.
//!
//! Le test est `#[ignore]` car il exige le vrai binaire ffmpeg (le dépôt n'en
//! embarque qu'un stub gitignoré). Pour le rejouer une fois le vrai
//! `src-tauri/bin/ffmpeg-x86_64-pc-windows-msvc.exe` en place :
//!
//! ```text
//! cargo test -p download-core --test dash_clearkey_smoke -- --ignored --nocapture
//! ```
//!
//! Clés de la fixture (KID/KEY 16 octets, ClearKey « clé en clair ») :
//!   default_KID = 11112222-3333-4444-5555-666677778888
//!   content KEY = 00112233445566778899aabbccddeeff
//!   JWK servi   = { keys:[{ kty:oct, k:base64url(KEY), kid:base64url(KID) }] }

use download_core::dash::engine::{download_dash, DashSelection};
use download_core::hls::remux::{RemuxInputs, Remuxer};
use download_core::http::ReqwestFetcher;
use download_core::model::{Progress, RequestHeaders};
use download_core::rate_limiter::RateLimiter;
use download_core::engine::{DownloadHandle, Outcome};
use download_core::CoreError;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use tempfile::tempdir;
use tiny_http::{Method, Response, Server};

const KID_HEX: &str = "11112222333344445555666677778888";
const KEY_HEX: &str = "00112233445566778899aabbccddeeff";
const KID_DASHED: &str = "11112222-3333-4444-5555-666677778888";
// JWK W3C ClearKey : k = base64url(KEY), kid = base64url(KID) (sans padding).
const JWK: &str = r#"{"keys":[{"kty":"oct","k":"ABEiM0RVZneImaq7zN3u_w","kid":"EREiIjMzRERVVWZmd3eIiA"}]}"#;

/// Chemin du vrai ffmpeg embarqué (relatif au crate download-core).
fn ffmpeg_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../bin/ffmpeg-x86_64-pc-windows-msvc.exe")
}

/// Remuxer de test : réplique fidèlement la construction d'arguments de
/// `SidecarRemuxer` (src-tauri/src/remux.rs) mais invoque ffmpeg directement,
/// hors du sidecar Tauri. C'est ce chemin `-decryption_key` qu'on valide.
struct CliRemuxer {
    ffmpeg: PathBuf,
}

#[async_trait::async_trait]
impl Remuxer for CliRemuxer {
    async fn remux(&self, inputs: RemuxInputs, output: &Path) -> Result<(), CoreError> {
        let mut args: Vec<String> = vec!["-y".into()];
        if let Some(key) = &inputs.decryption_key {
            args.push("-decryption_key".into());
            args.push(key.clone());
        }
        args.push("-i".into());
        args.push(inputs.video.to_string_lossy().to_string());
        if let Some(audio) = &inputs.audio {
            if let Some(key) = &inputs.decryption_key {
                args.push("-decryption_key".into());
                args.push(key.clone());
            }
            args.push("-i".into());
            args.push(audio.to_string_lossy().to_string());
        }
        args.extend(["-c".into(), "copy".into(), "-movflags".into(), "+faststart".into()]);
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

/// Découpe un fMP4 fragmenté en (init, [segments]). `init` = tout jusqu'au
/// premier `moof` ; chaque segment = d'un `moof` au `moof` suivant (le dernier
/// emporte les boîtes de fin type `mfra`). La concaténation init+segments
/// reproduit l'entrée **à l'octet près**.
fn split_fragments(data: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut moof_offsets = Vec::new();
    let mut off = 0usize;
    while off + 8 <= data.len() {
        let size = u32::from_be_bytes(data[off..off + 4].try_into().unwrap()) as usize;
        let typ = &data[off + 4..off + 8];
        let real = if size == 1 {
            u64::from_be_bytes(data[off + 8..off + 16].try_into().unwrap()) as usize
        } else {
            size
        };
        if real == 0 {
            break;
        }
        if typ == b"moof" {
            moof_offsets.push(off);
        }
        off += real;
    }
    assert!(!moof_offsets.is_empty(), "fMP4 sans moof : fixture invalide");
    let init = data[..moof_offsets[0]].to_vec();
    let mut segs = Vec::new();
    for (i, &start) in moof_offsets.iter().enumerate() {
        let end = moof_offsets.get(i + 1).copied().unwrap_or(data.len());
        segs.push(data[start..end].to_vec());
    }
    (init, segs)
}

/// Fabrique le fMP4 chiffré CENC via ffmpeg et le découpe en init + segments.
fn build_encrypted_fixture(ffmpeg: &Path, dir: &Path) -> (Vec<u8>, Vec<Vec<u8>>) {
    let enc = dir.join("enc.mp4");
    let status = Command::new(ffmpeg)
        .args([
            "-hide_banner", "-loglevel", "error", "-y",
            "-f", "lavfi", "-i", "testsrc=duration=3:size=320x240:rate=10",
            "-c:v", "libx264", "-pix_fmt", "yuv420p",
            "-g", "5", "-keyint_min", "5", "-sc_threshold", "0",
            "-frag_duration", "500000",
            "-encryption_scheme", "cenc-aes-ctr",
            "-encryption_key", KEY_HEX,
            "-encryption_kid", KID_HEX,
            "-movflags", "+frag_keyframe+empty_moov+default_base_moof",
        ])
        .arg(&enc)
        .status()
        .expect("lancer ffmpeg pour générer la fixture chiffrée");
    assert!(status.success(), "ffmpeg n'a pas pu générer la fixture chiffrée");
    let data = std::fs::read(&enc).unwrap();
    let (init, segs) = split_fragments(&data);
    // Invariant : init + segments reproduit le fichier chiffré d'origine.
    let mut joined = init.clone();
    for s in &segs {
        joined.extend_from_slice(s);
    }
    assert_eq!(joined, data, "le découpage init+segments doit reproduire le fMP4 d'origine");
    assert!(segs.len() >= 2, "fixture multi-segments attendue (obtenu {})", segs.len());
    (init, segs)
}

/// Sert (dans un thread) le MPD, l'init, les segments et la licence ClearKey
/// (POST → JWK) sur un serveur déjà lié.
fn serve(server: Server, mpd: Vec<u8>, init: Vec<u8>, segs: Vec<Vec<u8>>) {
    std::thread::spawn(move || {
        for request in server.incoming_requests() {
            let url = request.url().trim_start_matches('/').to_string();
            let is_post = *request.method() == Method::Post;
            let body: Option<Vec<u8>> = if is_post && url == "license" {
                Some(JWK.as_bytes().to_vec())
            } else if url == "manifest.mpd" {
                Some(mpd.clone())
            } else if url == "v_init.mp4" {
                Some(init.clone())
            } else if let Some(n) = url.strip_prefix("v_seg-").and_then(|s| s.strip_suffix(".m4s")) {
                n.parse::<usize>().ok().and_then(|i| segs.get(i - 1).cloned())
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

/// MPD ClearKey (SegmentList) pointant vers le serveur local.
fn build_mpd(port: u16, n_segments: usize) -> String {
    let urls: String = (1..=n_segments)
        .map(|i| format!("        <SegmentURL media=\"v_seg-{i}.m4s\"/>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<MPD xmlns="urn:mpeg:dash:schema:mpd:2011" xmlns:cenc="urn:mpeg:cenc:2013" type="static" mediaPresentationDuration="PT3S" profiles="urn:mpeg:dash:profile:isoff-on-demand:2011">
  <Period>
    <AdaptationSet contentType="video" mimeType="video/mp4">
      <ContentProtection schemeIdUri="urn:mpeg:dash:mp4protection:2011" cenc:default_KID="{KID_DASHED}"/>
      <ContentProtection schemeIdUri="urn:uuid:e2719d58-a985-b3c9-781a-b030af78d30e">
        <dashif:laurl xmlns:dashif="https://dashif.org/CPS">http://127.0.0.1:{port}/license</dashif:laurl>
      </ContentProtection>
      <Representation id="v0" bandwidth="200000" width="320" height="240" codecs="avc1.42c015">
        <SegmentList>
          <Initialization sourceURL="v_init.mp4"/>
{urls}        </SegmentList>
      </Representation>
    </AdaptationSet>
  </Period>
</MPD>"#
    )
}

/// Décode `path` avec ffmpeg en jetant la sortie ; renvoie (succès, stderr).
/// Un décodage propre (succès + stderr vide) prouve que le déchiffrement a
/// produit un flux H.264 valide (donc la bonne clé a été utilisée).
fn ffmpeg_decode_check(ffmpeg: &Path, path: &Path) -> (bool, String) {
    let out = Command::new(ffmpeg)
        .args(["-hide_banner", "-loglevel", "error", "-i"])
        .arg(path)
        .args(["-f", "null", "-"])
        .output()
        .expect("lancer ffmpeg pour la vérification de décodage");
    (out.status.success(), String::from_utf8_lossy(&out.stderr).to_string())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "exige le vrai ffmpeg.exe ; lancer avec --ignored"]
async fn dash_clearkey_decrypts_offline() {
    let ffmpeg = ffmpeg_path();
    assert!(
        ffmpeg.exists(),
        "ffmpeg introuvable en {} — déposez le vrai binaire avant de lancer ce smoke-test",
        ffmpeg.display()
    );

    let dir = tempdir().unwrap();
    let (init, segs) = build_encrypted_fixture(&ffmpeg, dir.path());
    let n = segs.len();
    eprintln!("fixture chiffrée : init={} octets, {} segments média", init.len(), n);

    // Bind d'abord pour connaître le port, puis construit le MPD (la laurl en a
    // besoin), puis sert tout sur ce même serveur.
    let server = Server::http("127.0.0.1:0").expect("bind port éphémère");
    let port = server.server_addr().to_ip().unwrap().port();
    let mpd = build_mpd(port, n).into_bytes();
    serve(server, mpd, init, segs);

    let fetcher = ReqwestFetcher::new();
    let remux = CliRemuxer { ffmpeg: ffmpeg.clone() };
    let dest = dir.path().join("out.mp4");
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel::<Progress>();
    let sel = DashSelection { video_index: 0, audio_lang: None };
    let mpd_url = format!("http://127.0.0.1:{port}/manifest.mpd");

    let outcome = download_dash(
        &fetcher, &remux, "ck1", &mpd_url, &sel, &dest,
        DownloadHandle::new(), Arc::new(RateLimiter::unlimited()), tx, &RequestHeaders::default(),
    )
    .await
    .expect("le téléchargement DASH ClearKey doit réussir");
    assert_eq!(outcome, Outcome::Completed);
    assert!(dest.exists(), "le .mp4 de sortie doit exister");

    // 1. Structure : le déchiffrement a retiré les boîtes CENC et conservé l'avc1 clair.
    let bytes = std::fs::read(&dest).unwrap();
    for tag in [b"senc".as_slice(), b"encv", b"tenc", b"sinf"] {
        assert!(
            !contains(&bytes, tag),
            "la sortie déchiffrée ne doit plus contenir la boîte CENC `{}`",
            String::from_utf8_lossy(tag)
        );
    }
    assert!(contains(&bytes, b"moov"), "mp4 de sortie invalide (pas de moov)");
    assert!(contains(&bytes, b"avc1"), "la sortie doit exposer un échantillon avc1 clair");

    // 2. Décodage : sans erreur => la bonne clé ClearKey a bien déchiffré les samples.
    let (ok, stderr) = ffmpeg_decode_check(&ffmpeg, &dest);
    assert!(ok && stderr.trim().is_empty(),
        "décodage de la sortie échoué (mauvaise clé ?) — stderr:\n{stderr}");

    eprintln!("OK : DASH ClearKey déchiffré et décodé proprement ({} octets)", bytes.len());
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}
