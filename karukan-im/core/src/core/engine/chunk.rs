//! Live-conversion chunking of the composing buffer.
//!
//! Splitting bounds each model call and freezes settled text: once a
//! boundary is behind the caret that chunk's reading and lctx stop
//! changing, so it stays a cache hit and its display no longer flickers.
//! Boundary rules live in [`group_chunks`]; the user-facing rationale is
//! `docs/chunking.md`.
//!
//! Every keystroke re-chunks the whole buffer from scratch. The conversion
//! cache turns unchanged chunks into lookups, so only chunks whose reading
//! or left context actually changed reach the model.
use tracing::debug;

use karukan_engine::kana::is_digit;

use super::*;

/// Whether `c` is "Japanese": hiragana, katakana (incl. `ー`), or kanji.
/// Everything else — digits, letters, symbols, all punctuation — is not, and
/// only a chunk containing Japanese reaches the model. The 中黒 `・` sits in
/// the katakana block but is special-cased as a separator symbol, so it
/// counts against the absorption budget like any other mark.
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

/// Split `chars` into chunks. A boundary opens at every position in
/// `breaks` (manual boundaries, sorted), wherever a chunk reaches `max`
/// chars, and around non-Japanese chars, except that a chunk containing
/// Japanese keeps marks up to `max_symbols` and digits up to `max_digits`.
/// Digits count per run, so a run is kept whole or split off whole and
/// never tears. Letters never join a Japanese chunk: latin text is
/// passthrough, and an unresolved romaji tail must not reach the model as
/// part of the reading. A chunk with no Japanese has nothing to convert and
/// is exempt from both caps.
fn group_chunks<'a>(
    chars: &'a [char],
    max: usize,
    max_symbols: usize,
    max_digits: usize,
    breaks: &[usize],
) -> Vec<&'a [char]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut has_japanese = false;
    let mut symbols = 0;
    let mut digits = 0;
    for (i, &c) in chars.iter().enumerate() {
        let japanese = is_japanese(c);
        // Whether the current chunk can keep `c`. A digit is judged by the
        // whole run it belongs to (decided at the run's first char; a kept
        // run's later digits re-check with a shorter tail and stay).
        let keeps = if japanese {
            true
        } else if is_digit(c) {
            let run = chars[i..].iter().take_while(|&&r| is_digit(r)).count();
            digits + run <= max_digits
        } else if c.is_alphabetic() {
            false
        } else {
            symbols < max_symbols
        };
        let cut = i > start
            && (breaks.contains(&i)
                || i - start >= max
                || (japanese && !has_japanese)
                || (!japanese && has_japanese && !keeps));
        if cut {
            out.push(&chars[start..i]);
            start = i;
            has_japanese = false;
            symbols = 0;
            digits = 0;
        }
        has_japanese |= japanese;
        if !japanese {
            if is_digit(c) {
                digits += 1;
            } else {
                symbols += 1;
            }
        }
    }
    if start < chars.len() {
        out.push(&chars[start..]);
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
    /// preceding chunks as its left context. Live conversion and the
    /// explicit conversion's grid replay share it, so the two can never
    /// disagree about where the boundaries are.
    fn convert_chunks(&mut self, chars: &[char], base_ctx: &str) -> Vec<ComposingChunk> {
        let groups = self.split_chunks(chars);
        let mut chunks: Vec<ComposingChunk> = Vec::new();
        let mut combined = String::new();
        for chunk in groups {
            let new = self.convert_new_chunk(chunk.iter().collect(), base_ctx, &combined);
            combined.push_str(&new.converted);
            chunks.push(new);
        }
        chunks
    }

    /// Start of the trailing run of Japanese chunks that fits `budget`
    /// chars, always at least the last chunk. Snapped to chunk boundaries,
    /// so a digit or symbol run is never swallowed into the span, and a
    /// manual break is a wall: the user froze everything left of it.
    pub(super) fn trailing_chunks_start(&self, chars: &[char], budget: usize) -> usize {
        let chunks = self.split_chunks(chars);
        let mut start = chars.len();
        let mut taken = 0;
        for chunk in chunks.iter().rev() {
            // A chunk with no Japanese has nothing to convert and must never
            // reach the model, so it walls the span off — including when it
            // is the last chunk, which then leaves the span empty.
            if !chunk.iter().any(|&c| is_japanese(c)) {
                break;
            }
            if taken > 0 && taken + chunk.len() > budget {
                break;
            }
            taken += chunk.len();
            start -= chunk.len();
            if self.chunk_breaks.contains(&start) {
                break;
            }
        }
        start
    }

    /// Split `chars` with the engine's configured rules. The single place
    /// the settings and the manual breaks meet [`group_chunks`], so a new
    /// rule reaches every caller at once.
    fn split_chunks<'a>(&self, chars: &'a [char]) -> Vec<&'a [char]> {
        group_chunks(
            chars,
            self.chunk_chars(),
            self.config.chunk_symbols,
            self.config.chunk_digits,
            &self.chunk_breaks,
        )
    }

    /// Top-1 conversion of `chars` on the same chunk grid live conversion
    /// uses: chunks containing Japanese go through the model (cache hits
    /// while the user types), purely non-Japanese chunks pass through
    /// verbatim, and each chunk's lctx is `base_ctx` plus the converted text
    /// before it.
    pub(super) fn convert_on_chunk_grid(&mut self, chars: &[char], base_ctx: &str) -> String {
        self.convert_chunks(chars, base_ctx)
            .into_iter()
            .map(|c| c.converted)
            .collect()
    }

    /// Build one converted chunk: a reading containing any Japanese goes to
    /// the model with `base_ctx` + `combined` as its left context (absorbed
    /// marks included); a purely non-Japanese one passes through verbatim,
    /// so digit and symbol runs stay exact.
    fn convert_new_chunk(
        &mut self,
        reading: String,
        base_ctx: &str,
        combined: &str,
    ) -> ComposingChunk {
        let converted = if reading.chars().any(is_japanese) {
            let lctx = self.lctx_for(base_ctx, combined);
            self.convert_chunk(&reading, &lctx)
        } else {
            reading.clone()
        };
        ComposingChunk { reading, converted }
    }

    /// Configured maximum chunk length in chars, clamped to at least 1.
    pub(super) fn chunk_chars(&self) -> usize {
        self.config.chunk_chars.max(1)
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

    /// Insert a manual chunk boundary at the caret, then reconvert. A
    /// boundary at the end of the reading takes effect as more text is
    /// typed: the settled text stops reconverting and the next keystroke
    /// starts a fresh chunk.
    pub(super) fn insert_chunk_break(&mut self) -> EngineResult {
        let pos = self.input_buf.reading_cursor();
        if pos > 0 && !self.chunk_breaks.contains(&pos) {
            self.chunk_breaks.push(pos);
            self.chunk_breaks.sort_unstable();
        }
        self.refresh_input_state()
    }

    /// Run a buffer edit, shifting the manual chunk boundaries with the text
    /// around the caret. Boundaries at or left of the edit stay put (text
    /// typed right after a break belongs to the new chunk); boundaries to
    /// the right move by the reading-length delta. Boundaries that fall off
    /// the reading are dropped.
    pub(super) fn edit_with_chunk_breaks<R>(&mut self, edit: impl FnOnce(&mut Self) -> R) -> R {
        if self.chunk_breaks.is_empty() {
            return edit(self);
        }
        let old_len = self.input_buf.reading().chars().count();
        let old_pos = self.input_buf.reading_cursor();
        let out = edit(self);
        let new_len = self.input_buf.reading().chars().count();
        let pos = old_pos.min(self.input_buf.reading_cursor());
        for b in &mut self.chunk_breaks {
            if *b <= pos {
                continue;
            }
            *b = if new_len >= old_len {
                *b + (new_len - old_len)
            } else {
                // A deletion can pull a boundary past the edit point; it
                // stops there rather than moving left of it.
                b.saturating_sub(old_len - new_len).max(pos)
            };
        }
        self.chunk_breaks.retain(|&b| b > 0 && b <= new_len);
        self.chunk_breaks.dedup();
        out
    }

    /// Index of the chunk the cursor sits in, by walking the actual chunk
    /// lengths (chunks are variable-length, so `cursor / chunk_chars` is
    /// wrong). Right after a manual break at the end of the reading this is
    /// `chunks.len()` — one past the end — because the new chunk is still
    /// empty. 0 for an empty buffer or a cursor at the very start.
    pub(super) fn current_chunk_index(&self) -> usize {
        let cursor = self.input_buf.reading_cursor();
        let at_break = self.chunk_breaks.contains(&cursor);
        let pos = if at_break {
            cursor
        } else {
            cursor.saturating_sub(1)
        };
        let mut end = 0;
        for (i, chunk) in self.chunks.iter().enumerate() {
            end += chunk.reading.chars().count();
            if pos < end {
                return i;
            }
        }
        if at_break {
            self.chunks.len()
        } else {
            self.chunks.len().saturating_sub(1)
        }
    }

    /// Reading of the chunk the caret currently types into — `Some("")`
    /// while [`Self::current_chunk_index`] points one past the end (a break
    /// armed at the end of the reading, whose chunk does not exist yet),
    /// `None` when the buffer has no chunks at all. Keeps the sentinel
    /// interpretation next to where it is produced.
    pub(super) fn current_chunk_reading(&self) -> Option<&str> {
        match self.chunks.get(self.current_chunk_index()) {
            Some(chunk) => Some(&chunk.reading),
            None if !self.chunks.is_empty() => Some(""),
            None => None,
        }
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

    /// The default per-chunk symbol cap (mirrors `EngineConfig::default` /
    /// default.toml).
    const SYMBOLS: usize = 1;
    /// Digits stay out of the converter (default.toml `chunk_digits = 0`).
    const DIGITS: usize = 0;

    /// Split with the default caps and no manual breaks.
    fn split(s: &str, max: usize) -> Vec<String> {
        split_full(s, max, SYMBOLS, DIGITS, &[])
    }

    fn split_full(
        s: &str,
        max: usize,
        max_symbols: usize,
        max_digits: usize,
        breaks: &[usize],
    ) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        group_chunks(&chars, max, max_symbols, max_digits, breaks)
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
    fn a_run_of_marks_after_japanese_forms_one_chunk() {
        // The first mark rides along; the rest have no Japanese in front of
        // them, so they grow into a single verbatim chunk instead of
        // splitting one by one.
        assert_eq!(split("ア、、、、、", 40), vec!["ア、", "、、、、"]);
    }

    #[test]
    fn a_mark_rides_along_with_the_japanese_around_it() {
        // The mark stays inline while the chunk has budget left, so 「おい、」
        // keeps converting as one unit instead of freezing 「おい」 (as 老)
        // the moment the mark is typed.
        assert_eq!(split("おい、", 10), vec!["おい、"]);
        assert_eq!(split("あ、いう", 10), vec!["あ、いう"]);
        assert_eq!(split("いいね！すごい", 10), vec!["いいね！すごい"]);
        assert_eq!(split("きごう〜", 10), vec!["きごう〜"]);
    }

    #[test]
    fn mark_past_the_cap_forces_a_new_chunk() {
        // One mark per chunk by default: the second opens a new chunk even
        // directly after Japanese, which is roughly one clause each.
        assert_eq!(split("あ、い。う", 10), vec!["あ、い", "。", "う"]);
        assert_eq!(
            split("おい、おまえだよ。まて、こら", 20),
            vec!["おい、おまえだよ", "。", "まて、こら"]
        );
        // The kept mark stays put, so the left chunk is not reshaped.
        assert_eq!(split("すごい！？", 10), vec!["すごい！", "？"]);
        assert_eq!(split("は、じ。", 10), vec!["は、じ", "。"]);
    }

    #[test]
    fn digits_ride_along_when_allowed() {
        // Raising the digit budget lets short runs go through the converter
        // with the text around them.
        assert_eq!(split_full("あ12い", 10, SYMBOLS, 4, &[]), vec!["あ12い"]);
        assert_eq!(
            split_full("だい3かい", 10, SYMBOLS, 4, &[]),
            vec!["だい3かい"]
        );
        // A run is kept whole or split off whole, never torn.
        assert_eq!(
            split_full("あ1234い", 40, SYMBOLS, 2, &[]),
            vec!["あ", "1234", "い"]
        );
    }

    #[test]
    fn letters_never_ride_along() {
        // Latin text is passthrough, and an unresolved romaji tail must not
        // reach the converter as part of the reading.
        assert_eq!(split("あいk", 40), vec!["あい", "k"]);
    }

    #[test]
    fn caps_are_configurable() {
        // Two marks per chunk.
        assert_eq!(
            split_full("あ、い。う", 10, 2, DIGITS, &[]),
            vec!["あ、い。う"]
        );
        // No marks at all: split at every one.
        assert_eq!(split_full("おい、", 10, 0, DIGITS, &[]), vec!["おい", "、"]);
    }

    #[test]
    fn chunk_with_no_japanese_is_exempt_from_the_cap() {
        // A chunk never *starts* with absorbed symbols: with no Japanese in
        // front of them, digits/symbols form a verbatim chunk of their own,
        // growing to the length cap regardless of how many symbols it holds.
        assert_eq!(split("123あ", 40), vec!["123", "あ"]);
        assert_eq!(split("1233413！！〜〜", 40), vec!["1233413！！〜〜"]);
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
    fn middle_dot_counts_toward_the_symbol_cap() {
        // 中黒 ・ (U+30FB) sits in the katakana block but is special-cased as
        // a symbol: it is absorbed like any other mark and counts against the
        // cap.
        assert_eq!(split("ジョン・スミス", 40), vec!["ジョン・スミス"]);
        assert_eq!(split("あ・い・う・え", 40), vec!["あ・い", "・", "う・え"]);
    }

    #[test]
    fn manual_breaks_force_boundaries() {
        assert_eq!(
            split_full("あいうえ", 40, SYMBOLS, DIGITS, &[2]),
            vec!["あい", "うえ"]
        );
        assert_eq!(
            split_full("あいうえ", 40, SYMBOLS, DIGITS, &[1, 3]),
            vec!["あ", "いう", "え"]
        );
        // A break at 0 or at the very end changes nothing.
        assert_eq!(split_full("あい", 40, SYMBOLS, DIGITS, &[0]), vec!["あい"]);
        assert_eq!(split_full("あい", 40, SYMBOLS, DIGITS, &[2]), vec!["あい"]);
    }

    #[test]
    fn manual_break_splits_a_non_japanese_run() {
        assert_eq!(
            split_full("1234", 40, SYMBOLS, DIGITS, &[2]),
            vec!["12", "34"]
        );
    }

    #[test]
    fn absorbed_symbols_count_against_the_length_cap() {
        assert_eq!(split("あ、いうえ", 3), vec!["あ、い", "うえ"]);
    }
}
