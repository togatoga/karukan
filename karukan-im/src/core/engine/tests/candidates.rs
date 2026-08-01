use super::*;

fn seed_shown_predictions(engine: &mut InputMethodEngine, reading: &str, texts: &[&str]) {
    engine.input_buf.text = reading.to_string();
    engine.input_buf.cursor_pos = reading.chars().count();
    engine.state = InputState::Composing {
        preedit: Preedit::with_text_underlined(reading),
        romaji_buffer: String::new(),
    };
    engine.suggestions = Some(CandidateList::new(
        texts
            .iter()
            .map(|text| Candidate::with_reading(*text, reading))
            .collect(),
    ));
}

#[test]
fn tab_enters_the_exact_predictions_already_shown() {
    let mut engine = InputMethodEngine::new();
    seed_shown_predictions(&mut engine, "きょう", &["今日", "京都", "教"]);

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let candidates = engine.state().candidates().unwrap();
    let texts: Vec<_> = candidates
        .candidates()
        .iter()
        .map(|candidate| candidate.text.as_str())
        .collect();
    assert_eq!(texts, vec!["今日", "京都", "教"]);
    assert_eq!(candidates.cursor(), 0);
    assert_eq!(candidates.selected_text(), Some("今日"));
}

#[test]
fn repeated_tab_moves_to_the_next_prediction_and_enter_commits_it() {
    let mut engine = InputMethodEngine::new();
    seed_shown_predictions(&mut engine, "きょう", &["今日", "京都", "教"]);

    engine.process_key(&press_key(Keysym::TAB));
    let move_result = engine.process_key(&press_key(Keysym::TAB));
    let shown_cursor = move_result.actions.iter().find_map(|action| match action {
        EngineAction::ShowCandidates(list) => Some(list.cursor()),
        _ => None,
    });
    assert_eq!(shown_cursor, Some(1));
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("京都")
    );

    let commit_result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        commit_result
            .actions
            .iter()
            .any(|action| matches!(action, EngineAction::Commit(text) if text == "京都"))
    );
    assert!(engine.state().is_empty());
}

#[test]
fn test_suggest_result_preserved_in_start_conversion() {
    // When Space is pressed, the previous auto-suggest/live conversion result
    // should be preserved in the candidate list even if re-inference doesn't produce it.
    // (Without a kanji converter, build_conversion_candidates returns fallback only,
    // so the live_conversion_text would be lost without the preservation logic.)
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.live.text = "愛".to_string();

    // Press Space → start_conversion()
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // "愛" should be preserved in the candidate list
    let candidates = engine.state().candidates().unwrap();
    assert!(
        candidates.candidates().iter().any(|c| c.text == "愛"),
        "Previous suggest result '愛' should be preserved in candidates"
    );
}

#[test]
fn down_uses_the_same_prediction_selection_path_as_tab() {
    let mut engine = InputMethodEngine::new();
    seed_shown_predictions(&mut engine, "きょう", &["今日", "京都"]);

    let result = engine.process_key(&press_key(Keysym::DOWN));
    assert!(result.consumed);
    assert_eq!(
        engine.state().candidates().unwrap().selected_text(),
        Some("今日")
    );
}

#[test]
fn space_uses_explicit_conversion_instead_of_the_shown_predictions() {
    let mut engine = InputMethodEngine::new();
    seed_shown_predictions(&mut engine, "あい", &["予測専用"]);

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        !engine
            .state()
            .candidates()
            .unwrap()
            .candidates()
            .iter()
            .any(|candidate| candidate.text == "予測専用"),
        "Space must rebuild explicit conversion candidates"
    );
}

#[test]
fn tab_without_predictions_is_consumed_and_keeps_composing() {
    let mut engine = InputMethodEngine::new();
    seed_shown_predictions(&mut engine, "あい", &["unused"]);
    engine.suggestions = None;

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.text, "あい");
}
