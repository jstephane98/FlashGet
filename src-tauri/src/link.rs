//! Analyse d'un lien pour la page « Nouveau » : un lien collé (`probe_link`) ou capturé
//! par l'extension (`capture`) donne une sonde typée `LinkProbe` (fichier simple, ou média
//! avec ses formats), mémorisée dans `AppState.pending_links` jusqu'à la confirmation.
//! Aucune création de téléchargement ici.

use crate::commands::{self, MediaVariant};
use crate::state::AppState;
use download_core::dash::mpd::Mpd;
use download_core::hls::playlist::MasterPlaylist;
use download_core::http::ByteFetcher;
use download_core::model::{Category, ProbeResult, RequestHeaders};
use download_core::ytdlp::{build_media_formats, FormatKind, MediaFormat, MediaInfo, YtdlpFormat};
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Message affiché quand une page ne contient aucune vidéo exploitable.
pub const NO_VIDEO: &str = "Aucune vidéo trouvée sur cette page.";

/// Durée de vie d'une sonde jamais confirmée (même règle que `pending_media`).
const PENDING_TTL: Duration = Duration::from_secs(30 * 60);

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "snake_case")]
pub enum MediaSource {
    Hls,
    Dash,
    Ytdlp,
}

#[derive(Serialize, Clone, Debug)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LinkProbe {
    File {
        probe_id: String,
        url: String,
        file_name: String,
        total_bytes: Option<u64>,
        supports_ranges: bool,
        category: Category,
        default_dir: String,
    },
    Media {
        probe_id: String,
        url: String,
        source: MediaSource,
        info: MediaInfo,
        formats: Vec<MediaFormat>,
        file_name: String,
        video_dir: String,
        music_dir: String,
    },
}

impl LinkProbe {
    pub fn probe_id(&self) -> &str {
        match self {
            LinkProbe::File { probe_id, .. } | LinkProbe::Media { probe_id, .. } => probe_id,
        }
    }
}

/// Sonde en attente de confirmation. `hls` / `dash` gardent le manifeste parsé pour
/// reconstruire le téléchargement sans nouvel aller-retour réseau.
#[allow(dead_code)] // `headers`, `hls`, `dash` : lus par `confirm_link` (Task 6).
pub struct PendingLink {
    pub probe: LinkProbe,
    pub headers: RequestHeaders,
    pub hls: Option<MasterPlaylist>,
    pub dash: Option<Mpd>,
    pub created: Instant,
}

/// Indice sur la nature du lien (le `kind` d'une capture de l'extension).
#[derive(Debug, Clone, PartialEq)]
pub enum ProbeHint {
    /// Lien collé ou fichier capturé : sonde HTTP puis routage (`classify_probe`).
    Auto,
    Hls,
    Dash,
    /// Page vidéo : extraction yt-dlp.
    Page,
    /// Bouton vidéo de l'extension : yt-dlp d'abord, puis repli sur le flux sniffé.
    Smart { fallback_url: Option<String>, fallback_kind: Option<String> },
}

/// Route choisie par `Auto` à partir de l'URL et du content-type.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProbeRoute {
    File,
    Hls,
    Dash,
    Page,
}

