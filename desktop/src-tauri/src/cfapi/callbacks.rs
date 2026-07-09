use crate::api::ApiClient;
use crate::cfapi::placeholders::{
    create_named_folder_placeholder, create_placeholders, is_duplicate_placeholder_error,
    MY_DRIVE_FOLDER_NAME,
};
use crate::cfapi::util::parse_file_identity;
use crate::db::DbHandle;
use crate::error::AppResult;
use crate::cfapi::util::callback_full_path;
use crate::cfapi::util::notify_directory_updated;
use crate::my_drive::{
    fetch_folder_contents, hydrate_file, is_under_my_drive, relative_path_from_sync_root,
    resolve_folder_id, resolve_my_drive_root_id,
};
use crate::sync::log::sync_log;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use windows::Win32::Foundation::{NTSTATUS, STATUS_SUCCESS};
use windows::Win32::Storage::CloudFilters::{
    CfExecute, CF_CALLBACK_INFO, CF_CALLBACK_PARAMETERS, CF_OPERATION_ACK_DATA_FLAGS,
    CF_OPERATION_INFO, CF_OPERATION_PARAMETERS, CF_OPERATION_PARAMETERS_0,
    CF_OPERATION_PARAMETERS_0_0, CF_OPERATION_PARAMETERS_0_6, CF_OPERATION_PARAMETERS_0_7,
    CF_OPERATION_TRANSFER_DATA_FLAG_NONE, CF_OPERATION_TRANSFER_PLACEHOLDERS_FLAG_NONE,
    CF_OPERATION_TYPE_ACK_DATA, CF_OPERATION_TYPE_TRANSFER_DATA,
    CF_OPERATION_TYPE_TRANSFER_PLACEHOLDERS,
};

const CALLBACK_TIMEOUT: Duration = Duration::from_secs(30);

struct CallbackContext {
    sync_root: PathBuf,
    db: DbHandle,
    api: ApiClient,
}

static CONTEXT: OnceLock<Mutex<Option<CallbackContext>>> = OnceLock::new();

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

pub unsafe extern "system" fn fetch_placeholders(
    info: *const CF_CALLBACK_INFO,
    params: *const CF_CALLBACK_PARAMETERS,
) {
    let _ = params;
    if info.is_null() {
        return;
    }
    let info = &*info;

    let result = std::panic::catch_unwind(|| handle_fetch_placeholders(info));
    match &result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log_callback_error("FETCH_PLACEHOLDERS", e),
        Err(_) => log_callback_error("FETCH_PLACEHOLDERS", "callback panicked"),
    }

    // Never fail the provider — Explorer treats NTSTATUS failure as "provider exited".
    let _ = ack_placeholders(info, STATUS_SUCCESS);
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
    let fetch = unsafe { params.Anonymous.FetchData };
    let offset = fetch.RequiredFileOffset;
    let length = fetch.RequiredLength;

    let result = std::panic::catch_unwind(|| handle_fetch_data(info, params));
    match &result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => log_callback_error("FETCH_DATA", e),
        Err(_) => log_callback_error("FETCH_DATA", "callback panicked"),
    }

    let _ = ack_data(info, STATUS_SUCCESS, offset, length);
}

fn handle_fetch_placeholders(info: &CF_CALLBACK_INFO) -> Result<(), String> {
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
        cfapi_callback_log("FETCH_PLACEHOLDERS sync root -> My Drive placeholder");
        match create_named_folder_placeholder(&ctx.0, MY_DRIVE_FOLDER_NAME, &folder_id) {
            Ok(()) => {
                notify_directory_updated(&ctx.0);
                return Ok(());
            }
            Err(e) if is_duplicate_placeholder_error(&e) => {
                notify_directory_updated(&ctx.0);
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        }
    }

    if !is_under_my_drive(&relative) {
        return Ok(());
    }

    let folder_id = resolve_folder_id(&ctx.1, &relative).map_err(|e| e.to_string())?;

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

    let stats = create_placeholders(&parent_dir, &contents.folders, &contents.files)
        .map_err(|e| e.to_string())?;
    cfapi_callback_log(&format!(
        "FETCH_PLACEHOLDERS created {} skipped {} under {:?}",
        stats.created,
        stats.skipped_duplicates,
        relative
    ));
    notify_directory_updated(&parent_dir);

    Ok(())
}

