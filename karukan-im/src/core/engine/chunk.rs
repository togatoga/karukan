//! Live-conversion chunking of the composing buffer.
//!
//! The composing buffer is split into internal [`ComposingChunk`]s so each
//! model call stays bounded for long input. Chunking asks one question per
//! character — Japanese or not (see [`is_japanese`]) — and starts a new chunk
//! whenever the current one is full or that answer changes. A Japanese run is
//! sent to the neural converter; a non-Japanese run (digits / symbols /
//! alphabet) is passed through verbatim. Re-chunking after an edit is
//! incremental: only the changed middle span is reconverted ([`ChunkPlan`]).

use tracing::debug;

use super::*;

/// Number of leading chars shared by `a` and `b`.
fn common_prefix_len(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Number of trailing chars shared by `a` and `b`, capped so it never overlaps
/// the already-counted common prefix of length `prefix_len`.
fn common_suffix_len(a: &[char], b: &[char], prefix_len: usize) -> usize {
    let max = a.len().min(b.len()) - prefix_len;
    let mut n = 0;
    while n < max && a[a.len() - 1 - n] == b[b.len() - 1 - n] {
        n += 1;
    }
    n
}

/// Whether `c` is "Japanese": hiragana, katakana (including the prolonged
/// sound mark `ー`), or a CJK ideograph (kanji).
///
/// Everything else — ASCII / full-width digits, letters, and symbols, plus all
/// punctuation — is non-Japanese. Chunking only ever asks this one question:
/// Japanese text goes to the neural converter, a non-Japanese run is passed
/// through to the preedit verbatim (the model otherwise tends to drop or
/// mangle digits in the middle of a run such as `123456`). Because punctuation
/// is non-Japanese it naturally separates clauses — `今日は。明日` chunks as
/// `今日は` / `。` / `明日` — so no separate punctuation rule is needed.
///
/// The middle dot `・` (U+30FB) sits in the katakana block but is a separator
/// symbol, so it is special-cased as non-Japanese: `ジョン・スミス` splits into
/// `ジョン` / `・` / `スミス` with the `・` passed through verbatim. A katakana
/// word like `スーパーマーケット` has no `・` and is entirely Japanese (the `ー`
/// stays Japanese), so it remains one chunk.
fn is_japanese(c: char) -> bool {
    // 中黒 (・): a katakana-block separator, treated as a non-Japanese symbol.
    if c == '\u{30FB}' {
        return false;
    }
    matches!(c,
        '\u{3040}'..='\u{309F}'   // hiragana
        | '\u{30A0}'..='\u{30FF}' // katakana (incl. ー U+30FC)
        | '\u{3400}'..='\u{9FFF}' // CJK ideographs (kanji)
    )
}

/// Whether two characters belong to the same chunk group (both Japanese, or
/// both non-Japanese). A new chunk starts whenever this changes — that and the
/// length cap are the only chunk boundaries.
fn same_group(a: char, b: char) -> bool {
    is_japanese(a) == is_japanese(b)
}

/// Split `chars` into chunks, starting a new chunk whenever the current one is
/// full (`max` chars) or the group changes (Japanese ⇄ non-Japanese, see
/// [`is_japanese`]). So a maximal Japanese run and a maximal non-Japanese run
/// each become their own chunk(s), and a run longer than `max` is hard-split
/// into `max`-char pieces.
fn group_chunks(chars: &[char], max: usize) -> Vec<&[char]> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let limit = (start + max).min(chars.len());
        let japanese = is_japanese(chars[start]);
        let mut i = start;
        while i < limit && is_japanese(chars[i]) == japanese {
            i += 1;
        }
        out.push(&chars[start..i]);
        start = i;
    }
    out
}

