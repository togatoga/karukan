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
//! - [`Element::Converted`]: one settled character — a fired rule's kana,
//!   a passthrough like `1`, or direct input (alphabet/emoji mode). Opaque
//!   to evaluation; it never reverts.
//!
//! **Evaluation** derives everything else: the display, the conversion
//! reading, and the aux romaji tail. After a romaji keystroke is recorded,
//! the Romaji run ending at the cursor is evaluated through the converter:
//! keystrokes a rule consumed are re-recorded as its output. Elements
//! right of the cursor are never touched, so nothing combines across the
//! caret, and the caret moves without settling anything — `[Romaji(k),
//! Romaji(y), Converted(K)]` plus `o` typed before the `K` evaluates to
//! 「きょK」.
//!
//! Every record edit ends with an evaluation. Typing evaluates the run
//! ending at the caret; backspace/delete remove exactly one element and
//! then evaluate the run the removal joined, so the result always equals
//! typing the remaining keystrokes fresh: removing こ from `ytko`
//! re-exposes the live elements (`o` → 「yと」, again 「よ」), and
//! removing the `1` from `yt1t` evaluates `ytt` → 「yっt」.
//!
//! **Keys.** A settled character also records the keys that typed it, for
//! F9/F10 (`si` stays `si`, never `shi`). The converter reports each rule
//! it fired as a [`Span`] (きょ ← `kyo`), and every character a span wrote
//! points at that span. A span's keys count while all its characters still
//! stand together. Once an edit breaks them up (deleting ょ, typing between
//! き and ょ), the keys can no longer be divided among what is left, so
//! those characters stand for themselves and F9/F10 leave them as kana
//! rather than show keys that were never typed. Edits need not know any of
//! this: [`InputBuffer::keystrokes`] tells a whole span from a broken one
//! when it reads them.

use std::rc::Rc;

use karukan_engine::RomajiConverter;
use karukan_engine::romaji::Span;

/// One display character of the composition.
enum Element {
    /// A keystroke not yet consumed by a conversion rule
    Romaji(char),
    /// A settled character: fired rule output (`ko` → こ), passthrough
    /// (`1`), or direct input — excluded from romaji evaluation
    /// `span` is the rule that wrote it, shared by every character it
    /// wrote (「き」 and 「ょ」 both point at きょ ← `kyo`).
    Converted { ch: char, span: Rc<Span> },
}

impl Element {
    fn ch(&self) -> char {
        match self {
            Element::Romaji(ch) | Element::Converted { ch, .. } => *ch,
        }
    }

    /// Direct input: the character is its own key.
    fn direct(ch: char) -> Self {
        Element::Converted {
            ch,
            span: Rc::new(Span {
                text: ch.to_string(),
                keys: ch.to_string(),
            }),
        }
    }

