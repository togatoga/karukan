//! Display and preedit construction for the IME engine

use super::*;

/// Deletion hint appended to the conversion aux text while a learning-cache
/// candidate is selected. Names Backspace rather than Delete because the Mac
/// "delete" key is Backspace — one wording everywhere.
pub(super) const LEARNING_DELETE_HINT: &str = "Ctrl+Backspaceで履歴から削除";

impl InputMethodEngine {
    /// Build display text from the element array.
    /// In katakana mode, kana parts are converted to katakana.
    pub(super) fn build_input_display(&self) -> String {
        let display = self.input_buf.display();
        if self.mode.current() == InputMode::Katakana {
            karukan_engine::hiragana_to_katakana(&display)
        } else {
            display
        }
    }

    /// Get the caret position in the display text (in characters)
    pub(super) fn display_caret_position(&self) -> usize {
        self.input_buf.cursor()
    }

    /// The current live-conversion text: the concatenated converted text of
    /// the chunks while the live display is shown, empty otherwise. Derived
    /// on demand — the string is never stored, so it cannot go stale against
    /// the chunks.
    pub(super) fn live_text(&self) -> String {
        if !self.live.shown {
            return String::new();
        }
        self.chunks.iter().map(|c| c.converted.as_str()).collect()
    }

    /// Build a preedit for composing state.
    /// If live conversion text is present, shows live text + pending romaji
    /// with caret at end. That layout is only faithful while typing at the
    /// end of the composition — when the cursor is elsewhere the pending is
    /// not the visual tail, so fall back to the kana display.
    /// Otherwise shows the input buffer display with cursor-based caret.
    pub(super) fn build_composing_preedit(&self) -> Preedit {
        let live = self.live_text();
        let live_at_end =
            !live.is_empty() && self.input_buf.cursor() == self.input_buf.char_count();
        let (display, caret) = if live_at_end {
            let buffer = self.input_buf.pending();
            let display = format!("{}{}", live, buffer);
            let caret = display.chars().count();
            (display, caret)
        } else {
            (self.build_input_display(), self.display_caret_position())
        };
        let mut preedit = Preedit::with_text_underlined(&display);
        preedit.set_caret(caret);
        preedit
    }

    /// The live conversion result as displayed: the live text plus the settled
    /// pending romaji tail (早稲田 + d → 早稲田d). Committing or preserving
    /// the live text alone would drop the tail, since the live suggestion only
    /// covers the settled reading. Empty when live conversion has no result
    /// or the cursor is away from the end — there the display already fell
    /// back to kana (see `build_composing_preedit`) and the pending run sits
    /// mid-buffer, so the concatenation would not match what is shown.
    /// Must be called before `settle_romaji` (which empties the pending run).
    pub(super) fn live_text_with_pending(&self) -> String {
        let live = self.live_text();
        if live.is_empty() || self.input_buf.cursor() != self.input_buf.char_count() {
            return String::new();
        }
        let pending = self.input_buf.pending();
        format!("{}{}", live, self.converters.romaji.convert_flush(&pending))
    }

    /// Format an `lctx: … rctx: …` line from explicit left/right context
    /// strings, each truncated to `display_context_len` (left keeps its tail,
    /// right keeps its head). Empty when both are absent or the limit is 0.
    fn context_line(&self, left: Option<&str>, right: Option<&str>) -> String {
        let max_len = self.config.display_context_len;
        if max_len == 0 {
            return String::new();
        }
        let lctx = left.filter(|s| !s.is_empty()).map(|left| {
            if left.chars().count() > max_len {
                format!("...{}", keep_last_chars(left, max_len))
            } else {
                left.to_string()
            }
        });

        let rctx = right.filter(|s| !s.is_empty()).map(|right| {
            if right.chars().count() > max_len {
                format!("{}...", keep_first_chars(right, max_len))
            } else {
                right.to_string()
            }
        });

        match (lctx, rctx) {
            (Some(l), Some(r)) => format!("lctx: {} rctx: {}", l, r),
            (Some(l), None) => format!("lctx: {}", l),
            (None, Some(r)) => format!("rctx: {}", r),
            (None, None) => String::new(),
        }
    }

    /// Surrounding-text context line (editor left/right). Used by conversion-mode
    /// aux text, where there is no live chunking.
    pub(super) fn display_context(&self) -> String {
        let ctx = self.surrounding_context.as_ref();
        self.context_line(
            ctx.and_then(|c| c.left.as_deref()),
            ctx.and_then(|c| c.right.as_deref()),
        )
    }

    /// Context line for live conversion (composing / auto-suggest). The single
    /// `lctx:` shown is the *current chunk's* left context — the editor
    /// surrounding text plus the converted text of the preceding chunks, derived
    /// via `chunk_lctx` — so the model context that chunk uses is what gets
    /// displayed, rather than a second redundant lctx. It already folds in the
    /// editor surrounding left context (so an empty buffer shows it as-is). The
    /// right side stays the editor surrounding right context.
    pub(super) fn display_context_chunked(&self) -> String {
        let lctx = self.chunk_lctx(self.current_chunk_index());
        let left = (!lctx.is_empty()).then_some(lctx.as_str());
        let right = self
            .surrounding_context
            .as_ref()
            .and_then(|c| c.right.as_deref());
        self.context_line(left, right)
    }

