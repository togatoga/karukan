//! Tests for the Ctrl+R conversion-window source filter.

use super::*;
use crate::core::engine::cache::ConversionCacheKey;

/// Conversion whose list mixes sources: learning (愛) + model (合い, via a
/// seeded conversion-cache entry standing in for the model) + fallback +
/// rewriter variants. No dictionaries are loaded, so those views are
/// empty.
fn engine_in_conversion() -> InputMethodEngine {
    let mut engine = engine_with_learned("あい", "愛");
    engine.conversion_cache.insert(
        ConversionCacheKey {
            katakana: "アイ".to_string(),
            lctx: String::new(),
            strategy: ConversionStrategy::MainModelOnly,
        },
        vec!["合い".to_string()],
    );
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    engine
}

/// Sources of the currently shown candidates.
fn shown_sources(engine: &InputMethodEngine) -> Vec<Option<CandidateSource>> {
    engine
        .candidates()
        .expect("conversion candidates")
        .candidates()
        .iter()
        .map(|c| c.source)
        .collect()
}

/// Press Ctrl+R (or Ctrl+T) and assert the rewriter view: rewriter
/// variants first, the plain kana pair at the tail, under the 🔄 header.
fn cycle_expecting_rewriter_view(engine: &mut InputMethodEngine, forward: bool) {
    let key = if forward {
        Keysym::KEY_R
    } else {
        Keysym::KEY_T
    };
    let result = engine.process_key(&press_ctrl(key));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:🔄]"), "aux was: {aux}");
    let candidates = engine.candidates().unwrap().candidates().to_vec();
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"あい") && texts.contains(&"アイ"),
        "kana pair missing: {texts:?}"
    );
    // Appended kana (Fallback) trail every rewriter variant.
    let first_fallback = candidates
        .iter()
        .position(|c| c.source == Some(CandidateSource::Fallback));
    let last_rewriter = candidates
        .iter()
        .rposition(|c| c.source == Some(CandidateSource::Rewriter));
    if let (Some(fallback), Some(rewriter)) = (first_fallback, last_rewriter) {
        assert!(
            fallback > rewriter,
            "kana must sit at the tail: {:?}",
            shown_sources(engine)
        );
    }
    assert!(
        candidates.iter().all(|c| matches!(
            c.source,
            Some(CandidateSource::Rewriter | CandidateSource::Fallback)
        )),
        "sources were: {:?}",
        shown_sources(engine)
    );
}

/// Press Ctrl+R (or Ctrl+Shift+R) and assert the window narrowed to an
/// EMPTY `source` view: no candidates, 「候補なし」 in the aux.
fn cycle_expecting_empty(engine: &mut InputMethodEngine, forward: bool, source: CandidateSource) {
    let result = if forward {
        engine.process_key(&press_ctrl(Keysym::KEY_R))
    } else {
        engine.process_key(&press_ctrl(Keysym::KEY_T))
    };
    assert_eq!(engine.candidates().unwrap().len(), 0);
    let aux = last_aux_text(&result).expect("aux text action");
    let header = format!("[変換:{}]", source.emoji());
    assert!(
        aux.starts_with(&header) && aux.contains("候補なし"),
        "aux was: {aux}"
    );
}

/// Press Ctrl+R (or Ctrl+Shift+R) and assert the window narrowed to
/// `source`, with its emoji in the aux header.
fn cycle_expecting(engine: &mut InputMethodEngine, forward: bool, source: CandidateSource) {
    let result = if forward {
        engine.process_key(&press_ctrl(Keysym::KEY_R))
    } else {
        engine.process_key(&press_ctrl(Keysym::KEY_T))
    };
    assert!(
        shown_sources(engine).iter().all(|s| *s == Some(source)),
        "sources were: {:?}",
        shown_sources(engine)
    );
    let aux = last_aux_text(&result).expect("aux text action");
    let header = format!("[変換:{}]", source.emoji());
    assert!(aux.starts_with(&header), "aux was: {aux}");
}

#[test]
fn test_cycle_visits_every_source_without_skipping() {
    let mut engine = engine_in_conversion();

    // Every press moves exactly one step; empty sources are shown as
    // 「候補なし」, never skipped, so the position is always predictable.
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    cycle_expecting_empty(&mut engine, true, CandidateSource::UserDictionary);
    cycle_expecting(&mut engine, true, CandidateSource::Model);
    cycle_expecting_empty(&mut engine, true, CandidateSource::Dictionary);
    cycle_expecting_rewriter_view(&mut engine, true);

    // The rotation never returns to the full list: one more wraps to the
    // learning view (the full list is what Space already shows).
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
}

