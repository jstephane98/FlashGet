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
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(url, &headers))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
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
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    add_url(&app, &args.url, &args.dest_dir, RequestHeaders::default(), None).await
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
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
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
    if mpd.dynamic {
        return Err("flux live DASH non supporté (à venir)".into());
    }
    let vlist = download_core::dash::select::video_variants(&mpd);
    if vlist.is_empty() {
        return Err("aucune variante vidéo DASH trouvée".into());
    }
    let variants: Vec<MediaVariant> = vlist.iter().map(|(_, v)| MediaVariant {
        index: v.index, resolution: v.resolution.clone(), bandwidth: v.bandwidth, est_size: v.est_size,
    }).collect();
    let probe_id = uuid::Uuid::new_v4().to_string();
    let file_name = suggested_name.unwrap_or_else(|| "video".to_string());
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
pub async fn add_media_download(app: AppHandle, probe_id: String, variant_index: u32) -> Result<Download, String> {
    enum Built { Hls { url: String, audio_url: Option<String> }, Dash { mpd_url: String, selection: String } }
    let (built, file_name, headers) = {
        let state = app.state::<AppState>();
        let mut pending = state.pending_media.lock().unwrap();
        let pm = pending.get(&probe_id).ok_or_else(|| "probe expiré".to_string())?;
        let file_name = download_core::naming::with_extension(
            &download_core::naming::choose_file_name(Some(&pm.file_name), None), "mp4");
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
            if matches!(p.status, DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error) {
                let st = app_for_rx.state::<AppState>();
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
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
