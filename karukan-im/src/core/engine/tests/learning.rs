//! Tests for the learning cache and the Tab-skips-learning behavior.
//!
//! Space/Down: include learning candidates (default conversion).
//! Tab: skip learning candidates (lets users escape stale learned entries).
//! Ctrl+Delete: delete the selected learning candidate from the history.

use karukan_engine::{LearningCache, LearningConfig};

use super::*;
use crate::core::engine::display::LEARNING_DELETE_HINT;

/// Engine seeded with a learning entry `reading → surface`, no kanji model.
/// We bypass `init.rs` (which gates learning on settings + file I/O) and just
/// inject a populated `LearningCache` directly — these tests assert the
/// build_conversion_candidates branching, not the load path.
fn engine_with_learned(reading: &str, surface: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(LearningConfig::default());
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
    assert!(
        selected.is_deletable(),
        "learning candidate must be flagged"
    );

    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(result.consumed);
    // The entry is gone from the cache...
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    // ...and the window stays up: the conversion is rebuilt in place,
    // staying in Conversion.
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

    // `藍` is no longer a *learning* candidate. It may still return from the
    // model or dictionary (deleting history doesn't blacklist a surface — the
    // whole point of rebuilding instead of dropping the row), but never again
    // flagged as user history.
    let candidates = engine.state().candidates().unwrap();
    assert!(
        !candidates
            .candidates()
            .iter()
            .any(|c| c.text == "藍" && c.is_deletable()),
        "`藍` must no longer be a learning candidate after deletion",
    );
    // The rebuilt list reopens at the top.
    assert_eq!(candidates.cursor(), 0);
}

#[test]
fn ctrl_delete_removes_prefix_twins_so_surface_does_not_resurface() {
    // The same surface learned under two prefix-related readings is shown as a
    // single deduped row; deleting it must clear both, or the twin under the
    // longer reading pops back on the next conversion of the same input.
    let mut engine = engine_with_learned("あい", "藍");
    engine.learning.as_mut().unwrap().record("あいさ", "藍");

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
    assert_eq!(selected.text, "藍");
    assert!(selected.is_deletable());

    engine.process_key(&press_ctrl(Keysym::DELETE));
    // Both the exact and the prefix entry are gone.
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("あいさ")
            .is_empty(),
        "the prefix twin (あいさ→藍) must be cleared too, not just the exact entry",
    );
}

#[test]
fn ctrl_delete_keeps_surface_that_another_source_also_produces() {
    // #2 regression: the learned surface equals the hiragana reading, which
    // the fallback ALWAYS produces. That fallback copy is deduped away under
    // the learning entry; deleting the entry must bring it back (now
    // non-learning) rather than remove the only row — which is why deletion
    // rebuilds the conversion instead of dropping the candidate in place.
    let mut engine = engine_with_learned("あい", "あい");

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
    assert_eq!(selected.text, "あい");
    assert!(selected.is_deletable());

    engine.process_key(&press_ctrl(Keysym::DELETE));
    assert!(engine.learning.as_ref().unwrap().lookup("あい").is_empty());

    // `あい` survives as an ordinary fallback candidate.
    let candidates = engine.state().candidates().unwrap();
    let ai = candidates.candidates().iter().find(|c| c.text == "あい");
    assert!(
        ai.is_some(),
        "the fallback `あい` must survive the deletion, not vanish with the \
         learning entry",
    );
    assert!(
        !ai.unwrap().is_deletable(),
        "the surviving `あい` must no longer be flagged as learning",
    );
}

#[test]
fn init_learning_cache_applies_configured_surface_cap() {
    // Guards the config→cache seam: if init_learning_cache stops applying
    // max_surface_chars, the 6-char surface (over the configured 5, under
    // the default 50) gets recorded and this fails.
    let mut engine = InputMethodEngine::new();
    engine.init_learning_cache(
        true,
        LearningConfig {
            max_entries: 10_000,
            max_surface_chars: 5,
        },
    );
    let cache = engine.learning.as_mut().expect("learning enabled");

    let before = cache.entry_count();
    cache.record("__karukan_seam_test__", &"漢".repeat(6));
    assert_eq!(
        cache.entry_count(),
        before,
        "a surface over the configured cap must be skipped; the configured \
         value is not reaching the cache",
    );
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
fn ctrl_backspace_does_nothing_for_non_learning_candidate() {
    // When the selection isn't a learning candidate, Ctrl+Backspace (like
    // Ctrl+Delete) is consumed so it can't leak to the app mid-conversion,
    // but the conversion is left intact. Cancelling stays on plain
    // Backspace / Escape.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    // Move the selection off the learning candidate.
    engine.process_key(&press_key(Keysym::SPACE));
    let before = engine.state().candidates().unwrap().clone();
    assert!(!before.selected().unwrap().is_deletable());

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(
        result.consumed,
        "the chord must be consumed, not leak to the app"
    );
    assert!(
        matches!(engine.state(), InputState::Conversion { .. }),
        "Ctrl+Backspace must not cancel when the selection isn't deletable"
    );
    // Nothing changed: same selection, same list, history intact.
    let after = engine.state().candidates().unwrap();
    assert_eq!(after.cursor(), before.cursor());
    assert_eq!(after.selected_text(), before.selected_text());
    assert!(!engine.learning.as_ref().unwrap().lookup("あい").is_empty());
}

#[test]
fn ctrl_backspace_in_composing_deletes_char_not_history() {
    // History deletion is a Conversion-state-only chord. During Composing,
    // Ctrl+Backspace edits text like plain Backspace, even while a learning
    // suggestion is on screen.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    let result = engine.process_key(&press_ctrl(Keysym::BACKSPACE));
    assert!(result.consumed);
    assert_eq!(engine.input_buf.text, "あ");
    assert!(
        !engine.learning.as_ref().unwrap().lookup("あい").is_empty(),
        "the learning entry must survive — deletion only works in Conversion",
    );
}

#[test]
fn ctrl_alt_delete_leaves_history_alone() {
    // Ctrl+Alt+Delete is a desktop chord, not ours. The delete arm guards on
    // `!alt_key` like its siblings, so the key passes through untouched
    // instead of irreversibly purging a history entry.
    let mut engine = engine_with_learned("あい", "藍");

    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(
        engine
            .state()
            .candidates()
            .unwrap()
            .selected()
            .unwrap()
            .is_deletable()
    );

    let result = engine.process_key(&press_ctrl_alt(Keysym::DELETE));
    assert!(!result.consumed, "Ctrl+Alt+Delete must reach the desktop");
    assert!(
        !engine.learning.as_ref().unwrap().lookup("あい").is_empty(),
        "Ctrl+Alt+Delete must not delete the learning entry"
    );
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
    assert!(!selected.is_deletable());

    let before_len = engine.state().candidates().unwrap().len();
    let result = engine.process_key(&press_ctrl(Keysym::DELETE));
    // The key is consumed (it must not leak to the app mid-conversion) but
    // nothing is deleted and the conversion continues.
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
    assert!(selected.is_deletable());

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

    // Learning candidate selected → aux carries the deletion hint.
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
