//! Explorer CustomStateHandler: COM `IStorageProviderItemPropertySource`.
//!
//! Without this, `SetAsync` alone does not paint Status glyphs. Nextcloud-style:
//! SyncRootManager\...\CustomStateHandler = {CLSID}, LocalServer32 = this exe,
//! and `CoRegisterClassObject` while FreeDrive is running.

use crate::cfapi::placeholders::is_dehydrated_placeholder;
use crate::cfapi::shell_register::icon_resource_path;
use crate::cfapi::storage_provider::STATUS_PROPERTY_ID;
use crate::error::{AppError, AppResult};
use crate::sync::log::sync_log;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use windows::core::{implement, Interface, GUID, HRESULT, HSTRING};
use windows::Foundation::Collections::IIterable;
use windows::Storage::Provider::{
    IStorageProviderItemPropertySource, IStorageProviderItemPropertySource_Impl,
    StorageProviderItemProperty,
};
use windows::Win32::Foundation::{BOOL, CLASS_E_NOAGGREGATION};
use windows::Win32::System::Com::{
    CoInitializeEx, CoRegisterClassObject, CoRevokeClassObject, IClassFactory, IClassFactory_Impl,
    CLSCTX_LOCAL_SERVER, COINIT_APARTMENTTHREADED, REGCLS_MULTIPLEUSE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, TranslateMessage, MSG,
};

/// Stable FreeDrive CustomStateHandler CLSID (must match registry).
pub const CUSTOM_STATE_CLSID: GUID = GUID::from_u128(0xfd9a2b3c_4d5e_6f70_8899_aabbccddee02);

pub fn custom_state_clsid_string() -> String {
    "{FD9A2B3C-4D5E-6F70-8899-AABBCCDDEE02}".to_string()
}

fn cs_log(message: impl AsRef<str>) {
    let line = format!("cfapi: {}", message.as_ref());
    eprintln!("{}", line);
    sync_log(line);
}

fn system_icon(resource: i32) -> String {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
    format!(r"{}\System32\imageres.dll,{}", root, resource)
}

fn status_icon_for_path(path: &Path) -> (String, String) {
    if is_dehydrated_placeholder(path) {
        ("Available when online".into(), system_icon(-506))
    } else {
        let app = icon_resource_path();
        if app.contains(".exe") || app.contains(".ico") {
            ("Available on this device".into(), app)
        } else {
            ("Available on this device".into(), system_icon(-102))
        }
    }
}

#[implement(IStorageProviderItemPropertySource)]
struct CustomStateHandler;

impl IStorageProviderItemPropertySource_Impl for CustomStateHandler_Impl {
    fn GetItemProperties(
        &self,
        item_path: &HSTRING,
    ) -> windows::core::Result<IIterable<StorageProviderItemProperty>> {
        let path_str = item_path.to_string();
        let path = Path::new(&path_str);
        let (value, icon) = if path.exists() {
            status_icon_for_path(path)
        } else {
            ("Available when online".into(), system_icon(-506))
        };

        let prop = StorageProviderItemProperty::new()?;
        prop.SetId(STATUS_PROPERTY_ID)?;
        prop.SetValue(&HSTRING::from(value.as_str()))?;
        let icon = if icon.trim().is_empty() {
            system_icon(-506)
        } else {
            icon
        };
        prop.SetIconResource(&HSTRING::from(icon.as_str()))?;

        vec![Some(prop)]
            .try_into()
            .map_err(|_| windows::core::Error::from(HRESULT(0x80004005u32 as i32)))
    }
}

#[implement(IClassFactory)]
struct CustomStateFactory;

impl IClassFactory_Impl for CustomStateFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&windows::core::IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> windows::core::Result<()> {
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let handler: IStorageProviderItemPropertySource = CustomStateHandler.into();
        unsafe { handler.query(riid, ppvobject).ok() }
    }

    fn LockServer(&self, _flock: BOOL) -> windows::core::Result<()> {
        Ok(())
    }
}

static STARTED: AtomicBool = AtomicBool::new(false);
static COOKIE: AtomicU32 = AtomicU32::new(0);

