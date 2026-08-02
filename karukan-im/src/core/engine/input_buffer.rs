//! InputBuffer: composed hiragana text with cursor and a pending romaji segment.
//!
//! `text` holds settled output: converted hiragana, passed-through symbols,
//! and directly inserted characters (alphabet mode, full-width space).
//! `pending` holds the raw keystrokes that may still combine with future
//! keys — the unresolved romaji tail plus passed-through consonants like
//! `yk`. It renders verbatim at the cursor and is excluded from the
//! conversion reading.
//!
//! Typing appends to `pending`; the derivation settles into `text` as soon
//! as any derived character can no longer start a rule (a fired rule's kana,
//! a digit, a symbol). Backspace pops one raw key while `pending` is
//! non-empty, so `ykt` → BS → `o` re-derives `yko` → 「yこ」; settled text
//! is only ever truncated per character, so っ/ん/きょ never revert to
//! romaji.

use karukan_engine::RomajiConverter;

/// Composed input buffer with cursor.
pub(super) struct InputBuffer {
    /// Settled text (source of truth for the conversion reading)
    pub text: String,
    /// Cursor position in `text` (in characters, not bytes)
    pub cursor_pos: usize,
    /// Raw keystrokes still eligible for romaji combination; renders at cursor
    pending: String,
}

impl InputBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            pending: String::new(),
        }
    }

    /// Clear the buffer (text, cursor, pending segment).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
        self.pending.clear();
    }

    /// Raw pending segment (shown verbatim at the cursor).
    pub fn pending(&self) -> &str {
        &self.pending
    }

    pub fn pending_char_count(&self) -> usize {
        self.pending.chars().count()
    }

    /// Push a kana-mode keystroke through the romaji derivation.
    ///
    /// The derived output settles into `text` once it contains any character
    /// that cannot start a new rule; until then the raw keystrokes stay in
    /// `pending` so backspace can unwind them.
    pub fn push_kana(&mut self, ch: char, romaji: &RomajiConverter) {
        self.pending.push(ch.to_ascii_lowercase());
        let derived = romaji.convert(&self.pending);
        if derived.text.chars().any(|c| !romaji.starts_rule(c)) {
            self.insert(&derived.text);
            self.pending = derived.pending;
        }
    }

    /// Remove the last pending keystroke. Returns false when `pending` is empty.
    pub fn backspace_pending(&mut self) -> bool {
        self.pending.pop().is_some()
    }

    /// Settle the pending segment into `text` (`ltu` → っ; unmatched
    /// consonants pass through literally).
    pub fn freeze_pending(&mut self, romaji: &RomajiConverter) {
        if self.pending.is_empty() {
            return;
        }
        let flushed = romaji.convert_flush(&self.pending);
        self.pending.clear();
        self.insert(&flushed);
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
