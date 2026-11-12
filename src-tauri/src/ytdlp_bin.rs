//! Choix du binaire yt-dlp à lancer : la copie du dossier de données (mise à jour) prime
//! sur le sidecar embarqué.

use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use tauri_plugin_shell::{process::Command, ShellExt};

use crate::platform::{YTDLP_FILE as CURRENT, YTDLP_NEXT as NEXT};

/// Dossier des binaires mis à jour : `{app_data}/bin`.
pub fn data_bin_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("bin"))
}

/// Copie de yt-dlp du dossier de données, si elle existe.
pub fn resolve_override(dir: &Path) -> Option<PathBuf> {
    let p = dir.join(CURRENT);
    p.is_file().then_some(p)
}

/// Installe une mise à jour déposée à côté (`yt-dlp.next.exe`) : elle remplace la copie
/// courante. Renvoie `true` si un remplacement a eu lieu.
pub fn apply_pending_swap(dir: &Path) -> std::io::Result<bool> {
    let next = dir.join(NEXT);
    if !next.is_file() {
        return Ok(false);
    }
    // Sous Windows, `rename` écrase le fichier cible tant qu'il n'est pas verrouillé.
    std::fs::rename(&next, dir.join(CURRENT))?;
    Ok(true)
}

/// Commande yt-dlp : la copie du dossier de données si présente, sinon le sidecar embarqué.
/// Appelée depuis Rust, donc hors du périmètre (scope) des permissions JS.
pub fn ytdlp_command(app: &AppHandle) -> Result<Command, String> {
    let cmd = match data_bin_dir(app).and_then(|d| resolve_override(&d)) {
        Some(path) => app.shell().command(path),
        None => app.shell().sidecar("yt-dlp").map_err(|e| e.to_string())?,
    };
    Ok(cmd.env("PYTHONUTF8", "1").env("PYTHONIOENCODING", "utf-8"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("flashget-ytdlp-bin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn resolve_override_none_when_empty() {
        let d = tmp("empty");
        assert_eq!(resolve_override(&d), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn resolve_override_some_when_present() {
        let d = tmp("present");
        std::fs::write(d.join(CURRENT), b"x").unwrap();
        assert_eq!(resolve_override(&d), Some(d.join(CURRENT)));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn swap_replaces_current_with_next() {
        let d = tmp("swap");
        std::fs::write(d.join(CURRENT), b"old").unwrap();
        std::fs::write(d.join(NEXT), b"new").unwrap();
        assert!(apply_pending_swap(&d).unwrap());
        assert_eq!(std::fs::read(d.join(CURRENT)).unwrap(), b"new");
        assert!(!d.join(NEXT).exists());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn swap_without_next_changes_nothing() {
        let d = tmp("noswap");
        std::fs::write(d.join(CURRENT), b"old").unwrap();
        assert!(!apply_pending_swap(&d).unwrap());
        assert_eq!(std::fs::read(d.join(CURRENT)).unwrap(), b"old");
        let _ = std::fs::remove_dir_all(&d);
    }
}
