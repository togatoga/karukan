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
    // What follows the learned word, converted after it.
    seed_model_cache(&mut engine, "ノモデル", "Anthropic", &["のモデル"]);
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
    assert!((cache.score("あんそろぴっく", "Anthropic", "〓〓", "") - 1.1).abs() < 1e-3);
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
    cache.record("かいとう", "解答", "試験の", "", Evidence::Chosen, 1.0);
    cache.record("かいとう", "回答", "質問へ", "", Evidence::Chosen, 1.0);
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
            {"reading":"かかる","candidates":[{"surface":"かかる","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(&mut engine, "ゴハンヲハシデ", "", &["ご飯を橋で"]);
    seed_model_cache(&mut engine, "カワニカカルハシ", "", &["川にかかる橋"]);
    seed_model_cache(&mut engine, "ゴハンヲハシ", "", &["ご飯を橋"]);
    engine
}

fn record(
    engine: &mut InputMethodEngine,
    reading: &str,
    surface: &str,
    around: (&str, &str),
    e: Evidence,
) {
    let mut cache = engine.learning_cache().unwrap();
    cache.record(reading, surface, around.0, around.1, e, 1.0);
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
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を箸で");
    // Another context: the pick only orders lists there.
    assert_eq!(live(&mut engine, "kawanikakaruhashi"), "川にかかる橋");
}

#[test]
fn a_pick_rewrites_where_what_was_typed_before_agrees() {
    let mut engine = chopsticks_engine();
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", "で"),
        Evidence::Chosen,
    );
    // はし and the four before it agree, without the で after.
    assert_eq!(live(&mut engine, "gohanwohashi"), "ご飯を箸");
    assert_eq!(live(&mut engine, "kawanikakaruhashi"), "川にかかる橋");
}

#[test]
fn a_pick_and_the_reverse_pick_cancel_out() {
    let mut engine = chopsticks_engine();
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
    record(
        &mut engine,
        "はし",
        "橋",
        ("ごはんを", ""),
        Evidence::Displaced,
    );
    record(
        &mut engine,
        "はし",
        "橋",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Displaced,
    );
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");
}

#[test]
fn the_model_text_is_second_while_composing_and_after_space() {
    let mut engine = chopsticks_engine();
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
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
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
    type_keys(&mut engine, "gohanwohashide");
    // Ctrl+2 while composing.
    engine.process_key(&press_ctrl(Keysym(b'2' as u32)));
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");

    // And the same from Space's list.
    let mut engine = chopsticks_engine();
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
    type_keys(&mut engine, "gohanwohashide");
    pick(&mut engine, "ご飯を橋で");
    assert_eq!(live(&mut engine, "gohanwohashide"), "ご飯を橋で");
}

#[test]
fn ctrl_backspace_on_the_rewrite_forgets_what_made_it() {
    let mut engine = chopsticks_engine();
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
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
    record(
        &mut engine,
        "はし",
        "箸",
        ("ごはんを", ""),
        Evidence::Chosen,
    );
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

/// The words of the sentences around ぶがわるい, and a model that always
/// writes ぶ as 部: the word that decides it, 悪い, comes after it.
fn odds_engine() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"きょう","candidates":[{"surface":"今日","score":1.0}]},
            {"reading":"の","candidates":[{"surface":"の","score":1.0}]},
            {"reading":"しあい","candidates":[{"surface":"試合","score":1.0}]},
            {"reading":"は","candidates":[{"surface":"は","score":1.0}]},
            {"reading":"ぶ","candidates":[{"surface":"部","score":1.0},{"surface":"分","score":2.0}]},
            {"reading":"が","candidates":[{"surface":"が","score":1.0}]},
            {"reading":"わるい","candidates":[{"surface":"悪い","score":1.0}]},
            {"reading":"この","candidates":[{"surface":"この","score":1.0}]},
            {"reading":"しょうぶ","candidates":[{"surface":"勝負","score":1.0}]},
            {"reading":"くらぶ","candidates":[{"surface":"クラブ","score":1.0}]},
            {"reading":"ある","candidates":[{"surface":"ある","score":1.0}]}
        ]"#,
    )));
    for (katakana, lctx, text) in [
        ("キョウノシアイハブガワルイ", "", "今日の試合は部が悪い"),
        ("ブガ", "今日の試合は", "部が"),
        ("コノショウブハブガワルイ", "", "この勝負は部が悪い"),
        ("コノショウブハブガ", "", "この勝負は部が"),
        ("ガワルイ", "この勝負は分", "が悪い"),
        ("クラブガワルイ", "", "クラブが悪い"),
        ("ブガアル", "", "部がある"),
    ] {
        seed_model_cache(&mut engine, katakana, lctx, &[text]);
    }
    // The segment ぶが alone, for its list.
    seed_model_cache(&mut engine, "ブガ", "今日の試合は", &["部が", "分が"]);
    engine
}

