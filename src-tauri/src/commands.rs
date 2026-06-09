use crate::state::AppState;
use crate::state::{MediaProbe, PendingMedia};
use download_core::engine::{self, DownloadHandle};
use download_core::hls::playlist::{is_master, parse_master, parse_media, MasterPlaylist, Variant};
use download_core::hls::select::{best_variant, estimated_size, pick_audio};
use download_core::http::ByteFetcher;
use download_core::model::{Download, DownloadKind, DownloadStatus, Priority, Progress, RequestHeaders, Settings, Category, CategoryDirs};
use download_core::scheduler::pick_to_start;
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_opener::OpenerExt;
use tokio::sync::mpsc;

#[derive(serde::Deserialize)]
pub struct AddArgs {
    pub url: String,
    pub dest_dir: String,
}

/// Crée un téléchargement (statut Queued), le persiste, puis ré-évalue la file.
/// Logique réutilisable par la commande Tauri ET par le bridge navigateur.
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<Download, String> {
    add_url_with_probe(app, url, dest_dir, headers, suggested_name, None).await
}

/// Variante acceptant un probe pré-calculé (évite un double sondage réseau).
pub(crate) async fn add_url_with_probe(app: &AppHandle, url: &str, dest_dir: &str, headers: RequestHeaders, suggested_name: Option<String>, probe: Option<download_core::model::ProbeResult>) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = match probe {
        Some(p) => p,
        None => tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(url, &headers))
            .await
            .map_err(|_| "timeout while probing URL".to_string())?
            .map_err(|e| e.to_string())?,
    };
    let file_name = download_core::naming::choose_file_name(suggested_name.as_deref(), probe.file_name.as_deref());

    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = if !dest_dir.trim().is_empty() {
        PathBuf::from(dest_dir)
    } else {
        let configured = { app.state::<AppState>().storage.category_dir(category).map_err(|e| e.to_string())? };
        match configured {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;

    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: probe.total_bytes,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: probe.supports_ranges,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind: DownloadKind::Regular,
        audio_url: None,
        media_selection: None,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(app);
    Ok(download)
}

/// Crée un téléchargement (statut Queued), le persiste, puis ré-évalue la file.
/// Si l'URL pointe vers une page HTML, on délègue à yt-dlp (émet media://variants)
/// et on renvoie l'erreur sentinelle `__media_pending__` (le frontend l'ignore).
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    let headers = RequestHeaders::default();
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(&args.url, &headers))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;

    if download_core::http::is_html(&probe.content_type) {
        // Page web → tenter l'extraction yt-dlp (émet media://variants ; pas de Download ici).
        probe_ytdlp(&app, &args.url, headers).await?;
        return Err("__media_pending__".to_string());
    }
    add_url_with_probe(&app, &args.url, &args.dest_dir, headers, None, Some(probe)).await
}

#[derive(serde::Deserialize)]
pub struct AddBatchArgs {
    pub text: String,
    pub dest_dir: String,
}

#[derive(serde::Serialize)]
pub struct BatchSummary {
    pub added: usize,
    pub skipped_invalid: usize,
    pub skipped_duplicate: usize,
    pub failed: usize,
}

