mod bridge;
mod commands;
mod dest;
mod library;
mod link;
mod prefs;
mod queue;
mod remux;
mod state;
mod ytdlp;
mod youtube;

use download_core::storage::Storage;
use state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            commands::focus_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--hidden"]),
        ))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            std::fs::create_dir_all(&dir).ok();
            let storage = Storage::open(&dir.join("downloads.sqlite")).map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let settings = storage.get_settings().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            app.manage(AppState::new(storage, settings.max_speed_bps));
            bridge::start(app.handle().clone());
            commands::recover_interrupted_recordings(app.handle());
            queue::start_planner_loop(app.handle().clone());

            // --- System tray ---
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
            use tauri::menu::{Menu, MenuItem};

            let open_i = MenuItem::with_id(app, "open", "Ouvrir FlashGet", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quitter", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &quit_i])?;
            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.unminimize();
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            // --- Close-to-tray + affichage initial conditionnel ---
            if let Some(win) = app.get_webview_window("main") {
                let win_for_close = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = win_for_close.hide();
                    }
                });
                let hidden = std::env::args().any(|a| a == "--hidden");
                if !hidden {
                    let _ = win.show();
                    let _ = win.set_focus();
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::add_batch_downloads,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::stop_recording,
            commands::cancel_download,
            commands::set_priority,
            commands::get_settings,
            commands::update_settings,
            commands::pause_all,
            commands::resume_all,
            commands::get_category_dirs,
            commands::set_category_dir,
            commands::remove_from_list,
            commands::clear_completed,
            commands::open_file,
            commands::open_folder,
            library::get_file_info,
            library::allow_playback,
            library::delete_download,
            commands::get_bridge_info,
            commands::regenerate_bridge_token,
            commands::get_autostart,
            commands::set_autostart,
            link::probe_link,
            link::confirm_link,
            youtube::youtube_login,
            youtube::youtube_status,
            youtube::youtube_logout,
            youtube::get_cookies_from_browser,
            youtube::set_cookies_from_browser,
            youtube::get_notify_on_complete,
            youtube::set_notify_on_complete,
            prefs::get_ui_pref,
            prefs::set_ui_pref,
            queue::reorder_queue,
            queue::start_queue,
            queue::pause_queue,
            queue::get_queue_paused,
            queue::get_schedule,
            queue::set_schedule,
            queue::get_end_action,
            queue::set_end_action,
            queue::run_end_action,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
