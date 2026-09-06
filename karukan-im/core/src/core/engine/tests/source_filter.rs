//! Tests for the Ctrl+R conversion-window source filter.

use super::*;

/// Conversion whose list mixes sources: learning (愛) + model (合い, via a
/// seeded conversion-cache entry standing in for the model) + fallback +
/// rewriter variants. No dictionaries are loaded, so those views are
/// empty.
fn engine_in_conversion() -> InputMethodEngine {
    let mut engine = engine_with_learned("あい", "愛");
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
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

/// Texts of the currently shown candidates.
fn shown_texts(engine: &InputMethodEngine) -> Vec<String> {
    engine
        .candidates()
        .expect("conversion candidates")
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect()
}

/// Press Ctrl+R (or Ctrl+T) and assert the rewriter view: rewriter
/// variants first, the plain kana pair at the tail, under the 🔄 header.
/// Press Ctrl+R/T and assert the 📚 view opened: one stop covering both
/// dictionaries, so its candidates carry either dictionary's source.
fn cycle_expecting_dictionary_view(engine: &mut InputMethodEngine, forward: bool) {
    let key = if forward {
        Keysym::KEY_T
    } else {
        Keysym::KEY_R
    };
    let result = engine.process_key(&press_ctrl(key));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📚]"), "aux was: {aux}");
    assert!(
        engine
            .candidates()
            .unwrap()
            .candidates()
            .iter()
            .all(|c| matches!(
                c.source,
                Some(CandidateSource::UserDictionary | CandidateSource::Dictionary)
            )),
        "sources were: {:?}",
        shown_sources(engine)
    );
}

fn cycle_expecting_rewriter_view(engine: &mut InputMethodEngine, forward: bool) {
    let key = if forward {
        Keysym::KEY_T
    } else {
        Keysym::KEY_R
    };
    let result = engine.process_key(&press_ctrl(key));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:🔄]"), "aux was: {aux}");
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
        engine.process_key(&press_ctrl(Keysym::KEY_T))
    } else {
        engine.process_key(&press_ctrl(Keysym::KEY_R))
    };
    assert_eq!(engine.candidates().unwrap().len(), 0);
    let aux = last_aux_text(&result).expect("aux text action");
    let header = format!("[変換:{}]", source.emoji());
    assert!(
        aux.contains(&header) && aux.contains("候補なし"),
        "aux was: {aux}"
    );
}

/// Open the 🤖 view with its own key, for tests about what the view shows
/// rather than how the cycle reaches it.
fn open_model_view(engine: &mut InputMethodEngine) {
    let result = engine.process_key(&press_ctrl(Keysym::KEY_I));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:🤖]"), "aux was: {aux}");
}

/// Press Ctrl+R (or Ctrl+Shift+R) and assert the window narrowed to
/// `source`, with its emoji in the aux header.
fn cycle_expecting(engine: &mut InputMethodEngine, forward: bool, source: CandidateSource) {
    let result = if forward {
        engine.process_key(&press_ctrl(Keysym::KEY_T))
    } else {
        engine.process_key(&press_ctrl(Keysym::KEY_R))
    };
    assert!(
        shown_sources(engine).iter().all(|s| *s == Some(source)),
        "sources were: {:?}",
        shown_sources(engine)
    );
    let aux = last_aux_text(&result).expect("aux text action");
    let header = format!("[変換:{}]", source.emoji());
    assert!(aux.contains(&header), "aux was: {aux}");
}

#[test]
fn test_cycle_visits_every_source_without_skipping() {
    let mut engine = engine_in_conversion();

    // Every press moves exactly one step; empty sources are shown as
    // 「候補なし」, never skipped, so the position is always predictable.
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    cycle_expecting_empty(&mut engine, true, CandidateSource::Dictionary);
    cycle_expecting(&mut engine, true, CandidateSource::Model);
    cycle_expecting_rewriter_view(&mut engine, true);

    // The rotation never returns to the full list: one more wraps to the
    // learning view (the full list is what Space already shows).
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
}

#[test]
fn test_ctrl_t_from_composing_opens_filtered_conversion() {
    // Straight from typing: Ctrl+T starts the conversion already narrowed
    // to the first source (learning) — no Space needed.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_T));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📝]"), "aux was: {aux}");
    assert!(
        shown_sources(&engine)
            .iter()
            .all(|s| *s == Some(CandidateSource::Learning))
    );
}

