//! FreeDrive Explorer ThumbnailProvider (InProc).
//! Serves JPEG cache under `%LOCALAPPDATA%\FreeDrive\thumbs\` for My Drive paths.
//! Outside My Drive, delegates to the previous ShellEx ThumbnailProvider CLSID.

#![allow(non_snake_case)]

use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fs;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::HBITMAP;
use windows::Win32::System::Com::*;
use windows::Win32::System::LibraryLoader::{DisableThreadLibraryCalls, GetModuleFileNameW};
use windows::Win32::UI::Shell::*;

const CLSID_THUMB: GUID = GUID::from_u128(0xFD9A2B3C_4D5E_6F70_8899_AABBCCDDEE10);
const HANDLER_SHELLEX: &str = "e357fccd-a995-4576-b01f-234630154e96";

fn thumbs_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("FreeDrive").join("thumbs"))
}

fn freedrive_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|d| d.join("FreeDrive"))
}

fn my_drive_root() -> Option<PathBuf> {
    let p = freedrive_dir()?.join("my_drive_root.txt");
    let s = fs::read_to_string(p).ok()?;
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(PathBuf::from(trimmed))
}

fn path_under_my_drive(path: &Path) -> bool {
    let Some(root) = my_drive_root() else {
        return false;
    };
    let pl = path.to_string_lossy().to_lowercase().replace('/', "\\");
    let rl = root.to_string_lossy().to_lowercase().replace('/', "\\");
    let rl = rl.trim_end_matches('\\');
    pl == rl || pl.starts_with(&(rl.to_string() + "\\"))
}

fn path_cache_key(path: &Path) -> String {
    let normalized = path.to_string_lossy().to_lowercase().replace('/', "\\");
    hex::encode(Sha256::digest(normalized.as_bytes()))
}

fn cache_path_for_local(path: &Path) -> Option<PathBuf> {
    Some(thumbs_dir()?.join(format!("p_{}.jpg", path_cache_key(path))))
}

fn jpeg_to_hbitmap(jpeg: &[u8], cx: u32) -> Result<HBITMAP> {
    let dir = thumbs_dir().ok_or_else(|| Error::from(E_FAIL))?;
    let _ = fs::create_dir_all(&dir);
    let tmp = dir.join(format!("_tmp_{}.jpg", std::process::id()));
    fs::write(&tmp, jpeg).map_err(|_| Error::from(E_FAIL))?;
    let wide: Vec<u16> = tmp
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let item: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
        let size = SIZE {
            cx: cx as i32,
            cy: cx as i32,
        };
        let hbmp = item.GetImage(size, SIIGBF_RESIZETOFIT | SIIGBF_BIGGERSIZEOK)?;
        let _ = fs::remove_file(&tmp);
        Ok(hbmp)
    }
}

fn clsid_string() -> String {
    "{FD9A2B3C-4D5E-6F70-8899-AABBCCDDEE10}".to_string()
}

fn ext_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn previous_handler_clsid(ext: &str) -> Option<GUID> {
    let p = freedrive_dir()?.join(format!("prev_thumb_{ext}.txt"));
    let s = fs::read_to_string(p).ok()?;
    let s = s.trim();
    if s.is_empty() || s.eq_ignore_ascii_case(&clsid_string()) {
        return None;
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe {
        let g = CLSIDFromString(PCWSTR(wide.as_ptr())).ok()?;
        Some(g)
    }
}

fn delegate_thumbnail(path: &Path, cx: u32) -> Result<(HBITMAP, WTS_ALPHATYPE)> {
    let ext = ext_of(path);
    let guid = previous_handler_clsid(&ext).ok_or_else(|| Error::from(E_FAIL))?;
    unsafe {
        let unk: IUnknown = CoCreateInstance(&guid, None, CLSCTX_INPROC_SERVER)?;
        if let Ok(init) = unk.cast::<IInitializeWithItem>() {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let item: IShellItem = SHCreateItemFromParsingName(PCWSTR(wide.as_ptr()), None)?;
            // STGM_READ = 0
            init.Initialize(&item, 0)?;
        }
        let provider: IThumbnailProvider = unk.cast()?;
        let mut hbmp = HBITMAP::default();
        let mut alpha = WTSAT_UNKNOWN;
        provider.GetThumbnail(cx, &mut hbmp, &mut alpha)?;
        Ok((hbmp, alpha))
    }
}

#[implement(IInitializeWithItem, IThumbnailProvider)]
struct ThumbProvider {
    path: Mutex<Option<PathBuf>>,
}

impl ThumbProvider {
    fn new() -> Self {
        Self {
            path: Mutex::new(None),
        }
    }
}

impl IInitializeWithItem_Impl for ThumbProvider_Impl {
    fn Initialize(&self, psi: Option<&IShellItem>, _grfmode: u32) -> Result<()> {
        let psi = psi.ok_or_else(|| Error::from(E_INVALIDARG))?;
        unsafe {
            let name = psi.GetDisplayName(SIGDN_FILESYSPATH)?;
            let path = PathBuf::from(OsString::from_wide(name.as_wide()));
            CoTaskMemFree(Some(name.0 as *const _));
            *self.path.lock().unwrap() = Some(path);
            Ok(())
        }
    }
}

impl IThumbnailProvider_Impl for ThumbProvider_Impl {
    fn GetThumbnail(
        &self,
        cx: u32,
        phbmp: *mut HBITMAP,
        pdwalpha: *mut WTS_ALPHATYPE,
    ) -> Result<()> {
        unsafe {
            if phbmp.is_null() || pdwalpha.is_null() {
                return Err(E_POINTER.into());
            }
            *phbmp = HBITMAP::default();
            *pdwalpha = WTSAT_UNKNOWN;
        }
        let path = self
            .path
            .lock()
            .unwrap()
            .clone()
            .ok_or_else(|| Error::from(E_UNEXPECTED))?;

        if !path_under_my_drive(&path) {
            let (hbmp, alpha) = delegate_thumbnail(&path, cx)?;
            unsafe {
                *phbmp = hbmp;
                *pdwalpha = alpha;
            }
            return Ok(());
        }

        let cache = cache_path_for_local(&path).ok_or_else(|| Error::from(E_FAIL))?;
        let jpeg = fs::read(&cache).map_err(|_| Error::from(E_FAIL))?;
        let hbmp = jpeg_to_hbitmap(&jpeg, cx.max(32))?;
        unsafe {
            *phbmp = hbmp;
            *pdwalpha = WTSAT_RGB;
        }
        Ok(())
    }
}

#[implement(IClassFactory)]
struct ThumbFactory;

impl IClassFactory_Impl for ThumbFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        if punkouter.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IThumbnailProvider = ThumbProvider::new().into();
        unsafe {
            provider.query(riid, ppvobject).ok()?;
        }
        Ok(())
    }

    fn LockServer(&self, _flock: BOOL) -> Result<()> {
        Ok(())
    }
}

