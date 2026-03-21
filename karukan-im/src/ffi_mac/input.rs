#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::ffi::{c_char, c_int};

use crate::core::keymap_mac::macos_key_to_event;

use super::{KarukanMacEngine, ffi_mut};

/// Process a macOS key event.
///
/// # Parameters
/// - `engine`: Engine instance pointer
/// - `keycode`: `NSEvent.keyCode` (Carbon virtual key code)
/// - `character`: Unicode scalar from `NSEvent.characters` (0 if unavailable)
/// - `modifier_flags`: `NSEvent.modifierFlags.rawValue`
/// - `is_key_down`: 1 for key-down, 0 for key-up
///
/// Returns 1 if the key was consumed, 0 if not.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_process_key(
    engine: *mut KarukanMacEngine,
    keycode: u16,
    character: u32,
    modifier_flags: u64,
    is_key_down: c_int,
) -> c_int {
    let engine = ffi_mut!(engine, 0);
    engine.clear_flags();

    let is_press = is_key_down != 0;
    let key_event = match macos_key_to_event(keycode, character, modifier_flags, is_press) {
        Some(e) => e,
        None => return 0,
    };

    let result = engine.engine.process_key(&key_event);

    engine.apply_actions(result.actions);
    engine.sync_timing();

    if result.consumed { 1 } else { 0 }
}

/// Reset the engine state.
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_reset(engine: *mut KarukanMacEngine) {
    let engine = ffi_mut!(engine);
    engine.engine.reset();
    engine.preedit = super::PreeditCache::default();
    engine.candidates = super::CandidateCache::default();
    engine.commit = super::CommitCache::default();
    engine.aux = super::AuxCache::default();
}

/// Set the surrounding text context from the editor.
///
/// # Parameters
/// - `text`: Surrounding text (null-terminated UTF-8)
/// - `cursor_pos`: Cursor position in **characters** (macOS uses character offsets)
#[unsafe(no_mangle)]
pub extern "C" fn karukan_mac_engine_set_surrounding_text(
    engine: *mut KarukanMacEngine,
    text: *const c_char,
    cursor_pos: u32,
) {
    if text.is_null() {
        tracing::debug!("set_surrounding_text: text is null, skipping");
        return;
    }
    let engine = ffi_mut!(engine);
    let text_str = unsafe {
        match std::ffi::CStr::from_ptr(text).to_str() {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("set_surrounding_text: invalid UTF-8: {}", e);
                engine.engine.set_surrounding_context("", "");
                return;
            }
        }
    };

    // cursor_pos is a character offset (macOS convention)
    let char_offset = cursor_pos as usize;
    let byte_offset = text_str
        .char_indices()
        .nth(char_offset)
        .map(|(i, _)| i)
        .unwrap_or(text_str.len());

    let left_context = &text_str[..byte_offset];
    let right_context = &text_str[byte_offset..];
    tracing::debug!(
        "set_surrounding_text: left=\"{}\" right=\"{}\"",
        left_context,
        right_context
    );
    engine
        .engine
        .set_surrounding_context(left_context, right_context);
}