#[test]
fn test_ctrl_r_from_composing_opens_reverse_filtered_conversion() {
    // The reverse entry works straight from typing too: Ctrl+R starts
    // the conversion narrowed to the cycle's tail (rewriter — one press
    // away for the plain kana).
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_R));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:🔄]"), "aux was: {aux}");
    assert!(shown_sources(&engine).iter().all(|s| matches!(
        s,
        Some(CandidateSource::Rewriter | CandidateSource::Fallback)
    )));
}

#[test]
fn test_uppercase_ctrl_t_without_shift_cycles_forward() {
    // Some environments deliver Ctrl+T as keysym 'T' (uppercase) with the
    // shift bit unset; direction must follow the modifier, not the case.
    let mut engine = engine_in_conversion();
    let key = KeyEvent::new(
        Keysym::KEY_T_UPPER,
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
    assert!(aux.contains("[変換:📝]"), "aux was: {aux}");
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
    cycle_expecting(&mut engine, false, CandidateSource::Model);
}

#[test]
fn test_commit_from_filtered_list() {
    // Return commits the selected row of the narrowed list.
    let mut engine = engine_in_conversion();
    engine.process_key(&press_ctrl(Keysym::KEY_T));
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
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[
        {"reading":"あ","candidates":[{"surface":"亜","score":1.0}]},
        {"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}
    ]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 学習（候補なし）
    let result = engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📚辞書
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📚]"), "aux was: {aux}");
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
fn test_typing_narrows_within_the_filtered_view() {
    // fzf-style: typing while a source view is active keeps the view and
    // narrows it with the grown reading.
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[
        {"reading":"あ","candidates":[{"surface":"亜","score":1.0}]},
        {"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}
    ]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📝（候補なし）
    let result = engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📚
    assert!(last_aux_text(&result).expect("aux").contains("[変換:📚]"));
    assert_eq!(shown_texts(&engine), vec!["亜", "藍"]);

    // Typing narrows the SAME view: reading grows to あい, only 藍 stays.
    let result = engine.process_key(&press('i'));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:📚]"), "aux was: {aux}");
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(shown_texts(&engine), vec!["藍"]);

    // A pending consonant keeps the view too (tail-aware narrowing).
    let result = engine.process_key(&press('k'));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:📚]"), "aux was: {aux}");
}

#[test]
fn test_backspace_widens_within_the_filtered_view() {
    // The mirror of typing-refine: Backspace shrinks the reading and the
    // view re-expands; emptying the buffer exits cleanly.
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[
        {"reading":"あ","candidates":[{"surface":"亜","score":1.0}]},
        {"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}
    ]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📝（候補なし）
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📚: [藍]
    assert_eq!(shown_texts(&engine), vec!["藍"]);

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📚]"), "aux was: {aux}");
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(shown_texts(&engine), vec!["亜", "藍"]);

    // Emptying the buffer leaves the conversion entirely.
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(matches!(engine.state(), InputState::Empty));
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
    // last one leaves an empty 「候補なし」 view, still filtered.
    let mut engine = engine_with_learned("あい", "愛");
    engine.learning.as_mut().unwrap().record("あい", "藍");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    assert_eq!(engine.candidates().unwrap().len(), 2);

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📝]"), "aux was: {aux}");
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
        aux.contains("[変換:📝]") && aux.contains("候補なし"),
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
fn test_typing_on_empty_view_keeps_view_and_refines() {
    // A printable key on the empty view extends the reading and stays in
    // the narrowed view — nothing is committed or lost.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    engine.process_key(&press_ctrl(Keysym::DELETE)); // empty learning view

    let result = engine.process_key(&press('k'));
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_)))
    );
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📝]"), "aux was: {aux}");
    assert_eq!(engine.input_buf.display(), "あいk");
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
    assert!(aux.contains("[変換]"), "aux was: {aux}");
    assert!(
        shown_sources(&engine)
            .iter()
            .any(|s| *s != Some(CandidateSource::Learning))
    );
}

#[test]
fn test_pending_tail_narrows_the_learning_view() {
    // A pending consonant narrows the learning view like the dictionary
    // one: the exact entry for the base reading is stale (the reading is
    // about to grow), so it drops while a prediction the tail can still
    // reach stays.
    let mut engine = engine_with_learned("あい", "愛");
    engine.learning.as_mut().unwrap().record("あいか", "愛香");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);
    assert_eq!(shown_texts(&engine), vec!["愛", "愛香"]);

    let result = engine.process_key(&press('k'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[変換:📝]"), "aux was: {aux}");
    assert_eq!(shown_texts(&engine), vec!["愛香"]);

    // Enter commits the prediction under its full reading — the typed
    // tail was consumed by it, not silently dropped.
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(text) if text == "愛香"))
    );
}

