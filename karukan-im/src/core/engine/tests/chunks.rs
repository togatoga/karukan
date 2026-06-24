//! Tests for the internal ComposingChunk splitting (`chunked_auto_suggest`).
//!
//! These run without a loaded model, so each chunk's `converted` text falls
//! back to its own `reading`. That is enough to verify the partitioning, the
//! per-chunk left-context (lctx) relationship, and current-chunk tracking,
//! which are all model-independent.

use super::*;
use crate::core::engine::EngineConfig;

/// Engine with a small chunk length so chunks form with short test input.
fn make_chunk_engine(chunk_len: usize) -> InputMethodEngine {
    let config = EngineConfig {
        composing_chunk_len: chunk_len,
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
fn test_buffer_split_into_chunks_of_n_chars() {
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine);
    assert_eq!(engine.input_buf.text, "あいうえ");

    // 4 chars / N=2 → two chunks, each exactly N chars.
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ"]);
    for chunk in &engine.chunks {
        assert!(chunk.reading.chars().count() <= 2);
    }
}

#[test]
fn test_typed_punctuation_splits_chunks() {
    // Real keystroke path: "," → "、" and "." → "。" via romaji. Punctuation is
    // non-Japanese, so each mark forms its own chunk and separates the clauses.
    let mut engine = make_chunk_engine(40);
    for k in ['h', 'a', ',', 'j', 'i', '.', 'm', 'e'] {
        engine.process_key(&press(k));
    }
    assert_eq!(engine.input_buf.text, "は、じ。め");
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["は", "、", "じ", "。", "め"]);
}

#[test]
fn test_typed_digits_form_their_own_chunk() {
    // Real keystroke path: a digit run typed amid hiragana is split into its
    // own chunk so it is passed through verbatim, never sent to the model
    // (which tends to drop digits mid-run).
    let mut engine = make_chunk_engine(40);
    for k in ['a', '1', '2', '3', 'i'] {
        engine.process_key(&press(k));
    }
    assert_eq!(engine.input_buf.text, "あ123い");
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あ", "123", "い"]);
}

#[test]
fn test_non_japanese_chunk_passes_through_and_reuses_japanese() {
    // Appending a digit after a Japanese chunk does NOT reopen/reconvert that
    // chunk: the digit starts its own non-Japanese chunk (passed through
    // verbatim), so the Japanese chunk is reused with its cached conversion.
    let mut engine = make_chunk_engine(40);
    engine.process_key(&press('a'));
    engine.process_key(&press('i')); // "あい" → one Japanese chunk
    assert_eq!(engine.chunks.len(), 1);
    engine.chunks[0].converted = "KEEP".to_string();

    engine.process_key(&press('1')); // "あい1"
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "1"]);
    assert_eq!(engine.chunks[0].converted, "KEEP"); // reused, not reconverted
    assert_eq!(engine.chunks[1].converted, "1"); // non-Japanese chunk verbatim
}

#[test]
fn test_katakana_word_with_prolonged_mark_stays_one_chunk() {
    // スーパーマーケット contains the prolonged sound mark ー but is all
    // Japanese, so it must NOT be split into latin chunks.
    let mut engine = make_chunk_engine(40);
    engine.input_buf.clear();
    engine.input_buf.insert("スーパーマーケット");
    engine.chunked_auto_suggest();
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["スーパーマーケット"]);
}

#[test]
fn test_chunks_break_at_punctuation() {
    // With a large chunk length nothing is split by char count, so the only
    // boundaries come from group changes: each punctuation mark is its own
    // non-Japanese chunk, separating the Japanese clauses around it.
    let mut engine = make_chunk_engine(40);
    engine.input_buf.clear();
    engine.input_buf.insert("あ、いう。え");
    engine.chunked_auto_suggest();

    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あ", "、", "いう", "。", "え"]);
}

