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

fn learned(engine: &InputMethodEngine, reading: &str) -> Vec<String> {
    let cache = engine.learning.as_ref().unwrap();
    cache.lookup(reading).into_iter().map(|(s, _)| s).collect()
}

#[test]
fn a_sentence_opens_split_with_its_first_segment_focused() {
    let engine = converted_sentence();
    assert!(engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日は");
    assert_eq!(engine.state().reading(), Some("きょうは"));
    assert_eq!(engine.preedit().unwrap().caret(), 0);
}

#[test]
fn one_word_stays_one_segment() {
    let mut engine = engine_with_learned("__none__", "__none__");
    seed_model_cache(&mut engine, "キョウ", "", &["今日"]);
    type_keys(&mut engine, "kyou");
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(!engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日");
}

#[test]
fn a_phrase_from_the_history_is_split_like_any_other() {
    let mut engine = engine_with_learned("きょうはいしゃにいく", "今日は医者に行く");
    type_keys(&mut engine, "kyouhaishaniiku");
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(preedit_text(&engine), "今日は医者に行く");
    assert!(engine.state().is_segmented());
    assert_eq!(focused_text(&engine), "今日は");
}

#[test]
fn the_aux_line_spells_out_where_the_focus_is() {
    // Drawn by the IME itself, so it shows in applications that draw every
    // underline of the preedit alike (a terminal).
    let mut engine = converted_sentence();
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
    assert_eq!(focused_text(&engine), "今日は");
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
    assert_eq!(learned(&engine, "きょうは"), ["今日は"]);
    assert_eq!(learned(&engine, "いしゃに"), ["慰謝に"]);
    // A segment never focused was cut out of the whole conversion: its
    // pairing with the reading is a guess, and teaches nothing.
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
