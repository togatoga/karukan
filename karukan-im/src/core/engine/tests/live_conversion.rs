use super::*;

// --- Live conversion tests ---

#[test]
fn test_live_conversion_disabled_by_default() {
    let engine = InputMethodEngine::new();
    assert!(!engine.live.enabled);
}

#[test]
fn test_live_conversion_enabled() {
    let engine = make_live_conversion_engine();
    assert!(engine.live.enabled);
}

#[test]
fn test_live_conversion_off_unchanged() {
    // With live_conversion=false, auto-suggest should show candidates (existing behavior)
    let mut engine = InputMethodEngine::new();
    assert!(!engine.live.enabled);

    // Type "ai" -> "あい" (standard hiragana preedit)
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.preedit().unwrap().text(), "あい");
    // live_conversion_text should be empty
    assert!(engine.live.text.is_empty());
}

#[test]
fn test_live_conversion_escape_shows_hiragana() {
    // Test that Escape clears live conversion text and shows hiragana
    let mut engine = make_live_conversion_engine();

    // Type "ai" -> "あい"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Simulate live conversion being active
    engine.live.text = "愛".to_string();

    // Press Escape -> should clear live_conversion_text and show hiragana
    let result = engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(result.consumed);
    assert!(engine.live.text.is_empty());
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あい");
}

#[test]
fn test_live_conversion_escape_twice_cancels() {
    // Test that double Escape cancels input
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Set live conversion text
    engine.live.text = "愛".to_string();

    // First Escape: clears live conversion, shows hiragana
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(engine.live.text.is_empty());

    // Second Escape: cancels input entirely
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_live_conversion_commit_with_converted_text() {
    // Test that Enter commits the live conversion text
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Simulate live conversion
    engine.live.text = "愛".to_string();

    // Press Enter -> should commit "愛", not "あい"
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);

    let commit_text = result
        .actions
        .iter()
        .find_map(|a| {
            if let EngineAction::Commit(text) = a {
                Some(text.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(commit_text, "愛");
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.live.text.is_empty());
}

#[test]
fn test_commit_composing_hides_candidate_window() {
    // Committing from Composing (Enter) must close the auto-suggest/live
    // conversion candidate window. The macOS frontend only closes its
    // NSPanel on an explicit hide_candidates action, so a commit without
    // it leaves a stale window on screen.
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.live.text = "愛".to_string();

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::HideCandidates)),
        "commit from Composing must emit HideCandidates"
    );
}

