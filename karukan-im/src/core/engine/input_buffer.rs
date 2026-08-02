//! InputBuffer: the composition as a per-input element array plus a cursor.
//!
//! The array is the single source of truth. Typing `wasedaytk` yields
//! `[Converted(わ), Converted(せ), Converted(だ), Romaji(y), Romaji(t), Romaji(k)]`.
//!
//! - [`Element::Romaji`]: one keystroke not yet consumed by a rule (`y`,
//!   `k`, a lone `n`). Shown verbatim; evaluation may later consume it.
//! - [`Element::Converted`]: a fired rule's output (or a settled
//!   passthrough like `1`). Opaque to evaluation — it never reverts.
//! - [`Element::Direct`]: one directly-input keystroke (alphabet/emoji
//!   mode). Opaque to evaluation.
//!
//! Every edit is an array splice at the cursor (a display-character
//! position). After a romaji keystroke is inserted, the run of Romaji
//! elements ending at the cursor is re-evaluated through the converter:
//! keystrokes a rule consumed become one `Converted`, the rest stay
//! `Romaji`. Elements right of the cursor are never touched, so nothing
//! combines across the cursor.
//!
//! Backspace removes one display character: a single-character element is
//! removed whole (こ vanishes with its keystrokes, re-exposing the live
//! element before it — `ytko` → BS → `o` gives 「yと」, again 「よ」), and
//! a longer `Converted` like きょ is truncated per character. The cursor
//! moves freely without settling anything, so `[Romaji(k), Romaji(y),
//! Direct(K)]` plus `o` typed before the `K` evaluates to 「きょK」.

use karukan_engine::RomajiConverter;

/// One input unit.
#[derive(Clone)]
enum Element {
    /// A keystroke not yet consumed by a conversion rule
    Romaji(char),
    /// Settled output: fired rule (`ko` → こ) or passthrough (`1`)
    Converted(String),
    /// A directly-input keystroke — excluded from romaji evaluation
    Direct(char),
}

impl Element {
    fn display(&self) -> String {
        match self {
            Element::Romaji(ch) | Element::Direct(ch) => ch.to_string(),
            Element::Converted(display) => display.clone(),
        }
    }

    fn char_count(&self) -> usize {
        match self {
            Element::Romaji(_) | Element::Direct(_) => 1,
            Element::Converted(display) => display.chars().count(),
        }
    }

    fn is_romaji(&self) -> bool {
        matches!(self, Element::Romaji(_))
    }
}

