//! Tests for the internal ComposingChunk splitting (`chunked_auto_suggest`).
//!
//! These run without a loaded model, so each chunk's `converted` text falls
//! back to its own `reading`. That is enough to verify the partitioning, the
//! per-chunk left-context (lctx) relationship, and current-chunk tracking,
//! which are all model-independent.

use super::*;
use crate::core::engine::EngineConfig;

/// Engine with a small chunk length so chunks form with short test input.
fn make_chunk_engine(chunk_chars: usize) -> InputMethodEngine {
    let config = EngineConfig {
        chunk_chars,
        // These tests assert on the aux line, which is quiet by default.
        verbose: true,
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
    assert_eq!(engine.input_buf.reading(), "あいうえ");

    // 4 chars / N=2 → two chunks, each exactly N chars.
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ"]);
    for chunk in &engine.chunks {
        assert!(chunk.reading.chars().count() <= 2);
    }
}

#[test]
fn test_typed_punctuation_is_absorbed() {
    // Real keystroke path: "," → "、" and "." → "。" via romaji. One mark
    // rides along with the Japanese around it, so the clause keeps
    // reconverting as one unit instead of freezing at the mark; the second
    // mark opens a new chunk.
    let mut engine = make_chunk_engine(40);
    for k in ['h', 'a', ',', 'j', 'i'] {
        engine.process_key(&press(k));
    }
    assert_eq!(engine.input_buf.reading(), "は、じ");
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["は、じ"]);

    engine.process_key(&press('.')); // 。 — the chunk already holds one mark
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["は、じ", "。"]);
}

#[test]
fn test_long_digit_run_forms_its_own_chunk() {
    // Real keystroke path: a digit run longer than the symbol cap (2) splits
    // off whole into its own chunk — never torn mid-run — and is passed
    // through verbatim, never sent to the model.
    let mut engine = make_chunk_engine(40);
    for k in ['a', '1', '2', '3', '4', 'i'] {
        engine.process_key(&press(k));
    }
    assert_eq!(engine.input_buf.reading(), "あ1234い");
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あ", "1234", "い"]);
}

#[test]
fn test_non_japanese_chunk_passes_through_and_japanese_stays_cached() {
    // Digits are absorbed while the run fits the cap (2); the keystroke that
    // a digit run too long for the budget opens its own verbatim chunk,
    // and the Japanese chunk — unchanged reading and lctx — stays a cache
    // hit.
    let mut engine = make_chunk_engine(40);
    seed_model_cache(&mut engine, "アイ", "", &["KEEP"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "KEEP");

    for k in ['1', '2', '3'] {
        engine.process_key(&press(k));
    }
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "123"]);
    assert_eq!(engine.chunks[0].converted, "KEEP"); // cache hit, not reconverted
    assert_eq!(engine.chunks[1].converted, "123"); // non-Japanese chunk verbatim
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
fn test_symbols_within_budget_stay_in_one_chunk() {
    // With a large chunk length and one symbol (within the absorb budget),
    // the whole clause is a single chunk sent to the model — no premature
    // boundary at the punctuation.
    let mut engine = make_chunk_engine(40);
    engine.input_buf.clear();
    engine.input_buf.insert("あ、いうえ");
    engine.chunked_auto_suggest();

    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あ、いうえ"]);
}

#[test]
fn test_short_buffer_is_a_single_chunk() {
    // With the default chunk length, short input is one chunk — identical
    // to a whole-buffer conversion (no behavior change for the common case).
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.input_buf.reading(), "あい");
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

    let budget = engine.config.context_chars;
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
    assert_eq!(engine.input_buf.reading_cursor(), 0);
    assert_eq!(engine.current_chunk_index(), 0);
}

