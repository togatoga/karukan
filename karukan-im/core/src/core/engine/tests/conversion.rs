use super::*;
use crate::core::preedit::AttributeType;
use karukan_engine::{LearningCache, LearningConfig};

/// Extract the committed text from an `EngineResult`, if any.
fn commit_text_of(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.clone()),
        _ => None,
    })
}

/// Type "あいうえお" (5 kana via romaji a,i,u,e,o), move the cursor to
/// position 2, then press Space — entering Conversion with reading "あい"
/// and tail "うえお".
///
/// Seeds the learning cache with exact-match entries for both readings
/// mapped to themselves. Learning candidates are always inserted first
/// (see `build_conversion_candidates`'s "Learning → ... " priority), so the
/// default-selected candidate is guaranteed to equal the reading regardless
/// of what a loaded model would produce.
fn engine_in_partial_conversion() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("あい", "あい");
    cache.record("うえお", "うえお");
    engine.learning = Some(cache);

    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("あい"),
        "test setup: default candidate for \"あい\" must be deterministic"
    );
    engine
}

/// Like `engine_in_partial_conversion`, but the seeded learning surfaces
/// differ from the readings ("あい"→"藍", "うえお"→"ウエオ") so tests can
/// tell converted text apart from raw kana.
fn engine_in_partial_conversion_with_kanji() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("あい", "藍");
    cache.record("うえお", "ウエオ");
    engine.learning = Some(cache);

    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("藍"),
        "test setup: default candidate for \"あい\" must be deterministic"
    );
    engine
}

#[test]
fn test_left_keeps_next_segment_converted() {
    // Going back to the previous segment must NOT revert the segment
    // we just left to raw kana.
    let mut engine = engine_in_partial_conversion_with_kanji();

    // Confirm "藍", advance into "うえお" (selected "ウエオ").
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("ウエオ")
    );

    // Go back to the first segment.
    engine.process_key(&press_key(Keysym::LEFT));

    // Current segment re-selects its previous choice "藍"...
    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("藍")
    );
    // ...and the segment we left stays converted in the preedit.
    let preedit = engine.preedit().unwrap();
    assert_eq!(
        preedit.text(),
        "藍ウエオ",
        "right-side segment must keep its conversion, not revert to うえお"
    );
    assert_eq!(engine.upcoming_segments.len(), 1);
    assert_eq!(engine.confirmed_segments.len(), 0);
}

#[test]
fn test_right_reenters_upcoming_segment_with_previous_selection() {
    let mut engine = engine_in_partial_conversion_with_kanji();

    engine.process_key(&press_key(Keysym::RIGHT)); // into "うえお"
    engine.process_key(&press_key(Keysym::LEFT)); // back to "あい"
    engine.process_key(&press_key(Keysym::RIGHT)); // forward again

    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("ウエオ"),
        "re-entering the segment must restore its previous selection"
    );
    assert_eq!(engine.upcoming_segments.len(), 0);
    assert_eq!(engine.confirmed_segments.len(), 1);

    // Enter commits everything joined in order.
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text_of(&result).as_deref(), Some("藍ウエオ"));
}

#[test]
fn test_commit_includes_upcoming_segments() {
    let mut engine = engine_in_partial_conversion_with_kanji();

    engine.process_key(&press_key(Keysym::RIGHT)); // into "うえお"
    engine.process_key(&press_key(Keysym::LEFT)); // back to "あい" (upcoming: ウエオ)

    // Committing from the first segment must include the still-pending
    // converted segment to its right.
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text_of(&result).as_deref(), Some("藍ウエオ"));
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.upcoming_segments.len(), 0);
}

#[test]
fn test_cancel_restores_reading_including_upcoming_segments() {
    let mut engine = engine_in_partial_conversion_with_kanji();

    engine.process_key(&press_key(Keysym::RIGHT)); // into "うえお"
    engine.process_key(&press_key(Keysym::LEFT)); // back to "あい" (upcoming: ウエオ)

    engine.process_key(&press_key(Keysym::ESCAPE));

    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");
    assert_eq!(engine.upcoming_segments.len(), 0);
}