#[test]
fn test_stale_learning_candidate_cannot_swallow_the_tail() {
    // With no entry the tail can reach, the view empties instead of
    // keeping the stale exact match; Enter then commits the settled
    // reading including the tail — the keystroke is never lost.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting(&mut engine, true, CandidateSource::Learning);

    let result = engine.process_key(&press('k'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(
        aux.contains("[変換:📝]") && aux.contains("候補なし"),
        "aux was: {aux}"
    );
    assert_eq!(engine.candidates().unwrap().len(), 0);

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(text) if text == "あいk"))
    );
}

#[test]
fn test_alt_chords_pass_through_mid_conversion() {
    // Alt chords reach the application even where the bare keysym is
    // bound: Alt+Return must not commit, Alt+Tab must not navigate.
    let mut engine = engine_in_conversion();
    for keysym in [Keysym::RETURN, Keysym::TAB, Keysym::SPACE, Keysym::UP] {
        let result = engine.process_key(&press_alt(keysym));
        assert!(!result.consumed, "{keysym:?} was consumed");
        assert!(matches!(engine.state(), InputState::Conversion { .. }));
    }
}

#[test]
fn test_filtered_conversion_replaces_live_display() {
    // Entering a filtered conversion drops the live display like Space
    // does; otherwise the stale chunks would survive the commit and
    // render as the next composition's preedit.
    let mut engine = engine_with_learned("あい", "愛");
    engine.live.enabled = true;
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    // Pretend the model had produced a live conversion for あい.
    engine.chunks = vec![ComposingChunk {
        reading: "あい".to_string(),
        converted: "愛".to_string(),
    }];
    engine.live.shown = true;

    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📝 view: [愛]
    engine.process_key(&press_key(Keysym::RETURN)); // commit 愛

    let result = engine.process_key(&press('k'));
    let preedit = result.actions.iter().find_map(|a| match a {
        EngineAction::UpdatePreedit(p) => Some(p.text().to_string()),
        _ => None,
    });
    assert_eq!(preedit.as_deref(), Some("k"));
}

#[test]
fn test_emoji_rewriter_view_has_no_literal_query() {
    // The 🔄 view in emoji mode shows emojis only — the literal `:query`
    // pair must not ride at the tail.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press(':'));
    engine.process_key(&press('s'));
    engine.process_key(&press_ctrl(Keysym::KEY_R)); // cycle tail = 🔄
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let texts = shown_texts(&engine);
    assert!(
        texts.iter().all(|t| !t.starts_with(':')),
        "texts were: {texts:?}"
    );
}

#[test]
fn test_model_view_queries_the_settled_reading() {
    // The AI view converts the state's settled reading — the exact text
    // Enter commits — so a pending tail is reflected in its candidates,
    // never silently dropped on commit. The tail `k` is its own
    // non-Japanese chunk, passed through after the converted kana run.
    let mut engine = InputMethodEngine::new();
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('k'));
    open_model_view(&mut engine);
    assert_eq!(shown_texts(&engine), vec!["合いk"]);
}

