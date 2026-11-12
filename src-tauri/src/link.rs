//! Analyse d'un lien pour la page « Nouveau » : un lien collé (`probe_link`) ou capturé
//! par l'extension (`capture`) donne une sonde typée `LinkProbe` (fichier simple, ou média
//! avec ses formats), mémorisée dans `AppState.pending_links` jusqu'à la confirmation.
//! Aucune création de téléchargement ici.

use crate::commands::{self, MediaVariant};
use crate::state::AppState;
use download_core::dash::mpd::Mpd;
use download_core::hls::playlist::MasterPlaylist;
use download_core::http::ByteFetcher;
use download_core::hls::select::pick_audio;
use download_core::model::{Category, Download, DownloadKind, DownloadStatus, Priority, ProbeResult, RequestHeaders, Settings};
use download_core::naming::{ensure_extension, sanitize_file_name, sanitize_title};
use download_core::ytdlp::{
    audio_format_ext, build_media_formats, parse_clock, pick_sub_langs, time_range, FormatKind, JobMode, MediaFormat, MediaInfo,
    MediaPrefs, YtdlpFormat, YtdlpJob, CONVERT_FORMAT_ID,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

/// Message affiché quand une page ne contient aucune vidéo exploitable.
#[allow(non_snake_case)]
pub fn NO_VIDEO() -> &'static str {
    download_core::i18n::tr("Aucune vidéo trouvée sur cette page.", "No video found on this page.")
}

/// Durée de vie d'une sonde jamais confirmée.
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
pub struct PendingLink {
    pub probe: LinkProbe,
    pub headers: RequestHeaders,
    pub hls: Option<MasterPlaylist>,
    pub dash: Option<Mpd>,
    pub created: Instant,
    /// Préférences avec lesquelles les formats ont été construits (ligne de conversion
    /// affichée) : la confirmation les reprend telles quelles, même si les réglages ont
    /// changé entre-temps. Valeurs par défaut hors yt-dlp.
    pub media_prefs: MediaPrefs,
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
        return Err(NO_VIDEO().to_string());
    }
    Ok((info, formats))
}

/// Préférences de formats tirées des réglages.
pub fn media_prefs(s: &Settings) -> MediaPrefs {
    MediaPrefs { max_height: s.video_max_height, audio_format: s.audio_format, audio_kbps: s.audio_kbps }
}

