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
    assert!(engine.live_text().is_empty());
}

#[test]
fn test_live_conversion_escape_shows_hiragana() {
    // Test that Escape clears live conversion text and shows hiragana
    let mut engine = make_live_conversion_engine();

    // Type "ai" -> "あい"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Simulate live conversion being active
    set_live_text(&mut engine, "愛");

    // Press Escape -> should clear live_conversion_text and show hiragana
    let result = engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(result.consumed);
    assert!(engine.live_text().is_empty());
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
    set_live_text(&mut engine, "愛");

    // First Escape: clears live conversion, shows hiragana
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(engine.live_text().is_empty());

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
    set_live_text(&mut engine, "愛");

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
    assert!(engine.live_text().is_empty());
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
    set_live_text(&mut engine, "愛");

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
    assert!(engine.live_text().is_empty());

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
fn test_live_conversion_commit_keeps_pending_tail() {
    // "wasedad" + Enter without any Space: the display was 早稲田 + pending
    // `d`, so the commit must be 早稲田d, not 早稲田.
    let mut engine = make_live_conversion_engine();
    for ch in "wasedad".chars() {
        engine.process_key(&press(ch));
    }
    set_live_text(&mut engine, "早稲田");

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
    assert_eq!(commit_text, "早稲田d");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_engine_commit_keeps_pending_tail() {
    // Same as above through the programmatic commit() path (focus-out).
    let mut engine = make_live_conversion_engine();
    for ch in "wasedad".chars() {
        engine.process_key(&press(ch));
    }
    set_live_text(&mut engine, "早稲田");

    assert_eq!(engine.commit(), "早稲田d");
}

#[test]
fn test_conversion_keeps_pending_tail_of_live_candidate() {
    // "wasedad": the live suggestion converts only the settled reading
    // (わせだ→早稲田) and the pending `d` is displayed after it. Starting a
    // conversion must surface 早稲田d — not 早稲田 — as the preserved top
    // candidate, and commit it whole.
    let mut engine = make_live_conversion_engine();
    for ch in "wasedad".chars() {
        engine.process_key(&press(ch));
    }
    set_live_text(&mut engine, "早稲田");

    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "早稲田d");

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
    assert_eq!(commit_text, "早稲田d");
}

#[test]
fn test_commit_mid_buffer_ignores_live_text() {
    // Typing away from the end shows the kana display (live text is not
    // faithful there), so Enter must commit what is shown — あdい — and not
    // splice the live text with the mid-buffer pending run (愛d).
    let mut engine = make_live_conversion_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press('d'));
    assert_eq!(engine.preedit().unwrap().text(), "あdい");
    set_live_text(&mut engine, "愛");

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
    assert_eq!(commit_text, "あdい");
}

#[test]
fn test_live_conversion_cursor_move_clears() {
    // Moving cursor should clear live conversion text
    let mut engine = make_live_conversion_engine();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    set_live_text(&mut engine, "愛");

    // Left arrow clears live conversion
    engine.process_key(&press_key(Keysym::LEFT));
    assert!(engine.live_text().is_empty());
}

#[test]
fn test_live_conversion_build_preedit() {
    // Test build_composing_preedit constructs correct display for live conversion
    let mut engine = make_live_conversion_engine();

    set_live_text(&mut engine, "漢字");

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
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert!(karukan_engine::contains_kana(&engine.input_buf.reading()));

    // Simulate a previous live conversion result lingering on screen.
    set_live_text(&mut engine, "亜A");

    // Typing another latin char re-runs refresh_input_state. Because the buffer
    // still has kana, the "preserve display" early-return is bypassed and the
    // buffer is re-chunked and reconverted (the converted text depends on the
    // loaded model, so assert on the chunk readings, not the output).
    engine.process_key(&press('b'));
    let rechunked: String = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(
        rechunked, "あAb",
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
    assert!(engine.mode.current() == InputMode::Alphabet);
    assert!(!karukan_engine::contains_kana(&engine.input_buf.reading()));

    set_live_text(&mut engine, "AB");

    // Another latin char keeps the preserved live.text (no reconversion).
    engine.process_key(&press('c'));
    assert_eq!(engine.live_text(), "AB");
}

// --- Shift+Space space insertion tests ---

#[test]
fn test_shift_space_alone_commits_a_fullwidth_space() {
    // The exception to the setting, committed directly rather than opening
    // a composition a second Space would convert.
    for mut engine in [InputMethodEngine::new(), fullwidth_space_engine()] {
        let result = engine.process_key(&press_shift_key(Keysym::SPACE));
        assert!(result.consumed);
        assert!(matches!(engine.state(), InputState::Empty));
        let commit = result.actions.iter().find_map(|a| match a {
            EngineAction::Commit(text) => Some(text.clone()),
            _ => None,
        });
        assert_eq!(commit.as_deref(), Some("\u{3000}"));
    }
}

#[test]
fn test_shift_space_inserts_the_configured_space_into_a_composition() {
    // Bare Space converts here, so this chord is the only way to put a
    // space into a composition — and it takes the everyday width, so a
    // half-width space can be part of what gets converted.
    for (mut engine, expected) in [
        (InputMethodEngine::new(), "あ "),
        (fullwidth_space_engine(), "あ\u{3000}"),
    ] {
        engine.process_key(&press('a'));
        engine.process_key(&press_shift_key(Keysym::SPACE));
        assert_eq!(engine.preedit().unwrap().text(), expected);

        let result = engine.process_key(&press_key(Keysym::RETURN));
        let commit = result.actions.iter().find_map(|a| match a {
            EngineAction::Commit(text) => Some(text.clone()),
            _ => None,
        });
        assert_eq!(commit.as_deref(), Some(expected));
    }
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
    set_live_text(&mut engine, "愛");

    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_L_UPPER));
    assert!(result.consumed);
    assert!(!engine.live.enabled);
    assert!(engine.live_text().is_empty());

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