/// Crée un `Download` en file SANS sonder l'URL (le moteur sondera au démarrage).
/// Le nom vient de l'URL ; taille/segments inconnus. Ne ré-évalue PAS la file
/// (l'appelant le fait une seule fois après le lot).
pub(crate) async fn add_url_no_probe(
    app: &AppHandle,
    url: &str,
    dest_dir: &str,
    headers: RequestHeaders,
) -> Result<Download, String> {
    let file_name = download_core::naming::choose_file_name(
        None,
        download_core::http::file_name_from_url(url).as_deref(),
    );
    let category = download_core::categorizer::categorize(&file_name);

    // Dossier : argument explicite > dossier configuré pour la catégorie > dossier Téléchargements.
    let dest_dir = if !dest_dir.trim().is_empty() {
        PathBuf::from(dest_dir)
    } else {
        let configured = { app.state::<AppState>().storage.category_dir(category).map_err(|e| e.to_string())? };
        match configured {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;

    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: url.to_string(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind: DownloadKind::Regular,
        audio_url: None,
        media_selection: None,
    };

    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    Ok(download)
}

/// Ajoute en lot : parse le texte (une URL par ligne), crée chaque téléchargement
/// en `Queued` sans sonder, puis ré-évalue la file une seule fois. Une ligne en échec
/// (ex. dossier inaccessible) n'interrompt pas les suivantes.
#[tauri::command]
pub async fn add_batch_downloads(app: AppHandle, args: AddBatchArgs) -> Result<BatchSummary, String> {
    let parsed = download_core::batch::parse_batch(&args.text);
    let mut added = 0usize;
    let mut failed = 0usize;
    for url in &parsed.urls {
        match add_url_no_probe(&app, url, &args.dest_dir, RequestHeaders::default()).await {
            Ok(_) => added += 1,
            Err(e) => {
                eprintln!("batch: échec ajout {url}: {e}");
                failed += 1;
            }
        }
    }
    reevaluate(&app);
    Ok(BatchSummary {
        added,
        skipped_invalid: parsed.skipped_invalid,
        skipped_duplicate: parsed.skipped_duplicate,
        failed,
    })
}

#[derive(serde::Serialize, Clone)]
pub struct MediaVariant {
    pub index: u32,
    pub resolution: Option<String>,
    pub bandwidth: u64,
    pub est_size: Option<u64>,
}

#[derive(serde::Serialize, Clone)]
pub struct MediaVariantsEvent {
    pub probe_id: String,
    pub file_name: String,
    pub variants: Vec<MediaVariant>,
}

#[derive(serde::Serialize, Clone)]
pub struct MediaErrorEvent {
    pub message: String,
}

/// Corps interne : sonde un master/media HLS, mémorise le probe et émet `media://variants`.
async fn probe_hls_inner(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    suggested_name: Option<String>,
) -> Result<(), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, url, &headers))
        .await
        .map_err(|_| "délai dépassé lors de la récupération de la playlist".to_string())?
        .map_err(|e| e.to_string())?;

    let (master, total_duration) = if is_master(&text) {
        let m = parse_master(&text, url);
        let dur = if let Some(v) = m.variants.get(best_variant(&m)) {
            match tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, &v.uri, &headers)).await {
                Ok(Ok(t)) => parse_media(&t, &v.uri).total_duration(),
                _ => 0.0,
            }
        } else {
            0.0
        };
        (m, dur)
    } else {
        let media = parse_media(&text, url);
        let dur = media.total_duration();
        let m = MasterPlaylist {
            variants: vec![Variant {
                uri: url.to_string(),
                bandwidth: 0,
                resolution: None,
                audio_group: None,
            }],
            audio: vec![],
        };
        (m, dur)
    };

    if master.variants.is_empty() {
        return Err("aucune variante HLS trouvée".into());
    }

    let variants: Vec<MediaVariant> = master
        .variants
        .iter()
        .enumerate()
        .map(|(i, v)| MediaVariant {
            index: i as u32,
            resolution: v.resolution.map(|(w, h)| format!("{w}x{h}")),
            bandwidth: v.bandwidth,
            est_size: if v.bandwidth > 0 && total_duration > 0.0 {
                Some(estimated_size(v.bandwidth, total_duration))
            } else {
                None
            },
        })
        .collect();

    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        // Éviction TTL : retire les probes jamais confirmés depuis > 30 min.
        let ttl = std::time::Duration::from_secs(30 * 60);
        pending.retain(|_, pm| pm.created.elapsed() < ttl);
        pending.insert(
            probe_id.clone(),
            PendingMedia { probe: MediaProbe::Hls(master), headers, file_name: file_name.clone(), source_url: url.to_string(), created: std::time::Instant::now() },
        );
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    Ok(())
}

/// Sonde un master/media HLS ; en cas d'échec, émet `media://error` et fait surgir la fenêtre.
pub(crate) async fn probe_hls(
    app: &AppHandle,
    url: &str,
    headers: RequestHeaders,
    suggested_name: Option<String>,
) -> Result<(), String> {
    let result = probe_hls_inner(app, url, headers, suggested_name).await;
    if let Err(e) = &result {
        let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
        if let Some(win) = app.get_webview_window("main") {
            let _ = win.unminimize();
            let _ = win.show();
            let _ = win.set_focus();
        }
    }
    result
}

