//! Tests for the overload guard as the engine uses it: a model resting
//! after an overrun is not called, typing carries on in kana, and what the
//! cache already knows is still shown.
//!
//! These run without a loaded model. The guard is put to rest by hand —
//! what rests it for real (a conversion past its deadline) is covered where
//! the deadline is enforced, in karukan-engine.

use std::time::Instant;

use super::*;

fn live_engine() -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        live_conversion: true,
        ..EngineConfig::default()
    })
}

fn rest_the_model(engine: &mut InputMethodEngine) {
    engine.guard.overran(ModelRole::Main, Instant::now());
}

fn type_keys(engine: &mut InputMethodEngine, keys: &str) -> EngineResult {
    let mut result = EngineResult::consumed();
    for ch in keys.chars() {
        result = engine.process_key(&press(ch));
    }
    result
}

#[test]
fn a_resting_model_is_shown_in_place_of_the_live_mark() {
    let mut engine = live_engine();
    let aux = last_aux_text(&type_keys(&mut engine, "a")).expect("aux");
    assert!(aux.starts_with("⚡[あ]"), "{aux}");

    rest_the_model(&mut engine);
    let aux = last_aux_text(&type_keys(&mut engine, "i")).expect("aux");
    assert!(aux.starts_with("💤[あ]"), "{aux}");
}

#[test]
fn the_mark_goes_when_the_model_is_back() {
    let mut engine = live_engine();
    rest_the_model(&mut engine);
    assert!(engine.model_is_resting());

    engine.guard.finished_in_time(ModelRole::Main);
    assert!(!engine.model_is_resting());
    let aux = last_aux_text(&type_keys(&mut engine, "a")).expect("aux");
    assert!(aux.starts_with("⚡[あ]"), "{aux}");
}

#[test]
fn the_mark_shows_without_live_conversion_too() {
    // Space asks the same resting model, so the reason is shown either way.
    let mut engine = InputMethodEngine::new();
    rest_the_model(&mut engine);
    let aux = last_aux_text(&type_keys(&mut engine, "a")).expect("aux");
    assert!(aux.starts_with("💤[あ]"), "{aux}");
}

#[test]
fn typing_stays_kana_while_the_model_rests() {
    let mut engine = live_engine();
    rest_the_model(&mut engine);
    type_keys(&mut engine, "kyou");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きょう"));
}

#[test]
fn a_resting_model_still_answers_from_the_cache() {
    // The cache is consulted before the guard: a conversion that already
    // ran costs nothing, so resting must not take it off the screen.
    let mut engine = live_engine();
    seed_model_cache(&mut engine, "キョウ", "", &["今日"]);
    rest_the_model(&mut engine);
    type_keys(&mut engine, "kyou");
    assert_eq!(engine.chunks[0].converted, "今日");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("今日"));
}

#[test]
fn the_dictionary_still_converts_while_the_model_rests() {
    let mut engine = live_engine();
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[{"reading": "きょう", "candidates": [{"surface": "今日", "score": 100.0}]}]"#,
    )));
    rest_the_model(&mut engine);
    type_keys(&mut engine, "kyou");
    engine.process_key(&press_key(Keysym::SPACE));
    let candidates = engine.state().candidates().expect("candidates");
    assert!(
        candidates.candidates().iter().any(|c| c.text == "今日"),
        "dictionary candidate missing while the model rests"
    );
}
