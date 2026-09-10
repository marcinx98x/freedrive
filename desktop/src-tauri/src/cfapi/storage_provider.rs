//! WinRT Storage Provider registration for Explorer cloud Status.
//!
//! Status glyphs (cloud / check / sync) come from Windows CfAPI placeholder state.
//! We do **not** paint custom IconResource via SetAsync / CustomStateHandler — that
//! stacked a blank "paper" glyph next to the native Status icons.

use crate::cfapi::register::{
    mark_registered, sync_root_identity_bytes, unregister_sync_root,
};
use crate::cfapi::shell_register::{
    active_sync_root_has_custom_state, clear_custom_state_handler_value,
    has_stale_freedrive_sync_roots, icon_resource_path, list_freedrive_sync_root_ids,
    purge_stale_freedrive_sync_roots, sync_root_shell_id,
};
use crate::cfapi::util::{notify_directory_updated, notify_shell_updated, PROVIDER_ID};
use crate::db::{config_get, config_set, DbHandle};
use crate::error::{AppError, AppResult};
use crate::sync::log::sync_log;
use std::path::{Path, PathBuf};
use windows::core::HSTRING;
use windows::Foundation::Collections::IIterable;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Provider::{
    StorageProviderHardlinkPolicy, StorageProviderHydrationPolicy,
    StorageProviderHydrationPolicyModifier, StorageProviderInSyncPolicy,
    StorageProviderItemProperties, StorageProviderItemProperty,
    StorageProviderPopulationPolicy, StorageProviderSyncRootInfo,
    StorageProviderSyncRootManager,
};
use windows::Storage::{IStorageItem, StorageFile, StorageFolder};
use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_UPDATEITEM, SHCNF_PATHW};

pub const CF_STATUS_PROPS_KEY: &str = "cf_status_props_v1";
/// Set after WinRT re-Register without custom Status property definitions.
pub const CF_STATUS_NATIVE_ONLY_KEY: &str = "cf_status_native_only_v1";
/// Legacy 0.1.45 clear flag (superseded by v2).
pub const CF_STATUS_PROPS_CLEARED_KEY: &str = "cf_status_props_cleared_v1";
/// Set after 0.1.46+ SetAsync(empty) walk + shell ASSOCCHANGED.
pub const CF_STATUS_PROPS_CLEARED_V2_KEY: &str = "cf_status_props_cleared_v2";

fn sp_log(message: impl AsRef<str>) {
    let line = format!("cfapi: {}", message.as_ref());
    eprintln!("{}", line);
    sync_log(line);
}

pub fn has_status_props(db: &DbHandle) -> AppResult<bool> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    Ok(config_get(&conn, CF_STATUS_PROPS_KEY)?.as_deref() == Some("true"))
}

fn mark_status_props(db: &DbHandle) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    config_set(&conn, CF_STATUS_PROPS_KEY, "true")?;
    Ok(())
}

pub fn clear_status_props_state(db: &DbHandle) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    config_set(&conn, CF_STATUS_PROPS_KEY, "false")?;
    config_set(&conn, CF_STATUS_NATIVE_ONLY_KEY, "false")?;
    config_set(&conn, CF_STATUS_PROPS_CLEARED_KEY, "false")?;
    config_set(&conn, CF_STATUS_PROPS_CLEARED_V2_KEY, "false")?;
    Ok(())
}

fn has_native_only(db: &DbHandle) -> AppResult<bool> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    Ok(config_get(&conn, CF_STATUS_NATIVE_ONLY_KEY)?.as_deref() == Some("true"))
}

fn mark_native_only(db: &DbHandle) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    config_set(&conn, CF_STATUS_NATIVE_ONLY_KEY, "true")?;
    Ok(())
}

fn has_status_props_cleared_v2(db: &DbHandle) -> AppResult<bool> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    Ok(config_get(&conn, CF_STATUS_PROPS_CLEARED_V2_KEY)?.as_deref() == Some("true"))
}

fn mark_status_props_cleared_v2(db: &DbHandle) -> AppResult<()> {
    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;
    config_set(&conn, CF_STATUS_PROPS_CLEARED_V2_KEY, "true")?;
    Ok(())
}

