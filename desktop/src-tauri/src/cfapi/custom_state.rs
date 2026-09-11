//! Legacy CustomStateHandler cleanup.
//!
//! FreeDrive no longer hosts a CustomState COM server or paints custom Status
//! IconResource (native CfAPI glyphs only). These helpers only remove leftover
//! HKCU CLSID\LocalServer32 entries from older installs.

use crate::sync::log::sync_log;

/// Stable FreeDrive CustomStateHandler CLSID (must match historical registry).
pub fn custom_state_clsid_string() -> String {
    "{FD9A2B3C-4D5E-6F70-8899-AABBCCDDEE02}".to_string()
}

/// No-op: in-process CustomState COM server is gone (0.1.58+).
pub fn stop_custom_state_com_server() {}

/// Remove legacy CLSID\LocalServer32 so Explorer cannot re-activate old handlers.
pub fn unregister_custom_state_com_registry() {
    let clsid = custom_state_clsid_string();
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{}", clsid));
    let line = "cfapi: CustomStateHandler COM registry removed";
    eprintln!("{}", line);
    sync_log(line);
}