/// The composition: element array plus a cursor in display characters.
pub(super) struct InputBuffer {
    elements: Vec<Element>,
    cursor: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.cursor = 0;
    }

    pub fn has_elements(&self) -> bool {
        !self.elements.is_empty()
    }

    /// Cursor position in display characters.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.char_count());
    }

    /// Full composition display.
    pub fn display(&self) -> String {
        self.elements.iter().map(|e| e.display()).collect()
    }

    pub fn char_count(&self) -> usize {
        self.elements.iter().map(Element::char_count).sum()
    }

    /// Element indices of the active run: the maximal Romaji run ending at
    /// the cursor — the keystrokes currently being typed. Empty when the
    /// element left of the cursor is settled (a stranded consonant elsewhere
    /// is NOT active; it stays part of the reading at its position).
    fn active_run(&self) -> std::ops::Range<usize> {
        let (index, offset) = self.locate(self.cursor);
        if offset != 0 {
            return index..index;
        }
        let start = self.elements[..index]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        start..index
    }

    /// Keystrokes of the active run (shown as the aux romaji tail).
    pub fn pending(&self) -> String {
        self.elements[self.active_run()]
            .iter()
            .map(|e| e.display())
            .collect()
    }

    /// Conversion reading: everything except the active run. A Romaji
    /// keystroke stranded away from the cursor counts as a literal
    /// character at its position, so `y1` + `ka` reads 「y1か」.
    pub fn reading(&self) -> String {
        let active = self.active_run();
        self.elements
            .iter()
            .enumerate()
            .filter(|(i, _)| !active.contains(i))
            .map(|(_, e)| e.display())
            .collect()
    }

    /// Cursor position within [`Self::reading`]. The active run sits just
    /// before the cursor and is excluded from the reading, so this is the
    /// cursor minus the active run's characters.
    pub fn reading_cursor(&self) -> usize {
        let active_chars: usize = self.elements[self.active_run()]
            .iter()
            .map(Element::char_count)
            .sum();
        self.cursor - active_chars
    }

    /// Push a kana-mode keystroke at the cursor: insert it like any other
    /// element, then re-evaluate the active run it now ends.
    pub fn push_romaji(&mut self, ch: char, romaji: &RomajiConverter) {
        let at = self.split_at_cursor();
        self.elements
            .insert(at, Element::Romaji(ch.to_ascii_lowercase()));
        self.cursor += 1;
        self.evaluate_active_run(romaji);
    }

    /// Re-evaluate the active run (the Romaji run ending at the cursor),
    /// replacing keystrokes a rule consumed with its output. The cursor
    /// lands after the run, whose display may have shrunk (`kyo` → きょ).
    /// A run that no fresh keystroke touched is already at a fixpoint, so
    /// only [`Self::push_romaji`] needs to call this.
    fn evaluate_active_run(&mut self, romaji: &RomajiConverter) {
        let range = self.active_run();
        if range.is_empty() {
            return;
        }
        let run: String = self.elements[range.clone()]
            .iter()
            .filter_map(|e| match e {
                Element::Romaji(c) => Some(*c),
                _ => None,
            })
            .collect();
        let evaluated = evaluate_run(&run, romaji);
        let evaluated_chars: usize = evaluated.iter().map(Element::char_count).sum();
        let prefix_chars: usize = self.elements[..range.start]
            .iter()
            .map(Element::char_count)
            .sum();
        self.elements.splice(range, evaluated);
        self.cursor = prefix_chars + evaluated_chars;
    }

    /// Push a direct-input keystroke at the cursor.
    pub fn push_direct(&mut self, ch: char) {
        let at = self.split_at_cursor();
        self.elements.insert(at, Element::Direct(ch));
        self.cursor += 1;
    }

    /// Insert settled text at the cursor (reconversion reading and other
    /// programmatic strings).
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        let at = self.split_at_cursor();
        self.elements
            .insert(at, Element::Converted(text.to_string()));
        self.cursor += text.chars().count();
    }

    /// Remove the display character before the cursor. A single-character
    /// element is removed whole; a longer `Converted` is truncated. Returns
    /// false when the cursor is at the start.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.remove_display_char(self.cursor - 1);
        self.cursor -= 1;
        true
    }

    /// Remove the display character at the cursor (delete key). Returns
    /// false when the cursor is at the end.
    pub fn delete_at_cursor(&mut self) -> bool {
        if self.cursor >= self.char_count() {
            return false;
        }
        self.remove_display_char(self.cursor);
        true
    }

    /// Settle all Romaji keystrokes in place (`ltu` → っ; unmatched
    /// consonants pass through literally). Called before conversion,
    /// commit, and katakana baking. The cursor keeps its distance from the
    /// end, so an end-of-composition cursor stays at the end.
    pub fn settle_romaji(&mut self, romaji: &RomajiConverter) {
        if !self.elements.iter().any(Element::is_romaji) {
            return;
        }
        let from_end = self.char_count() - self.cursor;
        let mut settled: Vec<Element> = Vec::with_capacity(self.elements.len());
        let mut run = String::new();
        for element in self.elements.drain(..) {
            match element {
                Element::Romaji(ch) => run.push(ch),
                other => {
                    flush_run(&mut settled, &mut run, romaji);
                    settled.push(other);
                }
            }
        }
        flush_run(&mut settled, &mut run, romaji);
        self.elements = settled;
        self.cursor = self.char_count().saturating_sub(from_end);
    }

    /// Convert every settled element's display to katakana permanently.
    /// Called when leaving katakana mode so the preedit doesn't revert.
    pub fn bake_katakana(&mut self) {
        for element in &mut self.elements {
            if let Element::Converted(display) = element {
                *display = karukan_engine::hiragana_to_katakana(display);
            }
        }
    }

    /// Remove one display character, truncating a multi-character
    /// `Converted` or removing a single-character element whole.
    fn remove_display_char(&mut self, pos: usize) {
        let (index, offset) = self.locate(pos);
        let element = &mut self.elements[index];
        if element.char_count() == 1 {
            self.elements.remove(index);
            return;
        }
        let Element::Converted(display) = element else {
            unreachable!("multi-char elements are always Converted");
        };
        let byte_start = display.char_indices().nth(offset).map(|(i, _)| i).unwrap();
        let byte_end = display
            .char_indices()
            .nth(offset + 1)
            .map(|(i, _)| i)
            .unwrap_or(display.len());
        display.replace_range(byte_start..byte_end, "");
    }

    /// Ensure an element boundary at the cursor (splitting a `Converted`
    /// when the cursor is inside it) and return the boundary index.
    fn split_at_cursor(&mut self) -> usize {
        let (index, offset) = self.locate(self.cursor);
        if offset == 0 {
            return index;
        }
        let Element::Converted(display) = &self.elements[index] else {
            unreachable!("multi-char elements are always Converted");
        };
        let byte_offset = display.char_indices().nth(offset).map(|(i, _)| i).unwrap();
        let right = display[byte_offset..].to_string();
        let left = display[..byte_offset].to_string();
        self.elements[index] = Element::Converted(left);
        self.elements.insert(index + 1, Element::Converted(right));
        index + 1
    }

    /// Element index and character offset for a display position. A
    /// position on a boundary returns the element starting there (offset
    /// 0); the end of the composition returns `(len, 0)`.
    fn locate(&self, pos: usize) -> (usize, usize) {
        let mut remaining = pos;
        for (i, element) in self.elements.iter().enumerate() {
            let len = element.char_count();
            if remaining < len {
                return (i, remaining);
            }
            remaining -= len;
        }
        (self.elements.len(), 0)
    }
}

