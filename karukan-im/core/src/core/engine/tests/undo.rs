//! A Backspace right after a commit takes back what the commit taught and
//! still reaches the application.

use super::*;

/// Engine with an empty learning cache and no model.
fn learning_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    engine.learning = Some(std::sync::Arc::new(std::sync::Mutex::new(
        LearningCache::new(LearningConfig::default()),
    )));
    engine
}

/// The score with no context matching.
fn base(engine: &InputMethodEngine, reading: &str, surface: &str) -> f64 {
    engine
        .learning_cache()
        .unwrap()
        .score(reading, surface, "〓〓", "")
}

fn learned_at_all(engine: &InputMethodEngine, reading: &str) -> bool {
    engine.learning_cache().unwrap().contains_reading(reading)
}

/// Type `あい` and commit it with Enter.
fn commit_ai(engine: &mut InputMethodEngine) {
    type_keys(engine, "ai");
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_)))
    );
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn backspace_after_commit_removes_a_new_unit_and_passes_through() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);
    assert!(learned_at_all(&engine, "あい"));

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(
        !result.consumed,
        "the application still deletes the character"
    );
    assert!(!learned_at_all(&engine, "あい"));
}

#[test]
fn backspace_after_a_pick_takes_back_both_sides_of_it() {
    let mut engine = engine_with_learned("あい", "藍");
    let before = (base(&engine, "あい", "藍"), base(&engine, "あい", "あい"));

    type_keys(&mut engine, "ai");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(engine.candidates().unwrap().selected_text(), Some("藍"));
    // Pick something else: chosen for it, displaced for 藍.
    engine.process_key(&press_key(Keysym::SPACE));
    let picked = engine
        .candidates()
        .unwrap()
        .selected_text()
        .unwrap()
        .to_string();
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(base(&engine, "あい", "藍") < before.0);

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(base(&engine, "あい", "藍"), before.0);
    assert_eq!(base(&engine, "あい", &picked), 0.0);
    assert_eq!(base(&engine, "あい", "あい"), before.1);
}

#[test]
fn backspace_takes_back_what_a_rewrite_was_given() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"ごはん","candidates":[{"surface":"ご飯","score":1.0}]},
            {"reading":"を","candidates":[{"surface":"を","score":1.0}]},
            {"reading":"はし","candidates":[{"surface":"橋","score":1.0}]},
            {"reading":"で","candidates":[{"surface":"で","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(&mut engine, "ゴハンヲハシデ", "", &["ご飯を橋で"]);
    let mut cache = engine.learning_cache().unwrap();
    cache.record("はし", "箸", "ごはんを", "", Evidence::Chosen, 1.0);
    drop(cache);
    let before = base(&engine, "はし", "箸");

    type_keys(&mut engine, "gohanwohashide");
    assert_eq!(engine.preedit().unwrap().text(), "ご飯を箸で");
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(base(&engine, "はし", "箸") > before);
    assert!(learned_at_all(&engine, "ごはんをはしで"));

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(base(&engine, "はし", "箸"), before);
    assert!(!learned_at_all(&engine, "ごはんをはしで"));
}

#[test]
fn only_the_next_key_can_revert() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);

    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(learned_at_all(&engine, "あい"));
}

#[test]
fn a_second_backspace_reverts_nothing_more() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);
    let first = base(&engine, "あい", "あい");
    commit_ai(&mut engine);
    assert!(base(&engine, "あい", "あい") > first);

    engine.process_key(&press_key(Keysym::BACKSPACE));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(
        base(&engine, "あい", "あい"),
        first,
        "the unit before this commit stays"
    );
}

#[test]
fn modifier_presses_keep_the_window_open() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);

    engine.process_key(&press_key(Keysym::SHIFT_L));
    engine.process_key(&release_key(Keysym::SHIFT_L));
    engine.process_key(&press_shift_key(Keysym::BACKSPACE));
    assert!(!learned_at_all(&engine, "あい"));
}

#[test]
fn ctrl_backspace_is_the_applications_and_reverts_nothing() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);

    engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(learned_at_all(&engine, "あい"));
}

#[test]
fn reset_closes_the_window() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);

    engine.reset();
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(learned_at_all(&engine, "あい"));
}

#[test]
fn focus_out_commit_is_not_revertible() {
    let mut engine = learning_engine();
    type_keys(&mut engine, "ai");
    assert_eq!(engine.commit(), "あい");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(learned_at_all(&engine, "あい"));
}

#[test]
fn backspace_while_composing_edits_and_keeps_the_learning() {
    let mut engine = learning_engine();
    commit_ai(&mut engine);

    // The commit and the next composition: Backspace edits the composition.
    type_keys(&mut engine, "ka");
    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert!(learned_at_all(&engine, "あい"));
}
