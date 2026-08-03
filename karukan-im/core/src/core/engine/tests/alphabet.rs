use super::*;

// --- Alphabet Mode Tests ---

#[test]
fn test_shift_alone_does_not_toggle_mode() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Shift press alone should NOT toggle mode
    let result = engine.process_key(&press_key(Keysym::SHIFT_L));
    assert!(!result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Shift release is a no-op
    let result = engine.process_key(&release_key(Keysym::SHIFT_L));
    assert!(!result.consumed);
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_shift_letter_enters_alphabet_mode() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Shift+A → enters alphabet mode and inputs 'A'
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "A");

    // Release shift → no-op
    engine.process_key(&release_key(Keysym::SHIFT_L));
    assert!(engine.mode.current() == InputMode::Alphabet); // Still in alphabet mode
}

#[test]
fn test_shift_letter_fcitx5_lowercase_keysym() {
    // fcitx5 sends lowercase keysym 'a' (0x0061) with shift modifier flag
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // fcitx5 sends keysym='a' (lowercase!) with modifiers.shift_key=true
    // This should enter alphabet mode and input uppercase 'A'
    let event = KeyEvent::new(
        Keysym(0x0061), // lowercase 'a'
        KeyModifiers::new().with_shift(true),
        true,
    );
    engine.process_key(&event);
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    // Should be uppercase 'A' in preedit
    assert_eq!(engine.preedit().unwrap().text(), "A");
}

#[test]
fn test_shift_letter_in_hiragana_enters_alphabet_and_uppercase() {
    let mut engine = InputMethodEngine::new();

    // Type some hiragana first
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Shift press
    engine.process_key(&press_key(Keysym::SHIFT_L));

    // Shift+a (fcitx5 sends lowercase keysym)
    let event = KeyEvent::new(Keysym(0x0061), KeyModifiers::new().with_shift(true), true);
    engine.process_key(&event);
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert_eq!(engine.preedit().unwrap().text(), "あA");
}

#[test]
fn test_uppercase_keysym_without_shift_flag_enters_alphabet() {
    // fcitx5 may resolve Shift into the keysym, sending 'A' (0x0041) without
    // the shift modifier flag. This must still enter alphabet mode.
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Empty state: uppercase keysym without shift flag
    let event = KeyEvent::new(
        Keysym(0x0041), // uppercase 'A'
        KeyModifiers::new(),
        true,
    );
    engine.process_key(&event);
    assert!(
        engine.mode.current() == InputMode::Alphabet,
        "Uppercase keysym should enter alphabet mode even without shift flag"
    );
    assert_eq!(engine.preedit().unwrap().text(), "A");
}

#[test]
fn test_uppercase_keysym_without_shift_flag_composing() {
    // Same as above but in Composing state (hiragana already entered)
    let mut engine = InputMethodEngine::new();

    // Type hiragana first
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Uppercase keysym without shift flag
    let event = KeyEvent::new(
        Keysym(0x0041), // uppercase 'A'
        KeyModifiers::new(),
        true,
    );
    engine.process_key(&event);
    assert!(
        engine.mode.current() == InputMode::Alphabet,
        "Uppercase keysym should enter alphabet mode during composing"
    );
    assert_eq!(engine.preedit().unwrap().text(), "あA");
}

#[test]
fn test_shift_symbol_stays_in_hiragana_mode() {
    // Shift+symbol should NOT enter alphabet mode (only Shift+letter does)
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // '!' with shift modifier → stays in hiragana mode
    let event = KeyEvent::new(
        Keysym(0x0021), // '!'
        KeyModifiers::new().with_shift(true),
        true,
    );
    engine.process_key(&event);
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_shift_digit_stays_in_hiragana_mode() {
    // Shift+digit should NOT enter alphabet mode (only Shift+letter does)
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // '2' with shift modifier → stays in hiragana mode
    let event = KeyEvent::new(
        Keysym(0x0032), // '2'
        KeyModifiers::new().with_shift(true),
        true,
    );
    engine.process_key(&event);
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_alphabet_mode_uppercase_with_shift() {
    let mut engine = InputMethodEngine::new();

    // Shift+A → enters alphabet mode and inputs 'A'
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Type lowercase 'a' → should be 'a'
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "Aa");

    // Shift+a → uppercase 'A' (still in alphabet mode)
    let event = KeyEvent::new(
        Keysym(0x0061), // lowercase keysym
        KeyModifiers::new().with_shift(true),
        true,
    );
    engine.process_key(&event);
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert_eq!(engine.preedit().unwrap().text(), "AaA");
}

#[test]
fn test_alphabet_mode_direct_input() {
    let mut engine = InputMethodEngine::new();

    // Shift+A → enters alphabet mode and inputs 'A'
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "A");

    // Type 'b' → should be 'Ab' (still in alphabet mode)
    engine.process_key(&press('b'));
    assert_eq!(engine.preedit().unwrap().text(), "Ab");

    // Type 'c' → should be 'Abc'
    engine.process_key(&press('c'));
    assert_eq!(engine.preedit().unwrap().text(), "Abc");
}

