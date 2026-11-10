//! Préférences d'interface persistées : liste blanche clé -> valeurs autorisées.
use crate::state::AppState;
use tauri::State;

const UI_PREFS: &[(&str, &[&str])] = &[("ui.theme", &["dark", "light", "system"])];

fn validate(key: &str, value: Option<&str>) -> Result<(), String> {
    let (_, allowed) = UI_PREFS
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| format!("clé de préférence inconnue : {key}"))?;
    if let Some(v) = value {
        if !allowed.contains(&v) {
            return Err(format!("valeur invalide pour {key} : {v}"));
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
}