/// Formats yt-dlp proposés. `build_media_formats` ajoute toujours l'entrée « conversion
/// audio » : s'il n'y a aucun autre format (aucun flux audio/vidéo réel, ex.
/// storyboards seulement), il n'y a rien à télécharger → liste vide.
pub fn ytdlp_formats(raw: &[YtdlpFormat], prefs: &MediaPrefs) -> Vec<MediaFormat> {
    let formats = build_media_formats(raw, prefs);
    if !formats.iter().any(|f| f.format_id != CONVERT_FORMAT_ID) {
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
        return NO_VIDEO().to_string();
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

/// Route `Auto` après la sonde HTTP. Une page HTML va toujours à yt-dlp, même en HTTP
/// >= 400 (Cloudflare, mur de connexion : yt-dlp + cookies peut réussir) ; le statut n'est
/// vérifié d'emblée que pour un fichier ou un manifeste.
pub fn route_after_probe(url: &str, content_type: Option<&str>, status: u16) -> Result<ProbeRoute, String> {
    let route = classify_probe(url, content_type);
    if route != ProbeRoute::Page {
        check_http_status(status)?;
    }
    Ok(route)
}

/// Erreur d'une page dont l'extraction yt-dlp a échoué : le statut HTTP de la sonde s'il
/// était un échec (>= 400, sauf 416), sinon l'erreur de yt-dlp.
pub fn page_error(status: u16, ytdlp_err: String) -> String {
    check_http_status(status).err().unwrap_or(ytdlp_err)
}

fn inaccessible(detail: impl std::fmt::Display) -> String {
    download_core::trf!("Lien inaccessible : {detail}", "Link unreachable: {detail}")
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
        return Err(inaccessible(download_core::i18n::tr("lien vide", "empty link")));
    }
    let pending = resolve(app, url, headers, hint, suggested_name).await?;
    let probe = pending.probe.clone();
    store_pending(app, pending);
    Ok(probe)
}

/// Capture de l'extension : `link://probing`, fenêtre au premier plan, analyse, puis
/// `link://probed` (LinkProbe) ou `link://error`. Un échec garde le contexte de la capture
/// (`failed_captures`) pour que « Réessayer » le renvoie à l'identique.
pub async fn capture(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    hint: ProbeHint,
    suggested_name: Option<String>,
) -> Result<LinkProbe, String> {
    let _ = app.emit("link://probing", LinkProbingEvent { url: url.to_string() });
    commands::focus_main_window(app);
    let (kept_headers, kept_hint, kept_name) = (headers.clone(), hint.clone(), suggested_name.clone());
    match analyze_named(app, url, headers, hint, suggested_name).await {
        Ok(p) => {
            let _ = app.emit("link://probed", &p);
            Ok(p)
        }
        Err(message) => {
            let (capture_id, origin) = if has_context(&kept_headers, &kept_hint) {
                let origin = origin_from_referrer(kept_headers.referrer.as_deref());
                let id = uuid::Uuid::new_v4().to_string();
                let now = Instant::now();
                let ctx = FailedCapture { url: url.to_string(), headers: kept_headers, hint: kept_hint, filename: kept_name, origin: origin.clone(), created: now };
                let state = app.state::<AppState>();
                remember_failed(&mut state.failed_captures.lock().unwrap(), id.clone(), ctx, now);
                (Some(id), origin)
            } else {
                (None, None)
            };
            let _ = app.emit(
                "link://error",
                LinkErrorEvent { url: url.to_string(), message: message.clone(), capture_id, origin },
            );
            Err(message)
        }
    }
}

/// Durée de vie du contexte d'une capture échouée.
const FAILED_TTL: Duration = Duration::from_secs(30 * 60);
/// Nombre maximal de contextes gardés (le plus ancien est retiré).
const FAILED_CAP: usize = 20;

/// Contexte d'une capture échouée, gardé pour « Réessayer ».
#[derive(Debug, Clone)]
pub struct FailedCapture {
    pub url: String,
    pub headers: RequestHeaders,
    pub hint: ProbeHint,
    pub filename: Option<String>,
    pub origin: Option<String>,
    pub created: Instant,
}

/// Une capture mérite un contexte de « Réessayer » seulement si elle porte des cookies,
/// un référent, ou un indice autre que `Auto` ; sinon la relance simple suffit.
pub fn has_context(headers: &RequestHeaders, hint: &ProbeHint) -> bool {
    headers.cookie.is_some() || headers.referrer.is_some() || *hint != ProbeHint::Auto
}

/// Hôte du référent, `None` s'il est absent ou invalide.
pub fn origin_from_referrer(referrer: Option<&str>) -> Option<String> {
    let url = url::Url::parse(referrer?).ok()?;
    url.host_str().map(str::to_string)
}

/// Insère un contexte après avoir purgé les expirés et appliqué le plafond.
pub fn remember_failed(map: &mut HashMap<String, FailedCapture>, id: String, capture: FailedCapture, now: Instant) {
    map.retain(|_, c| now.saturating_duration_since(c.created) < FAILED_TTL);
    while map.len() >= FAILED_CAP {
        let Some(oldest) = map.iter().min_by_key(|(_, c)| c.created).map(|(k, _)| k.clone()) else { break };
        map.remove(&oldest);
    }
    map.insert(id, capture);
}

/// Retire et renvoie le contexte, `None` s'il est inconnu ou expiré.
pub fn take_failed(map: &mut HashMap<String, FailedCapture>, id: &str, now: Instant) -> Option<FailedCapture> {
    map.remove(id).filter(|c| now.saturating_duration_since(c.created) < FAILED_TTL)
}

/// « Réessayer » : relance la capture échouée avec son contexte, ou `url` seule si le
/// contexte a expiré. Émet de nouveau probing / probed / error.
#[tauri::command]
pub async fn retry_capture(app: AppHandle, capture_id: String, url: String) -> Result<LinkProbe, String> {
    let ctx = {
        let state = app.state::<AppState>();
        let mut map = state.failed_captures.lock().unwrap();
        take_failed(&mut map, &capture_id, Instant::now())
    };
    match ctx {
        Some(c) => capture(&app, &c.url, c.headers, c.hint, c.filename).await,
        None => capture(&app, &url, RequestHeaders::default(), ProbeHint::Auto, None).await,
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
    pub capture_id: Option<String>,
    pub origin: Option<String>,
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
                .map_err(|_| inaccessible(download_core::i18n::tr("délai dépassé", "timed out")))?
                .map_err(inaccessible)?;
            let status = probe.status;
            match route_after_probe(url, probe.content_type.as_deref(), status)? {
                ProbeRoute::Page => media_ytdlp(app, url, headers).await.map_err(|e| page_error(status, e)),
                ProbeRoute::Hls => media_hls(app, url, headers, suggested_name).await,
                ProbeRoute::Dash => media_dash(app, url, headers, suggested_name).await,
                ProbeRoute::File => file_link(app, url, headers, suggested_name, probe),
            }
        }
    }
}

/// Réglages courants (valeurs par défaut si la lecture échoue).
fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().storage.get_settings().unwrap_or_default()
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
        media_prefs: MediaPrefs::default(),
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
        media_prefs: MediaPrefs::default(),
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
        media_prefs: MediaPrefs::default(),
    })
}

async fn media_ytdlp(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<PendingLink, String> {
    // Avec les en-têtes du navigateur puis sans ; le repli cookies YouTube est dans `probe_info`.
    let ((info, raw), effective_headers) = crate::ytdlp::with_header_fallback(headers, |h| async move {
        crate::ytdlp::probe_info(app, url, &h).await
    })
    .await
    .map_err(|e| ytdlp_error(&e))?;
    let prefs = media_prefs(&settings_of(app));
    let (info, formats) = media_or_error(info, ytdlp_formats(&raw, &prefs))?;
    let file_name = download_core::naming::sanitize_title(Some(&info.title));
    Ok(PendingLink {
        probe: media_probe(app, url, MediaSource::Ytdlp, info, formats, file_name)?,
        headers: effective_headers,
        hls: None,
        dash: None,
        created: Instant::now(),
        media_prefs: prefs,
    })
}

// --- Confirmation ---

/// Sonde inconnue, déjà confirmée ou expirée.
#[allow(non_snake_case)]
pub fn EXPIRED() -> &'static str {
    download_core::i18n::tr("Analyse expirée, relance l'analyse.", "The analysis expired, run it again.")
}
#[allow(non_snake_case)]
fn NO_FORMAT() -> &'static str {
    download_core::i18n::tr("Format introuvable", "Format not found")
}
#[allow(non_snake_case)]
fn BAD_CUT() -> &'static str {
    download_core::i18n::tr("Découpe invalide", "Invalid clip")
}
#[allow(non_snake_case)]
fn NO_CUT() -> &'static str {
    download_core::i18n::tr("Découpe non disponible pour ce flux", "Clipping isn't available for this stream")
}

