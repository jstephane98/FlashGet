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
            let dir = app.path().app_data_dir().expect("app data dir");
            std::fs::create_dir_all(&dir).ok();
            let storage = Storage::open(&dir.join("downloads.sqlite")).expect("open db");
            app.manage(AppState::new(storage));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_download,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::cancel_download,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