#[test]
fn test_live_conversion_commit_empty_falls_back_to_hiragana() {
    // When live_conversion_text is empty, commit should use hiragana
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    assert!(engine.live.text.is_empty());

    let result = engine.process_key(&press_key(Keysym::RETURN));
    let commit_text = result
        .actions
        .iter()
        .find_map(|a| {
            if let EngineAction::Commit(text) = a {
                Some(text.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(commit_text, "あ");
}

#[test]
fn test_live_conversion_cursor_move_clears() {
    // Moving cursor should clear live conversion text
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.live.text = "愛".to_string();

    // Left arrow clears live conversion
    engine.process_key(&press_key(Keysym::LEFT));
    assert!(engine.live.text.is_empty());
}

#[test]
fn test_live_conversion_build_preedit() {
    // Test build_composing_preedit constructs correct display for live conversion
    let mut engine = make_live_conversion_engine();

    engine.live.text = "漢字".to_string();

    let preedit = engine.build_composing_preedit();
    assert_eq!(preedit.text(), "漢字");
    assert_eq!(preedit.caret(), 2); // 漢字 = 2 chars
}

#[test]
fn test_alphabet_mode_with_kana_keeps_converting() {
    // Live conversion must stay alive in alphabet mode as long as the buffer
    // still contains kana. Type hiragana, switch to alphabet mode, keep typing:
    // the mixed reading (e.g. `あAb`) must keep being reconverted instead of
    // freezing at a stale live.text.
    let mut engine = make_live_conversion_engine();

    // "あ" then Shift+letter switches into alphabet mode -> buffer "あA"
    engine.process_key(&press('a'));
    engine.process_key(&press_shift('A'));
    assert!(engine.input_mode == InputMode::Alphabet);
    assert!(karukan_engine::contains_kana(&engine.input_buf.text));

    // Simulate a previous live conversion result lingering on screen.
    engine.live.text = "亜A".to_string();

    // Typing another latin char re-runs refresh_input_state. Because the buffer
    // still has kana, the "preserve display" early-return is bypassed and
    // conversion runs again; with no model loaded run_auto_suggest returns the
    // reading itself, so live.text is cleared rather than frozen.
    engine.process_key(&press('b'));
    assert!(
        engine.live.text.is_empty(),
        "mixed kana buffer must reconvert in alphabet mode, not preserve stale live.text"
    );
}

#[test]
fn test_alphabet_mode_pure_latin_preserves_live_text() {
    // Regression guard for the original behavior: with no kana in the buffer,
    // alphabet mode preserves an existing live.text display without re-running
    // conversion (raw latin has nothing for the model to convert).
    let mut engine = make_live_conversion_engine();

    // Enter alphabet mode with pure latin "Ab".
    engine.process_key(&press_shift('A'));
    engine.process_key(&press('b'));
    assert!(engine.input_mode == InputMode::Alphabet);
    assert!(!karukan_engine::contains_kana(&engine.input_buf.text));

    engine.live.text = "AB".to_string();

    // Another latin char keeps the preserved live.text (no reconversion).
    engine.process_key(&press('c'));
    assert_eq!(engine.live.text, "AB");
}

// --- Ctrl+Space full-width space tests ---

#[test]
fn test_ctrl_space_inserts_fullwidth_space_in_empty() {
    let mut engine = InputMethodEngine::new();

    // Ctrl+Space in Empty state -> start input with full-width space
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "\u{3000}");
}

#[test]
fn test_ctrl_space_inserts_fullwidth_space_in_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Type "あ"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Ctrl+Space -> insert full-width space
    let result = engine.process_key(&press_ctrl(Keysym::SPACE));
    assert!(result.consumed);
    assert_eq!(engine.preedit().unwrap().text(), "あ\u{3000}");
}

#[test]
fn test_ctrl_space_fullwidth_space_commit() {
    let mut engine = InputMethodEngine::new();

    // Type "あ" + fullwidth space
    engine.process_key(&press('a'));
    engine.process_key(&press_ctrl(Keysym::SPACE));

    // Enter to commit
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let commit_text = result
        .actions
        .iter()
        .find_map(|a| {
            if let EngineAction::Commit(text) = a {
                Some(text.clone())
            } else {
                None
            }
        })
        .unwrap();
    assert_eq!(commit_text, "あ\u{3000}");
}

// --- Ctrl+Shift+L live conversion toggle tests ---

#[test]
fn test_ctrl_shift_l_toggles_live_conversion() {
    let mut engine = InputMethodEngine::new();
    assert!(!engine.live.enabled);

    // Ctrl+Shift+L → toggle ON
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    assert!(result.consumed);
    assert!(engine.live.enabled);

    // Ctrl+Shift+L again → toggle OFF
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    assert!(result.consumed);
    assert!(!engine.live.enabled);
}

#[test]
fn test_ctrl_shift_l_lowercase_toggles() {
    let mut engine = InputMethodEngine::new();
    assert!(!engine.live.enabled);

    // Ctrl+Shift+l (lowercase keysym) → toggle ON
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L));
    assert!(result.consumed);
    assert!(engine.live.enabled);
}

#[test]
fn test_toggle_on_during_composing_applies_immediately() {
    // Toggling live conversion ON while composing should immediately attempt
    // live conversion against the current input buffer instead of waiting for
    // another keystroke. With no model loaded, run_auto_suggest falls back to
    // the reading itself (which equals input_buf.text), so live.text stays
    // empty — but the preedit must still be refreshed in a single action set.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert!(!engine.live.enabled);

    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    assert!(result.consumed);
    assert!(engine.live.enabled);

    // The toggle must produce a preedit refresh, not only an aux update.
    let has_preedit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::UpdatePreedit(_)));
    assert!(
        has_preedit,
        "toggling ON during composing should refresh preedit immediately"
    );
}

#[test]
fn test_toggle_off_during_composing_clears_live_text() {
    // Toggling OFF while live conversion is showing should revert the preedit
    // back to hiragana without requiring another keystroke.
    let mut engine = make_live_conversion_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.live.text = "愛".to_string();

    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    assert!(result.consumed);
    assert!(!engine.live.enabled);
    assert!(engine.live.text.is_empty());

    let preedit_text = result.actions.iter().find_map(|a| {
        if let EngineAction::UpdatePreedit(p) = a {
            Some(p.text().to_string())
        } else {
            None
        }
    });
    assert_eq!(preedit_text.as_deref(), Some("あい"));
}

#[test]
fn test_engine_config_live_conversion_enabled() {
    use crate::core::engine::EngineConfig;
    let config = EngineConfig {
        live_conversion: true,
        ..EngineConfig::default()
    };
    let engine = InputMethodEngine::with_config(config);
    assert!(engine.live.enabled);
}

#[test]
fn test_ctrl_shift_l_shows_aux_text() {
    let mut engine = InputMethodEngine::new();

    // Ctrl+Shift+L → check aux text shows "ライブ変換: ON"
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    let has_aux = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::UpdateAuxText(text) if text.contains("ライブ変換: ON")));
    assert!(has_aux);

    // Ctrl+Shift+L again → "ライブ変換: OFF"
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    let has_aux = result.actions.iter().any(
        |a| matches!(a, EngineAction::UpdateAuxText(text) if text.contains("ライブ変換: OFF")),
    );
    assert!(has_aux);
}
