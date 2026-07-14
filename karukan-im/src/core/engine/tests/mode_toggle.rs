use super::*;

// --- Mode toggle key tests (one-way: alphabet → hiragana) ---

#[test]
fn test_mode_toggle_key_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Alt_R press → switch to hiragana mode (mid-composition; the toggle key
    // is the explicit way out, independent of the per-word auto-revert)
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Clear the composed "A", then type 'a' → should be 'あ' (hiragana mode)
    engine.process_key(&press_key(Keysym::RETURN));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_mode_toggle_key_noop_in_hiragana() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Alt_R press in hiragana mode → not consumed, no mode change
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(!result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Type 'a' → still hiragana
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_mode_toggle_key_during_alphabet_input() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A and type "b"
    engine.process_key(&press_shift('A'));
    engine.process_key(&press('b'));
    assert_eq!(engine.preedit().unwrap().text(), "Ab");
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Alt_R → switch to hiragana
    let result = engine.process_key(&press_key(Keysym::ALT_R));
    assert!(result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Continue typing → hiragana
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "Abか");
}

#[test]
fn test_super_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Super_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::SUPER_R));
    assert!(result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_meta_r_also_switches_alphabet_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Meta_R press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::META_R));
    assert!(result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_henkan_switches_alphabet_to_hiragana() {
    // JIS 変換 key: the dedicated hiragana-return key for Japanese
    // keyboards, so JIS users aren't forced onto the right-modifier
    // gesture (issue #33).
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // 変換 press → switch to hiragana (one-way)
    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(result.consumed);
    assert!(engine.mode.current() == InputMode::Hiragana);

    // Clear the composed "A", then type 'a' → should be 'あ' (hiragana mode)
    engine.process_key(&press_key(Keysym::RETURN));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_henkan_switches_katakana_to_hiragana_and_bakes_preedit() {
    let mut engine = InputMethodEngine::new();

    // Compose "か", enter katakana mode via Ctrl+K
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert!(engine.mode.current() == InputMode::Katakana);
    assert_eq!(engine.preedit().unwrap().text(), "カ");

    // 変換 press → back to hiragana; the katakana preedit must stay
    // katakana (baked), not revert to hiragana display
    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(result.consumed);
    assert!(engine.mode.current() == InputMode::Hiragana);
    assert_eq!(engine.preedit().unwrap().text(), "カ");
}

#[test]
fn test_henkan_noop_in_hiragana_passes_through() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() == InputMode::Hiragana);

    // 変換 in hiragana mode → not consumed, no mode change (same policy
    // as the right-modifier toggle keys)
    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(!result.consumed);
    assert!(engine.mode.current() == InputMode::Hiragana);
}

#[test]
fn test_henkan_release_is_not_consumed() {
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // A release event must pass through and not switch the mode
    let result = engine.process_key(&release_key(Keysym::HENKAN));
    assert!(!result.consumed);
    assert!(engine.mode.current() == InputMode::Alphabet);
}

#[test]
fn test_modified_henkan_chord_is_not_hijacked() {
    // Ctrl+変換 may be an app or fcitx5 shortcut: only the bare press
    // toggles. The chord must not be consumed and must not switch modes.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    let result = engine.process_key(&press_ctrl(Keysym::HENKAN));
    assert!(!result.consumed);
    assert!(engine.mode.current() == InputMode::Alphabet);
}

#[test]
fn test_toggle_key_is_inert_during_conversion() {
    // Regression: toggling mid-Conversion used to katakana-bake the
    // conversion reading (Katakana mode) and defeat the Emoji-mode
    // learning guard. The toggle must leave Conversion state untouched.
    let mut engine = InputMethodEngine::new();

    // Katakana mode, compose かか, start conversion (candidate window open)
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::HENKAN));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(engine.mode.current() == InputMode::Katakana);
    // The conversion reading must not have been katakana-baked
    assert_eq!(engine.input_buf.text, "かか");

    // Escape back to Composing: the typed hiragana reading is intact
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}
