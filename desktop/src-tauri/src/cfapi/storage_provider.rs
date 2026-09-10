//! WinRT Storage Provider registration + Explorer Status column icons.
//!
//! CfRegisterSyncRoot alone does not populate Explorer Status. CloudMirror-style
//! `StorageProviderSyncRootManager.Register` with property definitions +
//! `StorageProviderItemProperties::SetAsync(IconResource)` does.

use crate::cfapi::register::{
    mark_registered, sync_root_identity_bytes, unregister_sync_root,
};
use crate::cfapi::shell_register::{
    active_sync_root_missing_custom_state, has_stale_freedrive_sync_roots, icon_resource_path,
    list_freedrive_sync_root_ids, purge_stale_freedrive_sync_roots, sync_root_shell_id,
};
use crate::cfapi::util::PROVIDER_ID;
use crate::cfapi::placeholders::is_dehydrated_placeholder;
use crate::db::{config_get, config_set, DbHandle};
use crate::error::{AppError, AppResult};
use crate::sync::log::sync_log;
use std::path::Path;
use windows::core::HSTRING;
use windows::Foundation::Collections::IIterable;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::Provider::{
    StorageProviderHardlinkPolicy, StorageProviderHydrationPolicy,
    StorageProviderHydrationPolicyModifier, StorageProviderInSyncPolicy,
    StorageProviderItemProperties, StorageProviderItemProperty,
    StorageProviderItemPropertyDefinition, StorageProviderPopulationPolicy,
    StorageProviderSyncRootInfo, StorageProviderSyncRootManager,
};
use windows::Storage::{StorageFile, StorageFolder};

pub const CF_STATUS_PROPS_KEY: &str = "cf_status_props_v1";
pub const STATUS_PROPERTY_ID: i32 = 1;

/// Cloud / online-only glyph (imageres).
const ICON_ONLINE_ONLY: &str = r"%SystemRoot%\System32\imageres.dll,-506";
/// Locally available / synced glyph (imageres).
const ICON_AVAILABLE_LOCAL: &str = r"%SystemRoot%\System32\imageres.dll,-102";

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

    let defs = info
        .StorageProviderItemPropertyDefinitions()
        .map_err(|e| AppError::msg(format!("StorageProviderItemPropertyDefinitions: {}", e)))?;
    let status_def = StorageProviderItemPropertyDefinition::new()
        .map_err(|e| AppError::msg(format!("StorageProviderItemPropertyDefinition::new: {}", e)))?;
    status_def
        .SetId(STATUS_PROPERTY_ID)
        .map_err(|e| AppError::msg(format!("property SetId: {}", e)))?;
    status_def
        .SetDisplayNameResource(&HSTRING::from("Status"))
        .map_err(|e| AppError::msg(format!("property SetDisplayNameResource: {}", e)))?;
    defs.Append(&status_def)
        .map_err(|e| AppError::msg(format!("property Append: {}", e)))?;

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

/// Register sync root via WinRT (includes CfAPI registration) with Status property def.
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
        "StorageProviderSyncRootManager.Register id={} path={}",
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
    mark_status_props(db)?;
    sp_log(format!(
        "Status property definitions registered id={} CustomStateHandler expected after shell refresh",
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
    if active_sync_root_missing_custom_state(&keep_id) {
        sp_log(format!(
            "Status repair needed: active root missing CustomStateHandler; keep={}",
            keep_id
        ));
        return Ok(true);
    }
    Ok(false)
}

/// Ensure Status property definitions exist (migrate CfRegister-only / dual-root installs).
pub fn ensure_status_props(db: &DbHandle, sync_root: &Path) -> AppResult<()> {
    if !needs_status_props_repair(db)? {
        return Ok(());
    }

    let keep_id = sync_root_shell_id(db)?;
    sp_log(format!(
        "migrating sync root registration for Explorer Status; keep={} path={}",
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
            sp_log("Status property registration ok");
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

/// Set Explorer Status icon for a file under the sync root.
pub fn set_status_property(path: &Path) -> AppResult<()> {
    if !path.is_file() {
        return Ok(());
    }
    let online_only = is_dehydrated_placeholder(path);
    let (value, icon) = if online_only {
        ("Available when online", ICON_ONLINE_ONLY)
    } else {
        ("Available on this device", ICON_AVAILABLE_LOCAL)
    };
    // Prefer system icon; fall back to FreeDrive icon if SetAsync rejects imageres.
    set_status_property_with_icon(path, value, icon).or_else(|e| {
        sp_log(format!(
            "Status SetAsync imageres failed {}, retry with app icon",
            e
        ));
        set_status_property_with_icon(path, value, &icon_resource_path())
    })
}

fn set_status_property_with_icon(path: &Path, value: &str, icon: &str) -> AppResult<()> {
    let path_h = HSTRING::from(path.to_string_lossy().as_ref());
    let file = StorageFile::GetFileFromPathAsync(&path_h)
        .map_err(|e| AppError::msg(format!("GetFileFromPathAsync: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("GetFileFromPathAsync.get: {}", e)))?;

    let prop = StorageProviderItemProperty::new()
        .map_err(|e| AppError::msg(format!("StorageProviderItemProperty::new: {}", e)))?;
    prop.SetId(STATUS_PROPERTY_ID)
        .map_err(|e| AppError::msg(format!("SetId: {}", e)))?;
    prop.SetValue(&HSTRING::from(value))
        .map_err(|e| AppError::msg(format!("SetValue: {}", e)))?;
    // Never pass empty IconResource — Explorer can crash.
    let icon = if icon.trim().is_empty() {
        ICON_ONLINE_ONLY
    } else {
        icon
    };
    prop.SetIconResource(&HSTRING::from(icon))
        .map_err(|e| AppError::msg(format!("SetIconResource: {}", e)))?;

    let iterable: IIterable<StorageProviderItemProperty> = vec![Some(prop)]
        .try_into()
        .map_err(|e| AppError::msg(format!("IIterable: {}", e)))?;
    StorageProviderItemProperties::SetAsync(&file, &iterable)
        .map_err(|e| AppError::msg(format!("SetAsync: {}", e)))?
        .get()
        .map_err(|e| AppError::msg(format!("SetAsync.get: {}", e)))?;
    sp_log(format!(
        "storage provider property set {} value={}",
        path.display(),
        value
    ));
    Ok(())
}

/// Best-effort walk of My Drive files to paint Status icons after reconnect.
pub fn backfill_status_properties(my_drive: &Path) -> u32 {
    if !my_drive.is_dir() {
        return 0;
    }
    let mut painted = 0u32;
    backfill_recursive(my_drive, &mut painted, 0);
    painted
}

fn backfill_recursive(dir: &Path, painted: &mut u32, depth: u32) {
    if depth > 32 || *painted >= 2_000 {
        return;
    }
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            backfill_recursive(&path, painted, depth + 1);
        } else if path.is_file() {
            if set_status_property(&path).is_ok() {
                *painted += 1;
            }
        }
        if *painted >= 2_000 {
            return;
        }
    }
}
