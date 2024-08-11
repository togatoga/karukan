#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CString, c_char, c_int, c_uint};
use std::ptr;

use super::{KarukanEngine, ffi_mut, ffi_ref};

/// Check if there's a preedit update pending
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_has_preedit(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.preedit.dirty { 1 } else { 0 }
}

/// Get the current preedit text
/// Returns a pointer to a null-terminated UTF-8 string (valid until next process_key call)
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_preedit(engine: *const KarukanEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.preedit.text.as_ptr()
}

/// Get the preedit length in bytes
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_preedit_len(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.preedit.text.as_bytes().len() as c_uint
}

/// Get the preedit caret position in bytes
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_preedit_caret(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.preedit.caret_bytes as c_uint
}

/// Check if there's a commit pending
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_has_commit(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.commit.dirty { 1 } else { 0 }
}

/// Get the commit text
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_commit(engine: *const KarukanEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.commit.text.as_ptr()
}

/// Get the commit text length in bytes
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_commit_len(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.commit.text.as_bytes().len() as c_uint
}

/// Check if there's a candidates update pending
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_has_candidates(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.candidates.dirty { 1 } else { 0 }
}

/// Check if candidates should be hidden
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_should_hide_candidates(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.candidates.hide { 1 } else { 0 }
}

/// Get the number of candidates
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_candidate_count(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.candidates.count as c_uint
}

/// Get a candidate by index
/// Returns a pointer to a null-terminated UTF-8 string, or null if index is out of range
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_candidate(
    engine: *const KarukanEngine,
    index: c_uint,
) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine
        .candidates
        .texts
        .get(index as usize)
        .map(|c| c.as_ptr())
        .unwrap_or(ptr::null())
}

/// Get a candidate annotation (comment) by index
/// Returns a pointer to a null-terminated UTF-8 string, or null if index is out of range
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_candidate_annotation(
    engine: *const KarukanEngine,
    index: c_uint,
) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine
        .candidates
        .annotations
        .get(index as usize)
        .map(|c| c.as_ptr())
        .unwrap_or(ptr::null())
}

/// Get the current candidate cursor position
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_candidate_cursor(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.candidates.cursor as c_uint
}

/// Check if there's an aux text update pending
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_has_aux(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.aux.dirty { 1 } else { 0 }
}

/// Get the aux text
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_aux(engine: *const KarukanEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.aux.text.as_ptr()
}

/// Get the aux text length in bytes
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_aux_len(engine: *const KarukanEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.aux.text.as_bytes().len() as c_uint
}

/// Get the last conversion time in milliseconds (inference only)
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_last_conversion_ms(engine: *const KarukanEngine) -> u64 {
    let engine = ffi_ref!(engine, 0);
    engine.last_conversion_ms
}

/// Get the last process_key time in milliseconds (input to result, end-to-end)
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_get_last_process_key_ms(engine: *const KarukanEngine) -> u64 {
    let engine = ffi_ref!(engine, 0);
    engine.last_process_key_ms
}

/// Save the learning cache to disk if there are unsaved changes.
/// Called on deactivate (IME switch / window switch) for periodic persistence.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_save_learning(engine: *mut KarukanEngine) {
    let engine = ffi_mut!(engine);
    engine.engine.save_learning();
}

/// Commit any pending input and return the committed text
/// This is used when the IME is deactivated (focus lost) to commit preedit
/// Check if the engine is in the Empty (idle) state.
/// Returns 1 if empty, 0 if composing or converting.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_is_empty(engine: *const KarukanEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.engine.state().is_empty() {
        1
    } else {
        0
    }
}

/// Returns 1 if text was committed, 0 otherwise
#[unsafe(no_mangle)]
pub extern "C" fn karukan_engine_commit(engine: *mut KarukanEngine) -> c_int {
    let engine = ffi_mut!(engine, 0);
    let text = engine.engine.commit();

    if text.is_empty() {
        return 0;
    }

    engine.commit.text = CString::new(text).unwrap_or_default();
    engine.commit.dirty = true;
    1
}