/// Settle one Romaji run into `out` and clear it.
fn flush_run(out: &mut Vec<Element>, run: &mut String, romaji: &RomajiConverter) {
    if run.is_empty() {
        return;
    }
    out.push(Element::Converted(romaji.convert_flush(run)));
    run.clear();
}

/// Evaluate a run of romaji keystrokes into elements: keystrokes consumed
/// by a fired rule become one `Converted`; passthrough keystrokes that can
/// still start a rule (`y`, `k`) stay `Romaji`, others settle (`1`); the
/// converter's trailing pending stays `Romaji` per keystroke.
fn evaluate_run(run: &str, romaji: &RomajiConverter) -> Vec<Element> {
    let mut elements = Vec::new();
    let mut processed = String::new();
    let mut prev = karukan_engine::Converted {
        text: String::new(),
        pending: String::new(),
    };

    for ch in run.chars() {
        processed.push(ch);
        let curr = romaji.convert(&processed);
        // Input consumed this step and output it produced
        let consumed: String = {
            let mut c = prev.pending.clone();
            c.push(ch);
            c.strip_suffix(curr.pending.as_str())
                .unwrap_or(&c)
                .to_string()
        };
        let mut produced = curr.text[prev.text.len()..].to_string();

        // Peel leading passthrough characters (input == output), then the
        // rest is a fired rule's output
        let mut consumed = consumed.as_str();
        while let (Some(c), Some(p)) = (consumed.chars().next(), produced.chars().next()) {
            if c != p {
                break;
            }
            if romaji.is_rule_prefix(&c.to_string()) {
                elements.push(Element::Romaji(c));
            } else {
                elements.push(Element::Converted(c.to_string()));
            }
            consumed = &consumed[c.len_utf8()..];
            produced.drain(..p.len_utf8());
        }
        if !produced.is_empty() {
            elements.push(Element::Converted(produced));
        }
        prev = curr;
    }

    for ch in prev.pending.chars() {
        elements.push(Element::Romaji(ch));
    }
    elements
}
