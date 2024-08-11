//! Key code definitions and key event handling

use std::fmt;

/// Key symbol (keysym) values
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Keysym(pub u32);

impl Keysym {
    // Common key symbols (XKB keysym values)
    pub const BACKSPACE: Keysym = Keysym(0xff08);
    pub const TAB: Keysym = Keysym(0xff09);
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

    // Space
    pub const SPACE: Keysym = Keysym(0x0020);

    // Numbers
    pub const KEY_0: Keysym = Keysym(0x0030);
    pub const KEY_1: Keysym = Keysym(0x0031);
    pub const KEY_2: Keysym = Keysym(0x0032);
    pub const KEY_3: Keysym = Keysym(0x0033);
    pub const KEY_4: Keysym = Keysym(0x0034);
    pub const KEY_5: Keysym = Keysym(0x0035);
    pub const KEY_6: Keysym = Keysym(0x0036);
    pub const KEY_7: Keysym = Keysym(0x0037);
    pub const KEY_8: Keysym = Keysym(0x0038);
    pub const KEY_9: Keysym = Keysym(0x0039);

    // Letters (lowercase and uppercase)
    pub const KEY_A: Keysym = Keysym(0x0061); // lowercase 'a'
    pub const KEY_A_UPPER: Keysym = Keysym(0x0041); // uppercase 'A'
    pub const KEY_B: Keysym = Keysym(0x0062); // lowercase 'b'
    pub const KEY_B_UPPER: Keysym = Keysym(0x0042); // uppercase 'B'
    pub const KEY_E: Keysym = Keysym(0x0065); // lowercase 'e'
    pub const KEY_E_UPPER: Keysym = Keysym(0x0045); // uppercase 'E'
    pub const KEY_F: Keysym = Keysym(0x0066); // lowercase 'f'
    pub const KEY_F_UPPER: Keysym = Keysym(0x0046); // uppercase 'F'
    pub const KEY_K: Keysym = Keysym(0x006b); // lowercase 'k'
    pub const KEY_K_UPPER: Keysym = Keysym(0x004b); // uppercase 'K'
    pub const KEY_N: Keysym = Keysym(0x006e); // lowercase 'n'
    pub const KEY_N_UPPER: Keysym = Keysym(0x004e); // uppercase 'N'
    pub const KEY_L: Keysym = Keysym(0x006c); // lowercase 'l'
    pub const KEY_L_UPPER: Keysym = Keysym(0x004c); // uppercase 'L'
    pub const KEY_P: Keysym = Keysym(0x0070); // lowercase 'p'
    pub const KEY_P_UPPER: Keysym = Keysym(0x0050); // uppercase 'P'

    // Function keys
    pub const F1: Keysym = Keysym(0xffbe);
    pub const F2: Keysym = Keysym(0xffbf);
    pub const F3: Keysym = Keysym(0xffc0);
    pub const F4: Keysym = Keysym(0xffc1);
    pub const F5: Keysym = Keysym(0xffc2);
    pub const F6: Keysym = Keysym(0xffc3);
    pub const F7: Keysym = Keysym(0xffc4);
    pub const F8: Keysym = Keysym(0xffc5);
    pub const F9: Keysym = Keysym(0xffc6);
    pub const F10: Keysym = Keysym(0xffc7);
    pub const F11: Keysym = Keysym(0xffc8);
    pub const F12: Keysym = Keysym(0xffc9);

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

    /// Check if this is a shift key
    pub fn is_shift(&self) -> bool {
        matches!(*self, Self::SHIFT_L | Self::SHIFT_R)
    }

    /// Check if this is a control key
    pub fn is_control(&self) -> bool {
        matches!(*self, Self::CONTROL_L | Self::CONTROL_R)
    }

    /// Check if this is a right-side modifier key used for alphabet/hiragana mode toggle.
    /// Different keyboards map the right CMD/Super key to different keysyms
    /// (Alt_R, Super_R, Meta_R, Hyper_R), so we accept all of them.
    pub fn is_mode_toggle_key(&self) -> bool {
        matches!(
            *self,
            Self::ALT_R | Self::SUPER_R | Self::META_R | Self::HYPER_R
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
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeyModifiers {
    pub shift_key: bool,
    pub control_key: bool,
    pub alt_key: bool,
    pub super_key: bool,
}

/// XKB modifier bitmask constants (used by both X11 and Wayland via fcitx5)
/// used in the FFI boundary between C++ (fcitx5) and Rust.
impl KeyModifiers {
    pub const SHIFT_MASK: u32 = 1; // ShiftMask
    pub const CONTROL_MASK: u32 = 4; // ControlMask
    pub const ALT_MASK: u32 = 8; // Mod1Mask
    pub const SUPER_MASK: u32 = 64; // Mod4Mask

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

    /// Get the character for this key event if it's a printable press
    pub fn to_char(&self) -> Option<char> {
        if self.is_printable_press() {
            self.keysym.to_char()
        } else {
            None
        }
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
        assert_eq!(Keysym::KEY_1.digit_value(), Some(1));
        assert_eq!(Keysym::KEY_9.digit_value(), Some(9));
        assert_eq!(Keysym::KEY_0.digit_value(), None);
        assert_eq!(Keysym(0x0061).digit_value(), None);
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
