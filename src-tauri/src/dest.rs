//! Choix du dossier de destination et noms de fichiers uniques.

use crate::state::AppState;
use download_core::model::{Category, ConflictPolicy};
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

/// Clé de comparaison de chemins à la manière de Windows (application Windows) :
/// séparateurs unifiés, casse ignorée. « Clip.mp4 » et « clip.mp4 » désignent le même
/// fichier, comme pour `exists()` / `is_file()` sur le disque.
fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// `p` est déjà visé par un téléchargement enregistré (comparaison insensible à la casse).
fn is_planned(p: &Path, taken: &[String]) -> bool {
    let key = path_key(p);
    taken.iter().any(|t| path_key(Path::new(t)) == key)
}

/// `name` s'il est libre (ni sur disque ni dans `taken`), sinon `"{stem} ({n}){.ext}"`, n >= 2.
pub fn unique_file_name(dir: &Path, name: &str, taken: &[String]) -> String {
    let is_free = |n: &str| {
        let p = dir.join(n);
        !p.exists() && !is_planned(&p, taken)
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

/// Décision pour un nom déjà pris.
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictResolution {
    /// Le nom est libre.
    Use(String),
    /// Nom suivant libre (`"{stem} ({n}){.ext}"`).
    Rename(String),
    /// L'ancien fichier (ce chemin) est remplacé ; le nom ne change pas.
    ReplaceExisting(PathBuf),
}

/// Applique la politique de conflit. `exists_on_disk` : un fichier ordinaire porte déjà ce
/// nom. `ReplaceExisting` seulement en « remplacer », pour un fichier présent sur le disque
/// qu'aucun autre téléchargement ne vise (`taken`) ; dans tous les autres cas de conflit
/// (dossier du même nom compris), on renomme.
pub fn resolve_conflict(dir: &Path, name: &str, taken: &[String], policy: ConflictPolicy, exists_on_disk: bool) -> ConflictResolution {
    let unique = unique_file_name(dir, name, taken);
    if unique == name {
        return ConflictResolution::Use(unique);
    }
    let path = dir.join(name);
    let planned = is_planned(&path, taken);
    if policy == ConflictPolicy::Overwrite && exists_on_disk && !planned {
        ConflictResolution::ReplaceExisting(path)
    } else {
        ConflictResolution::Rename(unique)
    }
}

/// Nom retenu pour un nouveau téléchargement.
#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub file_name: String,
    /// « Remplacer » demandé mais la Corbeille a échoué : on a renommé.
    pub replace_failed: bool,
}

/// `resolve_conflict` puis exécution : `ReplaceExisting` envoie l'ancien fichier à la
/// Corbeille (`trash`) et supprime son fichier de reprise ; si la Corbeille échoue, on
/// renomme et on le signale.
pub fn settle_conflict(
    dir: &Path,
    name: &str,
    taken: &[String],
    policy: ConflictPolicy,
    trash: impl FnOnce(&Path) -> Result<(), String>,
) -> Placement {
    let exists = dir.join(name).is_file();
    match resolve_conflict(dir, name, taken, policy, exists) {
        ConflictResolution::Use(n) | ConflictResolution::Rename(n) => Placement { file_name: n, replace_failed: false },
        ConflictResolution::ReplaceExisting(path) => match trash(&path) {
            Ok(()) => {
                download_core::resume::clear(&path);
                Placement { file_name: name.to_string(), replace_failed: false }
            }
            Err(e) => {
                eprintln!("conflit: corbeille impossible pour {}: {e}", path.display());
                Placement { file_name: unique_file_name(dir, name, taken), replace_failed: true }
            }
        },
    }
}

/// `settle_conflict` avec la politique des réglages, les chemins déjà prévus et la vraie Corbeille.
pub fn place_file(app: &AppHandle, dir: &Path, name: &str) -> Placement {
    let policy = app.state::<AppState>().storage.get_settings().map(|s| s.conflict).unwrap_or_default();
    settle_conflict(dir, name, &taken_paths(app), policy, |p| trash::delete(p).map_err(|e| e.to_string()))
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
    fn conflict_policy() {
        use ConflictPolicy::{Overwrite, Rename};
        let d = tempfile::tempdir().unwrap();
        let dir = d.path();
        // Nom libre : utilisé tel quel, quelle que soit la politique.
        assert_eq!(resolve_conflict(dir, "a.mp4", &[], Overwrite, false), ConflictResolution::Use("a.mp4".into()));
        assert_eq!(resolve_conflict(dir, "a.mp4", &[], Rename, false), ConflictResolution::Use("a.mp4".into()));
        std::fs::write(dir.join("a.mp4"), b"x").unwrap();
        // Fichier présent, « renommer » : nom suivant.
        assert_eq!(resolve_conflict(dir, "a.mp4", &[], Rename, true), ConflictResolution::Rename("a (2).mp4".into()));
        // Fichier présent, « remplacer » : l'ancien sera remplacé.
        assert_eq!(resolve_conflict(dir, "a.mp4", &[], Overwrite, true), ConflictResolution::ReplaceExisting(dir.join("a.mp4")));
        // Chemin déjà prévu par un autre téléchargement : on renomme même en « remplacer ».
        let taken = vec![dir.join("a.mp4").to_string_lossy().to_string()];
        assert_eq!(resolve_conflict(dir, "a.mp4", &taken, Overwrite, true), ConflictResolution::Rename("a (2).mp4".into()));
        let taken = vec![dir.join("b.mp4").to_string_lossy().to_string()];
        assert_eq!(resolve_conflict(dir, "b.mp4", &taken, Overwrite, false), ConflictResolution::Rename("b (2).mp4".into()));
        // Même chemin à la casse près (Windows) : prévu par un autre téléchargement → renommer,
        // jamais remplacer (sinon son fichier partiel partirait à la Corbeille).
        std::fs::write(dir.join("Clip.mp4"), b"x").unwrap();
        let taken = vec![dir.join("Clip.mp4").to_string_lossy().to_string()];
        let r = resolve_conflict(dir, "clip.mp4", &taken, Overwrite, true);
        assert!(matches!(r, ConflictResolution::Rename(_)), "{r:?}");
        // Prévu mais pas encore sur le disque, casse et séparateurs différents : renommer aussi.
        let taken = vec![format!("{}/PART.MP4", dir.to_string_lossy().replace('\\', "/"))];
        assert_eq!(resolve_conflict(dir, "part.mp4", &taken, Overwrite, false), ConflictResolution::Rename("part (2).mp4".into()));
        // Un dossier du même nom n'est jamais remplacé.
        std::fs::create_dir(dir.join("c.mp4")).unwrap();
        assert_eq!(resolve_conflict(dir, "c.mp4", &[], Overwrite, false), ConflictResolution::Rename("c (2).mp4".into()));
    }

    #[test]
    fn settle_conflict_trashes_or_falls_back() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path();
        let old = dir.join("a.mp4");
        std::fs::write(&old, b"x").unwrap();
        let sidecar = download_core::resume::sidecar_path(&old);
        std::fs::write(&sidecar, b"{}").unwrap();
        // Corbeille en échec : on renomme et on le signale.
        let p = settle_conflict(dir, "a.mp4", &[], ConflictPolicy::Overwrite, |_| Err("non".into()));
        assert_eq!(p, Placement { file_name: "a (2).mp4".into(), replace_failed: true });
        assert!(old.exists() && sidecar.exists());
        // Corbeille OK : même nom, fichier de reprise supprimé.
        let mut trashed = Vec::new();
        let p = settle_conflict(dir, "a.mp4", &[], ConflictPolicy::Overwrite, |p| {
            trashed.push(p.to_path_buf());
            std::fs::remove_file(p).map_err(|e| e.to_string())
        });
        assert_eq!(p, Placement { file_name: "a.mp4".into(), replace_failed: false });
        assert_eq!(trashed, [old.clone()]);
        assert!(!old.exists() && !sidecar.exists());
        // « Renommer » : la Corbeille n'est jamais appelée.
        std::fs::write(&old, b"x").unwrap();
        let p = settle_conflict(dir, "a.mp4", &[], ConflictPolicy::Rename, |_| panic!("corbeille"));
        assert_eq!(p, Placement { file_name: "a (2).mp4".into(), replace_failed: false });
    }

    #[test]
    fn unique_file_name_ignores_case_of_planned_paths() {
        let d = tempfile::tempdir().unwrap();
        let taken = vec![d.path().join("Clip.MP4").to_string_lossy().to_string()];
        assert_eq!(unique_file_name(d.path(), "clip.mp4", &taken), "clip (2).mp4");
    }

    #[test]
    fn unique_file_name_without_extension() {
        let d = tempfile::tempdir().unwrap();
        std::fs::write(d.path().join("README"), b"x").unwrap();
        assert_eq!(unique_file_name(d.path(), "README", &[]), "README (2)");
    }
}
