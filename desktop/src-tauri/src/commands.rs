use crate::api::types::{LoginSuccess, StorageInfo, User};
use crate::api::ApiClient;
use crate::auth_store::{clear_auth, load_auth, mirror_dir, save_auth, StoredAuth};
use crate::db::{
    config_get, config_set, list_activity, list_sync_folders, prepare_login_session,
    reset_session_on_logout, save_local_sync_folders,
};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use crate::sync::engine::SyncEngine;
use crate::sync::watcher::WatcherHandle;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

#[derive(Debug, Serialize)]
pub struct AuthState {
    pub logged_in: bool,
    pub server_url: Option<String>,
    pub user: Option<User>,
    pub onboarding_complete: bool,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub server_url: String,
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LoginResult {
    Success { user: User },
    TwoFactor { challenge_id: String, email_masked: String },
}

#[derive(Debug, Deserialize)]
pub struct TwoFactorRequest {
    pub server_url: String,
    pub challenge_id: String,
    pub code: String,
}

#[derive(Debug, Serialize)]
pub struct SystemFolder {
    pub label: String,
    pub path: String,
    pub suggested: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveSyncConfigRequest {
    pub folders: Vec<SelectedFolder>,
}

#[derive(Debug, Deserialize)]
pub struct SelectedFolder {
    pub path: String,
    pub label: String,
}

fn onboarding_complete(state: &AppState) -> bool {
    state.is_onboarding_complete()
}

fn restart_watcher(state: &AppState, engine: Arc<SyncEngine>) -> Result<(), String> {
    let folders = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        list_sync_folders(&conn).map_err(|e: crate::error::AppError| e.to_string())?
    };
    let paths: Vec<PathBuf> = folders
        .iter()
        .map(|f| PathBuf::from(&f.local_path))
        .collect();
    let watcher = WatcherHandle::start(paths, engine)
        .map_err(|e: crate::error::AppError| e.to_string())?;
    state.set_watcher(watcher);
    Ok(())
}

fn start_sync_services(
    state: &AppState,
    app: &AppHandle,
    engine: Arc<SyncEngine>,
    paths: Vec<PathBuf>,
    run_initial: bool,
    start_watcher: bool,
) -> Result<(), String> {
    let _paths = paths;
    if start_watcher {
        restart_watcher(state, engine.clone())?;
    }

    if run_initial {
        let eng = engine.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = eng.clone().run_initial_sync().await {
                eprintln!("initial sync failed: {}", e);
                eng.report_sync_error(e.to_string());
                let _ = app_handle.emit("sync-status-changed", eng.get_status());
            }
            if let Some(app_state) = app_handle.try_state::<AppState>() {
                if app_state.watcher.lock().is_none() {
                    let _ = restart_watcher(&app_state, eng);
                }
            }
        });
    }

    if state.sync_background.try_start_background() {
        let eng = engine.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if eng.is_shutdown() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                if eng.is_shutdown() {
                    break;
                }
                if eng.is_initial_sync_running() {
                    continue;
                }
                let _ = eng.poll_remote().await;
            }
        });

        let eng2 = engine.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                if eng2.is_shutdown() {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_secs(300)).await;
                if eng2.is_shutdown() {
                    break;
                }
                let _ = eng2.heartbeat_loop().await;
            }
        });
    }

    let _ = app.emit("sync-status-changed", engine.get_status());
    Ok(())
}

pub async fn restore_sync_on_startup(state: &AppState, app: &AppHandle) -> AppResult<()> {
    let complete = {
        let conn = state.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        config_get(&conn, "onboarding_complete")?
            .map(|v| v == "true")
            .unwrap_or(false)
    };
    if !complete {
        return Ok(());
    }

    let engine = state.sync_engine().map_err(|e| AppError::msg(e))?;
    if let Err(e) = engine.setup_computer().await {
        eprintln!("setup_computer on startup failed: {}", e);
    }

    let folders = {
        let conn = state.db.lock().map_err(|e| AppError::msg(e.to_string()))?;
        list_sync_folders(&conn)?
    };
    if folders.is_empty() {
        return Ok(());
    }

    let paths: Vec<PathBuf> = folders.iter().map(|f| PathBuf::from(&f.local_path)).collect();

    start_sync_services(state, app, engine.clone(), paths, false, true)
        .map_err(|e| AppError::msg(e))?;

    let eng = engine.clone();
    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = eng.clone().run_background_verify().await {
            eprintln!("background verify on startup failed: {}", e);
        }
        let _ = app_handle.emit("sync-status-changed", eng.get_status());
    });

    Ok(())
}

