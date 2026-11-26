//! A Backspace right after a commit takes back what the commit taught
//! (Mozc's Revert) and still reaches the application.

use super::*;

const WHOLE: &str = "きょうはいしゃにいく";

/// An empty history, the words of きょうはいしゃにいく, and the model
/// stood in for by the conversion cache: 今日は医者に行く first,
/// 今日は慰謝に行く as its alternative.
fn sentence_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"きょう","candidates":[{"surface":"今日","score":1.0}]},
            {"reading":"は","candidates":[{"surface":"は","score":1.0}]},
            {"reading":"いしゃ","candidates":[{"surface":"医者","score":1.0},{"surface":"慰謝","score":2.0}]},
            {"reading":"に","candidates":[{"surface":"に","score":1.0}]},
            {"reading":"いく","candidates":[{"surface":"行く","score":1.0},{"surface":"逝く","score":2.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "キョウハイシャニイク",
        "",
        &["今日は医者に行く", "今日は慰謝に行く"],
    );
    seed_model_cache(&mut engine, "イシャニ", "今日は", &["医者に", "慰謝に"]);
    engine
}

fn cache(engine: &InputMethodEngine) -> &LearningCache {
    engine.learning.as_ref().unwrap()
}

/// Type the sentence and select the model's alternative: a choice that
/// teaches several units (the whole, the stretch いしゃに, and the defaults
/// it displaced).
fn select_alternative(engine: &mut InputMethodEngine) {
    type_keys(engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.candidates().unwrap().selected_text(),
        Some("今日は慰謝に行く")
    );
}

fn pick_alternative(engine: &mut InputMethodEngine) {
    select_alternative(engine);
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(cache(engine).contains_reading(WHOLE));
    assert!(cache(engine).contains_reading("いしゃに"));
}

fn nothing_learned(engine: &InputMethodEngine) -> bool {
    !cache(engine).contains_reading(WHOLE) && !cache(engine).contains_reading("いしゃに")
}

#[test]
fn backspace_after_commit_takes_back_every_unit_it_taught_and_passes_through() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(
        !result.consumed,
        "the application still deletes the character"
    );
    assert!(nothing_learned(&engine));
}

#[test]
fn backspace_after_commit_restores_units_that_were_there() {
    let mut engine = sentence_engine();
    engine
        .learning
        .as_mut()
        .unwrap()
        .record(WHOLE, "今日は慰謝に行く", "", Evidence::Chosen, 1.0);
    let before = |e: &InputMethodEngine| {
        (
            cache(e).score(WHOLE, "今日は慰謝に行く", ""),
            cache(e).score(WHOLE, "今日は医者に行く", ""),
        )
    };
    let prior = before(&engine);

    // The history heads the list now: keeping it teaches the same units.
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.candidates().unwrap().selected_text(),
        Some("今日は慰謝に行く")
    );
    engine.process_key(&press_key(Keysym::RETURN));
    assert_ne!(before(&engine), prior);

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(before(&engine), prior);
}

#[test]
fn only_the_next_key_can_revert() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(cache(&engine).contains_reading(WHOLE));
}

#[test]
fn a_second_backspace_reverts_nothing_more() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(nothing_learned(&engine));

    // The second commit is reverted alone; the first stays reverted and
    // nothing before it is touched.
    pick_alternative(&mut engine);
    let after_one = cache(&engine).score(WHOLE, "今日は慰謝に行く", "");
    engine.process_key(&press_key(Keysym::BACKSPACE));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(nothing_learned(&engine));
    assert!(after_one > 0.0);
}

#[test]
fn modifier_presses_keep_the_window_open() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    engine.process_key(&press_key(Keysym::SHIFT_L));
    engine.process_key(&release_key(Keysym::SHIFT_L));
    engine.process_key(&press_shift_key(Keysym::BACKSPACE));
    assert!(nothing_learned(&engine));
}

#[test]
fn ctrl_backspace_is_the_applications_and_reverts_nothing() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(cache(&engine).contains_reading(WHOLE));
}

#[test]
fn reset_closes_the_window() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    engine.reset();
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(cache(&engine).contains_reading(WHOLE));
}

#[test]
fn focus_out_commit_is_not_revertible() {
    let mut engine = sentence_engine();
    select_alternative(&mut engine);
    assert_eq!(engine.commit(), "今日は慰謝に行く");
    assert!(cache(&engine).contains_reading(WHOLE));

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(cache(&engine).contains_reading(WHOLE));
}

#[test]
fn backspace_while_composing_edits_and_keeps_the_learning() {
    let mut engine = sentence_engine();
    pick_alternative(&mut engine);

    type_keys(&mut engine, "ka");
    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert!(cache(&engine).contains_reading(WHOLE));
}
