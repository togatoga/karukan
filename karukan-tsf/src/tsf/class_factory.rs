//! IClassFactory implementation for the karukan text service.

use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::core::*;

use crate::globals::{dll_add_ref, dll_release};
use crate::tsf::text_input_processor::KarukanTextService;

/// COM class factory for creating KarukanTextService instances.
#[implement(IClassFactory)]
pub struct KarukanClassFactory;

#[allow(clippy::new_without_default)]
impl KarukanClassFactory {
    pub fn new() -> Self {
        dll_add_ref();
        Self
    }
}

impl Drop for KarukanClassFactory {
    fn drop(&mut self) {
        dll_release();
    }
}

#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl IClassFactory_Impl for KarukanClassFactory_Impl {
    fn CreateInstance(
        &self,
        punkouter: Option<&IUnknown>,
        riid: *const GUID,
        ppvobject: *mut *mut core::ffi::c_void,
    ) -> Result<()> {
        unsafe {
            if ppvobject.is_null() {
                return Err(E_POINTER.into());
            }
            *ppvobject = core::ptr::null_mut();

            // Aggregation not supported
            if punkouter.is_some() {
                return Err(CLASS_E_NOAGGREGATION.into());
            }

            let service: IUnknown = KarukanTextService::new().into();
            service.query(&*riid, ppvobject).ok()
        }
    }

    fn LockServer(&self, flock: BOOL) -> Result<()> {
        if flock.as_bool() {
            dll_add_ref();
        } else {
            dll_release();
        }
        Ok(())
    }
}