static mut DLL_MODULE: HMODULE = HMODULE(std::ptr::null_mut());

#[no_mangle]
pub unsafe extern "system" fn DllMain(
    module: HMODULE,
    reason: u32,
    _reserved: *mut core::ffi::c_void,
) -> BOOL {
    if reason == 1 {
        DLL_MODULE = module;
        let _ = DisableThreadLibraryCalls(module);
    }
    TRUE
}

#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut core::ffi::c_void,
) -> HRESULT {
    if rclsid.is_null() || *rclsid != CLSID_THUMB {
        return CLASS_E_CLASSNOTAVAILABLE;
    }
    let factory: IClassFactory = ThumbFactory.into();
    factory.query(riid, ppv)
}

#[no_mangle]
pub unsafe extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}

fn dll_path() -> Result<PathBuf> {
    unsafe {
        let mut buf = vec![0u16; 520];
        let n = GetModuleFileNameW(DLL_MODULE, &mut buf);
        if n == 0 {
            return Err(Error::from_win32());
        }
        Ok(PathBuf::from(OsString::from_wide(&buf[..n as usize])))
    }
}

fn register_server() -> Result<()> {
    use winreg::enums::*;
    use winreg::RegKey;
    let dll = dll_path()?;
    let dll_s = dll.to_string_lossy().to_string();
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = clsid_string();
    let (clsid_key, _) = hkcu.create_subkey(format!(r"Software\Classes\CLSID\{clsid}"))?;
    let _ = clsid_key.set_value("", &"FreeDrive Thumbnail Provider");
    let (inproc, _) = clsid_key.create_subkey("InProcServer32")?;
    let _ = inproc.set_value("", &dll_s);
    let _ = inproc.set_value("ThreadingModel", &"Apartment");

    let fd = freedrive_dir().ok_or_else(|| Error::from(E_FAIL))?;
    let _ = fs::create_dir_all(&fd);

    for ext in [
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "mp4", "mov", "m4v", "avi",
        "mkv", "wmv", "webm",
    ] {
        let path = format!(r"Software\Classes\.{ext}\ShellEx\{{{HANDLER_SHELLEX}}}");
        let (key, _) = hkcu.create_subkey(&path)?;
        let prev: String = key.get_value("").unwrap_or_default();
        if !prev.is_empty() && !prev.eq_ignore_ascii_case(&clsid) {
            let _ = fs::write(fd.join(format!("prev_thumb_{ext}.txt")), prev);
        }
        let _ = key.set_value("", &clsid);
    }
    Ok(())
}

#[no_mangle]
pub unsafe extern "system" fn DllRegisterServer() -> HRESULT {
    match register_server() {
        Ok(()) => S_OK,
        Err(e) => e.code(),
    }
}

#[no_mangle]
pub unsafe extern "system" fn DllUnregisterServer() -> HRESULT {
    use winreg::enums::*;
    use winreg::RegKey;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let clsid = clsid_string();
    let fd = freedrive_dir();
    for ext in [
        "jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "mp4", "mov", "m4v", "avi",
        "mkv", "wmv", "webm",
    ] {
        let path = format!(r"Software\Classes\.{ext}\ShellEx\{{{HANDLER_SHELLEX}}}");
        if let Some(ref dir) = fd {
            if let Ok(prev) = fs::read_to_string(dir.join(format!("prev_thumb_{ext}.txt"))) {
                let prev = prev.trim();
                if !prev.is_empty() {
                    if let Ok((key, _)) = hkcu.create_subkey(&path) {
                        let _ = key.set_value("", &prev);
                        continue;
                    }
                }
            }
        }
        let _ = hkcu.delete_subkey_all(&path);
    }
    let _ = hkcu.delete_subkey_all(format!(r"Software\Classes\CLSID\{clsid}"));
    S_OK
}