/// How to re-chunk the buffer after an edit, derived purely from the previous
/// chunking and the new text — no engine or model needed (so it is unit
/// tested directly).
///
/// The new buffer is diffed against the old chunking by common character
/// prefix/suffix: whole chunks inside the unchanged prefix/suffix are kept,
/// and only the `mid_start..mid_end` span (in chars of the new text) has to be
/// re-chunked and reconverted.
#[derive(Debug, PartialEq, Eq)]
struct ChunkPlan {
    /// Leading old chunks to reuse verbatim.
    lead_count: usize,
    /// Trailing old chunks to reuse (cached conversion kept).
    trail_count: usize,
    /// Char offset in the new text where the changed span begins (= leading chars).
    mid_start: usize,
    /// Char offset in the new text where the changed span ends (= len - trailing chars).
    mid_end: usize,
}

impl ChunkPlan {
    /// Diff `old_text` (the concatenated readings of the previous chunks,
    /// whose individual char lengths are `old_lens`) against the new `text`.
    fn compute(old_lens: &[usize], old_text: &[char], text: &[char], chunk_len: usize) -> Self {
        let cp = common_prefix_len(old_text, text);
        let cs = common_suffix_len(old_text, text, cp);

        // Leading whole chunks that lie entirely inside the unchanged prefix.
        let mut lead_count = 0;
        let mut lead_chars = 0;
        while lead_count < old_lens.len() && lead_chars + old_lens[lead_count] <= cp {
            lead_chars += old_lens[lead_count];
            lead_count += 1;
        }
        // Reopen the last leading chunk when it sits right at the edit and is
        // not yet full, so an append/edit merges into it instead of spawning a
        // stray short chunk (keeps forward typing at one growing chunk). But not
        // across a group boundary: a non-Japanese char never merges into a
        // Japanese chunk (or vice versa) — re-chunking would only split them
        // apart again, forcing a needless reconversion.
        if lead_count > 0
            && lead_chars == cp
            && cp < text.len()
            && old_lens[lead_count - 1] < chunk_len
            && same_group(old_text[lead_chars - 1], text[cp])
        {
            lead_count -= 1;
            lead_chars -= old_lens[lead_count];
        }

        // Trailing whole chunks inside the unchanged suffix, without crossing
        // into the leading region.
        let mut trail_count = 0;
        let mut trail_chars = 0;
        while trail_count < old_lens.len() - lead_count {
            let idx = old_lens.len() - 1 - trail_count;
            if trail_chars + old_lens[idx] <= cs {
                trail_chars += old_lens[idx];
                trail_count += 1;
            } else {
                break;
            }
        }

        Self {
            lead_count,
            trail_count,
            mid_start: lead_chars,
            mid_end: text.len() - trail_chars,
        }
    }
}