/// Découpe demandée (`"1:30"`, `"90"`, `"01:02:03"`) ; `end` absent ou vide = jusqu'à la fin.
#[derive(Deserialize, Debug)]
pub struct TimeCut {
    pub start: String,
    pub end: Option<String>,
}

/// Choix de l'utilisateur sur la page Nouveau pour une sonde.
#[derive(Deserialize, Debug)]
pub struct ConfirmLink {
    pub probe_id: String,
    /// Obligatoire pour un média (indice dans `formats`).
    pub format_index: Option<u32>,
    pub subtitles: bool,
    /// yt-dlp seulement.
    pub cut: Option<TimeCut>,
    pub dest_dir: Option<String>,
    /// AVEC l'extension, telle qu'affichée à l'utilisateur.
    pub file_name: String,
    /// `false` = `Held`.
    pub start_now: bool,
}

/// Téléchargement prêt à enregistrer, sauf `dest_path` (résolu par `confirm_link`
/// dans le dossier de `category_for_dir`).
#[derive(Debug)]
pub struct PlannedDownload {
    pub download: Download,
    pub category_for_dir: Category,
}

/// Section yt-dlp `*début-fin` d'une découpe, validée contre la durée connue.
fn cut_range(cut: &TimeCut, duration_s: Option<u64>) -> Result<String, String> {
    let bad = || BAD_CUT().to_string();
    let start = parse_clock(&cut.start).ok_or_else(bad)?;
    let end = match cut.end.as_deref().map(str::trim).filter(|e| !e.is_empty()) {
        Some(e) => Some(parse_clock(e).ok_or_else(bad)?),
        None => None,
    };
    if end.is_some_and(|e| e <= start) {
        return Err(bad());
    }
    if let Some(d) = duration_s {
        if start >= d || end.is_some_and(|e| e > d) {
            return Err(bad());
        }
    }
    Ok(time_range(start, end))
}

/// Extension imposée par le format : celle du format de conversion de l'analyse (`prefs`),
/// extension native pour un audio natif (`m4a` à défaut de conteneur connu), `.mp4` sinon.
fn format_extension(f: &MediaFormat, prefs: &MediaPrefs) -> String {
    match f.kind {
        FormatKind::Audio if f.format_id == CONVERT_FORMAT_ID => audio_format_ext(prefs.audio_format).into(),
        FormatKind::Audio => f
            .container
            .as_deref()
            .map(|c| c.trim().to_ascii_lowercase())
            .filter(|c| !c.is_empty())
            .unwrap_or_else(|| "m4a".into()),
        _ => "mp4".into(),
    }
}

