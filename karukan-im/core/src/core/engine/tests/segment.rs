//! Segments (文節): splitting a conversion, moving the focus, resizing,
//! and what committing types and teaches.

use super::*;
use crate::core::preedit::AttributeType;

/// きょうはいしゃにいく typed and converted to 今日は医者に行く, the model
/// stood in for by the conversion cache.
fn converted_sentence() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(
        &mut engine,
        "キョウハイシャニイク",
        "",
        &["今日は医者に行く"],
    );
    // What the model gives for each segment alone, after the ones before.
    seed_model_cache(&mut engine, "キョウハ", "", &["今日は"]);
    seed_model_cache(&mut engine, "イシャニ", "今日は", &["医者に", "慰謝に"]);
    seed_model_cache(&mut engine, "イク", "今日は医者に", &["行く", "逝く"]);
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "今日は医者に行く");
    engine
}

fn preedit_text(engine: &InputMethodEngine) -> String {
    engine.preedit().unwrap().text().to_string()
}

/// The highlighted part of the preedit: the focused segment.
fn focused_text(engine: &InputMethodEngine) -> String {
    let preedit = engine.preedit().unwrap();
    let highlight = preedit
        .attributes()
        .iter()
        .find(|a| a.attr_type == AttributeType::Highlight)
        .expect("a focused segment");
    preedit
        .text()
        .chars()
        .skip(highlight.start)
        .take(highlight.end - highlight.start)
        .collect()
}

fn candidate_texts(engine: &InputMethodEngine) -> Vec<String> {
    engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect()
}

fn committed(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn a_conversion_opens_whole() {
    // Space shows what it always did: the list for the whole reading.
    // Segments are asked for with the arrow keys.
    let engine = converted_sentence();
    assert!(!engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日は医者に行く");
    assert_eq!(engine.state().reading(), Some("きょうはいしゃにいく"));
}

#[test]
fn home_splits_and_focuses_the_first_segment() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::HOME));
    assert!(engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日は");
    assert_eq!(engine.state().reading(), Some("きょうは"));
    assert_eq!(engine.preedit().unwrap().caret(), 0);
}

#[test]
fn a_phrase_from_the_history_is_split_like_any_other() {
    let mut engine = engine_with_learned("きょうはいしゃにいく", "今日は医者に行く");
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "今日は医者に行く");
    engine.process_key(&press_key(Keysym::HOME));
    assert!(engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日は");
}

#[test]
fn the_aux_line_is_quiet_about_segments() {
    let mut engine = converted_sentence();
    let result = engine.process_key(&press_key(Keysym::RIGHT));
    let aux = aux_text(&result);
    // The focused segment's reading, and none of the detail.
    assert!(aux.contains("いしゃに"), "aux was: {aux}");
    assert!(!aux.contains("文節"), "aux was: {aux}");
    assert!(!aux.contains("[医者に]"), "aux was: {aux}");
}

#[test]
fn the_verbose_aux_line_spells_out_where_the_focus_is() {
    // Debug detail: which segment of how many, and its neighbours.
    let mut engine = converted_sentence();
    engine.config.verbose = true;
    let result = engine.process_key(&press_key(Keysym::RIGHT));
    let aux = result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::UpdateAuxText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap();
    assert!(aux.contains("今日は[医者に]行く"), "aux was: {aux}");
    assert!(aux.contains("いしゃに 文節2/3"), "aux was: {aux}");
}

#[test]
fn right_moves_the_focus_to_the_second_segment() {
    let mut engine = converted_sentence();
    let result = engine.process_key(&press_key(Keysym::RIGHT));
    assert!(result.consumed);

    // Nothing on screen changed; only the focus did.
    assert_eq!(preedit_text(&engine), "今日は医者に行く");
    assert_eq!(focused_text(&engine), "医者に");
    assert_eq!(engine.state().reading(), Some("いしゃに"));
    // The caret sits where the focused segment starts, so the candidate
    // window follows it.
    assert_eq!(engine.preedit().unwrap().caret(), 3);
    // The segment's own list: what it showed first, then the alternatives.
    let texts = candidate_texts(&engine);
    assert_eq!(texts[..2], ["医者に", "慰謝に"]);
    assert!(texts.contains(&"いしゃに".to_string()));
}