impl InputMethodEngine {
    /// Auto-suggest over the composing buffer, split into chunks of at most
    /// `config.composing_chunk_len` reading characters so each model call
    /// stays bounded for long input.
    ///
    /// Re-chunking is *incremental* and content-anchored: the new buffer is
    /// diffed against the previous chunking (`self.chunks`) by common
    /// character prefix/suffix. Chunks that fall entirely in the unchanged
    /// prefix are reused as-is, chunks entirely in the unchanged suffix keep
    /// their cached conversion, and only the changed middle span is re-chunked
    /// and re-run through the model. So a keystroke at the end reconverts only
    /// the final chunk, and an edit/deletion in the middle reconverts only the
    /// chunk(s) it touched — not everything downstream.
    ///
    /// Trade-off: a middle edit changes the left context of the chunks to its
    /// right, but those suffix chunks are *not* reconverted (that is the whole
    /// point — bounded cost). Their displayed conversion stays as last computed
    /// until they are themselves edited or the text is committed. Each chunk's
    /// left context is still the editor surrounding text plus the converted text
    /// of all preceding chunks, truncated to `max_api_context_len`.
    ///
    /// Returns the concatenated conversion of the whole buffer, or `None` when
    /// it equals the raw reading (no useful model suggestion).
    ///
    /// Note: for input no longer than one chunk (the common case, default
    /// N=30) this produces exactly one model call over the whole buffer, i.e.
    /// identical behavior to a whole-buffer conversion.
    pub(super) fn chunked_auto_suggest(&mut self) -> Option<String> {
        let full_reading = self.input_buf.text.clone();
        if full_reading.is_empty() {
            self.chunks.clear();
            return None;
        }
        self.ensure_kanji_converter();

        let chunk_len = self.chunk_len();
        let text: Vec<char> = full_reading.chars().collect();
        let base_ctx = self.truncate_context_for_api();

        // Previous chunking (covers the pre-edit text). Move it out so the
        // model calls below don't conflict with borrowing `self.chunks`.
        let mut old = std::mem::take(&mut self.chunks);
        let old_lens: Vec<usize> = old.iter().map(|s| s.reading.chars().count()).collect();
        let old_text: Vec<char> = old.iter().flat_map(|s| s.reading.chars()).collect();

        let plan = ChunkPlan::compute(&old_lens, &old_text, &text, chunk_len);

        let mut chunks: Vec<ComposingChunk> = Vec::with_capacity(old.len() + 1);
        let mut combined = String::new();

        // 1. Reused leading chunks — reading + converted still valid (their left
        //    context is unchanged because everything before them is unchanged).
        for chunk in old.drain(..plan.lead_count) {
            combined.push_str(&chunk.converted);
            chunks.push(chunk);
        }
        // `old` now starts at the first non-leading chunk; the trailing
        // chunks to keep are its last `trail_count` entries.
        let trail_start = old.len() - plan.trail_count;

        // 2. Changed middle span: re-chunk and reconvert each new chunk against
        //    everything converted so far.
        let middle = &text[plan.mid_start..plan.mid_end];
        for chunk in group_chunks(middle, chunk_len) {
            let reading: String = chunk.iter().collect();
            let new = self.convert_new_chunk(reading, &base_ctx, &combined);
            combined.push_str(&new.converted);
            chunks.push(new);
        }

        // 3. Reused trailing chunks — cached conversion kept (the left context
        //    it was converted with may have drifted, but we don't reconvert).
        for chunk in old.drain(trail_start..) {
            combined.push_str(&chunk.converted);
            chunks.push(chunk);
        }

        let reconverted = chunks.len() - plan.lead_count - plan.trail_count;
        self.chunks = chunks;
        self.log_chunk_state("convert");
        debug!(
            "chunked_auto_suggest: reused {} leading + {} trailing chunk(s), reconverted {} middle chunk(s)",
            plan.lead_count, plan.trail_count, reconverted
        );

        (combined != full_reading).then_some(combined)
    }

    /// Build one freshly-converted chunk for `reading`, whose left context is
    /// `base_ctx` plus everything converted so far (`combined`). A non-Japanese
    /// reading (digits / symbols / alphabet) is passed through verbatim — never
    /// sent to the model, which tends to drop digits mid-run; a Japanese
    /// reading is converted with that left context. The reading is
    /// group-homogeneous, so its first char decides. See [`is_japanese`].
    fn convert_new_chunk(
        &mut self,
        reading: String,
        base_ctx: &str,
        combined: &str,
    ) -> ComposingChunk {
        let converted = if reading.chars().next().is_some_and(is_japanese) {
            let lctx = self.lctx_for(base_ctx, combined);
            self.convert_chunk(&reading, &lctx)
        } else {
            reading.clone()
        };
        ComposingChunk { reading, converted }
    }

    /// Configured maximum chunk length in chars, clamped to at least 1.
    fn chunk_len(&self) -> usize {
        self.config.composing_chunk_len.max(1)
    }

    /// The left context (lctx) a chunk is built with: the editor surrounding
    /// text `base` followed by the converted text of every preceding chunk,
    /// truncated to the API context budget. Defined once so the context the
    /// model is given at conversion time (`convert_new_chunk`) stays identical
    /// to the one displayed in the aux text (`chunk_lctx`).
    fn lctx_for(&self, base: &str, preceding_converted: &str) -> String {
        self.truncate_context(&format!("{base}{preceding_converted}"))
    }