/// 今日の試合は部が悪い, with 部 fixed to 分 in its segment.
fn fix_the_odds(engine: &mut InputMethodEngine) {
    type_keys(engine, "kyounoshiaihabugawarui");
    assert_eq!(preedit_text(engine), "今日の試合は部が悪い");
    engine.process_key(&press_key(Keysym::SPACE));
    while engine.state().reading() != Some("ぶが") {
        engine.process_key(&press_key(Keysym::RIGHT));
    }
    while engine.candidates().unwrap().selected_text() != Some("分が") {
        engine.process_key(&press_key(Keysym::SPACE));
    }
    engine.process_key(&press_key(Keysym::RETURN));
}

#[test]
fn a_rule_applies_by_what_follows_it() {
    let mut engine = odds_engine();
    fix_the_odds(&mut engine);
    // Another sentence: only は before agrees, and が悪い after.
    assert_eq!(
        live(&mut engine, "konoshoubuhabugawarui"),
        "この勝負は分が悪い"
    );
}

#[test]
fn a_rule_never_lands_inside_a_word() {
    let mut engine = odds_engine();
    fix_the_odds(&mut engine);
    assert_eq!(live(&mut engine, "kurabugawarui"), "クラブが悪い");
}

#[test]
fn a_short_rule_with_nothing_around_it_agreeing_stays_out() {
    let mut engine = odds_engine();
    fix_the_odds(&mut engine);
    assert_eq!(live(&mut engine, "bugaaru"), "部がある");
}

#[test]
fn only_what_has_been_typed_counts_after_it() {
    let mut engine = odds_engine();
    fix_the_odds(&mut engine);
    type_keys(&mut engine, "konoshoubuhabuga");
    assert_eq!(preedit_text(&engine), "この勝負は部が");
    type_keys(&mut engine, "warui");
    assert_eq!(preedit_text(&engine), "この勝負は分が悪い");
}

#[test]
fn a_rule_applies_however_the_model_wrote_its_reading() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    // The model's kana went astray: 教授う for きょうじゅう.
    seed_model_cache(&mut engine, "キョウジュウニヤル", "", &["教授うにやる"]);
    seed_model_cache(&mut engine, "ニヤル", "今日中", &["にやる"]);
    record(
        &mut engine,
        "きょうじゅう",
        "今日中",
        ("", ""),
        Evidence::Chosen,
    );
    assert_eq!(live(&mut engine, "kyoujuuniyaru"), "今日中にやる");
}

#[test]
fn a_long_rule_cuts_the_reading_when_the_model_text_does_not_show_it() {
    let mut engine = anthropic_engine();
    type_keys(&mut engine, "ansoropikku");
    pick(&mut engine, "Anthropic");
    // The model left the particle in katakana: its text no longer shows
    // where あんそろぴっく ends. Long enough to be strong, the rule cuts the
    // reading, and the rest is converted after it.
    seed_model_cache(
        &mut engine,
        "アンソロピックノモデル",
        "",
        &["アンソロピックノモデル"],
    );
    assert_eq!(
        live(&mut engine, "ansoropikkunomoderu"),
        "Anthropicのモデル"
    );
}

#[test]
fn a_weak_rewrite_replaces_only_the_word_the_model_wrote() {
    let mut engine = odds_engine();
    fix_the_odds(&mut engine);
    // は before and がわ after: 4 chars, weak. The model wrote 部 there,
    // and only that is replaced: nothing is converted again (no
    // conversion of がわ after 分 is seeded).
    seed_model_cache(
        &mut engine,
        "コノショウブハブガワ",
        "",
        &["この勝負は部が輪"],
    );
    assert_eq!(live(&mut engine, "konoshoubuhabugawa"), "この勝負は分が輪");
}

#[test]
fn a_rule_agreeing_on_too_little_for_a_strong_rewrite_needs_the_models_word() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    // The model's katakana does not show where こども ends, and 3 chars of
    // reading with が after are 4, not enough to force it.
    seed_model_cache(&mut engine, "コドモガ", "", &["コドモガ"]);
    record(
        &mut engine,
        "こども",
        "子ども",
        ("", "が"),
        Evidence::Chosen,
    );
    assert_eq!(live(&mut engine, "kodomoga"), "コドモガ");
    // Written as a word, it is replaced.
    seed_model_cache(&mut engine, "コドモガ", "", &["子供が"]);
    assert_eq!(live(&mut engine, "kodomoga"), "子どもが");
}

#[test]
fn six_chars_of_reading_alone_force_the_rule() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.live.enabled = true;
    // Kana rewritten and the rest in katakana: nothing in the model's text
    // shows where きょうじゅう ends, so only a strong rewrite can put it.
    seed_model_cache(&mut engine, "キョウジュウニヤル", "", &["教授ウニヤル"]);
    seed_model_cache(&mut engine, "ニヤル", "今日中", &["にやる"]);
    record(
        &mut engine,
        "きょうじゅう",
        "今日中",
        ("", ""),
        Evidence::Chosen,
    );
    assert_eq!(live(&mut engine, "kyoujuuniyaru"), "今日中にやる");
}
