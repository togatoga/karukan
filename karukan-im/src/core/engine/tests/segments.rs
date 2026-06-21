//! Tests for the internal ComposingSegment splitting (`segmented_auto_suggest`).
//!
//! These run without a loaded model, so each segment's `converted` text falls
//! back to its own `reading`. That is enough to verify the partitioning, the
//! per-segment left-context (lctx) relationship, and current-segment tracking,
//! which are all model-independent.

use super::*;
use crate::core::engine::EngineConfig;

/// Engine with a small segment length so segments form with short test input.
fn make_segment_engine(seg_len: usize) -> InputMethodEngine {
    let config = EngineConfig {
        composing_segment_len: seg_len,
        ..EngineConfig::default()
    };
    InputMethodEngine::with_config(config)
}

/// Type `あいうえ` (4 hiragana chars) via romaji.
fn type_aiue(engine: &mut InputMethodEngine) {
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    engine.process_key(&press('e'));
}

#[test]
fn test_buffer_split_into_segments_of_n_chars() {
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine);
    assert_eq!(engine.input_buf.text, "あいうえ");

    // 4 chars / N=2 → two segments, each exactly N chars.
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ"]);
    for seg in &engine.segments {
        assert!(seg.reading.chars().count() <= 2);
    }
}

#[test]
fn test_short_buffer_is_a_single_segment() {
    // With the default segment length, short input is one segment — identical
    // to a whole-buffer conversion (no behavior change for the common case).
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.text, "あい");
    assert_eq!(engine.segments.len(), 1);
    assert_eq!(engine.segments[0].reading, "あい");
}

#[test]
fn test_last_segment_may_be_shorter_than_n() {
    let mut engine = make_segment_engine(2);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u')); // "あいう" → ["あい", "う"]
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "う"]);
}

/// Tail of `s` limited to `budget` chars (mirrors `truncate_context`).
fn ctx_tail(s: &str, budget: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(budget);
    chars[start..].iter().collect()
}

#[test]
fn test_segment_lctx_is_left_segment_value() {
    // Segment 0 has no left context; each later segment's lctx is the converted
    // text of all preceding segments (truncated to the context budget) — i.e.
    // "the value of the left segment(s)", independent of what the model emits.
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine);
    assert!(engine.segments.len() >= 2);

    let budget = engine.config.max_api_context_len;
    let mut left = String::new();
    for seg in &engine.segments {
        assert_eq!(seg.lctx, ctx_tail(&left, budget));
        left.push_str(&seg.converted);
    }
    assert_eq!(engine.segments[0].lctx, "");
}

#[test]
fn test_current_segment_index_tracks_cursor() {
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine); // cursor at end (pos 4) → segment 1
    assert_eq!(engine.current_segment_index(), 1);

    // Move cursor to the left edge of the buffer → segment 0.
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.input_buf.cursor_pos, 0);
    assert_eq!(engine.current_segment_index(), 0);
}

#[test]
fn test_backspace_reconverts_last_segment_partition() {
    // Deleting a char at the end re-partitions: the final segment shrinks while
    // earlier segments keep their readings (and are served from cache).
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine); // ["あい", "うえ"]
    assert_eq!(engine.segments.len(), 2);

    engine.process_key(&press_key(Keysym::BACKSPACE)); // "あいう" → ["あい", "う"]
    assert_eq!(engine.input_buf.text, "あいう");
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "う"]);
    // First segment keeps an empty left context; the surviving last segment's
    // left context is the first segment's converted value.
    assert_eq!(engine.segments[0].lctx, "");
    assert_eq!(engine.segments[1].lctx, engine.segments[0].converted);
}

#[test]
fn test_segments_cleared_on_reset() {
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.segments.is_empty());

    engine.reset();
    assert!(engine.segments.is_empty());
}

#[test]
fn test_segments_cleared_on_commit() {
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.segments.is_empty());

    engine.process_key(&press_key(Keysym::RETURN));
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.segments.is_empty());
}

