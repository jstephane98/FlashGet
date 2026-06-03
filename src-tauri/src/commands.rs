use crate::state::AppState;
use download_core::engine::{self, DownloadHandle};
use download_core::http::ByteFetcher;
use download_core::model::{Download, DownloadStatus, Priority, Progress, Settings, Category, CategoryDirs};
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
pub(crate) async fn add_url(app: &AppHandle, url: &str, dest_dir: &str) -> Result<Download, String> {
    let fetcher = { app.state::<AppState>().fetcher.clone() };
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(url))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
    let file_name = probe.file_name.clone().unwrap_or_else(|| "download.bin".to_string());

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
    add_url(&app, &args.url, &args.dest_dir).await
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
    let id_for_dl = id.clone();
    let handle_for_err = handle.clone();
    let (fetcher, limiter) = {
        let st = app.state::<AppState>();
        (st.fetcher.clone(), st.limiter.clone())
    };
    tauri::async_runtime::spawn(async move {
        match engine::download(&fetcher, &id_for_dl, &url, &dest, handle, limiter, tx).await {
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