#[test]
fn test_short_buffer_is_a_single_chunk() {
    // With the default chunk length, short input is one chunk — identical
    // to a whole-buffer conversion (no behavior change for the common case).
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.text, "あい");
    assert_eq!(engine.chunks.len(), 1);
    assert_eq!(engine.chunks[0].reading, "あい");
}

#[test]
fn test_last_chunk_may_be_shorter_than_n() {
    let mut engine = make_chunk_engine(2);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u')); // "あいう" → ["あい", "う"]
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "う"]);
}

/// Tail of `s` limited to `budget` chars (mirrors `truncate_context`).
fn ctx_tail(s: &str, budget: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    let start = chars.len().saturating_sub(budget);
    chars[start..].iter().collect()
}

#[test]
fn test_chunk_lctx_is_left_chunk_value() {
    // Chunk 0 has no left context; each later chunk's lctx is the converted
    // text of all preceding chunks (truncated to the context budget) — i.e.
    // "the value of the left chunk(s)", independent of what the model emits.
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine);
    assert!(engine.chunks.len() >= 2);

    let budget = engine.config.max_api_context_len;
    let mut left = String::new();
    for i in 0..engine.chunks.len() {
        // lctx is derived on demand from the preceding chunks' converted text.
        assert_eq!(engine.chunk_lctx(i), ctx_tail(&left, budget));
        left.push_str(&engine.chunks[i].converted);
    }
    assert_eq!(engine.chunk_lctx(0), "");
}

#[test]
fn test_current_chunk_index_tracks_cursor() {
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine); // cursor at end (pos 4) → chunk 1
    assert_eq!(engine.current_chunk_index(), 1);

    // Move cursor to the left edge of the buffer → chunk 0.
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.input_buf.cursor_pos, 0);
    assert_eq!(engine.current_chunk_index(), 0);
}

#[test]
fn test_current_chunk_index_with_variable_length_chunks() {
    // Punctuation produces single-char non-Japanese chunks, so the index must
    // be found by walking actual chunk lengths — not a fixed cursor / chunk_len
    // division.
    let mut engine = make_chunk_engine(40);
    engine.input_buf.clear();
    engine.input_buf.insert("は、じ。め"); // chunks ["は", "、", "じ", "。", "め"]
    engine.chunked_auto_suggest();
    assert_eq!(engine.chunks.len(), 5);

    // cursor pos → expected chunk index
    for (pos, expected) in [(0, 0), (1, 0), (2, 1), (3, 2), (4, 3), (5, 4)] {
        engine.input_buf.cursor_pos = pos;
        assert_eq!(
            engine.current_chunk_index(),
            expected,
            "cursor pos {pos} should be in chunk {expected}"
        );
    }
}

#[test]
fn test_backspace_reconverts_last_chunk_partition() {
    // Deleting a char at the end re-partitions: the final chunk shrinks while
    // earlier chunks keep their readings (and are served from cache).
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine); // ["あい", "うえ"]
    assert_eq!(engine.chunks.len(), 2);

    engine.process_key(&press_key(Keysym::BACKSPACE)); // "あいう" → ["あい", "う"]
    assert_eq!(engine.input_buf.text, "あいう");
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "う"]);
    // First chunk keeps an empty left context; the surviving last chunk's
    // left context is the first chunk's converted value.
    assert_eq!(engine.chunk_lctx(0), "");
    assert_eq!(engine.chunk_lctx(1), engine.chunks[0].converted);
}

#[test]
fn test_chunks_cleared_on_reset() {
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.chunks.is_empty());

    engine.reset();
    assert!(engine.chunks.is_empty());
}

#[test]
fn test_chunks_cleared_on_commit() {
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.chunks.is_empty());

    engine.process_key(&press_key(Keysym::RETURN));
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.chunks.is_empty());
}

