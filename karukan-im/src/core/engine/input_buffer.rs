//! InputBuffer: composed hiragana text with cursor.
//!
//! This struct bundles `text` and `cursor_pos`
//! which are always operated on together.

/// Composed input buffer with cursor.
pub(super) struct InputBuffer {
    /// Composed hiragana text (source of truth)
    pub text: String,
    /// Cursor position (in characters, not bytes)
    pub cursor_pos: usize,
}

impl InputBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
        }
    }

    /// Clear the buffer (text, cursor).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
    }

    /// Insert text at the current cursor position.
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.insert_str(byte_pos, text);
        let char_count = text.chars().count();
        self.cursor_pos += char_count;
    }

    /// Remove the character at the given character position.
    pub fn remove_char_at(&mut self, char_pos: usize) -> Option<char> {
        let (byte_start, removed) = self.text.char_indices().nth(char_pos)?;
        let byte_end = self
            .text
            .char_indices()
            .nth(char_pos + 1)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        self.text.replace_range(byte_start..byte_end, "");
        Some(removed)
    }

    /// Remove the character before the cursor.
    pub fn remove_char_before_cursor(&mut self) -> Option<char> {
        if self.cursor_pos == 0 {
            return None;
        }
        self.cursor_pos -= 1;
        self.remove_char_at(self.cursor_pos)
    }

    /// Remove the character at the cursor position (delete key).
    pub fn remove_char_at_cursor(&mut self) -> Option<char> {
        self.remove_char_at(self.cursor_pos)
    }
}
