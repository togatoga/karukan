//! Cursor movement and character deletion

use super::*;

impl InputMethodEngine {
    /// Common helper for cursor movement: clear live conversion and set the
    /// new display position. Nothing settles — unevaluated romaji stays
    /// live, so typing after a move can still combine with it. Moving does
    /// end a temporary alphabet word (the user left the word they were
    /// typing), so the next key is evaluated as romaji again.
    fn move_caret(&mut self, new_pos: impl FnOnce(&InputBuffer) -> usize) -> EngineResult {
        if self.mode.current() == InputMode::Alphabet {
            self.mode.exit_temporary();
        }
        self.live.shown = false;
        self.input_buf.set_cursor(new_pos(&self.input_buf));
        self.log_chunk_state("cursor");
        let preedit = self.set_composing_state();
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Handle backspace in composing mode
    pub(super) fn backspace_composing(&mut self) -> EngineResult {
        // Remove one display character before the cursor: a single-character
        // element vanishes whole, re-exposing the live element before it
        // (`ytko` → BS → `o` → 「yと」); きょ is truncated per character.
        let reading_before = self.input_buf.reading();
        if !self.edit_with_chunk_breaks(|e| e.input_buf.backspace(&e.converters.romaji)) {
            // Nothing to delete
            return EngineResult::consumed();
        }

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        // Reading unchanged (a live keystroke was popped): keep the
        // candidate window as-is
        if self.input_buf.reading() == reading_before {
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
        }
        self.refresh_input_state()
    }

    /// Move caret left within the composition
    pub(super) fn move_caret_left(&mut self) -> EngineResult {
        self.move_caret(|buf| buf.cursor().saturating_sub(1))
    }

    /// Move caret right within the composition
    pub(super) fn move_caret_right(&mut self) -> EngineResult {
        self.move_caret(|buf| buf.cursor() + 1)
    }

    /// Handle delete key in composing mode
    pub(super) fn delete_composing(&mut self) -> EngineResult {
        if !self.edit_with_chunk_breaks(|e| e.input_buf.delete_at_cursor(&e.converters.romaji)) {
            return EngineResult::consumed();
        }

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.refresh_input_state()
    }

    /// Move caret to start of input
    pub(super) fn move_caret_home(&mut self) -> EngineResult {
        self.move_caret(|_| 0)
    }

    /// Move caret to end of input
    pub(super) fn move_caret_end(&mut self) -> EngineResult {
        self.move_caret(|buf| buf.char_count())
    }
}