    /// The span that wrote this settled character.
    fn span(&self) -> Option<&Rc<Span>> {
        match self {
            Element::Converted { span, .. } => Some(span),
            Element::Romaji(_) => None,
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
    /// elements: [Romaji(k), Romaji(y), Converted(1), Converted(K)]
    /// boundary: 0         1          2             3             4
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

    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
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

    /// Record a direct-input keystroke (alphabet/emoji mode) at the caret,
    /// settled as-is.
    pub fn push_direct(&mut self, ch: char) {
        self.elements.insert(self.cursor, Element::direct(ch));
        self.cursor += 1;
    }

    /// Record settled text at the caret. Test setup only — production
    /// code always goes through the typed-key paths.
    #[cfg(test)]
    pub fn insert(&mut self, text: &str) {
        let count = text.chars().count();
        self.elements
            .splice(self.cursor..self.cursor, text.chars().map(Element::direct));
        self.cursor += count;
    }

    /// Remove the element before the caret, then evaluate the Romaji run
    /// the removal joined, so the result matches typing the remaining
    /// keystrokes fresh (`yt1t` minus the `1` → 「yっt」). Returns false
    /// when the caret is at the start.
    pub fn backspace(&mut self, romaji: &RomajiConverter) -> bool {
        if self.cursor == 0 {
            return false;
        }
        self.cursor -= 1;
        self.elements.remove(self.cursor);
        self.evaluate_joined_run(romaji);
        true
    }

    /// Remove the element at the caret (delete key), then evaluate the
    /// Romaji run the removal joined. Returns false when the caret is at
    /// the end.
    pub fn delete_at_cursor(&mut self, romaji: &RomajiConverter) -> bool {
        if self.cursor == self.elements.len() {
            return false;
        }
        self.elements.remove(self.cursor);
        self.evaluate_joined_run(romaji);
        true
    }

    /// Evaluate the active run (the Romaji run ending at the cursor),
    /// re-recording keystrokes a rule consumed as its output. Typing never
    /// combines across the caret, so this stops there.
    fn evaluate_active_run(&mut self, romaji: &RomajiConverter) {
        let range = self.active_run();
        let evaluated_len = self.evaluate_range(range.clone(), romaji);
        self.cursor = range.start + evaluated_len;
    }

    /// Evaluate the Romaji run containing the caret — both sides of a
    /// deletion point. The caret keeps its offset from the run start,
    /// clamped to the evaluated length.
    fn evaluate_joined_run(&mut self, romaji: &RomajiConverter) {
        let start = self.elements[..self.cursor]
            .iter()
            .rposition(|e| !e.is_romaji())
            .map(|i| i + 1)
            .unwrap_or(0);
        let end = self.cursor
            + self.elements[self.cursor..]
                .iter()
                .position(|e| !e.is_romaji())
                .unwrap_or(self.elements.len() - self.cursor);
        let offset = self.cursor - start;
        let evaluated_len = self.evaluate_range(start..end, romaji);
        self.cursor = start + offset.min(evaluated_len);
    }

    /// Replace a Romaji range with its evaluation; returns the new length.
    fn evaluate_range(&mut self, range: std::ops::Range<usize>, romaji: &RomajiConverter) -> usize {
        if range.is_empty() {
            return 0;
        }
        let run: String = self.elements[range.clone()]
            .iter()
            .map(Element::ch)
            .collect();
        let evaluated = evaluate_run(&run, romaji);
        let len = evaluated.len();
        self.elements.splice(range, evaluated);
        len
    }

    /// The reading as it would settle: Romaji runs force-converted in
    /// place, everything else as displayed. The non-destructive
    /// counterpart of [`Self::settle_romaji`] — used when the composition
    /// must stay editable (starting a conversion that Escape can undo).
    pub fn settled_reading(&self, romaji: &RomajiConverter) -> String {
        let mut reading = String::new();
        let mut run = String::new();
        for element in &self.elements {
            match element {
                Element::Romaji(ch) => run.push(*ch),
                Element::Converted { ch, .. } => {
                    if !run.is_empty() {
                        reading.push_str(&romaji.convert_flush(&run));
                        run.clear();
                    }
                    reading.push(*ch);
                }
            }
        }
        if !run.is_empty() {
            reading.push_str(&romaji.convert_flush(&run));
        }
        reading
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

    /// What was typed, keystroke for keystroke (`kyouha` for 「きょうは」).
    /// A span whose characters still stand together gives its keys; the
    /// characters of a broken one give themselves (「きょ」 minus ょ → き).
    pub fn keystrokes(&self) -> String {
        let mut out = String::new();
        let mut rest = &self.elements[..];
        while let Some(first) = rest.first() {
            let Some(span) = first.span() else {
                out.push(first.ch());
                rest = &rest[1..];
                continue;
            };
            let together = rest
                .iter()
                .take_while(|e| e.span().is_some_and(|s| Rc::ptr_eq(s, span)))
                .count();
            if together == span.text.chars().count() {
                out.push_str(&span.keys);
            } else {
                out.extend(rest[..together].iter().map(Element::ch));
            }
            rest = &rest[together..];
        }
        out
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
    for span in romaji.convert_flush_spans(run) {
        out.extend(settled(span));
    }
    run.clear();
}

/// The settled characters a span wrote, each pointing at the span.
fn settled(span: Span) -> Vec<Element> {
    let span = Rc::new(span);
    span.text
        .chars()
        .map(|ch| Element::Converted {
            ch,
            span: Rc::clone(&span),
        })
        .collect()
}

/// Evaluate a run of romaji keystrokes: convert the whole run and record
/// one element per output character.
///
/// Rule outputs never contain ASCII (see the converter's contract), so an
/// ASCII character in the output is a keystroke that passed through: it
/// stays live (`Romaji`) if it can still begin a rule (`ykt` → BS → `o`
/// → 「yこ」) and settles otherwise (`1`). Everything else is a fired
/// rule's output, settled for good. The trailing pending stays `Romaji`
/// per keystroke.
///
/// Settling is where the configured width applies, after the classification
/// above: a character settles at the width in force when it was typed, so
/// switching to alphabet input mid-word (`（` then Shift+A) leaves what is
/// already settled alone.
fn evaluate_run(run: &str, romaji: &RomajiConverter) -> Vec<Element> {
    let converted = romaji.convert(run);
    converted
        .spans
        .into_iter()
        .flat_map(|span| {
            if span.text.chars().all(|c| romaji.starts_rule(c)) {
                span.text.chars().map(Element::Romaji).collect()
            } else {
                settled(Span {
                    text: span.text.chars().map(|c| romaji.settle_char(c)).collect(),
                    keys: span.keys,
                })
            }
        })
        .chain(converted.pending.chars().map(Element::Romaji))
        .collect()
}
