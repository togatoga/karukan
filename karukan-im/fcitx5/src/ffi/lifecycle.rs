#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::c_int;

use super::{KarukanEngine, ffi_mut, init_logging};

/// Create a new Karukan engine instance
/// Returns a pointer to the engine, or null on failure
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_new() -> *mut KarukanEngine {
    init_logging();
    let engine = Box::new(KarukanEngine::new());
    Box::into_raw(engine)
}

/// Initialize the kanji converter (loads the model)
/// Returns 0 on success, -1 on failure
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_init(engine: *mut KarukanEngine) -> c_int {
    let engine = ffi_mut!(engine, -1);
    match engine.engine.init_from_settings(&engine.settings) {
        Ok(()) => 0,
        Err(e) => {
            tracing::error!("Karukan init failed: {:#}", e);
            -1
        }
    }
}

/// Destroy a Karukan engine instance
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_free(engine: *mut KarukanEngine) {
    if !engine.is_null() {
        let engine_ref = unsafe { &mut *engine };
        engine_ref.engine.save_learning();
        // SAFETY: Pointer is non-null (checked above) and was created by Box::into_raw in karukan_engine_new
        unsafe {
            drop(Box::from_raw(engine));
        }
    }
}