#[test]
fn test_delete_all_chars_clears_segments() {
    // Erasing every character ends the composition (back to Empty). The segment
    // cache and live-conversion text must be cleared too, so nothing from the
    // previous composition leaks into the next one's preedit.
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.segments.is_empty());

    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::BACKSPACE));
    }
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_buf.text, "");
    assert!(engine.segments.is_empty(), "segment cache must be cleared");
    assert!(engine.live.text.is_empty(), "live text must be cleared");
}

/// Type `あいうえおか` (6 hiragana chars) via romaji.
fn type_aiueoka(engine: &mut InputMethodEngine) {
    for k in ['a', 'i', 'u', 'e', 'o', 'k', 'a'] {
        engine.process_key(&press(k));
    }
}

#[test]
fn test_delete_first_segment_reuses_remaining_suffix() {
    // Deleting the first segment leaves the rest as an unchanged common suffix,
    // so the surviving segment is REUSED (not reconverted) to save cost — its
    // cached conversion is kept even though it is now the leading segment.
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine); // "あいうえ" → ["あい", "うえ"]
    assert_eq!(engine.segments.len(), 2);
    engine.segments[1].converted = "SENTINEL".to_string();

    // Delete the first segment's two chars ("あい") from the front.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::DELETE));
    engine.process_key(&press_key(Keysym::DELETE));

    assert_eq!(engine.input_buf.text, "うえ");
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["うえ"]);
    // Reused from the suffix → cached conversion survives (no reconvert).
    assert_eq!(engine.segments[0].converted, "SENTINEL");
}

#[test]
fn test_middle_delete_reconverts_only_touched_segment() {
    // Deleting a character inside the middle segment reconverts ONLY that
    // segment; the leading and trailing neighbors are reused untouched.
    let mut engine = make_segment_engine(2);
    type_aiueoka(&mut engine); // "あいうえおか" → ["あい", "うえ", "おか"]
    assert_eq!(engine.segments.len(), 3);
    engine.segments[0].converted = "S0".to_string();
    engine.segments[2].converted = "S2".to_string();

    // Cursor after う (pos 3), backspace deletes う — inside the middle segment.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::BACKSPACE));

    assert_eq!(engine.input_buf.text, "あいえおか");
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "え", "おか"]);
    // Neighbors reused (sentinels survive); only the middle segment reconverted.
    assert_eq!(engine.segments[0].converted, "S0");
    assert_eq!(engine.segments[2].converted, "S2");
}

#[test]
fn test_aux_text_lctx_is_current_segment_lctx() {
    // The aux line shows a single `lctx:` — the current segment's actual left
    // context (here the conversion of the first segment) — not a separate
    // per-segment fragment on top of the editor surrounding context.
    let mut engine = make_segment_engine(2);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    let result = engine.process_key(&press('e')); // "あいうえ" → 2 segments, cursor in #2

    let aux = result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::UpdateAuxText(t) => Some(t.clone()),
            _ => None,
        })
        .expect("aux text action");

    let seg_lctx = engine.segments[engine.current_segment_index()].lctx.clone();
    assert!(!seg_lctx.is_empty());
    assert!(aux.contains(&format!("lctx: {seg_lctx}")), "aux was: {aux}");
    // No redundant separate segment fragment.
    assert!(
        !aux.contains("seg "),
        "aux should have a single lctx: {aux}"
    );
}

#[test]
fn test_append_reuses_leading_segments() {
    // Typing at the end reuses every existing segment and only converts the new
    // tail segment.
    let mut engine = make_segment_engine(2);
    type_aiue(&mut engine); // ["あい", "うえ"]
    engine.segments[0].converted = "KEEP0".to_string();

    engine.process_key(&press('o')); // "あいうえお"
    let readings: Vec<&str> = engine.segments.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ", "お"]);
    // The leading segment was reused, not reconverted.
    assert_eq!(engine.segments[0].converted, "KEEP0");
}