#[test]
fn test_shift_arrow_dissolves_upcoming_segments_without_losing_chars() {
    let mut engine = engine_in_partial_conversion_with_kanji();

    engine.process_key(&press_key(Keysym::RIGHT)); // into "うえお"
    engine.process_key(&press_key(Keysym::LEFT)); // back to "あい" (upcoming: ウエオ)

    // Moving the segment boundary invalidates downstream conversions:
    // the upcoming segment reverts to kana and total reading is preserved.
    engine.process_key(&press_shift_key(Keysym::LEFT));

    assert_eq!(engine.upcoming_segments.len(), 0);
    assert_eq!(engine.input_buf.display(), "あ");
    assert_eq!(engine.conversion_tail.as_deref(), Some("いうえお"));
}

#[test]
fn test_multiple_confirmed_segments_join_correctly_on_commit() {
    let mut engine = engine_in_partial_conversion();

    // Confirm "あい", advance into "うえお".
    engine.process_key(&press_key(Keysym::RIGHT));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.confirmed_segments.len(), 1);

    // Enter commits the confirmed segment + current selection joined in order.
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text_of(&result).as_deref(), Some("あいうえお"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_bare_right_with_no_tail_does_not_commit() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    for ch in ['a', 'i'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert!(
        commit_text_of(&result).is_none(),
        "bare Right with no tail must not commit"
    );
    assert!(
        matches!(engine.state(), InputState::Conversion { .. }),
        "state should remain Conversion"
    );
}

#[test]
fn test_bare_left_with_no_confirmed_segments_does_nothing() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    for ch in ['a', 'i'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let preedit_before = engine.preedit().unwrap().text().to_string();

    let result = engine.process_key(&press_key(Keysym::LEFT));
    assert!(result.consumed);
    assert!(commit_text_of(&result).is_none());
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.preedit().unwrap().text(), preedit_before);
}

#[test]
fn test_cancel_restores_full_reading_including_confirmed_segments_and_clears_them() {
    let mut engine = engine_in_partial_conversion();

    // Confirm "あい", advance into "うえお".
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.confirmed_segments.len(), 1);

    engine.process_key(&press_key(Keysym::ESCAPE));

    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");
    assert_eq!(
        engine.confirmed_segments.len(),
        0,
        "confirmed_segments must be cleared on cancel"
    );
}

#[test]
fn test_partial_conversion_preedit_segments_and_caret() {
    let engine = engine_in_partial_conversion();

    let preedit = engine.preedit().unwrap();
    assert_eq!(preedit.text(), "あいうえお");
    assert_eq!(
        preedit.caret(),
        2,
        "caret should sit right after the highlighted \"あい\" segment"
    );

    let attrs = preedit.attributes();
    assert_eq!(attrs.len(), 2, "expected highlight + underline segments");
    assert_eq!(attrs[0].start, 0);
    assert_eq!(attrs[0].end, 2);
    assert_eq!(attrs[0].attr_type, AttributeType::Highlight);
    assert_eq!(attrs[1].start, 2);
    assert_eq!(attrs[1].end, 5);
    assert_eq!(attrs[1].attr_type, AttributeType::Underline);
}

#[test]
fn test_ctrl_digit_selection_commits_and_learns_confirmed_segments() {
    // Uses its own fresh (unlearned) cache instead of engine_in_partial_conversion()'s
    // pre-seeded one, so it can assert that record_learning actually ran for the
    // confirmed segment — a pre-seeded cache would make that assertion vacuous.
    let mut engine = InputMethodEngine::new();
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let selected_for_ai = engine
        .candidates()
        .and_then(|c| c.selected_text())
        .unwrap()
        .to_string();

    // Confirm the first segment, advance into the tail.
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.confirmed_segments.len(), 1);

    // Select candidate 1 for the (now current) tail segment.
    let selected_for_tail = engine
        .candidates()
        .and_then(|c| c.selected_text())
        .unwrap()
        .to_string();
    let result = engine.process_key(&press_ctrl(Keysym::KEY_1));
    assert_eq!(
        commit_text_of(&result).as_deref(),
        Some(format!("{selected_for_ai}{selected_for_tail}").as_str())
    );
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.confirmed_segments.len(), 0);

    // The confirmed first segment must have been recorded in the learning cache.
    let learned = engine.learning.as_ref().unwrap().lookup("あい");
    assert!(
        learned
            .iter()
            .any(|(surface, _)| surface == &selected_for_ai),
        "confirmed segment should be recorded in the learning cache, got {:?}",
        learned
    );
}

