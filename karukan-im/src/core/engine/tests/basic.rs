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
fn space_in_empty_state_passes_through() {
    // Regression: pressing Space with no composition in progress used
    // to enter Composing with a single half-width space as the preedit
    // (the romaji converter PassThrough'd ' ' into input_buf). The
    // user then had to Escape or Enter to recover. The IME should not
    // intercept a bare Space when there's nothing being composed —
    // let it reach the application as a normal ASCII space. The
    // full-width space gesture remains Ctrl+Space.
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(
        !result.consumed,
        "Space in Empty state should not be consumed"
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
    // Sanity check that the Empty-state pass-through doesn't change
    // the Composing-state behavior: Space inside an existing
    // composition still acts as the conversion trigger.
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