#[test]
fn test_model_view_converts_japanese_run_and_passes_digits_through() {
    // The AI view chunks like live conversion: the Japanese run is
    // converted (via the shared conversion cache) and the trailing digit
    // run is passed through verbatim, never fed to the model.
    let mut engine = InputMethodEngine::new();
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    for ch in ['a', 'i', '1', '2', '3'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    open_model_view(&mut engine);
    assert_eq!(shown_texts(&engine), vec!["合い123"]);
}

#[test]
fn test_model_view_beams_a_tail_window_on_long_readings() {
    use crate::config::settings::StrategyMode;
    // A reading longer than one chunk: the beam window picks up the last
    // chunk_chars chars from the end, and the overflow ahead of it converts
    // top-1 into the prefix — so beam-width alternatives survive no
    // matter how long the reading grows.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 2;
    engine.config.strategy = StrategyMode::Main;
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    seed_model_cache(&mut engine, "ウエ", "合い", &["上", "植え"]);
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    open_model_view(&mut engine);
    assert_eq!(shown_texts(&engine), vec!["合い上", "合い植え"]);
}

#[test]
fn test_space_conversion_beams_the_tail_window() {
    use crate::config::settings::StrategyMode;
    // Space shares the tail-window conversion: a reading longer than the
    // window cap still gets beam-width model rows in the mixed list
    // (prefix top-1 + window beam) instead of one greedy candidate.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 2;
    engine.config.strategy = StrategyMode::Main;
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    seed_model_cache(&mut engine, "ウエ", "合い", &["上", "植え"]);
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    let texts = shown_texts(&engine);
    assert_eq!(&texts[..2], ["合い上", "合い植え"], "texts were: {texts:?}");
}

#[test]
fn test_space_head_is_the_live_grid_conversion() {
    // The window is the last chunk, so the text before it is exactly the
    // chunks live conversion already converted: every candidate carries
    // that same prefix, and the grid's own top-1 rides first.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 2;
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    seed_model_cache(&mut engine, "ウエ", "合い", &["上", "植え"]);
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    let texts = shown_texts(&engine);
    assert_eq!(&texts[..2], ["合い上", "合い植え"], "texts were: {texts:?}");
}

#[test]
fn test_model_view_head_is_the_live_grid_conversion() {
    // The AI view shares the injected head: live-grid top-1 first, then
    // the windowed beam alternatives, all carrying the same prefix.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 2;
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    seed_model_cache(&mut engine, "ウエ", "合い", &["上", "植え"]);
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    open_model_view(&mut engine);
    assert_eq!(shown_texts(&engine), vec!["合い上", "合い植え"]);
}

#[test]
fn test_dictionary_view_merges_both_dictionaries_user_first() {
    // One 📚 stop covers both dictionaries: usually the user just wants to
    // look the reading up. Their own entries come first, a surface in both
    // appears once as theirs, and each candidate still says which
    // dictionary it came from.
    let mut engine = InputMethodEngine::new();
    // Deterministic model result (a cache hit stands in for the model).
    seed_model_cache(&mut engine, "アイ", "", &["合い"]);
    engine.dicts.user = Some(dict_from_json(
        r#"[{"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}]"#,
    ));
    engine.dicts.system = Some(dict_from_json(
        r#"[{"reading":"あい","candidates":[{"surface":"藍","score":1.0},{"surface":"愛","score":2.0}]}]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    cycle_expecting_empty(&mut engine, true, CandidateSource::Learning);
    cycle_expecting_dictionary_view(&mut engine, true);

    assert_eq!(shown_texts(&engine), vec!["藍", "愛"]);
    assert_eq!(
        shown_sources(&engine),
        vec![
            Some(CandidateSource::UserDictionary),
            Some(CandidateSource::Dictionary),
        ]
    );
}

#[test]
fn test_mid_caret_typing_does_not_tail_predict() {
    // With the caret mid-composition the typed run is not a tail: あk|い
    // settles to あkい, so the narrowed view must not offer あい + か…
    // predictions whose commit would replace the composition with the
    // wrong reading.
    let mut engine = engine_with_learned("あいか", "愛香");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📝 view for あい
    assert_eq!(shown_texts(&engine), vec!["愛香"]);

    // `k` lands mid-buffer (あk|い): the prediction disappears instead of
    // narrowing as if the reading were あい + tail k.
    let result = engine.process_key(&press('k'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(
        aux.contains("[変換:📝]") && aux.contains("候補なし"),
        "aux was: {aux}"
    );

    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(text) if text == "あkい"))
    );
}

#[test]
fn test_beam_span_follows_the_chunk_grid() {
    // The window must start where live conversion actually splits, not at
    // the last non-Japanese char: a mark kept inside a Japanese chunk is
    // not a boundary, so the whole clause stays in the window and the
    // prefix is exactly the text typing already converted.
    let engine = InputMethodEngine::new();
    let chars: Vec<char> = "おい、おまえだよ".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 0);

    // A second mark does split: it becomes its own chunk, so the window is
    // the clause after it.
    let chars: Vec<char> = "おい、まて、こら".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 6);

    // Digits split by default and have nothing to beam, so a reading
    // ending in them leaves the window empty.
    let chars: Vec<char> = "へや301".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), chars.len());
}

