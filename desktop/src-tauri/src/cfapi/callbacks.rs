use crate::api::ApiClient;
use crate::cfapi::placeholders::{
    build_placeholder_infos, complete_fetch_placeholders, count_existing_children,
    create_named_folder_placeholder, ensure_cloud_placeholder, finalize_hydrated_file,
    finalize_stream_placeholder, filter_new_entries, is_cloud_placeholder,
    is_dehydrated_placeholder, is_duplicate_placeholder_error, is_pinned,
    mark_directory_populated, mark_hydrated_available, transfer_or_complete_fetch,
    transfer_placeholders_via_callback, PlaceholderEntry, MY_DRIVE_FOLDER_NAME,
};
use crate::cfapi::util::parse_file_identity;
use crate::db::DbHandle;
use crate::error::AppResult;
use crate::cfapi::util::{callback_full_path, cf_operation_param_size, notify_directory_updated};
use crate::my_drive::{
    begin_fetch_data_inflight, clear_delete_in_flight, clear_hydrate_cache_for_file,
    end_fetch_data_inflight, ensure_hydrated_plaintext_with_progress, fetch_folder_contents,
    is_fetch_data_inflight, is_free_up_in_progress, is_path_under_active_delete,
    is_path_under_active_free_up, is_under_my_drive, mark_delete_in_flight, mark_recent_hydrate,
    pin_hydrated_cache_to_path, relative_path_from_sync_root, resolve_folder_id_for_fetch,
    resolve_my_drive_root_id, was_recently_dehydrated, FolderIdSource,
};
use crate::sync::log::sync_log;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use windows::Win32::Foundation::{
    NTSTATUS, STATUS_CLOUD_FILE_DEHYDRATION_DISALLOWED, STATUS_CLOUD_FILE_UNSUCCESSFUL,
    STATUS_SUCCESS,
};
use windows::Win32::Storage::CloudFilters::{
    CfExecute, CfReportProviderProgress, CF_CALLBACK_INFO, CF_CALLBACK_PARAMETERS,
    CF_OPERATION_ACK_DELETE_FLAG_NONE, CF_OPERATION_ACK_DEHYDRATE_FLAG_NONE, CF_OPERATION_INFO,
    CF_OPERATION_PARAMETERS, CF_OPERATION_PARAMETERS_0, CF_OPERATION_PARAMETERS_0_1,
    CF_OPERATION_PARAMETERS_0_2, CF_OPERATION_PARAMETERS_0_6, CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
    CF_OPERATION_TYPE_ACK_DELETE, CF_OPERATION_TYPE_ACK_DEHYDRATE, CF_OPERATION_TYPE_TRANSFER_DATA,
};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(30);
/// Large My Drive opens (download + decrypt) can take many minutes — do not use the
/// short placeholder timeout. Google Drive for desktop likewise waits for full hydrate.
const HYDRATE_TIMEOUT: Duration = Duration::from_secs(2 * 60 * 60);
/// Cap concurrent Explorer hydrates so a folder stampede cannot starve JSON/API under load.
const FETCH_DATA_HYDRATE_CONCURRENCY: usize = 4;
const CLOSE_DEBOUNCE: Duration = Duration::from_secs(3);
/// CFAPI TRANSFER_DATA chunks (offset/length must be 4KiB-aligned except at EOF).
const TRANSFER_CHUNK: usize = 1024 * 1024;
const TRANSFER_ALIGN: usize = 4096;

static CLOSE_DEBOUNCE_MAP: OnceLock<Mutex<HashMap<String, Instant>>> = OnceLock::new();
static FETCH_DATA_HYDRATE_SEM: OnceLock<tokio::sync::Semaphore> = OnceLock::new();

fn close_debounce_map() -> &'static Mutex<HashMap<String, Instant>> {
    CLOSE_DEBOUNCE_MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

fn fetch_data_hydrate_sem() -> &'static tokio::sync::Semaphore {
    FETCH_DATA_HYDRATE_SEM
        .get_or_init(|| tokio::sync::Semaphore::new(FETCH_DATA_HYDRATE_CONCURRENCY))
}

/// Returns true if this close should be handled; false if it is a duplicate within debounce window.
fn try_acquire_close_debounce(path: &Path) -> bool {
    let key = path.to_string_lossy().to_ascii_lowercase();
    let Ok(mut map) = close_debounce_map().lock() else {
        return true;
    };
    let now = Instant::now();
    if let Some(prev) = map.get(&key) {
        if now.duration_since(*prev) < CLOSE_DEBOUNCE {
            return false;
        }
    }
    map.insert(key, now);
    if map.len() > 2048 {
        map.retain(|_, t| now.duration_since(*t) < Duration::from_secs(60));
    }
    true
}

struct CallbackContext {
    sync_root: PathBuf,
    db: DbHandle,
    api: ApiClient,
}

static CONTEXT: OnceLock<Mutex<Option<CallbackContext>>> = OnceLock::new();
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();
static CANCELLED_PLACEHOLDER_REQUESTS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
static CANCELLED_FETCH_DATA_REQUESTS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();

#[derive(Clone, Serialize)]
struct HydrateFailedPayload {
    message: String,
    file_id: String,
}

pub fn init_app_handle(app: AppHandle) {
    let slot = APP_HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(app);
    }
}

pub fn clear_app_handle() {
    if let Some(slot) = APP_HANDLE.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = None;
        }
    }
}

fn sync_engine_from_app() -> Option<std::sync::Arc<crate::sync::engine::SyncEngine>> {
    use tauri::Manager;
    let slot = APP_HANDLE.get()?;
    let app = slot.lock().ok()?.clone()?;
    let state = app.try_state::<crate::state::AppState>()?;
    state.sync_engine().ok()
}