fn handle_fetch_data(
    info: &CF_CALLBACK_INFO,
    params: &CF_CALLBACK_PARAMETERS,
) -> Result<(), String> {
    let _full_path = callback_full_path(info)?;
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

    let ctx = with_context(|c| (c.db.clone(), c.api.clone()))
        .ok_or_else(|| "CfAPI context not initialized".to_string())?;

    let bytes = crate::blocking::run_async_future_with_timeout(
        CALLBACK_TIMEOUT,
        async move {
            hydrate_file(&ctx.1, &ctx.0, &remote_id)
                .await
                .map_err(|e| e.to_string())
        },
    )?;

    let fetch = unsafe { params.Anonymous.FetchData };
    let offset = fetch.RequiredFileOffset;
    let length = fetch.RequiredLength as usize;
    let offset_usize = offset as usize;
    if offset_usize > bytes.len() {
        return Err("fetch offset beyond file".into());
    }
    let end = (offset_usize + length).min(bytes.len());
    let chunk = &bytes[offset_usize..end];

    unsafe { transfer_data(info, offset, chunk.len() as i64, chunk, STATUS_SUCCESS)? };
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
        ParamSize: std::mem::size_of::<CF_OPERATION_PARAMETERS_0_6>() as u32,
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

unsafe fn ack_placeholders(info: &CF_CALLBACK_INFO, status: NTSTATUS) -> AppResult<()> {
    let op_info = CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: CF_OPERATION_TYPE_TRANSFER_PLACEHOLDERS,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        RequestKey: info.RequestKey,
        SyncStatus: std::ptr::null(),
    };
    let mut op_params = CF_OPERATION_PARAMETERS {
        ParamSize: std::mem::size_of::<CF_OPERATION_PARAMETERS_0_7>() as u32,
        Anonymous: CF_OPERATION_PARAMETERS_0 {
            TransferPlaceholders: CF_OPERATION_PARAMETERS_0_7 {
                Flags: CF_OPERATION_TRANSFER_PLACEHOLDERS_FLAG_NONE,
                CompletionStatus: status,
                PlaceholderTotalCount: 0,
                PlaceholderArray: std::ptr::null_mut(),
                PlaceholderCount: 0,
                EntriesProcessed: 0,
            },
        },
    };
    CfExecute(&op_info, &mut op_params)
        .map_err(|e| crate::error::AppError::msg(format!("CfExecute TRANSFER_PLACEHOLDERS: {}", e)))
}

unsafe fn ack_data(
    info: &CF_CALLBACK_INFO,
    status: NTSTATUS,
    offset: i64,
    length: i64,
) -> AppResult<()> {
    let op_info = CF_OPERATION_INFO {
        StructSize: std::mem::size_of::<CF_OPERATION_INFO>() as u32,
        Type: CF_OPERATION_TYPE_ACK_DATA,
        ConnectionKey: info.ConnectionKey,
        TransferKey: info.TransferKey,
        CorrelationVector: info.CorrelationVector,
        RequestKey: info.RequestKey,
        SyncStatus: std::ptr::null(),
    };
    let mut op_params = CF_OPERATION_PARAMETERS {
        ParamSize: std::mem::size_of::<CF_OPERATION_PARAMETERS_0_0>() as u32,
        Anonymous: CF_OPERATION_PARAMETERS_0 {
            AckData: CF_OPERATION_PARAMETERS_0_0 {
                Flags: CF_OPERATION_ACK_DATA_FLAGS(0),
                CompletionStatus: status,
                Offset: offset,
                Length: length,
            },
        },
    };
    CfExecute(&op_info, &mut op_params)
        .map_err(|e| crate::error::AppError::msg(format!("CfExecute ACK_DATA: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_root_requires_my_drive_placeholder() {
        assert!(sync_root_fetch_requires_my_drive_placeholder(""));
        assert!(!sync_root_fetch_requires_my_drive_placeholder("My Drive"));
    }
}
