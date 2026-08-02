//! Cursor movement and character deletion

use super::*;

impl InputMethodEngine {
    /// Common helper for cursor movement: settle pending romaji, clear live
    /// conversion, then compute the new position from the settled buffer.
    fn move_caret(&mut self, new_pos: impl FnOnce(&InputBuffer) -> usize) -> EngineResult {
        self.freeze_pending_romaji();
        self.live.text.clear();
        self.input_buf.cursor_pos = new_pos(&self.input_buf);
        self.log_chunk_state("cursor");
        let preedit = self.set_composing_state();
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Handle backspace in composing mode
    pub(super) fn backspace_composing(&mut self) -> EngineResult {
        // Active elements first: remove the last one whole. The elements
        // before it stay live, so a freed consonant still combines with the
        // next key (`ytko` → BS → `o` → 「yと」).
        let reading_before = self.input_buf.reading();
        if self.input_buf.backspace_element(&self.converters.romaji) {
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
            return self.refresh_input_state();
        }

        // Remove character before cursor from composed_hiragana
        if self.input_buf.cursor_pos > 0 {
            self.input_buf.remove_char_before_cursor();
        } else {
            // Nothing to delete
            return EngineResult::consumed();
        }

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.refresh_input_state()
    }

    /// Move caret left within hiragana input
    pub(super) fn move_caret_left(&mut self) -> EngineResult {
        self.move_caret(|buf| buf.cursor_pos.saturating_sub(1))
    }

    /// Move caret right within hiragana input
    pub(super) fn move_caret_right(&mut self) -> EngineResult {
        self.move_caret(|buf| (buf.cursor_pos + 1).min(buf.text.chars().count()))
    }

    /// Handle delete key in hiragana mode
    pub(super) fn delete_composing(&mut self) -> EngineResult {
        // Active elements sit at the cursor; don't delete from composed text
        if self.input_buf.has_elements() {
            return EngineResult::consumed();
        }

        // Delete character at cursor position
        if self.input_buf.remove_char_at_cursor().is_none() {
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
        self.move_caret(|buf| buf.text.chars().count())
    }
}