    /// Get the current mode indicator string
    pub(super) fn mode_indicator(&self) -> String {
        let base = match self.mode.current() {
            InputMode::Alphabet => "[A]",
            InputMode::Katakana => "[カ]",
            InputMode::Hiragana => "[あ]",
            // ☺ (U+263A, Unicode 1.1 / 1993) — the oldest smiley-face
            // codepoint in Unicode; gives emoji mode an unambiguous
            // glyph in the aux text that's distinct from `[A]` so the
            // user sees they're not in plain alphabet input.
            InputMode::Emoji => "[☺]",
        };
        // The effective persona is shown verbatim (e.g. ⚡[あ]P:プログラミング)
        // so what the model receives is visible at a glance.
        let live = if self.live.enabled { "⚡" } else { "" };
        let persona = self.effective_persona();
        if persona.is_empty() {
            format!("{}{}", live, base)
        } else {
            format!("{}{}P:{}", live, base, persona)
        }
    }

    /// Format aux text for composing input mode
    pub(super) fn format_aux_composing(&self) -> String {
        let ctx = self.display_context_chunked();
        let model = self.model_name();
        let indicator = self.mode_indicator();
        // Show reading + unconverted pending romaji (e.g. "わせだd")
        let romaji_buf = self.input_buf.pending();
        let base = self.input_buf.reading();
        let reading = if base.is_empty() && romaji_buf.is_empty() {
            String::new()
        } else {
            format!(" {}{}", base, romaji_buf)
        };
        if ctx.is_empty() {
            format!("{}{} Karukan ({})", indicator, reading, model)
        } else {
            format!("{}{} Karukan ({}) | {}", indicator, reading, model, ctx)
        }
    }

    /// Get token count for a reading (returns None if converter not initialized)
    pub(super) fn get_token_count(&self, reading: &str) -> Option<usize> {
        self.converters
            .kanji
            .as_ref()
            .and_then(|c| c.count_input_tokens(reading).ok())
    }

    /// Get the display name of the model used for the last conversion
    /// Falls back to the static model name if no conversion has happened yet
    fn last_used_model(&self) -> String {
        if self.metrics.model_name.is_empty() {
            self.model_name()
        } else {
            self.metrics.model_name.clone()
        }
    }

    /// Format aux text for conversion mode
    pub(super) fn format_aux_conversion_with_page(
        &self,
        reading: &str,
        candidates: Option<&CandidateList>,
    ) -> String {
        let ctx = self.display_context();
        let ctx = if ctx.is_empty() {
            String::new()
        } else {
            format!(" | {}", ctx)
        };
        let timing = format!(
            "{}ms/{}ms",
            self.metrics.conversion_ms, self.metrics.process_key_ms
        );
        let model = self.last_used_model();
        let tokens = self
            .get_token_count(reading)
            .map(|t| format!("{}tok", t))
            .unwrap_or_default();
        let page_info = candidates
            .filter(|c| c.total_pages() > 1)
            .map(|c| format!(" ({}/{})", c.current_page() + 1, c.total_pages()))
            .unwrap_or_default();
        let selected = candidates.and_then(|c| c.selected());
        let source_label = selected
            .and_then(Candidate::source_label)
            .map(|a| format!(" | {}", a))
            .unwrap_or_default();
        // Footer hint, shown only while the selected candidate is a
        // deletable user-history entry.
        let delete_hint = selected
            .filter(|c| c.is_deletable())
            .map(|_| format!(" ({})", LEARNING_DELETE_HINT))
            .unwrap_or_default();
        format!(
            "[変換]{} {}{} | {} {} | {}{}{}",
            page_info, reading, ctx, timing, tokens, model, source_label, delete_hint
        )
    }

    /// Format aux text for auto-suggest mode
    /// Note: token count is not shown here to avoid performance overhead on every keystroke
    /// Timing shows inference_ms/process_key_ms (process_key_ms is from previous keystroke)
    pub(super) fn format_aux_suggest(&self, reading: &str) -> String {
        // Single context block: the lctx is the current chunk's actual left
        // context (see `display_context_chunked`), so there is no separate
        // per-chunk lctx fragment widening the candidate window.
        let ctx = self.display_context_chunked();
        let timing = format!(
            "{}ms/{}ms",
            self.metrics.conversion_ms, self.metrics.process_key_ms
        );
        let model = self.last_used_model();
        let indicator = self.mode_indicator();
        // Append unconverted pending romaji to reading (e.g. "わせだ" + "d" → "わせだd")
        let romaji_buf = self.input_buf.pending();
        let display_reading = if romaji_buf.is_empty() {
            reading.to_string()
        } else {
            format!("{}{}", reading, romaji_buf)
        };
        if ctx.is_empty() {
            format!("{} {} | {} | {}", indicator, display_reading, timing, model)
        } else {
            format!(
                "{} {} | ctx: {} | {} | {}",
                indicator, display_reading, ctx, timing, model
            )
        }
    }

    /// Truncate context to safe size for API calls
    pub(super) fn truncate_context_for_api(&self) -> String {
        match self
            .surrounding_context
            .as_ref()
            .and_then(|ctx| ctx.left.as_deref())
        {
            Some(left) => self.truncate_context(left),
            None => String::new(),
        }
    }

    /// Truncate a context string to safe size for API calls
    pub(super) fn truncate_context(&self, context: &str) -> String {
        keep_last_chars(context, self.config.max_api_context_len)
    }
}