#[test]
fn test_current_chunk_index_with_variable_length_chunks() {
    // A long digit run produces a variable-length chunk layout, so the index
    // must be found by walking actual chunk lengths — not a fixed
    // cursor / chunk_chars division.
    let mut engine = make_chunk_engine(40);
    engine.input_buf.clear();
    engine.input_buf.insert("あ1234いう"); // chunks ["あ", "1234", "いう"]
    engine.chunked_auto_suggest();
    assert_eq!(engine.chunks.len(), 3);

    // cursor pos → expected chunk index
    for (pos, expected) in [(0, 0), (1, 0), (2, 1), (5, 1), (6, 2), (7, 2)] {
        engine.input_buf.set_cursor(pos);
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
    assert_eq!(engine.input_buf.reading(), "あいう");
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
    assert_eq!(engine.input_buf.reading(), "");
    assert!(engine.chunks.is_empty(), "chunk cache must be cleared");
    assert!(engine.live_text().is_empty(), "live text must be cleared");
}

/// Type `あいうえおか` (6 hiragana chars) via romaji.
fn type_aiueoka(engine: &mut InputMethodEngine) {
    for k in ['a', 'i', 'u', 'e', 'o', 'k', 'a'] {
        engine.process_key(&press(k));
    }
}

#[test]
fn test_delete_first_chunk_reconverts_survivor_with_new_lctx() {
    // Deleting the first chunk changes the survivor's left context (it is now
    // the leading chunk), so its old conversion is a cache miss and it is
    // reconverted with the correct, updated lctx — unlike the old prefix/suffix
    // reuse, which kept a conversion made against a context that no longer
    // exists.
    let mut engine = make_chunk_engine(2);
    // Pin the first chunk's conversion so the second chunk's lctx is a
    // deterministic "あい" regardless of whether a real model is loaded.
    // While typing, chunk "うえ" is converted with that lctx; after the
    // delete its lctx is empty.
    seed_model_cache(&mut engine, "アイ", "", &["あい"]);
    seed_model_cache(&mut engine, "ウエ", "あい", &["OLD"]);
    seed_model_cache(&mut engine, "ウエ", "", &["NEW"]);
    type_aiue(&mut engine); // "あいうえ" → ["あい", "うえ"]
    assert_eq!(engine.chunks.len(), 2);
    assert_eq!(engine.chunks[1].converted, "OLD");

    // Delete the first chunk's two chars ("あい") from the front.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::DELETE));
    engine.process_key(&press_key(Keysym::DELETE));

    assert_eq!(engine.input_buf.reading(), "うえ");
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["うえ"]);
    assert_eq!(engine.chunks[0].converted, "NEW");
}

#[test]
fn test_middle_delete_repartitions_and_keeps_leading_cache_hit() {
    // The chunking is a pure function of the current text: after a middle
    // delete, "あいえおか" partitions fresh as ["あい", "えお", "か"] (no
    // path-dependent ["あい", "え", "おか"]). The leading chunk — unchanged
    // reading and lctx — stays a cache hit; everything at and after the edit
    // is reconverted.
    let mut engine = make_chunk_engine(2);
    seed_model_cache(&mut engine, "アイ", "", &["S0"]);
    type_aiueoka(&mut engine); // "あいうえおか" → ["あい", "うえ", "おか"]
    assert_eq!(engine.chunks.len(), 3);
    assert_eq!(engine.chunks[0].converted, "S0");

    // Cursor after う (pos 3), backspace deletes う — inside the middle chunk.
    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::BACKSPACE));

    assert_eq!(engine.input_buf.reading(), "あいえおか");
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "えお", "か"]);
    // The untouched leading chunk is still served from cache.
    assert_eq!(engine.chunks[0].converted, "S0");
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
fn test_ctrl_j_starts_a_new_chunk() {
    // Ctrl+J places a manual boundary at the caret: the settled text stops
    // reconverting and the next keystroke starts a fresh chunk.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine); // "あいうえ" → one chunk
    assert_eq!(engine.chunks.len(), 1);

    engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert_eq!(engine.chunk_breaks, vec![4]);

    engine.process_key(&press('o')); // "あいうえお"
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あいうえ", "お"]);
}

#[test]
fn test_ctrl_j_at_end_shows_empty_new_chunk_in_aux() {
    // A break armed at the end of the reading has no chunk text yet; the aux
    // must switch to the new empty chunk (0/max) so the user can see the cut
    // happened at all.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("0/40"), "aux was: {aux}");
    assert_eq!(engine.current_chunk_index(), engine.chunks.len());
    // The new chunk's lctx is everything before the break.
    assert!(aux.contains("lctx:"), "aux was: {aux}");

    // The next keystroke opens the chunk for real: 1 char used.
    let result = engine.process_key(&press('o'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("お 1/40"), "aux was: {aux}");
}

#[test]
fn test_caret_on_manual_break_tracks_right_chunk() {
    // On a mid-reading manual break the next keystroke joins the right-hand
    // chunk (the boundary stays left of the insert), so that is the chunk
    // the aux shows.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT)); // caret between い and う
    let result = engine.process_key(&press_ctrl(Keysym::KEY_J)); // ["あい", "うえ"]
    assert_eq!(engine.current_chunk_index(), 1);
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("うえ 2/40"), "aux was: {aux}");
}