#[test]
fn test_ctrl_r_from_composing_opens_filtered_conversion() {
    // Straight from typing: Ctrl+R starts the conversion already narrowed
    // to the first source (learning) — no Space needed.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_R));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:📝]"), "aux was: {aux}");
    assert!(
        shown_sources(&engine)
            .iter()
            .all(|s| *s == Some(CandidateSource::Learning))
    );
}

#[test]
fn test_ctrl_t_from_composing_opens_reverse_filtered_conversion() {
    // The reverse entry works straight from typing too: Ctrl+T starts
    // the conversion narrowed to the cycle's tail (rewriter — one press
    // away for things like future date conversion).
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_T));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:🔄]"), "aux was: {aux}");
    assert!(shown_sources(&engine).iter().all(|s| matches!(
        s,
        Some(CandidateSource::Rewriter | CandidateSource::Fallback)
    )));
}

#[test]
fn test_uppercase_ctrl_r_without_shift_cycles_forward() {
    // Some environments deliver Ctrl+R as keysym 'R' (uppercase) with the
    // shift bit unset; direction must follow the modifier, not the case.
    let mut engine = engine_in_conversion();
    let key = KeyEvent::new(
        Keysym::KEY_R_UPPER,
        KeyModifiers::new().with_control(true),
        true,
    );
    let result = engine.process_key(&key);
    assert!(
        shown_sources(&engine)
            .iter()
            .all(|s| *s == Some(CandidateSource::Learning))
    );
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:📝]"), "aux was: {aux}");
}

#[test]
fn test_tab_keeps_mozc_candidate_navigation() {
    // Tab must stay next-candidate and Shift+Tab (ISO_Left_Tab on X11)
    // prev-candidate — mozc-compatible muscle memory, never the filter.
    let mut engine = engine_in_conversion();
    let before = engine.candidates().unwrap().cursor();
    engine.process_key(&press_key(Keysym::TAB));
    assert_eq!(engine.candidates().unwrap().cursor(), before + 1);
    engine.process_key(&press_key(Keysym::ISO_LEFT_TAB));
    assert_eq!(engine.candidates().unwrap().cursor(), before);
    // Still unfiltered: Tab never touches the source filter.
    let sources = shown_sources(&engine);
    assert!(
        sources
            .iter()
            .any(|s| *s != Some(CandidateSource::Learning))
    );
}

#[test]
fn test_cycle_backward_reaches_last_source_first() {
    // Ctrl+Shift+R from the full list walks the cycle in reverse, without
    // skipping empty sources.
    let mut engine = engine_in_conversion();
    cycle_expecting_rewriter_view(&mut engine, false);
    cycle_expecting_empty(&mut engine, false, CandidateSource::Dictionary);
}

#[test]
fn test_commit_from_filtered_list() {
    // Return commits the selected row of the narrowed list.
    let mut engine = engine_in_conversion();
    engine.process_key(&press_ctrl(Keysym::KEY_R));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let committed = result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(committed.as_deref(), Some("愛"));
}

#[test]
fn test_kana_survive_mixed_list_dedup_in_rewriter_view() {
    // When another source already owns the plain kana text (here the
    // learning cache learned the reading itself), the mixed list dedups
    // the fallback entry away; the 🔄 view must still show the pair at
    // its tail, derived from the reading.
    let mut engine = engine_with_learned("あい", "あい");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting_rewriter_view(&mut engine, false);
}

#[test]
fn test_dictionary_view_prefix_matches_from_one_char() {
    // The dictionary views are full browsers: predictive matches kick in
    // from the very first char, unlike the mixed list's 2-char guard.
    use std::io::Write;
    let mut engine = InputMethodEngine::new();
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    let json = r#"[
        {"reading":"あ","candidates":[{"surface":"亜","score":1.0}]},
        {"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}
    ]"#;
    tmp.write_all(json.as_bytes()).unwrap();
    tmp.flush().unwrap();
    engine.dicts.user = Some(Dictionary::build_from_json(tmp.path()).unwrap());

    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_ctrl(Keysym::KEY_R)); // 学習（候補なし）
    let result = engine.process_key(&press_ctrl(Keysym::KEY_R)); // ユーザー辞書
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:👤]"), "aux was: {aux}");
    let candidates = engine.candidates().unwrap().candidates().to_vec();
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    assert!(
        texts.contains(&"亜") && texts.contains(&"藍"),
        "texts were: {texts:?}"
    );
    // Predictive entries keep their full reading, so committing records
    // under the right key.
    let prefixed = candidates.iter().find(|c| c.text == "藍").unwrap();
    assert_eq!(prefixed.reading.as_deref(), Some("あい"));
}

