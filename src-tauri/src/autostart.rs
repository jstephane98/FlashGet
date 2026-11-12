//! Démarrage avec Windows.
//!
//! Le choix de l'utilisateur est gardé dans la base de FlashGet (préférence `launch_at_startup`),
//! et non plus seulement dans la clé `Run` du registre : le désinstalleur efface cette clé à chaque
//! désinstallation ou réinstallation manuelle, ce qui faisait perdre le réglage. À chaque lancement,
//! l'entrée du registre est remise en accord avec le choix enregistré.

use download_core::storage::Storage;
use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

pub const PREF_KEY: &str = "launch_at_startup";
/// Sans choix enregistré (nouvelle installation), FlashGet démarre avec Windows.
pub const DEFAULT_ENABLED: bool = true;

/// Ce qu'il faut faire de l'entrée du registre au lancement.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Écrire (ou réécrire) l'entrée : le chemin de l'exe a pu changer.
    Enable,
    /// Retirer l'entrée encore présente.
    Disable,
    Nothing,
}

pub fn parse_pref(v: Option<&str>) -> Option<bool> {
    match v {
        Some("1") => Some(true),
        Some("0") => Some(false),
        _ => None,
    }
}

/// Choix voulu et action à mener, d'après la préférence enregistrée et l'état du registre.
pub fn plan(pref: Option<bool>, registry_enabled: bool) -> (bool, Action) {
    let wanted = pref.unwrap_or(DEFAULT_ENABLED);
    let action = match (wanted, registry_enabled) {
        (true, _) => Action::Enable,
        (false, true) => Action::Disable,
        (false, false) => Action::Nothing,
    };
    (wanted, action)
}

/// Choix de l'utilisateur, tel qu'affiché dans les Paramètres.
pub fn wanted(storage: &Storage) -> bool {
    parse_pref(storage.get_pref(PREF_KEY).ok().flatten().as_deref()).unwrap_or(DEFAULT_ENABLED)
}

/// Enregistre le choix et met le registre à jour.
pub fn set(app: &AppHandle, storage: &Storage, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled { manager.enable() } else { manager.disable() }.map_err(|e| e.to_string())?;
    storage.set_pref(PREF_KEY, if enabled { "1" } else { "0" }).map_err(|e| e.to_string())
}

/// Au lancement : remet l'entrée du registre en accord avec le choix enregistré.
/// Jamais en développement, où l'entrée pointerait vers l'exe de débogage.
pub fn reconcile(app: &AppHandle, storage: &Storage) {
    if cfg!(debug_assertions) {
        return;
    }
    let manager = app.autolaunch();
    let registry_enabled = manager.is_enabled().unwrap_or(false);
    let pref = parse_pref(storage.get_pref(PREF_KEY).ok().flatten().as_deref());
    let (wanted, action) = plan(pref, registry_enabled);
    let result = match action {
        Action::Enable => manager.enable(),
        Action::Disable => manager.disable(),
        Action::Nothing => Ok(()),
    };
    if let Err(e) = result {
        eprintln!("démarrage avec Windows : {e}");
    }
    if pref.is_none() {
        let _ = storage.set_pref(PREF_KEY, if wanted { "1" } else { "0" });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sans_choix_enregistre_active_par_defaut() {
        assert_eq!(plan(None, false), (true, Action::Enable));
    }

    #[test]
    fn entree_effacee_par_le_desinstalleur_est_reecrite() {
        assert_eq!(plan(Some(true), false), (true, Action::Enable));
    }

    #[test]
    fn entree_presente_est_rafraichie() {
        assert_eq!(plan(Some(true), true), (true, Action::Enable));
    }

    #[test]
    fn choix_desactive_retire_une_entree_restante() {
        assert_eq!(plan(Some(false), true), (false, Action::Disable));
        assert_eq!(plan(Some(false), false), (false, Action::Nothing));
    }

    #[test]
    fn preference_lue() {
        assert_eq!(parse_pref(Some("1")), Some(true));
        assert_eq!(parse_pref(Some("0")), Some(false));
        assert_eq!(parse_pref(Some("oui")), None);
        assert_eq!(parse_pref(None), None);
    }

    #[test]
    fn choix_relu_depuis_la_base() {
        let s = Storage::open_in_memory().unwrap();
        assert!(wanted(&s));
        s.set_pref(PREF_KEY, "0").unwrap();
        assert!(!wanted(&s));
        s.set_pref(PREF_KEY, "1").unwrap();
        assert!(wanted(&s));
    }
}