/// Corps interne : sonde un MPD DASH, mémorise le probe et émet `media://variants`.
async fn probe_dash_inner(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<(), String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let text = tokio::time::timeout(std::time::Duration::from_secs(30), fetch_text(&fetcher, url, &headers))
        .await
        .map_err(|_| "délai dépassé lors de la récupération du MPD".to_string())?
        .map_err(|e| e.to_string())?;
    let mpd = download_core::dash::mpd::parse_mpd(&text, url).map_err(|e| e.to_string())?;
    let vlist = download_core::dash::select::video_variants(&mpd);
    if vlist.is_empty() {
        return Err("aucune variante vidéo DASH trouvée".into());
    }
    let variants: Vec<MediaVariant> = vlist.iter().map(|(_, v)| MediaVariant {
        index: v.index, resolution: v.resolution.clone(), bandwidth: v.bandwidth, est_size: v.est_size,
    }).collect();
    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = download_core::naming::sanitize_title(suggested_name.as_deref());
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let ttl = std::time::Duration::from_secs(30 * 60);
        pending.retain(|_, pm| pm.created.elapsed() < ttl);
        pending.insert(probe_id.clone(), PendingMedia { probe: MediaProbe::Dash(mpd), headers, file_name: file_name.clone(), source_url: url.to_string(), created: std::time::Instant::now() });
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus();
    }
    Ok(())
}

/// Sonde un MPD DASH ; en cas d'échec, émet `media://error` et fait surgir la fenêtre.
pub(crate) async fn probe_dash(app: &AppHandle, url: &str, headers: RequestHeaders, suggested_name: Option<String>) -> Result<(), String> {
    let result = probe_dash_inner(app, url, headers, suggested_name).await;
    if let Err(e) = &result {
        let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
        if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    }
    result
}

