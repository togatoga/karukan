#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{CString, c_char, c_int, c_uint};
use std::ptr;

use super::{KarukanMacEngine, ffi_mut, ffi_ref};

// --- Preedit ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_has_preedit(engine: *const KarukanMacEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.preedit.dirty { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_preedit(engine: *const KarukanMacEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.preedit.text.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_preedit_len(engine: *const KarukanMacEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.preedit.text.as_bytes().len() as c_uint
}

/// Get the preedit caret position in **characters** (not bytes).
/// macOS `setMarkedText` needs character offset for `selectedRange`.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_preedit_caret(engine: *const KarukanMacEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.preedit.caret_chars as c_uint
}

// --- Commit ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_has_commit(engine: *const KarukanMacEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.commit.dirty { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_commit(engine: *const KarukanMacEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.commit.text.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_commit_len(engine: *const KarukanMacEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.commit.text.as_bytes().len() as c_uint
}

// --- Candidates ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_has_candidates(engine: *const KarukanMacEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.candidates.dirty { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_should_hide_candidates(
    engine: *const KarukanMacEngine,
) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.candidates.hide { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_candidate_count(
    engine: *const KarukanMacEngine,
) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.candidates.count as c_uint
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_candidate(
    engine: *const KarukanMacEngine,
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

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_candidate_annotation(
    engine: *const KarukanMacEngine,
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

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_candidate_cursor(
    engine: *const KarukanMacEngine,
) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.candidates.cursor as c_uint
}

// --- Aux text ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_has_aux(engine: *const KarukanMacEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.aux.dirty { 1 } else { 0 }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_aux(engine: *const KarukanMacEngine) -> *const c_char {
    let engine = ffi_ref!(engine, ptr::null());
    engine.aux.text.as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_aux_len(engine: *const KarukanMacEngine) -> c_uint {
    let engine = ffi_ref!(engine, 0);
    engine.aux.text.as_bytes().len() as c_uint
}

// --- Timing ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_last_conversion_ms(
    engine: *const KarukanMacEngine,
) -> u64 {
    let engine = ffi_ref!(engine, 0);
    engine.last_conversion_ms
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_get_last_process_key_ms(
    engine: *const KarukanMacEngine,
) -> u64 {
    let engine = ffi_ref!(engine, 0);
    engine.last_process_key_ms
}

// --- Learning ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_save_learning(engine: *mut KarukanMacEngine) {
    let engine = ffi_mut!(engine);
    engine.engine.save_learning();
}

// --- State ---

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_is_empty(engine: *const KarukanMacEngine) -> c_int {
    let engine = ffi_ref!(engine, 0);
    if engine.engine.state().is_empty() {
        1
    } else {
        0
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_commit(engine: *mut KarukanMacEngine) -> c_int {
    let engine = ffi_mut!(engine, 0);
    let text = engine.engine.commit();

    if text.is_empty() {
        return 0;
    }

    engine.commit.text = CString::new(text).unwrap_or_default();
    engine.commit.dirty = true;
    1
}
