use super::*;
use input::*;
use lifecycle::*;
use query::*;
use std::ffi::CStr;
use std::ptr;

// XKB keysyms for common keys
const XKB_KEY_A: u32 = 0x61;
const XKB_KEY_I: u32 = 0x69;
const XKB_KEY_K: u32 = 0x6b;
const XKB_KEY_RETURN: u32 = 0xff0d;
const XKB_KEY_ESCAPE: u32 = 0xff1b;
const XKB_KEY_BACKSPACE: u32 = 0xff08;
const XKB_KEY_SHIFT_L: u32 = 0xffe1;
const SHIFT_MASK: u32 = crate::core::keycode::KeyModifiers::SHIFT_MASK;

/// RAII wrapper around a raw `KarukanEngine` pointer.
/// Automatically frees the engine on drop, preventing leaks in tests.
struct TestEngine(*mut KarukanEngine);

impl TestEngine {
    fn new() -> Self {
        let ptr = karukan_engine_new();
        assert!(!ptr.is_null());
        Self(ptr)
    }

    fn ptr(&self) -> *mut KarukanEngine {
        self.0
    }

    /// Send a key press event. Returns true if consumed.
    fn press(&self, keysym: u32) -> bool {
        karukan_engine_process_key(self.0, keysym, 0, 0) == 1
    }

    /// Send a key press event with modifier state. Returns true if consumed.
    fn press_with(&self, keysym: u32, state: u32) -> bool {
        karukan_engine_process_key(self.0, keysym, state, 0) == 1
    }

    /// Send a key release event. Returns true if consumed.
    fn release(&self, keysym: u32) -> bool {
        karukan_engine_process_key(self.0, keysym, 0, 1) == 1
    }

    /// Get the current preedit text as a &str.
    fn preedit(&self) -> &str {
        let ptr = karukan_engine_get_preedit(self.0);
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap()
    }

    /// Get the preedit length in bytes.
    fn preedit_len(&self) -> u32 {
        karukan_engine_get_preedit_len(self.0)
    }

    /// Get the commit text as a &str.
    fn commit_text(&self) -> &str {
        let ptr = karukan_engine_get_commit(self.0);
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap()
    }

    /// Get the aux text as a &str.
    fn aux(&self) -> &str {
        let ptr = karukan_engine_get_aux(self.0);
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap()
    }

    fn has_preedit(&self) -> bool {
        karukan_engine_has_preedit(self.0) == 1
    }

    fn has_commit(&self) -> bool {
        karukan_engine_has_commit(self.0) == 1
    }

    fn has_candidates(&self) -> bool {
        karukan_engine_has_candidates(self.0) == 1
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        karukan_engine_free(self.0);
    }
}

#[test]
fn test_engine_lifecycle() {
    let _engine = TestEngine::new();
}

#[test]
fn test_null_engine_safety() {
    // All functions should handle null safely
    assert_eq!(
        karukan_engine_process_key(ptr::null_mut(), XKB_KEY_A, 0, 0),
        0
    );
    assert_eq!(karukan_engine_has_preedit(ptr::null()), 0);
    assert!(karukan_engine_get_preedit(ptr::null()).is_null());
    assert_eq!(karukan_engine_get_preedit_len(ptr::null()), 0);
    assert_eq!(karukan_engine_has_commit(ptr::null()), 0);
    assert!(karukan_engine_get_commit(ptr::null()).is_null());
    assert_eq!(karukan_engine_has_candidates(ptr::null()), 0);
    assert_eq!(karukan_engine_get_candidate_count(ptr::null()), 0);
    assert_eq!(karukan_engine_get_last_conversion_ms(ptr::null()), 0);
    karukan_engine_reset(ptr::null_mut());
    karukan_engine_free(ptr::null_mut());
}

#[test]
fn test_basic_input() {
    let e = TestEngine::new();

    // Type 'a' -> should produce "あ"
    assert!(e.press(XKB_KEY_A));
    assert!(e.has_preedit());
    assert_eq!(e.preedit(), "あ");
    assert_eq!(e.preedit_len(), 3); // "あ" is 3 bytes in UTF-8
}

#[test]
fn test_romaji_to_hiragana() {
    let e = TestEngine::new();

    // Type 'k' -> should show "k" in preedit
    e.press(XKB_KEY_K);
    assert_eq!(e.preedit(), "k");

    // Type 'a' -> should become "か"
    e.press(XKB_KEY_A);
    assert_eq!(e.preedit(), "か");
}

#[test]
fn test_commit_composing() {
    let e = TestEngine::new();

    // Type "ai" -> "あい"
    e.press(XKB_KEY_A);
    e.press(XKB_KEY_I);
    assert_eq!(e.preedit(), "あい");

    // Press Enter to commit
    e.press(XKB_KEY_RETURN);
    assert!(e.has_commit());
    assert_eq!(e.commit_text(), "あい");
}

