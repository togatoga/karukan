//! What commits teach the history: picks told apart from the default that
//! was on screen.

use super::*;

/// The words of きょうはいしゃにいく, enough to cut its conversions.
fn sentence_dict() -> Dictionary {
    dict_from_json(
        r#"[
            {"reading":"きょう","candidates":[{"surface":"今日","score":1.0}]},
            {"reading":"は","candidates":[{"surface":"は","score":1.0}]},
            {"reading":"いしゃ","candidates":[{"surface":"医者","score":1.0},{"surface":"慰謝","score":2.0}]},
            {"reading":"に","candidates":[{"surface":"に","score":1.0}]},
            {"reading":"いく","candidates":[{"surface":"行く","score":1.0},{"surface":"逝く","score":2.0}]}
        ]"#,
    )
}

/// Live conversion on, an empty history, the sentence's words, and the
/// model stood in for by the conversion cache: 今日は医者に行く first,
/// 今日は慰謝に行く as its alternative.
fn sentence_engine() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(sentence_dict()));
    seed_model_cache(
        &mut engine,
        "キョウハイシャニイク",
        "",
        &["今日は医者に行く", "今日は慰謝に行く"],
    );
    seed_model_cache(&mut engine, "イシャニ", "今日は", &["医者に", "慰謝に"]);
    engine
}

fn committed(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

fn score(engine: &InputMethodEngine, reading: &str, surface: &str, preceding: &str) -> f64 {
    engine
        .learning_cache()
        .unwrap()
        .score(reading, surface, preceding, "")
}

/// The score with no context matching: what holds anywhere.
fn base(engine: &InputMethodEngine, reading: &str, surface: &str) -> f64 {
    score(engine, reading, surface, "〓〓")
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn picking_another_candidate_is_chosen_and_the_default_displaced() {
    let mut engine = sentence_engine();
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.candidates().unwrap().selected_text(),
        Some("今日は慰謝に行く")
    );
    engine.process_key(&press_key(Keysym::RETURN));

    let whole = "きょうはいしゃにいく";
    assert_near(base(&engine, whole, "今日は慰謝に行く"), 1.0);
    assert_near(base(&engine, whole, "今日は医者に行く"), -1.0);
    // The one word that changed is taught too, at half weight, with the
    // reading before it as its context.
    // The word that changed, without the に both share, as a rule with
    // the readings around it.
    assert_near(base(&engine, "いしゃ", "慰謝"), 1.0);
    let cache = engine.learning_cache().unwrap();
    assert_near(cache.score("いしゃ", "慰謝", "きょうは", "にいく"), 2.0);
    assert_eq!(cache.rules("いしゃ")[0].right, "にいく");
    drop(cache);
    assert_near(base(&engine, "いしゃ", "医者"), -1.0);
    assert_eq!(base(&engine, "きょうは", "今日は"), 0.0);
}

#[test]
fn keeping_the_default_after_looking_is_confirmed() {
    let mut engine = sentence_engine();
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RETURN));

    let whole = "きょうはいしゃにいく";
    assert_near(base(&engine, whole, "今日は医者に行く"), 0.4);
    assert_eq!(base(&engine, whole, "今日は慰謝に行く"), 0.0);
    assert_eq!(base(&engine, "いしゃに", "医者に"), 0.0);
}

#[test]
fn enter_on_the_live_conversion_accepts_the_whole_and_makes_no_chunk_units() {
    let mut engine = sentence_engine();
    seed_model_cache(&mut engine, "キョウハ", "", &["今日は"]);
    seed_model_cache(&mut engine, "イシャニイク", "今日は", &["医者に行く"]);
    type_keys(&mut engine, "kyouha");
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    type_keys(&mut engine, "ishaniiku");
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("今日は医者に行く"));

    // The whole is kept for prediction, as only `accepted`.
    assert_near(
        base(&engine, "きょうはいしゃにいく", "今日は医者に行く"),
        0.1,
    );
    // The chunks are the model's output: committing them as shown makes
    // no unit of them.
    let cache = engine.learning_cache().unwrap();
    assert!(!cache.contains_reading("きょうは"));
    assert!(!cache.contains_reading("いしゃにいく"));
    drop(cache);

    // A chunk that is learned already is kept fresh, in its context.
    engine.learning_cache().unwrap().record(
        "いしゃにいく",
        "医者に行く",
        "きょうは",
        "",
        Evidence::Confirmed,
        1.0,
    );
    type_keys(&mut engine, "kyouha");
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    type_keys(&mut engine, "ishaniiku");
    engine.process_key(&press_key(Keysym::RETURN));
    assert_near(
        score(&engine, "いしゃにいく", "医者に行く", "きょうは"),
        1.0,
    );
}

