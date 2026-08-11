//! Live-conversion chunking of the composing buffer.
//!
//! The buffer is split into [`ComposingChunk`]s — a new chunk on every
//! Japanese ⇄ non-Japanese switch ([`is_japanese`]) or length cap — so each
//! model call stays bounded. Japanese runs go to the model, non-Japanese
//! runs pass through verbatim. Every keystroke re-chunks from scratch; the
//! conversion cache makes unchanged chunks free.

use tracing::debug;

use super::*;

/// Whether `c` is "Japanese": hiragana, katakana (incl. `ー`), or kanji.
/// Everything else — digits, letters, symbols, all punctuation — is not,
/// which keeps digits out of the model and lets punctuation separate
/// clauses with no extra rule. The 中黒 `・` sits in the katakana block but
/// is special-cased as a separator (`ジョン・スミス` splits around it).
pub(super) fn is_japanese(c: char) -> bool {
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
    /// Auto-suggest over the composing buffer via [`Self::convert_chunks`],
    /// storing the chunks for display. Returns the concatenated conversion,
    /// or `None` when it equals the raw reading (no useful suggestion).
    /// Input no longer than one chunk — the common case — is a single
    /// whole-buffer model call.
    pub(super) fn chunked_auto_suggest(&mut self) -> Option<String> {
        let full_reading = self.input_buf.reading();
        if full_reading.is_empty() {
            self.chunks.clear();
            return None;
        }
        self.ensure_kanji_converter();

        let text: Vec<char> = full_reading.chars().collect();
        let base_ctx = self.truncate_context_for_api();

        let chunks = self.convert_chunks(&text, &base_ctx);
        let combined: String = chunks.iter().map(|c| c.converted.as_str()).collect();

        self.chunks = chunks;
        self.log_chunk_state("convert");

        (combined != full_reading).then_some(combined)
    }

    /// The single implementation of the chunk-grid conversion: split with
    /// [`group_chunks`], each chunk built with the converted text of the
    /// preceding chunks as its left context.
    fn convert_chunks(&mut self, chars: &[char], base_ctx: &str) -> Vec<ComposingChunk> {
        let mut chunks: Vec<ComposingChunk> = Vec::new();
        let mut combined = String::new();
        for chunk in group_chunks(chars, self.chunk_len()) {
            let new = self.convert_new_chunk(chunk.iter().collect(), base_ctx, &combined);
            combined.push_str(&new.converted);
            chunks.push(new);
        }
        chunks
    }

    /// Top-1 conversion of `chars` on the live-conversion chunk grid:
    /// Japanese chunks go through the model (conversion-cache hits while
    /// the user types), non-Japanese chunks pass through verbatim, and
    /// each chunk's lctx is `base_ctx` plus the converted text before it.
    pub(super) fn convert_on_chunk_grid(&mut self, chars: &[char], base_ctx: &str) -> String {
        self.convert_chunks(chars, base_ctx)
            .into_iter()
            .map(|c| c.converted)
            .collect()
    }

    /// Build one converted chunk: a Japanese reading goes to the model with
    /// `base_ctx` + `combined` as left context, a non-Japanese one passes
    /// through verbatim. The reading is group-homogeneous, so its first
    /// char decides.
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
    pub(super) fn chunk_len(&self) -> usize {
        self.config.composing_chunk_len.max(1)
    }

    /// The left context (lctx) a chunk is built with: `base` (editor
    /// surrounding text) + preceding converted text, truncated to the API
    /// budget. Defined once so conversion and the aux display can't drift.
    pub(super) fn lctx_for(&self, base: &str, preceding_converted: &str) -> String {
        self.truncate_context(&format!("{base}{preceding_converted}"))
    }

    /// Left context for the chunk at `index`, derived on demand from the
    /// chunks to its left.
    pub(super) fn chunk_lctx(&self, index: usize) -> String {
        let base = self.truncate_context_for_api();
        let preceding: String = self.chunks[..index.min(self.chunks.len())]
            .iter()
            .map(|c| c.converted.as_str())
            .collect();
        self.lctx_for(&base, &preceding)
    }

    /// Best-effort lazy init of the kanji converter. Chunking proceeds even
    /// on failure so `self.chunks` always mirrors the buffer; each chunk
    /// then falls back to its own reading.
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

    /// Index of the chunk the cursor sits in, by walking the actual chunk
    /// lengths (chunks are variable-length, so `cursor / chunk_len` is
    /// wrong). 0 for an empty buffer or a cursor at the very start.
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

    /// Debug-log the current chunking; `at` labels the call site
    /// (`"convert"`, `"cursor"`, …).
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