#[test]
fn focus_wraps_and_jumps() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(focused_text(&engine), "行く", "← from the first wraps");
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(focused_text(&engine), "今日は", "→ from the last wraps");
    engine.process_key(&press_key(Keysym::END));
    assert_eq!(focused_text(&engine), "行く");
    engine.process_key(&press_key(Keysym::HOME));
    assert_eq!(focused_text(&engine), "今日は");
    engine.process_key(&press_ctrl(Keysym::KEY_F));
    assert_eq!(focused_text(&engine), "医者に");
    engine.process_key(&press_ctrl(Keysym::KEY_B));
    assert_eq!(focused_text(&engine), "今日は");
}

#[test]
fn a_choice_survives_the_focus_moving_away() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "今日は慰謝に行く");

    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(focused_text(&engine), "行く");
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(focused_text(&engine), "慰謝に");
    assert_eq!(preedit_text(&engine), "今日は慰謝に行く");
}

#[test]
fn enter_commits_every_segment_and_learns_the_chosen_ones() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_key(Keysym::RETURN));

    assert_eq!(committed(&result).as_deref(), Some("今日は慰謝に行く"));
    assert!(engine.state().is_empty());
    // A segment that was focused is learned under its own reading: its
    // text was chosen from a list built for that reading.
    // The stretch that changed, its particle left to the context.
    assert_eq!(learned(&engine, "いしゃ"), ["慰謝"]);
    // A segment never focused was cut out of the whole conversion: its
    // pairing with the reading is a guess, and teaches nothing.
    assert!(learned(&engine, "きょうは").is_empty());
    assert!(learned(&engine, "いく").is_empty());
    // The sentence as a whole is learned as it always was.
    assert_eq!(
        learned(&engine, "きょうはいしゃにいく"),
        ["今日は慰謝に行く"]
    );
}

#[test]
fn shrinking_reconverts_the_segment_and_what_follows() {
    let mut engine = converted_sentence();
    seed_model_cache(&mut engine, "キョウ", "", &["今日", "京"]);
    seed_model_cache(&mut engine, "ハイシャニイク", "今日", &["歯医者に行く"]);

    engine.process_key(&press_shift_key(Keysym::LEFT));
    assert_eq!(engine.state().reading(), Some("きょう"));
    assert_eq!(focused_text(&engine), "今日");
    assert_eq!(preedit_text(&engine), "今日歯医者に行く");
    // The rest was converted with the new segment before it, and split.
    let (left, right) = engine.state().neighbours();
    assert!(left.is_empty());
    let rest: Vec<_> = right
        .iter()
        .map(|s| (s.reading.as_str(), s.text()))
        .collect();
    assert_eq!(rest, [("はいしゃに", "歯医者に"), ("いく", "行く")]);
}

#[test]
fn growing_takes_from_what_follows_and_stops_at_the_end() {
    let mut engine = converted_sentence();
    seed_model_cache(&mut engine, "キョウハイ", "", &["今日杯"]);
    seed_model_cache(&mut engine, "シャニイク", "今日杯", &["社に行く"]);

    engine.process_key(&press_shift_key(Keysym::RIGHT));
    assert_eq!(engine.state().reading(), Some("きょうはい"));
    assert_eq!(preedit_text(&engine), "今日杯社に行く");

    // The last segment has nothing after it to take.
    engine.process_key(&press_key(Keysym::END));
    let before = preedit_text(&engine);
    let result = engine.process_key(&press_shift_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert_eq!(preedit_text(&engine), before);
}

#[test]
fn a_segment_of_one_char_does_not_shrink() {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(&mut engine, "キ", "", &["木"]);
    type_keys(&mut engine, "ki");
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_shift_key(Keysym::LEFT));
    assert!(result.consumed);
    assert_eq!(preedit_text(&engine), "木");
    assert!(!engine.state().is_segmented());
}

#[test]
fn the_rest_stays_kana_when_the_model_has_no_answer() {
    let mut engine = converted_sentence();
    engine.process_key(&press_shift_key(Keysym::LEFT));
    // Nothing seeded for きょう or the rest: both fall back to the reading.
    assert_eq!(preedit_text(&engine), "きょうはいしゃにいく");
    assert_eq!(engine.state().reading(), Some("きょう"));
    assert_eq!(engine.state().neighbours().1.len(), 1);
}