fn cancelled_requests() -> &'static Mutex<HashSet<i64>> {
    CANCELLED_PLACEHOLDER_REQUESTS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn cancelled_fetch_data_requests() -> &'static Mutex<HashSet<i64>> {
    CANCELLED_FETCH_DATA_REQUESTS.get_or_init(|| Mutex::new(HashSet::new()))
}

fn is_placeholder_request_cancelled(request_key: i64) -> bool {
    if request_key == 0 {
        return false;
    }
    cancelled_requests()
        .lock()
        .ok()
        .is_some_and(|set| set.contains(&request_key))
}

fn mark_placeholder_request_cancelled(request_key: i64) {
    if request_key == 0 {
        return;
    }
    if let Ok(mut set) = cancelled_requests().lock() {
        set.insert(request_key);
    }
}

fn clear_placeholder_request_cancelled(request_key: i64) {
    if request_key == 0 {
        return;
    }
    if let Ok(mut set) = cancelled_requests().lock() {
        set.remove(&request_key);
    }
}

fn is_fetch_data_request_cancelled(request_key: i64) -> bool {
    if request_key == 0 {
        return false;
    }
    cancelled_fetch_data_requests()
        .lock()
        .ok()
        .is_some_and(|set| set.contains(&request_key))
}

fn mark_fetch_data_request_cancelled(request_key: i64) {
    if request_key == 0 {
        return;
    }
    if let Ok(mut set) = cancelled_fetch_data_requests().lock() {
        set.insert(request_key);
        if set.len() > 4096 {
            set.clear();
        }
    }
}

fn clear_fetch_data_request_cancelled(request_key: i64) {
    if request_key == 0 {
        return;
    }
    if let Ok(mut set) = cancelled_fetch_data_requests().lock() {
        set.remove(&request_key);
    }
}

fn is_cloud_op_canceled(msg: &str) -> bool {
    let lower = msg.to_ascii_lowercase();
    lower.contains("0x8007018e")
        || lower.contains("canceled by user")
        || lower.contains("cancelled by user")
        || lower.contains("cloud operation was canceled")
        || lower.contains("cloud operation was cancelled")
}

pub fn init_context(db: DbHandle, sync_root: PathBuf, api: ApiClient) {
    let slot = CONTEXT.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        *guard = Some(CallbackContext {
            sync_root,
            db,
            api,
        });
    }
}

pub fn clear_context() {
    if let Some(slot) = CONTEXT.get() {
        if let Ok(mut guard) = slot.lock() {
            *guard = None;
        }
    }
    clear_app_handle();
    if let Ok(mut set) = cancelled_requests().lock() {
        set.clear();
    }
    if let Ok(mut set) = cancelled_fetch_data_requests().lock() {
        set.clear();
    }
}

fn with_context<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&CallbackContext) -> R,
{
    let slot = CONTEXT.get()?;
    let guard = slot.lock().ok()?;
    guard.as_ref().map(f)
}

fn cfapi_callback_log(message: impl AsRef<str>) {
    let line = format!("cfapi: {}", message.as_ref());
    eprintln!("{}", line);
    sync_log(line);
}

pub fn sync_root_fetch_requires_my_drive_placeholder(relative: &str) -> bool {
    relative.is_empty()
}

fn log_callback_error(kind: &str, error: &str) {
    cfapi_callback_log(&format!("{} error: {}", kind, error));
}

fn folder_id_source_label(source: FolderIdSource) -> &'static str {
    match source {
        FolderIdSource::Identity => "identity",
        FolderIdSource::Database => "database",
        FolderIdSource::RootConfig => "root_config",
    }
}

pub unsafe extern "system" fn fetch_placeholders(
    info: *const CF_CALLBACK_INFO,
    params: *const CF_CALLBACK_PARAMETERS,
) {
    let _ = params;
    if info.is_null() {
        return;
    }
    let info = &*info;

    match std::panic::catch_unwind(|| handle_fetch_placeholders(info)) {
        Ok(Ok(count)) => {
            cfapi_callback_log(&format!("TRANSFER_PLACEHOLDERS ok entries={}", count));
        }
        Ok(Err(e)) => {
            log_callback_error("FETCH_PLACEHOLDERS", &e);
            if let Err(ack_err) = complete_fetch_placeholders(info, 0, 0) {
                log_callback_error("TRANSFER_PLACEHOLDERS", &ack_err.to_string());
            }
        }
        Err(_) => {
            log_callback_error("FETCH_PLACEHOLDERS", "callback panicked");
            if let Err(ack_err) = complete_fetch_placeholders(info, 0, 0) {
                log_callback_error("TRANSFER_PLACEHOLDERS", &ack_err.to_string());
            }
        }
    }

    clear_placeholder_request_cancelled(info.RequestKey);
}

pub unsafe extern "system" fn cancel_fetch_placeholders(
    info: *const CF_CALLBACK_INFO,
    params: *const CF_CALLBACK_PARAMETERS,
) {
    let _ = params;
    if info.is_null() {
        return;
    }
    let info = &*info;
    if info.RequestKey == 0 {
        return;
    }
    mark_placeholder_request_cancelled(info.RequestKey);
    cfapi_callback_log(&format!(
        "CANCEL_FETCH_PLACEHOLDERS request_key={}",
        info.RequestKey
    ));
}

