//! Choix du dossier de destination et noms de fichiers uniques.

use crate::state::AppState;
use download_core::model::Category;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Dossier explicite non vide > dossier de la catégorie > dossier Téléchargements.
pub fn resolve_dest_dir(
    explicit: Option<&str>,
    category_dir: Option<String>,
    downloads_dir: Option<PathBuf>,
) -> Result<PathBuf, String> {
    if let Some(e) = explicit {
        if !e.trim().is_empty() {
            return Ok(PathBuf::from(e));
        }
    }
    if let Some(c) = category_dir {
        if !c.trim().is_empty() {
            return Ok(PathBuf::from(c));
        }
    }
    downloads_dir.ok_or_else(|| "aucun dossier de destination".to_string())
}

/// Résout puis crée le dossier de destination.
pub fn dest_dir_for(app: &AppHandle, explicit: Option<&str>, category: Category) -> Result<PathBuf, String> {
    let configured = app.state::<AppState>().storage.category_dir(category).map_err(|e| e.to_string())?;
    let dir = resolve_dest_dir(explicit, configured, app.path().download_dir().ok())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("dossier de destination inaccessible: {e}"))?;
    Ok(dir)
}

/// Chemins `dest_path` des téléchargements déjà enregistrés.
pub fn taken_paths(app: &AppHandle) -> Vec<String> {
    app.state::<AppState>()
        .storage
        .list()
        .map(|l| l.into_iter().map(|d| d.dest_path).collect())
        .unwrap_or_default()
}

/// `name` s'il est libre (ni sur disque ni dans `taken`), sinon `"{stem} ({n}){.ext}"`, n >= 2.
pub fn unique_file_name(dir: &Path, name: &str, taken: &[String]) -> String {
    let is_free = |n: &str| {
        let p = dir.join(n);
        !p.exists() && !taken.iter().any(|t| Path::new(t) == p.as_path())
    };
    if is_free(name) {
        return name.to_string();
    }
    let (stem, ext) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    };
    let mut n = 2u32;
    loop {
        let candidate = format!("{stem} ({n}){ext}");
        if is_free(&candidate) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_dir_wins() {
        assert_eq!(
            resolve_dest_dir(Some("D:/X"), Some("D:/Cat".into()), Some("D:/Dl".into())).unwrap(),
            PathBuf::from("D:/X")
        );
    }

    #[test]
    fn blank_explicit_falls_back_to_category_then_downloads() {
        assert_eq!(
            resolve_dest_dir(Some("  "), Some("D:/Cat".into()), Some("D:/Dl".into())).unwrap(),
            PathBuf::from("D:/Cat")
        );
        assert_eq!(resolve_dest_dir(None, None, Some("D:/Dl".into())).unwrap(), PathBuf::from("D:/Dl"));
        assert!(resolve_dest_dir(None, None, None).is_err());
    }

    #[test]
    fn unique_file_name_keeps_free_name() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(unique_file_name(d.path(), "a.mp4", &[]), "a.mp4");
    }

    #[test]
    fn unique_file_name_skips_existing_file() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("a.mp4"), b"x").unwrap();
        std::fs::write(d.path().join("a (2).mp4"), b"x").unwrap();
        assert_eq!(unique_file_name(d.path(), "a.mp4", &[]), "a (3).mp4");
    }

    #[test]
    fn unique_file_name_skips_planned_paths() {
        let d = tempfile::tempdir().unwrap();
        let taken = vec![d.path().join("a.mp4").to_string_lossy().to_string()];
        assert_eq!(unique_file_name(d.path(), "a.mp4", &taken), "a (2).mp4");
    }

    #[test]
    fn unique_file_name_without_extension() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("README"), b"x").unwrap();
        assert_eq!(unique_file_name(d.path(), "README", &[]), "README (2)");
    }
}
