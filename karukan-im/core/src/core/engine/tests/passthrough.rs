use super::*;

#[test]
fn test_passthrough_no_double_counting() {
    // Regression test: typing '<' twice should produce "<<" in the preedit,
    // not "<<<" or "<<<<". The converter adds PassThrough chars to output()
    // AND returns them as PassThrough events; without proper handling, both
    // paths would insert the char.
    let mut engine = InputMethodEngine::new();

    // Type '<' from empty state → enters Composing with preedit "<"
    engine.process_key(&press('<'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "<");

    // Type '<' again → preedit becomes "<<", not "<<<"
    engine.process_key(&press('<'));
    assert_eq!(
        engine.preedit().unwrap().text(),
        "<<",
        "Second '<' should produce '<<', not over-count chars"
    );
}

#[test]
fn test_apostrophe_starts_input_mode() {
    // Regression for: typing `'` in empty state should enter Composing,
    // not auto-commit. This lets users type `'word'` or get symbol variants.
    let mut engine = InputMethodEngine::new();

    let result = engine.process_key(&press('\''));
    assert!(result.consumed);
    assert!(
        matches!(engine.state(), InputState::Composing { .. }),
        "Apostrophe should enter Composing, not auto-commit"
    );
    assert_eq!(engine.preedit().unwrap().text(), "'");

    // No Commit action should have fired.
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_))),
        "First apostrophe should not commit"
    );
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
