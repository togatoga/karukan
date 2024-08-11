use super::*;

#[test]
fn test_passthrough_no_double_counting() {
    // Regression test: typing '<' twice should produce "<<", not "<<<".
    // PassThrough chars are added to converter.output() AND returned as
    // PassThrough events. Without proper handling, both paths insert the char.
    let mut engine = InputMethodEngine::new();

    // Type '<' (Shift+,) in empty state → should commit directly
    let result = engine.process_key(&press('<'));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "<"));
    assert!(has_commit, "First '<' should commit '<'");

    // Type '<' again in empty state → should commit directly again
    let result = engine.process_key(&press('<'));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "<"));
    assert!(has_commit, "Second '<' should commit '<'");
}

#[test]
fn test_thx_chars_not_lost() {
    // Regression test: typing "thx" should show "thx" in preedit, not lose chars.
    // The converter recursively passes through 't' and 'h', keeps 'x' in buffer.
    // The engine must pick up ALL chars from output delta, not just the last PassThrough.
    let mut engine = InputMethodEngine::new();

    // Type 't'
    engine.process_key(&press('t'));
    assert_eq!(engine.preedit().unwrap().text(), "t");

    // Type 'h'
    engine.process_key(&press('h'));
    assert_eq!(engine.preedit().unwrap().text(), "th");

    // Type 'x' → converter breaks "thx" into output="th" + buffer="x"
    engine.process_key(&press('x'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "thx", "Should show 'thx', not lose characters");

    // Commit should produce "thx"
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "thx"));
    assert!(has_commit, "Should commit 'thx'");
}

#[test]
fn test_passthrough_after_hiragana_no_double() {
    // Typing hiragana then '<' should append exactly one '<', not two
    let mut engine = InputMethodEngine::new();

    // Type "あ" (a)
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Type '<' while in hiragana input state
    engine.process_key(&press('<'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "あ<", "Should be 'あ<', not 'あ<<'");

    // Type another '<'
    engine.process_key(&press('<'));
    let preedit = engine.preedit().unwrap().text().to_string();
    assert_eq!(preedit, "あ<<", "Should be 'あ<<', not 'あ<<<'");
}

#[test]
fn test_digit_starts_input_mode() {
    // Typing a digit from Empty state should enter Composing,
    // not commit immediately. This allows typing "20世紀" etc.
    let mut engine = InputMethodEngine::new();

    // Type '2' from Empty state
    let result = engine.process_key(&press('2'));
    assert!(result.consumed);
    assert!(
        matches!(engine.state(), InputState::Composing { .. }),
        "Digit should enter Composing, not stay Empty"
    );
    assert_eq!(engine.preedit().unwrap().text(), "2");

    // Type '0'
    engine.process_key(&press('0'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "20");

    // Type "seiki" -> "20せいき"
    engine.process_key(&press('s'));
    engine.process_key(&press('e'));
    engine.process_key(&press('i'));
    engine.process_key(&press('k'));
    engine.process_key(&press('i'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "20せいき");

    // Commit should produce "20せいき"
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "20せいき"));
    assert!(has_commit, "Should commit '20せいき'");
}

#[test]
fn test_digit_in_middle_of_hiragana() {
    // Typing a digit while in Composing should keep the preedit
    let mut engine = InputMethodEngine::new();

    // Type "あ" then "2"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    engine.process_key(&press('2'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ2");
}