#[test]
fn test_shrink_range_ignores_predictive_learning_match() {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    // Learn a long phrase whose reading starts with "ゆ".
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("ゆーざーじしょをかくにんして", "ユーザー辞書を確認して");
    engine.learning = Some(cache);

    // Type "ゆー" and enter conversion (2 chars).
    engine.process_key(&press_key(Keysym::LEFT)); // no-op, engine empty
    for ch in ['y', 'u'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press('-'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Shrink down to reading="ゆ" (1 char): the predictive learning match
    // "ユーザー辞書を確認して" (reading "ゆーざーじしょをかくにんして") must NOT
    // become the default selected candidate — it corresponds to a much
    // longer reading than the 1 char actually in scope.
    engine.process_key(&press_shift_key(Keysym::LEFT));

    let candidates = engine.candidates().expect("should be in conversion state");
    let selected = candidates.selected_text().unwrap_or("");
    assert_ne!(
        selected, "ユーザー辞書を確認して",
        "predictive learning candidate must not be auto-selected for a shrunk reading"
    );
    assert!(
        selected.chars().count() <= 3,
        "selected candidate {:?} is suspiciously long for a 1-char reading",
        selected
    );
}

#[test]
fn test_shrink_expand_conversion_range_does_not_duplicate_chars() {
    let mut engine = InputMethodEngine::new();

    // Type "あいうえお" and enter conversion
    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Shrink 4 times (down to 1 char reading), then expand 4 times back.
    for _ in 0..4 {
        engine.process_key(&press_shift_key(Keysym::LEFT));
    }
    for _ in 0..4 {
        engine.process_key(&press_shift_key(Keysym::RIGHT));
    }

    // Total reading length (current conversion reading + tail) must still be 5.
    let reading_len = engine.input_buf.display().chars().count();
    let tail_len = engine
        .conversion_tail
        .as_deref()
        .map(|t| t.chars().count())
        .unwrap_or(0);
    assert_eq!(
        reading_len + tail_len,
        5,
        "reading={:?} tail={:?}",
        engine.input_buf.display(),
        engine.conversion_tail.as_deref()
    );
}

#[test]
fn test_shrink_only_repeated_does_not_duplicate_chars() {
    let mut engine = InputMethodEngine::new();

    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));

    // Shrink repeatedly past the minimum (10 times on a 5-char reading).
    for _ in 0..10 {
        engine.process_key(&press_shift_key(Keysym::LEFT));
    }

    let reading_len = engine.input_buf.display().chars().count();
    let tail_len = engine
        .conversion_tail
        .as_deref()
        .map(|t| t.chars().count())
        .unwrap_or(0);
    assert_eq!(
        reading_len + tail_len,
        5,
        "reading={:?} tail={:?}",
        engine.input_buf.display(),
        engine.conversion_tail.as_deref()
    );
}

#[test]
fn test_expand_only_repeated_does_not_duplicate_chars() {
    let mut engine = InputMethodEngine::new();

    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));

    for _ in 0..4 {
        engine.process_key(&press_shift_key(Keysym::LEFT));
    }
    // Expand repeatedly past the maximum (10 times when only 4 chars in tail).
    for _ in 0..10 {
        engine.process_key(&press_shift_key(Keysym::RIGHT));
    }

    let reading_len = engine.input_buf.display().chars().count();
    let tail_len = engine
        .conversion_tail
        .as_deref()
        .map(|t| t.chars().count())
        .unwrap_or(0);
    assert_eq!(
        reading_len + tail_len,
        5,
        "reading={:?} tail={:?}",
        engine.input_buf.display(),
        engine.conversion_tail.as_deref()
    );
}