/// Partie pure de `ProbeHint::Auto`. L'extension du chemin (`.m3u8`, `.mpd`) prime sur le
/// content-type (les CDN servent souvent les manifestes en `application/octet-stream`).
pub fn classify_probe(url: &str, content_type: Option<&str>) -> ProbeRoute {
    let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
    if path.ends_with(".m3u8") {
        return ProbeRoute::Hls;
    }
    if path.ends_with(".mpd") {
        return ProbeRoute::Dash;
    }
    let mime = content_type
        .and_then(|c| c.split(';').next())
        .map(|m| m.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match mime.as_str() {
        "application/vnd.apple.mpegurl" | "application/x-mpegurl" | "audio/mpegurl" | "audio/x-mpegurl" => ProbeRoute::Hls,
        "application/dash+xml" => ProbeRoute::Dash,
        "text/html" | "application/xhtml+xml" => ProbeRoute::Page,
        _ => ProbeRoute::File,
    }
}

/// Indice correspondant au `kind` d'une capture (absent ou inconnu → `Auto`).
pub fn hint_for_kind(kind: Option<&str>, fallback_url: Option<String>, fallback_kind: Option<String>) -> ProbeHint {
    match kind {
        Some("smart") => ProbeHint::Smart { fallback_url, fallback_kind },
        Some("hls") => ProbeHint::Hls,
        Some("dash") => ProbeHint::Dash,
        Some("page") => ProbeHint::Page,
        _ => ProbeHint::Auto,
    }
}

/// Refuse une sonde média sans aucun format proposable.
pub fn media_or_error(info: MediaInfo, formats: Vec<MediaFormat>) -> Result<(MediaInfo, Vec<MediaFormat>), String> {
    if formats.is_empty() {
        return Err(NO_VIDEO.to_string());
    }
    Ok((info, formats))
}

/// Formats yt-dlp proposés. `build_media_formats` ajoute toujours l'entrée « MP3
/// (conversion) » : s'il n'y a aucun autre format (aucun flux audio/vidéo réel, ex.
/// storyboards seulement), il n'y a rien à télécharger → liste vide.
pub fn ytdlp_formats(raw: &[YtdlpFormat]) -> Vec<MediaFormat> {
    let formats = build_media_formats(raw);
    if !formats.iter().any(|f| f.format_id != "mp3") {
        return Vec::new();
    }
    formats
}

/// Un format `Av` par variante HLS/DASH ; recommandé = la variante de plus haut débit.
pub fn variant_formats(variants: &[MediaVariant]) -> Vec<MediaFormat> {
    let best = variants
        .iter()
        .enumerate()
        .max_by_key(|(_, v)| v.bandwidth)
        .map(|(i, _)| i);
    variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let height = v
                .resolution
                .as_deref()
                .and_then(|r| r.split('x').nth(1))
                .and_then(|h| h.trim().parse::<u32>().ok());
            let label = match height {
                Some(h) => format!("{h}p"),
                None if v.bandwidth > 0 => format!("{} kb/s", v.bandwidth / 1000),
                None => "Source".to_string(),
            };
            MediaFormat {
                index: i as u32,
                kind: FormatKind::Av,
                format_id: v.index.to_string(),
                height,
                label,
                container: Some("MP4".into()),
                codecs: None,
                fps: None,
                est_size: v.est_size,
                recommended: best == Some(i),
            }
        })
        .collect()
}

/// Message d'erreur (français) pour un échec d'extraction yt-dlp.
pub fn ytdlp_error(detail: &str) -> String {
    if detail.contains("Unsupported URL") {
        return NO_VIDEO.to_string();
    }
    inaccessible(detail)
}

/// Statut HTTP de la sonde `Auto` : >= 400 = lien mort (lien signé expiré, 404, erreur
/// d'API…), sauf 416 (Range refusé sur un fichier vide ou déjà complet : le lien répond).
pub fn check_http_status(status: u16) -> Result<(), String> {
    if status >= 400 && status != 416 {
        return Err(inaccessible(format!("HTTP {status}")));
    }
    Ok(())
}

fn inaccessible(detail: impl std::fmt::Display) -> String {
    format!("Lien inaccessible : {detail}")
}

/// Analyse un lien et mémorise la sonde dans `pending_links`. N'émet aucun événement.
pub async fn analyze(app: &AppHandle, url: &str, headers: RequestHeaders, hint: ProbeHint) -> Result<LinkProbe, String> {
    analyze_named(app, url, headers, hint, None).await
}

/// Comme `analyze`, avec un nom suggéré (nom de fichier du navigateur, ou titre de l'onglet
/// pour un flux HLS/DASH qui n'en a pas).
pub async fn analyze_named(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    hint: ProbeHint,
    suggested_name: Option<String>,
) -> Result<LinkProbe, String> {
    let url = url.trim();
    if url.is_empty() {
        return Err(inaccessible("lien vide"));
    }
    let pending = resolve(app, url, headers, hint, suggested_name).await?;
    let probe = pending.probe.clone();
    store_pending(app, pending);
    Ok(probe)
}

/// Capture de l'extension : `link://probing`, fenêtre au premier plan, analyse, puis
/// `link://probed` (LinkProbe) ou `link://error`.
pub async fn capture(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    hint: ProbeHint,
    suggested_name: Option<String>,
) -> Result<LinkProbe, String> {
    let _ = app.emit("link://probing", LinkProbingEvent { url: url.to_string() });
    commands::focus_main_window(app);
    match analyze_named(app, url, headers, hint, suggested_name).await {
        Ok(p) => {
            let _ = app.emit("link://probed", &p);
            Ok(p)
        }
        Err(message) => {
            let _ = app.emit("link://error", LinkErrorEvent { url: url.to_string(), message: message.clone() });
            Err(message)
        }
    }
}

#[derive(Serialize, Clone)]
pub struct LinkProbingEvent {
    pub url: String,
}

#[derive(Serialize, Clone)]
pub struct LinkErrorEvent {
    pub url: String,
    pub message: String,
}

