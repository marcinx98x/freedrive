//! Soft session invalidation (Google Drive–style): clear tokens and return to sign-in
//! without wiping local My Drive / sync folder contents.

use crate::auth_store::{clear_auth, load_auth};
use crate::state::AppState;
use crate::sync::log::sync_log;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter, Manager};

static APP: OnceLock<AppHandle> = OnceLock::new();
static FIRED: AtomicBool = AtomicBool::new(false);

pub fn register_app(app: AppHandle) {
    let _ = APP.set(app);
}

/// Allow soft invalidate again after a successful login.
pub fn reset_fired() {
    FIRED.store(false, Ordering::SeqCst);
}

/// Clear auth and stop sync/CfAPI once when refresh is definitively rejected.
/// Does **not** wipe My Drive contents (unlike manual Sign out).
pub fn soft_invalidate_if_needed() {
    if FIRED.swap(true, Ordering::SeqCst) {
        return;
    }

    let Some(app) = APP.get().cloned() else {
        FIRED.store(false, Ordering::SeqCst);
        return;
    };

    sync_log("session expired — soft invalidate (tokens cleared, local files kept)");

    let user_id = load_auth()
        .ok()
        .flatten()
        .and_then(|a| serde_json::from_str::<serde_json::Value>(&a.user_json).ok())
        .and_then(|v| v.get("id").and_then(|x| x.as_str()).map(|s| s.to_string()));

    if let Some(uid) = user_id.as_deref() {
        crate::account_crypto::clear_uek(uid);
    }

    if let Some(state) = app.try_state::<AppState>() {
        if let Ok(engine) = state.sync_engine() {
            engine.shutdown();
        }
        *state.sync_engine.lock() = None;
        *state.watcher.lock() = None;
        state.sync_background.reset();
        *state.api.lock() = None;
    }

    #[cfg(windows)]
    crate::cfapi::stop();

    let _ = clear_auth();
    let _ = app.emit("session-expired", ());
}
