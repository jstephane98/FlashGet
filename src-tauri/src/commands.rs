use crate::state::AppState;
use download_core::engine::{self, DownloadHandle};
use download_core::http::{ByteFetcher, ReqwestFetcher};
use download_core::model::{Download, DownloadStatus, Progress};
use std::path::PathBuf;
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::mpsc;

#[derive(serde::Deserialize)]
pub struct AddArgs {
    pub url: String,
    pub dest_dir: String,
}

/// Crée un téléchargement (statut Queued) et le persiste. Retourne l'objet créé.
#[tauri::command]
pub async fn add_download(app: AppHandle, args: AddArgs) -> Result<Download, String> {
    // Inspection préalable via le trait public du cœur (évite de modifier le crate `download-core`).
    let fetcher = ReqwestFetcher::new();
    // FIX M4 — bound the probe so a hung server can't hang the command forever.
    let probe = tokio::time::timeout(std::time::Duration::from_secs(30), fetcher.probe(&args.url))
        .await
        .map_err(|_| "timeout while probing URL".to_string())?
        .map_err(|e| e.to_string())?;
    let file_name = probe
        .file_name
        .clone()
        .unwrap_or_else(|| "download.bin".to_string());

    // Répertoire de destination : si vide, on tombe sur le dossier "Téléchargements" du système.
    let dest_dir = if args.dest_dir.trim().is_empty() {
        app.path().download_dir().map_err(|e| e.to_string())?
    } else {
        PathBuf::from(&args.dest_dir)
    };
    let dest_path = dest_dir.join(&file_name);

    // FIX C1 — defensive containment: file_name is a single sanitized component,
    // so dest_path's parent must be exactly dest_dir.
    if dest_path.parent() != Some(dest_dir.as_path()) {
        return Err("invalid file name derived from URL".to_string());
    }

    let download = Download {
        id: uuid::Uuid::new_v4().to_string(),
        url: args.url.clone(),
        file_name,
        dest_path: dest_path.to_string_lossy().to_string(),
        total_bytes: probe.total_bytes,
        downloaded_bytes: 0,
        status: DownloadStatus::Queued,
        supports_ranges: probe.supports_ranges,
        error: None,
        priority: download_core::model::Priority::Normal,
    };

    let state = app.state::<AppState>();
    state.storage.upsert(&download).map_err(|e| e.to_string())?;
    Ok(download)
}

#[tauri::command]
pub fn list_downloads(state: State<AppState>) -> Result<Vec<Download>, String> {
    state.storage.list().map_err(|e| e.to_string())
}

/// Démarre (ou reprend) un téléchargement. Lance une tâche async qui émet la progression.
#[tauri::command]
pub async fn start_download(app: AppHandle, id: String) -> Result<(), String> {
    let download = {
        let state = app.state::<AppState>();
        state
            .storage
            .list()
            .map_err(|e| e.to_string())?
            .into_iter()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("download not found: {id}"))?
    };

    // FIX I5 — guard against double-start
    {
        let state = app.state::<AppState>();
        let mut handles = state.handles.lock().unwrap();
        if handles.contains_key(&id) {
            return Err(format!("download already running: {id}"));
        }
        let handle = DownloadHandle::new();
        handles.insert(id.clone(), handle);
    }

    // FIX I2 — persist Downloading status before spawning tasks
    {
        let state = app.state::<AppState>();
        state
            .storage
            .update_state(&id, download.downloaded_bytes, DownloadStatus::Downloading)
            .map_err(|e| e.to_string())?;
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Tâche d'émission : calcule la vitesse et relaie vers l'UI + persiste.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tokio::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        // FIX I1 — keep last known speed; only recompute when dt >= 0.5
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
            // Always emit the last known rolling speed (never 0 between windows)
            p.speed_bps = current_speed;
            let _ = app_for_rx.emit("download://progress", &p);
            // FIX C1 — UPDATE-only persistence: won't recreate a deleted/cancelled row
            if matches!(
                p.status,
                DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error
            ) {
                let st = app_for_rx.state::<AppState>();
                // FIX I4 — log storage write failures instead of silently dropping
                if let Err(e) = st.storage.update_state(&id_for_rx, p.downloaded_bytes, p.status) {
                    eprintln!("failed to persist download {id_for_rx}: {e}");
                }
            }
        }
    });

    // Tâche de téléchargement.
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let id_for_dl = id.clone();
    // Retrieve the handle we inserted above
    let handle = {
        let state = app.state::<AppState>();
        let guard = state.handles.lock().unwrap();
        guard.get(&id_for_dl).cloned().unwrap()
    };
    // FIX I2 — clone the handle before it's moved into the engine so we can read
    // how many bytes were downloaded if the download fails.
    let handle_for_err = handle.clone();
    tokio::spawn(async move {
        let fetcher = ReqwestFetcher::new();
        match engine::download(&fetcher, &id_for_dl, &url, &dest, handle, tx).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("download {id_for_dl} failed: {e}");
                let bytes = handle_for_err.downloaded();
                let st = app.state::<AppState>();
                if let Err(se) = st.storage.mark_error(&id_for_dl, bytes, &e.to_string()) {
                    eprintln!("failed to persist error for {id_for_dl}: {se}");
                }
                let _ = app.emit(
                    "download://progress",
                    &Progress {
                        id: id_for_dl.clone(),
                        downloaded_bytes: bytes,
                        total_bytes: None,
                        speed_bps: 0,
                        status: DownloadStatus::Error,
                    },
                );
            }
        }
        // Nettoyage du handle une fois terminé/pausé/échoué.
        let st = app.state::<AppState>();
        st.handles.lock().unwrap().remove(&id_for_dl);
    });

    Ok(())
}

#[tauri::command]
pub fn pause_download(state: State<AppState>, id: String) -> Result<(), String> {
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.pause();
    }
    Ok(())
}

#[tauri::command]
pub fn cancel_download(state: State<AppState>, id: String) -> Result<(), String> {
    // FIX m4 — cancel signal + remove handle + delete from storage, all tight
    {
        let mut handles = state.handles.lock().unwrap();
        if let Some(h) = handles.get(&id) {
            h.cancel();
        }
        handles.remove(&id);
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    Ok(())
}