/// Transforme une sonde et les choix de l'utilisateur en `Download` (fonction pure :
/// ni disque, ni état). `dest_path` reste vide. La conversion audio suit
/// `pending.media_prefs` (ce que l'utilisateur a vu) ; `settings` ne sert qu'aux langues de
/// sous-titres.
pub fn plan_download(pending: &PendingLink, req: &ConfirmLink, now_id: String, settings: &Settings) -> Result<PlannedDownload, String> {
    let headers = &pending.headers;
    let status = if req.start_now { DownloadStatus::Queued } else { DownloadStatus::Held };
    let base = |url: &str, file_name: String, category: Category, kind: DownloadKind| Download {
        id: now_id.clone(),
        url: url.to_string(),
        file_name,
        dest_path: String::new(),
        total_bytes: None,
        downloaded_bytes: 0,
        status,
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: headers.referrer.clone(),
        cookie: headers.cookie.clone(),
        user_agent: headers.user_agent.clone(),
        kind,
        audio_url: None,
        media_selection: None,
        thumbnail_url: None,
        duration_s: None,
        created_at: None,
        started_at: None,
        completed_at: None,
        queue_rank: None,
    };

    match &pending.probe {
        LinkProbe::File { url, total_bytes, supports_ranges, category, .. } => {
            if req.cut.is_some() {
                return Err(NO_CUT().into());
            }
            let mut d = base(url, sanitize_file_name(&req.file_name), *category, DownloadKind::Regular);
            d.total_bytes = *total_bytes;
            d.supports_ranges = *supports_ranges;
            Ok(PlannedDownload { download: d, category_for_dir: *category })
        }
        LinkProbe::Media { url, source, info, formats, .. } => {
            let fmt = req
                .format_index
                .and_then(|i| formats.get(i as usize))
                .ok_or_else(|| NO_FORMAT().to_string())?;
            let audio = fmt.kind == FormatKind::Audio;
            let category = if audio { Category::Music } else { Category::Video };
            let file_name = ensure_extension(&sanitize_title(Some(&req.file_name)), &format_extension(fmt, &pending.media_prefs));
            let mut d = base(url, file_name, category, DownloadKind::Ytdlp);
            d.thumbnail_url = info.thumbnail.clone();
            d.duration_s = info.duration_s;
            match source {
                MediaSource::Ytdlp => {
                    let convert = audio && fmt.format_id == CONVERT_FORMAT_ID;
                    let mode = match fmt.kind {
                        FormatKind::Av => JobMode::Av,
                        FormatKind::Video => JobMode::Video,
                        FormatKind::Audio => JobMode::Audio,
                    };
                    // Sous-titres : incrustés dans une vidéo seulement (jamais pour l'audio).
                    let subs = if req.subtitles && !audio {
                        Some(pick_sub_langs(&info.subtitle_langs, &settings.sub_langs)).filter(|l| !l.is_empty())
                    } else {
                        None
                    };
                    let time_range = req.cut.as_ref().map(|c| cut_range(c, info.duration_s)).transpose()?;
                    let job = YtdlpJob {
                        format_id: if convert { "bestaudio".into() } else { fmt.format_id.clone() },
                        mode,
                        time_range,
                        subs,
                        convert_mp3: convert,
                        audio_format: convert.then(|| audio_format_ext(pending.media_prefs.audio_format).to_string()),
                        audio_kbps: convert.then_some(pending.media_prefs.audio_kbps),
                    };
                    d.media_selection = Some(serde_json::to_string(&job).map_err(|e| e.to_string())?);
                }
                MediaSource::Hls => {
                    if req.cut.is_some() {
                        return Err(NO_CUT().into());
                    }
                    // `format_id` = indice de la variante dans la playlist maître (`variant_formats`).
                    let master = pending.hls.as_ref().ok_or_else(|| NO_FORMAT().to_string())?;
                    let variant = fmt
                        .format_id
                        .parse::<usize>()
                        .ok()
                        .and_then(|i| master.variants.get(i))
                        .ok_or_else(|| NO_FORMAT().to_string())?;
                    d.kind = DownloadKind::Hls;
                    d.url = variant.uri.clone();
                    d.audio_url = pick_audio(master, variant).and_then(|a| a.uri.clone());
                }
                MediaSource::Dash => {
                    if req.cut.is_some() {
                        return Err(NO_CUT().into());
                    }
                    // `format_id` = indice de la variante vidéo DASH ; `url` = le MPD source.
                    let video: u32 = fmt.format_id.parse().map_err(|_| NO_FORMAT().to_string())?;
                    if let Some(mpd) = &pending.dash {
                        if !download_core::dash::select::video_variants(mpd).iter().any(|(_, v)| v.index == video) {
                            return Err(NO_FORMAT().into());
                        }
                    }
                    d.kind = DownloadKind::Dash;
                    d.media_selection = Some(serde_json::json!({ "video": video, "audio": serde_json::Value::Null }).to_string());
                }
            }
            Ok(PlannedDownload { download: d, category_for_dir: category })
        }
    }
}

