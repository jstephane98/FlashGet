use download_core::model::DownloadStatus;
use serde::Serialize;
use std::path::Path;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Playable {
    Video,
    Audio,
}

/// Type de lecture possible d'après l'extension (insensible à la casse).
pub fn playable_kind(path: &Path) -> Option<Playable> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    match ext.as_str() {
        "mp4" | "webm" | "m4v" => Some(Playable::Video),
        "mp3" | "m4a" | "aac" | "ogg" | "opus" | "wav" | "flac" => Some(Playable::Audio),
        _ => None,
    }
}

#[derive(Serialize)]
pub struct FileInfo {
    pub exists: bool,
    pub size: Option<u64>,
    pub playable: Option<Playable>,
}

pub fn file_info(path: &Path) -> FileInfo {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => FileInfo { exists: true, size: Some(m.len()), playable: playable_kind(path) },
        _ => FileInfo { exists: false, size: None, playable: None },
    }
}

/// Envoie le fichier à la corbeille (si demandé et présent). Un fichier absent n'est pas une erreur.
pub fn delete_entry(
    path: &Path,
    delete_file: bool,
    trash: impl Fn(&Path) -> Result<(), String>,
) -> Result<(), String> {
    if delete_file && path.exists() {
        trash(path).map_err(|e| format!("Impossible d'envoyer le fichier à la corbeille : {e}"))?;
    }
    Ok(())
}

fn find(app: &AppHandle, id: &str) -> Result<download_core::model::Download, String> {
    app.state::<AppState>()
        .storage
        .get_by_id(id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("download not found: {id}"))
}

#[tauri::command(async)]
pub fn get_file_info(app: AppHandle, id: String) -> Result<FileInfo, String> {
    let d = find(&app, &id)?;
    Ok(file_info(Path::new(&d.dest_path)))
}

/// Autorise la lecture de CE fichier uniquement via le protocole d'actifs.
#[tauri::command]
pub fn allow_playback(app: AppHandle, id: String) -> Result<String, String> {
    let d = find(&app, &id)?;
    if d.status != DownloadStatus::Completed {
        return Err("le téléchargement n'est pas terminé".to_string());
    }
    let path = Path::new(&d.dest_path);
    if !path.is_file() {
        return Err("le fichier n'existe plus".to_string());
    }
    if playable_kind(path).is_none() {
        return Err("ce type de fichier ne peut pas être lu".to_string());
    }
    app.asset_protocol_scope().allow_file(path).map_err(|e| e.to_string())?;
    Ok(d.dest_path)
}

#[tauri::command]
pub fn delete_download(app: AppHandle, id: String, delete_file: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.handles.lock().unwrap().contains_key(&id) {
        return Err("arrêtez le téléchargement avant de le retirer".to_string());
    }
    let d = find(&app, &id)?;
    delete_entry(Path::new(&d.dest_path), delete_file, |p| trash::delete(p).map_err(|e| e.to_string()))?;
    state.storage.delete(&id).map_err(|e| e.to_string())?;
    crate::queue::reset_run_if_idle(&state);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test] fn playable_kinds() { assert_eq!(playable_kind(Path::new("a.MP4")), Some(Playable::Video)); assert_eq!(playable_kind(Path::new("a.opus")), Some(Playable::Audio)); assert_eq!(playable_kind(Path::new("a.mkv")), None); assert_eq!(playable_kind(Path::new("a")), None); }
    #[test] fn file_info_existing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.mp3");
        std::fs::write(&p, b"12345").unwrap();
        let i = file_info(&p);
        assert!(i.exists);
        assert_eq!(i.size, Some(5));
        assert_eq!(i.playable, Some(Playable::Audio));
    }
    #[test] fn file_info_missing() {
        let d = tempfile::tempdir().unwrap();
        let i = file_info(&d.path().join("nope.mp3"));
        assert!(!i.exists);
        assert_eq!(i.size, None);
        assert_eq!(i.playable, None);
    }
    #[test] fn delete_entry_list_only_never_trashes() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.bin");
        std::fs::write(&p, b"x").unwrap();
        let calls = Cell::new(0);
        assert!(delete_entry(&p, false, |_| { calls.set(calls.get() + 1); Ok(()) }).is_ok());
        assert_eq!(calls.get(), 0);
    }
    #[test] fn delete_entry_trashes_existing_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.bin");
        std::fs::write(&p, b"x").unwrap();
        let seen = std::cell::RefCell::new(Vec::new());
        assert!(delete_entry(&p, true, |q| { seen.borrow_mut().push(q.to_path_buf()); Ok(()) }).is_ok());
        assert_eq!(*seen.borrow(), vec![p]);
    }
    #[test] fn delete_entry_missing_file_is_ok() {
        let d = tempfile::tempdir().unwrap();
        let calls = Cell::new(0);
        assert!(delete_entry(&d.path().join("gone"), true, |_| { calls.set(calls.get() + 1); Ok(()) }).is_ok());
        assert_eq!(calls.get(), 0);
    }
    #[test] fn delete_keeps_row_when_trash_fails() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.bin");
        std::fs::write(&p, b"x").unwrap();
        let e = delete_entry(&p, true, |_| Err("verrouillé".to_string())).unwrap_err();
        assert!(e.starts_with("Impossible d'envoyer le fichier à la corbeille"));
        assert!(e.contains("verrouillé"));
    }
}