#[test]
fn test_ai_view_respects_a_manual_chunk_break() {
    // A break the user inserted with Ctrl+J must hold through the explicit
    // conversion too: the AI view converts on the same grid, so the text
    // left of the break stays the conversion the user already saw and only
    // the chunk after it is beamed.
    let mut engine = InputMethodEngine::new();
    // Distinguishable per-chunk conversions: the second one only matches if
    // the break really split the reading (and carried its lctx along).
    seed_model_cache(&mut engine, "アイ", "", &["愛"]);
    seed_model_cache(&mut engine, "ウエ", "愛", &["上"]);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    engine.process_key(&press('u'));
    engine.process_key(&press('e'));
    assert_eq!(engine.chunk_breaks, vec![2]);

    // The window starts at the manual break, so the prefix is the frozen
    // 「愛」 and only 「うえ」 is beamed.
    let chars: Vec<char> = "あいうえ".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 2);

    engine.process_key(&press_key(Keysym::SPACE));
    open_model_view(&mut engine);
    assert_eq!(shown_texts(&engine), vec!["愛上"]);
}

#[test]
fn test_conversion_aux_shows_the_beamed_chunk() {
    // The conversion aux shows the span the alternatives cover, labelled,
    // and nothing else — the same shape the composing aux uses for the
    // chunk being typed.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 2;
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("🎯 うえ 2/2"), "the beam span is shown: {aux}");
    assert!(!aux.contains("あいうえ"), "the frozen head is not: {aux}");

    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("aux");
    assert!(
        aux.contains("🎯 あい 2/30"),
        "whole reading is the span: {aux}"
    );
}

#[test]
fn test_aux_is_quiet_by_default() {
    // The debug details are opt-in: a default engine shows the quiet line
    // composing shows plus the conversion's own fields, nothing else.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("あい"), "aux was: {aux}");
    for noise in ["ms", "🎯", "jinen"] {
        assert!(!aux.contains(noise), "`{noise}` must be opt-in: {aux}");
    }
}

#[test]
fn test_conversion_aux_reports_mode_and_chunk_like_composing() {
    // Space must not blank out what typing reported: the window leads with
    // the same mode indicator and carries the composing line's own reading
    // field, so the line only gains the conversion's own fields.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    let composing = last_aux_text(&engine.process_key(&press('i'))).expect("aux");
    assert!(
        composing.starts_with("[あ] あい 2/30"),
        "aux was: {composing}"
    );

    let aux = last_aux_text(&engine.process_key(&press_key(Keysym::SPACE))).expect("aux");
    assert!(aux.starts_with("[あ][変換]"), "aux was: {aux}");
    assert!(aux.contains("あい 2/30"), "aux was: {aux}");

    // An unfired romaji tail rides along in both states, as typed.
    let mut engine = InputMethodEngine::new();
    for ch in ['a', 'i'] {
        engine.process_key(&press(ch));
    }
    let composing = last_aux_text(&engine.process_key(&press('k'))).expect("aux");
    assert!(
        composing.starts_with("[あ] あいk 2/30"),
        "aux was: {composing}"
    );
    let aux = last_aux_text(&engine.process_key(&press_key(Keysym::SPACE))).expect("aux");
    assert!(aux.starts_with("[あ][変換] あいk 2/30"), "aux was: {aux}");

    // A reading past the cap shows the caret's chunk alone, as composing
    // does — the counter would read 4/2 against the whole reading.
    let mut engine = InputMethodEngine::new();
    engine.config.chunk_chars = 2;
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    let aux = last_aux_text(&engine.process_key(&press_key(Keysym::SPACE))).expect("aux");
    assert!(aux.contains("うえ 2/2"), "aux was: {aux}");
    assert!(!aux.contains("あいうえ"), "frozen head is not shown: {aux}");
}

#[test]
fn test_typing_in_a_filtered_view_keeps_the_reading_field() {
    // Regression: typing inside a view suppresses the suggestion, which
    // clears the chunk grid. Reading the counter off the grid made it (and
    // the romaji tail) vanish from the second keystroke on, so the field is
    // split fresh from the buffer instead.
    let mut engine = engine_in_conversion();
    open_model_view(&mut engine);

    let aux = last_aux_text(&engine.process_key(&press('k'))).expect("aux");
    assert!(
        aux.starts_with("[あ][変換:🤖] あいk 2/30"),
        "aux was: {aux}"
    );

    let aux = last_aux_text(&engine.process_key(&press('a'))).expect("aux");
    assert!(
        aux.starts_with("[あ][変換:🤖] あいか 3/30"),
        "aux was: {aux}"
    );

    let aux = last_aux_text(&engine.process_key(&press('s'))).expect("aux");
    assert!(
        aux.starts_with("[あ][変換:🤖] あいかs 3/30"),
        "aux was: {aux}"
    );
}