#[test]
fn test_advance_then_shrink_expand_does_not_duplicate_chars() {
    let mut engine = InputMethodEngine::new();

    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    // Move cursor to middle before converting: "あい" | "うえお"
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Right arrow: advance to next segment (confirm "あい", convert "うえお")
    engine.process_key(&press_key(Keysym::RIGHT));

    // Now shrink/expand repeatedly on the "うえお" segment.
    for _ in 0..5 {
        engine.process_key(&press_shift_key(Keysym::LEFT));
    }
    for _ in 0..5 {
        engine.process_key(&press_shift_key(Keysym::RIGHT));
    }

    let reading_len = engine.input_buf.display().chars().count();
    let tail_len = engine
        .conversion_tail
        .as_deref()
        .map(|t| t.chars().count())
        .unwrap_or(0);
    assert_eq!(
        reading_len + tail_len,
        3,
        "reading={:?} tail={:?}",
        engine.input_buf.display(),
        engine.conversion_tail.as_deref()
    );
}

#[test]
fn test_shrink_expand_multi_char_romaji_does_not_duplicate_chars() {
    let mut engine = InputMethodEngine::new();

    // Type "きょうと" via romaji: kyouto
    for ch in ['k', 'y', 'o', 'u', 't', 'o'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let expected_len = engine.input_buf.display().chars().count()
        + engine
            .conversion_tail
            .as_deref()
            .map(|t| t.chars().count())
            .unwrap_or(0);

    for _ in 0..3 {
        engine.process_key(&press_shift_key(Keysym::LEFT));
    }
    for _ in 0..3 {
        engine.process_key(&press_shift_key(Keysym::RIGHT));
    }

    let reading_len = engine.input_buf.display().chars().count();
    let tail_len = engine
        .conversion_tail
        .as_deref()
        .map(|t| t.chars().count())
        .unwrap_or(0);
    assert_eq!(
        reading_len + tail_len,
        expected_len,
        "reading={:?} tail={:?}",
        engine.input_buf.display(),
        engine.conversion_tail.as_deref()
    );
}

#[test]
fn test_conversion_char_refines_reading() {
    let mut engine = InputMethodEngine::new();

    // Type "あい" and enter conversion
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Typing during conversion must NOT commit — it drops back to the
    // composition and extends the reading (incremental-search feel).
    let result = engine.process_key(&press('k'));
    assert!(result.consumed);
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_))),
        "typing must refine, not commit"
    );
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    engine.process_key(&press('a'));
    assert_eq!(engine.input_buf.reading(), "あいか");

    // The refined reading converts and commits as one unit.
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_)))
    );
}

#[test]
fn test_alphabet_mode_space_inserts_literal_space() {
    let mut engine = InputMethodEngine::new();

    // Enter alphabet mode via Shift+N
    engine.process_key(&press_shift('N'));
    assert!(engine.mode.current() == InputMode::Alphabet);

    // Type "ew"
    engine.process_key(&press('e'));
    engine.process_key(&press('w'));
    assert_eq!(engine.preedit().unwrap().text(), "New");

    // Space → should insert literal space, NOT start conversion
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "New ");

    // Type "york"
    engine.process_key(&press('y'));
    engine.process_key(&press('o'));
    engine.process_key(&press('r'));
    engine.process_key(&press('k'));
    assert_eq!(engine.preedit().unwrap().text(), "New york");
}

#[test]
fn test_stray_keys_are_consumed_during_conversion() {
    // Unbound chords and special keys must be consumed as no-ops while the
    // conversion window is shown — leaking them would let the application
    // act on them (e.g. Ctrl+R reloading a browser page) mid-conversion.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    let cursor = engine.candidates().unwrap().cursor();

    for key in [
        press_ctrl(Keysym(0x0067)), // Ctrl+g (unbound)
        press_ctrl(Keysym(0x0077)), // Ctrl+w (unbound; closes a browser tab)
        press_key(Keysym(0xffc2)),  // F5
    ] {
        let result = engine.process_key(&key);
        assert!(result.consumed, "key must not leak to the application");
        assert!(matches!(engine.state(), InputState::Conversion { .. }));
        assert_eq!(engine.candidates().unwrap().cursor(), cursor);
    }
}

