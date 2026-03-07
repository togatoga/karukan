//! Global state and GUID definitions for the karukan TSF IME.

#[cfg(target_os = "windows")]
use once_cell::sync::OnceCell;
#[cfg(target_os = "windows")]
use windows::Win32::Foundation::HMODULE;
#[cfg(target_os = "windows")]
use windows::core::GUID;

/// CLSID for the karukan text service COM class.
///
/// Generated UUID: {A7E3F1B2-4C8D-4F2E-9A1B-6D3E5F7C8A90}
#[cfg(target_os = "windows")]
pub const CLSID_KARUKAN_TEXT_SERVICE: GUID =
    GUID::from_u128(0xA7E3_F1B2_4C8D_4F2E_9A1B_6D3E_5F7C_8A90);

/// GUID for the karukan input profile.
///
/// Generated UUID: {B8F4E2C3-5D9A-4E3F-8B2C-7E4F6A8D9B01}
#[cfg(target_os = "windows")]
pub const GUID_KARUKAN_PROFILE: GUID = GUID::from_u128(0xB8F4_E2C3_5D9A_4E3F_8B2C_7E4F_6A8D_9B01);

/// GUID for the preedit display attribute (underline).
///
/// Generated UUID: {C9A5D3E4-6EAB-4F40-9C3D-8F5A7B9EAC12}
#[cfg(target_os = "windows")]
pub const GUID_DISPLAY_ATTRIBUTE_INPUT: GUID =
    GUID::from_u128(0xC9A5_D3E4_6EAB_4F40_9C3D_8F5A_7B9E_AC12);

/// GUID for the converted text display attribute (bold underline).
///
/// Generated UUID: {DAB6E4F5-7FBC-4051-AD4E-9A6B8CAFBD23}
#[cfg(target_os = "windows")]
pub const GUID_DISPLAY_ATTRIBUTE_CONVERTED: GUID =
    GUID::from_u128(0xDAB6_E4F5_7FBC_4051_AD4E_9A6B_8CAF_BD23);

/// GUID for the PreservedKey ON/OFF toggle (Hankaku/Zenkaku).
///
/// Generated UUID: {EBC7F5A6-8D1E-4F51-BE3D-AF6C9DAE0B34}
#[cfg(target_os = "windows")]
pub const GUID_PRESERVED_KEY_ONOFF: GUID =
    GUID::from_u128(0xEBC7_F5A6_8D1E_4F51_BE3D_AF6C_9DAE_0B34);

/// GUID for the Language Bar button item.
///
/// Generated UUID: {FCD8A6B7-9E2F-4062-CF4E-BA7DAEBF1C45}
#[cfg(target_os = "windows")]
pub const GUID_LANGBAR_ITEM_BUTTON: GUID =
    GUID::from_u128(0xFCD8_A6B7_9E2F_4062_CF4E_BA7D_AEBF_1C45);

/// Japanese language ID (LANGID 0x0411 = Japanese)
pub const LANGID_JAPANESE: u16 = 0x0411;

/// Newtype wrapper around HMODULE to implement Send + Sync.
#[cfg(target_os = "windows")]
#[derive(Clone, Copy)]
pub struct SyncHmodule(pub HMODULE);

#[cfg(target_os = "windows")]
unsafe impl Send for SyncHmodule {}
#[cfg(target_os = "windows")]
unsafe impl Sync for SyncHmodule {}

/// DLL module handle, set during DllMain.
#[cfg(target_os = "windows")]
pub static DLL_INSTANCE: OnceCell<SyncHmodule> = OnceCell::new();

/// Global reference count for COM objects (used by DllCanUnloadNow).
#[cfg(target_os = "windows")]
pub static DLL_REF_COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Increment the global DLL reference count.
#[cfg(target_os = "windows")]
pub fn dll_add_ref() {
    DLL_REF_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Decrement the global DLL reference count.
#[cfg(target_os = "windows")]
pub fn dll_release() {
    DLL_REF_COUNT.fetch_sub(1, std::sync::atomic::Ordering::SeqCst);
}

/// Check if the DLL can be unloaded (no outstanding references).
#[cfg(target_os = "windows")]
pub fn dll_can_unload() -> bool {
    DLL_REF_COUNT.load(std::sync::atomic::Ordering::SeqCst) == 0
}