#[test]
fn test_filtered_view_aux_shows_the_input_mode() {
    // Shift+letter switches to direct input without leaving the window, so
    // the header is the only place that can say so.
    let mut engine = engine_in_conversion();
    open_model_view(&mut engine);

    let aux = last_aux_text(&engine.process_key(&press_shift('A'))).expect("aux");
    assert!(aux.starts_with("[A][変換:🤖]"), "aux was: {aux}");

    // Alt_R leaves the window standing and re-renders its line, so the
    // switch back to hiragana shows without waiting for a keystroke.
    let aux = last_aux_text(&engine.process_key(&press_key(Keysym::ALT_R))).expect("aux");
    assert!(aux.starts_with("[あ][変換:🤖]"), "aux was: {aux}");
}

#[test]
fn test_ctrl_j_narrows_the_window_without_leaving_the_conversion() {
    // Ctrl+J splits at the caret while the conversion is on screen: the
    // window shrinks to the text after the break and the source filter
    // survives the rebuild.
    let mut engine = InputMethodEngine::new();
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_I));
    assert!(last_aux_text(&result).expect("aux").contains("[変換:🤖]"));

    // A break at the end of the reading arms the next chunk; one at the
    // caret after moving would split. Here the caret sits at the end, so
    // the break lands there and the conversion is rebuilt in place.
    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(result.consumed);
    assert_eq!(engine.chunk_breaks, vec![4]);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        last_aux_text(&result).expect("aux").contains("[変換:🤖]"),
        "the filter must survive the rebuild"
    );
}

#[test]
fn test_model_kana_top1_survives() {
    // Words that stay in kana (きゃりーぱみゅぱみゅ) make the model's top-1
    // equal the reading. That is a real answer, not a missing one, so it
    // must reach the AI view instead of leaving it empty.
    let mut engine = InputMethodEngine::new();
    seed_model_cache(&mut engine, "アイ", "", &["あい"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_I));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:🤖]"), "aux was: {aux}");
    assert_eq!(shown_texts(&engine), vec!["あい"]);
}

#[test]
fn test_ctrl_j_in_conversion_shows_the_armed_chunk() {
    // Breaking at the end of the reading arms an empty chunk: the counter
    // restarts so the cut is visible, exactly as it does while composing.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    for ch in ['a', 'i'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("0/30"), "aux was: {aux}");
}

#[test]
fn test_beam_span_grows_up_to_the_budget() {
    // The span snaps to chunk boundaries: it grows backwards over Japanese
    // chunks while `beam_chars` allows, and never less than one chunk.
    let mut engine = InputMethodEngine::new();
    engine.config.chunk_chars = 2;
    let chars: Vec<char> = "あいうえおか".chars().collect();

    engine.config.beam_chars = 2;
    assert_eq!(engine.beam_span_start(&chars), 4, "one chunk");
    engine.config.beam_chars = 4;
    assert_eq!(engine.beam_span_start(&chars), 2, "two chunks");
    engine.config.beam_chars = 30;
    assert_eq!(engine.beam_span_start(&chars), 0, "all of it");
}

#[test]
fn test_beam_span_stops_at_a_manual_break_and_at_digits() {
    // Both walls hold however large the budget is: crossing a manual break
    // would undo the freeze the user asked for, and crossing a digit chunk
    // would hand the digits to the model.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 30;

    let chars: Vec<char> = "あいうえ".chars().collect();
    engine.chunk_breaks = vec![2];
    assert_eq!(engine.beam_span_start(&chars), 2, "manual break is a wall");

    engine.chunk_breaks.clear();
    let chars: Vec<char> = "あ12うえ".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 3, "digits are a wall");
}

