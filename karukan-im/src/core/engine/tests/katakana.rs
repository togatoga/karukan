use super::*;

// --- Katakana Conversion Tests ---

#[test]
fn test_hiragana_to_katakana() {
    assert_eq!(
        InputMethodEngine::hiragana_to_katakana("あいうえお"),
        "アイウエオ"
    );
    assert_eq!(
        InputMethodEngine::hiragana_to_katakana("かきくけこ"),
        "カキクケコ"
    );
    assert_eq!(
        InputMethodEngine::hiragana_to_katakana("がぎぐげご"),
        "ガギグゲゴ"
    );
    assert_eq!(
        InputMethodEngine::hiragana_to_katakana("ぱぴぷぺぽ"),
        "パピプペポ"
    );
    assert_eq!(InputMethodEngine::hiragana_to_katakana("っ"), "ッ");
    assert_eq!(InputMethodEngine::hiragana_to_katakana("ゃゅょ"), "ャュョ");
    // Mixed: non-hiragana characters should remain unchanged
    assert_eq!(
        InputMethodEngine::hiragana_to_katakana("あいabc"),
        "アイabc"
    );
    assert_eq!(InputMethodEngine::hiragana_to_katakana("テスト"), "テスト"); // Already katakana
}

#[test]
fn test_ctrl_k_converts_to_katakana() {
    let mut engine = InputMethodEngine::new();

    // Type "aiueo" -> "あいうえお"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    engine.process_key(&press('e'));
    engine.process_key(&press('o'));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");

    // Press Ctrl+k -> should convert preedit to katakana (preedit shows "アイウエオ")
    let ctrl_k = KeyEvent {
        keysym: Keysym::KEY_K,
        modifiers: KeyModifiers {
            control_key: true,
            shift_key: false,
            alt_key: false,
            super_key: false,
        },
        is_press: true,
    };
    let result = engine.process_key(&ctrl_k);

    assert!(result.consumed);
    // Should NOT commit yet - just convert display
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(!has_commit, "Should NOT commit on Ctrl+K");

    // Preedit should show katakana
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(
        engine.input_mode == InputMode::Katakana,
        "Should be in katakana mode"
    );

    // Now press Enter -> should commit as katakana
    let enter_result = engine.process_key(&press_key(Keysym::RETURN));
    let has_katakana_commit = enter_result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "アイウエオ"));
    assert!(has_katakana_commit, "Should commit as katakana after Enter");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_ctrl_k_with_empty_input() {
    let mut engine = InputMethodEngine::new();

    // No input, Ctrl+k should do nothing harmful
    let ctrl_k = KeyEvent {
        keysym: Keysym::KEY_K,
        modifiers: KeyModifiers {
            control_key: true,
            shift_key: false,
            alt_key: false,
            super_key: false,
        },
        is_press: true,
    };
    let result = engine.process_key(&ctrl_k);

    // Should not crash, state should remain empty
    assert!(matches!(engine.state(), InputState::Empty));
    // No commit action with empty text
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(!has_commit);
}

#[test]
fn test_ctrl_k_uppercase_converts_to_katakana() {
    let mut engine = InputMethodEngine::new();

    // Type "aiueo" -> "あいうえお"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    engine.process_key(&press('e'));
    engine.process_key(&press('o'));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");

    // Press Ctrl+K (uppercase K) -> should convert preedit to katakana
    let ctrl_k_upper = KeyEvent {
        keysym: Keysym::KEY_K_UPPER,
        modifiers: KeyModifiers {
            control_key: true,
            shift_key: false,
            alt_key: false,
            super_key: false,
        },
        is_press: true,
    };
    let result = engine.process_key(&ctrl_k_upper);

    assert!(result.consumed);
    // Should NOT commit yet
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)));
    assert!(!has_commit, "Should NOT commit on Ctrl+K");

    // Preedit should show katakana
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");
    assert!(
        engine.input_mode == InputMode::Katakana,
        "Should be in katakana mode"
    );

    // Type more → katakana mode persists (like alphabet mode)
    engine.process_key(&press('a'));
    assert!(
        engine.input_mode == InputMode::Katakana,
        "Katakana mode should persist across input"
    );
    // Preedit should show katakana for the new input too
    assert!(engine.preedit().unwrap().text().ends_with("ア"));
}

#[test]
fn test_katakana_baked_on_switch_to_alphabet() {
    let mut engine = InputMethodEngine::new();

    // Type "aiueo" → "あいうえお"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    engine.process_key(&press('e'));
    engine.process_key(&press('o'));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");

    // Ctrl+K → katakana mode, displays "アイウエオ"
    let ctrl_k = KeyEvent {
        keysym: Keysym::KEY_K,
        modifiers: KeyModifiers {
            control_key: true,
            shift_key: false,
            alt_key: false,
            super_key: false,
        },
        is_press: true,
    };
    engine.process_key(&ctrl_k);
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");

    // Switch to alphabet mode via Shift+L → katakana should be baked in
    engine.process_key(&press_shift('L'));
    assert!(engine.input_mode == InputMode::Alphabet);
    // The katakana text should be preserved, not reverted to hiragana
    assert_eq!(engine.input_buf.text, "アイウエオL");

    // Type alphabet chars → appended after katakana
    engine.process_key(&press('i'));
    engine.process_key(&press('n'));
    engine.process_key(&press('u'));
    engine.process_key(&press('x'));
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオLinux");
}

#[test]
fn test_ctrl_k_is_one_way_to_katakana() {
    let mut engine = InputMethodEngine::new();

    // Type "ai" → "あい"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Ctrl+K → katakana mode
    let ctrl_k = KeyEvent {
        keysym: Keysym::KEY_K,
        modifiers: KeyModifiers {
            control_key: true,
            shift_key: false,
            alt_key: false,
            super_key: false,
        },
        is_press: true,
    };
    engine.process_key(&ctrl_k);
    assert!(engine.input_mode == InputMode::Katakana);
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    // Ctrl+K again → still katakana mode (not a toggle)
    engine.process_key(&ctrl_k);
    assert!(engine.input_mode == InputMode::Katakana);
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    // Right Super → return to hiragana mode, katakana is baked in
    engine.process_key(&press_key(Keysym::SUPER_R));
    assert!(engine.input_mode == InputMode::Hiragana);
    assert_eq!(engine.input_buf.text, "アイ");
    assert_eq!(engine.preedit().unwrap().text(), "アイ");

    // New input in hiragana mode
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "アイう");
}