/// Confirme une sonde : crée le téléchargement (`Queued` ou `Held`), le persiste, retire
/// la sonde, puis relance la file si `start_now`.
#[tauri::command]
pub async fn confirm_link(app: AppHandle, req: ConfirmLink) -> Result<Download, String> {
    let settings = settings_of(&app);
    let PlannedDownload { mut download, category_for_dir } = {
        let state = app.state::<AppState>();
        let map = state.pending_links.lock().unwrap();
        let pending = map
            .get(&req.probe_id)
            .filter(|p| p.created.elapsed() < PENDING_TTL)
            .ok_or_else(|| EXPIRED().to_string())?;
        plan_download(pending, &req, uuid::Uuid::new_v4().to_string(), &settings)?
    };
    let dir = crate::dest::dest_dir_for(&app, req.dest_dir.as_deref(), category_for_dir)?;
    // Nom invalide refusé avant tout passage à la Corbeille.
    if dir.join(&download.file_name).parent() != Some(dir.as_path()) {
        return Err(download_core::i18n::tr("nom de fichier invalide", "Invalid file name").into());
    }
    // Conflit : renommer, ou (réglage « remplacer ») ancien fichier à la Corbeille.
    download.file_name = crate::dest::place_file(&app, &dir, &download.file_name).file_name;
    let dest_path = dir.join(&download.file_name);
    if dest_path.parent() != Some(dir.as_path()) {
        return Err(download_core::i18n::tr("nom de fichier invalide", "Invalid file name").into());
    }
    download.dest_path = dest_path.to_string_lossy().to_string();
    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
        state.pending_links.lock().unwrap().remove(&req.probe_id);
    }
    if req.start_now {
        commands::reevaluate(&app);
    } else {
        // Dans une plage active, le planificateur lâche tout de suite les `held` ajoutés.
        crate::queue::planner_tick(&app);
    }
    Ok(download)
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_core::model::AudioFormat;

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
    fn ytdlp_formats_empty_when_only_synthetic_convert() {
        assert!(ytdlp_formats(&[], &MediaPrefs::default()).is_empty());
        // Storyboards seulement (ni vidéo ni audio) → rien de téléchargeable.
        assert!(ytdlp_formats(&[yf("sb0", Some(90), "none", "none")], &MediaPrefs::default()).is_empty());
        let f = ytdlp_formats(&[yf("18", Some(360), "avc1.42001E", "mp4a.40.2"), yf("140", None, "none", "mp4a.40.2")], &MediaPrefs::default());
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
    fn html_page_goes_to_ytdlp_whatever_the_status() {
        let page = "https://site.example/watch/1";
        // HTML + 403 (Cloudflare, mur de connexion) : yt-dlp est tenté.
        assert_eq!(route_after_probe(page, Some("text/html; charset=utf-8"), 403), Ok(ProbeRoute::Page));
        assert_eq!(route_after_probe(page, Some("text/html"), 200), Ok(ProbeRoute::Page));
        // Fichier ou manifeste mort : refusé avant toute analyse.
        assert_eq!(
            route_after_probe("https://x/a.bin", Some("application/octet-stream"), 404).unwrap_err(),
            "Lien inaccessible : HTTP 404"
        );
        assert_eq!(
            route_after_probe("https://x/a", Some("application/vnd.apple.mpegurl"), 410).unwrap_err(),
            "Lien inaccessible : HTTP 410"
        );
        assert_eq!(route_after_probe("https://x/a.bin", Some("application/octet-stream"), 416), Ok(ProbeRoute::File));
        // Échec yt-dlp : le statut HTTP s'il était un échec, sinon l'erreur de yt-dlp.
        assert_eq!(page_error(403, NO_VIDEO().into()), "Lien inaccessible : HTTP 403");
        assert_eq!(page_error(200, NO_VIDEO().into()), NO_VIDEO());
        assert_eq!(page_error(416, "boum".into()), "boum");
    }

    #[test]
    fn ytdlp_formats_keep_heightless_video() {
        let f = ytdlp_formats(&[yf("0", None, "avc1", "mp4a")], &MediaPrefs::default());
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

    // --- confirm_link / plan_download ---

    fn mf(index: u32, kind: FormatKind, id: &str, container: Option<&str>) -> MediaFormat {
        MediaFormat {
            index,
            kind,
            format_id: id.into(),
            height: None,
            label: id.into(),
            container: container.map(String::from),
            codecs: None,
            fps: None,
            est_size: None,
            recommended: false,
        }
    }

    fn hdrs() -> RequestHeaders {
        RequestHeaders { referrer: Some("https://ref/".into()), cookie: Some("c=1".into()), user_agent: Some("UA".into()) }
    }

    fn pending(probe: LinkProbe) -> PendingLink {
        PendingLink { probe, headers: hdrs(), hls: None, dash: None, created: Instant::now(), media_prefs: MediaPrefs::default() }
    }

    fn file_pending() -> PendingLink {
        pending(LinkProbe::File {
            probe_id: "f1".into(),
            url: "https://x/a.iso".into(),
            file_name: "a.iso".into(),
            total_bytes: Some(1234),
            supports_ranges: true,
            category: Category::Program,
            default_dir: "D:/Dl".into(),
        })
    }

    fn yt_info() -> MediaInfo {
        MediaInfo {
            title: "Clip".into(),
            thumbnail: Some("https://i/t.jpg".into()),
            duration_s: Some(768),
            subtitle_langs: vec!["en".into(), "fr".into()],
            can_cut: true,
            ..Default::default()
        }
    }

    fn media_pending(source: MediaSource) -> PendingLink {
        pending(LinkProbe::Media {
            probe_id: "m1".into(),
            url: "https://www.youtube.com/watch?v=x".into(),
            source,
            info: yt_info(),
            formats: vec![
                mf(0, FormatKind::Av, "137", Some("MP4")),
                mf(1, FormatKind::Video, "137", Some("MP4")),
                mf(2, FormatKind::Audio, "convert", Some("MP3")),
                mf(3, FormatKind::Audio, "140", Some("M4A")),
            ],
            file_name: "Clip".into(),
            video_dir: "D:/V".into(),
            music_dir: "D:/M".into(),
        })
    }

    fn req(format_index: Option<u32>, file_name: &str) -> ConfirmLink {
        ConfirmLink {
            probe_id: "m1".into(),
            format_index,
            subtitles: false,
            cut: None,
            dest_dir: None,
            file_name: file_name.into(),
            start_now: true,
        }
    }

    fn job_of(p: &PlannedDownload) -> YtdlpJob {
        serde_json::from_str(p.download.media_selection.as_deref().unwrap()).unwrap()
    }

    fn cut(start: &str, end: Option<&str>) -> Option<TimeCut> {
        Some(TimeCut { start: start.into(), end: end.map(String::from) })
    }

    #[test]
    fn confirm_file_queued_or_held() {
        let p = file_pending();
        let r = req(None, "a.iso");
        let planned = plan_download(&p, &r, "id1".into(), &Settings::default()).unwrap();
        let d = &planned.download;
        assert_eq!(d.id, "id1");
        assert_eq!(d.status, DownloadStatus::Queued);
        assert_eq!(d.kind, DownloadKind::Regular);
        assert_eq!(d.url, "https://x/a.iso");
        assert_eq!(d.file_name, "a.iso");
        assert_eq!(d.total_bytes, Some(1234));
        assert!(d.supports_ranges);
        assert_eq!(d.category, Category::Program);
        assert_eq!(planned.category_for_dir, Category::Program);
        assert_eq!(d.referrer.as_deref(), Some("https://ref/"));
        assert_eq!(d.cookie.as_deref(), Some("c=1"));
        assert_eq!(d.user_agent.as_deref(), Some("UA"));

        let held = ConfirmLink { start_now: false, ..req(None, "a.iso") };
        assert_eq!(plan_download(&p, &held, "id2".into(), &Settings::default()).unwrap().download.status, DownloadStatus::Held);
    }

    #[test]
    fn confirm_file_long_name_keeps_extension() {
        let long = format!("{}.iso", "n".repeat(196));
        assert_eq!(long.chars().count(), 200);
        let d = plan_download(&file_pending(), &req(None, &long), "id".into(), &Settings::default()).unwrap().download;
        assert!(d.file_name.ends_with(".iso"), "{}", d.file_name);
        assert!(d.file_name.chars().count() <= 120);
    }

    #[test]
    fn confirm_media_replaces_other_media_extension() {
        // Nom saisi « x.mp4 » puis format MP3 : « x.mp3 », jamais « x.mp4.mp3 ».
        let p = media_pending(MediaSource::Ytdlp);
        assert_eq!(plan_download(&p, &req(Some(2), "x.mp4"), "id".into(), &Settings::default()).unwrap().download.file_name, "x.mp3");
        assert_eq!(plan_download(&p, &req(Some(3), "x.mp4"), "id".into(), &Settings::default()).unwrap().download.file_name, "x.m4a");
    }

    #[test]
    fn confirm_av_builds_job() {
        let p = media_pending(MediaSource::Ytdlp);
        let r = ConfirmLink { subtitles: true, ..req(Some(0), "Mon clip") };
        let planned = plan_download(&p, &r, "id".into(), &Settings::default()).unwrap();
        let job = job_of(&planned);
        assert_eq!(job.format_id, "137");
        assert_eq!(job.mode, JobMode::Av);
        assert_eq!(job.subs, Some(vec!["fr".to_string(), "en".to_string()]));
        assert!(!job.convert_mp3);
        assert_eq!(job.time_range, None);
        assert_eq!(planned.download.kind, DownloadKind::Ytdlp);
        assert_eq!(planned.download.url, "https://www.youtube.com/watch?v=x");
        assert_eq!(planned.download.category, Category::Video);
        assert_eq!(planned.category_for_dir, Category::Video);
        assert_eq!(planned.download.file_name, "Mon clip.mp4");

        // Vidéo seule : mode Video, toujours .mp4.
        let v = plan_download(&p, &req(Some(1), "Mon clip.mp4"), "id".into(), &Settings::default()).unwrap();
        assert_eq!(job_of(&v).mode, JobMode::Video);
        assert_eq!(job_of(&v).subs, None);
        assert_eq!(v.download.file_name, "Mon clip.mp4");
    }

    #[test]
    fn confirm_audio_mp3_is_music() {
        let p = media_pending(MediaSource::Ytdlp);
        let planned = plan_download(&p, &req(Some(2), "Chanson"), "id".into(), &Settings::default()).unwrap();
        let job = job_of(&planned);
        assert_eq!(job.format_id, "bestaudio");
        assert_eq!(job.mode, JobMode::Audio);
        assert!(job.convert_mp3);
        assert_eq!(planned.download.category, Category::Music);
        assert_eq!(planned.category_for_dir, Category::Music);
        assert_eq!(planned.download.file_name, "Chanson.mp3");
    }

    #[test]
    fn confirm_native_audio_keeps_ext() {
        let p = media_pending(MediaSource::Ytdlp);
        let planned = plan_download(&p, &req(Some(3), "Chanson"), "id".into(), &Settings::default()).unwrap();
        let job = job_of(&planned);
        assert_eq!(job.format_id, "140");
        assert_eq!(job.mode, JobMode::Audio);
        assert!(!job.convert_mp3);
        assert_eq!(planned.download.file_name, "Chanson.m4a");
        assert_eq!(planned.category_for_dir, Category::Music);
    }

    #[test]
    fn confirm_audio_ignores_subtitles() {
        let p = media_pending(MediaSource::Ytdlp);
        for idx in [2, 3] {
            let r = ConfirmLink { subtitles: true, ..req(Some(idx), "Chanson") };
            assert_eq!(job_of(&plan_download(&p, &r, "id".into(), &Settings::default()).unwrap()).subs, None);
        }
    }

    #[test]
    fn confirm_cut_valid_and_invalid() {
        let p = media_pending(MediaSource::Ytdlp);
        let ok = ConfirmLink { cut: cut("1:30", Some("2:45")), ..req(Some(0), "c") };
        assert_eq!(job_of(&plan_download(&p, &ok, "id".into(), &Settings::default()).unwrap()).time_range.as_deref(), Some("*90-165"));
        // Sans fin (ou fin vide) : jusqu'au bout.
        let open = ConfirmLink { cut: cut("1:30", None), ..req(Some(0), "c") };
        assert_eq!(job_of(&plan_download(&p, &open, "id".into(), &Settings::default()).unwrap()).time_range.as_deref(), Some("*90-inf"));
        let blank = ConfirmLink { cut: cut("1:30", Some("  ")), ..req(Some(0), "c") };
        assert_eq!(job_of(&plan_download(&p, &blank, "id".into(), &Settings::default()).unwrap()).time_range.as_deref(), Some("*90-inf"));

        for c in [cut("1:30", Some("0:10")), cut("1:30", Some("1:30")), cut("1:30", Some("20:00")), cut("abc", None), cut("0:10", Some("x"))] {
            let r = ConfirmLink { cut: c, ..req(Some(0), "c") };
            assert_eq!(plan_download(&p, &r, "id".into(), &Settings::default()).unwrap_err(), "Découpe invalide");
        }

        let mut hls = media_pending(MediaSource::Hls);
        hls.hls = Some(MasterPlaylist {
            variants: vec![download_core::hls::playlist::Variant { uri: "https://x/v0.m3u8".into(), bandwidth: 1, resolution: None, audio_group: None }],
            audio: vec![],
        });
        let r = ConfirmLink { cut: cut("1:30", Some("2:45")), ..req(Some(0), "c") };
        assert_eq!(plan_download(&hls, &r, "id".into(), &Settings::default()).unwrap_err(), "Découpe non disponible pour ce flux");
        let r = ConfirmLink { cut: cut("1:30", Some("2:45")), ..req(Some(0), "c") };
        assert_eq!(plan_download(&media_pending(MediaSource::Dash), &r, "id".into(), &Settings::default()).unwrap_err(), "Découpe non disponible pour ce flux");
    }

    #[test]
    fn confirm_media_requires_format() {
        let p = media_pending(MediaSource::Ytdlp);
        assert_eq!(plan_download(&p, &req(None, "c"), "id".into(), &Settings::default()).unwrap_err(), "Format introuvable");
        assert_eq!(plan_download(&p, &req(Some(99), "c"), "id".into(), &Settings::default()).unwrap_err(), "Format introuvable");
    }

    #[test]
    fn confirm_copies_thumbnail_and_duration() {
        let p = media_pending(MediaSource::Ytdlp);
        let d = plan_download(&p, &req(Some(0), "c"), "id".into(), &Settings::default()).unwrap().download;
        assert_eq!(d.thumbnail_url.as_deref(), Some("https://i/t.jpg"));
        assert_eq!(d.duration_s, Some(768));
        assert_eq!(d.referrer.as_deref(), Some("https://ref/"));
    }

    #[test]
    fn confirm_hls_uses_variant_and_audio() {
        use download_core::hls::playlist::{AudioRendition, Variant};
        let mut p = media_pending(MediaSource::Hls);
        if let LinkProbe::Media { formats, .. } = &mut p.probe {
            *formats = vec![mf(0, FormatKind::Av, "0", Some("MP4")), mf(1, FormatKind::Av, "1", Some("MP4"))];
        }
        p.hls = Some(MasterPlaylist {
            variants: vec![
                Variant { uri: "https://x/v0.m3u8".into(), bandwidth: 1, resolution: None, audio_group: None },
                Variant { uri: "https://x/v1.m3u8".into(), bandwidth: 2, resolution: None, audio_group: Some("aud".into()) },
            ],
            audio: vec![AudioRendition { group_id: "aud".into(), name: "fr".into(), uri: Some("https://x/a.m3u8".into()), default: true, autoselect: true }],
        });
        let r = ConfirmLink { subtitles: true, ..req(Some(1), "Flux") };
        let planned = plan_download(&p, &r, "id".into(), &Settings::default()).unwrap();
        let d = planned.download;
        assert_eq!(d.kind, DownloadKind::Hls);
        assert_eq!(d.url, "https://x/v1.m3u8");
        assert_eq!(d.audio_url.as_deref(), Some("https://x/a.m3u8"));
        assert_eq!(d.media_selection, None);
        assert_eq!(d.file_name, "Flux.mp4");
        assert_eq!(planned.category_for_dir, Category::Video);
    }

    #[test]
    fn confirm_dash_selects_video_index() {
        let mut p = media_pending(MediaSource::Dash);
        if let LinkProbe::Media { formats, url, .. } = &mut p.probe {
            *formats = vec![mf(0, FormatKind::Av, "4", Some("MP4"))];
            *url = "https://x/manifest.mpd".into();
        }
        let d = plan_download(&p, &req(Some(0), "Flux"), "id".into(), &Settings::default()).unwrap().download;
        assert_eq!(d.kind, DownloadKind::Dash);
        assert_eq!(d.url, "https://x/manifest.mpd");
        let sel: serde_json::Value = serde_json::from_str(d.media_selection.as_deref().unwrap()).unwrap();
        assert_eq!(sel, serde_json::json!({ "video": 4, "audio": null }));
    }

    #[test]
    fn plan_convert_uses_settings() {
        // Préférences de l'analyse (ce que l'utilisateur a vu) : Opus 192.
        let mut p = media_pending(MediaSource::Ytdlp);
        p.media_prefs = MediaPrefs { audio_format: AudioFormat::Opus, audio_kbps: 192, ..MediaPrefs::default() };
        // Réglages changés entre l'analyse et la confirmation : sans effet sur la conversion.
        let s = Settings { audio_format: AudioFormat::M4a, audio_kbps: 320, ..Settings::default() };
        let planned = plan_download(&p, &req(Some(2), "Chanson.mp4"), "id".into(), &s).unwrap();
        let job = job_of(&planned);
        assert_eq!(planned.download.file_name, "Chanson.opus");
        assert_eq!(planned.download.category, Category::Music);
        assert_eq!(planned.category_for_dir, Category::Music);
        assert_eq!((job.format_id.as_str(), job.mode, job.convert_mp3), ("bestaudio", JobMode::Audio, true));
        assert_eq!((job.audio_format.as_deref(), job.audio_kbps), (Some("opus"), Some(192)));
        // Audio natif : pas de conversion.
        let native = job_of(&plan_download(&p, &req(Some(3), "Chanson"), "id".into(), &s).unwrap());
        assert_eq!((native.audio_format, native.audio_kbps), (None, None));
    }

    #[test]
    fn subs_from_settings() {
        let mut p = media_pending(MediaSource::Ytdlp);
        if let LinkProbe::Media { info, .. } = &mut p.probe {
            info.subtitle_langs = vec!["en".into(), "de".into()];
        }
        let s = Settings { sub_langs: vec!["de".into()], ..Settings::default() };
        let r = ConfirmLink { subtitles: true, ..req(Some(0), "Clip") };
        assert_eq!(job_of(&plan_download(&p, &r, "id".into(), &s).unwrap()).subs, Some(vec!["de".to_string()]));
    }

    #[test]
    fn ytdlp_formats_follow_media_prefs() {
        let raw = [yf("a", Some(2160), "avc1", "none"), yf("b", Some(720), "avc1", "none"), yf("140", None, "none", "mp4a.40.2")];
        let p = MediaPrefs { max_height: 720, audio_format: AudioFormat::M4a, audio_kbps: 128 };
        let f = ytdlp_formats(&raw, &p);
        assert!(f.iter().any(|m| m.kind == FormatKind::Av && m.recommended && m.label == "720p"));
        assert!(f.iter().any(|m| m.format_id == "convert" && m.label == "M4A · 128 kb/s"));
        assert_eq!(media_prefs(&Settings { video_max_height: 480, ..Settings::default() }).max_height, 480);
    }

    #[test]
    fn confirm_request_deserializes() {
        let r: ConfirmLink = serde_json::from_value(serde_json::json!({
            "probe_id": "p", "format_index": 2, "subtitles": true,
            "cut": { "start": "0:10", "end": null }, "dest_dir": null,
            "file_name": "a.mp4", "start_now": false
        }))
        .unwrap();
        assert_eq!(r.format_index, Some(2));
        assert_eq!(r.cut.unwrap().start, "0:10");
        assert!(!r.start_now);
    }

    #[test]
    fn ytdlp_errors_are_french() {
        assert_eq!(ytdlp_error("yt-dlp -J a échoué: ERROR: Unsupported URL: https://x"), NO_VIDEO());
        assert_eq!(ytdlp_error("yt-dlp -J a échoué: HTTP 403"), "Lien inaccessible : yt-dlp -J a échoué: HTTP 403");
    }

    // --- contexte des captures échouées ---

    #[test]
    fn has_context_only_with_cookie_referrer_or_hint() {
        let none = RequestHeaders { referrer: None, cookie: None, user_agent: Some("UA".into()) };
        assert!(!has_context(&none, &ProbeHint::Auto));
        assert!(has_context(&RequestHeaders { cookie: Some("c=1".into()), ..none.clone() }, &ProbeHint::Auto));
        assert!(has_context(&RequestHeaders { referrer: Some("https://r/".into()), ..none.clone() }, &ProbeHint::Auto));
        assert!(has_context(&none, &ProbeHint::Page));
        assert!(has_context(&none, &ProbeHint::Hls));
    }

    fn failed(url: &str, created: Instant) -> FailedCapture {
        FailedCapture { url: url.into(), headers: hdrs(), hint: ProbeHint::Page, filename: Some("n.mp4".into()), origin: None, created }
    }

    #[test]
    fn remember_and_take_failed() {
        let now = Instant::now();
        let mut map = HashMap::new();
        remember_failed(&mut map, "a".into(), failed("https://x/a", now), now);
        let got = take_failed(&mut map, "a", now).unwrap();
        assert_eq!(got.url, "https://x/a");
        assert_eq!(got.hint, ProbeHint::Page);
        assert_eq!(got.filename.as_deref(), Some("n.mp4"));
        assert_eq!(got.headers, hdrs());
        assert!(take_failed(&mut map, "a", now).is_none());
    }

    #[test]
    fn failed_capture_expires() {
        let t0 = Instant::now();
        let mut map = HashMap::new();
        remember_failed(&mut map, "a".into(), failed("https://x/a", t0), t0);
        let later = t0 + FAILED_TTL + Duration::from_secs(1);
        assert!(take_failed(&mut map, "a", later).is_none());
        // Une insertion tardive purge aussi les entrées expirées.
        remember_failed(&mut map, "b".into(), failed("https://x/b", t0), t0);
        remember_failed(&mut map, "c".into(), failed("https://x/c", later), later);
        assert!(!map.contains_key("b"));
        assert!(map.contains_key("c"));
    }

    #[test]
    fn failed_capture_cap() {
        let t0 = Instant::now();
        let mut map = HashMap::new();
        for i in 0..=FAILED_CAP {
            let t = t0 + Duration::from_millis(i as u64);
            remember_failed(&mut map, format!("id{i}"), failed("https://x/", t), t);
        }
        assert_eq!(map.len(), FAILED_CAP);
        assert!(!map.contains_key("id0"));
        assert!(map.contains_key("id1"));
        assert!(map.contains_key(&format!("id{FAILED_CAP}")));
    }

    #[test]
    fn retry_unknown_capture_id() {
        let mut map: HashMap<String, FailedCapture> = HashMap::new();
        assert!(take_failed(&mut map, "inconnu", Instant::now()).is_none());
    }

    #[test]
    fn origin_from_referrer() {
        assert_eq!(super::origin_from_referrer(Some("https://www.exemple.com/a/b?c=1")), Some("www.exemple.com".to_string()));
        assert_eq!(super::origin_from_referrer(Some("pas une url")), None);
        assert_eq!(super::origin_from_referrer(Some("")), None);
        assert_eq!(super::origin_from_referrer(None), None);
    }
}