#[test]
fn typing_commits_the_segments_and_starts_the_next_input() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    let result = engine.process_key(&press('a'));

    assert_eq!(committed(&result).as_deref(), Some("今日は医者に行く"));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(preedit_text(&engine), "あ");
}

#[test]
fn escape_returns_to_the_whole_composition() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(preedit_text(&engine), "きょうはいしゃにいく");
}

#[test]
fn ctrl_digit_picks_for_the_segment_and_moves_on() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    let result = engine.process_key(&press_ctrl(Keysym('2' as u32)));

    assert!(committed(&result).is_none(), "one segment is not the whole");
    assert_eq!(preedit_text(&engine), "今日は慰謝に行く");
    assert_eq!(focused_text(&engine), "行く");
}

#[test]
fn a_source_view_works_on_the_focused_segment() {
    let mut engine = converted_sentence();
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"いしゃに","candidates":[{"surface":"医者に","score":1.0}]},
            {"reading":"いしゃにかかる","candidates":[{"surface":"医者にかかる","score":1.0}]}
        ]"#,
    )));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // learning
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // dictionary

    assert_eq!(engine.state().filter(), Some(CandidateSource::Dictionary));
    // The segment's reading is fixed: no longer word predicted from it.
    assert_eq!(candidate_texts(&engine), ["医者に"]);
    assert_eq!(preedit_text(&engine), "今日は医者に行く");
}

#[test]
fn katakana_key_rewrites_the_focused_segment_only() {
    let mut engine = converted_sentence();
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(preedit_text(&engine), "今日はイシャニ行く");
    assert!(engine.state().is_segmented());
}

#[test]
fn a_conversion_that_dropped_kana_is_not_split() {
    // The model answered わせだにだいがく with 早稲田大学. The dictionary
    // knows both words, and に fits neither: the text cannot be trusted to
    // line up with the reading, so it stays one segment.
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1.0}]},
            {"reading":"だいがく","candidates":[{"surface":"大学","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "ワセダニダイガクヘイク",
        "",
        &["早稲田大学へ行く"],
    );
    type_keys(&mut engine, "wasedanidaigakuheiku");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "早稲田大学へ行く");

    assert!(!engine.state().is_segmented());
    engine.process_key(&press_key(Keysym::RIGHT));
    assert!(!engine.state().is_segmented());
    assert_eq!(preedit_text(&engine), "早稲田大学へ行く");
}

/// わせだだいがくにいく converted to 早稲田大学に行く, with a dictionary
/// that knows the university both as one word and as its two parts.
fn converted_university() -> InputMethodEngine {
    let mut engine = typed_university();
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(!engine.state().is_segmented());
    engine
}

/// The same sentence typed and not yet converted: the live conversion.
fn typed_university() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1.0}]},
            {"reading":"だいがく","candidates":[{"surface":"大学","score":1.0}]},
            {"reading":"わせだだいがく","candidates":[{"surface":"早稲田大学","score":1.0}]},
            {"reading":"に","candidates":[{"surface":"に","score":1.0}]},
            {"reading":"いく","candidates":[{"surface":"行く","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "ワセダダイガクニイク",
        "",
        &["早稲田大学に行く"],
    );
    type_keys(&mut engine, "wasedadaigakuniiku");
    engine
}

/// The segments' texts in order.
fn segment_texts(engine: &InputMethodEngine) -> Vec<String> {
    let (left, right) = engine.state().neighbours();
    left.iter()
        .map(|s| s.text().to_string())
        .chain([focused_text(engine)])
        .chain(right.iter().map(|s| s.text().to_string()))
        .collect()
}

fn press_ctrl_s() -> KeyEvent {
    press_ctrl(Keysym::KEY_S)
}

#[test]
fn ctrl_s_while_typing_converts_and_cuts_in_one_press() {
    let mut engine = typed_university();
    engine.process_key(&press_ctrl_s());

    assert_eq!(segment_texts(&engine), ["早稲田大学に", "行く"]);
    assert_eq!(focused_text(&engine), "早稲田大学に");
    assert_eq!(preedit_text(&engine), "早稲田大学に行く");
}

#[test]
fn ctrl_s_while_typing_cuts_what_is_on_screen_not_the_history() {
    // The history has another text for this reading, and a longer phrase
    // that starts with it; Space would open on the history.
    let mut engine = typed_university_with_history();
    set_live_text(&mut engine, "早稲田大学に行く");

    let result = engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["早稲田大学に", "行く"]);
    assert_eq!(preedit_text(&engine), "早稲田大学に行く");
    // The frontend is told: the split preedit is in the result.
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::UpdatePreedit(_)))
    );
}