/// Text of the first Commit action in a result, if any.
fn committed(result: &EngineResult) -> Option<String> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.clone()),
        _ => None,
    })
}

#[test]
fn test_bare_digit_during_conversion_refines_instead_of_selecting() {
    // Digits are plain text input everywhere: during conversion they extend
    // the reading like any printable char, never select a candidate.
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[{"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press('2'));
    assert!(committed(&result).is_none(), "a digit must not commit");
    assert_eq!(engine.input_buf.reading(), "あい2");
}

#[test]
fn test_ctrl_digit_selects_candidate_during_conversion() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.user = Some(dict_from_json(
        r#"[{"reading":"あい","candidates":[
            {"surface":"藍","score":2.0},
            {"surface":"愛","score":1.0}
        ]}]"#,
    ));

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    let shown: Vec<String> = engine
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();

    let result = engine.process_key(&press_ctrl(Keysym::KEY_2));
    assert_eq!(committed(&result).as_deref(), Some(shown[1].as_str()));
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.input_buf.is_empty(), "buffer must be cleared");
}

#[test]
fn test_ctrl_digit_selects_candidate_while_composing() {
    // The suggestion window is on screen while composing, so Ctrl+digit
    // commits straight from it — no Space needed first.
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    engine.dicts.user = Some(dict_from_json(
        r#"[{"reading":"あい","candidates":[{"surface":"藍","score":1.0}]}]"#,
    ));

    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));
    let shown = result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::ShowCandidates(list) => Some(list.candidates().to_vec()),
            _ => None,
        })
        .expect("suggestion window");
    // The dictionary entry's position depends on what else the suggestion
    // list holds (a loaded model contributes its own row), so select it by
    // the digit it is actually shown under.
    let digit = shown
        .iter()
        .position(|c| c.text == "藍")
        .expect("dictionary candidate in the suggestion window")
        + 1;
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    let result = engine.process_key(&press_ctrl(Keysym(b'0' as u32 + digit as u32)));
    assert_eq!(committed(&result).as_deref(), Some("藍"));
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_ctrl_digit_with_no_suggestion_is_consumed() {
    // Nothing to select: the chord must still be swallowed rather than
    // leaking to the application mid-composition.
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;

    engine.process_key(&press('a'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_9));
    assert!(result.consumed);
    assert!(committed(&result).is_none());
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn test_emoji_digit_selection_does_not_pollute_learning() {
    // Committing an emoji by number must not record `:query` → 😀 into the
    // kana-keyed learning cache, and must leave emoji mode.
    let mut engine = engine_with_learned("あい", "愛");
    engine.process_key(&press(':'));
    assert_eq!(engine.mode.current(), InputMode::Emoji);
    for ch in "smile".chars() {
        engine.process_key(&press(ch));
    }

    let result = engine.process_key(&press_ctrl(Keysym::KEY_1));
    assert!(committed(&result).is_some(), "emoji must commit");
    assert_eq!(engine.mode.current(), InputMode::Hiragana);
    let learned = engine.learning.as_ref().unwrap();
    assert!(
        learned.lookup(":smile").is_empty(),
        "emoji query must not enter the learning cache"
    );
}

#[test]
fn test_home_end_in_conversion_return_to_composing_and_move_caret() {
    // The arrow keys walk the segments, so Home/End are the caret keys that
    // dissolve the conversion and edit the raw composition — matching live
    // conversion, where a caret key ends the converted display.
    let mut engine = InputMethodEngine::new();
    for ch in "kyou".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_buf.cursor(), 3); // き ょ う
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::HOME));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.cursor(), 0, "caret must move to the start");

    engine.process_key(&press_key(Keysym::END));
    assert_eq!(engine.input_buf.cursor(), 3);
}

#[test]
fn test_home_in_source_view_dissolves_the_filter() {
    // From the Ctrl+I model view: the caret key exits to editing and the
    // filter dies with the conversion state.
    let mut engine = InputMethodEngine::new();
    for ch in "kyou".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_ctrl(Keysym::KEY_I));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_key(Keysym::HOME));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.cursor(), 0);
    assert!(engine.state().filter().is_none());
}