/// Analyse d'un lien collé dans la page Nouveau : `Auto`, sans en-têtes.
/// N'émet aucun événement et ne met pas la fenêtre au premier plan.
#[tauri::command]
pub async fn probe_link(app: AppHandle, url: String) -> Result<LinkProbe, String> {
    analyze(&app, &url, RequestHeaders::default(), ProbeHint::Auto).await
}

/// Éviction TTL puis insertion.
fn store_pending(app: &AppHandle, pending: PendingLink) {
    let state = app.state::<AppState>();
    let mut map = state.pending_links.lock().unwrap();
    map.retain(|_, p| p.created.elapsed() < PENDING_TTL);
    map.insert(pending.probe.probe_id().to_string(), pending);
}

async fn resolve(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    hint: ProbeHint,
    suggested_name: Option<String>,
) -> Result<PendingLink, String> {
    match hint {
        ProbeHint::Hls => media_hls(app, url, headers, suggested_name).await,
        ProbeHint::Dash => media_dash(app, url, headers, suggested_name).await,
        ProbeHint::Page => media_ytdlp(app, url, headers).await,
        ProbeHint::Smart { fallback_url, fallback_kind } => {
            // yt-dlp d'abord (URL fraîches, sites à URL signée), puis le flux sniffé.
            let yt = media_ytdlp(app, url, headers.clone()).await;
            if yt.is_ok() {
                return yt;
            }
            if let Some(fb) = fallback_url {
                let fr = if fallback_kind.as_deref() == Some("dash") {
                    media_dash(app, &fb, headers, suggested_name).await
                } else {
                    media_hls(app, &fb, headers, suggested_name).await
                };
                if fr.is_ok() {
                    return fr;
                }
            }
            yt
        }
        ProbeHint::Auto => {
            // Manifeste reconnaissable à l'URL : pas besoin de sonde HTTP.
            match classify_probe(url, None) {
                ProbeRoute::Hls => return media_hls(app, url, headers, suggested_name).await,
                ProbeRoute::Dash => return media_dash(app, url, headers, suggested_name).await,
                _ => {}
            }
            let fetcher = { app.state::<AppState>().fetcher.clone() };
            let probe = tokio::time::timeout(Duration::from_secs(30), fetcher.probe(url, &headers))
                .await
                .map_err(|_| inaccessible("délai dépassé"))?
                .map_err(inaccessible)?;
            check_http_status(probe.status)?;
            match classify_probe(url, probe.content_type.as_deref()) {
                ProbeRoute::Page => media_ytdlp(app, url, headers).await,
                ProbeRoute::Hls => media_hls(app, url, headers, suggested_name).await,
                ProbeRoute::Dash => media_dash(app, url, headers, suggested_name).await,
                ProbeRoute::File => file_link(app, url, headers, suggested_name, probe),
            }
        }
    }
}

fn dir_of(app: &AppHandle, category: Category) -> Result<String, String> {
    Ok(crate::dest::dest_dir_for(app, None, category)?.to_string_lossy().to_string())
}

fn file_link(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    suggested_name: Option<String>,
    probe: ProbeResult,
) -> Result<PendingLink, String> {
    let file_name = download_core::naming::choose_file_name(suggested_name.as_deref(), probe.file_name.as_deref());
    let category = download_core::categorizer::categorize(&file_name);
    let default_dir = dir_of(app, category)?;
    Ok(PendingLink {
        probe: LinkProbe::File {
            probe_id: uuid::Uuid::new_v4().to_string(),
            url: url.to_string(),
            file_name,
            total_bytes: probe.total_bytes,
            supports_ranges: probe.supports_ranges,
            category,
            default_dir,
        },
        headers,
        hls: None,
        dash: None,
        created: Instant::now(),
    })
}

fn media_probe(
    app: &AppHandle,
    url: &str,
    source: MediaSource,
    info: MediaInfo,
    formats: Vec<MediaFormat>,
    file_name: String,
) -> Result<LinkProbe, String> {
    Ok(LinkProbe::Media {
        probe_id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        source,
        info,
        formats,
        file_name,
        video_dir: dir_of(app, Category::Video)?,
        music_dir: dir_of(app, Category::Music)?,
    })
}

/// `MediaInfo` d'un flux HLS/DASH : seul le titre est connu, pas de découpe.
fn stream_info(file_name: &str) -> MediaInfo {
    MediaInfo { title: file_name.to_string(), can_cut: false, ..Default::default() }
}

