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
