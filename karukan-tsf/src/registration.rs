//! COM/TSF registration and unregistration.
//!
//! Handles writing/removing registry entries for the karukan TSF text service.
//! Called from DllRegisterServer/DllUnregisterServer.

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::{BOOL, HMODULE};
#[cfg(target_os = "windows")]
use windows::Win32::System::LibraryLoader::GetModuleFileNameW;
#[cfg(target_os = "windows")]
use windows::Win32::System::Registry::*;
#[cfg(target_os = "windows")]
use windows::core::w;

#[cfg(target_os = "windows")]
use crate::globals::*;

/// Register the COM class and TSF profile.
///
/// Creates the following registry entries:
/// - `HKCR\CLSID\{clsid}` — COM class registration
/// - `HKCR\CLSID\{clsid}\InProcServer32` — DLL path and threading model
///
/// Also registers with TSF via `ITfInputProcessorProfileMgr::RegisterProfile`
/// and `ITfCategoryMgr::RegisterCategory`.
#[cfg(target_os = "windows")]
pub fn register_server() -> windows::core::Result<()> {
    use windows::Win32::UI::TextServices::*;

    let dll_path = get_dll_path()?;
    let clsid_str = format!("{{{:?}}}", CLSID_KARUKAN_TEXT_SERVICE);

    // Register CLSID in HKCR
    let key_path = format!("CLSID\\{}", clsid_str);
    unsafe {
        let mut hkey = HKEY::default();
        RegCreateKeyW(
            HKEY_CLASSES_ROOT,
            &windows::core::HSTRING::from(&key_path),
            &mut hkey,
        )
        .ok()?;
        let desc = w!("karukan Japanese Input Method");
        RegSetValueExW(
            hkey,
            None,
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                desc.as_ptr() as *const u8,
                (desc.len() + 1) * 2,
            )),
        )
        .ok()?;
        RegCloseKey(hkey).ok()?;

        // InProcServer32
        let inproc_path = format!("{}\\InProcServer32", key_path);
        let mut hkey_inproc = HKEY::default();
        RegCreateKeyW(
            HKEY_CLASSES_ROOT,
            &windows::core::HSTRING::from(&inproc_path),
            &mut hkey_inproc,
        )
        .ok()?;

        let dll_path_wide: Vec<u16> = dll_path.encode_utf16().chain(std::iter::once(0)).collect();
        RegSetValueExW(
            hkey_inproc,
            None,
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                dll_path_wide.as_ptr() as *const u8,
                dll_path_wide.len() * 2,
            )),
        )
        .ok()?;

        let threading = w!("Apartment");
        RegSetValueExW(
            hkey_inproc,
            w!("ThreadingModel"),
            0,
            REG_SZ,
            Some(std::slice::from_raw_parts(
                threading.as_ptr() as *const u8,
                (threading.len() + 1) * 2,
            )),
        )
        .ok()?;
        RegCloseKey(hkey_inproc).ok()?;
    }

    // Register TSF profile
    unsafe {
        use windows::Win32::UI::Input::KeyboardAndMouse::HKL;

        let profile_mgr: ITfInputProcessorProfileMgr =
            windows::Win32::System::Com::CoCreateInstance(
                &CLSID_TF_InputProcessorProfiles,
                None,
                windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            )?;

        let desc_wide: Vec<u16> = "karukan".encode_utf16().collect();
        profile_mgr.RegisterProfile(
            &CLSID_KARUKAN_TEXT_SERVICE,
            LANGID_JAPANESE,
            &GUID_KARUKAN_PROFILE,
            &desc_wide,
            &[],            // icon file (none)
            0,              // icon index
            HKL::default(), // HKL (none)
            0,              // description (none)
            BOOL(0),        // flags
            0,              // enable
        )?;

        // Register category
        let cat_mgr: ITfCategoryMgr = windows::Win32::System::Com::CoCreateInstance(
            &CLSID_TF_CategoryMgr,
            None,
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
        )?;

        cat_mgr.RegisterCategory(
            &CLSID_KARUKAN_TEXT_SERVICE,
            &GUID_TFCAT_TIP_KEYBOARD,
            &CLSID_KARUKAN_TEXT_SERVICE,
        )?;

        cat_mgr.RegisterCategory(
            &CLSID_KARUKAN_TEXT_SERVICE,
            &GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
            &CLSID_KARUKAN_TEXT_SERVICE,
        )?;
    }

    Ok(())
}

/// Unregister the COM class and TSF profile.
#[cfg(target_os = "windows")]
pub fn unregister_server() -> windows::core::Result<()> {
    use windows::Win32::UI::TextServices::*;

    let clsid_str = format!("{{{:?}}}", CLSID_KARUKAN_TEXT_SERVICE);

    // Unregister TSF profile
    unsafe {
        if let Ok(profile_mgr) =
            windows::Win32::System::Com::CoCreateInstance::<_, ITfInputProcessorProfileMgr>(
                &CLSID_TF_InputProcessorProfiles,
                None,
                windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
            )
        {
            let _ = profile_mgr.UnregisterProfile(
                &CLSID_KARUKAN_TEXT_SERVICE,
                LANGID_JAPANESE,
                &GUID_KARUKAN_PROFILE,
                0,
            );
        }

        if let Ok(cat_mgr) = windows::Win32::System::Com::CoCreateInstance::<_, ITfCategoryMgr>(
            &CLSID_TF_CategoryMgr,
            None,
            windows::Win32::System::Com::CLSCTX_INPROC_SERVER,
        ) {
            let _ = cat_mgr.UnregisterCategory(
                &CLSID_KARUKAN_TEXT_SERVICE,
                &GUID_TFCAT_TIP_KEYBOARD,
                &CLSID_KARUKAN_TEXT_SERVICE,
            );
            let _ = cat_mgr.UnregisterCategory(
                &CLSID_KARUKAN_TEXT_SERVICE,
                &GUID_TFCAT_DISPLAYATTRIBUTEPROVIDER,
                &CLSID_KARUKAN_TEXT_SERVICE,
            );
        }
    }

    // Remove CLSID registry keys
    unsafe {
        let inproc_path = format!("CLSID\\{}\\InProcServer32", clsid_str);
        let _ = RegDeleteKeyW(
            HKEY_CLASSES_ROOT,
            &windows::core::HSTRING::from(&inproc_path),
        );
        let key_path = format!("CLSID\\{}", clsid_str);
        let _ = RegDeleteKeyW(HKEY_CLASSES_ROOT, &windows::core::HSTRING::from(&key_path));
    }

    Ok(())
}

/// Get the full path of the current DLL module.
#[cfg(target_os = "windows")]
fn get_dll_path() -> windows::core::Result<String> {
    let hmodule = DLL_INSTANCE.get().map(|s| s.0).unwrap_or_default();
    let mut buf = [0u16; 260];
    let len = unsafe { GetModuleFileNameW(hmodule, &mut buf) } as usize;
    Ok(String::from_utf16_lossy(&buf[..len]))
}

// Stubs for non-Windows platforms (allow compilation and testing on Linux)
#[cfg(not(target_os = "windows"))]
pub fn register_server() -> Result<(), String> {
    Err("TSF registration is only supported on Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn unregister_server() -> Result<(), String> {
    Err("TSF unregistration is only supported on Windows".to_string())
}