pub fn init_api_from_storage(state: &AppState) -> AppResult<()> {
    if let Some(auth) = load_auth()? {
        let client = ApiClient::from_auth(&auth);
        state.set_api(client);
    }
    Ok(())
}

#[tauri::command]
pub async fn get_auth_state(state: State<'_, AppState>) -> Result<AuthState, String> {
    let complete = onboarding_complete(&state);
    if let Some(auth) = load_auth().map_err(|e: crate::error::AppError| e.to_string())? {
        let user: Option<User> = serde_json::from_str(&auth.user_json).ok();
        let _ = init_api_from_storage(&state);
        return Ok(AuthState {
            logged_in: true,
            server_url: Some(auth.server_url),
            user,
            onboarding_complete: complete,
        });
    }
    Ok(AuthState {
        logged_in: false,
        server_url: None,
        user: None,
        onboarding_complete: complete,
    })
}

#[tauri::command]
pub async fn login(
    state: State<'_, AppState>,
    app: AppHandle,
    req: LoginRequest,
) -> Result<LoginResult, String> {
    let data = ApiClient::login(&req.server_url, &req.email, &req.password)
        .await
        .map_err(|e: crate::error::AppError| e.to_string())?;

    if let Some(requires) = data.get("requires_2fa").and_then(|v| v.as_bool()) {
        if requires {
            return Ok(LoginResult::TwoFactor {
                challenge_id: data["challenge_id"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                email_masked: data["email_masked"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
            });
        }
    }

    let success: LoginSuccess = serde_json::from_value(data).map_err(|e| e.to_string())?;
    finish_login(&state, &app, &req.server_url, &success).await?;
    Ok(LoginResult::Success {
        user: success.user,
    })
}

#[tauri::command]
pub async fn verify_2fa(
    state: State<'_, AppState>,
    app: AppHandle,
    req: TwoFactorRequest,
) -> Result<User, String> {
    let success = ApiClient::verify_2fa(&req.challenge_id, &req.code, &req.server_url)
        .await
        .map_err(|e: crate::error::AppError| e.to_string())?;
    finish_login(&state, &app, &req.server_url, &success).await?;
    Ok(success.user)
}

async fn finish_login(
    state: &AppState,
    app: &AppHandle,
    server_url: &str,
    success: &LoginSuccess,
) -> Result<(), String> {
    let normalized_url = server_url.trim_end_matches('/').to_string();

    let folders_to_remap = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        prepare_login_session(&conn, &success.user.id, &normalized_url)
            .map_err(|e: crate::error::AppError| e.to_string())?
            .folders_to_remap
    };
    state.refresh_onboarding_from_db();

    let auth = StoredAuth {
        server_url: normalized_url,
        access_token: success.tokens.access_token.clone(),
        refresh_token: success.tokens.refresh_token.clone(),
        user_json: serde_json::json!({
            "id": success.user.id,
            "email": success.user.email,
            "username": success.user.username,
            "role": success.user.role,
        })
        .to_string(),
    };
    save_auth(&auth).map_err(|e: crate::error::AppError| e.to_string())?;

    let client = ApiClient::from_auth(&auth);
    state.set_api(client.clone());

    let engine = Arc::new(SyncEngine::new(client, state.db.clone(), app.clone()));
    engine.load_computer_from_db();
    state.set_sync_engine(engine.clone());

    if let Err(e) = engine.setup_computer().await {
        eprintln!("setup_computer on login failed: {}", e);
    }

    if !folders_to_remap.is_empty() {
        let eng = engine.clone();
        let st = state.db.clone();
        let app_handle = app.clone();
        tauri::async_runtime::spawn(async move {
            if let Err(e) = eng.configure_folders(folders_to_remap.clone()).await {
                eprintln!("remap folders after server change failed: {}", e);
                return;
            }
            {
                let conn = match st.lock() {
                    Ok(c) => c,
                    Err(_) => return,
                };
                let _ = config_set(&conn, "initial_sync_complete", "false");
            }
            let paths: Vec<PathBuf> = folders_to_remap
                .into_iter()
                .map(|(p, _)| PathBuf::from(p))
                .collect();
            if let Some(app_state) = app_handle.try_state::<AppState>() {
                let _ = start_sync_services(&app_state, &app_handle, eng, paths, true, false);
            }
        });
    } else if onboarding_complete(state) {
        restore_sync_on_startup(state, app)
            .await
            .map_err(|e: crate::error::AppError| e.to_string())?;
    }

    Ok(())
}

#[tauri::command]
pub async fn logout(state: State<'_, AppState>) -> Result<(), String> {
    if let Ok(client) = state.api() {
        let _ = client.logout().await;
    }
    if let Ok(engine) = state.sync_engine() {
        engine.shutdown();
    }
    clear_auth().map_err(|e: crate::error::AppError| e.to_string())?;
    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        reset_session_on_logout(&conn)
            .map_err(|e: crate::error::AppError| e.to_string())?;
    }
    *state.api.lock() = None;
    *state.sync_engine.lock() = None;
    *state.watcher.lock() = None;
    state.sync_background.reset();
    Ok(())
}