#[test]
fn test_beam_span_follows_the_symbol_and_digit_settings() {
    // The span is built from `group_chunks`' own output, so the thresholds
    // that shape chunks shape the span too: a mark inside the budget keeps
    // the clause together, one past it walls the span off, and digits obey
    // `chunk_digits` the same way.
    let mut engine = InputMethodEngine::new();
    engine.config.beam_chars = 30;

    // chunk_symbols = 1: 「おい、まて」 is one chunk, the second mark walls.
    let chars: Vec<char> = "おい、まて".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 0);
    let chars: Vec<char> = "あ、い。う".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 4, "second mark walls");

    // Raising it lets both marks ride along, so the whole reading is beamed.
    engine.config.chunk_symbols = 2;
    assert_eq!(engine.beam_span_start(&chars), 0);

    // chunk_digits = 0: digits are their own chunk and wall the span.
    engine.config.chunk_symbols = 1;
    let chars: Vec<char> = "あ12うえ".chars().collect();
    assert_eq!(engine.beam_span_start(&chars), 3);

    // Raising it folds them into the Japanese chunk, so nothing walls.
    engine.config.chunk_digits = 2;
    assert_eq!(engine.beam_span_start(&chars), 0);
}

#[test]
fn test_conversion_aux_counter_uses_the_beam_budget() {
    // The span is bounded by `beam_chars`, so that is what the counter
    // counts against — using the chunk length would read like 4/2.
    let mut engine = InputMethodEngine::new();
    engine.config.verbose = true;
    engine.config.chunk_chars = 2;
    engine.config.beam_chars = 8;
    for ch in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(ch));
    }
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("🎯 あいうえ 4/8"), "aux was: {aux}");
}

#[test]
fn test_verbose_toggle_keeps_what_conversion_needs() {
    // Ctrl+Shift+V only adds or removes the debug details. What the user
    // needs to convert — the state, the reading, the candidate's source —
    // stays either way.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let quiet = last_aux_text(&engine.process_key(&press_key(Keysym::SPACE))).expect("aux");
    assert!(quiet.contains("[変換]"), "state: {quiet}");
    assert!(quiet.contains("あい"), "reading: {quiet}");
    assert!(quiet.contains("📝"), "candidate source: {quiet}");
    assert!(!quiet.contains("推論"), "timing is a detail: {quiet}");

    // The toggle re-renders the line being looked at, so the details show
    // now rather than on the next keystroke.
    let loud = last_aux_text(&engine.process_key(&press_ctrl_shift(Keysym::KEY_V))).expect("aux");
    assert!(loud.contains("[変換]"), "state: {loud}");
    assert!(loud.contains("あい"), "reading: {loud}");
    assert!(loud.contains("📝"), "candidate source: {loud}");
    assert!(loud.contains("推論"), "timing now shown: {loud}");
}

#[test]
fn test_ctrl_i_jumps_straight_to_the_ai_view() {
    // One press reaches the AI view from either state, instead of walking
    // the cycle to it.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_ctrl(Keysym::KEY_I));
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:🤖]"), "from composing: {aux}");

    // And from inside another view: one press comes back, however far the
    // cycle has wandered.
    let result = engine.process_key(&press_ctrl(Keysym::KEY_R));
    assert!(last_aux_text(&result).expect("aux").contains("[変換:📚]"));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_I));
    assert!(last_aux_text(&result).expect("aux").contains("[変換:🤖]"));
}

#[test]
fn test_filtered_view_aux_shows_what_is_being_typed() {
    // Typing refines the view in place, so the aux has to keep showing the
    // query — including an unfired romaji tail. It used to show only the
    // selected candidate's own reading, which for a predictive entry runs
    // past what was typed and left no sign of the actual input.
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[
        {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1.0}]},
        {"reading":"わせだだいがく","candidates":[{"surface":"早稲田大学","score":1.0}]}
    ]"#,
    ));
    for c in ['w', 'a', 's', 'e', 'd', 'a'] {
        engine.process_key(&press(c));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📝（候補なし）
    let result = engine.process_key(&press_ctrl(Keysym::KEY_T)); // 📚

    // An exact match commits what was typed: the query alone.
    let aux = last_aux_text(&result).expect("aux");
    assert!(aux.contains("[変換:📚] わせだ 3/30 |"), "aux was: {aux}");

    // The tail `d` is unfired, and the surviving entry is predictive: the
    // query leads, its full reading follows.
    let result = engine.process_key(&press('d'));
    let aux = last_aux_text(&result).expect("aux");
    assert!(
        aux.contains("[変換:📚] わせだd → わせだだいがく"),
        "aux was: {aux}"
    );
    assert_eq!(shown_texts(&engine), vec!["早稲田大学"]);

    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux");
    assert!(
        aux.contains("[変換:📚] わせだだ → わせだだいがく"),
        "aux was: {aux}"
    );
}
