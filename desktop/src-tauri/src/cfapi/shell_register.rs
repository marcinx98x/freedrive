use crate::cfapi::register::sync_root_identity_bytes;

use crate::db::{config_get, config_set, DbHandle};

use crate::error::{AppError, AppResult};

use crate::sync::log::sync_log;

use std::path::{Path, PathBuf};

use winreg::enums::*;
use winreg::{HKEY, RegKey, RegValue};

use windows::Win32::Foundation::HANDLE;

use windows::Win32::Security::Authorization::ConvertSidToStringSidW;

use windows::Win32::Security::{GetTokenInformation, TokenUser, TOKEN_QUERY, TOKEN_USER};

use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};



pub const CF_SHELL_REGISTERED_KEY: &str = "cf_shell_registered";

const SHELL_PROVIDER_NAME: &str = "FreeDrive";

const SHELL_ACCOUNT_FALLBACK: &str = "default";

const SYNC_ROOT_MANAGER_KEY: &str =

    r"Software\Microsoft\Windows\CurrentVersion\Explorer\SyncRootManager";



fn shell_log(message: impl AsRef<str>) {

    let line = format!("cfapi: {}", message.as_ref());

    eprintln!("{}", line);

    sync_log(line);

}



pub fn is_shell_registered(db: &DbHandle) -> AppResult<bool> {

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;

    Ok(config_get(&conn, CF_SHELL_REGISTERED_KEY)?.as_deref() == Some("true"))

}



fn mark_shell_registered(db: &DbHandle) -> AppResult<()> {

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;

    config_set(&conn, CF_SHELL_REGISTERED_KEY, "true")?;

    Ok(())

}



pub fn clear_shell_registration_state(db: &DbHandle) -> AppResult<()> {

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;

    config_set(&conn, CF_SHELL_REGISTERED_KEY, "false")?;

    Ok(())

}



pub fn sync_root_shell_id(db: &DbHandle) -> AppResult<String> {

    let account = shell_account_id(db)?;

    let sid = current_user_sid_string()?;

    Ok(format!("{}!{}!{}", SHELL_PROVIDER_NAME, sid, account))

}



fn shell_account_id(db: &DbHandle) -> AppResult<String> {

    let conn = db.lock().map_err(|e| AppError::msg(e.to_string()))?;

    Ok(config_get(&conn, "last_user_id")?.unwrap_or_else(|| SHELL_ACCOUNT_FALLBACK.to_string()))

}



fn current_user_sid_string() -> AppResult<String> {

    unsafe {

        let mut token = HANDLE::default();

        OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token)

            .map_err(|e| AppError::msg(format!("OpenProcessToken failed: {}", e)))?;



        let mut size = 0u32;

        let _ = GetTokenInformation(token, TokenUser, None, 0, &mut size);

        if size == 0 {

            return Err(AppError::msg("GetTokenInformation size query failed"));

        }



        let mut buffer = vec![0u8; size as usize];

        GetTokenInformation(

            token,

            TokenUser,

            Some(buffer.as_mut_ptr() as *mut _),

            size,

            &mut size,

        )

        .map_err(|e| AppError::msg(format!("GetTokenInformation failed: {}", e)))?;



        let token_user = &*(buffer.as_ptr() as *const TOKEN_USER);

        let mut sid_string = windows::core::PWSTR::null();

        ConvertSidToStringSidW(token_user.User.Sid, &mut sid_string)

            .map_err(|e| AppError::msg(format!("ConvertSidToStringSidW failed: {}", e)))?;

        let sid = sid_string.to_string().map_err(|e| AppError::msg(e.to_string()))?;

        let _ = windows::Win32::Foundation::LocalFree(windows::Win32::Foundation::HLOCAL(

            sid_string.0 as _,

        ));

        Ok(sid)

    }

}



fn icon_resource_path() -> String {

    if let Ok(exe) = std::env::current_exe() {

        if let Some(parent) = exe.parent() {

            let bundled = parent.join("icons").join("icon.ico");

            if bundled.exists() {

                return format!("{},0", bundled.display());

            }

        }

    }

    let dev_icon = PathBuf::from(env!("CARGO_MANIFEST_DIR"))

        .join("icons")

        .join("icon.ico");

    if dev_icon.exists() {

        return format!("{},0", dev_icon.display());

    }

    "%SystemRoot%\\System32\\imageres.dll,-189".to_string()

}



fn provider_id_string() -> String {

    "{fd9a2b3c-4d5e-6f70-8899-aabbccddeeff}".to_string()

}



fn is_access_denied(error: &AppError) -> bool {

    let message = error.to_string().to_ascii_lowercase();

    message.contains("access is denied") || message.contains("os error 5")

}



fn sync_root_registry_valid(sync_root_id: &str, sid: &str) -> bool {

    for hive in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {

        let hk = RegKey::predef(hive);

        let key_path = format!("{}\\{}", SYNC_ROOT_MANAGER_KEY, sync_root_id);

        if let Ok(root) = hk.open_subkey(&key_path) {

            if let Ok(user_sync) = root.open_subkey("UserSyncRoots") {

                if user_sync.get_value::<String, _>(sid).is_ok() {

                    return true;

                }

            }

        }

    }

    false

}



