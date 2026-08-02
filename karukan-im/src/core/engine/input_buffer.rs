//! InputBuffer: a recorded element array plus a caret in the record, with
//! everything else derived by evaluation.
//!
//! **The record** is the single source of truth: one element per input
//! unit plus `gap`, the caret as an element boundary index. Typing
//! `wasedaytk` records
//! `[Converted(わ), Converted(せ), Converted(だ), Romaji(y), Romaji(t), Romaji(k)]`.
//!
//! - [`Element::Romaji`]: one keystroke not yet consumed by a rule (`y`,
//!   `k`, a lone `n`). Shown verbatim; evaluation may later consume it.
//! - [`Element::Converted`]: a fired rule's output (or a settled
//!   passthrough like `1`). Opaque to evaluation — it never reverts.
//! - [`Element::Direct`]: one directly-input keystroke (alphabet/emoji
//!   mode). Opaque to evaluation.
//!
//! **Evaluation** derives every view from the record: the display, the
//! conversion reading, the aux romaji tail, and the display caret
//! ([`InputBuffer::cursor`] is the character count left of the gap). After
//! a romaji keystroke is recorded, the Romaji run ending at the gap is
//! evaluated through the converter: keystrokes a rule consumed are
//! re-recorded as one `Converted`, the rest stay `Romaji`. Elements right
//! of the gap are never touched, so nothing combines across the caret.
//!
//! Record edits never do display-coordinate arithmetic — only
//! [`InputBuffer::set_cursor`] maps a display position back into the
//! record (splitting a multi-character `Converted` when the caret lands
//! inside it). Backspace removes the element left of the gap whole when it
//! shows one character (こ vanishes with its keystrokes, re-exposing the
//! live element before it — `ytko` → BS → `o` gives 「yと」, again 「よ」);
//! a longer きょ is truncated per character. The caret moves without
//! settling anything, so `[Romaji(k), Romaji(y), Direct(K)]` plus `o`
//! typed before the `K` evaluates to 「きょK」.

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

/// The recorded composition: elements plus the caret as a boundary index.
pub(super) struct InputBuffer {
    elements: Vec<Element>,
    /// Caret in the record: an element boundary, `0..=elements.len()`
    gap: usize,
}