#[test]
fn test_ctrl_j_freezes_left_chunk_conversion() {
    // The chunk left of a manual boundary keeps its reading and lctx as
    // typing continues, so it stays a cache hit and never flickers.
    let mut engine = make_chunk_engine(40);
    seed_model_cache(&mut engine, "アイウエ", "", &["KEEP"]);
    type_aiue(&mut engine);
    assert_eq!(engine.chunks[0].converted, "KEEP");

    engine.process_key(&press_ctrl(Keysym::KEY_J));
    engine.process_key(&press('o'));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    engine.process_key(&press('k'));
    engine.process_key(&press('a'));
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あいうえ", "か"]);
    assert_eq!(engine.chunks[0].converted, "KEEP");
}

#[test]
fn test_ctrl_j_overrides_symbol_absorption() {
    // Absorption would keep 「あいうえ、」 one chunk; a manual boundary at
    // the mark forces the split the user asked for.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    engine.process_key(&press(',')); // 、
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["あいうえ", "、"]);
}

#[test]
fn test_manual_break_shifts_with_edits_to_its_left() {
    // Typing before a manual boundary moves it with the text, so it keeps
    // pointing at the same spot in the reading.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    engine.process_key(&press('o')); // ["あいうえ", "お"]

    engine.process_key(&press_key(Keysym::HOME));
    engine.process_key(&press('k'));
    engine.process_key(&press('a')); // "かあいうえお"
    assert_eq!(engine.input_buf.reading(), "かあいうえお");
    let readings: Vec<&str> = engine.chunks.iter().map(|c| c.reading.as_str()).collect();
    assert_eq!(readings, vec!["かあいうえ", "お"]);
}

#[test]
fn test_manual_break_cleared_on_commit() {
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(!engine.chunk_breaks.is_empty());

    engine.process_key(&press_key(Keysym::RETURN));
    assert!(engine.chunk_breaks.is_empty());
}

#[test]
fn test_manual_break_dropped_when_erased_past() {
    // Backspacing the right-hand chunk away leaves the boundary at the end
    // of the reading (still armed); erasing further keeps it in range.
    let mut engine = make_chunk_engine(40);
    type_aiue(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_J));
    engine.process_key(&press('o')); // ["あいうえ", "お"]

    for _ in 0..5 {
        engine.process_key(&press_key(Keysym::BACKSPACE));
    }
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(engine.chunk_breaks.is_empty());
}

#[test]
fn test_aux_shows_current_chunk_with_fill_counter() {
    // The aux reading is the chunk under the caret plus a used/max counter,
    // and it follows cursor movement across chunks.
    let mut engine = make_chunk_engine(2);
    engine.process_key(&press('a'));
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("ああ 2/2"), "aux was: {aux}");

    type_aiue(&mut engine); // "ああ" + "あいうえ" → ["ああ", "あい", "うえ"]
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    let result = engine.process_key(&press_key(Keysym::LEFT)); // caret inside "あい"
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("あい 2/2"), "aux was: {aux}");
}

#[test]
fn test_append_keeps_leading_chunks_cached() {
    // Typing at the end leaves the leading chunks' readings and lctx
    // unchanged, so they are cache hits — only the tail chunk is converted.
    let mut engine = make_chunk_engine(2);
    seed_model_cache(&mut engine, "アイ", "", &["KEEP0"]);
    type_aiue(&mut engine); // ["あい", "うえ"]
    assert_eq!(engine.chunks[0].converted, "KEEP0");

    engine.process_key(&press('o')); // "あいうえお"
    let readings: Vec<&str> = engine.chunks.iter().map(|s| s.reading.as_str()).collect();
    assert_eq!(readings, vec!["あい", "うえ", "お"]);
    assert_eq!(engine.chunks[0].converted, "KEEP0");
}
