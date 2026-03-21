//! macOS key mapping
//!
//! Maps macOS Carbon virtual key codes (kVK_*) and NSEvent.modifierFlags
//! to the Keysym / KeyModifiers / KeyEvent types used by InputMethodEngine.

use super::keycode::{KeyEvent, KeyModifiers, Keysym};

/// macOS NSEvent.modifierFlags bit masks
const NS_EVENT_MODIFIER_FLAG_SHIFT: u64 = 1 << 17; // 0x20000
const NS_EVENT_MODIFIER_FLAG_CONTROL: u64 = 1 << 18; // 0x40000
const NS_EVENT_MODIFIER_FLAG_OPTION: u64 = 1 << 19; // 0x80000
const NS_EVENT_MODIFIER_FLAG_COMMAND: u64 = 1 << 20; // 0x100000

/// Convert macOS modifier flags to `KeyModifiers`.
pub fn macos_modifiers(modifier_flags: u64) -> KeyModifiers {
    KeyModifiers {
        shift_key: (modifier_flags & NS_EVENT_MODIFIER_FLAG_SHIFT) != 0,
        control_key: (modifier_flags & NS_EVENT_MODIFIER_FLAG_CONTROL) != 0,
        alt_key: (modifier_flags & NS_EVENT_MODIFIER_FLAG_OPTION) != 0,
        super_key: (modifier_flags & NS_EVENT_MODIFIER_FLAG_COMMAND) != 0,
    }
}

/// Map a macOS Carbon virtual key code to a `Keysym`.
///
/// For printable characters, the `character` parameter (Unicode scalar from
/// `NSEvent.characters`) is used directly, since the virtual key code alone
/// does not carry case/layout information.
///
/// Returns `None` for unknown/unmapped key codes when `character` is also 0.
pub fn macos_keycode_to_keysym(keycode: u16, character: u32) -> Option<Keysym> {
    // First, check non-printable / special keys by virtual key code.
    let keysym = match keycode {
        0x24 => Keysym::RETURN,
        0x33 => Keysym::BACKSPACE,
        0x75 => Keysym::DELETE,
        0x35 => Keysym::ESCAPE,
        0x30 => Keysym::TAB,
        0x31 => Keysym::SPACE,
        // Arrows
        0x7B => Keysym::LEFT,
        0x7C => Keysym::RIGHT,
        0x7E => Keysym::UP,
        0x7D => Keysym::DOWN,
        // Navigation
        0x73 => Keysym::HOME,
        0x77 => Keysym::END,
        0x74 => Keysym::PAGE_UP,
        0x79 => Keysym::PAGE_DOWN,
        // Modifier keys (for flagsChanged events)
        0x3D => Keysym::ALT_R,   // Right Option — mode toggle
        0x36 => Keysym::SUPER_R, // Right Command — mode toggle
        0x38 => Keysym::SHIFT_L,
        0x3C => Keysym::SHIFT_R,
        0x3B => Keysym::CONTROL_L,
        0x3E => Keysym::CONTROL_R,
        0x3A => Keysym::ALT_L,
        0x37 => Keysym::SUPER_L,
        // Function keys
        0x7A => Keysym::F1,
        0x78 => Keysym::F2,
        0x63 => Keysym::F3,
        0x76 => Keysym::F4,
        0x60 => Keysym::F5,
        0x61 => Keysym::F6,
        0x62 => Keysym::F7,
        0x64 => Keysym::F8,
        0x65 => Keysym::F9,
        0x6D => Keysym::F10,
        0x67 => Keysym::F11,
        0x6F => Keysym::F12,
        _ => {
            // For printable characters, use the Unicode scalar value.
            // XKB keysyms for ASCII printable (0x20-0x7e) are identical to Unicode.
            if (0x0020..=0x007e).contains(&character) {
                return Some(Keysym(character));
            }
            return if character != 0 {
                // Non-ASCII printable — pass through as keysym
                Some(Keysym(character))
            } else {
                None
            };
        }
    };
    Some(keysym)
}

