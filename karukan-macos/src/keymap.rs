//! macOS Carbon key code → karukan Keysym conversion.
//!
//! Maps macOS Carbon key codes and modifier state to karukan's internal KeyEvent type.
//! This module is the only macOS-specific key handling code; the rest of the
//! engine uses platform-independent Keysym values.

use karukan_im::{KeyEvent, KeyModifiers, Keysym};

/// Convert a macOS Carbon key code to a karukan Keysym.
///
/// For printable characters, `unicode_char` (obtained from `NSEvent.characters`)
/// should be provided to get the correct casing and layout-aware mapping.
pub fn carbon_keycode_to_keysym(keycode: u16, unicode_char: Option<char>) -> Keysym {
    // If we have a Unicode character from NSEvent.characters, use it directly for printable chars
    if let Some(ch) = unicode_char {
        let code = ch as u32;
        if (0x0020..=0x007e).contains(&code) {
            return Keysym(code);
        }
    }

    match keycode {
        // Control keys
        0x33 => Keysym::BACKSPACE, // kVK_Delete (backspace)
        0x30 => Keysym::TAB,       // kVK_Tab
        0x24 => Keysym::RETURN,    // kVK_Return
        0x35 => Keysym::ESCAPE,    // kVK_Escape
        0x31 => Keysym::SPACE,     // kVK_Space
        0x75 => Keysym::DELETE,    // kVK_ForwardDelete

        // Cursor movement
        0x73 => Keysym::HOME,      // kVK_Home
        0x7B => Keysym::LEFT,      // kVK_LeftArrow
        0x7E => Keysym::UP,        // kVK_UpArrow
        0x7C => Keysym::RIGHT,     // kVK_RightArrow
        0x7D => Keysym::DOWN,      // kVK_DownArrow
        0x74 => Keysym::PAGE_UP,   // kVK_PageUp
        0x79 => Keysym::PAGE_DOWN, // kVK_PageDown
        0x77 => Keysym::END,       // kVK_End

        // Function keys
        0x7A => Keysym::F1,  // kVK_F1
        0x78 => Keysym::F2,  // kVK_F2
        0x63 => Keysym::F3,  // kVK_F3
        0x76 => Keysym::F4,  // kVK_F4
        0x60 => Keysym::F5,  // kVK_F5
        0x61 => Keysym::F6,  // kVK_F6
        0x62 => Keysym::F7,  // kVK_F7
        0x64 => Keysym::F8,  // kVK_F8
        0x65 => Keysym::F9,  // kVK_F9
        0x6D => Keysym::F10, // kVK_F10
        0x67 => Keysym::F11, // kVK_F11
        0x6F => Keysym::F12, // kVK_F12

        // Modifier keys
        0x38 => Keysym::SHIFT_L,   // kVK_Shift
        0x3C => Keysym::SHIFT_R,   // kVK_RightShift
        0x3B => Keysym::CONTROL_L, // kVK_Control
        0x3E => Keysym::CONTROL_R, // kVK_RightControl
        0x3A => Keysym::ALT_L,     // kVK_Option
        0x3D => Keysym::ALT_R,     // kVK_RightOption
        0x37 => Keysym::SUPER_L,   // kVK_Command
        0x36 => Keysym::SUPER_R,   // kVK_RightCommand

        // Japanese IME keys (JIS keyboard)
        0x66 => Keysym::MUHENKAN, // kVK_JIS_Eisu (英数) → 無変換
        0x68 => Keysym::HENKAN,   // kVK_JIS_Kana (かな) → 変換

        // ANSI key fallback (when unicode_char is not available)
        // Map ANSI key codes to lowercase ASCII
        0x00 => Keysym(0x0061), // kVK_ANSI_A → 'a'
        0x0B => Keysym(0x0062), // kVK_ANSI_B → 'b'
        0x08 => Keysym(0x0063), // kVK_ANSI_C → 'c'
        0x02 => Keysym(0x0064), // kVK_ANSI_D → 'd'
        0x0E => Keysym(0x0065), // kVK_ANSI_E → 'e'
        0x03 => Keysym(0x0066), // kVK_ANSI_F → 'f'
        0x05 => Keysym(0x0067), // kVK_ANSI_G → 'g'
        0x04 => Keysym(0x0068), // kVK_ANSI_H → 'h'
        0x22 => Keysym(0x0069), // kVK_ANSI_I → 'i'
        0x26 => Keysym(0x006A), // kVK_ANSI_J → 'j'
        0x28 => Keysym(0x006B), // kVK_ANSI_K → 'k'
        0x25 => Keysym(0x006C), // kVK_ANSI_L → 'l'
        0x2E => Keysym(0x006D), // kVK_ANSI_M → 'm'
        0x2D => Keysym(0x006E), // kVK_ANSI_N → 'n'
        0x1F => Keysym(0x006F), // kVK_ANSI_O → 'o'
        0x23 => Keysym(0x0070), // kVK_ANSI_P → 'p'
        0x0C => Keysym(0x0071), // kVK_ANSI_Q → 'q'
        0x0F => Keysym(0x0072), // kVK_ANSI_R → 'r'
        0x01 => Keysym(0x0073), // kVK_ANSI_S → 's'
        0x11 => Keysym(0x0074), // kVK_ANSI_T → 't'
        0x20 => Keysym(0x0075), // kVK_ANSI_U → 'u'
        0x09 => Keysym(0x0076), // kVK_ANSI_V → 'v'
        0x0D => Keysym(0x0077), // kVK_ANSI_W → 'w'
        0x07 => Keysym(0x0078), // kVK_ANSI_X → 'x'
        0x10 => Keysym(0x0079), // kVK_ANSI_Y → 'y'
        0x06 => Keysym(0x007A), // kVK_ANSI_Z → 'z'

        // Number keys fallback
        0x1D => Keysym(0x0030), // kVK_ANSI_0 → '0'
        0x12 => Keysym(0x0031), // kVK_ANSI_1 → '1'
        0x13 => Keysym(0x0032), // kVK_ANSI_2 → '2'
        0x14 => Keysym(0x0033), // kVK_ANSI_3 → '3'
        0x15 => Keysym(0x0034), // kVK_ANSI_4 → '4'
        0x17 => Keysym(0x0035), // kVK_ANSI_5 → '5'
        0x16 => Keysym(0x0036), // kVK_ANSI_6 → '6'
        0x1A => Keysym(0x0037), // kVK_ANSI_7 → '7'
        0x1C => Keysym(0x0038), // kVK_ANSI_8 → '8'
        0x19 => Keysym(0x0039), // kVK_ANSI_9 → '9'

        // Unknown key
        _ => Keysym(0),
    }
}

