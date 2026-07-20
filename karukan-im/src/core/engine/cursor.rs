//! Cursor movement and character deletion

use super::*;

impl InputMethodEngine {
    /// Common helper for cursor movement: flush romaji, clear live
    /// conversion, set new position, and reclaim any pass-through
    /// consonant that ends up immediately before the cursor.
    fn move_caret(&mut self, new_pos: usize) -> EngineResult {
        if !self.converters.romaji.buffer().is_empty() {
            self.flush_romaji_to_composed();
            self.converters.romaji.reset();
        }
        self.live.text.clear();
        self.input_buf.cursor_pos = new_pos;
        self.reclaim_passthrough_before_cursor();
        self.log_chunk_state("cursor");
        let preedit = self.set_composing_state();
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// If the character immediately before the cursor is an ASCII
    /// pass-through consonant that could start a romaji sequence, pull
    /// it out of `input_buf` and into the romaji buffer so the next
    /// keystroke can combine with it.
    ///
    /// Called after cursor movement and after deletion (Backspace /
    /// Delete) that may have exposed a previously orphaned consonant
    /// at the cursor boundary.
    ///
    /// Example: input_buf="kし", cursor at end.  Backspace removes
    /// "し", leaving "k".  Without reclaim, typing "i" yields "kい";
    /// with reclaim "k" moves to the romaji buffer and "ki" → "き".
    /// Similarly, moving the cursor to right after "k" reclaims it so
    /// typing "a" produces "か" instead of inserting a separate "あ".
    fn reclaim_passthrough_before_cursor(&mut self) {
        if matches!(
            self.mode.current(),
            InputMode::Alphabet | InputMode::Emoji
        ) {
            return;
        }
        if self.input_buf.cursor_pos == 0 {
            return;
        }
        let prev_char = self
            .input_buf
            .text
            .chars()
            .nth(self.input_buf.cursor_pos - 1);
        if let Some(ch) = prev_char {
            if ch.is_ascii_alphabetic()
                && self.converters.romaji.can_start_sequence(ch)
            {
                self.input_buf.remove_char_before_cursor();
                self.converters.romaji.reset();
                let _event = self.converters.romaji.push(ch);
            }
        }
    }

    /// Handle backspace in composing mode
    pub(super) fn backspace_composing(&mut self) -> EngineResult {
        // If romaji buffer is not empty, backspace from buffer (not from composed text)
        if !self.converters.romaji.buffer().is_empty() {
            let prev_output_len = self.converters.romaji.output().chars().count();
            self.converters.romaji.backspace();
            let curr_output_len = self.converters.romaji.output().chars().count();

            // The converter may reclaim a pass-through character from output
            // back into the buffer (e.g. "ks" → BS → "k" moves from output
            // to buffer).  Mirror that in input_buf so the two stay in sync.
            if curr_output_len < prev_output_len {
                for _ in 0..(prev_output_len - curr_output_len) {
                    self.input_buf.remove_char_before_cursor();
                }
            }

            if let Some(result) = self.try_reset_if_empty() {
                return result;
            }

            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));
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

        self.reclaim_passthrough_before_cursor();
        self.refresh_input_state()
    }

    /// Move caret left within hiragana input
    pub(super) fn move_caret_left(&mut self) -> EngineResult {
        let new_pos = self.input_buf.cursor_pos.saturating_sub(1);
        self.move_caret(new_pos)
    }

    /// Move caret right within hiragana input
    pub(super) fn move_caret_right(&mut self) -> EngineResult {
        let total = self.input_buf.text.chars().count();
        let new_pos = (self.input_buf.cursor_pos + 1).min(total);
        self.move_caret(new_pos)
    }

    /// Handle delete key in composing mode
    pub(super) fn delete_composing(&mut self) -> EngineResult {
        // If romaji buffer is not empty, flush it into input_buf first
        // so the forward-delete can reach the composed character after
        // the buffered consonant (which may have been reclaimed by a
        // cursor move).
        if !self.converters.romaji.buffer().is_empty() {
            self.flush_romaji_to_composed();
            self.converters.romaji.reset();
        }

        // Delete character at cursor position
        if self.input_buf.remove_char_at_cursor().is_none() {
            return EngineResult::consumed();
        }

        if let Some(result) = self.try_reset_if_empty() {
            return result;
        }

        self.reclaim_passthrough_before_cursor();
        self.refresh_input_state()
    }

    /// Move caret to start of input
    pub(super) fn move_caret_home(&mut self) -> EngineResult {
        self.move_caret(0)
    }

    /// Move caret to end of input
    pub(super) fn move_caret_end(&mut self) -> EngineResult {
        let total = self.input_buf.text.chars().count();
        self.move_caret(total)
    }
}