/// Convert a macOS key event into a `KeyEvent` suitable for `InputMethodEngine`.
///
/// # Parameters
/// - `keycode`: `NSEvent.keyCode` (Carbon virtual key code)
/// - `character`: Unicode scalar value from `NSEvent.characters` (0 if unavailable)
/// - `modifier_flags`: `NSEvent.modifierFlags.rawValue`
/// - `is_press`: `true` for key-down, `false` for key-up
pub fn macos_key_to_event(
    keycode: u16,
    character: u32,
    modifier_flags: u64,
    is_press: bool,
) -> Option<KeyEvent> {
    let keysym = macos_keycode_to_keysym(keycode, character)?;
    let modifiers = macos_modifiers(modifier_flags);
    Some(KeyEvent::new(keysym, modifiers, is_press))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_return_key() {
        let keysym = macos_keycode_to_keysym(0x24, 0).unwrap();
        assert_eq!(keysym, Keysym::RETURN);
    }

    #[test]
    fn test_backspace() {
        let keysym = macos_keycode_to_keysym(0x33, 0).unwrap();
        assert_eq!(keysym, Keysym::BACKSPACE);
    }

    #[test]
    fn test_escape() {
        let keysym = macos_keycode_to_keysym(0x35, 0).unwrap();
        assert_eq!(keysym, Keysym::ESCAPE);
    }

    #[test]
    fn test_arrow_keys() {
        assert_eq!(macos_keycode_to_keysym(0x7B, 0).unwrap(), Keysym::LEFT);
        assert_eq!(macos_keycode_to_keysym(0x7C, 0).unwrap(), Keysym::RIGHT);
        assert_eq!(macos_keycode_to_keysym(0x7E, 0).unwrap(), Keysym::UP);
        assert_eq!(macos_keycode_to_keysym(0x7D, 0).unwrap(), Keysym::DOWN);
    }

    #[test]
    fn test_printable_ascii() {
        // 'a' = 0x61
        let keysym = macos_keycode_to_keysym(0x00, 0x61).unwrap();
        assert_eq!(keysym, Keysym(0x61));
        assert!(keysym.is_printable());
        assert_eq!(keysym.to_char(), Some('a'));
    }

    #[test]
    fn test_printable_uppercase() {
        // 'A' = 0x41
        let keysym = macos_keycode_to_keysym(0x00, 0x41).unwrap();
        assert_eq!(keysym, Keysym(0x41));
        assert_eq!(keysym.to_char(), Some('A'));
    }

    #[test]
    fn test_space_by_keycode() {
        // Space has a dedicated keycode (0x31)
        let keysym = macos_keycode_to_keysym(0x31, 0x20).unwrap();
        assert_eq!(keysym, Keysym::SPACE);
    }

    #[test]
    fn test_modifier_flags() {
        let mods = macos_modifiers(0x20000); // Shift
        assert!(mods.shift_key);
        assert!(!mods.control_key);

        let mods = macos_modifiers(0x40000 | 0x80000); // Control + Option
        assert!(mods.control_key);
        assert!(mods.alt_key);
        assert!(!mods.shift_key);

        let mods = macos_modifiers(0x100000); // Command
        assert!(mods.super_key);
    }

    #[test]
    fn test_macos_key_to_event() {
        // 'a' key press with no modifiers
        let event = macos_key_to_event(0x00, 0x61, 0, true).unwrap();
        assert_eq!(event.keysym, Keysym(0x61));
        assert!(event.is_press);
        assert!(event.modifiers.is_empty());
    }

    #[test]
    fn test_macos_key_to_event_with_shift() {
        // 'A' key press with Shift
        let event = macos_key_to_event(0x00, 0x41, 0x20000, true).unwrap();
        assert_eq!(event.keysym, Keysym(0x41));
        assert!(event.is_press);
        assert!(event.modifiers.shift_key);
    }

    #[test]
    fn test_function_keys() {
        assert_eq!(macos_keycode_to_keysym(0x7A, 0).unwrap(), Keysym::F1);
        assert_eq!(macos_keycode_to_keysym(0x78, 0).unwrap(), Keysym::F2);
        assert_eq!(macos_keycode_to_keysym(0x6F, 0).unwrap(), Keysym::F12);
    }

    #[test]
    fn test_mode_toggle_keys() {
        // Right Option
        let keysym = macos_keycode_to_keysym(0x3D, 0).unwrap();
        assert_eq!(keysym, Keysym::ALT_R);
        assert!(keysym.is_mode_toggle_key());

        // Right Command
        let keysym = macos_keycode_to_keysym(0x36, 0).unwrap();
        assert_eq!(keysym, Keysym::SUPER_R);
        assert!(keysym.is_mode_toggle_key());
    }

    #[test]
    fn test_unknown_key_no_character() {
        // Unknown keycode with no character → None
        assert!(macos_keycode_to_keysym(0xFF, 0).is_none());
    }

    #[test]
    fn test_navigation_keys() {
        assert_eq!(macos_keycode_to_keysym(0x73, 0).unwrap(), Keysym::HOME);
        assert_eq!(macos_keycode_to_keysym(0x77, 0).unwrap(), Keysym::END);
        assert_eq!(macos_keycode_to_keysym(0x74, 0).unwrap(), Keysym::PAGE_UP);
        assert_eq!(macos_keycode_to_keysym(0x79, 0).unwrap(), Keysym::PAGE_DOWN);
    }

    #[test]
    fn test_modifier_keycodes() {
        assert_eq!(macos_keycode_to_keysym(0x38, 0).unwrap(), Keysym::SHIFT_L);
        assert_eq!(macos_keycode_to_keysym(0x3C, 0).unwrap(), Keysym::SHIFT_R);
        assert_eq!(macos_keycode_to_keysym(0x3B, 0).unwrap(), Keysym::CONTROL_L);
        assert_eq!(macos_keycode_to_keysym(0x3E, 0).unwrap(), Keysym::CONTROL_R);
        assert_eq!(macos_keycode_to_keysym(0x3A, 0).unwrap(), Keysym::ALT_L);
        assert_eq!(macos_keycode_to_keysym(0x37, 0).unwrap(), Keysym::SUPER_L);
    }
}