#[test]
fn test_ctrl_b_in_conversion_moves_caret_like_left() {
    let mut engine = InputMethodEngine::new();
    for ch in "kyou".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let result = engine.process_key(&press_ctrl(Keysym::KEY_B));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_buf.cursor(), 2);
}

#[test]
fn test_mid_buffer_conversion_does_not_adopt_longer_predictive_candidate() {
    // `あい|さつ` + Space converts only `あい`; a predictive learning match
    // (`挨拶`, reading あいさつ) must not become the default selection — the
    // tail `さつ` is still in the preedit, so committing it would duplicate
    // those characters (`挨拶さつ`).
    let mut engine = InputMethodEngine::new();
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("あいさつ", "挨拶");
    engine.learning = Some(cache);

    for ch in ['a', 'i', 's', 'a', 't', 's', 'u'] {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_buf.display(), "あいさつ");
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));

    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.conversion_tail.as_deref(), Some("さつ"));

    let selected = engine
        .candidates()
        .and_then(|c| c.selected_text())
        .unwrap_or("");
    assert_ne!(
        selected, "挨拶",
        "predictive learning candidate must not be auto-selected when a tail remains"
    );
}

#[test]
fn test_home_after_confirming_a_segment_keeps_the_confirmed_text() {
    // Same for the caret keys: Home dissolves the conversion, so the
    // composition it lands in must hold everything that was on screen.
    let mut engine = engine_in_partial_conversion_with_kanji();
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.confirmed_segments.len(), 1);

    engine.process_key(&press_key(Keysym::HOME));

    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(engine.confirmed_segments.is_empty());
    assert_eq!(engine.input_buf.display(), "あいうえお");
    assert_eq!(engine.input_buf.cursor(), 0);
}

#[test]
fn test_segment_advance_from_a_filtered_view_keeps_the_text() {
    // Segment navigation rebuilds the conversion, which dissolves the source
    // filter (like the caret keys do). What must not happen is losing the
    // segment that was on screen.
    let mut engine = engine_in_partial_conversion_with_kanji();
    engine.process_key(&press_ctrl(Keysym::KEY_T));
    assert_eq!(engine.state().filter(), Some(CandidateSource::Learning));

    let result = engine.process_key(&press_key(Keysym::RIGHT));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.state().filter(), None);
    assert_eq!(engine.confirmed_segments.len(), 1);
    assert_eq!(engine.preedit().unwrap().text(), "藍ウエオ");
}

#[test]
fn test_refining_after_confirming_a_segment_keeps_the_confirmed_text() {
    // Typing during conversion refines the reading (the incremental
    // conversion of togatoga/karukan#95) by dropping back to the
    // composition. A segment already confirmed with → must come back with
    // it — otherwise the confirmed text survives only in a field nothing
    // displays and is lost on commit.
    let mut engine = engine_in_partial_conversion_with_kanji();
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.confirmed_segments.len(), 1);

    engine.process_key(&press('k'));
    engine.process_key(&press('a'));

    // Everything goes back to its reading, as it does on Escape: the whole
    // composition is what the refined conversion runs over next.
    assert!(engine.confirmed_segments.is_empty());
    assert!(engine.conversion_tail.is_none());
    assert_eq!(engine.input_buf.display(), "あいうえおか");
}

#[test]
fn test_live_conversion_entry_keeps_displayed_text_selected() {
    // Entering conversion from live conversion (Left arrow) must keep the
    // displayed live text selected even when it is already present in the
    // rebuilt candidate list (deduplicated instead of re-inserted at the
    // top) and a predictive learning candidate outranks it.
    let mut engine = InputMethodEngine::new();
    engine.live.enabled = true;
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record("あいさつ", "挨拶");
    engine.learning = Some(cache);

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    // "アイ" is also produced as a katakana fallback candidate, so the live
    // text gets deduplicated against the list instead of inserted at index 0.
    set_live_text(&mut engine, "アイ");

    engine.process_key(&press_key(Keysym::LEFT));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(
        engine.candidates().and_then(|c| c.selected_text()),
        Some("アイ"),
        "the text displayed during live conversion must stay selected"
    );
}
