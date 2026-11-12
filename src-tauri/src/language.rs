//! Langue du moteur : reçue de l'interface, gardée dans la base pour le prochain démarrage
//! (le menu de la zone de notification est construit avant que l'interface soit chargée).

use crate::state::AppState;
use download_core::i18n::{set_english, tr};
use download_core::storage::Storage;
use tauri::menu::MenuItem;
use tauri::{AppHandle, Manager, State, Wry};

pub const PREF_KEY: &str = "app.lang";

/// Éléments du menu de la zone de notification, renommés quand la langue change.
pub struct TrayItems {
    pub open: MenuItem<Wry>,
    pub quit: MenuItem<Wry>,
}

pub fn open_label() -> &'static str {
    tr("Ouvrir FlashGet", "Open FlashGet")
}

pub fn quit_label() -> &'static str {
    tr("Quitter", "Quit")
}

/// « en » ou « fr » ; toute autre valeur est refusée.
pub fn parse(lang: &str) -> Option<bool> {
    match lang {
        "en" => Some(true),
        "fr" => Some(false),
        _ => None,
    }
}

/// Au démarrage : reprend la dernière langue transmise par l'interface (français par défaut).
pub fn load(storage: &Storage) {
    let english = storage.get_pref(PREF_KEY).ok().flatten().as_deref().and_then(parse).unwrap_or(false);
    set_english(english);
}

#[tauri::command]
pub fn set_app_language(app: AppHandle, state: State<'_, AppState>, lang: String) -> Result<(), String> {
    let english = parse(&lang).ok_or_else(|| format!("langue inconnue : {lang}"))?;
    set_english(english);
    state.storage.set_pref(PREF_KEY, &lang).map_err(|e| e.to_string())?;
    if let Some(items) = app.try_state::<TrayItems>() {
        let _ = items.open.set_text(open_label());
        let _ = items.quit.set_text(quit_label());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn langues_acceptees() {
        assert_eq!(parse("en"), Some(true));
        assert_eq!(parse("fr"), Some(false));
        assert_eq!(parse("de"), None);
        assert_eq!(parse(""), None);
    }
}
