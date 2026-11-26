//! The history rewriting the live conversion.

use super::*;

/// Live conversion on, an empty history, and a model that writes
/// あんそろぴっく in katakana; the dictionary also has it in latin script.
fn anthropic_engine() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"あんそろぴっく","candidates":[{"surface":"アンソロピック","score":1.0},{"surface":"Anthropic","score":2.0}]},
            {"reading":"の","candidates":[{"surface":"の","score":1.0}]},
            {"reading":"もでる","candidates":[{"surface":"モデル","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(&mut engine, "アンソロピック", "", &["アンソロピック"]);
    seed_model_cache(
        &mut engine,
        "アンソロピックノモデル",
        "",
        &["アンソロピックのモデル"],
    );
    engine
}

fn preedit_text(engine: &InputMethodEngine) -> String {
    engine.preedit().unwrap().text().to_string()
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

/// Space, then move the selection to `text` and commit it.
fn pick(engine: &mut InputMethodEngine, text: &str) {
    engine.process_key(&press_key(Keysym::SPACE));
    while engine.candidates().unwrap().selected_text() != Some(text) {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    engine.process_key(&press_key(Keysym::RETURN));
}

#[test]
fn a_pick_rewrites_the_next_live_conversion_of_the_chunk() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    assert_eq!(preedit_text(&engine), "アンソロピック");
    pick(&mut engine, "Anthropic");

    type_keys(&mut engine, "ansoropikku");
    assert_eq!(preedit_text(&engine), "Anthropic");
    // Space opens on what is on screen, the model's own text right after.
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.candidates().unwrap().selected_text(),
        Some("Anthropic")
    );
    assert!(list_texts(&engine).contains(&"アンソロピック".to_string()));
}

#[test]
fn a_pick_rewrites_the_word_inside_a_sentence() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    pick(&mut engine, "Anthropic");

    // The model still writes the sentence in katakana; the segment that
    // reads as the learned word is replaced, its particle kept.
    type_keys(&mut engine, "ansoropikkunomoderu");
    assert_eq!(preedit_text(&engine), "Anthropicのモデル");
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "Anthropicのモデル"))
    );
    // Committed as shown, the rewrite is only accepted.
    let cache = engine.learning_cache().unwrap();
    assert!((cache.score("あんそろぴっく", "Anthropic", "〓〓") - 1.1).abs() < 1e-3);
}

#[test]
fn keeping_the_default_rewrites_nothing() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RETURN));
    for _ in 0..3 {
        type_keys(&mut engine, "ansoropikku");
        engine.process_key(&press_key(Keysym::RETURN));
    }
    type_keys(&mut engine, "ansoropikkunomoderu");
    assert_eq!(preedit_text(&engine), "アンソロピックのモデル");
}

#[test]
fn picking_the_model_text_back_undoes_the_rewrite() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    pick(&mut engine, "Anthropic");
    // The rewrite committed as shown twice gains only a little.
    for _ in 0..2 {
        type_keys(&mut engine, "ansoropikku");
        assert_eq!(preedit_text(&engine), "Anthropic");
        engine.process_key(&press_key(Keysym::RETURN));
    }

    type_keys(&mut engine, "ansoropikku");
    pick(&mut engine, "アンソロピック");
    type_keys(&mut engine, "ansoropikku");
    assert_eq!(preedit_text(&engine), "アンソロピック");
}

#[test]
fn the_context_picks_between_rewrites() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    let mut cache = engine.learning_cache().unwrap();
    cache.record("かいとう", "解答", "試験の", Evidence::Chosen, 1.0);
    cache.record("かいとう", "回答", "質問へ", Evidence::Chosen, 1.0);
    drop(cache);
    for (before, expected) in [("試験の", "解答"), ("質問へ", "回答")] {
        seed_model_cache(&mut engine, "カイトウ", before, &["回答"]);
        engine.set_surrounding_context(before, "");
        type_keys(&mut engine, "kaitou");
        assert_eq!(preedit_text(&engine), expected, "after {before}");
        engine.process_key(&press_key(Keysym::ESCAPE));
        engine.process_key(&press_key(Keysym::ESCAPE));
    }
}

#[test]
fn direct_input_is_not_rewritten() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    pick(&mut engine, "Anthropic");
    // Shift+letter starts alphabet mode for the word; the kana before it
    // still converts, but nothing is rewritten while it lasts.
    type_keys(&mut engine, "ansoropikku");
    engine.process_key(&press_shift('A'));
    assert!(preedit_text(&engine).starts_with("アンソロピック"));
}

