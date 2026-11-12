mod app_update;
mod autostart;
mod bridge;
mod commands;
mod dest;
mod library;
mod link;
mod migrate;
mod prefs;
mod queue;
mod remux;
mod retry;
mod state;
mod ytdlp;
mod ytdlp_bin;
mod ytdlp_update;
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            let dir = app.path().app_data_dir().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            // Reprise des données de l'ancien identifiant avant toute ouverture de la base.
            match migrate::migrate_legacy_data(&dir, &migrate::legacy_dir(&dir)) {
                migrate::MigrationOutcome::Failed(e) => eprintln!("migration des données : {e}"),
                migrate::MigrationOutcome::Migrated { files } => eprintln!("migration des données : {files} fichier(s) repris"),
                migrate::MigrationOutcome::NotNeeded => {}
            }
            std::fs::create_dir_all(&dir).ok();
            // Installe un yt-dlp mis à jour en attente, avant toute utilisation de yt-dlp.
            if let Some(bin) = ytdlp_bin::data_bin_dir(app.handle()) {
                if let Err(e) = ytdlp_bin::apply_pending_swap(&bin) {
                    eprintln!("mise à jour de yt-dlp : {e}");
                }
            }
            let storage = Storage::open(&dir.join("downloads.sqlite")).map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            let settings = storage.get_settings().map_err(|e| Box::<dyn std::error::Error>::from(e.to_string()))?;
            app.manage(AppState::new(storage, settings.max_speed_bps));
            // Remet l'entrée de démarrage Windows en accord avec le choix enregistré
            // (le désinstalleur l'efface, et le chemin de l'exe peut avoir changé).
            autostart::reconcile(app.handle(), &app.state::<AppState>().storage);
            bridge::start(app.handle().clone());
            commands::recover_interrupted_recordings(app.handle());
            let requeued = commands::recover_interrupted_downloads(app.handle())
                + app_update::requeue_after_update(app.handle());
            // Charge les marques et fait un premier passage synchrone avant de rendre la main.
            queue::start_planner_loop(app.handle().clone());
            ytdlp_update::start_ytdlp_auto_check(app.handle().clone());
            app_update::start_app_update_check(app.handle().clone());
            // Reprise automatique : relancer la file après ce premier passage, pour que les
            // téléchargements récupérés ne démarrent pas hors de la plage horaire.
            if requeued > 0 {
                commands::reevaluate(app.handle());
            }

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

            // --- Fermeture (zone de notification ou sortie) + affichage initial conditionnel ---
            if let Some(win) = app.get_webview_window("main") {
                let win_for_close = win.clone();
                let app_for_close = app.handle().clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        // Réglage relu à chaque fermeture ; lecture en échec → garder dans la zone.
                        let to_tray = app_for_close
                            .state::<AppState>()
                            .storage
                            .get_settings()
                            .map(|s| s.close_to_tray)
                            .unwrap_or(true);
                        if to_tray {
                            api.prevent_close();
                            let _ = win_for_close.hide();
                        } else {
                            app_for_close.exit(0);
                        }
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
            commands::open_extension_folder,
            link::retry_capture,
            commands::list_downloads,
            commands::start_download,
            commands::pause_download,
            commands::stop_recording,
            commands::cancel_download,
            commands::set_priority,
            commands::get_settings,
            commands::save_settings,
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
            commands::get_bridge_status,
            commands::get_ignored_sites,
            commands::set_ignored_sites,
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
            ytdlp::get_ytdlp_version,
            ytdlp_update::check_ytdlp_update,
            ytdlp_update::update_ytdlp,
            ytdlp_update::get_ytdlp_last_check,
            app_update::check_app_update,
            app_update::download_app_update,
            app_update::install_app_update,
            app_update::get_app_last_check,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