#[test]
fn test_learning_filter_shows_full_history() {
    // The mixed list caps learning entries at 3; the narrowed learning view
    // is a history browser and shows them all.
    let mut engine = engine_with_learned("あい", "愛");
    for surface in ["藍", "相", "合い", "間"] {
        engine.learning.as_mut().unwrap().record("あい", surface);
    }
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    let learning_in_all = shown_sources(&engine)
        .iter()
        .filter(|s| **s == Some(CandidateSource::Learning))
        .count();
    assert_eq!(learning_in_all, 3);

    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    assert_eq!(engine.candidates().unwrap().len(), 5);
}

#[test]
fn test_learning_delete_keeps_the_filter() {
    // Deleting a learning candidate from the narrowed learning view keeps
    // the filter, so consecutive deletes stay in that view; deleting the
    // last one falls back to the full list.
    let mut engine = engine_with_learned("あい", "愛");
    engine.learning.as_mut().unwrap().record("あい", "藍");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    assert_eq!(engine.candidates().unwrap().len(), 2);

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換:📝]"), "aux was: {aux}");
    assert_eq!(engine.candidates().unwrap().len(), 1);
    assert!(
        shown_sources(&engine)
            .iter()
            .all(|s| *s == Some(CandidateSource::Learning))
    );

    // Deleting the last learning entry keeps the filter: the view shows
    // 「候補なし」 instead of falling back to the full list.
    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(
        aux.starts_with("[変換:📝]") && aux.contains("候補なし"),
        "aux was: {aux}"
    );
    assert_eq!(engine.candidates().unwrap().len(), 0);

    // Navigation on the empty view is a no-op — it must not blank the
    // reading preedit.
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(result.actions.is_empty());
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Enter commits the displayed reading (the empty view's preedit) —
    // never an empty commit that would eat the composition.
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let committed = result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(committed.as_deref(), Some("あい"));
}

#[test]
fn test_typing_on_empty_view_commits_reading_and_continues() {
    // A printable key on the empty view commits the displayed reading and
    // starts the new input — the composition is never silently dropped.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    engine.process_key(&press_ctrl(Keysym::DELETE)); // empty learning view

    let result = engine.process_key(&press('k'));
    let committed = result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    });
    assert_eq!(committed.as_deref(), Some("あい"));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "k");
}

#[test]
fn test_delete_keeps_cursor_position() {
    // Deleting row N leaves the cursor at N (the old N+1 slides in), so
    // consecutive deletes chew through the list without jumping to the top.
    let mut engine = engine_with_learned("あい", "愛");
    engine.learning.as_mut().unwrap().record("あい", "藍");
    engine.learning.as_mut().unwrap().record("あい", "相");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    let before: Vec<String> = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert_eq!(before.len(), 3);

    engine.process_key(&press_key(Keysym::DOWN)); // cursor → 1
    engine.process_key(&press_ctrl(Keysym::DELETE));
    let list = engine.candidates().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list.cursor(), 1);
    assert_eq!(list.selected_text(), Some(before[2].as_str()));

    // Deleting the (new) last row clamps the cursor to the new end.
    engine.process_key(&press_ctrl(Keysym::DELETE));
    let list = engine.candidates().unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list.cursor(), 0);
}

#[test]
fn test_filter_resets_on_new_conversion() {
    let mut engine = engine_in_conversion();
    engine.process_key(&press_ctrl(Keysym::KEY_R));

    // Cancel back to composing, convert again: the window is unfiltered.
    engine.process_key(&press_key(Keysym::ESCAPE));
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.starts_with("[変換]"), "aux was: {aux}");
    assert!(
        shown_sources(&engine)
            .iter()
            .any(|s| *s != Some(CandidateSource::Learning))
    );
}