#[test]
fn ctrl_digit_while_composing_is_told_apart_from_the_live_conversion() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[{"reading":"きのう","candidates":[{"surface":"昨日","score":1.0},{"surface":"機能","score":2.0}]}]"#,
    )));
    seed_model_cache(&mut engine, "キノウ", "", &["機能"]);
    type_keys(&mut engine, "kinou");
    let shown: Vec<String> = engine
        .shown_suggestions
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(shown[..2], ["機能", "昨日"]);

    engine.process_key(&press_ctrl(Keysym(b'2' as u32)));
    assert_near(base(&engine, "きのう", "昨日"), 1.0);
    assert_near(base(&engine, "きのう", "機能"), -1.0);
    assert_eq!(learned(&engine, "きのう"), ["昨日"]);
}

#[test]
fn a_split_conversion_teaches_the_segments_that_were_focused() {
    let mut engine = sentence_engine();
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    // → from the whole lands on the second segment, いしゃに.
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.state().reading(), Some("いしゃに"));
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("今日は慰謝に行く"));

    assert_near(base(&engine, "いしゃ", "慰謝"), 1.0);
    assert_near(base(&engine, "いしゃ", "医者"), -1.0);
    // In the context of the segment before it, by its reading.
    assert_near(score(&engine, "いしゃ", "慰謝", "きょうは"), 2.0);
    // Never focused: a guess, and nothing learned.
    assert!(
        !engine
            .learning_cache()
            .unwrap()
            .contains_reading("きょうは")
    );
    assert!(!engine.learning_cache().unwrap().contains_reading("いく"));
    // The whole against what the conversion opened with.
    let whole = "きょうはいしゃにいく";
    assert_near(base(&engine, whole, "今日は慰謝に行く"), 1.0);
    assert_near(base(&engine, whole, "今日は医者に行く"), -1.0);
}

#[test]
fn a_segment_kept_as_cut_is_confirmed() {
    let mut engine = sentence_engine();
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RETURN));

    assert_near(base(&engine, "いしゃに", "医者に"), 0.4);
    assert_near(
        base(&engine, "きょうはいしゃにいく", "今日は医者に行く"),
        0.4,
    );
}

fn list_texts(engine: &InputMethodEngine) -> Vec<String> {
    engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect()
}

#[test]
fn the_history_is_read_in_the_context_typed_in() {
    for (before, expected) in [("質問へ", "回答"), ("試験の", "解答")] {
        let mut engine = engine_with_learned("__none__", "__none__");
        let mut cache = engine.learning_cache().unwrap();
        cache.record("かいとう", "解答", "試験の", "", Evidence::Chosen, 1.0);
        cache.record("かいとう", "回答", "質問へ", "", Evidence::Chosen, 1.0);
        drop(cache);
        engine.set_surrounding_context(before, "");
        type_keys(&mut engine, "kaitou");
        engine.process_key(&press_key(Keysym::SPACE));
        assert_eq!(list_texts(&engine)[0], expected, "after {before}");
    }
}

#[test]
fn a_surface_replaced_more_than_picked_is_not_offered_from_the_history() {
    let mut engine = engine_with_learned("__none__", "__none__");
    let mut cache = engine.learning_cache().unwrap();
    cache.record("きのう", "機能", "", "", Evidence::Chosen, 1.0);
    cache.record("きのう", "機能", "", "", Evidence::Displaced, 2.0);
    drop(cache);
    type_keys(&mut engine, "kinou");
    engine.process_key(&press_key(Keysym::SPACE));
    let list = engine.candidates().unwrap();
    assert!(
        list.candidates()
            .iter()
            .all(|c| !(c.text == "機能" && c.source == Some(CandidateSource::Learning)))
    );
}

#[test]
fn the_model_alternatives_are_ordered_by_their_segments_history() {
    let mut engine = sentence_engine();
    seed_model_cache(
        &mut engine,
        "キョウハイシャニイク",
        "",
        &["今日は医者に行く", "今日は医者に逝く", "今日は慰謝に行く"],
    );
    // Too little to rewrite the live conversion, enough to order by.
    let mut cache = engine.learning_cache().unwrap();
    cache.record("いしゃに", "慰謝に", "", "", Evidence::Chosen, 0.25);
    drop(cache);
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    // The head is what was on screen; the alternatives follow the history.
    assert_eq!(
        list_texts(&engine)[..3],
        ["今日は医者に行く", "今日は慰謝に行く", "今日は医者に逝く"]
    );
}