    /// Left context for the chunk at `index`: the editor surrounding text plus
    /// the converted text of every preceding chunk, truncated to the context
    /// budget. Derived on demand (the chunk doesn't store it) — it is just "the
    /// value of the chunks to the left".
    pub(super) fn chunk_lctx(&self, index: usize) -> String {
        let base = self.truncate_context_for_api();
        let preceding: String = self.chunks[..index.min(self.chunks.len())]
            .iter()
            .map(|c| c.converted.as_str())
            .collect();
        self.lctx_for(&base, &preceding)
    }

    /// Best-effort lazy init of the kanji converter. Chunking proceeds even
    /// on failure so `self.chunks` always mirrors the current buffer (which
    /// chunk the cursor is in, etc.); `run_kana_kanji_conversion` handles a
    /// missing converter by yielding nothing, and each chunk falls back to its
    /// own reading.
    fn ensure_kanji_converter(&mut self) {
        if self.converters.kanji.is_none()
            && let Err(e) = self.init_kanji_converter()
        {
            debug!("Failed to initialize kanji converter: {}", e);
        }
    }

    /// Model conversion of one chunk's `reading` given `lctx`, falling back to
    /// the reading itself when the model yields nothing.
    fn convert_chunk(&mut self, reading: &str, lctx: &str) -> String {
        self.run_kana_kanji_conversion(reading, lctx, 1)
            .into_iter()
            .next()
            .unwrap_or_else(|| reading.to_string())
    }

    /// Index of the chunk the cursor currently sits in, found by walking the
    /// actual chunk lengths (chunks are variable-length — group splits and the
    /// length cap — so a fixed `cursor / chunk_len` is wrong). This is the
    /// chunk a character insert/delete at the cursor lands in. Returns 0 for an
    /// empty buffer or a cursor at the very start.
    pub(super) fn current_chunk_index(&self) -> usize {
        let pos = self.input_buf.cursor_pos.saturating_sub(1);
        let mut end = 0;
        for (i, chunk) in self.chunks.iter().enumerate() {
            end += chunk.reading.chars().count();
            if pos < end {
                return i;
            }
        }
        self.chunks.len().saturating_sub(1)
    }

    /// Emit a debug line describing the current chunking: how many chunks
    /// exist and which one — and how long — the cursor currently sits in. `at`
    /// labels the call site (e.g. `"convert"` after re-chunking, `"cursor"`
    /// after a caret move) so the log shows chunk changes on cursor movement,
    /// not just on conversion.
    pub(super) fn log_chunk_state(&self, at: &str) {
        let current = self.current_chunk_index();
        let current_len = self
            .chunks
            .get(current)
            .map(|chunk| chunk.reading.chars().count())
            .unwrap_or(0);
        debug!(
            "chunks [{}]: {} chunk(s); cursor at pos {} in chunk {} ({} char(s))",
            at,
            self.chunks.len(),
            self.input_buf.cursor_pos,
            current,
            current_len
        );
    }
}

#[cfg(test)]
mod group_chunk_tests {
    use super::group_chunks;