pub unsafe extern "system" fn fetch_data(
    info: *const CF_CALLBACK_INFO,
    params: *const CF_CALLBACK_PARAMETERS,
) {
    if info.is_null() || params.is_null() {
        return;
    }
    let info = &*info;
    let params = &*params;

    let file_id = unsafe {
        let identity = std::slice::from_raw_parts(
            info.FileIdentity as *const u8,
            info.FileIdentityLength as usize,
        );
        parse_file_identity(identity)
            .map(|(_, id)| id)
            .unwrap_or_default()
    };

    let result = std::panic::catch_unwind(|| handle_fetch_data(info, params));
    match result {
        Ok(Ok(())) => {}
        Ok(Err(e)) if is_cloud_op_canceled(&e) => {
            complete_provider_progress(info.ConnectionKey, info.TransferKey);
            cfapi_callback_log(&format!(
                "FETCH_DATA soft-cancel file={file_id}: {e}"
            ));
        }
        Ok(Err(e)) => {
            let mapped = map_fetch_data_error(&e);
            let detail = if file_id.is_empty() {
                mapped
            } else {
                format!("file={file_id}: {mapped}")
            };
            log_callback_error("FETCH_DATA", &detail);
            if !is_fetch_data_request_cancelled(info.RequestKey) {
                emit_hydrate_failed(&detail, &file_id);
                if let Err(te) = fail_fetch_data(info, params) {
                    if !is_cloud_op_canceled(&te.to_string()) {
                        log_callback_error("TRANSFER_DATA", &te);
                    }
                }
            }
        }
        Err(_) => {
            log_callback_error("FETCH_DATA", "callback panicked");
            if !is_fetch_data_request_cancelled(info.RequestKey) {
                if let Err(te) = fail_fetch_data(info, params) {
                    if !is_cloud_op_canceled(&te.to_string()) {
                        log_callback_error("TRANSFER_DATA", &te);
                    }
                }
            }
        }
    }

    clear_fetch_data_request_cancelled(info.RequestKey);
    if !file_id.is_empty() {
        end_fetch_data_inflight(&file_id);
    }
}

pub unsafe extern "system" fn cancel_fetch_data(
    info: *const CF_CALLBACK_INFO,
    _params: *const CF_CALLBACK_PARAMETERS,
) {
    if info.is_null() {
        return;
    }
    let info = &*info;
    if info.RequestKey == 0 {
        return;
    }
    mark_fetch_data_request_cancelled(info.RequestKey);
    cfapi_callback_log(&format!(
        "CANCEL_FETCH_DATA request_key={}",
        info.RequestKey
    ));
}

fn map_fetch_data_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if lower.contains("failed to read file") || lower.contains("blob missing") {
        "cloud file missing on server (blob unreadable)".to_string()
    } else {
        error.to_string()
    }
}

fn emit_hydrate_failed(message: &str, file_id: &str) {
    let Some(slot) = APP_HANDLE.get() else {
        return;
    };
    let Ok(guard) = slot.lock() else {
        return;
    };
    let Some(app) = guard.as_ref() else {
        return;
    };
    let _ = app.emit(
        "my-drive-hydrate-failed",
        HydrateFailedPayload {
            message: message.to_string(),
            file_id: file_id.to_string(),
        },
    );
}

