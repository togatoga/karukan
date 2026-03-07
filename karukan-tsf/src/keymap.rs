//! Windows Virtual Key → karukan Keysym conversion.
//!
//! Maps Windows VK codes and modifier state to karukan's internal KeyEvent type.
//! This module is the only Windows-specific key handling code; the rest of the
//! engine uses platform-independent Keysym values.

use karukan_im::{KeyEvent, KeyModifiers, Keysym};

/// Convert a Windows Virtual Key code to a karukan Keysym.
///
/// For printable characters, `unicode_char` (obtained from `ToUnicode`/`MapVirtualKeyW`)
/// should be provided to get the correct casing.
pub fn vk_to_keysym(vk: u32, unicode_char: Option<char>) -> Keysym {
    // If we have a Unicode character from ToUnicode, use it directly for printable chars
    if let Some(ch) = unicode_char {
        let code = ch as u32;
        if (0x0020..=0x007e).contains(&code) {
            return Keysym(code);
        }
    }

    match vk {
        // Control keys
        0x08 => Keysym::BACKSPACE, // VK_BACK
        0x09 => Keysym::TAB,       // VK_TAB
        0x0D => Keysym::RETURN,    // VK_RETURN
        0x1B => Keysym::ESCAPE,    // VK_ESCAPE
        0x20 => Keysym::SPACE,     // VK_SPACE
        0x2E => Keysym::DELETE,    // VK_DELETE

        // Cursor movement
        0x24 => Keysym::HOME,      // VK_HOME
        0x25 => Keysym::LEFT,      // VK_LEFT
        0x26 => Keysym::UP,        // VK_UP
        0x27 => Keysym::RIGHT,     // VK_RIGHT
        0x28 => Keysym::DOWN,      // VK_DOWN
        0x21 => Keysym::PAGE_UP,   // VK_PRIOR
        0x22 => Keysym::PAGE_DOWN, // VK_NEXT
        0x23 => Keysym::END,       // VK_END

        // Function keys
        0x70 => Keysym::F1, // VK_F1
        0x71 => Keysym::F2,
        0x72 => Keysym::F3,
        0x73 => Keysym::F4,
        0x74 => Keysym::F5,
        0x75 => Keysym::F6,
        0x76 => Keysym::F7,
        0x77 => Keysym::F8,
        0x78 => Keysym::F9,
        0x79 => Keysym::F10,
        0x7A => Keysym::F11,
        0x7B => Keysym::F12, // VK_F12

        // Modifier keys
        0xA0 => Keysym::SHIFT_L,   // VK_LSHIFT
        0xA1 => Keysym::SHIFT_R,   // VK_RSHIFT
        0xA2 => Keysym::CONTROL_L, // VK_LCONTROL
        0xA3 => Keysym::CONTROL_R, // VK_RCONTROL
        0xA4 => Keysym::ALT_L,     // VK_LMENU
        0xA5 => Keysym::ALT_R,     // VK_RMENU
        0x5B => Keysym::SUPER_L,   // VK_LWIN
        0x5C => Keysym::SUPER_R,   // VK_RWIN

        // Generic shift/control/alt (when L/R distinction unavailable)
        0x10 => Keysym::SHIFT_L,   // VK_SHIFT
        0x11 => Keysym::CONTROL_L, // VK_CONTROL
        0x12 => Keysym::ALT_L,     // VK_MENU

        // Alphanumeric fallback (when unicode_char is not available)
        // VK codes for 'A'-'Z' are 0x41-0x5A, map to lowercase keysym
        vk @ 0x41..=0x5A => Keysym(vk - 0x41 + 0x61), // 'a'-'z'
        // VK codes for '0'-'9' are 0x30-0x39
        vk @ 0x30..=0x39 => Keysym(vk),

        // IME-specific keys
        0x15 => Keysym(0xff2a), // VK_KANJI / Hankaku/Zenkaku — mapped to Kanji keysym
        0x19 => Keysym(0xff23), // VK_KANJI (alternate) — Kana keysym
        0x1C => Keysym::HENKAN, // VK_CONVERT — 変換キー
        0x1D => Keysym::MUHENKAN, // VK_NONCONVERT — 無変換キー

        // Unknown key
        _ => Keysym(0),
    }
}

/// Build `KeyModifiers` from Windows modifier key state.
///
/// On Windows, modifier state is typically obtained from `GetKeyState()`.
/// - `shift`: true if VK_SHIFT is pressed
/// - `control`: true if VK_CONTROL is pressed
/// - `alt`: true if VK_MENU is pressed
/// - `win`: true if VK_LWIN or VK_RWIN is pressed
pub fn build_modifiers(shift: bool, control: bool, alt: bool, win: bool) -> KeyModifiers {
    KeyModifiers {
        shift_key: shift,
        control_key: control,
        alt_key: alt,
        super_key: win,
    }
}

