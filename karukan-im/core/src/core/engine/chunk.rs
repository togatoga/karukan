//! Live-conversion chunking of the composing buffer.
//!
//! The composing buffer is split into internal [`ComposingChunk`]s so each
//! model call stays bounded for long input. Chunking asks one question per
//! character — Japanese or not (see [`is_japanese`]) — and starts a new chunk
//! whenever the current one is full or that answer changes. A Japanese run is
//! sent to the neural converter; a non-Japanese run (digits / symbols /
//! alphabet) is passed through verbatim.
//!
//! Re-chunking after an edit is *not* incremental: every keystroke re-chunks
//! the whole buffer from scratch and re-runs every chunk through
//! `run_kana_kanji_conversion`, whose conversion cache (keyed by reading +
//! lctx + strategy) turns unchanged chunks into lookups. Only chunks whose
//! reading or left context actually changed reach the model, so the cost per
//! keystroke matches the old prefix/suffix-diff scheme — without the diff
//! algorithm, and with downstream chunks correctly reconverted when a middle
//! edit changes their left context.

use tracing::debug;

use super::*;

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

impl InputMethodEngine {
    /// Auto-suggest over the composing buffer, split into chunks of at most
    /// `config.composing_chunk_len` reading characters so each model call
    /// stays bounded for long input.
    ///
    /// The chunking is a pure function of the current text: the buffer is
    /// re-chunked from scratch and every chunk re-converted on each call.
    /// `run_kana_kanji_conversion` caches results by reading + lctx +
    /// strategy, so chunks whose reading and left context are unchanged are
    /// cache hits — a keystroke at the end only infers the final chunk, and
    /// backspacing over just-typed text hits the cache for every chunk. A
    /// middle edit changes the left context of the chunks to its right, so
    /// those miss and are reconverted with the correct context.
    ///
    /// Each chunk's left context is the editor surrounding text plus the
    /// converted text of all preceding chunks, truncated to
    /// `max_api_context_len`.
    ///
    /// Returns the concatenated conversion of the whole buffer, or `None` when
    /// it equals the raw reading (no useful model suggestion).
    ///
    /// Note: for input no longer than one chunk (the common case, default
    /// N=30) this produces exactly one model call over the whole buffer, i.e.
    /// identical behavior to a whole-buffer conversion.
    pub(super) fn chunked_auto_suggest(&mut self) -> Option<String> {
        let full_reading = self.input_buf.reading();
        if full_reading.is_empty() {
            self.chunks.clear();
            return None;
        }
        self.ensure_kanji_converter();

        let chunk_len = self.chunk_len();
        let text: Vec<char> = full_reading.chars().collect();
        let base_ctx = self.truncate_context_for_api();

        let mut chunks: Vec<ComposingChunk> = Vec::new();
        let mut combined = String::new();
        for chunk in group_chunks(&text, chunk_len) {
            let reading: String = chunk.iter().collect();
            let new = self.convert_new_chunk(reading, &base_ctx, &combined);
            combined.push_str(&new.converted);
            chunks.push(new);
        }

        self.chunks = chunks;
        self.log_chunk_state("convert");

        (combined != full_reading).then_some(combined)
    }

    /// Build one converted chunk for `reading`, whose left context is
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
        let pos = self.input_buf.reading_cursor().saturating_sub(1);
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
            self.input_buf.reading_cursor(),
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
