import Cocoa

/// XKB-style modifier flags sent to the engine.
struct KeyModifiers {
    var shift = false
    var control = false
    var alt = false
    var superKey = false

    var jsonObject: [String: Any] {
        ["shift": shift, "control": control, "alt": alt, "super": superKey]
    }
}

/// A key event translated into the engine's representation.
struct EngineKeyEvent {
    let keysym: UInt32
    let modifiers: KeyModifiers
}

/// Translates macOS key events into XKB keysyms, the representation the
/// karukan engine shares with the fcitx5 frontend (see
/// karukan-im/src/core/keycode.rs).
enum KeyCodeMap {
    // macOS virtual key codes (Carbon kVK_*) for non-printable keys.
    private static let specialKeys: [UInt16: UInt32] = [
        36: 0xff0d,  // Return
        48: 0xff09,  // Tab
        51: 0xff08,  // Delete (Backspace)
        53: 0xff1b,  // Escape
        76: 0xff8d,  // Keypad Enter
        115: 0xff50,  // Home
        116: 0xff55,  // Page Up
        117: 0xffff,  // Forward Delete
        119: 0xff57,  // End
        121: 0xff56,  // Page Down
        123: 0xff51,  // Left
        124: 0xff53,  // Right
        125: 0xff54,  // Down
        126: 0xff52,  // Up
        // Function keys F1-F12
        122: 0xffbe, 120: 0xffbf, 99: 0xffc0, 118: 0xffc1,
        96: 0xffc2, 97: 0xffc3, 98: 0xffc4, 100: 0xffc5,
        101: 0xffc6, 109: 0xffc7, 103: 0xffc8, 111: 0xffc9,
    ]

    /// JIS keyboard かな key (kVK_JIS_Kana).
    static let kanaKeyCode: UInt16 = 104
    /// JIS keyboard 英数 key (kVK_JIS_Eisu).
    static let eisuKeyCode: UInt16 = 102
    /// XKB Super_R keysym — the engine's katakana→hiragana toggle.
    static let superRKeysym: UInt32 = 0xffec

    static func modifiers(from flags: NSEvent.ModifierFlags) -> KeyModifiers {
        KeyModifiers(
            shift: flags.contains(.shift),
            control: flags.contains(.control),
            alt: flags.contains(.option),
            superKey: flags.contains(.command)
        )
    }

    /// Translate a key-down event into an XKB keysym. Returns nil for keys
    /// the engine has no representation for (the event passes through).
    static func translate(
        keyCode: UInt16, characters: String?, charactersIgnoringModifiers: String?,
        flags: NSEvent.ModifierFlags
    ) -> EngineKeyEvent? {
        let modifiers = modifiers(from: flags)

        if let keysym = specialKeys[keyCode] {
            return EngineKeyEvent(keysym: keysym, modifiers: modifiers)
        }

        // Printable ASCII: XKB keysyms for Latin-1 equal the code point.
        // Prefer `characters` — for IMK key events it is what reliably has
        // Shift applied to punctuation (Shift+/ → "?");
        // charactersIgnoringModifiers can resolve to the unshifted key
        // ("/"), which turned ？ into ・ (Mozc reads `characters` too).
        // When Control/Option mangle `characters` into a control character
        // (Ctrl+A → U+0001) or an option glyph (Option+a → å), fall back to
        // charactersIgnoringModifiers so the engine gets the plain key plus
        // modifier flags, matching what fcitx5 delivers.
        func asciiScalar(of string: String?) -> UInt32? {
            guard let scalar = string?.unicodeScalars.first,
                (0x20...0x7e).contains(scalar.value)
            else { return nil }
            return scalar.value
        }
        guard
            let keysym = asciiScalar(of: characters)
                ?? asciiScalar(of: charactersIgnoringModifiers)
        else {
            return nil
        }
        return EngineKeyEvent(keysym: keysym, modifiers: modifiers)
    }

    static func translate(event: NSEvent) -> EngineKeyEvent? {
        translate(
            keyCode: event.keyCode,
            characters: event.characters,
            charactersIgnoringModifiers: event.charactersIgnoringModifiers,
            flags: event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        )
    }
}