/// Sonde une page via yt-dlp ; émet media://variants ou media://error.
pub(crate) async fn probe_ytdlp(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<(), String> {
    let result = probe_ytdlp_inner(app, url, headers).await;
    if let Err(e) = &result {
        let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
        if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    }
    result
}

/// Bouton vidéo intelligent : yt-dlp d'abord (ré-extrait des URL fraîches, gère les sites
/// à URL signée), puis repli sur le flux HLS/DASH sniffé si yt-dlp ne trouve rien.
/// Émet `media://error` une seule fois si tout échoue.
pub(crate) async fn probe_smart(
    app: &AppHandle,
    page_url: &str,
    suggested_name: Option<String>,
    fallback_url: Option<String>,
    fallback_kind: Option<String>,
    headers: RequestHeaders,
) -> Result<(), String> {
    let yt = probe_ytdlp_inner(app, page_url, headers.clone()).await;
    if yt.is_ok() {
        return Ok(());
    }
    if let Some(fb) = fallback_url {
        // Repli : pas de titre via le moteur direct → on utilise le titre de la page
        // (tab.title) transmis par l'extension, sinon « video » par défaut.
        let fr = if fallback_kind.as_deref() == Some("dash") {
            probe_dash_inner(app, &fb, headers, suggested_name).await
        } else {
            probe_hls_inner(app, &fb, headers, suggested_name).await
        };
        if fr.is_ok() {
            return Ok(());
        }
    }
    let e = yt.err().unwrap_or_else(|| "aucune vidéo trouvée sur cette page".to_string());
    let _ = app.emit("media://error", MediaErrorEvent { message: e.clone() });
    if let Some(win) = app.get_webview_window("main") {
        let _ = win.unminimize();
        let _ = win.show();
        let _ = win.set_focus();
    }
    Err(e)
}

async fn probe_ytdlp_inner(app: &AppHandle, url: &str, headers: RequestHeaders) -> Result<(), String> {
    // Certains sites (ex. YouTube) refusent l'extraction quand on transmet les en-têtes du
    // navigateur (les cookies déclenchent un downgrade de client → « Requested format is not
    // available »), alors que d'autres (ex. Facebook) en ont besoin. On essaie donc d'abord
    // AVEC les en-têtes, puis on réessaie SANS en cas d'échec (= comportement « coller l'URL »
    // qui fonctionne). Le jeu d'en-têtes qui a réussi est mémorisé pour que le téléchargement
    // soit cohérent avec la sonde.
    let has_headers =
        headers.cookie.is_some() || headers.referrer.is_some() || headers.user_agent.is_some();
    let (title, formats, effective_headers) = match crate::ytdlp::probe_formats(app, url, &headers).await {
        Ok((t, f)) => (t, f, headers),
        Err(_first) if has_headers => {
            let (t, f) = crate::ytdlp::probe_formats(app, url, &RequestHeaders::default()).await?;
            (t, f, RequestHeaders::default())
        }
        Err(e) => return Err(e),
    };
    let selected = download_core::ytdlp::select_variants(&formats);
    if selected.is_empty() {
        return Err("aucune vidéo trouvée sur cette page".into());
    }
    let format_ids: Vec<String> = selected.iter().map(|(id, _)| id.clone()).collect();
    let variants: Vec<MediaVariant> = selected
        .iter()
        .enumerate()
        .map(|(i, (_, v))| MediaVariant {
            index: i as u32,
            resolution: v.resolution.clone(),
            bandwidth: v.bandwidth,
            est_size: v.est_size,
        })
        .collect();

    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = download_core::naming::sanitize_title(Some(&title));
    {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let ttl = std::time::Duration::from_secs(30 * 60);
        pending.retain(|_, pm| pm.created.elapsed() < ttl);
        pending.insert(probe_id.clone(), PendingMedia {
            probe: MediaProbe::Ytdlp(format_ids),
            headers: effective_headers,
            file_name: file_name.clone(),
            source_url: url.to_string(),
            created: std::time::Instant::now(),
        });
    }
    let _ = app.emit("media://variants", MediaVariantsEvent { probe_id, file_name, variants });
    if let Some(win) = app.get_webview_window("main") { let _ = win.unminimize(); let _ = win.show(); let _ = win.set_focus(); }
    Ok(())
}

/// Lit entièrement une ressource texte (playlist) via le fetcher.
async fn fetch_text<F: ByteFetcher>(fetcher: &F, url: &str, headers: &RequestHeaders) -> Result<String, download_core::CoreError> {
    use futures_util::StreamExt;
    let mut stream = fetcher.fetch_range(url, None, headers).await?;
    let mut buf = Vec::new();
    while let Some(chunk) = stream.next().await {
        buf.extend_from_slice(&chunk?);
    }
    Ok(String::from_utf8_lossy(&buf).to_string())
}

/// Crée un download média (HLS ou DASH) à partir d'un probe et de la variante choisie.
#[tauri::command]
pub async fn add_media_download(app: AppHandle, probe_id: String, variant_index: u32, file_name: Option<String>) -> Result<Download, String> {
    enum Built { Hls { url: String, audio_url: Option<String> }, Dash { mpd_url: String, selection: String }, Ytdlp { url: String, format_id: String } }
    let (built, file_name, headers) = {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        // Nom édité par l'utilisateur (assaini de nouveau, défense en profondeur) ;
        // sinon repli sur le titre déjà assaini au probe.
        let stem = match file_name.as_deref() {
            Some(n) => download_core::naming::sanitize_title(Some(n)),
            None => pm.file_name.clone(),
        };
        let file_name = download_core::naming::ensure_extension(&stem, "mp4");
        let headers = pm.headers.clone();
        let built = match &pm.probe {
            MediaProbe::Hls(master) => {
                let variant = master.variants.get(variant_index as usize).ok_or_else(|| "variante invalide".to_string())?;
                let audio_url = pick_audio(master, variant).and_then(|a| a.uri.clone());
                Built::Hls { url: variant.uri.clone(), audio_url }
            }
            MediaProbe::Dash(mpd) => {
                let exists = download_core::dash::select::video_variants(mpd).iter().any(|(_, v)| v.index == variant_index);
                if !exists { return Err("variante invalide".into()); }
                let selection = serde_json::json!({ "video": variant_index, "audio": serde_json::Value::Null }).to_string();
                Built::Dash { mpd_url: pm.source_url.clone(), selection }
            }
            MediaProbe::Ytdlp(format_ids) => {
                let format_id = format_ids.get(variant_index as usize).ok_or_else(|| "variante invalide".to_string())?.clone();
                Built::Ytdlp { url: pm.source_url.clone(), format_id }
            }
        };
        let r = (built, file_name, headers);
        pending.remove(&probe_id);
        r
    };

    let dest_dir = {
        let state = app.state::<AppState>();
        match state.storage.category_dir(Category::Video).map_err(|e| e.to_string())? {
            Some(p) => PathBuf::from(p),
            None => app.path().download_dir().map_err(|e| e.to_string())?,
        }
    };
    std::fs::create_dir_all(&dest_dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;
    let dest_path = dest_dir.join(&file_name);
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("nom de fichier invalide".into());
    }

    let (kind, url, audio_url, media_selection) = match built {
        Built::Hls { url, audio_url } => (DownloadKind::Hls, url, audio_url, None),
        Built::Dash { mpd_url, selection } => (DownloadKind::Dash, mpd_url, None, Some(selection)),
        Built::Ytdlp { url, format_id } => (DownloadKind::Ytdlp, url, None, Some(format_id)),
    };
    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url,
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: None,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: false,
        error: None,
        priority: Priority::Normal,
        category: Category::Video,
        referrer: headers.referrer,
        cookie: headers.cookie,
        user_agent: headers.user_agent,
        kind,
        audio_url,
        media_selection,
    };
    {
        let state = app.state::<AppState>();
        state.storage.upsert(&download).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(download)
}

#[tauri::command]
pub fn list_downloads(state: State<AppState>) -> Result<Vec<Download>, String> {
    state.storage.list().map_err(|e| e.to_string())
}

/// Reprend un téléchargement (le repasse `Queued`) puis ré-évalue.
#[tauri::command]
pub fn start_download(app: AppHandle, id: String) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let d = state.storage.get_by_id(&id).map_err(|e| e.to_string())?
            .ok_or_else(|| format!("download not found: {id}"))?;
        // Ne pas reprendre un téléchargement déjà terminé.
        if d.status == DownloadStatus::Completed {
            return Ok(());
        }
        // Ne rien faire si déjà actif.
        if state.handles.lock().unwrap().contains_key(&id) {
            return Ok(());
        }
        state.storage.update_state(&id, d.downloaded_bytes, DownloadStatus::Queued)
            .map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.pause();
    }
    // Le slot se libérera quand la tâche moteur se terminera (elle ré-évalue alors).
    Ok(())
}

