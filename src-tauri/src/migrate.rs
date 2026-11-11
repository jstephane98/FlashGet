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

/// Fichier temporaire du snapshot : renommé en `downloads.sqlite` seulement une fois complet.
const TMP_DB_NAME: &str = "downloads.sqlite.migrating";

/// Fichiers annexes SQLite de la base : déjà pris en compte par le snapshot, jamais copiés.
fn is_db_side_file(name: &str) -> bool {
    name == TMP_DB_NAME
        || [".sqlite-wal", ".sqlite-shm", ".sqlite-journal"]
            .iter()
            .any(|suffix| name.strip_suffix(suffix) == Some("downloads"))
}

/// Copie récursive sans écraser, sans suivre les liens symboliques. Une erreur sur une entrée
/// n'interrompt pas la copie : elle est ajoutée à `errors`. Renvoie le nombre de fichiers copiés.
fn copy_tree(src: &Path, dst: &Path, top_level: bool, errors: &mut Vec<String>) -> usize {
    if let Err(e) = std::fs::create_dir_all(dst) {
        errors.push(format!("{}: {e}", dst.display()));
        return 0;
    }
    let entries = match std::fs::read_dir(src) {
        Ok(it) => it,
        Err(e) => {
            errors.push(format!("{}: {e}", src.display()));
            return 0;
        }
    };
    let mut count = 0;
    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                errors.push(e.to_string());
                continue;
            }
        };
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if top_level && (name_str == DB_NAME || is_db_side_file(&name_str)) {
            continue;
        }
        let from = entry.path();
        let to = dst.join(&name);
        let ty = match entry.file_type() {
            Ok(t) => t,
            Err(e) => {
                errors.push(format!("{}: {e}", from.display()));
                continue;
            }
        };
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            count += copy_tree(&from, &to, false, errors);
        } else if !to.exists() {
            match std::fs::copy(&from, &to) {
                Ok(_) => count += 1,
                Err(e) => errors.push(format!("{}: {e}", from.display())),
            }
        }
    }
    count
}

/// Reprend les données de `old_dir` dans `new_dir` au premier lancement. Ne fait rien si
/// `new_dir` contient déjà une base, ou si `old_dir` n'a pas de base. Les fichiers sont copiés
/// d'abord ; la base, copiée en dernier de façon atomique, sert de marqueur de fin : si elle
/// manque, le prochain lancement réessaie (sans rien écraser).
pub fn migrate_legacy_data(new_dir: &Path, old_dir: &Path) -> MigrationOutcome {
    let new_db = new_dir.join(DB_NAME);
    let old_db = old_dir.join(DB_NAME);
    if new_db.exists() || !old_db.is_file() {
        return MigrationOutcome::NotNeeded;
    }
    if let Err(e) = std::fs::create_dir_all(new_dir) {
        return MigrationOutcome::Failed(e.to_string());
    }
    let tmp = new_dir.join(TMP_DB_NAME);
    let _ = std::fs::remove_file(&tmp);

    let mut errors = Vec::new();
    let mut files = copy_tree(old_dir, new_dir, true, &mut errors);
    for e in &errors {
        eprintln!("migration des données : copie ignorée ({e})");
    }

    let result = Storage::snapshot_to(&old_db, &tmp)
        .map_err(|e| e.to_string())
        .and_then(|_| std::fs::rename(&tmp, &new_db).map_err(|e| e.to_string()));
    match result {
        Ok(()) => {
            files += 1;
            MigrationOutcome::Migrated { files }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            MigrationOutcome::Failed(e)
        }
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

    #[test]
    fn existing_files_are_not_overwritten() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        Storage::open(&old.join(DB_NAME)).unwrap();
        std::fs::write(old.join("youtube-cookies.txt"), "old").unwrap();
        std::fs::write(new.join("youtube-cookies.txt"), "new").unwrap();
        assert!(matches!(migrate_legacy_data(&new, &old), MigrationOutcome::Migrated { .. }));
        assert_eq!(std::fs::read_to_string(new.join("youtube-cookies.txt")).unwrap(), "new");
        assert!(new.join(DB_NAME).exists());
        assert!(!new.join(TMP_DB_NAME).exists());
    }

    #[test]
    fn old_dir_without_database_is_not_needed() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join("youtube-cookies.txt"), "c").unwrap();
        assert_eq!(migrate_legacy_data(&new, &old), MigrationOutcome::NotNeeded);
        assert!(!new.join("youtube-cookies.txt").exists());
    }

    #[test]
    fn failed_snapshot_leaves_no_database_and_retry_succeeds() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("new");
        std::fs::create_dir_all(&old).unwrap();
        std::fs::write(old.join(DB_NAME), "ceci n'est pas une base SQLite, loin de là").unwrap();
        std::fs::write(old.join("youtube-cookies.txt"), "c").unwrap();
        assert!(matches!(migrate_legacy_data(&new, &old), MigrationOutcome::Failed(_)));
        assert!(!new.join(DB_NAME).exists());
        assert!(!new.join(TMP_DB_NAME).exists());
        // Nouvelle tentative avec une base valide.
        std::fs::remove_file(old.join(DB_NAME)).unwrap();
        {
            let s = Storage::open(&old.join(DB_NAME)).unwrap();
            s.upsert(&sample("a")).unwrap();
        }
        assert!(matches!(migrate_legacy_data(&new, &old), MigrationOutcome::Migrated { .. }));
        assert_eq!(Storage::open(&new.join(DB_NAME)).unwrap().list().unwrap().len(), 1);
        assert!(new.join("youtube-cookies.txt").exists());
    }

    #[test]
    fn copy_errors_are_not_fatal() {
        let root = tempfile::tempdir().unwrap();
        let old = root.path().join(LEGACY_IDENTIFIER);
        let new = root.path().join("new");
        std::fs::create_dir_all(old.join("d")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("d").join("x.txt"), "x").unwrap();
        std::fs::write(old.join("ok.txt"), "ok").unwrap();
        Storage::open(&old.join(DB_NAME)).unwrap();
        // Un fichier occupe la place du sous-dossier `d` : sa copie échoue, le reste continue.
        std::fs::write(new.join("d"), "bloqueur").unwrap();
        assert!(matches!(migrate_legacy_data(&new, &old), MigrationOutcome::Migrated { .. }));
        assert!(new.join("ok.txt").exists());
        assert!(new.join(DB_NAME).exists());
    }
}
