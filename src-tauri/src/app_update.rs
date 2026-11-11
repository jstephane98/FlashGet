//! Mise à jour de FlashGet lui-même, via `tauri-plugin-updater` et les Releases GitHub.
//! Rien ne s'installe sans un clic : la vérification et le téléchargement sont séparés de
//! l'installation, et aucune vérification ne retarde la fenêtre ni n'ouvre de message d'erreur.

use crate::state::AppState;
use download_core::model::DownloadStatus;
use download_core::storage::Storage;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

const LAST_CHECK_KEY: &str = "app_update.last_check";
const RESUME_IDS_KEY: &str = "app_update.resume_ids";
const CHECK_ERR: &str = "Impossible de rechercher les mises à jour";
const DOWNLOAD_ERR: &str = "Échec du téléchargement de la mise à jour";
const INSTALL_ERR: &str = "Échec de l'installation de la mise à jour";
const BUSY: &str = "Mise à jour déjà en cours";
const NONE_DOWNLOADED: &str = "Aucune mise à jour téléchargée";

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AppUpdateInfo {
    pub version: String,
    pub date: Option<String>,
    pub notes: Option<String>,
}

/// Enregistre les identifiants à relancer après l'installation.
pub fn remember_resume_ids(storage: &Storage, ids: &[String]) {
    let json = serde_json::to_string(ids).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = storage.set_pref(RESUME_IDS_KEY, &json) {
        eprintln!("mise à jour de FlashGet : liste de reprise : {e}");
    }
}

/// Lit puis efface les identifiants enregistrés ; absents ou illisibles : liste vide.
pub fn take_resume_ids(storage: &Storage) -> Vec<String> {
    let raw = storage.get_pref(RESUME_IDS_KEY).ok().flatten();
    if raw.is_some() {
        let _ = storage.set_pref(RESUME_IDS_KEY, "[]");
    }
    raw.and_then(|r| serde_json::from_str(&r).ok()).unwrap_or_default()
}

/// Remet en `queued` les lignes encore `paused` parmi `ids` ; renvoie leur nombre.
pub fn requeue_ids(storage: &Storage, ids: &[String]) -> usize {
    if ids.is_empty() {
        return 0;
    }
    let Ok(rows) = storage.list() else { return 0 };
    let mut n = 0;
    for d in rows.iter().filter(|d| d.status == DownloadStatus::Paused && ids.contains(&d.id)) {
        if storage.update_state(&d.id, d.downloaded_bytes, DownloadStatus::Queued).is_ok() {
            n += 1;
        }
    }
    n
}

/// Au démarrage : relance les téléchargements que la mise à jour avait mis en pause.
/// À appeler avant `start_planner_loop` ; le résultat s'ajoute au nombre de reprises.
pub fn requeue_after_update(app: &AppHandle) -> usize {
    let st = app.state::<AppState>();
    let ids = take_resume_ids(&st.storage);
    requeue_ids(&st.storage, &ids)
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn info_of(u: &tauri_plugin_updater::Update) -> AppUpdateInfo {
    AppUpdateInfo {
        version: u.version.clone(),
        date: u
            .date
            .and_then(|d| chrono::DateTime::from_timestamp(d.unix_timestamp(), 0))
            .map(|d| d.to_rfc3339()),
        notes: u.body.clone().filter(|b| !b.trim().is_empty()),
    }
}

/// Interroge le point d'accès ; met l'`Update` trouvée en cache pour le téléchargement.
async fn run_check(app: &AppHandle) -> Result<Option<AppUpdateInfo>, String> {
    let updater = app.updater().map_err(|e| {
        eprintln!("mise à jour de FlashGet : {e}");
        CHECK_ERR.to_string()
    })?;
    let found = updater.check().await.map_err(|e| {
        eprintln!("mise à jour de FlashGet : {e}");
        CHECK_ERR.to_string()
    })?;
    let st = app.state::<AppState>();
    let info = found.as_ref().map(info_of);
    if found.is_none() {
        *st.pending_update.lock().unwrap() = None;
    }
    *st.checked_update.lock().unwrap() = found;
    // Une version déjà téléchargée mais dépassée n'est plus la bonne.
    if let Some(i) = &info {
        let mut pending = st.pending_update.lock().unwrap();
        if pending.as_ref().is_some_and(|(u, _)| u.version != i.version) {
            *pending = None;
        }
    }
    let _ = st.storage.set_pref(LAST_CHECK_KEY, &now_secs().to_string());
    Ok(info)
}

#[tauri::command]
pub async fn check_app_update(app: AppHandle) -> Result<Option<AppUpdateInfo>, String> {
    run_check(&app).await
}

#[tauri::command]
pub fn get_app_last_check(app: AppHandle) -> Option<i64> {
    app.state::<AppState>().storage.get_pref(LAST_CHECK_KEY).ok().flatten()?.parse().ok()
}

#[tauri::command]
pub async fn download_app_update(app: AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    if st.app_update_busy.swap(true, AtomicOrdering::SeqCst) {
        return Err(BUSY.to_string());
    }
    let _busy = FlagGuard(&st.app_update_busy);
    if st.pending_update.lock().unwrap().is_some() {
        return Ok(());
    }
    let cached = st.checked_update.lock().unwrap().clone();
    let update = match cached {
        Some(u) => u,
        None => {
            run_check(&app).await?;
            let found = st.checked_update.lock().unwrap().clone();
            found.ok_or_else(|| "Aucune mise à jour disponible".to_string())?
        }
    };
    let mut downloaded: u64 = 0;
    let mut last_emit = Instant::now().checked_sub(Duration::from_secs(1)).unwrap_or_else(Instant::now);
    let app2 = app.clone();
    let bytes = update
        .download(
            |chunk, total| {
                downloaded += chunk as u64;
                if last_emit.elapsed() >= Duration::from_millis(100) {
                    last_emit = Instant::now();
                    let _ = app2.emit("app-update-progress", serde_json::json!({ "downloaded": downloaded, "total": total }));
                }
            },
            || {},
        )
        .await
        .map_err(|e| {
            eprintln!("mise à jour de FlashGet : téléchargement : {e}");
            DOWNLOAD_ERR.to_string()
        })?;
    let _ = app.emit("app-update-progress", serde_json::json!({ "downloaded": bytes.len(), "total": bytes.len() }));
    *st.pending_update.lock().unwrap() = Some((update, bytes));
    Ok(())
}

/// `true` tant que l'installation est en cours : aucun nouveau lancement n'est permis.
pub(crate) fn launches_blocked(st: &AppState) -> bool {
    st.app_installing.load(AtomicOrdering::SeqCst)
}

/// Remet un drapeau à faux quelle que soit la sortie.
struct FlagGuard<'a>(&'a AtomicBool);
impl Drop for FlagGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, AtomicOrdering::SeqCst);
    }
}

