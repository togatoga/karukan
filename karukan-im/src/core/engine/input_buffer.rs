//! InputBuffer: a recorded element array plus a caret, with every view
//! derived by evaluation.
//!
//! **The record** is the single source of truth: one element per display
//! character plus `cursor`, the caret as an element index. Typing `kyo` records
//! `[Romaji(k), Romaji(y), Romaji(o)]`, which evaluation re-records as
//! `[Converted(き), Converted(ょ)]` — elements and displayed characters
//! always correspond one to one, so the record can never disagree with
//! what is shown, and the caret is simply an index into both.
//!
//! - [`Element::Romaji`]: one keystroke not yet consumed by a rule (`y`,
//!   `k`, a lone `n`). Shown verbatim; evaluation may later consume it.
//! - [`Element::Converted`]: one character of settled output — a fired
//!   rule's kana or a passthrough like `1`. Opaque to evaluation; it never
//!   reverts.
//! - [`Element::Direct`]: one directly-input keystroke (alphabet/emoji
//!   mode). Opaque to evaluation.
//!
//! **Evaluation** derives everything else: the display, the conversion
//! reading, and the aux romaji tail. After a romaji keystroke is recorded,
//! the Romaji run ending at the cursor is evaluated through the converter:
//! keystrokes a rule consumed are re-recorded as its output. Elements
//! right of the cursor are never touched, so nothing combines across the
//! caret, and the caret moves without settling anything — `[Romaji(k),
//! Romaji(y), Direct(K)]` plus `o` typed before the `K` evaluates to
//! 「きょK」.
//!
//! Backspace and delete remove exactly one element. Removing こ (recorded
//! from `ko`) drops both its keystrokes and re-exposes the live element
//! before it: `ytko` → BS → `o` gives 「yと」, again 「よ」.

use karukan_engine::RomajiConverter;

/// One display character of the composition.
#[derive(Clone, Copy)]
enum Element {
    /// A keystroke not yet consumed by a conversion rule
    Romaji(char),
    /// Settled output: a fired rule's character (`ko` → こ) or passthrough (`1`)
    Converted(char),
    /// A directly-input keystroke — excluded from romaji evaluation
    Direct(char),
}

impl Element {
    fn ch(&self) -> char {
        match self {
            Element::Romaji(ch) | Element::Converted(ch) | Element::Direct(ch) => *ch,
        }
    }

    fn is_romaji(&self) -> bool {
        matches!(self, Element::Romaji(_))
    }
}

/// The recorded composition: elements plus the caret index.
pub(super) struct InputBuffer {
    elements: Vec<Element>,
    /// Caret: a boundary index into `elements`, which — with one element
    /// per display character — is also the display position.
    ///
    /// ```text
    /// elements: [Romaji(k), Romaji(y), Converted(1), Direct(K)]
    /// boundary: 0         1          2             3          4
    ///                                ↑ cursor = 2 (between y and 1)
    /// ```
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

    // --- Record edits -----------------------------------------------------

    /// Record a kana-mode keystroke at the caret, then evaluate the active
    /// run it now ends.
    pub fn push_romaji(&mut self, ch: char, romaji: &RomajiConverter) {
        self.elements
            .insert(self.cursor, Element::Romaji(ch.to_ascii_lowercase()));
        self.cursor += 1;
        self.evaluate_active_run(romaji);
    }

    /// Record a direct-input keystroke at the caret.
    pub fn push_direct(&mut self, ch: char) {
        self.elements.insert(self.cursor, Element::Direct(ch));
        self.cursor += 1;
    }

    /// Record settled text at the caret (reconversion reading and other
    /// programmatic strings).
    pub fn insert(&mut self, text: &str) {
        let settled = text.chars().map(Element::Converted);
        let count = self
            .elements
            .splice(self.cursor..self.cursor, settled)
            .count();
        debug_assert_eq!(count, 0);
        self.cursor += text.chars().count();
    }

    /// Remove the element before the caret. Returns false when the caret
    /// is at the start.
    pub fn backspace(&mut self) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.elements.remove(self.cursor);
        true
    }

    /// Remove the element at the caret (delete key). Returns false when
    /// the caret is at the end.
    pub fn delete_at_cursor(&mut self) -> bool {
        if self.cursor == self.elements.len() {
            return false;
        }
        self.elements.remove(self.cursor);
        true
    }

    /// Evaluate the active run (the Romaji run ending at the cursor),
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
            .map(Element::ch)
            .collect();
        let evaluated = evaluate_run(&run, romaji);
        self.cursor = range.start + evaluated.len();
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
        let from_end = self.elements.len() - self.cursor;
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
        self.cursor = self.elements.len().saturating_sub(from_end);
    }

    /// Convert every settled element to katakana permanently. Called when
    /// leaving katakana mode so the preedit doesn't revert.
    pub fn bake_katakana(&mut self) {
        for element in &mut self.elements {
            if let Element::Converted(ch) = element {
                let katakana = karukan_engine::hiragana_to_katakana(&ch.to_string());
                *ch = katakana.chars().next().unwrap_or(*ch);
            }
        }
    }

    /// Move the caret to a display position (also its element index).
    pub fn set_cursor(&mut self, pos: usize) {
        self.cursor = pos.min(self.elements.len());
    }

    // --- Evaluation: views derived from the record ------------------------

    /// Display caret position (== the element index of the caret).
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Full composition display.
    pub fn display(&self) -> String {
        self.elements.iter().map(Element::ch).collect()
    }

    pub fn char_count(&self) -> usize {
        self.elements.len()
    }

    /// Element indices of the active run: the maximal Romaji run ending at
    /// the cursor — the keystrokes currently being typed. Empty when the
    /// element left of the cursor is settled (a stranded consonant elsewhere
    /// is NOT active; it stays part of the reading at its position).
    fn active_run(&self) -> std::ops::Range<usize> {
        let start = self.elements[..self.cursor]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        start..self.cursor
    }

    /// Keystrokes of the active run (shown as the aux romaji tail).
    pub fn pending(&self) -> String {
        self.elements[self.active_run()]
            .iter()
            .map(Element::ch)
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
            .map(|(_, e)| e.ch())
            .collect()
    }

    /// Caret position within [`Self::reading`]. The active run sits just
    /// before the cursor and is excluded from the reading, so this is the
    /// caret minus the active run's length.
    pub fn reading_cursor(&self) -> usize {
        self.cursor - self.active_run().len()
    }
}

/// Settle one Romaji run into `out` and clear it.
fn flush_run(out: &mut Vec<Element>, run: &mut String, romaji: &RomajiConverter) {
    if run.is_empty() {
        return;
    }
    out.extend(romaji.convert_flush(run).chars().map(Element::Converted));
    run.clear();
}

/// Evaluate a run of romaji keystrokes into elements: keystrokes consumed
/// by a fired rule become its output characters (`Converted` each);
/// passthrough keystrokes that can still start a rule (`y`, `k`) stay
/// `Romaji`, others settle (`1`); the converter's trailing pending stays
/// `Romaji` per keystroke.
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
                elements.push(Element::Converted(c));
            }
            consumed = &consumed[c.len_utf8()..];
            produced.drain(..p.len_utf8());
        }
        elements.extend(produced.chars().map(Element::Converted));
        prev = curr;
    }

    elements.extend(prev.pending.chars().map(Element::Romaji));
    elements
}