async fn media_hls(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<PendingLink, String> {
    let (master, variants) = commands::hls_variants(app, url, &headers).await.map_err(inaccessible)?;
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
    let (info, formats) = media_or_error(stream_info(&file_name), variant_formats(&variants))?;
    Ok(PendingLink {
        probe: media_probe(app, url, MediaSource::Hls, info, formats, file_name)?,
        headers,
        hls: Some(master),
        dash: None,
        created: Instant::now(),
    })
}

async fn media_dash(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<PendingLink, String> {
    let (mpd, variants) = commands::dash_variants(app, url, &headers).await.map_err(inaccessible)?;
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
    let (info, formats) = media_or_error(stream_info(&file_name), variant_formats(&variants))?;
    Ok(PendingLink {
        probe: media_probe(app, url, MediaSource::Dash, info, formats, file_name)?,
        headers,
        hls: None,
        dash: Some(mpd),
        created: Instant::now(),
    })
}

async fn media_ytdlp(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<PendingLink, String> {
    // Avec les en-têtes du navigateur puis sans ; le repli cookies YouTube est dans `probe_info`.
    let ((info, raw), effective_headers) = crate::ytdlp::with_header_fallback(headers, |h| async move {
        crate::ytdlp::probe_info(app, url, &h).await
    })
    .await
    .map_err(|e| ytdlp_error(&e))?;
    let (info, formats) = media_or_error(info, ytdlp_formats(&raw))?;
    let file_name = download_core::naming::sanitize_title(Some(&info.title));
    Ok(PendingLink {
        probe: media_probe(app, url, MediaSource::Ytdlp, info, formats, file_name)?,
        headers: effective_headers,
        hls: None,
        dash: None,
        created: Instant::now(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yf(id: &str, h: Option<u32>, vcodec: &str, acodec: &str) -> YtdlpFormat {
        YtdlpFormat {
            format_id: id.into(),
            height: h,
            width: None,
            ext: Some("mp4".into()),
            vcodec: Some(vcodec.into()),
            acodec: Some(acodec.into()),
            filesize: None,
            tbr: None,
            fps: None,
            abr: None,
        }
    }

    fn mv(index: u32, resolution: Option<&str>, bandwidth: u64, est_size: Option<u64>) -> MediaVariant {
        MediaVariant { index, resolution: resolution.map(String::from), bandwidth, est_size }
    }

    #[test]
    fn classify_html_is_page() {
        assert_eq!(classify_probe("https://www.youtube.com/watch?v=x", Some("text/html; charset=utf-8")), ProbeRoute::Page);
    }

    #[test]
    fn classify_m3u8_is_hls() {
        assert_eq!(classify_probe("https://cdn.x/live/master.m3u8?t=1", Some("application/octet-stream")), ProbeRoute::Hls);
        assert_eq!(classify_probe("https://x/a", Some("application/vnd.apple.mpegurl")), ProbeRoute::Hls);
    }

    #[test]
    fn classify_mpd_is_dash() {
        assert_eq!(classify_probe("https://x/manifest.mpd", None), ProbeRoute::Dash);
    }

    #[test]
    fn classify_binary_is_file() {
        assert_eq!(classify_probe("https://releases.ubuntu.com/a.iso", Some("application/octet-stream")), ProbeRoute::File);
    }

    #[test]
    fn classify_extra_cases() {
        // Content-types HLS/DASH alternatifs, casse, fragment.
        assert_eq!(classify_probe("https://x/a", Some("Application/X-MpegURL")), ProbeRoute::Hls);
        assert_eq!(classify_probe("https://x/a", Some("application/dash+xml")), ProbeRoute::Dash);
        assert_eq!(classify_probe("https://x/Master.M3U8#t", None), ProbeRoute::Hls);
        // Sans indice : fichier ; un « .m3u8 » dans la query ne compte pas.
        assert_eq!(classify_probe("https://x/a.zip", None), ProbeRoute::File);
        assert_eq!(classify_probe("https://x/get?f=a.m3u8", Some("application/zip")), ProbeRoute::File);
    }

    #[test]
    fn link_probe_serializes_with_type_tag() {
        let file = LinkProbe::File {
            probe_id: "p1".into(),
            url: "https://x/a.iso".into(),
            file_name: "a.iso".into(),
            total_bytes: Some(10),
            supports_ranges: true,
            category: Category::Program,
            default_dir: "D:/Dl".into(),
        };
        let v = serde_json::to_value(&file).unwrap();
        assert_eq!(v["type"], "file");
        assert_eq!(v["probe_id"], "p1");
        assert_eq!(v["category"], "program");
        assert_eq!(v["total_bytes"], 10);

        let media = LinkProbe::Media {
            probe_id: "p2".into(),
            url: "https://www.youtube.com/watch?v=x".into(),
            source: MediaSource::Ytdlp,
            info: MediaInfo { title: "T".into(), can_cut: true, ..Default::default() },
            formats: vec![],
            file_name: "T".into(),
            video_dir: "D:/V".into(),
            music_dir: "D:/M".into(),
        };
        let v = serde_json::to_value(&media).unwrap();
        assert_eq!(v["type"], "media");
        assert_eq!(v["source"], "ytdlp");
        assert_eq!(v["info"]["title"], "T");
        assert_eq!(v["music_dir"], "D:/M");
        assert_eq!(media.probe_id(), "p2");
    }

    #[test]
    fn probe_link_reports_error_for_page_without_video() {
        let r = media_or_error(MediaInfo::default(), vec![]);
        assert_eq!(r.unwrap_err(), "Aucune vidéo trouvée sur cette page.");

        let one = variant_formats(&[mv(0, None, 0, None)]);
        let (_, formats) = media_or_error(MediaInfo::default(), one).unwrap();
        assert_eq!(formats.len(), 1);
    }

    #[test]
    fn ytdlp_formats_empty_when_only_synthetic_mp3() {
        assert!(ytdlp_formats(&[]).is_empty());
        // Storyboards seulement (ni vidéo ni audio) → rien de téléchargeable.
        assert!(ytdlp_formats(&[yf("sb0", Some(90), "none", "none")]).is_empty());
        let f = ytdlp_formats(&[yf("18", Some(360), "avc1.42001E", "mp4a.40.2"), yf("140", None, "none", "mp4a.40.2")]);
        assert!(f.len() > 1);
    }

    #[test]
    fn dead_http_status_is_inaccessible() {
        assert!(check_http_status(200).is_ok());
        assert!(check_http_status(206).is_ok());
        assert!(check_http_status(416).is_ok());
        assert_eq!(check_http_status(404).unwrap_err(), "Lien inaccessible : HTTP 404");
        assert_eq!(check_http_status(403).unwrap_err(), "Lien inaccessible : HTTP 403");
        assert_eq!(check_http_status(500).unwrap_err(), "Lien inaccessible : HTTP 500");
    }

    #[test]
    fn ytdlp_formats_keep_heightless_video() {
        let f = ytdlp_formats(&[yf("0", None, "avc1", "mp4a")]);
        assert_eq!(f[0].format_id, "bestvideo");
        assert!(f[0].recommended);
    }

    #[test]
    fn variant_formats_one_av_per_variant() {
        let f = variant_formats(&[
            mv(0, Some("640x360"), 800_000, Some(1_000)),
            mv(1, Some("1920x1080"), 5_000_000, Some(9_000)),
            mv(2, None, 0, None),
        ]);
        assert_eq!(f.len(), 3);
        assert!(f.iter().all(|m| m.kind == FormatKind::Av));
        assert_eq!(f[0].format_id, "0");
        assert_eq!(f[1].format_id, "1");
        assert_eq!(f[0].label, "360p");
        assert_eq!(f[0].height, Some(360));
        assert_eq!(f[1].label, "1080p");
        assert_eq!(f[1].est_size, Some(9_000));
        assert_eq!(f[2].height, None);
        assert!(!f[2].label.is_empty());
        assert_eq!(f.iter().map(|m| m.index).collect::<Vec<_>>(), vec![0, 1, 2]);
        // Recommandé : la variante de plus haut débit, et elle seule.
        assert_eq!(f.iter().filter(|m| m.recommended).count(), 1);
        assert!(f[1].recommended);
    }

    #[test]
    fn hint_follows_capture_kind() {
        assert_eq!(hint_for_kind(None, None, None), ProbeHint::Auto);
        assert_eq!(hint_for_kind(Some("hls"), None, None), ProbeHint::Hls);
        assert_eq!(hint_for_kind(Some("dash"), None, None), ProbeHint::Dash);
        assert_eq!(hint_for_kind(Some("page"), None, None), ProbeHint::Page);
        assert_eq!(hint_for_kind(Some("bogus"), None, None), ProbeHint::Auto);
        assert_eq!(
            hint_for_kind(Some("smart"), Some("https://x/m.mpd".into()), Some("dash".into())),
            ProbeHint::Smart { fallback_url: Some("https://x/m.mpd".into()), fallback_kind: Some("dash".into()) }
        );
    }

    #[test]
    fn ytdlp_errors_are_french() {
        assert_eq!(ytdlp_error("yt-dlp -J a échoué: ERROR: Unsupported URL: https://x"), NO_VIDEO);
        assert_eq!(ytdlp_error("yt-dlp -J a échoué: HTTP 403"), "Lien inaccessible : yt-dlp -J a échoué: HTTP 403");
    }
}