    fn split(s: &str, max: usize) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        group_chunks(&chars, max)
            .into_iter()
            .map(|c| c.iter().collect())
            .collect()
    }

    #[test]
    fn japanese_run_splits_by_length_cap() {
        assert_eq!(split("あいうえお", 2), vec!["あい", "うえ", "お"]);
    }

    #[test]
    fn long_japanese_run_hard_breaks() {
        assert_eq!(split("あいうえお", 3), vec!["あいう", "えお"]);
    }

    #[test]
    fn punctuation_is_a_non_japanese_chunk_that_separates_clauses() {
        // Punctuation is non-Japanese, so it forms its own chunk and naturally
        // splits the clauses around it — no special punctuation rule needed.
        assert_eq!(split("あ、いう", 10), vec!["あ", "、", "いう"]);
        assert_eq!(split("あ、い。う", 10), vec!["あ", "、", "い", "。", "う"]);
    }

    #[test]
    fn consecutive_punctuation_groups_together() {
        assert_eq!(split("あ！？い", 10), vec!["あ", "！？", "い"]);
    }

    #[test]
    fn digits_form_their_own_chunk() {
        // A digit run is split off from the surrounding Japanese so it can be
        // passed through verbatim instead of being mangled by the model.
        assert_eq!(split("あ123い", 40), vec!["あ", "123", "い"]);
    }

    #[test]
    fn pure_non_japanese_is_one_chunk() {
        assert_eq!(split("123456", 40), vec!["123456"]);
        assert_eq!(split("iPhone15", 40), vec!["iPhone15"]);
    }

    #[test]
    fn non_japanese_run_is_capped_at_max() {
        assert_eq!(split("abcdef", 2), vec!["ab", "cd", "ef"]);
    }

    #[test]
    fn katakana_word_with_prolonged_mark_stays_together() {
        // `ー` (U+30FC) lives in the katakana block, so a katakana word is one
        // Japanese chunk and is never split off as a symbol.
        assert_eq!(split("スーパーマーケット", 40), vec!["スーパーマーケット"]);
    }

    #[test]
    fn japanese_and_non_japanese_runs_alternate() {
        assert_eq!(split("型1番2", 40), vec!["型", "1", "番", "2"]);
    }

    #[test]
    fn middle_dot_is_a_non_japanese_separator() {
        // 中黒 ・ (U+30FB) is special-cased as a symbol, so it splits the
        // katakana around it — while the prolonged mark ー stays Japanese.
        assert_eq!(split("ジョン・スミス", 40), vec!["ジョン", "・", "スミス"]);
        assert_eq!(
            split("スーパー・マーケット", 40),
            vec!["スーパー", "・", "マーケット"]
        );
    }
}

#[cfg(test)]
mod plan_tests {
    use super::ChunkPlan;

    /// Build a `ChunkPlan` from chunk char-lengths and the new text. The old
    /// text is reconstructed as `0..old_len` filler chars, and the new text as
    /// `new` — only the diff positions matter, so distinct chars suffice.
    fn plan(old_lens: &[usize], old_text: &str, new_text: &str, chunk_len: usize) -> ChunkPlan {
        let old: Vec<char> = old_text.chars().collect();
        let new: Vec<char> = new_text.chars().collect();
        assert_eq!(
            old.len(),
            old_lens.iter().sum::<usize>(),
            "old_lens vs old_text"
        );
        ChunkPlan::compute(old_lens, &old, &new, chunk_len)
    }

    #[test]
    fn fresh_buffer_reconverts_everything() {
        // No previous chunking → whole buffer is the changed middle.
        let p = plan(&[], "", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 0,
                trail_count: 0,
                mid_start: 0,
                mid_end: 4
            }
        );
    }

    #[test]
    fn append_after_full_chunk_reuses_all_leading() {
        // [ab][cd] + "e": both full chunks reused, only "e" is middle.
        let p = plan(&[2, 2], "abcd", "abcde", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 2,
                trail_count: 0,
                mid_start: 4,
                mid_end: 5
            }
        );
    }

    #[test]
    fn append_after_nonfull_chunk_reopens_it() {
        // [ab][c] + "d": the non-full last chunk is reopened so "cd" merges.
        let p = plan(&[2, 1], "abc", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 1,
                trail_count: 0,
                mid_start: 2,
                mid_end: 4
            }
        );
    }

    #[test]
    fn middle_insert_reuses_both_neighbors() {
        // [ab][cd][ef], insert X at pos 3 → only the middle chunk is rebuilt.
        let p = plan(&[2, 2, 2], "abcdef", "abcXdef", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 1,
                trail_count: 1,
                mid_start: 2,
                mid_end: 5
            }
        );
    }

    #[test]
    fn delete_leading_char_keeps_suffix() {
        // [ab][cd], delete 'a' → "bcd": "cd" stays as a reused suffix chunk.
        let p = plan(&[2, 2], "abcd", "bcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 0,
                trail_count: 1,
                mid_start: 0,
                mid_end: 1
            }
        );
    }

    #[test]
    fn unchanged_text_reconverts_nothing() {
        // Same text (e.g. a refresh with no edit) → empty middle, all reused.
        let p = plan(&[2, 2], "abcd", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 2,
                trail_count: 0,
                mid_start: 4,
                mid_end: 4
            }
        );
    }
}