/// Arrête (finalise) un enregistrement live : pose le drapeau `stop` ; le moteur
/// remux le partiel et émet `Completed`. No-op si le handle n'est plus actif.
#[tauri::command]
pub fn stop_recording(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.stop();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_download(app: AppHandle, id: String) -> Result<(), String> {
    let state = app.state::<AppState>();
    let was_active = {
        let mut handles = state.handles.lock().unwrap();
        if let Some(h) = handles.get(&id) {
            h.cancel();
        }
        handles.remove(&id).is_some()
    };
    // Non actif => aucune tâche moteur ne supprimera le fichier partiel : on le fait ici.
    if !was_active {
        if let Ok(Some(d)) = state.storage.get_by_id(&id) {
            let p = std::path::PathBuf::from(&d.dest_path);
            let _ = std::fs::remove_file(&p);
            download_core::resume::clear(&p);
            let _ = std::fs::remove_dir_all(format!("{}.fghls", d.dest_path));
        }
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn set_priority(app: AppHandle, id: String, priority: Priority) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        state.storage.set_priority(&id, priority).map_err(|e| e.to_string())?;
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn get_settings(state: State<AppState>) -> Result<Settings, String> {
    state.storage.get_settings().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_settings(app: AppHandle, max_concurrent: u32, max_speed_bps: u64) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let s = Settings { max_concurrent: max_concurrent.max(1), max_speed_bps };
        state.storage.set_settings(&s).map_err(|e| e.to_string())?;
        state.limiter.set_rate(s.max_speed_bps);
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn pause_all(state: State<AppState>) -> Result<(), String> {
    let handles = state.handles.lock().unwrap();
    for h in handles.values() {
        h.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn resume_all(app: AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let downloads = state.storage.list().map_err(|e| e.to_string())?;
        for d in downloads.iter().filter(|d| d.status == DownloadStatus::Paused) {
            state.storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Queued)
                .map_err(|e| e.to_string())?;
        }
    }
    reevaluate(&app);
    Ok(())
}

#[tauri::command]
pub fn get_category_dirs(state: State<AppState>) -> Result<CategoryDirs, String> {
    state.storage.get_category_dirs().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_category_dir(state: State<AppState>, category: Category, path: Option<String>) -> Result<(), String> {
    let path = path.filter(|p| !p.trim().is_empty());
    state.storage.set_category_dir(category, path).map_err(|e| e.to_string())
}

/// Retire l'entrée de la liste SANS supprimer le fichier (distinct d'`annuler`).
/// Refuse si le téléchargement est encore actif.
#[tauri::command]
pub fn remove_from_list(state: State<AppState>, id: String) -> Result<(), String> {
    if state.handles.lock().unwrap().contains_key(&id) {
        return Err("arrêtez le téléchargement avant de le retirer".to_string());
    }
    state.storage.delete(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn clear_completed(state: State<AppState>) -> Result<(), String> {
    state.storage.clear_completed().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_file(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    app.opener().open_path(d.dest_path, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_folder(app: AppHandle, id: String) -> Result<(), String> {
    let d = app.state::<AppState>().storage.get_by_id(&id).map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))?;
    app.opener().reveal_item_in_dir(&d.dest_path).map_err(|e| e.to_string())
}

/// Ré-évalue la file : démarre les `Queued` élus selon la limite + priorités.
/// Atomique vis-à-vis de la map `handles` (verrou tenu pendant pick + launch).
fn reevaluate(app: &AppHandle) {
    let state = app.state::<AppState>();
    let settings = match state.storage.get_settings() {
        Ok(s) => s,
        Err(e) => { eprintln!("reevaluate: get_settings failed: {e}"); return; }
    };
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => { eprintln!("reevaluate: list failed: {e}"); return; }
    };
    let to_start = pick_to_start(&downloads, settings.max_concurrent);
    if to_start.is_empty() {
        return;
    }
    // Verrou tenu pour rendre pick+launch atomique (évite un double-démarrage concurrent).
    let mut handles = state.handles.lock().unwrap();
    let mut free = settings.max_concurrent.saturating_sub(handles.len() as u32);
    for id in to_start {
        if free == 0 {
            break;
        }
        if handles.contains_key(&id) {
            continue; // déjà actif
        }
        let download = match downloads.iter().find(|d| d.id == id) {
            Some(d) => d.clone(),
            None => continue,
        };
        let handle = DownloadHandle::new();
        handles.insert(id.clone(), handle.clone());
        // Marquer Downloading conditionnellement — ne jamais écraser un statut terminal.
        let _ = state.storage.mark_active(&id, download.downloaded_bytes);
        launch(app, download, handle);
        free -= 1;
    }
}

/// Au démarrage : finalise les enregistrements live interrompus (statut
/// `Recording` persisté mais aucune tâche moteur active). work dir non vide →
/// concat + remux → `Completed` ; vide → la ligne est retirée.
pub fn recover_interrupted_recordings(app: &AppHandle) {
    let state = app.state::<AppState>();
    let downloads = match state.storage.list() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("recovery: list failed: {e}");
            return;
        }
    };
    for d in downloads.into_iter().filter(|d| d.status == DownloadStatus::Recording) {
        let dest = PathBuf::from(&d.dest_path);
        let work = PathBuf::from(format!("{}.fghls", d.dest_path));
        let has_segments = std::fs::read_dir(&work)
            .map(|rd| {
                rd.filter_map(|e| e.ok()).any(|e| {
                    e.file_name()
                        .to_str()
                        .map(|n| n.ends_with(".seg") && !n.contains("_init"))
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false);
        if !has_segments {
            let _ = std::fs::remove_dir_all(&work);
            let _ = state.storage.delete(&d.id);
            continue;
        }
        // Finalise de façon asynchrone (au démarrage, hors UI critique).
        let app2 = app.clone();
        let id = d.id.clone();
        tauri::async_runtime::spawn(async move {
            let st = app2.state::<AppState>();
            let remux = crate::remux::SidecarRemuxer::new(app2.clone());
            let v_has_init = work.join("v_init.seg").exists();
            let a_has_init = work.join("a_init.seg").exists();
            let has_audio = std::fs::read_dir(&work)
                .map(|rd| {
                    rd.filter_map(|e| e.ok()).any(|e| {
                        e.file_name()
                            .to_str()
                            .map(|n| n.starts_with('a') && n.ends_with(".seg") && !n.contains("_init"))
                            .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            // Clé ClearKey persistée par le moteur DASH live (None pour HLS / clair).
            let decryption_key = std::fs::read_to_string(work.join("clearkey.hex"))
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            let audio_arg = if has_audio { Some(a_has_init) } else { None };
            match download_core::media_io::finalize_and_remux(&work, v_has_init, audio_arg, decryption_key, &remux, &dest).await {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&work);
                    let bytes = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
                    let _ = st.storage.update_state(&id, bytes, DownloadStatus::Completed);
                    let _ = app2.emit(
                        "download://progress",
                        &Progress {
                            id: id.clone(),
                            downloaded_bytes: bytes,
                            total_bytes: None,
                            speed_bps: 0,
                            status: DownloadStatus::Completed,
                            segments_done: None,
                            segments_total: None,
                        },
                    );
                }
                Err(e) => {
                    eprintln!("recovery: remux failed for {id}: {e}");
                    let _ = st
                        .storage
                        .mark_error(&id, 0, "finalisation de l'enregistrement échouée");
                }
            }
        });
    }
}

/// Ramène la fenêtre principale au premier plan (montre + désiconifie + focus).
pub(crate) fn focus_main_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Émet une notif système de fin de téléchargement (Completed/Error) si le réglage
/// `notify_on_complete` est activé. Le corps de la notif = nom du fichier (repli : id).
fn notify_terminal(app: &AppHandle, status: DownloadStatus, id: &str) {
    let st = app.state::<AppState>();
    if !st.storage.get_notify_on_complete().unwrap_or(true) {
        return;
    }
    let file_name = st
        .storage
        .get_by_id(id)
        .ok()
        .flatten()
        .map(|d| d.file_name)
        .unwrap_or_else(|| id.to_string());
    let (title, body) = match status {
        DownloadStatus::Completed => ("Téléchargement terminé", file_name),
        _ => ("Échec du téléchargement", file_name),
    };

    #[cfg(target_os = "windows")]
    {
        // Émission native : permet de gérer le clic (on_activated) — absent du plugin desktop.
        // app_id = identifiant du config = AUMID du raccourci créé par l'installateur.
        // NB : le callback se déclenche tant que l'app tourne (cas normal : elle vient d'émettre
        // la notif). Un clic depuis le Centre de notifications après fermeture de l'app ne fait rien
        // (aucun serveur COM d'activation enregistré) — limite acceptée.
        let app_id = app.config().identifier.clone();
        let handle = app.clone();
        if let Err(e) = tauri_winrt_notification::Toast::new(&app_id)
            .title(title)
            .text1(&body)
            .on_activated(move |_action| {
                // on_activated tourne sur le thread COM/WinRT, pas le thread principal Tauri :
                // marshaler la manipulation de fenêtre sur le thread principal.
                let h = handle.clone();
                let _ = handle.run_on_main_thread(move || focus_main_window(&h));
                Ok(())
            })
            .show()
        {
            eprintln!("notif Windows échouée: {e}");
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        use tauri_plugin_notification::NotificationExt;
        let _ = app.notification().builder().title(title).body(&body).show();
    }
}

/// Lance les deux tâches (relais de progression + moteur) pour un téléchargement.
fn launch(app: &AppHandle, download: Download, handle: DownloadHandle) {
    let id = download.id.clone();
    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Relais de progression : vitesse + emit + persistance des états terminaux.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tauri::async_runtime::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        let mut current_speed: u64 = 0;
        let mut last_persisted: Option<DownloadStatus> = None;
        while let Some(mut p) = rx.recv().await {
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64();
            if dt >= 0.5 {
                let delta = p.downloaded_bytes.saturating_sub(last_bytes);
                current_speed = (delta as f64 / dt) as u64;
                last_t = now;
                last_bytes = p.downloaded_bytes;
            }
            p.speed_bps = current_speed;
            let _ = app_for_rx.emit("download://progress", &p);
            let persist = matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error)
                || (p.status == DownloadStatus::Recording && last_persisted != Some(DownloadStatus::Recording));
            if persist {
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
                // Notif de fin via le canal de progression (Completed). Une seule fois par
                // transition terminale : la garde `last_persisted != Some(p.status)` est évaluée
                // AVANT la réaffectation de `last_persisted` ci-dessous. Les échecs (Error) ne
                // passent pas par ce canal : ils sont notifiés depuis le bras Err de la tâche moteur.
                if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Error)
                    && last_persisted != Some(p.status)
                {
                    notify_terminal(&app_for_rx, p.status, &id_for_rx);
                }
                last_persisted = Some(p.status);
            }
        }
    });

    // Tâche moteur.
    let app_for_dl = app.clone();
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let headers = download.request_headers();
    let id_for_dl = id.clone();
    let handle_for_err = handle.clone();
    let kind = download.kind;
    let audio_url = download.audio_url.clone();
    let media_selection = download.media_selection.clone();
    let (fetcher, limiter) = {
        let st = app.state::<AppState>();
        (st.fetcher.clone(), st.limiter.clone())
    };
    tauri::async_runtime::spawn(async move {
        let result = match kind {
            download_core::model::DownloadKind::Regular => {
                engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx, &headers).await
            }
            download_core::model::DownloadKind::Hls => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                download_core::hls::engine::download_hls(
                    &fetcher,
                    &remux,
                    &id_for_dl,
                    &url,
                    audio_url.as_deref(),
                    &dest,
                    handle,
                    limiter,
                    tx,
                    &headers,
                )
                .await
            }
            download_core::model::DownloadKind::Dash => {
                let remux = crate::remux::SidecarRemuxer::new(app_for_dl.clone());
                let selection = match media_selection.as_deref().and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok()) {
                    Some(v) => download_core::dash::engine::DashSelection {
                        video_index: v.get("video").and_then(|x| x.as_u64()).unwrap_or(0) as u32,
                        audio_lang: v.get("audio").and_then(|x| x.as_str()).map(String::from),
                    },
                    None => download_core::dash::engine::DashSelection { video_index: 0, audio_lang: None },
                };
                download_core::dash::engine::download_dash(&fetcher, &remux, &id_for_dl, &url, &selection, &dest, handle, limiter, tx, &headers).await
            }
            download_core::model::DownloadKind::Ytdlp => {
                let format_id = media_selection.clone().unwrap_or_default();
                crate::ytdlp::run_download(app_for_dl.clone(), &id_for_dl, &url, &format_id, &dest, handle, &headers, tx).await
                    .map(|()| download_core::engine::Outcome::Completed)
            }
        };
        match result {
            Ok(_) => {}
            Err(e) => {
                eprintln!("download {id_for_dl} failed: {e}");
                let bytes = handle_for_err.downloaded();
                let st = app_for_dl.state::<AppState>();
                if let Err(se) = st.storage.mark_error(&id_for_dl, bytes, &e.to_string()) {
                    eprintln!("failed to persist error for {id_for_dl}: {se}");
                }
                let _ = app_for_dl.emit("download://progress", &Progress {
                    id: id_for_dl.clone(),
                    downloaded_bytes: bytes,
                    total_bytes: None,
                    speed_bps: 0,
                    status: DownloadStatus::Error,
                    segments_done: None,
                    segments_total: None,
                });
                notify_terminal(&app_for_dl, DownloadStatus::Error, &id_for_dl);
            }
        }
        // Libérer le slot et relancer la file.
        {
            let st = app_for_dl.state::<AppState>();
            st.handles.lock().unwrap().remove(&id_for_dl);
        }
        reevaluate(&app_for_dl);
    });
}

#[derive(serde::Serialize)]
pub struct BridgeInfo {
    pub token: String,
    pub port: u16,
    pub listening: bool,
}

/// Renvoie le jeton (créé si absent), le port d'écoute et l'état du serveur local.
#[tauri::command]
pub fn get_bridge_info(state: State<AppState>) -> Result<BridgeInfo, String> {
    let token = state.storage.get_or_create_bridge_token().map_err(|e| e.to_string())?;
    Ok(BridgeInfo {
        token,
        port: crate::bridge::PORT,
        listening: state.bridge_listening.load(std::sync::atomic::Ordering::Relaxed),
    })
}

/// Régénère le jeton du bridge et renvoie la nouvelle valeur.
#[tauri::command]
pub fn regenerate_bridge_token(state: State<AppState>) -> Result<String, String> {
    state.storage.regenerate_bridge_token().map_err(|e| e.to_string())
}

/// Indique si l'app est inscrite au démarrage de Windows (lecture du registre via le plugin).
#[tauri::command]
pub fn get_autostart(app: AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|e| e.to_string())
}

/// Active/désactive le démarrage avec Windows. Le plugin enregistre la clé `Run`
/// avec l'argument `--hidden` (configuré à l'init), pour un démarrage caché dans le tray.
#[tauri::command]
pub fn set_autostart(app: AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable().map_err(|e| e.to_string())
    } else {
        manager.disable().map_err(|e| e.to_string())
    }
}
