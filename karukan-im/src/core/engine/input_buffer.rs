//! InputBuffer: composed hiragana text with cursor and per-input elements.
//!
//! `text` holds settled output (frozen on cursor move, conversion, commit).
//! `elements` is the active composition at the cursor, one element per
//! input unit. Typing `wasedaytk` yields
//! `[Converted(わ), Converted(せ), Converted(だ), Romaji(y), Romaji(t), Romaji(k)]`.
//!
//! - [`Element::Romaji`]: raw keystrokes still forming a rule prefix (`y`,
//!   `ky`, a lone `n`). Shown verbatim; the next key may extend or convert
//!   them.
//! - [`Element::Converted`]: a fired rule's output. Never re-evaluated,
//!   so it never reverts to romaji.
//! - [`Element::Direct`]: direct input (alphabet/emoji mode). Never
//!   romaji-evaluated.
//!
//! Backspace removes the last element whole when it displays one character
//! (こ vanishes with both its keystrokes, re-exposing the still-live element
//! before it: `ytko` → BS → `o` gives 「yと」, again 「よ」); a longer
//! display like きょ is truncated per character.

use karukan_engine::RomajiConverter;

/// One composition unit.
enum Element {
    /// Raw romaji keystrokes, still a rule prefix — may extend or convert
    Romaji(String),
    /// Settled kana-mode output: fired rule (`ko` → こ) or passthrough (`1`)
    Converted(String),
    /// Direct input — excluded from romaji evaluation
    Direct(String),
}

impl Element {
    fn display(&self) -> &str {
        match self {
            Element::Romaji(raw) => raw,
            Element::Converted(display) => display,
            Element::Direct(text) => text,
        }
    }

    /// Whether the next kana key may still extend or convert this element.
    fn is_open(&self) -> bool {
        matches!(self, Element::Romaji(_))
    }
}

/// Composed input buffer with cursor.
pub(super) struct InputBuffer {
    /// Settled text
    pub text: String,
    /// Cursor position in `text` (in characters, not bytes); the active
    /// elements render here
    pub cursor_pos: usize,
    /// Active composition elements
    elements: Vec<Element>,
}

impl InputBuffer {
    /// Create a new empty buffer.
    pub fn new() -> Self {
        Self {
            text: String::new(),
            cursor_pos: 0,
            elements: Vec::new(),
        }
    }

    /// Clear the buffer (text, cursor, elements).
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor_pos = 0;
        self.elements.clear();
    }

    pub fn has_elements(&self) -> bool {
        !self.elements.is_empty()
    }

    /// Concatenated display of the active elements (rendered at the cursor).
    pub fn composing_display(&self) -> String {
        self.elements.iter().map(Element::display).collect()
    }

    pub fn composing_char_count(&self) -> usize {
        self.elements
            .iter()
            .map(|e| e.display().chars().count())
            .sum()
    }

    /// Raw keystrokes of the trailing Romaji elements — the unconverted romaji
    /// tail shown in aux text (`d` in わせだd, or a consonant run `ytk`).
    pub fn pending(&self) -> String {
        self.elements[self.settled_count()..]
            .iter()
            .map(Element::display)
            .collect()
    }

    /// Conversion reading: settled text plus element displays, excluding the
    /// trailing Romaji elements (still being typed).
    pub fn reading(&self) -> String {
        let element_part: String = self.elements[..self.settled_count()]
            .iter()
            .map(Element::display)
            .collect();
        let byte_pos = self
            .text
            .char_indices()
            .nth(self.cursor_pos)
            .map(|(i, _)| i)
            .unwrap_or(self.text.len());
        let mut reading = String::with_capacity(self.text.len() + element_part.len());
        reading.push_str(&self.text[..byte_pos]);
        reading.push_str(&element_part);
        reading.push_str(&self.text[byte_pos..]);
        reading
    }

    /// Cursor position in reading coordinates: `cursor_pos` plus the settled
    /// element characters that precede the caret in [`Self::reading`].
    pub fn reading_cursor(&self) -> usize {
        let settled_chars: usize = self.elements[..self.settled_count()]
            .iter()
            .map(|e| e.display().chars().count())
            .sum();
        self.cursor_pos + settled_chars
    }

    /// Number of leading elements before the trailing Romaji run.
    fn settled_count(&self) -> usize {
        self.elements
            .iter()
            .rposition(|e| !e.is_open())
            .map(|i| i + 1)
            .unwrap_or(0)
    }

    /// Push a kana-mode keystroke.
    ///
    /// Tries to extend or convert the last Romaji element; when the key cannot
    /// combine, that element stays live and the key starts its own element.
    pub fn push_romaji(&mut self, ch: char, romaji: &RomajiConverter) {
        let ch = ch.to_ascii_lowercase();

        if let Some(Element::Romaji(raw)) = self.elements.last_mut() {
            let mut candidate = raw.clone();
            candidate.push(ch);
            let derived = romaji.convert(&candidate);

            if derived.text.is_empty() {
                // Still a rule prefix (`k` + `y` → `ky`): keep buffering
                *raw = candidate;
                return;
            }

            if !derived.text.starts_with(raw.as_str()) {
                // A rule fired and consumed the element (`ko` → こ, `nk` →
                // ん+k). Any leftover pending starts a fresh Romaji element.
                *self.elements.last_mut().expect("last element exists") =
                    Element::Converted(derived.text);
                if !derived.pending.is_empty() {
                    self.elements.push(Element::Romaji(derived.pending));
                }
                return;
            }
            // No combine — the old raw passed through unchanged (`k` + `t`).
            // Leave it live and give the key its own element below.
        }

        self.push_fresh(ch, romaji);
    }

    /// Start a new element from a single kana-mode keystroke.
    fn push_fresh(&mut self, ch: char, romaji: &RomajiConverter) {
        let raw = ch.to_string();
        let derived = romaji.convert(&raw);
        if derived.text.is_empty() {
            // Rule prefix (`k`, `y`, `n`): live element shown verbatim
            self.elements.push(Element::Romaji(raw));
        } else {
            // Fired (`a` → あ, `.` → 。) or passed through (`1`): settled
            self.elements.push(Element::Converted(derived.text));
        }
    }

    /// Push a direct-input character (alphabet/emoji mode).
    pub fn push_direct(&mut self, ch: char) {
        self.elements.push(Element::Direct(ch.to_string()));
    }

    /// Backspace over the active elements. A single-character display is
    /// removed whole (こ vanishes with both its keystrokes); a multi-character
    /// display like きょ is truncated by one character and settles as
    /// literal text. Returns false when there are no elements.
    pub fn backspace_element(&mut self, romaji: &RomajiConverter) -> bool {
        let Some(last) = self.elements.last_mut() else {
            return false;
        };
        let mut display = last.display().to_string();
        if display.chars().count() > 1 {
            display.pop();
            *last = match last {
                Element::Direct(_) => Element::Direct(display),
                _ if romaji.is_rule_prefix(&display) => Element::Romaji(display),
                _ => Element::Converted(display),
            };
        } else {
            self.elements.pop();
        }
        true
    }

    /// Settle the active elements into `text` at the cursor. Romaji elements
    /// are force-converted (`ltu` → っ; unmatched consonants pass through).
    pub fn freeze_pending(&mut self, romaji: &RomajiConverter) {
        if self.elements.is_empty() {
            return;
        }
        let mut settled = String::new();
        for element in &self.elements {
            match element {
                Element::Romaji(raw) => settled.push_str(&romaji.flush_pending(raw)),
                _ => settled.push_str(element.display()),
            }
        }
        self.elements.clear();
        self.insert(&settled);
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