fn track_all_insync_policy() -> StorageProviderInSyncPolicy {
    StorageProviderInSyncPolicy::FileCreationTime
        | StorageProviderInSyncPolicy::FileReadOnlyAttribute
        | StorageProviderInSyncPolicy::FileHiddenAttribute
        | StorageProviderInSyncPolicy::FileSystemAttribute
        | StorageProviderInSyncPolicy::DirectoryCreationTime
        | StorageProviderInSyncPolicy::DirectoryReadOnlyAttribute
        | StorageProviderInSyncPolicy::DirectoryHiddenAttribute
        | StorageProviderInSyncPolicy::DirectorySystemAttribute
        | StorageProviderInSyncPolicy::FileLastWriteTime
        | StorageProviderInSyncPolicy::DirectoryLastWriteTime
}

fn build_sync_root_info(db: &DbHandle, sync_root: &Path) -> AppResult<StorageProviderSyncRootInfo> {
    let id = sync_root_shell_id(db)?;
    let identity = sync_root_identity_bytes(db)?;
    let path_h = HSTRING::from(sync_root.to_string_lossy().as_ref());
    let folder = StorageFolder::GetFolderFromPathAsync(&path_h)
        .map_err(|e| AppError::msg(format!("GetFolderFromPathAsync: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("GetFolderFromPathAsync.get: {}", e)))?;

    let info = StorageProviderSyncRootInfo::new()
        .map_err(|e| AppError::msg(format!("StorageProviderSyncRootInfo::new: {}", e)))?;
    info.SetId(&HSTRING::from(id.as_str()))
        .map_err(|e| AppError::msg(format!("SetId: {}", e)))?;
    info.SetPath(&folder)
        .map_err(|e| AppError::msg(format!("SetPath: {}", e)))?;
    info.SetDisplayNameResource(&HSTRING::from("FreeDrive"))
        .map_err(|e| AppError::msg(format!("SetDisplayNameResource: {}", e)))?;
    info.SetIconResource(&HSTRING::from(icon_resource_path()))
        .map_err(|e| AppError::msg(format!("SetIconResource: {}", e)))?;
    info.SetVersion(&HSTRING::from(env!("CARGO_PKG_VERSION")))
        .map_err(|e| AppError::msg(format!("SetVersion: {}", e)))?;
    info.SetProviderId(PROVIDER_ID)
        .map_err(|e| AppError::msg(format!("SetProviderId: {}", e)))?;
    info.SetHydrationPolicy(StorageProviderHydrationPolicy::Partial)
        .map_err(|e| AppError::msg(format!("SetHydrationPolicy: {}", e)))?;
    info.SetHydrationPolicyModifier(StorageProviderHydrationPolicyModifier::None)
        .map_err(|e| AppError::msg(format!("SetHydrationPolicyModifier: {}", e)))?;
    // WinRT enum omits Partial (0); match CF_POPULATION_POLICY_PARTIAL.
    info.SetPopulationPolicy(StorageProviderPopulationPolicy(0))
        .map_err(|e| AppError::msg(format!("SetPopulationPolicy: {}", e)))?;
    info.SetInSyncPolicy(track_all_insync_policy())
        .map_err(|e| AppError::msg(format!("SetInSyncPolicy: {}", e)))?;
    info.SetHardlinkPolicy(StorageProviderHardlinkPolicy::None)
        .map_err(|e| AppError::msg(format!("SetHardlinkPolicy: {}", e)))?;
    info.SetAllowPinning(true)
        .map_err(|e| AppError::msg(format!("SetAllowPinning: {}", e)))?;
    info.SetShowSiblingsAsGroup(false)
        .map_err(|e| AppError::msg(format!("SetShowSiblingsAsGroup: {}", e)))?;

    let context = CryptographicBuffer::CreateFromByteArray(&identity)
        .map_err(|e| AppError::msg(format!("CreateFromByteArray: {}", e)))?;
    info.SetContext(&context)
        .map_err(|e| AppError::msg(format!("SetContext: {}", e)))?;

    // Do not append StorageProviderItemPropertyDefinition for Status — Explorer
    // then only shows native CfAPI glyphs (cloud / check / sync arrows).

    Ok(info)
}

fn unregister_winrt_id(id: &str) {
    match StorageProviderSyncRootManager::Unregister(&HSTRING::from(id)) {
        Ok(()) => sp_log(format!("StorageProviderSyncRootManager.Unregister id={}", id)),
        Err(e) => sp_log(format!(
            "StorageProviderSyncRootManager.Unregister id={} (ok if missing): {} (0x{:08X})",
            id,
            e,
            e.code().0 as u32
        )),
    }
}

/// Unregister every FreeDrive!* WinRT sync root (and the active id), then drop stale registry keys.
fn purge_stale_and_unregister_winrt(keep_id: &str) {
    let mut ids = list_freedrive_sync_root_ids();
    if !ids.iter().any(|e| e == keep_id) {
        ids.push(keep_id.to_string());
    }
    for id in &ids {
        unregister_winrt_id(id);
    }
    let removed = purge_stale_freedrive_sync_roots(keep_id);
    if !removed.is_empty() {
        sp_log(format!(
            "purged {} stale FreeDrive SyncRootManager key(s); keep={}",
            removed.len(),
            keep_id
        ));
    }
}

/// Register sync root via WinRT (includes CfAPI registration) without custom Status props.
pub fn register_via_winrt(db: &DbHandle, sync_root: &Path) -> AppResult<()> {
    if !sync_root.is_dir() {
        std::fs::create_dir_all(sync_root)
            .map_err(|e| AppError::msg(format!("create sync root: {}", e)))?;
    }

    let id = sync_root_shell_id(db)?;
    // Avoid dual FreeDrive!* roots (Explorer may bind Status to the stale one).
    purge_stale_and_unregister_winrt(&id);

    let info = build_sync_root_info(db, sync_root)?;
    sp_log(format!(
        "StorageProviderSyncRootManager.Register id={} path={} (native Status only)",
        id,
        sync_root.display()
    ));
    StorageProviderSyncRootManager::Register(&info).map_err(|e| {
        AppError::msg(format!(
            "StorageProviderSyncRootManager.Register failed: {} (0x{:08X})",
            e,
            e.code().0 as u32
        ))
    })?;
    clear_custom_state_handler_value(&id);
    mark_status_props(db)?;
    mark_native_only(db)?;
    sp_log(format!(
        "WinRT sync root registered id={} without custom Status property defs / CustomStateHandler",
        id
    ));
    Ok(())
}

fn unregister_winrt(db: &DbHandle) {
    if let Ok(id) = sync_root_shell_id(db) {
        purge_stale_and_unregister_winrt(&id);
    }
}

pub fn unregister_winrt_only(db: &DbHandle) {
    unregister_winrt(db);
}

/// Whether Explorer Status registration must be (re)applied despite `cf_status_props_v1`.
fn needs_status_props_repair(db: &DbHandle) -> AppResult<bool> {
    let keep_id = sync_root_shell_id(db)?;
    if !has_status_props(db)? {
        sp_log(format!(
            "Status repair needed: cf_status_props_v1 unset; keep={}",
            keep_id
        ));
        return Ok(true);
    }
    if !has_native_only(db)? {
        sp_log(format!(
            "Status repair needed: migrate off custom Status property defs; keep={}",
            keep_id
        ));
        return Ok(true);
    }
    if has_stale_freedrive_sync_roots(&keep_id) {
        let extras: Vec<_> = list_freedrive_sync_root_ids()
            .into_iter()
            .filter(|id| id != &keep_id)
            .collect();
        sp_log(format!(
            "Status repair needed: stale FreeDrive sync roots {:?}; keep={}",
            extras, keep_id
        ));
        return Ok(true);
    }
    if active_sync_root_has_custom_state(&keep_id) {
        sp_log(format!(
            "Status repair needed: CustomStateHandler still present (paper Status); keep={}",
            keep_id
        ));
        return Ok(true);
    }
    Ok(false)
}

/// Ensure WinRT sync root is registered without custom Status property definitions.
pub fn ensure_status_props(db: &DbHandle, sync_root: &Path) -> AppResult<()> {
    if !needs_status_props_repair(db)? {
        return Ok(());
    }

    let keep_id = sync_root_shell_id(db)?;
    sp_log(format!(
        "migrating sync root registration for native-only Status; keep={} path={}",
        keep_id,
        sync_root.display()
    ));
    crate::cfapi::connection::disconnect();
    unregister_winrt(db);
    if let Err(e) = unregister_sync_root(sync_root) {
        sp_log(format!("CfUnregister before Status migrate (ok if missing): {}", e));
    }
    let _ = clear_status_props_state(db);

    match register_via_winrt(db, sync_root) {
        Ok(()) => {
            mark_registered(db)?;
            sp_log("native-only Status registration ok");
            Ok(())
        }
        Err(e) => {
            sp_log(format!("WinRT Status register failed, falling back to CfRegister: {}", e));
            // Keep sync working even if WinRT Status registration fails.
            crate::cfapi::register::register_sync_root(db, sync_root)?;
            mark_registered(db)?;
            Err(e)
        }
    }
}

fn notify_item_updated(path: &Path) {
    let wide = crate::cfapi::util::path_to_wide(path);
    unsafe {
        SHChangeNotify(
            SHCNE_UPDATEITEM,
            SHCNF_PATHW,
            Some(wide.as_ptr() as *const _),
            None,
        );
    }
}

fn empty_property_iterable() -> AppResult<IIterable<StorageProviderItemProperty>> {
    let empty: Vec<Option<StorageProviderItemProperty>> = Vec::new();
    empty
        .try_into()
        .map_err(|e| AppError::msg(format!("empty IIterable: {}", e)))
}

fn set_async_clear_item(item: &IStorageItem, path: &Path) -> AppResult<()> {
    let iterable = empty_property_iterable()?;
    StorageProviderItemProperties::SetAsync(item, &iterable)
        .map_err(|e| AppError::msg(format!("SetAsync clear: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("SetAsync clear.get: {}", e)))?;
    notify_item_updated(path);
    Ok(())
}

/// Prefer parent StorageFolder + TryGetItemAsync (CloudMirror-style); fallback GetFileFromPathAsync.
fn clear_item_properties(path: &Path) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::msg("clear Status: no parent"))?;
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| AppError::msg("clear Status: no file name"))?;

    let parent_h = HSTRING::from(parent.to_string_lossy().as_ref());
    if let Ok(op) = StorageFolder::GetFolderFromPathAsync(&parent_h) {
        if let Ok(folder) = op.get() {
            let name_h = HSTRING::from(name);
            if let Ok(item_op) = folder.TryGetItemAsync(&name_h) {
                match item_op.get() {
                    Ok(item) => return set_async_clear_item(&item, path),
                    Err(_) => {}
                }
            }
        }
    }

    let path_h = HSTRING::from(path.to_string_lossy().as_ref());
    let file = StorageFile::GetFileFromPathAsync(&path_h)
        .map_err(|e| AppError::msg(format!("GetFileFromPathAsync: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("GetFileFromPathAsync.get: {}", e)))?;
    let iterable = empty_property_iterable()?;
    StorageProviderItemProperties::SetAsync(&file, &iterable)
        .map_err(|e| AppError::msg(format!("SetAsync clear: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("SetAsync clear.get: {}", e)))?;
    notify_item_updated(path);
    Ok(())
}

/// Walk My Drive and clear cached custom Status properties. Returns (cleared, failed).
pub fn clear_cached_status_properties(my_drive: &Path) -> (u32, u32) {
    if !my_drive.is_dir() {
        sp_log(format!(
            "clear cached Status skipped (missing): {}",
            my_drive.display()
        ));
        return (0, 0);
    }

    let mut cleared = 0u32;
    let mut failed = 0u32;
    let mut stack: Vec<PathBuf> = vec![my_drive.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => {
                sp_log(format!("clear Status read_dir {}: {}", dir.display(), e));
                failed += 1;
                continue;
            }
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if !path.is_file() {
                continue;
            }
            match clear_item_properties(&path) {
                Ok(()) => cleared += 1,
                Err(e) => {
                    failed += 1;
                    // Cap noisy logs (large trees).
                    if failed <= 40 || failed % 500 == 0 {
                        sp_log(format!("clear Status props {}: {}", path.display(), e));
                    }
                }
            }
        }
    }
    notify_directory_updated(my_drive);
    notify_shell_updated();
    sp_log(format!(
        "cleared cached Status properties on {} file(s) (failed={}) under {}",
        cleared,
        failed,
        my_drive.display()
    ));
    (cleared, failed)
}

/// One-time migration (v2): clear SetAsync-cached Status paper icons + shell refresh.
pub fn ensure_cached_status_cleared(db: &DbHandle, my_drive: &Path) -> AppResult<u32> {
    if has_status_props_cleared_v2(db)? {
        return Ok(0);
    }
    if !my_drive.is_dir() {
        return Err(AppError::msg(format!(
            "clear Status v2: My Drive missing {}",
            my_drive.display()
        )));
    }
    let (n, failed) = clear_cached_status_properties(my_drive);
    // Mark done even with per-file failures (handler removal is the main fix);
    // only refuse when the tree itself was unreadable (n==0 && failed==0 already handled).
    if n == 0 && failed > 0 && failed >= 10 {
        sp_log(format!(
            "clear Status v2: all attempts failed (failed={}); will retry next start",
            failed
        ));
        return Err(AppError::msg(format!(
            "clear Status v2 failed for {} file(s)",
            failed
        )));
    }
    mark_status_props_cleared_v2(db)?;
    Ok(n)
}

/// No-op: custom Status IconResource stacks a blank paper glyph next to Windows
/// CfAPI Status (cloud / check / sync). Native glyphs only.
pub fn set_status_property(_path: &Path) -> AppResult<()> {
    Ok(())
}