/// Create a karukan `KeyEvent` from Windows key event parameters.
///
/// - `vk`: Windows Virtual Key code
/// - `unicode_char`: character from `ToUnicode` (None if non-printable)
/// - `shift`, `control`, `alt`, `win`: modifier key states
/// - `is_press`: true for key down, false for key up
pub fn create_key_event(
    vk: u32,
    unicode_char: Option<char>,
    shift: bool,
    control: bool,
    alt: bool,
    win: bool,
    is_press: bool,
) -> KeyEvent {
    let keysym = vk_to_keysym(vk, unicode_char);
    let modifiers = build_modifiers(shift, control, alt, win);
    KeyEvent::new(keysym, modifiers, is_press)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vk_to_keysym_control_keys() {
        assert_eq!(vk_to_keysym(0x08, None), Keysym::BACKSPACE);
        assert_eq!(vk_to_keysym(0x09, None), Keysym::TAB);
        assert_eq!(vk_to_keysym(0x0D, None), Keysym::RETURN);
        assert_eq!(vk_to_keysym(0x1B, None), Keysym::ESCAPE);
        assert_eq!(vk_to_keysym(0x20, None), Keysym::SPACE);
        assert_eq!(vk_to_keysym(0x2E, None), Keysym::DELETE);
    }

    #[test]
    fn test_vk_to_keysym_cursor_keys() {
        assert_eq!(vk_to_keysym(0x24, None), Keysym::HOME);
        assert_eq!(vk_to_keysym(0x25, None), Keysym::LEFT);
        assert_eq!(vk_to_keysym(0x26, None), Keysym::UP);
        assert_eq!(vk_to_keysym(0x27, None), Keysym::RIGHT);
        assert_eq!(vk_to_keysym(0x28, None), Keysym::DOWN);
        assert_eq!(vk_to_keysym(0x21, None), Keysym::PAGE_UP);
        assert_eq!(vk_to_keysym(0x22, None), Keysym::PAGE_DOWN);
        assert_eq!(vk_to_keysym(0x23, None), Keysym::END);
    }

    #[test]
    fn test_vk_to_keysym_function_keys() {
        assert_eq!(vk_to_keysym(0x70, None), Keysym::F1);
        assert_eq!(vk_to_keysym(0x7B, None), Keysym::F12);
    }

    #[test]
    fn test_vk_to_keysym_alpha_fallback() {
        // Without unicode_char, VK 'A' (0x41) maps to lowercase 'a'
        assert_eq!(vk_to_keysym(0x41, None), Keysym(0x0061)); // 'a'
        assert_eq!(vk_to_keysym(0x5A, None), Keysym(0x007a)); // 'z'
    }

    #[test]
    fn test_vk_to_keysym_unicode_override() {
        // With unicode_char, the char takes precedence
        assert_eq!(vk_to_keysym(0x41, Some('A')), Keysym(0x0041)); // uppercase 'A'
        assert_eq!(vk_to_keysym(0x41, Some('a')), Keysym(0x0061)); // lowercase 'a'
    }

    #[test]
    fn test_vk_to_keysym_numbers() {
        assert_eq!(vk_to_keysym(0x30, None), Keysym(0x0030)); // '0'
        assert_eq!(vk_to_keysym(0x39, None), Keysym(0x0039)); // '9'
    }

    #[test]
    fn test_vk_to_keysym_modifiers() {
        assert_eq!(vk_to_keysym(0xA0, None), Keysym::SHIFT_L);
        assert_eq!(vk_to_keysym(0xA1, None), Keysym::SHIFT_R);
        assert_eq!(vk_to_keysym(0xA2, None), Keysym::CONTROL_L);
        assert_eq!(vk_to_keysym(0xA3, None), Keysym::CONTROL_R);
        assert_eq!(vk_to_keysym(0xA4, None), Keysym::ALT_L);
        assert_eq!(vk_to_keysym(0xA5, None), Keysym::ALT_R);
    }

    #[test]
    fn test_build_modifiers() {
        let mods = build_modifiers(true, false, true, false);
        assert!(mods.shift_key);
        assert!(!mods.control_key);
        assert!(mods.alt_key);
        assert!(!mods.super_key);
    }

    #[test]
    fn test_create_key_event() {
        let event = create_key_event(0x41, Some('a'), false, false, false, false, true);
        assert_eq!(event.keysym, Keysym(0x0061));
        assert!(event.is_press);
        assert!(!event.modifiers.shift_key);

        let event = create_key_event(0x41, Some('A'), true, false, false, false, true);
        assert_eq!(event.keysym, Keysym(0x0041));
        assert!(event.modifiers.shift_key);
    }

    #[test]
    fn test_printable_symbols() {
        // Symbols should be handled via unicode_char
        assert_eq!(vk_to_keysym(0xBB, Some('+')), Keysym(0x002b)); // VK_OEM_PLUS with Shift
        assert_eq!(vk_to_keysym(0xBB, Some('=')), Keysym(0x003d)); // VK_OEM_PLUS without Shift
        assert_eq!(vk_to_keysym(0xBA, Some(';')), Keysym(0x003b)); // VK_OEM_1 (semicolon)
    }

    #[test]
    fn test_unknown_vk() {
        assert_eq!(vk_to_keysym(0xFF, None), Keysym(0));
    }

    #[test]
    fn test_vk_to_keysym_japanese_keys() {
        assert_eq!(vk_to_keysym(0x1C, None), Keysym::HENKAN); // VK_CONVERT
        assert_eq!(vk_to_keysym(0x1D, None), Keysym::MUHENKAN); // VK_NONCONVERT
    }
}
