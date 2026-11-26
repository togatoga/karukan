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
    let cache = engine.learning.as_ref().unwrap();
    assert!((cache.score("あんそろぴっく", "Anthropic", "") - 1.1).abs() < 1e-3);
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
    let cache = engine.learning.as_mut().unwrap();
    cache.record("かいとう", "解答", "試験の", Evidence::Chosen, 1.0);
    cache.record("かいとう", "回答", "質問へ", Evidence::Chosen, 1.0);
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