/// The words of ごはんをはしで and かわにかかるはし, and a model that writes
/// はし as 橋 in both.
fn chopsticks_engine() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"ごはん","candidates":[{"surface":"ご飯","score":1.0}]},
            {"reading":"を","candidates":[{"surface":"を","score":1.0}]},
            {"reading":"はし","candidates":[{"surface":"橋","score":1.0},{"surface":"箸","score":2.0}]},
            {"reading":"で","candidates":[{"surface":"で","score":1.0}]},
            {"reading":"かわ","candidates":[{"surface":"川","score":1.0}]},
            {"reading":"に","candidates":[{"surface":"に","score":1.0}]},
            {"reading":"かかる","candidates":[{"surface":"かかる","score":1.0}]},
            {"reading":"あの","candidates":[{"surface":"あの","score":1.0}]},
            {"reading":"こども","candidates":[{"surface":"子供","score":1.0},{"surface":"子ども","score":2.0}]}
        ]"#,
    )));
    seed_model_cache(&mut engine, "ゴハンヲハシデ", "", &["ご飯を橋で"]);
    seed_model_cache(&mut engine, "カワニカカルハシ", "", &["川にかかる橋"]);
    seed_model_cache(&mut engine, "アノコドモ", "", &["あの子供"]);
    engine
}

fn record(engine: &mut InputMethodEngine, reading: &str, surface: &str, before: &str, e: Evidence) {
    let mut cache = engine.learning_cache().unwrap();
    cache.record(reading, surface, before, e, 1.0);
}

fn live(engine: &mut InputMethodEngine, keys: &str) -> String {
    type_keys(engine, keys);
    let shown = preedit_text(engine);
    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_key(Keysym::ESCAPE));
    shown
}

#[test]
fn a_pick_rewrites_only_after_the_reading_it_was_made_after() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を箸で");
    // Another context: the pick only orders lists there.
    assert_eq!(live(&mut engine, "kawanikakaruhashi"), "川にかかる橋");
}

#[test]
fn picks_in_two_contexts_rewrite_anywhere() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "こども", "子ども", "うちの", Evidence::Chosen);
    assert_eq!(live(&mut engine, "anokodomo"), "あの子供");
    record(&mut engine, "こども", "子ども", "", Evidence::Chosen);
    assert_eq!(live(&mut engine, "anokodomo"), "あの子ども");
}

#[test]
fn a_pick_and_the_reverse_pick_cancel_out() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    record(&mut engine, "はし", "橋", "ごはんを", Evidence::Displaced);
    record(&mut engine, "はし", "橋", "ごはんを", Evidence::Chosen);
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Displaced);
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");
}

#[test]
fn the_model_text_is_second_while_composing_and_after_space() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    type_keys(&mut engine, "gohanwohashide");
    let rows = engine.shown_suggestions.candidates().to_vec();
    assert_eq!(rows[0].text, "ご飯を箸で");
    assert_eq!(rows[0].source, Some(CandidateSource::Learning));
    assert_eq!(rows[1].text, "ご飯を橋で");
    assert_eq!(rows[1].source, Some(CandidateSource::Model));

    engine.process_key(&press_key(Keysym::SPACE));
    let rows = engine.candidates().unwrap().candidates().to_vec();
    assert_eq!(rows[0].text, "ご飯を箸で");
    assert_eq!(rows[0].source, Some(CandidateSource::Learning));
    assert_eq!(rows[1].text, "ご飯を橋で");
}

#[test]
fn picking_the_model_text_stops_the_rewrite() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    type_keys(&mut engine, "gohanwohashide");
    // Ctrl+2 while composing.
    engine.process_key(&press_ctrl(Keysym(b'2' as u32)));
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");

    // And the same from Space's list.
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    type_keys(&mut engine, "gohanwohashide");
    pick(&mut engine, "ご飯を橋で");
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");
}

#[test]
fn ctrl_backspace_on_the_rewrite_forgets_what_made_it() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    type_keys(&mut engine, "gohanwohashide");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        engine.candidates().unwrap().selected_text(),
        Some("ご飯を箸で")
    );
    engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(!engine.learning_cache().unwrap().contains_reading("はし"));
    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_key(Keysym::ESCAPE));
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");
}

#[test]
fn a_rewritten_segment_offers_the_model_text() {
    let mut engine = chopsticks_engine();
    record(&mut engine, "はし", "箸", "ごはんを", Evidence::Chosen);
    seed_model_cache(&mut engine, "ハシデ", "ご飯を", &["箸で"]);
    type_keys(&mut engine, "gohanwohashide");
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.state().reading(), Some("はしで"));
    let texts: Vec<String> = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(texts[..2], ["箸で", "橋で"]);
}
