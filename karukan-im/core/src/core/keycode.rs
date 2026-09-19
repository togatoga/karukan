//! Key code definitions and key event handling

use std::fmt;

/// Key symbol (keysym) values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keysym(pub u32);

impl Keysym {
    // Common key symbols (XKB keysym values)
    pub const BACKSPACE: Keysym = Keysym(0xff08);
    pub const TAB: Keysym = Keysym(0xff09);
    /// Shift+Tab as X11 delivers it (a distinct keysym, not Tab+Shift)
    pub const ISO_LEFT_TAB: Keysym = Keysym(0xfe20);
    pub const RETURN: Keysym = Keysym(0xff0d);
    pub const ESCAPE: Keysym = Keysym(0xff1b);
    pub const DELETE: Keysym = Keysym(0xffff);

    // Cursor movement
    pub const HOME: Keysym = Keysym(0xff50);
    pub const LEFT: Keysym = Keysym(0xff51);
    pub const UP: Keysym = Keysym(0xff52);
    pub const RIGHT: Keysym = Keysym(0xff53);
    pub const DOWN: Keysym = Keysym(0xff54);
    pub const PAGE_UP: Keysym = Keysym(0xff55);
    pub const PAGE_DOWN: Keysym = Keysym(0xff56);
    pub const END: Keysym = Keysym(0xff57);

    // Modifiers
    pub const SHIFT_L: Keysym = Keysym(0xffe1);
    pub const SHIFT_R: Keysym = Keysym(0xffe2);
    pub const CONTROL_L: Keysym = Keysym(0xffe3);
    pub const CONTROL_R: Keysym = Keysym(0xffe4);
    pub const ALT_L: Keysym = Keysym(0xffe9);
    pub const ALT_R: Keysym = Keysym(0xffea);
    pub const META_L: Keysym = Keysym(0xffe7);
    pub const META_R: Keysym = Keysym(0xffe8);
    pub const SUPER_L: Keysym = Keysym(0xffeb);
    pub const SUPER_R: Keysym = Keysym(0xffec);
    pub const HYPER_L: Keysym = Keysym(0xffed);
    pub const HYPER_R: Keysym = Keysym(0xffee);

    // Japanese keyboard keys (JIS layout)
    /// 変換 key (XK_Henkan_Mode; XK_Henkan is a non-deprecated alias for the
    /// same value). Delivered by fcitx5 for the key right of Space on JIS
    /// keyboards.
    pub const HENKAN: Keysym = Keysym(0xff23);

    // Space
    pub const SPACE: Keysym = Keysym(0x0020);

    /// Check if this keysym represents a printable character
    pub fn is_printable(&self) -> bool {
        // ASCII printable range (0x20-0x7e)
        (0x0020..=0x007e).contains(&self.0)
    }

    /// Try to convert this keysym to a character
    pub fn to_char(&self) -> Option<char> {
        if self.is_printable() {
            char::from_u32(self.0)
        } else {
            None
        }
    }

    /// Check if this keysym is a digit (1-9)
    pub fn digit_value(&self) -> Option<usize> {
        match self.0 {
            0x0031..=0x0039 => Some((self.0 - 0x0030) as usize),
            _ => None,
        }
    }

    /// The letter this keysym types, lowercased. Chords are bound to the
    /// key, and some environments fold Shift into the uppercase keysym.
    pub fn letter(&self) -> Option<char> {
        self.to_char()
            .filter(char::is_ascii_alphabetic)
            .map(|c| c.to_ascii_lowercase())
    }

    /// Check if this key switches back to hiragana input mode.
    ///
    /// Two families of keys qualify:
    /// * Right-side modifiers — different keyboards map the right CMD/Super
    ///   key to different keysyms (Alt_R, Super_R, Meta_R, Hyper_R), so we
    ///   accept all of them. The macOS frontend also synthesizes Super_R
    ///   for the JIS かな key and the lone right-⌘ tap.
    /// * The JIS 変換 key (Henkan) — the dedicated "give me Japanese" key on
    ///   Japanese keyboards under Linux/fcitx5, so JIS users aren't forced
    ///   onto the right-modifier gesture (issue #33).
    pub fn is_mode_toggle_key(&self) -> bool {
        matches!(
            *self,
            Self::ALT_R | Self::SUPER_R | Self::META_R | Self::HYPER_R | Self::HENKAN
        )
    }

    /// Check if this is a modifier key
    pub fn is_modifier(&self) -> bool {
        matches!(
            *self,
            Self::SHIFT_L
                | Self::SHIFT_R
                | Self::CONTROL_L
                | Self::CONTROL_R
                | Self::ALT_L
                | Self::ALT_R
                | Self::META_L
                | Self::META_R
                | Self::SUPER_L
                | Self::SUPER_R
                | Self::HYPER_L
                | Self::HYPER_R
        )
    }
}

/// Printable keys are spelled as the character they type: a Latin-1
/// keysym is its code point.
impl From<char> for Keysym {
    fn from(c: char) -> Self {
        Keysym(c as u32)
    }
}

impl fmt::Display for Keysym {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(ch) = self.to_char() {
            write!(f, "{}", ch)
        } else {
            write!(f, "Keysym(0x{:04x})", self.0)
        }
    }
}

