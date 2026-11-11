//! Migration des données de l'ancien identifiant (`com.steph.tauri-app`) vers le nouveau
//! (`com.jstephane98.flashget`). L'ancien dossier n'est jamais modifié ni supprimé.

use download_core::storage::Storage;
use std::path::{Path, PathBuf};

const LEGACY_IDENTIFIER: &str = "com.steph.tauri-app";
const DB_NAME: &str = "downloads.sqlite";

#[derive(Debug, PartialEq, Eq)]
pub enum MigrationOutcome {
    /// Rien à faire : nouvelle base déjà présente, ou ancien dossier absent.
    NotNeeded,
    /// Migration effectuée ; nombre de fichiers copiés (base comprise).
    Migrated { files: usize },
    Failed(String),
}

/// Ancien dossier de données : frère du nouveau dans le dossier parent.
pub fn legacy_dir(new_dir: &Path) -> PathBuf {
    new_dir.parent().map(|p| p.join(LEGACY_IDENTIFIER)).unwrap_or_else(|| PathBuf::from(LEGACY_IDENTIFIER))
}

/// Fichiers annexes SQLite de la base : déjà pris en compte par le snapshot, jamais copiés.
fn is_db_side_file(name: &str) -> bool {
    [DB_NAME].iter().any(|db| {
        name.strip_prefix(db)
            .map(|suffix| matches!(suffix, "-wal" | "-shm" | "-journal"))
            .unwrap_or(false)
    })
}

/// Copie récursive sans écraser. Renvoie le nombre de fichiers copiés.
fn copy_tree(src: &Path, dst: &Path, top_level: bool) -> std::io::Result<usize> {
    std::fs::create_dir_all(dst)?;
    let mut count = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if top_level && (name_str == DB_NAME || is_db_side_file(&name_str)) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        if entry.file_type()?.is_dir() {
            count += copy_tree(&from, &to, false)?;
        } else if !to.exists() {
            std::fs::copy(&from, &to)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Reprend les données de `old_dir` dans `new_dir` au premier lancement. Ne fait rien si
/// `new_dir` contient déjà une base ou si `old_dir` n'existe pas.
pub fn migrate_legacy_data(new_dir: &Path, old_dir: &Path) -> MigrationOutcome {
    let new_db = new_dir.join(DB_NAME);
    if new_db.exists() || !old_dir.is_dir() {
        return MigrationOutcome::NotNeeded;
    }
    let run = || -> Result<usize, String> {
        std::fs::create_dir_all(new_dir).map_err(|e| e.to_string())?;
        let mut files = 0;
        let old_db = old_dir.join(DB_NAME);
        if old_db.is_file() {
            Storage::snapshot_to(&old_db, &new_db).map_err(|e| e.to_string())?;
            files += 1;
        }
        files += copy_tree(old_dir, new_dir, true).map_err(|e| e.to_string())?;
        Ok(files)
    };
    match run() {
        Ok(files) => MigrationOutcome::Migrated { files },
        Err(e) => MigrationOutcome::Failed(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use download_core::model::{Category, Download, DownloadKind, DownloadStatus, Priority};

    fn sample(id: &str) -> Download {
        Download {
            id: id.into(),
            url: "http://x/f".into(),
            file_name: "f.bin".into(),
            dest_path: "/tmp/f.bin".into(),
            total_bytes: Some(100),
            downloaded_bytes: 40,
            status: DownloadStatus::Paused,
            supports_ranges: true,
            error: None,
            priority: Priority::Normal,
            category: Category::Other,
            referrer: None,
            cookie: None,
            user_agent: None,
            kind: DownloadKind::Regular,
            audio_url: None,
            media_selection: None,
            thumbnail_url: None,
            duration_s: None,
            created_at: None,
            started_at: None,
            completed_at: None,
            queue_rank: None,
        }
    }

    #[test]
    fn migrates_database_and_files() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("com.jstephane98.flashget");
        std::fs::create_dir_all(old.join("sub")).unwrap();
        {
            let s = Storage::open(&old.join(DB_NAME)).unwrap();
            s.upsert(&sample("a")).unwrap();
            s.upsert(&sample("b")).unwrap();
        }
        std::fs::write(old.join("youtube-cookies.txt"), "cookies").unwrap();
        std::fs::write(old.join("sub").join("x.txt"), "x").unwrap();
        // Annexes SQLite factices : ne doivent pas être copiées.
        std::fs::write(old.join("downloads.sqlite-wal"), "w").unwrap();
        std::fs::write(old.join("downloads.sqlite-shm"), "s").unwrap();
        let before = std::fs::read(old.join(DB_NAME)).unwrap();

        let out = migrate_legacy_data(&new, &old);
        assert_eq!(out, MigrationOutcome::Migrated { files: 3 });
        assert_eq!(Storage::open(&new.join(DB_NAME)).unwrap().list().unwrap().len(), 2);
        assert_eq!(std::fs::read_to_string(new.join("youtube-cookies.txt")).unwrap(), "cookies");
        assert_eq!(std::fs::read_to_string(new.join("sub").join("x.txt")).unwrap(), "x");
        assert!(!new.join("downloads.sqlite-wal").exists());
        assert!(!new.join("downloads.sqlite-shm").exists());
        // Ancien dossier intact.
        assert_eq!(std::fs::read(old.join(DB_NAME)).unwrap(), before);
        assert!(old.join("youtube-cookies.txt").exists());
        assert!(old.join("downloads.sqlite-wal").exists());
    }

    #[test]
    fn migration_never_overwrites() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        {
            let s = Storage::open(&old.join(DB_NAME)).unwrap();
            s.upsert(&sample("a")).unwrap();
        }
        std::fs::write(new.join(DB_NAME), b"existing bytes").unwrap();
        assert_eq!(migrate_legacy_data(&new, &old), MigrationOutcome::NotNeeded);
        assert_eq!(std::fs::read(new.join(DB_NAME)).unwrap(), b"existing bytes");

        // Ancien dossier absent.
        let new2 = root.path().join("new2");
        assert_eq!(migrate_legacy_data(&new2, &root.path().join("absent")), MigrationOutcome::NotNeeded);
        assert!(!new2.exists());
    }

    #[test]
    fn legacy_dir_is_sibling() {
        assert_eq!(
            legacy_dir(Path::new("C:/x/Roaming/com.jstephane98.flashget")),
            Path::new("C:/x/Roaming").join("com.steph.tauri-app")
        );
    }
}
