//! Settings switched by typing their name: converting でばっぐもーど
//! offers a candidate that flips the setting and inputs nothing.

use super::*;

fn type_keys(engine: &mut InputMethodEngine, keys: &str) {
    for ch in keys.chars() {
        engine.process_key(&press(ch));
    }
}

/// The candidate list after typing `keys` and pressing Space.
fn convert(engine: &mut InputMethodEngine, keys: &str) -> CandidateList {
    type_keys(engine, keys);
    engine.process_key(&press_key(Keysym::SPACE));
    engine.candidates().expect("converting").clone()
}

fn commits(result: &EngineResult) -> bool {
    result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(_)))
}

#[test]
fn the_setting_is_offered_second() {
    // The plain conversion stays first, so writing the word is unchanged.
    let mut engine = InputMethodEngine::new();
    let list = convert(&mut engine, "debaggumo-do");
    let rows = list.candidates();
    assert_ne!(rows[0].source, Some(CandidateSource::Command), "{rows:?}");
    assert_eq!(rows[1].text, "【デバッグモードをオン】");
    assert_eq!(rows[1].source, Some(CandidateSource::Command));
}

#[test]
fn choosing_it_flips_the_setting_and_inputs_nothing() {
    let mut engine = InputMethodEngine::new();
    engine.learning = Some(std::sync::Arc::new(std::sync::Mutex::new(
        LearningCache::new(LearningConfig::default()),
    )));
    assert!(!engine.config.verbose);

    convert(&mut engine, "debaggumo-do");
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(!commits(&result), "{:?}", result.actions);
    assert!(engine.config.verbose);
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(
        last_aux_text(&result).as_deref(),
        Some("デバッグモード: オン")
    );
    assert!(
        engine
            .learning_cache()
            .unwrap()
            .lookup("でばっぐもーど", "", "")
            .is_empty()
    );

    // Offered again, it turns the setting back off.
    let list = convert(&mut engine, "debaggumo-do");
    assert_eq!(list.candidates()[1].text, "【デバッグモードをオフ】");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(!engine.config.verbose);
}

#[test]
fn the_common_misspelling_works_too() {
    let mut engine = InputMethodEngine::new();
    let list = convert(&mut engine, "debakkumo-do");
    assert_eq!(list.candidates()[1].source, Some(CandidateSource::Command));
}

#[test]
fn choosing_it_by_number_works_too() {
    let mut engine = InputMethodEngine::new();
    convert(&mut engine, "debaggumo-do");
    let result = engine.process_key(&press_ctrl(Keysym::KEY_2));
    assert!(!commits(&result), "{:?}", result.actions);
    assert!(engine.config.verbose);
}

#[test]
fn other_readings_offer_no_setting() {
    let mut engine = InputMethodEngine::new();
    let list = convert(&mut engine, "debaggu");
    assert!(
        !list
            .candidates()
            .iter()
            .any(|c| c.source == Some(CandidateSource::Command))
    );
}

#[test]
fn ctrl_shift_v_is_left_to_the_application() {
    // A terminal pastes with it; no chord is taken for the setting, with
    // nothing typed or in the middle of typing.
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_V_UPPER));
    assert!(!result.consumed);

    type_keys(&mut engine, "ai");
    let result = engine.process_key(&press_ctrl_shift(Keysym::KEY_V_UPPER));
    assert!(!result.consumed);
    assert!(!engine.config.verbose);
}

#[test]
fn the_confirmation_comes_in_an_empty_panel() {
    // On macOS the candidate panel is what draws the aux line, so hiding it
    // would hide the confirmation too.
    let mut engine = InputMethodEngine::new();
    convert(&mut engine, "debaggumo-do");
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let shows_empty = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::ShowCandidates(list) if list.is_empty()));
    let hides = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::HideCandidates));
    assert!(shows_empty && !hides, "{:?}", result.actions);
}

#[test]
fn deleting_a_learned_candidate_keeps_the_setting() {
    // Ctrl+Backspace rebuilds the list in place; the setting stays second.
    let mut engine = engine_with_learned("でばっぐもーど", "でばっぐモード");
    let list = convert(&mut engine, "debaggumo-do");
    assert_eq!(list.candidates()[0].source, Some(CandidateSource::Learning));

    engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    let rows = engine.candidates().expect("still converting").candidates();
    assert!(
        !rows
            .iter()
            .any(|c| c.source == Some(CandidateSource::Learning)),
        "{rows:?}"
    );
    assert_eq!(rows[1].source, Some(CandidateSource::Command), "{rows:?}");
}

#[test]
fn leaving_the_field_types_nothing_and_keeps_the_setting() {
    // The frontends commit through commit() when focus leaves. The label is
    // never typed, and an unconfirmed setting stays as it was.
    let mut engine = InputMethodEngine::new();
    convert(&mut engine, "debaggumo-do");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.commit(), "");
    assert!(!engine.config.verbose);
    assert!(matches!(engine.state(), InputState::Empty));
}