#[test]
fn test_delete_all_chars_clears_chunks() {
    // Erasing every character ends the composition (back to Empty). The chunk
    // cache and live-conversion text must be cleared too, so nothing from the
    // previous composition leaks into the next one's preedit.
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine);
    assert!(!engine.chunks.is_empty());

    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::BACKSPACE));
    }
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_buf.text, "");
    assert!(engine.chunks.is_empty(), "chunk cache must be cleared");
    assert!(engine.live.text.is_empty(), "live text must be cleared");
}

/// Type `あいうえおか` (6 hiragana chars) via romaji.
fn type_aiueoka(engine: &mut InputMethodEngine) {
    for k in ['a', 'i', 'u', 'e', 'o', 'k', 'a'] {
        engine.process_key(&press(k));
    }
}

#[test]
fn test_delete_first_chunk_reuses_remaining_suffix() {
    // Deleting the first chunk leaves the rest as an unchanged common suffix,
    // so the surviving chunk is REUSED (not reconverted) to save cost — its
    // cached conversion is kept even though it is now the leading chunk.
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine); // "あいうえ" → ["あい", "うえ"]
    assert_eq!(engine.chunks.len(), 2);
    engine.chunks[1].converted = "SENTINEL".to_string();

    // Delete the first chunk's two chars ("あい") from the front.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::DELETE));
    engine.process_key(&press_key(Keysym::DELETE));

    assert_eq!(engine.input_buf.text, "うえ");
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["うえ"]);
    // Reused from the suffix → cached conversion survives (no reconvert).
    assert_eq!(engine.chunks[0].converted, "SENTINEL");
}

#[test]
fn test_middle_delete_reconverts_only_touched_chunk() {
    // Deleting a character inside the middle chunk reconverts ONLY that
    // chunk; the leading and trailing neighbors are reused untouched.
    let mut engine = make_chunk_engine(2);
    type_aiueoka(&mut engine); // "あいうえおか" → ["あい", "うえ", "おか"]
    assert_eq!(engine.chunks.len(), 3);
    engine.chunks[0].converted = "S0".to_string();
    engine.chunks[2].converted = "S2".to_string();

    // Cursor after う (pos 3), backspace deletes う — inside the middle chunk.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::BACKSPACE));

    assert_eq!(engine.input_buf.text, "あいえおか");
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "え", "おか"]);
    // Neighbors reused (sentinels survive); only the middle chunk reconverted.
    assert_eq!(engine.chunks[0].converted, "S0");
    assert_eq!(engine.chunks[2].converted, "S2");
}

#[test]
fn test_aux_text_lctx_is_current_chunk_lctx() {
    // The aux line shows a single `lctx:` — the current chunk's actual left
    // context (here the conversion of the first chunk) — not a separate
    // per-chunk fragment on top of the editor surrounding context.
    let mut engine = make_chunk_engine(2);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    let result = engine.process_key(&press('e')); // "あいうえ" → 2 chunks, cursor in #2

    let aux = result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::UpdateAuxText(t) => Some(t.clone()),
            _ => None,
        })
        .expect("aux text action");

    let chunk_lctx = engine.chunk_lctx(engine.current_chunk_index());
    assert!(!chunk_lctx.is_empty());
    assert!(
        aux.contains(&format!("lctx: {chunk_lctx}")),
        "aux was: {aux}"
    );
    // No redundant separate chunk fragment.
    assert!(
        !aux.contains("chunk "),
        "aux should have a single lctx: {aux}"
    );
}

#[test]
fn test_append_reuses_leading_chunks() {
    // Typing at the end reuses every existing chunk and only converts the new
    // tail chunk.
    let mut engine = make_chunk_engine(2);
    type_aiue(&mut engine); // ["あい", "うえ"]
    engine.chunks[0].converted = "KEEP0".to_string();

    engine.process_key(&press('o')); // "あいうえお"
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ", "お"]);
    // The leading chunk was reused, not reconverted.
    assert_eq!(engine.chunks[0].converted, "KEEP0");
}
