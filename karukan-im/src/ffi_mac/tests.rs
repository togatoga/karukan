use super::*;
use input::*;
use lifecycle::*;
use query::*;
use std::ffi::CStr;
use std::ptr;

// macOS Carbon virtual key codes
const KVK_A: u16 = 0x00;
const KVK_I: u16 = 0x22; // 'i' key
const KVK_K: u16 = 0x28; // 'k' key
const KVK_RETURN: u16 = 0x24;
const KVK_ESCAPE: u16 = 0x35;
const KVK_DELETE: u16 = 0x33; // backspace

// NSEvent.modifierFlags
const NS_SHIFT: u64 = 0x20000;

/// RAII wrapper around a raw `KarukanMacEngine` pointer.
struct TestEngine(*mut KarukanMacEngine);

impl TestEngine {
    fn new() -> Self {
        let ptr = karukan_mac_engine_new();
        assert!(!ptr.is_null());
        Self(ptr)
    }

    fn ptr(&self) -> *mut KarukanMacEngine {
        self.0
    }

    /// Send a key press event with character. Returns true if consumed.
    fn press(&self, keycode: u16, character: u32) -> bool {
        karukan_mac_process_key(self.0, keycode, character, 0, 1) == 1
    }

    /// Send a key press with modifiers. Returns true if consumed.
    fn press_with(&self, keycode: u16, character: u32, mods: u64) -> bool {
        karukan_mac_process_key(self.0, keycode, character, mods, 1) == 1
    }

    /// Send a key release event. Returns true if consumed.
    fn release(&self, keycode: u16, character: u32) -> bool {
        karukan_mac_process_key(self.0, keycode, character, 0, 0) == 1
    }

    fn preedit(&self) -> &str {
        let ptr = karukan_mac_engine_get_preedit(self.0);
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap()
    }

    fn preedit_len(&self) -> u32 {
        karukan_mac_engine_get_preedit_len(self.0)
    }

    fn preedit_caret(&self) -> u32 {
        karukan_mac_engine_get_preedit_caret(self.0)
    }

    fn commit_text(&self) -> &str {
        let ptr = karukan_mac_engine_get_commit(self.0);
        if ptr.is_null() {
            return "";
        }
        unsafe { CStr::from_ptr(ptr) }.to_str().unwrap()
    }

    fn has_preedit(&self) -> bool {
        karukan_mac_engine_has_preedit(self.0) == 1
    }

    fn has_commit(&self) -> bool {
        karukan_mac_engine_has_commit(self.0) == 1
    }

    fn has_candidates(&self) -> bool {
        karukan_mac_engine_has_candidates(self.0) == 1
    }
}

impl Drop for TestEngine {
    fn drop(&mut self) {
        karukan_mac_engine_free(self.0);
    }
}

#[test]
fn test_engine_lifecycle() {
    let _engine = TestEngine::new();
}

#[test]
fn test_null_engine_safety() {
    assert_eq!(
        karukan_mac_process_key(ptr::null_mut(), 0, 0x61, 0, 1),
        0
    );
    assert_eq!(karukan_mac_engine_has_preedit(ptr::null()), 0);
    assert!(karukan_mac_engine_get_preedit(ptr::null()).is_null());
    assert_eq!(karukan_mac_engine_get_preedit_len(ptr::null()), 0);
    assert_eq!(karukan_mac_engine_has_commit(ptr::null()), 0);
    assert!(karukan_mac_engine_get_commit(ptr::null()).is_null());
    assert_eq!(karukan_mac_engine_has_candidates(ptr::null()), 0);
    assert_eq!(karukan_mac_engine_get_candidate_count(ptr::null()), 0);
    assert_eq!(karukan_mac_engine_get_last_conversion_ms(ptr::null()), 0);
    karukan_mac_engine_reset(ptr::null_mut());
    karukan_mac_engine_free(ptr::null_mut());
}

#[test]
fn test_basic_input() {
    let e = TestEngine::new();

    // Type 'a' (keycode 0x00, character 'a' = 0x61)
    assert!(e.press(KVK_A, 0x61));
    assert!(e.has_preedit());
    assert_eq!(e.preedit(), "あ");
    assert_eq!(e.preedit_len(), 3); // "あ" is 3 bytes in UTF-8
}

#[test]
fn test_romaji_to_hiragana() {
    let e = TestEngine::new();

    // Type 'k' (keycode 0x28, character 'k' = 0x6b)
    e.press(KVK_K, 0x6b);
    assert_eq!(e.preedit(), "k");

    // Type 'a' (keycode 0x00, character 'a' = 0x61)
    e.press(KVK_A, 0x61);
    assert_eq!(e.preedit(), "か");
}

#[test]
fn test_commit_composing() {
    let e = TestEngine::new();

    // Type "ai" -> "あい"
    e.press(KVK_A, 0x61);
    e.press(KVK_I, 0x69);
    assert_eq!(e.preedit(), "あい");

    // Press Return to commit
    e.press(KVK_RETURN, 0);
    assert!(e.has_commit());
    assert_eq!(e.commit_text(), "あい");
}

#[test]
fn test_backspace() {
    let e = TestEngine::new();

    e.press(KVK_A, 0x61);
    e.press(KVK_I, 0x69);

    // Backspace
    e.press(KVK_DELETE, 0);
    assert_eq!(e.preedit(), "あ");

    // Backspace again -> empty
    e.press(KVK_DELETE, 0);
    assert_eq!(e.preedit_len(), 0);
}

#[test]
fn test_escape_cancel() {
    let e = TestEngine::new();

    e.press(KVK_A, 0x61);
    e.press(KVK_I, 0x69);

    e.press(KVK_ESCAPE, 0);
    assert_eq!(e.preedit_len(), 0);
    assert!(!e.has_commit());
}

#[test]
fn test_reset() {
    let e = TestEngine::new();

    e.press(KVK_A, 0x61);
    assert!(e.preedit_len() > 0);

    karukan_mac_engine_reset(e.ptr());

    assert_eq!(e.preedit_len(), 0);
    assert!(!e.has_preedit());
    assert!(!e.has_commit());
    assert!(!e.has_candidates());
}

#[test]
fn test_key_release_ignored() {
    let e = TestEngine::new();
    assert!(!e.release(KVK_A, 0x61));
}

#[test]
fn test_preedit_caret_is_character_offset() {
    let e = TestEngine::new();

    // Type "あい" (2 characters, 6 bytes)
    e.press(KVK_A, 0x61);
    e.press(KVK_I, 0x69);
    assert_eq!(e.preedit(), "あい");

    // Caret should be at character position 2 (not byte position 6)
    assert_eq!(e.preedit_caret(), 2);
}

#[test]
fn test_shift_a_produces_uppercase() {
    let e = TestEngine::new();

    // Shift_L press (keycode 0x38)
    e.press(0x38, 0);

    // 'A' with Shift modifier
    assert!(e.press_with(KVK_A, 0x41, NS_SHIFT));

    assert!(e.has_preedit());
    assert_eq!(e.preedit(), "A");
}

#[test]
fn test_surrounding_text_null_safety() {
    let e = TestEngine::new();
    karukan_mac_engine_set_surrounding_text(ptr::null_mut(), ptr::null(), 0);
    karukan_mac_engine_set_surrounding_text(e.ptr(), ptr::null(), 0);
}