fn write_sync_root_registry_to_hive(

    hive: HKEY,

    sync_root_id: &str,

    sync_root: &Path,

    identity: &[u8],

    sid: &str,

) -> AppResult<()> {

    let hk = RegKey::predef(hive);

    let manager = hk

        .create_subkey(SYNC_ROOT_MANAGER_KEY)

        .map_err(|e| AppError::msg(format!("open SyncRootManager failed: {}", e)))?

        .0;



    let root_key = manager

        .create_subkey(sync_root_id)

        .map_err(|e| AppError::msg(format!("create sync root key failed: {}", e)))?

        .0;



    root_key

        .set_value("DisplayNameResource", &SHELL_PROVIDER_NAME)

        .map_err(|e| AppError::msg(format!("set DisplayNameResource failed: {}", e)))?;

    root_key

        .set_value("IconResource", &icon_resource_path())

        .map_err(|e| AppError::msg(format!("set IconResource failed: {}", e)))?;

    root_key

        .set_value("ProviderId", &provider_id_string())

        .map_err(|e| AppError::msg(format!("set ProviderId failed: {}", e)))?;

    root_key

        .set_value(

            "Version",

            &format!("{}.0.0", env!("CARGO_PKG_VERSION")),

        )

        .map_err(|e| AppError::msg(format!("set Version failed: {}", e)))?;

    root_key

        .set_raw_value(

            "Context",

            &RegValue {

                vtype: REG_BINARY,

                bytes: identity.to_vec(),

            },

        )

        .map_err(|e| AppError::msg(format!("set Context failed: {}", e)))?;



    // MS docs: UserSyncRoots\{Windows SID} = sync root path on disk.

    let user_sync_roots = root_key

        .create_subkey("UserSyncRoots")

        .map_err(|e| AppError::msg(format!("create UserSyncRoots failed: {}", e)))?

        .0;

    let path_str = sync_root.to_string_lossy().to_string();

    user_sync_roots

        .set_value(sid, &path_str)

        .map_err(|e| AppError::msg(format!("set UserSyncRoots path failed: {}", e)))?;



    Ok(())

}



fn write_sync_root_registry(

    sync_root_id: &str,

    sync_root: &Path,

    identity: &[u8],

) -> AppResult<()> {

    let sid = current_user_sid_string()?;



    match write_sync_root_registry_to_hive(

        HKEY_LOCAL_MACHINE,

        sync_root_id,

        sync_root,

        identity,

        &sid,

    ) {

        Ok(()) => {

            shell_log("shell registry written to HKLM");

            return Ok(());

        }

        Err(e) if is_access_denied(&e) => {

            shell_log("HKLM shell registry denied, falling back to HKCU");

        }

        Err(e) => return Err(e),

    }



    write_sync_root_registry_to_hive(

        HKEY_CURRENT_USER,

        sync_root_id,

        sync_root,

        identity,

        &sid,

    )?;

    shell_log("shell registry written to HKCU");

    Ok(())

}



fn delete_sync_root_registry_from_hive(hive: HKEY, sync_root_id: &str) -> AppResult<()> {

    let hk = RegKey::predef(hive);

    let manager = hk

        .open_subkey_with_flags(SYNC_ROOT_MANAGER_KEY, KEY_WRITE)

        .map_err(|e| AppError::msg(format!("open SyncRootManager for delete failed: {}", e)))?;

    manager

        .delete_subkey_all(sync_root_id)

        .map_err(|e| AppError::msg(format!("delete sync root registry failed: {}", e)))?;

    Ok(())

}



fn delete_sync_root_registry(sync_root_id: &str) -> AppResult<()> {

    let _ = delete_sync_root_registry_from_hive(HKEY_LOCAL_MACHINE, sync_root_id);

    let _ = delete_sync_root_registry_from_hive(HKEY_CURRENT_USER, sync_root_id);

    Ok(())

}



pub fn ensure_shell_registered(db: &DbHandle, sync_root: &Path) -> AppResult<()> {

    let sync_root_id = sync_root_shell_id(db)?;

    let sid = current_user_sid_string()?;

    let needs_register =

        !is_shell_registered(db)? || !sync_root_registry_valid(&sync_root_id, &sid);

    if !needs_register {

        return Ok(());

    }



    let identity = sync_root_identity_bytes(db)?;



    match write_sync_root_registry(&sync_root_id, sync_root, &identity) {

        Ok(()) => {

            mark_shell_registered(db)?;

            shell_log(&format!("shell registered id={}", sync_root_id));

            Ok(())

        }

        Err(e) => {

            let msg = e.to_string();

            if msg.to_ascii_lowercase().contains("already exists") {

                mark_shell_registered(db)?;

                shell_log(&format!("shell already registered id={}", sync_root_id));

                Ok(())

            } else {

                Err(e)

            }

        }

    }

}



pub fn unregister_shell(db: &DbHandle) -> AppResult<()> {

    let sync_root_id = sync_root_shell_id(db)?;

    let sid = current_user_sid_string().unwrap_or_default();

    if !is_shell_registered(db)? && !sync_root_registry_valid(&sync_root_id, &sid) {

        return Ok(());

    }



    delete_sync_root_registry(&sync_root_id)?;

    clear_shell_registration_state(db)?;

    shell_log("shell unregistered");

    Ok(())

}