/// [`typed_university`] with a history that reads the same reading another
/// way, and has a longer phrase starting with it.
fn typed_university_with_history() -> InputMethodEngine {
    let mut engine = engine_with_learned("わせだだいがくにいく", "ワセダ大学に行く");
    engine.live.enabled = true;
    if let Some(mut cache) = engine.learning_cache() {
        cache.record(
            "わせだだいがくにいくよ",
            "早稲田大学に行くよ",
            "",
            "",
            Evidence::Chosen,
            1.0,
        );
    }
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1.0}]},
            {"reading":"だいがく","candidates":[{"surface":"大学","score":1.0}]},
            {"reading":"わせだだいがく","candidates":[{"surface":"早稲田大学","score":1.0}]},
            {"reading":"に","candidates":[{"surface":"に","score":1.0}]},
            {"reading":"いく","candidates":[{"surface":"行く","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "ワセダダイガクニイク",
        "",
        &["早稲田大学に行く"],
    );
    type_keys(&mut engine, "wasedadaigakuniiku");
    engine
}

#[test]
fn ctrl_s_while_typing_one_word_is_a_plain_conversion() {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(&mut engine, "キョウ", "", &["今日"]);
    type_keys(&mut engine, "kyou");
    let result = engine.process_key(&press_ctrl_s());

    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(!engine.state().is_segmented());
    assert_eq!(preedit_text(&engine), "今日");
}

#[test]
fn ctrl_s_cuts_the_conversion_into_segments() {
    let mut engine = converted_university();

    // The dictionary's longest words, first segment focused: に, a word
    // of one hiragana, with the word before it.
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["早稲田大学に", "行く"]);
    assert_eq!(focused_text(&engine), "早稲田大学に");
    assert_eq!(preedit_text(&engine), "早稲田大学に行く");

    // The script: wherever it changes, the verb's ending off too.
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["早稲田大学", "に", "行", "く"]);

    // The dictionary's shortest words.
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["早稲田", "大学", "に", "行く"]);

    // Then the whole again, one segment — and round to the first way.
    engine.process_key(&press_ctrl_s());
    assert!(!engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "早稲田大学に行く");
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["早稲田大学に", "行く"]);
    assert_eq!(preedit_text(&engine), "早稲田大学に行く");
}

#[test]
fn ctrl_s_goes_between_the_one_way_and_the_whole_when_every_way_cuts_alike() {
    let mut engine = converted_tower();
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["東京", "タワー"]);

    engine.process_key(&press_ctrl_s());
    assert!(!engine.state().is_segmented());
    assert_eq!(preedit_text(&engine), "東京タワー");

    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["東京", "タワー"]);
}

#[test]
fn ctrl_s_back_to_the_whole_keeps_what_was_chosen() {
    let mut engine = converted_tower();
    seed_model_cache(&mut engine, "トウキョウ", "", &["東京", "東響"]);
    engine.process_key(&press_ctrl_s());
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "東響タワー");

    engine.process_key(&press_ctrl_s());
    assert!(!engine.state().is_segmented());
    assert_eq!(preedit_text(&engine), "東響タワー");
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result).as_deref(), Some("東響タワー"));
}

/// とうきょうたわー converted to 東京タワー with no dictionary: the three
/// ways all cut it 東京|タワー.
fn converted_tower() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(&mut engine, "トウキョウタワー", "", &["東京タワー"]);
    type_keys(&mut engine, "toukyoutawa-");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "東京タワー");
    engine
}

