//! Display and preedit construction for the IME engine

use super::*;

/// Marks the part of the reading the beam produced alternatives for.
const BEAM_SPAN_LABEL: &str = "🎯";

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

    /// The current live-conversion text: the concatenated converted text of
    /// the chunks while the live display is shown, empty otherwise. Derived
    /// on demand — the string is never stored, so it cannot go stale against
    /// the chunks.
    pub(super) fn live_text(&self) -> String {
        if !self.live.shown {
            return String::new();
        }
        // Model output, so it settles here: the prompt is NFKC-normalized
        // and the answer comes back half-width whatever was typed.
        let converted: String = self.chunks.iter().map(|c| c.converted.as_str()).collect();
        self.settle_text(&converted)
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
            (self.build_input_display(), self.input_buf.cursor())
        };
        let mut preedit = Preedit::with_text_underlined(&display);
        preedit.set_caret(caret);
        preedit
    }

    /// The live conversion result as displayed: live text plus the settled
    /// pending romaji tail (早稲田 + d → 早稲田d). Empty when there is no
    /// live result or the cursor is away from the end (the display fell
    /// back to kana there). Call before `settle_romaji` empties the tail.
    pub(super) fn live_text_with_pending(&self) -> String {
        let live = self.live_text();
        if live.is_empty() || self.input_buf.cursor() != self.input_buf.char_count() {
            return String::new();
        }
        let pending = self.input_buf.pending();
        format!("{}{}", live, self.converters.romaji.convert_flush(&pending))
    }

    /// Format an `lctx: … rctx: …` line from explicit left/right context
    /// strings, each truncated to `display_context_chars` (left keeps its tail,
    /// right keeps its head). Empty when both are absent or the limit is 0.
    fn context_line(&self, left: Option<&str>, right: Option<&str>) -> String {
        let max_len = self.config.display_context_chars;
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

    /// Context line for live conversion: the `lctx:` shown is the *current
    /// chunk's* left context (`chunk_lctx`), i.e. exactly what the model
    /// uses for that chunk; the right side stays the editor surrounding
    /// right context.
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
        if self.live.enabled {
            format!("⚡{}", base)
        } else {
            base.to_string()
        }
    }

    /// The caret's chunk reading plus the pending romaji tail ("わせだd")
    /// with a `used/max` fill counter, so cursor movement shows which chunk
    /// is being edited. A manual break armed at the end of the reading has
    /// no chunk yet and restarts the counter at `0/max`, making the cut
    /// visible.
    fn aux_reading(&self) -> String {
        // Nothing typed, nothing to count.
        if self.input_buf.is_empty() {
            return String::new();
        }
        let pending = self.input_buf.pending();
        let chunk = self.caret_chunk_reading();
        let head = format!("{}{}", chunk, pending);
        let fill = self.fill(&chunk, self.chunk_chars());
        if head.is_empty() {
            fill
        } else {
            format!("{} {}", head, fill)
        }
    }

    /// Inference and whole-keystroke time, labelled so the two are not
    /// confused: `推論:` is the model call this keystroke made (0 when it
    /// was served from the cache), `key:` is everything the keystroke did.
    fn aux_timing(&self) -> String {
        format!(
            "推論: {}ms key: {}ms",
            self.metrics.conversion_ms, self.metrics.process_key_ms
        )
    }

    /// `used/max` counter shared by the composing and conversion aux.
    fn fill(&self, reading: &str, max: usize) -> String {
        format!("{}/{}", reading.chars().count(), max)
    }

    /// The reading part of the conversion line: the composing line's own
    /// field ([`Self::aux_reading`]), so typing and converting report the
    /// same chunk, the same romaji tail and the same counter.
    ///
    /// A view that passes its own text (a predictive entry's 「query →
    /// reading」) is not a chunk, so that text stays and only the counter is
    /// taken from the chunk.
    fn conversion_reading(&self, shown: &str) -> String {
        if self.state.reading() == Some(shown) {
            return self.aux_reading();
        }
        let fill = self.fill(&self.caret_chunk_reading(), self.chunk_chars());
        format!("{shown} {fill}")
    }

    /// The span the conversion beams for alternatives, labelled and with
    /// its `used/max` counter, so which part got them is visible.
    ///
    /// Only the model-backed views (the mixed list and the AI view) beam a
    /// span; the learning and dictionary views query the whole reading,
    /// and a selected predictive candidate carries a reading of its own.
    /// Both cases keep `shown` as it is.
    fn conversion_chunk_reading(&self, shown: &str) -> Option<String> {
        if !matches!(self.state.filter(), None | Some(CandidateSource::Model)) {
            return None;
        }
        let reading = self.state.reading().filter(|r| *r == shown)?;
        let chars: Vec<char> = reading.chars().collect();
        // A break armed at the end of the reading opens an empty chunk, so
        // the counter restarts like the composing aux does and the cut is
        // visible.
        if self.chunk_breaks.contains(&chars.len()) {
            return Some(format!(
                "{BEAM_SPAN_LABEL} {}",
                self.fill("", self.config.beam_chars)
            ));
        }
        let start = self.beam_span_start(&chars);
        if start >= chars.len() {
            return None;
        }
        let span: String = chars[start..].iter().collect();
        let fill = self.fill(&span, self.config.beam_chars);
        // The span, not the whole reading: the label says what it is, and the
        // line already carries the reading each candidate commits.
        Some(format!("{BEAM_SPAN_LABEL} {span} {fill}"))
    }

    /// Format aux text for composing input mode
    pub(super) fn format_aux_composing(&self) -> String {
        let indicator = self.mode_indicator();
        let base = self.aux_reading();
        let reading = if base.is_empty() {
            String::new()
        } else {
            format!(" {}", base)
        };
        if !self.config.verbose {
            return format!("{indicator}{reading}");
        }
        let model = format!(" Karukan ({})", self.model_name());
        let ctx = Some(self.display_context_chunked())
            .filter(|c| !c.is_empty())
            .map(|c| format!(" | {c}"))
            .unwrap_or_default();
        format!("{indicator}{reading}{model}{ctx}")
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
        let page_info = candidates
            .filter(|c| c.total_pages() > 1)
            .map(|c| format!(" ({}/{})", c.current_page() + 1, c.total_pages()))
            .unwrap_or_default();
        // An empty (source-filtered) window states it outright — the
        // candidate list disappearing alone would be ambiguous.
        let empty_note = if candidates.is_some_and(|c| c.is_empty()) {
            " 候補なし"
        } else {
            ""
        };
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
        // The input mode leads the line as it does while composing — a
        // candidate window is open for keystrokes too (typing refines the
        // reading, Shift+letter switches to direct input), so which mode is
        // in force belongs here. Then the active Ctrl+R source filter, so
        // the user knows the window is narrowed (e.g. [あ][変換:📝]).
        let header = match self.state.filter().map(|s| s.emoji()) {
            Some(emoji) => format!("{}[変換:{}]", self.mode_indicator(), emoji),
            None => format!("{}[変換]", self.mode_indicator()),
        };
        let shown = self.conversion_reading(reading);
        if !self.config.verbose {
            return format!("{header}{page_info} {shown}{empty_note}{source_label}{delete_hint}");
        }
        // Verbose swaps the chunk for the beam span: the same shape counted
        // against `beam_chars`, the cap on what the alternatives cover.
        let shown = self.conversion_chunk_reading(reading).unwrap_or(shown);
        let ctx = Some(self.display_context())
            .filter(|c| !c.is_empty())
            .map(|c| format!(" | {c}"))
            .unwrap_or_default();
        let timing = self.aux_timing();
        let model = self.last_used_model();
        format!(
            "{header}{page_info} {shown}{empty_note}{ctx} | {timing} | {model}{source_label}{delete_hint}"
        )
    }

    /// The open window's line as it stands, for a key that changes something
    /// around it without rebuilding the list (the mode toggle, the verbose
    /// toggle). The selected candidate's own reading wins: a predictive
    /// candidate reads longer than the state's.
    pub(super) fn format_aux_conversion(
        &self,
        reading: &str,
        candidates: &CandidateList,
    ) -> String {
        let shown = candidates
            .selected()
            .and_then(|c| c.reading.as_deref())
            .unwrap_or(reading);
        self.format_aux_conversion_with_page(shown, Some(candidates))
    }

    /// Format aux text for auto-suggest mode
    /// Timing shows inference_ms/process_key_ms (process_key_ms is from previous keystroke)
    pub(super) fn format_aux_suggest(&self) -> String {
        // Single context block: the lctx is the current chunk's actual left
        // context (see `display_context_chunked`), so there is no separate
        // per-chunk lctx fragment widening the candidate window.
        let indicator = self.mode_indicator();
        // Current chunk's reading + pending romaji with its fill counter, so
        // the user sees which chunk they are typing into and how full it is.
        let display_reading = self.aux_reading();
        if !self.config.verbose {
            return format!("{indicator} {display_reading}");
        }
        let ctx = Some(self.display_context_chunked())
            .filter(|c| !c.is_empty())
            .map(|c| format!(" | ctx: {c}"))
            .unwrap_or_default();
        format!(
            "{indicator} {display_reading}{ctx} | {} | {}",
            self.aux_timing(),
            self.last_used_model()
        )
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
        keep_last_chars(context, self.config.context_chars)
    }
}
