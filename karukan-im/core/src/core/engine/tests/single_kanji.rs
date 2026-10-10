//! Single-kanji candidates: the whole reading as one kanji, among the
//! rewriter variants. No kanji model is loaded.

use super::*;

/// Mixed list for `reading` as `(text, source, description)`.
fn mixed_list(
    engine: &mut InputMethodEngine,
    reading: &str,
) -> Vec<(String, CandidateSource, Option<String>)> {
    engine
        .build_conversion_candidates(reading, reading, "", 9, LearningLookup::Use)
        .into_iter()
        .map(|c| (c.text, c.source, c.description))
        .collect()
}

fn position(list: &[(String, CandidateSource, Option<String>)], text: &str) -> usize {
    list.iter()
        .position(|(t, _, _)| t == text)
        .unwrap_or_else(|| panic!("{text} missing from {list:?}"))
}

#[test]
fn single_kanji_sit_behind_the_kana_pair_before_the_width_variants() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let list = mixed_list(&mut engine, "たか");

    // たか・タカ, then every single kanji in one run, then ﾀｶ.
    let first = position(&list, "考");
    let last = position(&list, "髙");
    assert!(position(&list, "タカ") < first, "list: {list:?}");
    assert!(last < position(&list, "ﾀｶ"), "list: {list:?}");
    assert!(
        list[first..=last].iter().all(|(text, source, _)| {
            *source == CandidateSource::Rewriter && text.chars().count() == 1
        }),
        "list: {list:?}"
    );
    assert!(
        list.iter().any(|(text, _, _)| text == "高"),
        "list: {list:?}"
    );
}

#[test]
fn single_kanji_dedup_against_higher_sources() {
    let mut engine = engine_with_learned("たか", "高");
    let list = mixed_list(&mut engine, "たか");
    let rows: Vec<_> = list.iter().filter(|(text, _, _)| text == "高").collect();
    assert_eq!(rows.len(), 1, "list: {list:?}");
    assert_eq!(rows[0].1, CandidateSource::Learning);
}

#[test]
fn kanji_used_under_another_reading_moves_up() {
    // 高 committed as こう: for たか it is not a learning row (different
    // reading) but heads the single kanji, ahead of the data order; the
    // unlearned keep that order.
    let mut engine = engine_with_learned("こう", "高");
    let list = mixed_list(&mut engine, "たか");
    let takai = position(&list, "高");
    assert_eq!(list[takai].1, CandidateSource::Rewriter);
    assert!(takai < position(&list, "考"), "list: {list:?}");
    assert!(
        position(&list, "考") < position(&list, "嵩"),
        "list: {list:?}"
    );
}

#[test]
fn ranking_stays_inside_the_single_kanji() {
    // ﾀｶ committed under another reading scores too, but it is not a
    // single kanji: it keeps its slot behind them.
    let mut engine = engine_with_learned("はんかく", "ﾀｶ");
    let list = mixed_list(&mut engine, "たか");
    assert!(
        position(&list, "髙") < position(&list, "ﾀｶ"),
        "list: {list:?}"
    );
}

#[test]
fn variant_note_decorates_the_same_kanji_from_any_source() {
    // The learning row for 髙 wins the dedup; the note the single-kanji
    // row carried still shows on it, in every list.
    let mut engine = engine_with_learned("たか", "髙");
    for ch in "taka".chars() {
        engine.process_key(&press(ch));
    }
    let note_on = |list: &CandidateList| {
        let row = list
            .candidates()
            .iter()
            .find(|c| c.text == "髙")
            .expect("髙 shown");
        assert_eq!(row.source, Some(CandidateSource::Learning));
        row.description.clone()
    };
    // Composing suggestions.
    assert_eq!(
        note_on(&engine.shown_suggestions).as_deref(),
        Some("高の異体字")
    );
    // Mixed list.
    engine.process_key(&press_key(Keysym::SPACE));
    assert_eq!(
        note_on(engine.candidates().unwrap()).as_deref(),
        Some("高の異体字")
    );
    let plain = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .find(|c| c.text == "高");
    assert_eq!(plain.and_then(|c| c.description.clone()), None);
    // 📝 learning view.
    engine.process_key(&press_ctrl(Keysym::KEY_T));
    assert_eq!(
        note_on(engine.candidates().unwrap()).as_deref(),
        Some("高の異体字")
    );
}

#[test]
fn rewriter_view_lists_the_single_kanji() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    for ch in "taka".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_ctrl(Keysym::KEY_R));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:🔄]"), "aux: {aux}");
    let list = engine.candidates().expect("candidates");
    let kasa = list
        .candidates()
        .iter()
        .find(|c| c.text == "嵩")
        .expect("嵩 in the rewriter view");
    assert_eq!(kasa.source, Some(CandidateSource::Rewriter));
    assert_eq!(kasa.reading.as_deref(), Some("たか"));
}