#[test]
fn test_mixed_hiragana_alphabet_input() {
    let mut engine = InputMethodEngine::new();

    // Type hiragana first: "わたしは"
    engine.process_key(&press('w'));
    engine.process_key(&press('a'));
    engine.process_key(&press('t'));
    engine.process_key(&press('a'));
    engine.process_key(&press('s'));
    engine.process_key(&press('i'));
    engine.process_key(&press('h'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "わたしは");
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Shift+L → enters alphabet mode, inputs 'L'
    // fcitx5 sends lowercase keysym with shift flag
    let event = KeyEvent::new(
        Keysym(0x006c), // lowercase 'l'
        KeyModifiers::new().with_shift(true),
        true,
    );
    engine.process_key(&event);
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert_eq!(engine.preedit().unwrap().text(), "わたしはL");

    // Continue typing in alphabet mode (without shift → lowercase)
    engine.process_key(&press('i'));
    engine.process_key(&press('n'));
    engine.process_key(&press('u'));
    engine.process_key(&press('x'));
    assert_eq!(engine.preedit().unwrap().text(), "わたしはLinux");
}

#[test]
fn test_shift_alphabet_reverts_to_hiragana_after_commit() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+H from the default Hiragana mode
    engine.process_key(&press_shift('H'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Type and commit the alphabet word
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(matches!(engine.state(), InputState::Empty));

    // Shift-alphabet is a temporary per-word mode: after commit we are back
    // in Hiragana, so the next word is converted to kana again (issue #37).
    assert!(engine.mode.current() == InputMode::Hiragana);
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_shift_alphabet_reverts_to_hiragana_after_cancel() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A, type, then cancel
    engine.process_key(&press_shift('A'));
    engine.process_key(&press('b'));

    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Empty));
    // Cancelling the temporary alphabet word restores Hiragana
    assert!(engine.mode.current() == InputMode::Hiragana);
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_shift_alphabet_reverts_to_hiragana_after_erase_to_empty() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Erasing back to an empty buffer ends the temporary alphabet word
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.mode.current() == InputMode::Hiragana);
}

#[test]
fn test_shift_alphabet_from_katakana_reverts_to_katakana() {
    let mut engine = InputMethodEngine::new();

    // Type a char, then switch to katakana mode (Ctrl+K)
    engine.process_key(&press('a'));
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert!(engine.mode.current() == InputMode::Katakana);

    // Shift+A enters alphabet, remembering Katakana as the prior mode
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Commit → reverts to Katakana (the mode before the Shift gesture),
    // not all the way to Hiragana
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(engine.mode.current() == InputMode::Katakana);
}

#[test]
fn test_alphabet_mode_aux_text() {
    let mut engine = InputMethodEngine::new();

    // In hiragana mode, aux should show [あ]
    engine.process_key(&press('a'));
    let aux_hiragana = engine.format_aux_composing();
    assert!(aux_hiragana.starts_with("[あ]"));

    engine.process_key(&press_key(Keysym::ESCAPE));

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));

    let aux_alpha = engine.format_aux_composing();
    assert!(aux_alpha.starts_with("[A]"));
}

#[test]
fn test_shift_right_alone_does_not_toggle() {
    let mut engine = InputMethodEngine::new();
    assert!(engine.mode.current() != InputMode::Alphabet);

    // Right Shift alone should NOT toggle alphabet mode
    engine.process_key(&press_key(Keysym::SHIFT_R));
    assert!(engine.mode.current() != InputMode::Alphabet);
}

#[test]
fn test_reset_clears_alphabet_mode() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+A
    engine.process_key(&press_shift('A'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    engine.reset();
    assert!(engine.mode.current() != InputMode::Alphabet);
}