/// Sous un seul verrou `handles` : met en pause les téléchargements actifs (leurs ids sont à
/// reprendre) et arrête les enregistrements en direct (finalisés, jamais repris).
/// Renvoie `(à_reprendre, enregistrements_arrêtés)`.
pub(crate) fn capture_active(st: &AppState) -> (Vec<String>, Vec<String>) {
    let handles = st.handles.lock().unwrap();
    let mut resume = Vec::new();
    let mut stopped = Vec::new();
    for d in st.storage.list().unwrap_or_default() {
        let Some(h) = handles.get(&d.id) else { continue };
        if h.is_paused() || h.is_cancelled() {
            continue;
        }
        match d.status {
            DownloadStatus::Downloading => {
                h.pause();
                resume.push(d.id);
            }
            DownloadStatus::Recording => {
                h.stop();
                stopped.push(d.id);
            }
            _ => {}
        }
    }
    (resume, stopped)
}

#[tauri::command]
pub async fn install_app_update(app: AppHandle) -> Result<(), String> {
    let st = app.state::<AppState>();
    if st.app_update_busy.swap(true, AtomicOrdering::SeqCst) {
        return Err(BUSY.to_string());
    }
    let _busy = FlagGuard(&st.app_update_busy);
    let taken = st.pending_update.lock().unwrap().take();
    let Some((update, bytes)) = taken else {
        return Err(NONE_DOWNLOADED.to_string());
    };
    // Gèle la file : plus aucun lancement, y compris ceux du planificateur.
    st.app_installing.store(true, AtomicOrdering::SeqCst);
    let (ids, stopped) = capture_active(&st);
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        let gone = {
            let handles = st.handles.lock().unwrap();
            ids.iter().chain(stopped.iter()).all(|id| !handles.contains_key(id))
        };
        if gone {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    remember_resume_ids(&st.storage, &ids);
    // Sous Windows, l'installeur NSIS se lance en mode passif et le processus se termine.
    if let Err(e) = update.install(&bytes) {
        eprintln!("mise à jour de FlashGet : installation : {e}");
        *st.pending_update.lock().unwrap() = Some((update, bytes));
        remember_resume_ids(&st.storage, &[]);
        requeue_ids(&st.storage, &ids);
        st.app_installing.store(false, AtomicOrdering::SeqCst);
        crate::commands::reevaluate(&app);
        return Err(INSTALL_ERR.to_string());
    }
    Ok(())
}

/// Vérification 10 s après l'ouverture ; n'ouvre jamais de message en cas d'échec.
pub fn start_app_update_check(app: AppHandle) {
    let enabled = app.state::<AppState>().storage.get_settings().map(|s| s.app_update_check).unwrap_or(true);
    if !enabled {
        return;
    }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(Duration::from_secs(10)).await;
        match run_check(&app).await {
            Ok(Some(info)) => {
                let _ = app.emit("app-update-available", &info);
            }
            Ok(None) => {}
            Err(e) => eprintln!("vérification de FlashGet : {e}"),
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_core::model::Download;

    fn dl(id: &str, status: DownloadStatus) -> Download {
        serde_json::from_value(serde_json::json!({
            "id": id, "url": "https://x/a", "file_name": "a", "dest_path": "a",
            "total_bytes": null, "downloaded_bytes": 0, "status": status,
            "supports_ranges": false, "error": null,
        }))
        .unwrap()
    }

    fn status_of(s: &Storage, id: &str) -> DownloadStatus {
        s.get_by_id(id).unwrap().unwrap().status
    }

    #[test]
    fn resume_ids_requeued_once() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&dl("a", DownloadStatus::Paused)).unwrap();
        s.upsert(&dl("b", DownloadStatus::Completed)).unwrap();
        s.upsert(&dl("c", DownloadStatus::Paused)).unwrap();
        remember_resume_ids(&s, &["a".to_string(), "b".to_string()]);
        let ids = take_resume_ids(&s);
        assert_eq!(requeue_ids(&s, &ids), 1);
        assert_eq!(status_of(&s, "a"), DownloadStatus::Queued);
        assert_eq!(status_of(&s, "b"), DownloadStatus::Completed);
        assert_eq!(status_of(&s, "c"), DownloadStatus::Paused);
        assert_eq!(s.get_pref(RESUME_IDS_KEY).unwrap().as_deref().map(|v| v == "[]" || v.is_empty()), Some(true));
        // Second passage : plus rien à reprendre.
        assert!(take_resume_ids(&s).is_empty());
        assert_eq!(requeue_ids(&s, &take_resume_ids(&s)), 0);
    }

    #[test]
    fn take_resume_ids_tolerates_absent_or_invalid() {
        let s = Storage::open_in_memory().unwrap();
        assert!(take_resume_ids(&s).is_empty());
        s.set_pref(RESUME_IDS_KEY, "pas du json").unwrap();
        assert!(take_resume_ids(&s).is_empty());
        assert!(take_resume_ids(&s).is_empty());
    }

    fn state_with(rows: &[(&str, DownloadStatus)]) -> AppState {
        let storage = Storage::open_in_memory().unwrap();
        for (id, st) in rows {
            storage.upsert(&dl(id, *st)).unwrap();
        }
        AppState::new(storage, 0)
    }

    #[test]
    fn capture_only_live_unpaused_handles() {
        use download_core::engine::DownloadHandle;
        let st = state_with(&[
            ("run", DownloadStatus::Downloading),
            ("nohandle", DownloadStatus::Downloading),
            ("paused_row", DownloadStatus::Paused),
            ("already", DownloadStatus::Downloading),
            ("cancelled", DownloadStatus::Downloading),
            ("rec", DownloadStatus::Recording),
        ]);
        let (run, already, cancelled, rec, prow) =
            (DownloadHandle::new(), DownloadHandle::new(), DownloadHandle::new(), DownloadHandle::new(), DownloadHandle::new());
        already.pause();
        cancelled.cancel();
        {
            let mut h = st.handles.lock().unwrap();
            h.insert("run".into(), run.clone());
            h.insert("already".into(), already);
            h.insert("cancelled".into(), cancelled);
            h.insert("rec".into(), rec.clone());
            h.insert("paused_row".into(), prow);
        }
        let (resume, stopped) = capture_active(&st);
        assert_eq!(resume, vec!["run".to_string()]);
        assert_eq!(stopped, vec!["rec".to_string()]);
        assert!(run.is_paused());
        assert!(rec.is_stopped() && !rec.is_paused());
    }

    #[test]
    fn launches_blocked_follows_flag() {
        let st = state_with(&[]);
        assert!(!launches_blocked(&st));
        st.app_installing.store(true, AtomicOrdering::SeqCst);
        assert!(launches_blocked(&st));
    }

    #[test]
    fn interrupted_row_is_requeued_after_update_even_without_auto_resume() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert(&dl("a", DownloadStatus::Downloading)).unwrap();
        remember_resume_ids(&s, &["a".to_string()]);
        // Récupération au démarrage avec reprise automatique désactivée : `downloading` -> `paused`.
        s.update_state("a", 0, DownloadStatus::Paused).unwrap();
        let ids = take_resume_ids(&s);
        assert_eq!(requeue_ids(&s, &ids), 1);
        assert_eq!(status_of(&s, "a"), DownloadStatus::Queued);
    }
}
