mod account_crypto;
mod api;
mod auth_store;
mod blocking;
mod cfapi;
mod commands;
mod crypto;
mod db;
mod error;
mod my_drive;
mod state;
mod sync;

use state::AppState;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager, RunEvent, WindowEvent,
};

fn shutdown_cfapi() {
    #[cfg(windows)]
    crate::cfapi::stop();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db = db::open_db().expect("failed to open database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new(db))
        .invoke_handler(tauri::generate_handler![
            commands::get_auth_state,
            commands::login,
            commands::verify_2fa,
            commands::logout,
            commands::get_system_folders,
            commands::pick_folder,
            commands::save_sync_config,
            commands::complete_onboarding,
            commands::get_sync_status,
            commands::get_sync_activity,
            commands::get_sync_folders,
            commands::add_sync_folder,
            commands::pause_sync,
            commands::resume_sync,
            commands::open_drive_folder,
            commands::get_explorer_integration_status,
            commands::get_profile,
            commands::get_storage_info,
            commands::get_shared_with_me,
            commands::open_server_url,
            commands::open_path_in_explorer,
            commands::import_encryption_keys,
            commands::export_encryption_keys,
            commands::unregister_explorer_integration,
            commands::get_crypto_status,
            commands::unlock_crypto_recovery,
            commands::rotate_crypto_key,
        ])
        .setup(|app| {
            let state = app.state::<AppState>();
            let _ = commands::init_api_from_storage(&state);

            if auth_store::load_auth().ok().flatten().is_some() {
                if let Ok(client) = state.api() {
                    let engine = std::sync::Arc::new(sync::engine::SyncEngine::new(
                        client,
                        state.db.clone(),
                        app.handle().clone(),
                    ));
                    engine.load_computer_from_db();
                    state.set_sync_engine(engine);
                }

                #[cfg(windows)]
                commands::spawn_cfapi_integration(app.handle());

                let app_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    if let Some(state) = app_handle.try_state::<AppState>() {
                        let _ = commands::restore_sync_on_startup(&state, &app_handle).await;
                    }
                });
            }

            let open_i = MenuItem::with_id(app, "open", "Open FreeDrive", true, None::<&str>)?;
            let pause_i = MenuItem::with_id(app, "pause", "Pause sync", true, None::<&str>)?;
            let resume_i = MenuItem::with_id(app, "resume", "Resume sync", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open_i, &pause_i, &resume_i, &quit_i])?;

            let icon = app.default_window_icon().cloned().expect("tray icon");
            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .menu(&menu)
                .tooltip("FreeDrive")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => {
                        if let Some(w) = app.get_webview_window("main") {
                            let _ = w.show();
                            let _ = w.set_focus();
                        }
                    }
                    "pause" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(engine) = state.sync_engine() {
                                engine.set_paused(true);
                            }
                        }
                    }
                    "resume" => {
                        if let Some(state) = app.try_state::<AppState>() {
                            if let Ok(engine) = state.sync_engine() {
                                engine.set_paused(false);
                                let eng = engine.clone();
                                tauri::async_runtime::spawn(async move {
                                    let _ = eng.clone().run_initial_sync().await;
                                });
                            }
                        }
                    }
                    "quit" => {
                        shutdown_cfapi();
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
                            let _ = w.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|_app_handle, event| {
            if matches!(event, RunEvent::ExitRequested { .. } | RunEvent::Exit) {
                shutdown_cfapi();
            }
        });
}
