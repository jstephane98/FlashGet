mod commands;
mod state;

use download_core::storage::Storage;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            std::fs::create_dir_all(&dir).ok();
            let storage = Storage::open(&dir.join("downloads.sqlite")).map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let settings = storage.get_settings().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            app.manage(AppState::new(storage, settings.max_speed_bps));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_download,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::cancel_download,
            commands::set_priority,
            commands::get_settings,
            commands::update_settings,
            commands::pause_all,
            commands::resume_all,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