#[tauri::command]
pub async fn get_system_folders() -> Result<Vec<SystemFolder>, String> {
    let mut folders = Vec::new();
    if let Some(p) = dirs::desktop_dir() {
        folders.push(SystemFolder {
            label: "Desktop".into(),
            path: p.to_string_lossy().into_owned(),
            suggested: true,
        });
    }
    if let Some(p) = dirs::document_dir() {
        folders.push(SystemFolder {
            label: "Documents".into(),
            path: p.to_string_lossy().into_owned(),
            suggested: true,
        });
    }
    if let Some(p) = dirs::download_dir() {
        folders.push(SystemFolder {
            label: "Downloads".into(),
            path: p.to_string_lossy().into_owned(),
            suggested: true,
        });
    }
    Ok(folders)
}

#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    let picked = tokio::task::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_folder()
            .map(|p| p.to_string())
    })
    .await
    .map_err(|e| e.to_string())?;
    Ok(picked)
}

#[tauri::command]
pub async fn save_sync_config(
    state: State<'_, AppState>,
    app: AppHandle,
    req: SaveSyncConfigRequest,
) -> Result<(), String> {
    let pairs: Vec<(String, String)> = req
        .folders
        .into_iter()
        .map(|f| (f.path, f.label))
        .collect();

    {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        save_local_sync_folders(&conn, &pairs)
            .map_err(|e: crate::error::AppError| e.to_string())?;
        crate::db::config_set(&conn, "initial_sync_complete", "false")
            .map_err(|e: crate::error::AppError| e.to_string())?;
    }

    let app_handle = app.clone();
    let pairs_bg = pairs.clone();
    tauri::async_runtime::spawn(async move {
        let Some(app_state) = app_handle.try_state::<AppState>() else {
            return;
        };
        let Ok(engine) = app_state.sync_engine() else {
            return;
        };
        if let Err(e) = engine.configure_folders(pairs_bg.clone()).await {
            eprintln!("configure_folders after save_sync_config failed: {}", e);
            return;
        }
        let paths: Vec<PathBuf> = pairs_bg
            .into_iter()
            .map(|(p, _)| PathBuf::from(p))
            .collect();
        if let Err(e) = start_sync_services(&app_state, &app_handle, engine, paths, true, false) {
            eprintln!("start_sync_services after save_sync_config failed: {}", e);
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn complete_onboarding(
    state: State<'_, AppState>,
    app: AppHandle,
) -> Result<(), String> {
    state.mark_onboarding_complete();

    let app_handle = app.clone();
    tauri::async_runtime::spawn(async move {
        let Some(app_state) = app_handle.try_state::<AppState>() else {
            return;
        };
        if app_state.watcher.lock().is_none() {
            if let Ok(engine) = app_state.sync_engine() {
                if let Err(e) = restart_watcher(&app_state, engine) {
                    eprintln!("restart_watcher after onboarding failed: {}", e);
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub async fn get_sync_status(state: State<'_, AppState>) -> Result<crate::sync::engine::SyncStatus, String> {
    let engine = state.sync_engine().map_err(|e| e.to_string())?;
    Ok(engine.get_status())
}

#[tauri::command]
pub async fn get_sync_activity(state: State<'_, AppState>) -> Result<Vec<crate::db::ActivityRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    list_activity(&conn, 50).map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn get_sync_folders(state: State<'_, AppState>) -> Result<Vec<crate::db::SyncFolderRow>, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    list_sync_folders(&conn).map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn add_sync_folder(
    state: State<'_, AppState>,
    path: String,
) -> Result<String, String> {
    let engine = state.sync_engine().map_err(|e| e.to_string())?;
    let label = Path::new(&path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("Folder")
        .to_string();

    engine
        .add_sync_folder(&path, &label)
        .await
        .map_err(|e: crate::error::AppError| e.to_string())?;

    let sync_folder_id = {
        let conn = state.db.lock().map_err(|e| e.to_string())?;
        crate::db::get_sync_folder_by_path(&conn, &path)
            .map_err(|e: crate::error::AppError| e.to_string())?
            .ok_or_else(|| "folder was not saved".to_string())?
            .id
    };

    restart_watcher(&state, engine.clone())?;

    let eng = engine.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = eng.sync_single_folder(sync_folder_id).await {
            eprintln!("sync after add folder failed: {}", e);
        }
    });

    Ok(format!("Added {} to sync", label))
}

#[tauri::command]
pub async fn pause_sync(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.sync_engine().map_err(|e| e.to_string())?;
    engine.set_paused(true);
    Ok(())
}

#[tauri::command]
pub async fn resume_sync(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.sync_engine().map_err(|e| e.to_string())?;
    engine.set_paused(false);
    let eng = engine.clone();
    tauri::async_runtime::spawn(async move {
        eng.drain_pending_paths().await;
    });
    Ok(())
}

#[tauri::command]
pub async fn open_drive_folder(app: AppHandle) -> Result<(), String> {
    let dir = mirror_dir().map_err(|e: crate::error::AppError| e.to_string())?;
    app.opener()
        .open_path(dir.to_string_lossy().into_owned(), None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_profile(state: State<'_, AppState>) -> Result<User, String> {
    state
        .api()
        .map_err(|e| e.to_string())?
        .get_me()
        .await
        .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn get_storage_info(state: State<'_, AppState>) -> Result<StorageInfo, String> {
    state
        .api()
        .map_err(|e| e.to_string())?
        .get_my_storage()
        .await
        .map_err(|e: crate::error::AppError| e.to_string())
}

#[tauri::command]
pub async fn open_server_url(
    state: State<'_, AppState>,
    app: AppHandle,
    path: Option<String>,
) -> Result<(), String> {
    let base = state.api().map_err(|e| e.to_string())?.server_url();
    let url = match path.filter(|p| !p.is_empty()) {
        Some(p) if p.starts_with('#') => format!("{}{}", base, p),
        Some(p) if p.starts_with('/') => format!("{}{}", base, p),
        Some(p) => format!("{}/{}", base.trim_end_matches('/'), p.trim_start_matches('/')),
        None => base,
    };
    app.opener()
        .open_url(&url, None::<&str>)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn open_path_in_explorer(app: AppHandle, path: String) -> Result<(), String> {
    app.opener()
        .open_path(&path, None::<&str>)
        .map_err(|e| e.to_string())
}