fn fail_fetch_data(
    info: &CF_CALLBACK_INFO,
    params: &CF_CALLBACK_PARAMETERS,
) -> Result<(), String> {
    // Clear any Explorer Status progress bar left at 1..99 from a failed hydrate.
    complete_provider_progress(info.ConnectionKey, info.TransferKey);
    let fetch = unsafe { params.Anonymous.FetchData };
    let offset = fetch.RequiredFileOffset;
    // CfExecute requires 4KiB-aligned Length (except trailing EOF). Passing raw RequiredLength
    // for multi-GB Free-up skips → 0x80070057 and a false hydrate_failed toast.
    let length = fail_fetch_ack_length(fetch.RequiredLength);
    unsafe {
        transfer_data(info, offset, length, &[], STATUS_CLOUD_FILE_UNSUCCESSFUL)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Safe Length for a failed TRANSFER_DATA ACK (empty buffer).
fn fail_fetch_ack_length(required_length: i64) -> i64 {
    const MAX_FAIL_ACK: u64 = 1024 * 1024; // 1 MiB
    let align = TRANSFER_ALIGN as u64;
    let req = required_length.max(0) as u64;
    if req == 0 || req > MAX_FAIL_ACK {
        return align as i64;
    }
    let aligned = (req + align - 1) / align * align;
    aligned as i64
}

/// Refuse FETCH during Free up without surfacing hydrate_failed in the UI.
fn ack_fetch_data_skipped_for_free_up(
    info: &CF_CALLBACK_INFO,
    params: &CF_CALLBACK_PARAMETERS,
    reason: &str,
    remote_id: &str,
) {
    cfapi_callback_log(format!("FETCH_DATA skipped ({reason}) file={remote_id}"));
    if let Err(e) = fail_fetch_data(info, params) {
        if !is_cloud_op_canceled(&e) {
            cfapi_callback_log(format!(
                "FETCH_DATA skip ack failed (ignored) file={remote_id}: {e}"
            ));
        }
    }
}

/// Finish native CfAPI Status progress UI (white bar). Incomplete 1..99 ticks stick forever.
fn complete_provider_progress(
    connection_key: windows::Win32::Storage::CloudFilters::CF_CONNECTION_KEY,
    transfer_key: i64,
) {
    let _ = unsafe { CfReportProviderProgress(connection_key, transfer_key, 100, 100) };
}

fn handle_fetch_placeholders(info: &CF_CALLBACK_INFO) -> Result<u32, String> {
    if is_placeholder_request_cancelled(info.RequestKey) {
        return unsafe { complete_fetch_placeholders(info, 0, 0).map_err(|e| e.to_string()) };
    }

    let full_path = callback_full_path(info)?;
    let ctx = with_context(|c| (c.sync_root.clone(), c.db.clone(), c.api.clone()))
        .ok_or_else(|| "CfAPI context not initialized".to_string())?;

    let relative = relative_path_from_sync_root(&ctx.0, &full_path)
        .ok_or_else(|| format!("path outside sync root: {}", full_path.display()))?;

    cfapi_callback_log(&format!(
        "FETCH_PLACEHOLDERS relative={:?} path={}",
        relative,
        full_path.display()
    ));

    if sync_root_fetch_requires_my_drive_placeholder(&relative) {
        let folder_id = resolve_my_drive_root_id(&ctx.1).map_err(|e| e.to_string())?;
        let my_drive_path = ctx.0.join(MY_DRIVE_FOLDER_NAME);
        cfapi_callback_log("FETCH_PLACEHOLDERS sync root -> My Drive placeholder");
        if my_drive_path.exists() {
            ensure_cloud_placeholder(&my_drive_path, "folder", &folder_id)
                .map_err(|e| e.to_string())?;
        }
        match create_named_folder_placeholder(&ctx.0, MY_DRIVE_FOLDER_NAME, &folder_id) {
            Ok(()) => {}
            Err(e) if is_duplicate_placeholder_error(&e) => {}
            Err(e) => return Err(e.to_string()),
        }
        let entry = PlaceholderEntry::folder(MY_DRIVE_FOLDER_NAME, &folder_id);
        let count = unsafe {
            transfer_or_complete_fetch(info, std::slice::from_ref(&entry), 1)
                .map_err(|e| e.to_string())?
        };
        cfapi_callback_log(&format!(
            "sync root transfer My Drive entries=1 transferred={}",
            count
        ));
        notify_directory_updated(&ctx.0);
        return Ok(count);
    }

    if !is_under_my_drive(&relative) {
        return unsafe { complete_fetch_placeholders(info, 0, 0).map_err(|e| e.to_string()) };
    }

    let (folder_id, id_source) =
        resolve_folder_id_for_fetch(&ctx.1, info, &relative).map_err(|e| e.to_string())?;
    cfapi_callback_log(&format!(
        "FETCH_PLACEHOLDERS folder_id={:?} source={}",
        folder_id,
        folder_id_source_label(id_source)
    ));

    if is_placeholder_request_cancelled(info.RequestKey) {
        return unsafe { complete_fetch_placeholders(info, 0, 0).map_err(|e| e.to_string()) };
    }

    let sync_root = ctx.0.clone();
    let relative_owned = relative.clone();
    let parent_dir = sync_root.join(relative.replace('\\', std::path::MAIN_SEPARATOR_STR));
    let contents = crate::blocking::run_async_future_with_timeout(
        CALLBACK_TIMEOUT,
        async move {
            fetch_folder_contents(
                &ctx.2,
                &ctx.1,
                &sync_root,
                &relative_owned,
                folder_id.as_deref(),
            )
            .await
            .map_err(|e| e.to_string())
        },
    )?;

    if is_placeholder_request_cancelled(info.RequestKey) {
        return unsafe { complete_fetch_placeholders(info, 0, 0).map_err(|e| e.to_string()) };
    }

    let total = (contents.folders.len() + contents.files.len()) as u32;
    let existing = count_existing_children(&parent_dir, &contents.folders, &contents.files);

    let count = if total == 0 {
        mark_directory_populated(&parent_dir).map_err(|e| e.to_string())?;
        cfapi_callback_log(&format!(
            "mark_directory_populated empty path={}",
            parent_dir.display()
        ));
        unsafe { complete_fetch_placeholders(info, 0, 0).map_err(|e| e.to_string())? };
        0
    } else if existing >= total {
        let entries = build_placeholder_infos(&contents.folders, &contents.files);
        cfapi_callback_log(&format!(
            "subfolder transfer total={} existing={} path={}",
            total,
            existing,
            parent_dir.display()
        ));
        let count = unsafe {
            transfer_or_complete_fetch(info, &entries, total).map_err(|e| e.to_string())?
        };
        if count < total {
            cfapi_callback_log(&format!(
                "subfolder complete_fetch fallback total={} processed={}",
                total, count
            ));
        }
        if let Err(e) = mark_directory_populated(&parent_dir) {
            cfapi_callback_log(&format!(
                "mark_directory_populated warning path={}: {}",
                parent_dir.display(),
                e
            ));
        } else {
            cfapi_callback_log(&format!(
                "mark_directory_populated path={} existing={}",
                parent_dir.display(),
                existing
            ));
        }
        count
    } else {
        let new_entries =
            filter_new_entries(&parent_dir, &contents.folders, &contents.files);
        let stats = crate::cfapi::placeholders::create_placeholders(
            &parent_dir,
            &contents.folders,
            &contents.files,
        )
        .map_err(|e| e.to_string())?;
        cfapi_callback_log(&format!(
            "FETCH_PLACEHOLDERS created {} skipped {} (total {}) under {:?}",
            stats.created,
            stats.skipped_duplicates,
            total,
            relative
        ));
        let transferred = unsafe {
            transfer_placeholders_via_callback(info, &new_entries, total)
                .map_err(|e| e.to_string())?
        };
        cfapi_callback_log(&format!(
            "transfer_placeholders path={} transferred={} total={}",
            parent_dir.display(),
            transferred,
            total
        ));
        transferred
    };

    notify_directory_updated(&parent_dir);
    Ok(count)
}

fn handle_fetch_data(
    info: &CF_CALLBACK_INFO,
    params: &CF_CALLBACK_PARAMETERS,
) -> Result<(), String> {
    let identity = unsafe {
        std::slice::from_raw_parts(
            info.FileIdentity as *const u8,
            info.FileIdentityLength as usize,
        )
    };
    let (item_type, remote_id) = parse_file_identity(identity)
        .ok_or_else(|| "invalid file identity".to_string())?;
    if item_type != "file" {
        return Err("FETCH_DATA on non-file".into());
    }

    let placeholder_path = callback_full_path(info).ok();
    let fetch = unsafe { params.Anonymous.FetchData };
    let req_offset = fetch.RequiredFileOffset;
    let req_length = fetch.RequiredLength as u64;

    cfapi_callback_log(format!(
        "FETCH_DATA start file={remote_id} offset={req_offset} length={req_length} path={}",
        placeholder_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default()
    ));

    if is_fetch_data_request_cancelled(info.RequestKey) {
        cfapi_callback_log(format!("FETCH_DATA already cancelled file={remote_id}"));
        return Ok(());
    }

    // After Free up, Explorer thumbnails re-request content — refuse unless Always keep (PINNED).
    // Path may be empty on some FETCH_DATA callbacks — still block while any free-up is active.
    let pinned = placeholder_path.as_ref().is_some_and(|p| is_pinned(p));
    if !pinned {
        if is_free_up_in_progress()
            || placeholder_path
                .as_ref()
                .is_some_and(|p| is_path_under_active_free_up(p))
        {
            ack_fetch_data_skipped_for_free_up(info, params, "free-up in progress", &remote_id);
            return Ok(());
        }
        if was_recently_dehydrated(&remote_id) {
            ack_fetch_data_skipped_for_free_up(info, params, "recent free-up", &remote_id);
            return Ok(());
        }
    }

    begin_fetch_data_inflight(&remote_id);

    let ctx = with_context(|c| (c.db.clone(), c.api.clone()))
        .ok_or_else(|| "CfAPI context not initialized".to_string())?;

    let request_key = info.RequestKey;
    let connection_key = info.ConnectionKey;
    let transfer_key = info.TransferKey;

    let progress_completed = std::sync::Arc::new(AtomicU64::new(0));
    let progress_total = std::sync::Arc::new(AtomicU64::new(100));
    let stop_progress = std::sync::Arc::new(AtomicBool::new(false));
    let stop_flag = stop_progress.clone();
    let completed_flag = progress_completed.clone();
    let total_flag = progress_total.clone();
    let progress_thread = std::thread::spawn(move || {
        while !stop_flag.load(Ordering::Relaxed) {
            if is_fetch_data_request_cancelled(request_key) {
                break;
            }
            let total = total_flag.load(Ordering::Relaxed).max(1);
            let done = completed_flag.load(Ordering::Relaxed).min(total);
            // Keep Completed in 1..99 until hydrate finishes so Windows does not time out.
            let tick = ((done * 99) / total).clamp(1, 99);
            let _ = unsafe {
                CfReportProviderProgress(connection_key, transfer_key, 100, tick as i64)
            };
            std::thread::sleep(Duration::from_millis(250));
        }
    });

    let progress_cb: crate::api::UploadProgressCb = {
        let completed = progress_completed.clone();
        let total = progress_total.clone();
        std::sync::Arc::new(move |done: u64, tot: u64| {
            if tot > 0 {
                total.store(tot, Ordering::Relaxed);
            }
            completed.store(done, Ordering::Relaxed);
        })
    };

    let hydrate_started = Instant::now();
    let remote_id_for_hydrate = remote_id.clone();
    let hydrate_result = crate::blocking::run_async_future_with_timeout(
        HYDRATE_TIMEOUT,
        async move {
            let _permit = fetch_data_hydrate_sem()
                .acquire()
                .await
                .map_err(|_| "hydrate semaphore closed".to_string())?;
            ensure_hydrated_plaintext_with_progress(
                &ctx.1,
                &ctx.0,
                &remote_id_for_hydrate,
                Some(progress_cb),
            )
            .await
            .map_err(|e| e.to_string())
        },
    );

    stop_progress.store(true, Ordering::Relaxed);
    let _ = progress_thread.join();

    let cache_path = match hydrate_result {
        Ok(path) => path,
        Err(e) => {
            // Clear stuck Status progress on hydrate failure only — success continues to TRANSFER.
            complete_provider_progress(connection_key, transfer_key);
            cfapi_callback_log(format!(
                "FETCH_DATA hydrate failed file={remote_id} after {:?}: {e}",
                hydrate_started.elapsed()
            ));
            return Err(e);
        }
    };

    cfapi_callback_log(format!(
        "FETCH_DATA hydrate ok file={remote_id} in {:?} cancelled={}",
        hydrate_started.elapsed(),
        is_fetch_data_request_cancelled(request_key)
    ));
    mark_recent_hydrate(&remote_id);

    let pin_after_cancel = |reason: &str| {
        let Some(dest) = placeholder_path.as_ref() else {
            cfapi_callback_log(format!(
                "FETCH_DATA {reason}: no placeholder path file={remote_id}"
            ));
            return;
        };
        match pin_hydrated_cache_to_path(&cache_path, dest) {
            Ok(()) => {
                finalize_hydrated_file(dest, &remote_id);
                cfapi_callback_log(format!(
                    "FETCH_DATA pin-after-cancel ok file={remote_id} {}",
                    dest.display()
                ));
            }
            Err(e) => cfapi_callback_log(format!(
                "FETCH_DATA pin-after-cancel failed file={remote_id}: {e}"
            )),
        }
    };

    // Windows often cancels large-file FETCH before TRANSFER_DATA; cache is ready — pin to disk.
    if is_fetch_data_request_cancelled(request_key) {
        pin_after_cancel("cancelled after hydrate");
        complete_provider_progress(connection_key, transfer_key);
        return Ok(());
    }

    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(&cache_path).map_err(|e| e.to_string())?;
    let file_len = file.metadata().map_err(|e| e.to_string())?.len();
    let offset_u = req_offset as u64;
    if offset_u > file_len {
        complete_provider_progress(connection_key, transfer_key);
        return Err("fetch offset beyond file".into());
    }
    let end = (offset_u.saturating_add(req_length)).min(file_len);

    let transfer_started = Instant::now();
    let mut pos = offset_u;
    let mut transferred = 0u64;
    while pos < end {
        if is_fetch_data_request_cancelled(request_key) {
            pin_after_cancel("cancelled during transfer");
            complete_provider_progress(connection_key, transfer_key);
            return Ok(());
        }

        let mut chunk_end = (pos + TRANSFER_CHUNK as u64).min(end);
        // Align length to 4KiB unless this chunk reaches EOF.
        if chunk_end < file_len {
            let len = chunk_end - pos;
            let aligned = (len / TRANSFER_ALIGN as u64) * TRANSFER_ALIGN as u64;
            if aligned == 0 {
                chunk_end = (pos + TRANSFER_ALIGN as u64).min(file_len);
            } else {
                chunk_end = pos + aligned;
            }
            if chunk_end > end && end < file_len {
                // Stay within required range when not finishing the file: shrink to aligned end.
                let max_aligned = (end / TRANSFER_ALIGN as u64) * TRANSFER_ALIGN as u64;
                if max_aligned > pos {
                    chunk_end = max_aligned;
                } else {
                    // Required range ends mid-block and not at EOF — over-read to EOF or next align.
                    chunk_end = (pos + TRANSFER_ALIGN as u64).min(file_len);
                }
            }
        }

        let need = (chunk_end - pos) as usize;
        file.seek(SeekFrom::Start(pos))
            .map_err(|e| e.to_string())?;
        let mut chunk = vec![0u8; need];
        file.read_exact(&mut chunk).map_err(|e| e.to_string())?;

        if is_fetch_data_request_cancelled(request_key) {
            pin_after_cancel("cancelled before chunk transfer");
            complete_provider_progress(connection_key, transfer_key);
            return Ok(());
        }

        match unsafe {
            transfer_data(
                info,
                pos as i64,
                chunk.len() as i64,
                &chunk,
                STATUS_SUCCESS,
            )
        } {
            Ok(()) => {}
            Err(e) if is_cloud_op_canceled(&e.to_string()) => {
                pin_after_cancel("TRANSFER_DATA canceled");
                complete_provider_progress(connection_key, transfer_key);
                return Ok(());
            }
            Err(e) => {
                complete_provider_progress(connection_key, transfer_key);
                return Err(e.to_string());
            }
        }

        transferred += chunk.len() as u64;
        pos = chunk_end;
        let range_total = (end - offset_u).max(1);
        let tick = ((transferred * 99) / range_total).clamp(1, 99);
        let _ = unsafe {
            CfReportProviderProgress(connection_key, transfer_key, 100, tick as i64)
        };
    }

    complete_provider_progress(connection_key, transfer_key);
    cfapi_callback_log(format!(
        "FETCH_DATA transfer ok file={remote_id} bytes={transferred} in {:?}",
        transfer_started.elapsed()
    ));
    if let Some(dest) = placeholder_path.as_ref() {
        if is_cloud_placeholder(dest) {
            mark_hydrated_available(dest);
        } else {
            finalize_hydrated_file(dest, &remote_id);
        }
    }
    Ok(())
}

unsafe fn transfer_data(
    info: &CF_CALLBACK_INFO,
    offset: i64,
    length: i64,
    buffer: &[u8],
    status: NTSTATUS,
) -> AppResult<()> {
    let op_info = CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: CF_OPERATION_TYPE_TRANSFER_DATA,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        RequestKey: info.RequestKey,
        SyncStatus: std::ptr::null(),
    };
    let mut op_params = CF_OPERATION_PARAMETERS {
        ParamSize: cf_operation_param_size::<CF_OPERATION_PARAMETERS_0_6>(),
        Anonymous: CF_OPERATION_PARAMETERS_0 {
            TransferData: CF_OPERATION_PARAMETERS_0_6 {
                Flags: CF_OPERATION_TRANSFER_DATA_FLAG_NONE,
                CompletionStatus: status,
                Buffer: buffer.as_ptr() as *const _,
                Offset: offset,
                Length: length,
            },
        },
    };
    CfExecute(&op_info, &mut op_params)
        .map_err(|e| crate::error::AppError::msg(format!("CfExecute TRANSFER_DATA: {}", e)))
}

fn path_is_under_my_drive(sync_root: &std::path::Path, full: &std::path::Path) -> bool {
    relative_path_from_sync_root(sync_root, full)
        .as_deref()
        .is_some_and(is_under_my_drive)
}

pub unsafe extern "system" fn notify_file_close(
    info: *const CF_CALLBACK_INFO,
    _params: *const CF_CALLBACK_PARAMETERS,
) {
    if info.is_null() {
        return;
    }
    let info = &*info;
    let _ = std::panic::catch_unwind(|| handle_notify_file_close(info));
}

fn handle_notify_file_close(info: &CF_CALLBACK_INFO) -> Result<(), String> {
    let full = callback_full_path(info).map_err(|e| e.to_string())?;
    if !full.is_file() {
        return Ok(());
    }
    let file_name = full
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file");
    if crate::sync::should_skip_file(file_name) {
        return Ok(());
    }
    let (db, sync_root) = with_context(|ctx| (ctx.db.clone(), ctx.sync_root.clone()))
        .ok_or_else(|| "cfapi context missing".to_string())?;
    if !path_is_under_my_drive(&sync_root, &full) {
        return Ok(());
    }

    if is_path_under_active_free_up(&full) {
        cfapi_callback_log(&format!(
            "NOTIFY_FILE_CLOSE skipped (free-up) {}",
            full.display()
        ));
        return Ok(());
    }

    if is_path_under_active_delete(&full) {
        cfapi_callback_log(&format!(
            "NOTIFY_FILE_CLOSE skipped (delete) {}",
            full.display()
        ));
        return Ok(());
    }

    if is_dehydrated_placeholder(&full) {
        cfapi_callback_log(&format!(
            "NOTIFY_FILE_CLOSE skipped (dehydrated) {}",
            full.display()
        ));
        return Ok(());
    }

    if !try_acquire_close_debounce(&full) {
        return Ok(());
    }

    let remote_id = unsafe {
        let identity = std::slice::from_raw_parts(
            info.FileIdentity as *const u8,
            info.FileIdentityLength as usize,
        );
        parse_file_identity(identity)
            .filter(|(ty, _)| ty == "file")
            .map(|(_, id)| id)
    }
    .or_else(|| {
        let relative = relative_path_from_sync_root(&sync_root, &full)?;
        let conn = db.lock().ok()?;
        crate::db::my_drive_get_placeholder(&conn, &relative)
            .ok()
            .flatten()
            .filter(|(_, item_type, _)| item_type == "file")
            .map(|(id, _, _)| id)
    });

    let stream_mode = crate::sync::engine::sync_mode_is_stream(&db);
    cfapi_callback_log(&format!(
        "NOTIFY_FILE_CLOSE {} stream={} remote={:?}",
        full.display(),
        stream_mode,
        remote_id
    ));

    // Upload only through SyncEngine (shared semaphore + path dedupe). Never unbounded.
    tauri::async_runtime::spawn(async move {
        let Some(engine) = sync_engine_from_app() else {
            cfapi_callback_log(&format!(
                "NOTIFY_FILE_CLOSE upload skipped (no sync engine) {}",
                full.display()
            ));
            return;
        };
        let uploaded = match engine.upload_my_drive_path_gated(&full).await {
            Ok(Some(u)) => u,
            Ok(None) => false,
            Err(e) => {
                cfapi_callback_log(&format!("NOTIFY_FILE_CLOSE upload failed: {}", e));
                false
            }
        };
        if !stream_mode {
            return;
        }
        // Prefer DB id after upload (new copies often lack FileIdentity until convert).
        let id = remote_id.or_else(|| {
            let relative = relative_path_from_sync_root(&sync_root, &full)?;
            let conn = db.lock().ok()?;
            crate::db::my_drive_get_placeholder(&conn, &relative)
                .ok()
                .flatten()
                .filter(|(_, item_type, _)| item_type == "file")
                .map(|(id, _, _)| id)
        });
        let Some(id) = id else {
            return;
        };
        if is_path_under_active_free_up(&full) {
            return;
        }
        // Drive-like Stream: never dehydrate on close — Free up does that.
        if uploaded {
            clear_hydrate_cache_for_file(&id);
        }
        tokio::time::sleep(Duration::from_millis(400)).await;
        if is_fetch_data_inflight(&id)
            || is_path_under_active_free_up(&full)
            || is_path_under_active_delete(&full)
        {
            return;
        }
        match finalize_stream_placeholder(&full, &id) {
            Ok(()) => cfapi_callback_log(&format!(
                "NOTIFY_FILE_CLOSE marked In-Sync {}",
                full.display()
            )),
            Err(e) => cfapi_callback_log(&format!(
                "NOTIFY_FILE_CLOSE In-Sync skipped {}: {}",
                full.display(),
                e
            )),
        }
    });
    Ok(())
}

pub unsafe extern "system" fn notify_dehydrate(
    info: *const CF_CALLBACK_INFO,
    _params: *const CF_CALLBACK_PARAMETERS,
) {
    if info.is_null() {
        return;
    }
    let info = &*info;
    let _ = std::panic::catch_unwind(|| handle_notify_dehydrate(info));
}

/// Native Explorer "Free up space" — take over with upload-first free_up (0.1.52+).
/// `CfDehydratePlaceholder` from our own free-up does **not** fire this callback.
fn handle_notify_dehydrate(info: &CF_CALLBACK_INFO) -> Result<(), String> {
    let full = callback_full_path(info).map_err(|e| e.to_string())?;
    let db = with_context(|ctx| ctx.db.clone()).ok_or_else(|| "cfapi context missing".to_string())?;
    let sync_root = with_context(|ctx| ctx.sync_root.clone())
        .ok_or_else(|| "cfapi context missing".to_string())?;

    if !path_is_under_my_drive(&sync_root, &full) {
        ack_dehydrate(info, STATUS_SUCCESS);
        return Ok(());
    }

    // While our free-up walk runs, block concurrent OS dehydrates (upload-first race).
    if is_path_under_active_free_up(&full) {
        cfapi_callback_log(&format!(
            "NOTIFY_DEHYDRATE denied (active free-up) {}",
            full.display()
        ));
        ack_dehydrate(info, STATUS_CLOUD_FILE_DEHYDRATION_DISALLOWED);
        return Ok(());
    }

    if !crate::sync::engine::sync_mode_is_stream(&db) {
        cfapi_callback_log(&format!(
            "NOTIFY_DEHYDRATE denied (Mirror) {}",
            full.display()
        ));
        ack_dehydrate(info, STATUS_CLOUD_FILE_DEHYDRATION_DISALLOWED);
        return Ok(());
    }

    // Deny OS dehydrate; run safe free_up (upload + blob probe) asynchronously.
    ack_dehydrate(info, STATUS_CLOUD_FILE_DEHYDRATION_DISALLOWED);
    cfapi_callback_log(&format!(
        "NOTIFY_DEHYDRATE → safe free_up {}",
        full.display()
    ));
    tauri::async_runtime::spawn(async move {
        let Some(engine) = sync_engine_from_app() else {
            cfapi_callback_log(&format!(
                "NOTIFY_DEHYDRATE free_up skipped (no sync engine) {}",
                full.display()
            ));
            return;
        };
        match engine.free_up_my_drive_path(&full).await {
            Ok(_) => cfapi_callback_log(&format!(
                "NOTIFY_DEHYDRATE free_up done {}",
                full.display()
            )),
            Err(e) => cfapi_callback_log(&format!(
                "NOTIFY_DEHYDRATE free_up failed {}: {}",
                full.display(),
                e
            )),
        }
    });
    Ok(())
}

fn ack_dehydrate(info: &CF_CALLBACK_INFO, status: NTSTATUS) {
    let op_info = CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: CF_OPERATION_TYPE_ACK_DEHYDRATE,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        RequestKey: info.RequestKey,
        SyncStatus: std::ptr::null(),
    };
    let mut op_params = CF_OPERATION_PARAMETERS {
        ParamSize: cf_operation_param_size::<CF_OPERATION_PARAMETERS_0_1>(),
        Anonymous: CF_OPERATION_PARAMETERS_0 {
            AckDehydrate: CF_OPERATION_PARAMETERS_0_1 {
                Flags: CF_OPERATION_ACK_DEHYDRATE_FLAG_NONE,
                CompletionStatus: status,
                FileIdentity: std::ptr::null(),
                FileIdentityLength: 0,
            },
        },
    };
    if let Err(e) = unsafe { CfExecute(&op_info, &mut op_params) } {
        cfapi_callback_log(format!("CfExecute ACK_DEHYDRATE failed: {e}"));
    }
}

