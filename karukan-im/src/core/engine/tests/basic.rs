use super::*;

#[test]
fn test_engine_basic_input() {
    let mut engine = InputMethodEngine::new();

    // Type "a" -> "あ"
    let result = engine.process_key(&press('a'));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_engine_romaji_to_hiragana() {
    let mut engine = InputMethodEngine::new();

    // Type "ka" -> "か"
    engine.process_key(&press('k'));
    assert_eq!(engine.preedit().unwrap().text(), "k");

    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "か");
}

#[test]
fn test_engine_commit_composing() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.preedit().unwrap().text(), "あい");

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(result.consumed);

    // Check for commit action
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "あい"));
    assert!(has_commit);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_engine_backspace() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.preedit().unwrap().text(), "あい");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn space_in_empty_hiragana_commits_fullwidth_space() {
    // Bare Space from Empty in Hiragana mode commits a full-width `　`
    // directly without entering Composing — the Japanese-IME
    // convention, but without the side effect of "second Space starts
    // Conversion mode" that a Composing-state insertion would cause.
    let mut engine = InputMethodEngine::new();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    let committed = result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(committed.as_deref(), Some("\u{3000}"));
}

#[test]
fn double_space_in_empty_hiragana_commits_two_fullwidth_spaces() {
    // Regression for the conversion-mode-on-second-Space issue: two
    // consecutive Spaces from Empty must produce two committed `　`s,
    // never enter Composing, and never trigger Conversion.
    let mut engine = InputMethodEngine::new();
    for _ in 0..2 {
        let result = engine.process_key(&press_key(Keysym::SPACE));
        assert!(matches!(engine.state(), InputState::Empty));
        let committed = result.actions.iter().find_map(|a| match a {
            EngineAction::Commit(t) => Some(t.clone()),
            _ => None,
        });
        assert_eq!(committed.as_deref(), Some("\u{3000}"));
    }
}

#[test]
fn space_in_empty_katakana_passes_through() {
    // Non-Hiragana modes pass the bare Space through to the OS so the
    // application gets a normal half-width ASCII space.
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Katakana;

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(
        result.actions.is_empty(),
        "expected no actions, got {:?}",
        result.actions
    );
}

#[test]
fn space_in_empty_alphabet_passes_through() {
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Alphabet;

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(
        result.actions.is_empty(),
        "expected no actions, got {:?}",
        result.actions
    );
}

#[test]
fn space_after_composing_starts_still_triggers_conversion() {
    // Sanity check that the Empty-state change doesn't affect
    // Composing-state behavior: Space inside an existing composition
    // still acts as the conversion trigger.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn test_engine_cancel() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_pipeline_config_defaults() {
    // Verify pipeline config has sensible defaults
    let config = EngineConfig::default();
    assert_eq!(config.num_candidates, 3);
}

#[test]
fn test_truncate_context() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 5;

    // Short context - unchanged
    let short = engine.truncate_context("abc");
    assert_eq!(short, "abc");

    // Exact length - unchanged
    let exact = engine.truncate_context("abcde");
    assert_eq!(exact, "abcde");

    // Long context - truncated from the end
    let long = engine.truncate_context("abcdefghij");
    assert_eq!(long, "fghij"); // Last 5 chars

    // Japanese characters
    let jp = engine.truncate_context("今日はとても良い天気");
    assert_eq!(jp.chars().count(), 5); // Last 5 chars
}