impl InputBuffer {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            gap: 0,
        }
    }

    pub fn clear(&mut self) {
        self.elements.clear();
        self.gap = 0;
    }

    pub fn has_elements(&self) -> bool {
        !self.elements.is_empty()
    }

    // --- Record edits -----------------------------------------------------

    /// Record a kana-mode keystroke at the caret, then evaluate the active
    /// run it now ends.
    pub fn push_romaji(&mut self, ch: char, romaji: &RomajiConverter) {
        self.elements
            .insert(self.gap, Element::Romaji(ch.to_ascii_lowercase()));
        self.gap += 1;
        self.evaluate_active_run(romaji);
    }

    /// Record a direct-input keystroke at the caret.
    pub fn push_direct(&mut self, ch: char) {
        self.elements.insert(self.gap, Element::Direct(ch));
        self.gap += 1;
    }

    /// Record settled text at the caret (reconversion reading and other
    /// programmatic strings).
    pub fn insert(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.elements
            .insert(self.gap, Element::Converted(text.to_string()));
        self.gap += 1;
    }

    /// Remove the display character before the caret. A single-character
    /// element is removed whole; a longer `Converted` is truncated. Returns
    /// false when the caret is at the start.
    pub fn backspace(&mut self) -> bool {
        if self.gap == 0 {
            return false;
        }
        let element = &mut self.elements[self.gap - 1];
        if element.char_count() > 1 {
            let Element::Converted(display) = element else {
                unreachable!("multi-char elements are always Converted");
            };
            display.pop();
        } else {
            self.elements.remove(self.gap - 1);
            self.gap -= 1;
        }
        true
    }

    /// Remove the display character at the caret (delete key). Returns
    /// false when the caret is at the end.
    pub fn delete_at_cursor(&mut self) -> bool {
        if self.gap == self.elements.len() {
            return false;
        }
        let element = &mut self.elements[self.gap];
        if element.char_count() > 1 {
            let Element::Converted(display) = element else {
                unreachable!("multi-char elements are always Converted");
            };
            let first = display.chars().next().expect("display is non-empty");
            display.drain(..first.len_utf8());
        } else {
            self.elements.remove(self.gap);
        }
        true
    }

    /// Evaluate the active run (the Romaji run ending at the gap),
    /// re-recording keystrokes a rule consumed as its output. A run no
    /// fresh keystroke touched is already at a fixpoint, so only
    /// [`Self::push_romaji`] needs to call this.
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
        self.gap = range.start + evaluated.len();
        self.elements.splice(range, evaluated);
    }

    /// Settle all Romaji keystrokes in place (`ltu` → っ; unmatched
    /// consonants pass through literally). Called before conversion,
    /// commit, and katakana baking. The caret keeps its distance from the
    /// end, so an end-of-composition caret stays at the end.
    pub fn settle_romaji(&mut self, romaji: &RomajiConverter) {
        if !self.elements.iter().any(Element::is_romaji) {
            return;
        }
        let from_end = self.char_count() - self.cursor();
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
        self.set_cursor(self.char_count().saturating_sub(from_end));
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

    /// Move the caret to a display position, mapping it back into the
    /// record. The only display→record conversion: a position inside a
    /// multi-character `Converted` splits it at the caret.
    pub fn set_cursor(&mut self, pos: usize) {
        let pos = pos.min(self.char_count());
        let mut remaining = pos;
        for (i, element) in self.elements.iter().enumerate() {
            let len = element.char_count();
            if remaining == 0 {
                self.gap = i;
                return;
            }
            if remaining < len {
                let Element::Converted(display) = &self.elements[i] else {
                    unreachable!("multi-char elements are always Converted");
                };
                let byte_offset = display
                    .char_indices()
                    .nth(remaining)
                    .map(|(b, _)| b)
                    .expect("offset is inside the display");
                let right = display[byte_offset..].to_string();
                let left = display[..byte_offset].to_string();
                self.elements[i] = Element::Converted(left);
                self.elements.insert(i + 1, Element::Converted(right));
                self.gap = i + 1;
                return;
            }
            remaining -= len;
        }
        self.gap = self.elements.len();
    }

    // --- Evaluation: views derived from the record ------------------------

    /// Display caret position in characters: everything left of the gap.
    pub fn cursor(&self) -> usize {
        self.elements[..self.gap]
            .iter()
            .map(Element::char_count)
            .sum()
    }

    /// Full composition display.
    pub fn display(&self) -> String {
        self.elements.iter().map(|e| e.display()).collect()
    }

    pub fn char_count(&self) -> usize {
        self.elements.iter().map(Element::char_count).sum()
    }

    /// Element indices of the active run: the maximal Romaji run ending at
    /// the gap — the keystrokes currently being typed. Empty when the
    /// element left of the gap is settled (a stranded consonant elsewhere
    /// is NOT active; it stays part of the reading at its position).
    fn active_run(&self) -> std::ops::Range<usize> {
        let start = self.elements[..self.gap]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        start..self.gap
    }

    /// Keystrokes of the active run (shown as the aux romaji tail).
    pub fn pending(&self) -> String {
        self.elements[self.active_run()]
            .iter()
            .map(|e| e.display())
            .collect()
    }

    /// Conversion reading: everything except the active run. A Romaji
    /// keystroke stranded away from the caret counts as a literal
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

    /// Caret position within [`Self::reading`]. The active run sits just
    /// before the gap and is excluded from the reading, so this is the
    /// display caret minus the active run's characters.
    pub fn reading_cursor(&self) -> usize {
        let active_chars: usize = self.elements[self.active_run()]
            .iter()
            .map(Element::char_count)
            .sum();
        self.cursor() - active_chars
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