/// Build `KeyModifiers` from macOS modifier key state.
///
/// - `shift`: true if Shift is pressed
/// - `control`: true if Control is pressed
/// - `option`: true if Option (Alt) is pressed
/// - `command`: true if Command is pressed
pub fn build_modifiers(shift: bool, control: bool, option: bool, command: bool) -> KeyModifiers {
    KeyModifiers {
        shift_key: shift,
        control_key: control,
        alt_key: option,
        super_key: command,
    }
}

/// Create a karukan `KeyEvent` from macOS key event parameters.
///
/// - `keycode`: macOS Carbon key code (from `NSEvent.keyCode`)
/// - `unicode_char`: character from `NSEvent.characters` (None if non-printable)
/// - `shift`, `control`, `option`, `command`: modifier key states
/// - `is_press`: true for key down, false for key up
pub fn create_key_event(
    keycode: u16,
    unicode_char: Option<char>,
    shift: bool,
    control: bool,
    option: bool,
    command: bool,
    is_press: bool,
) -> KeyEvent {
    let keysym = carbon_keycode_to_keysym(keycode, unicode_char);
    let modifiers = build_modifiers(shift, control, option, command);
    KeyEvent::new(keysym, modifiers, is_press)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_carbon_keycode_control_keys() {
        assert_eq!(carbon_keycode_to_keysym(0x33, None), Keysym::BACKSPACE);
        assert_eq!(carbon_keycode_to_keysym(0x30, None), Keysym::TAB);
        assert_eq!(carbon_keycode_to_keysym(0x24, None), Keysym::RETURN);
        assert_eq!(carbon_keycode_to_keysym(0x35, None), Keysym::ESCAPE);
        assert_eq!(carbon_keycode_to_keysym(0x31, None), Keysym::SPACE);
        assert_eq!(carbon_keycode_to_keysym(0x75, None), Keysym::DELETE);
    }

    #[test]
    fn test_carbon_keycode_cursor_keys() {
        assert_eq!(carbon_keycode_to_keysym(0x73, None), Keysym::HOME);
        assert_eq!(carbon_keycode_to_keysym(0x7B, None), Keysym::LEFT);
        assert_eq!(carbon_keycode_to_keysym(0x7E, None), Keysym::UP);
        assert_eq!(carbon_keycode_to_keysym(0x7C, None), Keysym::RIGHT);
        assert_eq!(carbon_keycode_to_keysym(0x7D, None), Keysym::DOWN);
        assert_eq!(carbon_keycode_to_keysym(0x74, None), Keysym::PAGE_UP);
        assert_eq!(carbon_keycode_to_keysym(0x79, None), Keysym::PAGE_DOWN);
        assert_eq!(carbon_keycode_to_keysym(0x77, None), Keysym::END);
    }

    #[test]
    fn test_carbon_keycode_function_keys() {
        assert_eq!(carbon_keycode_to_keysym(0x7A, None), Keysym::F1);
        assert_eq!(carbon_keycode_to_keysym(0x6F, None), Keysym::F12);
    }

    #[test]
    fn test_carbon_keycode_alpha_fallback() {
        // Without unicode_char, ANSI key codes map to lowercase
        assert_eq!(carbon_keycode_to_keysym(0x00, None), Keysym(0x0061)); // 'a'
        assert_eq!(carbon_keycode_to_keysym(0x06, None), Keysym(0x007A)); // 'z'
    }

    #[test]
    fn test_carbon_keycode_unicode_override() {
        // With unicode_char, the char takes precedence
        assert_eq!(carbon_keycode_to_keysym(0x00, Some('A')), Keysym(0x0041)); // uppercase 'A'
        assert_eq!(carbon_keycode_to_keysym(0x00, Some('a')), Keysym(0x0061)); // lowercase 'a'
    }

    #[test]
    fn test_carbon_keycode_numbers() {
        assert_eq!(carbon_keycode_to_keysym(0x1D, None), Keysym(0x0030)); // '0'
        assert_eq!(carbon_keycode_to_keysym(0x19, None), Keysym(0x0039)); // '9'
    }

    #[test]
    fn test_carbon_keycode_modifiers() {
        assert_eq!(carbon_keycode_to_keysym(0x38, None), Keysym::SHIFT_L);
        assert_eq!(carbon_keycode_to_keysym(0x3C, None), Keysym::SHIFT_R);
        assert_eq!(carbon_keycode_to_keysym(0x3B, None), Keysym::CONTROL_L);
        assert_eq!(carbon_keycode_to_keysym(0x3E, None), Keysym::CONTROL_R);
        assert_eq!(carbon_keycode_to_keysym(0x3A, None), Keysym::ALT_L);
        assert_eq!(carbon_keycode_to_keysym(0x3D, None), Keysym::ALT_R);
        assert_eq!(carbon_keycode_to_keysym(0x37, None), Keysym::SUPER_L);
        assert_eq!(carbon_keycode_to_keysym(0x36, None), Keysym::SUPER_R);
    }

    #[test]
    fn test_carbon_keycode_japanese_keys() {
        assert_eq!(carbon_keycode_to_keysym(0x66, None), Keysym::MUHENKAN); // kVK_JIS_Eisu
        assert_eq!(carbon_keycode_to_keysym(0x68, None), Keysym::HENKAN); // kVK_JIS_Kana
    }

    #[test]
    fn test_build_modifiers() {
        let mods = build_modifiers(true, false, true, false);
        assert!(mods.shift_key);
        assert!(!mods.control_key);
        assert!(mods.alt_key); // option → alt
        assert!(!mods.super_key);
    }

    #[test]
    fn test_create_key_event() {
        let event = create_key_event(0x00, Some('a'), false, false, false, false, true);
        assert_eq!(event.keysym, Keysym(0x0061));
        assert!(event.is_press);
        assert!(!event.modifiers.shift_key);

        let event = create_key_event(0x00, Some('A'), true, false, false, false, true);
        assert_eq!(event.keysym, Keysym(0x0041));
        assert!(event.modifiers.shift_key);
    }

    #[test]
    fn test_printable_symbols() {
        // Symbols should be handled via unicode_char
        assert_eq!(carbon_keycode_to_keysym(0x18, Some('=')), Keysym(0x003d)); // kVK_ANSI_Equal
        assert_eq!(carbon_keycode_to_keysym(0x18, Some('+')), Keysym(0x002b)); // with Shift
        assert_eq!(carbon_keycode_to_keysym(0x29, Some(';')), Keysym(0x003b)); // kVK_ANSI_Semicolon
    }

    #[test]
    fn test_unknown_keycode() {
        assert_eq!(carbon_keycode_to_keysym(0xFF, None), Keysym(0));
    }
}
