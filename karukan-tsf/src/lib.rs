//! karukan-tsf: Windows TSF (Text Services Framework) IME addon for karukan.
//!
//! This crate provides a Windows IME using the TSF framework, reusing
//! karukan-im's InputMethodEngine for the core input processing logic.
//!
//! # Architecture
//!
//! - `engine_bridge` — wraps InputMethodEngine, handles initialization
//! - `keymap` — Windows VK code → karukan Keysym conversion
//! - `globals` — CLSID/GUID definitions, DLL reference counting
//! - `registration` — COM/TSF registry registration
//! - `tsf/` — COM interface implementations (ITfTextInputProcessor, etc.)
//! - `candidate/` — Win32 candidate window

pub mod engine_bridge;
pub mod globals;
pub mod keymap;
pub mod registration;

#[cfg(target_os = "windows")]
pub mod tsf;

#[cfg(target_os = "windows")]
pub mod candidate;

// DLL entry points (Windows only)
#[cfg(target_os = "windows")]
mod dll_exports {
    use windows::Win32::Foundation::*;
    use windows::core::*;

    use crate::globals::*;

    const DLL_PROCESS_ATTACH: u32 = 1;
    use crate::tsf::class_factory::KarukanClassFactory;

    /// DLL entry point — saves the module handle.
    #[unsafe(no_mangle)]
    extern "system" fn DllMain(
        hinstance: HINSTANCE,
        reason: u32,
        _reserved: *mut core::ffi::c_void,
    ) -> BOOL {
        if reason == DLL_PROCESS_ATTACH {
            let _ = DLL_INSTANCE.set(SyncHmodule(HMODULE(hinstance.0)));

            // Initialize logging
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
                )
                .with_writer(std::io::stderr)
                .init();
        }
        TRUE
    }

    /// COM entry point — creates a class factory for the given CLSID.
    #[unsafe(no_mangle)]
    extern "system" fn DllGetClassObject(
        rclsid: *const GUID,
        riid: *const GUID,
        ppv: *mut *mut core::ffi::c_void,
    ) -> HRESULT {
        unsafe {
            if ppv.is_null() || rclsid.is_null() || riid.is_null() {
                return E_POINTER;
            }
            *ppv = core::ptr::null_mut();

            if *rclsid != CLSID_KARUKAN_TEXT_SERVICE {
                return CLASS_E_CLASSNOTAVAILABLE;
            }

            let factory: IUnknown = KarukanClassFactory::new().into();
            factory.query(&*riid, ppv)
        }
    }

    /// COM entry point — checks if the DLL can be unloaded.
    #[unsafe(no_mangle)]
    extern "system" fn DllCanUnloadNow() -> HRESULT {
        if dll_can_unload() { S_OK } else { S_FALSE }
    }

    /// COM entry point — registers the text service.
    #[unsafe(no_mangle)]
    extern "system" fn DllRegisterServer() -> HRESULT {
        match crate::registration::register_server() {
            Ok(()) => S_OK,
            Err(e) => e.code(),
        }
    }

    /// COM entry point — unregisters the text service.
    #[unsafe(no_mangle)]
    extern "system" fn DllUnregisterServer() -> HRESULT {
        match crate::registration::unregister_server() {
            Ok(()) => S_OK,
            Err(e) => e.code(),
        }
    }
}