/// Key modifier flags
///
/// Also the `process_key` wire format of the stdio JSON-RPC server
/// (`server::protocol`), where the fields are named `shift` / `control` /
/// `alt` / `super`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Deserialize)]
pub struct KeyModifiers {
    #[serde(default, rename = "shift")]
    pub shift_key: bool,
    #[serde(default, rename = "control")]
    pub control_key: bool,
    #[serde(default, rename = "alt")]
    pub alt_key: bool,
    #[serde(default, rename = "super")]
    pub super_key: bool,
}

/// XKB modifier bitmask constants (used by both X11 and Wayland via fcitx5)
/// used in the FFI boundary between C++ (fcitx5) and Rust.
impl KeyModifiers {
    pub const SHIFT_MASK: u32 = 1; // ShiftMask
    pub const CONTROL_MASK: u32 = 4; // ControlMask
    pub const ALT_MASK: u32 = 8; // Mod1Mask
    pub const SUPER_MASK: u32 = 64; // Mod4Mask

    /// True when any modifier (Shift/Ctrl/Alt/Super) is held.
    pub fn any(&self) -> bool {
        self.shift_key || self.control_key || self.alt_key || self.super_key
    }

    /// Decode a bitmask of XKB modifier flags into a `KeyModifiers` struct.
    pub fn from_modifier_state(state: u32) -> Self {
        Self {
            shift_key: (state & Self::SHIFT_MASK) != 0,
            control_key: (state & Self::CONTROL_MASK) != 0,
            alt_key: (state & Self::ALT_MASK) != 0,
            super_key: (state & Self::SUPER_MASK) != 0,
        }
    }
}

impl KeyModifiers {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_shift(mut self, shift: bool) -> Self {
        self.shift_key = shift;
        self
    }

    pub fn with_control(mut self, control: bool) -> Self {
        self.control_key = control;
        self
    }

    pub fn is_empty(&self) -> bool {
        !self.shift_key && !self.control_key && !self.alt_key && !self.super_key
    }
}

/// A key event
#[derive(Debug, Clone)]
pub struct KeyEvent {
    /// The key symbol
    pub keysym: Keysym,
    /// Modifier key state
    pub modifiers: KeyModifiers,
    /// Whether this is a key press (true) or release (false)
    pub is_press: bool,
}

impl KeyEvent {
    pub fn new(keysym: Keysym, modifiers: KeyModifiers, is_press: bool) -> Self {
        Self {
            keysym,
            modifiers,
            is_press,
        }
    }

    /// Create a simple key press event without modifiers
    pub fn press(keysym: Keysym) -> Self {
        Self::new(keysym, KeyModifiers::default(), true)
    }

    /// Check if this is a printable character key press
    pub fn is_printable_press(&self) -> bool {
        self.is_press
            && self.keysym.is_printable()
            && !self.modifiers.control_key
            && !self.modifiers.alt_key
    }

    /// The character a printable press types. Shift+letter is the uppercase
    /// letter whichever way the frontend spells it (the uppercase keysym, or
    /// the lowercase one with the Shift bit), so callers see one shape.
    pub fn to_char(&self) -> Option<char> {
        if !self.is_printable_press() {
            return None;
        }
        let c = self.keysym.to_char()?;
        Some(if self.modifiers.shift_key {
            c.to_ascii_uppercase()
        } else {
            c
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keysym_printable() {
        assert!(Keysym(0x0061).is_printable()); // 'a'
        assert!(Keysym(0x0041).is_printable()); // 'A'
        assert!(Keysym(0x0020).is_printable()); // space
        assert!(!Keysym::BACKSPACE.is_printable());
        assert!(!Keysym::RETURN.is_printable());
    }

    #[test]
    fn test_keysym_to_char() {
        assert_eq!(Keysym(0x0061).to_char(), Some('a'));
        assert_eq!(Keysym(0x0041).to_char(), Some('A'));
        assert_eq!(Keysym::BACKSPACE.to_char(), None);
    }

    #[test]
    fn test_digit_value() {
        assert_eq!(Keysym::from('1').digit_value(), Some(1));
        assert_eq!(Keysym::from('9').digit_value(), Some(9));
        assert_eq!(Keysym::from('0').digit_value(), None);
        assert_eq!(Keysym::from('a').digit_value(), None);
    }

    #[test]
    fn test_keysym_from_char() {
        assert_eq!(Keysym::from('a'), Keysym(0x0061));
        assert_eq!(Keysym::from('é'), Keysym(0x00e9));
    }

    #[test]
    fn test_letter() {
        assert_eq!(Keysym::from('t').letter(), Some('t'));
        assert_eq!(Keysym::from('T').letter(), Some('t'));
        assert_eq!(Keysym::from('1').letter(), None);
        assert_eq!(Keysym::SPACE.letter(), None);
    }

    #[test]
    fn test_key_event_shift_letter() {
        let shifted =
            |c| KeyEvent::new(Keysym::from(c), KeyModifiers::new().with_shift(true), true);
        assert_eq!(shifted('a').to_char(), Some('A'));
        assert_eq!(KeyEvent::press(Keysym::from('A')).to_char(), Some('A'));
        assert_eq!(shifted(';').to_char(), Some(';'));
    }

    #[test]
    fn test_key_event_printable() {
        let event = KeyEvent::press(Keysym(0x0061));
        assert!(event.is_printable_press());
        assert_eq!(event.to_char(), Some('a'));

        let ctrl_a = KeyEvent::new(Keysym(0x0061), KeyModifiers::new().with_control(true), true);
        assert!(!ctrl_a.is_printable_press());
    }
}