#[test]
fn test_backspace() {
    let e = TestEngine::new();

    // Type "ai" -> "あい"
    e.press(XKB_KEY_A);
    e.press(XKB_KEY_I);

    // Backspace -> "あ"
    e.press(XKB_KEY_BACKSPACE);
    assert_eq!(e.preedit(), "あ");

    // Backspace again -> empty
    e.press(XKB_KEY_BACKSPACE);
    assert_eq!(e.preedit_len(), 0);
}

#[test]
fn test_escape_cancel() {
    let e = TestEngine::new();

    // Type "ai"
    e.press(XKB_KEY_A);
    e.press(XKB_KEY_I);

    // Escape to cancel
    e.press(XKB_KEY_ESCAPE);
    assert_eq!(e.preedit_len(), 0);
    assert!(!e.has_commit());
}

#[test]
fn test_reset() {
    let e = TestEngine::new();

    // Type something
    e.press(XKB_KEY_A);
    assert!(e.preedit_len() > 0);

    // Reset
    karukan_engine_reset(e.ptr());

    // Everything should be cleared
    assert_eq!(e.preedit_len(), 0);
    assert!(!e.has_preedit());
    assert!(!e.has_commit());
    assert!(!e.has_candidates());
}

#[test]
fn test_key_release_ignored() {
    let e = TestEngine::new();
    assert!(!e.release(XKB_KEY_A));
}

#[test]
fn test_cstring_null_termination() {
    let e = TestEngine::new();
    e.press(XKB_KEY_A);

    // Verify the pointer is valid and null-terminated
    let preedit_ptr = karukan_engine_get_preedit(e.ptr());
    assert!(!preedit_ptr.is_null());

    // This should not crash - CStr::from_ptr requires null termination
    let preedit = unsafe { CStr::from_ptr(preedit_ptr) };
    assert!(!preedit.to_str().unwrap().is_empty());
}

#[test]
fn test_conversion_timing() {
    let e = TestEngine::new();

    // Initially, conversion time should be 0
    assert_eq!(karukan_engine_get_last_conversion_ms(e.ptr()), 0);

    // Type something - auto-suggest might trigger conversion after 2+ chars
    e.press(XKB_KEY_A);
    e.press(XKB_KEY_I);

    // Note: actual conversion timing depends on whether kanji converter is initialized
    // Just verify the getter works and returns a reasonable value (0 or positive)
    let timing = karukan_engine_get_last_conversion_ms(e.ptr());
    assert!(timing < 60000); // Should be less than 60 seconds
}

#[test]
fn test_surrounding_text_sets_context() {
    let e = TestEngine::new();

    // Set surrounding text (simulating editor content)
    // Simulate GTK/Qt: cursor_pos as character offset (10 chars, 30 bytes)
    let text = std::ffi::CString::new("今日は良い天気です。").unwrap();
    let cursor_pos = "今日は良い天気です。".chars().count() as u32;

    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), cursor_pos);

    // Surrounding text was set, so context is displayed
    e.press(XKB_KEY_A);

    assert!(
        e.aux().contains("Karukan"),
        "Aux should contain mode indicator: {}",
        e.aux()
    );
}

#[test]
fn test_surrounding_text_null_safety() {
    let e = TestEngine::new();

    // Should not crash with null engine
    karukan_engine_set_surrounding_text(ptr::null_mut(), ptr::null(), 0);

    // Should not crash with null text
    karukan_engine_set_surrounding_text(e.ptr(), ptr::null(), 0);
}

#[test]
fn test_surrounding_text_cursor_at_start() {
    let e = TestEngine::new();

    // Cursor at start (no left context, only right context)
    let text = std::ffi::CString::new("Hello World").unwrap();
    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), 0);

    e.press(XKB_KEY_A);

    // Cursor at start: no left context, right context exists
    assert!(
        !e.aux().contains("lctx:"),
        "Should not contain left context: {}",
        e.aux()
    );
    assert!(
        e.aux().contains("rctx:"),
        "Should contain right context: {}",
        e.aux()
    );
}

#[test]
fn test_surrounding_text_with_both_contexts() {
    let e = TestEngine::new();

    // Cursor in the middle: "左側|右側"
    // Simulate GTK/Qt: character offset (2 chars, 6 bytes)
    let text = std::ffi::CString::new("左側右側").unwrap();
    let cursor_pos = "左側".chars().count() as u32;

    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), cursor_pos);

    e.press(XKB_KEY_A);

    // Both left and right context should be displayed
    assert!(
        e.aux().contains("lctx:"),
        "Should contain left context: {}",
        e.aux()
    );
    assert!(
        e.aux().contains("左側"),
        "Left context should contain '左側': {}",
        e.aux()
    );
}

