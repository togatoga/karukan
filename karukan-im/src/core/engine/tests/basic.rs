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
fn space_in_empty_hiragana_inserts_fullwidth_space() {
    // Regression: pressing Space with no composition in progress used
    // to enter Composing with a literal *half-width* space as the
    // preedit (the romaji converter PassThrough'd ' ' into input_buf).
    // mozc-compat behavior is to insert a *full-width* `　` and enter
    // Composing in kana modes — the typical Japanese-IME expectation.
    let mut engine = InputMethodEngine::new();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "\u{3000}");
}

#[test]
fn space_in_empty_katakana_inserts_fullwidth_space() {
    // Katakana is a kana mode too — same full-width behavior as
    // Hiragana when Space is pressed from Empty.
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Katakana;

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "\u{3000}");
}

#[test]
fn space_in_empty_alphabet_passes_through() {
    // In Alphabet mode the user is typing ASCII — injecting `　` would
    // be wrong. Return not_consumed so the OS delivers a normal
    // half-width space to the application.
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Alphabet;

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(
        !result.consumed,
        "Space in Empty+Alphabet should not be consumed"
    );
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