fn ack_delete(info: &CF_CALLBACK_INFO, status: NTSTATUS) {
    let op_info = CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: CF_OPERATION_TYPE_ACK_DELETE,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        RequestKey: info.RequestKey,
        SyncStatus: std::ptr::null(),
    };
    let mut op_params = CF_OPERATION_PARAMETERS {
        ParamSize: cf_operation_param_size::<CF_OPERATION_PARAMETERS_0_2>(),
        Anonymous: CF_OPERATION_PARAMETERS_0 {
            AckDelete: CF_OPERATION_PARAMETERS_0_2 {
                Flags: CF_OPERATION_ACK_DELETE_FLAG_NONE,
                CompletionStatus: status,
            },
        },
    };
    if let Err(e) = unsafe { CfExecute(&op_info, &mut op_params) } {
        cfapi_callback_log(format!("CfExecute ACK_DELETE failed: {e}"));
    }
}

pub unsafe extern "system" fn notify_delete(
    info: *const CF_CALLBACK_INFO,
    _params: *const CF_CALLBACK_PARAMETERS,
) {
    if info.is_null() {
        return;
    }
    let info = &*info;
    let _ = std::panic::catch_unwind(|| handle_notify_delete(info));
}

fn handle_notify_delete(info: &CF_CALLBACK_INFO) -> Result<(), String> {
    // Always ACK so Explorer can complete the local delete (Drive-like soft trash in background).
    let full = match callback_full_path(info) {
        Ok(p) => p,
        Err(e) => {
            cfapi_callback_log(format!("NOTIFY_DELETE path resolve failed: {e}"));
            ack_delete(info, STATUS_SUCCESS);
            return Ok(());
        }
    };
    let Some((api, db, sync_root)) = with_context(|ctx| {
        (ctx.api.clone(), ctx.db.clone(), ctx.sync_root.clone())
    }) else {
        cfapi_callback_log(&format!(
            "NOTIFY_DELETE ack (no context) {}",
            full.display()
        ));
        ack_delete(info, STATUS_SUCCESS);
        return Ok(());
    };
    if !path_is_under_my_drive(&sync_root, &full) {
        ack_delete(info, STATUS_SUCCESS);
        return Ok(());
    }
    // Reconcile clears DB before disk remove — ACK orphan deletes; no server call.
    let relative = match relative_path_from_sync_root(&sync_root, &full) {
        Some(r) => r,
        None => {
            ack_delete(info, STATUS_SUCCESS);
            return Ok(());
        }
    };
    let tracked = {
        let conn = db.lock().map_err(|e| e.to_string())?;
        crate::db::my_drive_get_placeholder(&conn, &relative)
            .map_err(|e| e.to_string())?
            .is_some()
    };
    if !tracked {
        cfapi_callback_log(&format!(
            "NOTIFY_DELETE ignored (no placeholder) {}",
            full.display()
        ));
        ack_delete(info, STATUS_SUCCESS);
        return Ok(());
    }
    cfapi_callback_log(&format!("NOTIFY_DELETE {}", full.display()));
    // ACK first so Explorer removes locally; soft-trash on server runs async.
    // Suppress CLOSE upload / watcher re-upload while soft-trash is in flight.
    mark_delete_in_flight(&full);
    ack_delete(info, STATUS_SUCCESS);
    tauri::async_runtime::spawn(async move {
        let result = crate::my_drive::delete_my_drive_path(&api, &db, &full).await;
        clear_delete_in_flight(&full);
        if let Err(e) = result {
            cfapi_callback_log(&format!("NOTIFY_DELETE failed: {}", e));
        }
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_root_requires_my_drive_placeholder() {
        assert!(sync_root_fetch_requires_my_drive_placeholder(""));
        assert!(!sync_root_fetch_requires_my_drive_placeholder("My Drive"));
    }

    #[test]
    fn cancel_ignored_for_request_key_zero() {
        assert!(!is_placeholder_request_cancelled(0));
        mark_placeholder_request_cancelled(0);
        assert!(!is_placeholder_request_cancelled(0));
    }
}
