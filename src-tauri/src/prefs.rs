//! Préférences d'interface persistées : liste blanche clé -> valeurs autorisées.
use crate::state::AppState;
use tauri::State;

const UI_PREFS: &[(&str, &[&str])] = &[
    ("ui.theme", &["dark", "light", "system"]),
    ("ui.completedView", &["list", "grid"]),
    ("ui.lang", &["auto", "fr", "en"]),
];

fn validate(key: &str, value: Option<&str>) -> Result<(), String> {
    let (_, allowed) = UI_PREFS
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| download_core::trf!("clé de préférence inconnue : {key}", "Unknown preference key: {key}"))?;
    if let Some(v) = value {
        if !allowed.contains(&v) {
            return Err(download_core::trf!("valeur invalide pour {key} : {v}", "Invalid value for {key}: {v}"));
        }
    }
    Ok(())
}

#[tauri::command]
pub fn get_ui_pref(state: State<'_, AppState>, key: String) -> Result<Option<String>, String> {
    validate(&key, None)?;
    state.storage.get_pref(&key).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_ui_pref(state: State<'_, AppState>, key: String, value: String) -> Result<(), String> {
    validate(&key, Some(&value))?;
    state.storage.set_pref(&key, &value).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test] fn accepts_known_key_and_value() { assert!(validate("ui.theme", Some("light")).is_ok()); assert!(validate("ui.theme", None).is_ok()); }
    #[test] fn rejects_unknown_key() { assert!(validate("bridge.token", None).is_err()); }
    #[test] fn rejects_unknown_value() { assert!(validate("ui.theme", Some("rose")).is_err()); }
    #[test] fn lang_pref() { assert!(validate("ui.lang", Some("auto")).is_ok()); assert!(validate("ui.lang", Some("en")).is_ok()); assert!(validate("ui.lang", Some("de")).is_err()); }
    #[test] fn completed_view_pref() { assert!(validate("ui.completedView", Some("grid")).is_ok()); assert!(validate("ui.completedView", Some("table")).is_err()); }
}