#[test]
fn test_surrounding_text_cursor_at_end() {
    let e = TestEngine::new();

    // Cursor at end (only left context)
    // Simulate GTK/Qt: character offset (4 chars, 12 bytes)
    let text = std::ffi::CString::new("全部左側").unwrap();
    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), "全部左側".chars().count() as u32);

    e.press(XKB_KEY_A);

    // Cursor at end: left context exists, no right context
    assert!(
        e.aux().contains("lctx:"),
        "Should contain left context: {}",
        e.aux()
    );
    assert!(
        e.aux().contains("全部左側"),
        "Left context should contain '全部左側': {}",
        e.aux()
    );
    assert!(
        !e.aux().contains("rctx:"),
        "Should not contain right context: {}",
        e.aux()
    );
}

#[test]
fn test_surrounding_text_char_offset_japanese() {
    let e = TestEngine::new();

    // cursor_pos is always a character (code point) offset from fcitx5.
    // "あいうえお" = 15 bytes, 5 chars. cursor_pos=5 → end of text.
    let text = std::ffi::CString::new("あいうえお").unwrap();
    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), 5);

    e.press(XKB_KEY_A);

    // Cursor at end: all text is left context
    assert!(
        e.aux().contains("lctx:"),
        "Should contain left context: {}",
        e.aux()
    );
    assert!(
        e.aux().contains("あいうえお"),
        "Left context should contain full text: {}",
        e.aux()
    );
    assert!(
        !e.aux().contains("rctx:"),
        "Should not contain right context: {}",
        e.aux()
    );
}

#[test]
fn test_surrounding_text_char_offset_middle() {
    let e = TestEngine::new();

    // "あいうえお" = 5 chars. cursor_pos=2 → after "あい"
    let text = std::ffi::CString::new("あいうえお").unwrap();
    karukan_engine_set_surrounding_text(e.ptr(), text.as_ptr(), 2);

    e.press(XKB_KEY_A);

    assert!(
        e.aux().contains("lctx:"),
        "Should contain left context: {}",
        e.aux()
    );
    assert!(
        e.aux().contains("あい"),
        "Left context should contain 'あい': {}",
        e.aux()
    );
    assert!(
        e.aux().contains("rctx:"),
        "Should contain right context: {}",
        e.aux()
    );
}

// --- Shift+letter alphabet mode FFI tests ---

#[test]
fn test_ffi_shift_a_produces_uppercase_a() {
    let e = TestEngine::new();

    // Shift_L press
    e.press(XKB_KEY_SHIFT_L);

    // 'a' press with ShiftMask (fcitx5 sends lowercase keysym + shift state)
    assert!(e.press_with(XKB_KEY_A, SHIFT_MASK));

    // Preedit should be uppercase "A", not "あ"
    assert!(e.has_preedit());
    assert_eq!(
        e.preedit(),
        "A",
        "Shift+A should produce 'A' in preedit, not hiragana"
    );
}

#[test]
fn test_ffi_shift_a_after_hiragana() {
    let e = TestEngine::new();

    // Type "あ"
    e.press(XKB_KEY_A);
    assert_eq!(e.preedit(), "あ");

    // Shift_L press
    e.press(XKB_KEY_SHIFT_L);

    // 'a' with ShiftMask → should enter alphabet mode and add 'A'
    e.press_with(XKB_KEY_A, SHIFT_MASK);
    assert_eq!(
        e.preedit(),
        "あA",
        "Shift+A after hiragana should append 'A'"
    );
}

#[test]
fn test_ffi_uppercase_keysym_without_shift_flag() {
    // fcitx5 may send uppercase keysym 'A' (0x41) without the shift flag
    // when Shift is consumed during keysym resolution.
    let e = TestEngine::new();

    // Send uppercase 'A' keysym (0x41) without shift modifier
    const XKB_KEY_A_UPPER: u32 = 0x41;
    assert!(e.press(XKB_KEY_A_UPPER));

    assert!(e.has_preedit());
    assert_eq!(
        e.preedit(),
        "A",
        "Uppercase keysym without shift flag should produce 'A', not hiragana"
    );
}

#[test]
fn test_ffi_standalone_shift_does_not_toggle_mode() {
    let e = TestEngine::new();

    // Shift_L press
    e.press(XKB_KEY_SHIFT_L);

    // Shift_L release → standalone Shift should NOT toggle mode
    e.release(XKB_KEY_SHIFT_L);

    // Now type 'a' without shift → should be 'あ' (still in hiragana mode)
    e.press(XKB_KEY_A);
    assert_eq!(
        e.preedit(),
        "あ",
        "After standalone Shift, 'a' should still produce hiragana"
    );
}