fn register_class_object_on_current_thread() -> AppResult<()> {
    let factory: IClassFactory = CustomStateFactory.into();
    let cookie = unsafe {
        CoRegisterClassObject(
            &CUSTOM_STATE_CLSID,
            &factory,
            CLSCTX_LOCAL_SERVER,
            REGCLS_MULTIPLEUSE,
        )
    }
    .map_err(|e| AppError::msg(format!("CoRegisterClassObject CustomStateHandler: {}", e)))?;
    // Keep factory alive for the process lifetime (COM holds a ref, but be safe).
    std::mem::forget(factory);
    COOKIE.store(cookie, Ordering::SeqCst);
    cs_log(format!(
        "CustomStateHandler CoRegisterClassObject ok clsid={}",
        custom_state_clsid_string()
    ));
    Ok(())
}

/// Spawn STA thread that keeps CustomStateHandler class object alive for Explorer.
pub fn start_custom_state_com_server() {
    if STARTED.swap(true, Ordering::SeqCst) {
        return;
    }
    std::thread::Builder::new()
        .name("fd-custom-state".into())
        .spawn(|| {
            unsafe {
                let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                // S_OK=0, S_FALSE=1 (already initialized).
                if hr.is_err() {
                    let code = hr.0;
                    if code != 0 && code != 1 {
                        cs_log(format!(
                            "CustomStateHandler CoInitializeEx hr=0x{:08X}",
                            code as u32
                        ));
                    }
                }
            }
            if let Err(e) = register_class_object_on_current_thread() {
                cs_log(format!("CustomStateHandler register failed: {}", e));
                return;
            }
            loop {
                let mut msg = MSG::default();
                let ok = unsafe { GetMessageW(&mut msg, None, 0, 0) };
                if !ok.as_bool() {
                    break;
                }
                unsafe {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
            let cookie = COOKIE.swap(0, Ordering::SeqCst);
            if cookie != 0 {
                unsafe {
                    let _ = CoRevokeClassObject(cookie);
                }
            }
        })
        .ok();
}

/// Dedicated process mode: LocalServer32 launches `exe --com-custom-state`.
pub fn run_com_custom_state_process() -> ! {
    cs_log("CustomStateHandler LocalServer process started");
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
    }
    if let Err(e) = register_class_object_on_current_thread() {
        cs_log(format!("CustomStateHandler LocalServer register failed: {}", e));
        std::process::exit(1);
    }
    loop {
        let mut msg = MSG::default();
        let ok = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if !ok.as_bool() {
            break;
        }
        unsafe {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    std::process::exit(0);
}

pub fn stop_custom_state_com_server() {
    let cookie = COOKIE.swap(0, Ordering::SeqCst);
    if cookie != 0 {
        unsafe {
            let _ = CoRevokeClassObject(cookie);
        }
        cs_log("CustomStateHandler CoRevokeClassObject");
    }
}

/// CLSID\LocalServer32 registration for Explorer out-of-proc activation.
pub fn register_custom_state_com_registry() -> AppResult<()> {
    let exe = std::env::current_exe()
        .map_err(|e| AppError::msg(format!("current_exe: {}", e)))?;
    let local_server = format!("\"{}\" --com-custom-state", exe.display());
    let clsid = custom_state_clsid_string();
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let (clsid_key, _) = hkcu
        .create_subkey(format!(r"Software\Classes\CLSID\{}", clsid))
        .map_err(|e| AppError::msg(format!("create CLSID key: {}", e)))?;
    clsid_key
        .set_value("", &"FreeDrive Custom State Handler")
        .map_err(|e| AppError::msg(format!("set CLSID default: {}", e)))?;
    let (local, _) = clsid_key
        .create_subkey("LocalServer32")
        .map_err(|e| AppError::msg(format!("create LocalServer32: {}", e)))?;
    local
        .set_value("", &local_server)
        .map_err(|e| AppError::msg(format!("set LocalServer32: {}", e)))?;
    cs_log(format!("CustomStateHandler COM registry ok {}", local_server));
    Ok(())
}

pub fn unregister_custom_state_com_registry() {
    let clsid = custom_state_clsid_string();
    let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{}", clsid));
    cs_log("CustomStateHandler COM registry removed");
}
