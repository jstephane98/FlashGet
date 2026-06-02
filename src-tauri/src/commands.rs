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
    let probe = fetcher.probe(&args.url).await.map_err(|e| e.to_string())?;
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

    let handle = DownloadHandle::new();
    {
        let state = app.state::<AppState>();
        state
            .handles
            .lock()
            .unwrap()
            .insert(id.clone(), handle.clone());
    }

    let (tx, mut rx) = mpsc::unbounded_channel::<Progress>();

    // Tâche d'émission : calcule la vitesse et relaie vers l'UI + persiste.
    let app_for_rx = app.clone();
    let id_for_rx = id.clone();
    tokio::spawn(async move {
        let mut last_t = Instant::now();
        let mut last_bytes = 0u64;
        while let Some(mut p) = rx.recv().await {
            let now = Instant::now();
            let dt = now.duration_since(last_t).as_secs_f64();
            if dt >= 0.5 {
                let delta = p.downloaded_bytes.saturating_sub(last_bytes);
                p.speed_bps = (delta as f64 / dt) as u64;
                last_t = now;
                last_bytes = p.downloaded_bytes;
            }
            let _ = app_for_rx.emit("download://progress", &p);
            // Persistance à chaque transition d'état importante.
            if matches!(
                p.status,
                DownloadStatus::Completed | DownloadStatus::Paused | DownloadStatus::Error
            ) {
                let st = app_for_rx.state::<AppState>();
                if let Ok(list) = st.storage.list() {
                    if let Some(mut d) = list.into_iter().find(|d| d.id == id_for_rx) {
                        d.downloaded_bytes = p.downloaded_bytes;
                        d.status = p.status;
                        let _ = st.storage.upsert(&d);
                    }
                }
            }
        }
    });

    // Tâche de téléchargement.
    let dest = PathBuf::from(&download.dest_path);
    let url = download.url.clone();
    let id_for_dl = id.clone();
    tokio::spawn(async move {
        let fetcher = ReqwestFetcher::new();
        let _ = engine::download(&fetcher, &id_for_dl, &url, &dest, handle, tx).await;
        // Nettoyage du handle une fois terminé/pausé.
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
    if let Some(h) = state.handles.lock().unwrap().get(&id) {
        h.cancel();
    }
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    Ok(())
}
