//! Tests for the learning cache and the Tab-skips-learning behavior.
//!
//! Space/Down: include learning candidates (default conversion).
//! Tab: skip learning candidates (lets users escape stale learned entries).
//! Ctrl+Delete: delete the selected learning candidate from the history
//! (mozc's DeleteSelectedCandidate).

use karukan_engine::LearningCache;

use super::*;
use crate::core::engine::display::LEARNING_DELETE_HINT;

/// Last UpdateAuxText emitted by an engine result, if any.
fn last_aux_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().rev().find_map(|a| match a {
        EngineAction::UpdateAuxText(text) => Some(text.clone()),
        _ => None,
    })
}

/// Engine seeded with a learning entry `reading → surface`, no kanji model.
/// We bypass `init.rs` (which gates learning on settings + file I/O) and just
/// inject a populated `LearningCache` directly — these tests assert the
/// build_conversion_candidates branching, not the load path.
fn engine_with_learned(reading: &str, surface: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(100);
    cache.record(reading, surface);
    engine.learning = Some(cache);
    engine
}

#[test]
fn build_candidates_includes_learning_when_not_skipped() {
    let mut engine = engine_with_learned("あい", "藍");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, false)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        texts.contains(&"藍".to_string()),
        "Space path (skip_learning=false) should surface learned `藍`, got {:?}",
        texts,
    );
}

#[test]
fn build_candidates_omits_learning_when_skipped() {
    let mut engine = engine_with_learned("あい", "藍");

    let texts: Vec<String> = engine
        .build_conversion_candidates("あい", 9, true)
        .into_iter()
        .map(|c| c.text)
        .collect();

    assert!(
        !texts.contains(&"藍".to_string()),
        "Tab path (skip_learning=true) must drop learned `藍`, got {:?}",
        texts,
    );
}

#[test]
fn tab_key_skips_learning_in_composing() {
    // End-to-end: type the reading, press Tab → learned candidate is gone.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.text, "あい");

    let result = engine.process_key(&press_key(Keysym::TAB));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let texts: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert!(
        !texts.contains(&"藍".to_string()),
        "Tab must skip the learned `藍` candidate, got {:?}",
        texts,
    );
}

#[test]
fn ctrl_delete_removes_selected_learning_entry() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    // Learning candidates are force-pushed first, so the learned entry is
    // the initial selection.
    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "藍");
    assert!(selected.from_learning, "learning candidate must be flagged");

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    // The entry is gone from the cache...
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    // ...and the candidate disappears from the open list in place: mozc
    // blinks its window closed and reopen; karukan stays in Conversion with
    // the window up.
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::ShowCandidates(_))),
        "deletion must refresh the candidate window, not close it"
    );
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::HideCandidates)),
        "deletion must not hide the candidate window"
    );

    let candidates = engine.state().candidates().unwrap();
    let texts: Vec<&str> = candidates
        .candidates()
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        !texts.contains(&"藍"),
        "deleted `藍` must leave the list, got {:?}",
        texts,
    );
    // The selection index is preserved — the next candidate slides in.
    // (Which text that is depends on whether the kanji model is available
    // in the test environment, so only the position is asserted.)
    assert_eq!(candidates.cursor(), 0);
    assert_ne!(candidates.selected_text(), Some("藍"));
}

#[test]
fn ctrl_backspace_deletes_learning_entry_like_ctrl_delete() {
    // Mac keyboards label the Backspace key "delete", so the natural macOS
    // chord is Ctrl+delete = Ctrl+Backspace; it must behave like Ctrl+Delete
    // (forward delete), not like the plain-Backspace cancel.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
}

#[test]
fn plain_backspace_still_cancels_conversion() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let result = engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(result.consumed);
    // Backspace without Ctrl keeps its cancel-to-composing behavior and
    // deletes nothing from the history.
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_delete_ignores_non_learning_candidate() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    // Move the selection off the learning candidate onto a fallback one.
    engine.process_key(&press_key(Keysym::SPACE));
    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert!(!selected.from_learning);

    let before_len = engine.state().candidates().unwrap().len();
    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    // mozc's DoNothing: the key is consumed (it must not leak to the app
    // mid-conversion) but nothing is deleted and the conversion continues.
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert_eq!(engine.state().candidates().unwrap().len(), before_len);
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_delete_removes_prefix_matched_entry_by_full_reading() {
    // A prefix-matched learning candidate carries its own (longer) reading;
    // deletion must remove the cache entry under that full reading.
    let mut engine = engine_with_learned("あいさつ", "挨拶");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));

    let selected = engine
        .state()
        .candidates()
        .unwrap()
        .selected()
        .unwrap()
        .clone();
    assert_eq!(selected.text, "挨拶");
    assert_eq!(selected.reading.as_deref(), Some("あいさつ"));
    assert!(selected.from_learning);

    engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("あいさつ")
            .is_empty()
    );
}

#[test]
fn aux_shows_delete_hint_only_for_learning_candidate() {
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Learning candidate selected → aux carries the mozc-style footer hint.
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("conversion must update aux text");
    assert!(
        aux.contains(LEARNING_DELETE_HINT),
        "aux should show the deletion hint for a learning candidate, got {:?}",
        aux,
    );

    // Moving to a non-learning candidate drops the hint.
    let result = engine.process_key(&press_key(Keysym::SPACE));
    let aux = last_aux_text(&result).expect("navigation must update aux text");
    assert!(
        !aux.contains(LEARNING_DELETE_HINT),
        "aux must not show the deletion hint for non-learning candidates, got {:?}",
        aux,
    );
}

#[test]
fn space_key_keeps_learning_in_composing() {
    // Counterpart to tab_key_skips_learning_in_composing: Space stays on the
    // learning-included path so the default UX is unchanged.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let texts: Vec<String> = engine
        .state()
        .candidates()
        .unwrap()
        .candidates()
        .iter()
        .map(|c| c.text.clone())
        .collect();
    assert!(
        texts.contains(&"藍".to_string()),
        "Space must surface learned `藍`, got {:?}",
        texts,
    );
}