/// きょうぎぷろぐらみんぐをまなぶ converted, with a dictionary that has
/// 競技プログラミング as one word and as its two parts.
fn converted_programming() -> InputMethodEngine {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"きょうぎぷろぐらみんぐ","candidates":[{"surface":"競技プログラミング","score":1.0}]},
            {"reading":"きょうぎ","candidates":[{"surface":"競技","score":1.0}]},
            {"reading":"ぷろぐらみんぐ","candidates":[{"surface":"プログラミング","score":1.0}]},
            {"reading":"を","candidates":[{"surface":"を","score":1.0}]},
            {"reading":"まなぶ","candidates":[{"surface":"学ぶ","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "キョウギプログラミングヲマナブ",
        "",
        &["競技プログラミングを学ぶ"],
    );
    type_keys(&mut engine, "kyougipuroguraminguwomanabu");
    engine.process_key(&press_key(Keysym::SPACE));
    engine
}

fn aux_text(result: &EngineResult) -> String {
    result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::UpdateAuxText(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn ctrl_s_again_cuts_the_next_way_and_round() {
    let mut engine = converted_programming();
    // The aux line names the way of cutting in verbose mode.
    engine.config.verbose = true;

    // By the dictionary: its longest word is one segment.
    let result = engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["競技プログラミングを", "学ぶ"]);
    assert!(
        aux_text(&result).contains("文節1/2 区切り:辞書(最長)"),
        "{}",
        aux_text(&result)
    );

    // By script: wherever it changes.
    let result = engine.process_key(&press_ctrl_s());
    assert_eq!(
        segment_texts(&engine),
        ["競技", "プログラミング", "を", "学", "ぶ"]
    );
    assert!(
        aux_text(&result).contains("文節1/5 区切り:字種"),
        "{}",
        aux_text(&result)
    );

    // By the dictionary's shortest words: を comes off too.
    let result = engine.process_key(&press_ctrl_s());
    assert_eq!(
        segment_texts(&engine),
        ["競技", "プログラミング", "を", "学ぶ"]
    );
    assert!(
        aux_text(&result).contains("文節1/4 区切り:辞書(最短)"),
        "{}",
        aux_text(&result)
    );

    // The whole, and then the first way again.
    engine.process_key(&press_ctrl_s());
    assert!(!engine.state().is_segmented());
    engine.process_key(&press_ctrl_s());
    assert_eq!(segment_texts(&engine), ["競技プログラミングを", "学ぶ"]);
    assert_eq!(preedit_text(&engine), "競技プログラミングを学ぶ");

    // Resized, the segments are neither cut: no name, and the next press
    // starts from the dictionary's again.
    let result = engine.process_key(&press_shift_key(Keysym::LEFT));
    assert!(
        !aux_text(&result).contains("区切り"),
        "{}",
        aux_text(&result)
    );
}

#[test]
fn ctrl_s_names_no_way_of_cutting_when_there_is_one() {
    let mut engine = converted_tower();
    engine.config.verbose = true;
    let result = engine.process_key(&press_ctrl_s());
    assert!(
        aux_text(&result).contains("文節1/2"),
        "{}",
        aux_text(&result)
    );
    assert!(
        !aux_text(&result).contains("区切り"),
        "{}",
        aux_text(&result)
    );
}

#[test]
fn ctrl_s_on_one_dictionary_word_cuts_inside_it() {
    let mut engine = engine_with_learned("__none__", "__none__");
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[
            {"reading":"きょうぎぷろぐらみんぐ","candidates":[{"surface":"競技プログラミング","score":1.0}]},
            {"reading":"きょうぎ","candidates":[{"surface":"競技","score":1.0}]},
            {"reading":"ぷろぐらみんぐ","candidates":[{"surface":"プログラミング","score":1.0}]},
            {"reading":"ぷろ","candidates":[{"surface":"プロ","score":1.0}]},
            {"reading":"ぐら","candidates":[{"surface":"グラ","score":1.0}]},
            {"reading":"みんぐ","candidates":[{"surface":"ミング","score":1.0}]}
        ]"#,
    )));
    seed_model_cache(
        &mut engine,
        "キョウギプログラミング",
        "",
        &["競技プログラミング"],
    );
    type_keys(&mut engine, "kyougipuroguramingu");
    engine.process_key(&press_ctrl_s());

    assert_eq!(segment_texts(&engine), ["競技", "プログラミング"]);
    assert_eq!(preedit_text(&engine), "競技プログラミング");
}

#[test]
fn ctrl_s_does_nothing_where_nothing_comes_apart() {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(&mut engine, "キョウ", "", &["今日"]);
    type_keys(&mut engine, "kyou");
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_ctrl_s());
    assert!(result.consumed);
    assert!(!engine.state().is_segmented());
    assert_eq!(preedit_text(&engine), "今日");
}
