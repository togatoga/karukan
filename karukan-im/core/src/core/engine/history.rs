//! What a commit teaches the learning cache, and how the history is read
//! back into the conversion.
//!
//! A commit is told apart from what was shown as its default
//! ([`Shown`]): picking another candidate is `chosen` for it and
//! `displaced` for the default, keeping the default after looking is
//! `confirmed`, and committing the live conversion as it stood is only
//! `accepted`. Each is recorded with the readings typed around it, and a
//! pick so recorded is a rule: where those readings come round again, the
//! picked text stands in for the model's ([`convert_with_history`]).
//!
//! [`convert_with_history`]: InputMethodEngine::convert_with_history

use std::collections::HashMap;
use std::rc::Rc;

use karukan_engine::Evidence;

use super::segment::Piece;
use karukan_engine::Rule;
use karukan_engine::learning::context_match;

use super::types::{ComposingChunk, Rewrite};
use super::*;
use crate::core::candidate::{Candidate, CandidateSource};
use crate::core::state::{Segment, Shown};

/// How far the history must outscore the model's text before it is
/// replaced: the model's answer is the default, and a little evidence for
/// another is not enough to override it.
const REWRITE_MARGIN: f64 = 0.3;

/// Chars a rule's reading and the readings around it must agree on, all
/// told, to apply: when the model's answer cuts into segments, which show
/// where its words start, and when it does not (its kana rewritten, or left
/// unconverted), where a wrong place is harder to rule out.
const RULE_CHARS: usize = 4;
const RULE_CHARS_UNCUT: usize = 6;

/// Something a commit teaches.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Lesson {
    pub reading: String,
    pub text: String,
    /// The readings typed before and after it, for its context (the
    /// editor's text before it when it starts the input).
    pub left: String,
    pub right: String,
    pub evidence: Evidence,
    pub weight: f64,
    /// Whether it may make a unit that is not there yet. Committing what
    /// was shown only keeps existing units fresh, or the model's output
    /// would become history of its own.
    pub create: bool,
}

impl Lesson {
    fn new(reading: &str, text: &str, left: &str, right: &str, evidence: Evidence) -> Self {
        Self {
            reading: reading.to_string(),
            text: text.to_string(),
            left: left.to_string(),
            right: right.to_string(),
            evidence,
            weight: 1.0,
            create: true,
        }
    }

    /// `accepted` for a unit only if it is already learned.
    fn reinforce(reading: &str, text: &str, left: &str, right: &str) -> Self {
        Self {
            create: false,
            ..Self::new(reading, text, left, right, Evidence::Accepted)
        }
    }
}

/// `picked` chosen over `default`, both for `reading` typed between `left`
/// and `right`. Kana both texts share with the reading at either end are
/// the reading's own and go to the context, so what is learned is the
/// stretch that changed: 部が → 分が teaches ぶ → 分 before が.
fn pick_lessons(
    reading: &str,
    picked: &str,
    default: &str,
    left: &str,
    right: &str,
) -> [Lesson; 2] {
    let r: Vec<char> = reading.chars().collect();
    let p: Vec<char> = picked.chars().collect();
    let d: Vec<char> = default.chars().collect();
    let shortest = r.len().min(p.len()).min(d.len());
    let kana = |c: char| matches!(c, '\u{3041}'..='\u{3096}');
    let lead = (0..shortest.saturating_sub(1))
        .take_while(|&k| kana(r[k]) && r[k] == p[k] && r[k] == d[k])
        .count();
    let tail = (1..shortest.saturating_sub(lead))
        .take_while(|&k| {
            let c = r[r.len() - k];
            kana(c) && c == p[p.len() - k] && c == d[d.len() - k]
        })
        .count();
    let mid = |v: &[char]| v[lead..v.len() - tail].iter().collect::<String>();
    let left = format!("{left}{}", r[..lead].iter().collect::<String>());
    let right = format!("{}{right}", r[r.len() - tail..].iter().collect::<String>());
    let reading = mid(&r);
    [
        Lesson::new(&reading, &mid(&p), &left, &right, Evidence::Chosen),
        Lesson::new(&reading, &mid(&d), &left, &right, Evidence::Displaced),
    ]
}

/// A rule found in a chunk's reading: the stretch it covers (chars), the
/// text it puts there, and how many chars it agrees on with its context.
struct Hit {
    start: usize,
    end: usize,
    agree: usize,
    rule: Rule,
    original: String,
}

/// The text `pieces` give the stretch `start..end` of their reading, when
/// `start` is where a piece starts: the pieces up to `end`, and of the last
/// one only what reads up to `end` when what reads past it is its own kana
/// (部が for ぶ gives 部). None when the stretch cannot be told apart.
fn text_of(pieces: &[Piece], start: usize, end: usize) -> Option<String> {
    let mut at = 0;
    let mut text = String::new();
    for piece in pieces {
        let len = piece.reading.chars().count();
        let next = at + len;
        if next <= start {
            at = next;
            continue;
        }
        if at < start {
            return None;
        }
        if next <= end {
            text.push_str(&piece.text);
            if next == end {
                return Some(text);
            }
        } else {
            let excess: String = piece.reading.chars().skip(end - at).collect();
            let kept = piece.text.strip_suffix(excess.as_str())?;
            if kept.is_empty() {
                return None;
            }
            text.push_str(kept);
            return Some(text);
        }
        at = next;
    }
    None
}

/// The one stretch of reading where two cuts of the same reading differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stretch {
    pub reading: String,
    /// Its text in the first cut and in the second.
    pub texts: (String, String),
    /// The text before it, the same in both, and its reading.
    pub before: String,
    pub before_reading: String,
}

/// Where `a` and `b`, two cuts of one reading, differ, when that is one
/// stretch between edges they share and not all of it.
pub(super) fn single_difference(a: &[Piece], b: &[Piece]) -> Option<Stretch> {
    let (mut i, mut j) = (0, 0);
    let mut before = String::new();
    let mut before_reading = String::new();
    let mut found = None;
    let mut stretches = 0;
    while i < a.len() && j < b.len() {
        let mut group = (Piece::default(), String::new());
        let (mut len_a, mut len_b) = (0, 0);
        loop {
            if len_a <= len_b && i < a.len() {
                group.0.reading.push_str(&a[i].reading);
                group.0.text.push_str(&a[i].text);
                len_a += a[i].reading.chars().count();
                i += 1;
            } else if j < b.len() {
                group.1.push_str(&b[j].text);
                len_b += b[j].reading.chars().count();
                j += 1;
            }
            if len_a == len_b || (i == a.len() && j == b.len()) {
                break;
            }
        }
        stretches += 1;
        if group.0.text != group.1 {
            if found.is_some() {
                return None;
            }
            found = Some(Stretch {
                reading: group.0.reading.clone(),
                texts: (group.0.text.clone(), group.1),
                before: before.clone(),
                before_reading: before_reading.clone(),
            });
        }
        before.push_str(&group.0.text);
        before_reading.push_str(&group.0.reading);
    }
    found.filter(|_| stretches > 1)
}

/// Cuts of `(reading, text)` already worked out: the live conversion asks
/// for the same few on every keystroke.
#[derive(Default)]
pub(super) struct Alignments {
    /// Keyed by (reading, text): the cut and when it was last asked for.
    entries: HashMap<(String, String), Aligned>,
    clock: u64,
}

struct Aligned {
    pieces: Rc<[Piece]>,
    used: u64,
}

impl Alignments {
    const CAPACITY: usize = 256;
}

impl InputMethodEngine {
    /// `text`, the conversion of `reading`, cut into segments; one piece
    /// when it does not come apart.
    pub(super) fn align(&mut self, reading: &str, text: &str) -> Rc<[Piece]> {
        let key = (reading.to_string(), text.to_string());
        self.alignments.clock += 1;
        let clock = self.alignments.clock;
        if let Some(aligned) = self.alignments.entries.get_mut(&key) {
            aligned.used = clock;
            return aligned.pieces.clone();
        }
        let pieces: Rc<[Piece]> = self.cut(reading, text).into();
        let entries = &mut self.alignments.entries;
        if entries.len() >= Alignments::CAPACITY
            && let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, a)| a.used)
                .map(|(k, _)| k.clone())
        {
            entries.remove(&oldest);
        }
        entries.insert(
            key,
            Aligned {
                pieces: pieces.clone(),
                used: clock,
            },
        );
        pieces
    }

    /// Record `lessons`, revertibly (`undo.rs`). Nothing in emoji mode: the
    /// buffer is a `:query`, not a kana reading.
    pub(super) fn learn(&mut self, lessons: &[Lesson]) {
        if self.mode.current() == InputMode::Emoji {
            return;
        }
        if let Some(shared) = self.learning.clone() {
            let mut cache = shared
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for l in lessons {
                // Saved first, so a Backspace right after can take it back.
                self.learning_undo
                    .push(&l.reading, &l.text, cache.snapshot(&l.reading, &l.text));
                if l.create {
                    cache.record(&l.reading, &l.text, &l.left, &l.right, l.evidence, l.weight);
                } else {
                    cache.reinforce(&l.reading, &l.text, &l.left, &l.right);
                }
            }
        }
    }

    /// What the composition shows as its default: the live conversion, or
    /// the kana when there is none.
    pub(super) fn composing_shown(&self) -> Shown {
        let reading = self.input_buf.settled_reading(&self.converters.romaji);
        let live = self.live_text_with_pending();
        Shown {
            text: if live.is_empty() {
                reading.clone()
            } else {
                live
            },
            reading,
        }
    }

    /// `text` for `reading` was committed where `shown` was the default,
    /// after `left`: `confirmed` when it is the default, else `chosen` for
    /// it and `displaced` for the default, the whole and, when both read the
    /// same and differ in one stretch of it, that stretch with the readings
    /// around it, so a pick made over a sentence is a rule for the word it
    /// changed.
    pub(super) fn choice_lessons(
        &mut self,
        shown: &Shown,
        reading: &str,
        text: &str,
        left: &str,
    ) -> Vec<Lesson> {
        if text == shown.text {
            return vec![Lesson::new(reading, text, left, "", Evidence::Confirmed)];
        }
        let mut lessons = vec![
            Lesson::new(reading, text, left, "", Evidence::Chosen),
            Lesson::new(&shown.reading, &shown.text, left, "", Evidence::Displaced),
        ];
        // The model's own text picked back over the history's rewrite of
        // it: each rule that rewrote is taught the other way round, in its
        // own context, which cancels it there.
        if let Some((rewritten, original)) = self.rewritten_live()
            && shown.text == rewritten
            && text == original
        {
            for r in self.chunks.iter().flat_map(|c| &c.rewrites) {
                if !r.original.is_empty() {
                    lessons.push(Lesson::new(
                        &r.reading,
                        &r.original,
                        &r.left,
                        &r.right,
                        Evidence::Chosen,
                    ));
                }
                lessons.push(Lesson::new(
                    &r.reading,
                    &r.text,
                    &r.left,
                    &r.right,
                    Evidence::Displaced,
                ));
            }
            return lessons;
        }
        if shown.reading == reading {
            let default = self.align(reading, &shown.text);
            let picked = self.align(reading, text);
            if let Some(stretch) = single_difference(&default, &picked) {
                let before = self.context_before(&stretch.before_reading);
                let after: String = reading
                    .chars()
                    .skip(stretch.before_reading.chars().count() + stretch.reading.chars().count())
                    .collect();
                lessons.extend(pick_lessons(
                    &stretch.reading,
                    &stretch.texts.1,
                    &stretch.texts.0,
                    &before,
                    &after,
                ));
            }
        }
        lessons
    }

    /// The model's `model` text for a chunk's `reading`, with the rules the
    /// history holds applied: where a learned reading turns up between
    /// readings that agree with a rule's context ([`RULE_CHARS`], or
    /// [`RULE_CHARS_UNCUT`] when the model's text does not show where its
    /// words start), and the rule outscores what the model wrote there by
    /// [`REWRITE_MARGIN`], that stretch is the rule's text. The reading is
    /// cut there rather than the text: what precedes the first rule is the
    /// model's text for it, and the rest is converted again on the model,
    /// each part after what is before it, the way chunks are. So it works
    /// however the model wrote that stretch, its kana rewritten or not
    /// converted at all.
    ///
    /// `base_ctx` and `combined` are the chunk's lctx; `reading_before` and
    /// `reading_after` what is typed around it in the composition. Only
    /// what has been typed counts on the right: a rule that needs more
    /// applies once it is there.
    pub(super) fn convert_with_history(
        &mut self,
        reading: &str,
        model: String,
        base_ctx: &str,
        combined: &str,
        reading_before: &str,
        reading_after: &str,
    ) -> (String, Vec<Rewrite>) {
        if self.mode.current() != InputMode::Hiragana {
            return (model, Vec::new());
        }
        let chars: Vec<char> = reading.chars().collect();
        let found: Vec<(usize, usize, Rule)> = {
            let Some(cache) = self.learning_cache() else {
                return (model, Vec::new());
            };
            (0..chars.len())
                .flat_map(|i| (i + 1..=chars.len()).map(move |j| (i, j)))
                .flat_map(|(i, j)| {
                    let stretch: String = chars[i..j].iter().collect();
                    if !cache.contains_reading(&stretch) {
                        return Vec::new();
                    }
                    cache
                        .rules(&stretch)
                        .into_iter()
                        .map(|rule| (i, j, rule))
                        .collect()
                })
                .collect()
        };
        if found.is_empty() {
            return (model, Vec::new());
        }

        let pieces = self.align(reading, &model);
        let cut = pieces.len() > 1;
        let starts: Vec<usize> = pieces
            .iter()
            .scan(0, |at, p| {
                let start = *at;
                *at += p.reading.chars().count();
                Some(start)
            })
            .collect();
        let mut hits = Vec::new();
        for (i, j, rule) in found {
            if cut && !starts.contains(&i) {
                continue;
            }
            let before: String = reading_before
                .chars()
                .chain(chars[..i].iter().copied())
                .collect();
            let left = self.context_before(&before);
            let right: String = chars[j..].iter().collect::<String>() + reading_after;
            let (l, r) = context_match(&rule.left, &rule.right, &left, &right);
            let agree = (j - i) + l + r;
            if agree < if cut { RULE_CHARS } else { RULE_CHARS_UNCUT } {
                continue;
            }
            let original = if cut {
                text_of(&pieces, i, j).map(|t| self.settle_text(&t))
            } else {
                None
            };
            if original.as_deref() == Some(rule.surface.as_str()) {
                continue;
            }
            let stretch: String = chars[i..j].iter().collect();
            let against = original.as_deref().map_or(0.0, |o| {
                self.learning_cache()
                    .map_or(0.0, |c| c.context_score(&stretch, o, &left, &right))
            });
            if rule.score <= against + REWRITE_MARGIN {
                continue;
            }
            hits.push(Hit {
                start: i,
                end: j,
                agree,
                rule,
                original: original.unwrap_or_default(),
            });
        }
        // The rules that agree on the most go first; none overlaps another.
        hits.sort_by_key(|h| std::cmp::Reverse((h.agree, h.end - h.start)));
        let mut taken: Vec<Hit> = Vec::new();
        for hit in hits {
            if taken
                .iter()
                .all(|t| hit.end <= t.start || t.end <= hit.start)
            {
                taken.push(hit);
            }
        }
        if taken.is_empty() {
            return (model, Vec::new());
        }
        taken.sort_by_key(|h| h.start);

        let mut out = String::new();
        let mut rewrites = Vec::new();
        let mut at = 0;
        for hit in taken {
            if at < hit.start {
                let gap = if at == 0 && cut {
                    text_of(&pieces, 0, hit.start)
                } else {
                    None
                };
                let gap = match gap {
                    Some(text) => text,
                    None => self.convert_gap(&chars[at..hit.start], base_ctx, combined, &out),
                };
                out.push_str(&gap);
            }
            out.push_str(&hit.rule.surface);
            rewrites.push(Rewrite {
                start: hit.start,
                reading: chars[hit.start..hit.end].iter().collect(),
                original: hit.original,
                text: hit.rule.surface,
                left: hit.rule.left,
                right: hit.rule.right,
            });
            at = hit.end;
        }
        if at < chars.len() {
            let gap = self.convert_gap(&chars[at..], base_ctx, combined, &out);
            out.push_str(&gap);
        }
        if out == model {
            return (model, Vec::new());
        }
        (out, rewrites)
    }

    /// A stretch of a chunk between rules, on the model, after the chunk's
    /// lctx and what the chunk has so far.
    fn convert_gap(
        &mut self,
        gap: &[char],
        base_ctx: &str,
        combined: &str,
        so_far: &str,
    ) -> String {
        let reading: String = gap.iter().collect();
        let lctx = self.lctx_for(base_ctx, &format!("{combined}{so_far}"));
        self.convert_chunk(&reading, &lctx)
    }

    /// What a unit's left context is read from: the reading typed before it
    /// in this composition, which the model and the rewrites leave as it
    /// is, or the editor's text when it starts the input.
    pub(super) fn context_before(&self, reading_before: &str) -> String {
        if reading_before.is_empty() {
            self.truncate_context_for_api()
        } else {
            reading_before.to_string()
        }
    }

    /// The context of what is being converted, left and right: the
    /// segments around the focused one when split.
    pub(super) fn learning_context(&self) -> (String, String) {
        let (left, right) = self.state.neighbours();
        let before: String = left.iter().map(|s| s.reading.as_str()).collect();
        let after: String = right.iter().map(|s| s.reading.as_str()).collect();
        (self.context_before(&before), after)
    }

    /// The live conversion as the history rewrote it and as the model gave
    /// it, when the history rewrote any of it; with the romaji tail as
    /// `live_text_with_pending` adds it.
    pub(super) fn rewritten_live(&self) -> Option<(String, String)> {
        if self.chunks.iter().all(|c| c.rewrites.is_empty())
            || self.input_buf.cursor() != self.input_buf.char_count()
        {
            return None;
        }
        let tail = match self.settled_tail() {
            Some(_) => String::new(),
            None => self
                .converters
                .romaji
                .convert_flush(&self.input_buf.pending()),
        };
        let join = |text: fn(&ComposingChunk) -> &str| {
            let joined: String = self.chunks.iter().map(text).collect();
            format!("{}{tail}", self.settle_text(&joined))
        };
        Some((join(|c| &c.converted), join(|c| &c.model)))
    }

    /// Put the live conversion's rewrite first, as the history's, and the
    /// model's own text second, so a rewrite that missed is one key away.
    pub(super) fn put_rewrite_first(&self, candidates: &mut Vec<Candidate>, reading: &str) {
        let Some((rewritten, original)) = self.rewritten_live() else {
            return;
        };
        candidates.retain(|c| c.text != rewritten && c.text != original);
        for (at, text, source) in [
            (0, rewritten, CandidateSource::Learning),
            (1, original, CandidateSource::Model),
        ] {
            candidates.insert(
                at,
                Candidate {
                    text,
                    reading: Some(reading.to_string()),
                    source: Some(source),
                    description: None,
                },
            );
        }
    }

    /// The model's own text for a segment of the rewritten live conversion
    /// that starts at `start` (reading chars) and shows `text`: a rewrite
    /// inside it undone. None when no rewrite falls inside it.
    pub(super) fn segment_original(
        &self,
        start: usize,
        reading: &str,
        text: &str,
    ) -> Option<String> {
        let end = start + reading.chars().count();
        let mut chunk_start = 0;
        for chunk in &self.chunks {
            for r in chunk.rewrites.iter().filter(|r| !r.original.is_empty()) {
                let at = chunk_start + r.start;
                if at >= start && at + r.reading.chars().count() <= end && text.contains(&r.text) {
                    return Some(text.replacen(&r.text, &r.original, 1));
                }
            }
            chunk_start += chunk.reading.chars().count();
        }
        None
    }

    /// Order the model's alternatives by what the history says of their
    /// pieces: each is cut against `reading` and the scores of its
    /// segments summed, each after the text before it. The head stays: it
    /// is what the live conversion showed.
    pub(super) fn rank_model_candidates(&mut self, reading: &str, candidates: &mut [String]) {
        if candidates.len() < 3 || self.learning.is_none() {
            return;
        }
        let (ctx, _) = self.learning_context();
        let mut scored: Vec<(f64, String)> = candidates[1..]
            .iter()
            .map(|text| (self.history_score(reading, text, &ctx), text.clone()))
            .collect();
        scored.sort_by(|a, b| b.0.total_cmp(&a.0));
        for (slot, (_, text)) in candidates[1..].iter_mut().zip(scored) {
            *slot = text;
        }
    }

    /// The history's score of `text` for `reading` after `preceding`,
    /// summed over the segments it cuts into.
    fn history_score(&mut self, reading: &str, text: &str, preceding: &str) -> f64 {
        let pieces = self.align(reading, text);
        let Some(cache) = self.learning_cache() else {
            return 0.0;
        };
        let mut read = String::new();
        let mut total = 0.0;
        for (k, piece) in pieces.iter().enumerate() {
            let before = if read.is_empty() { preceding } else { &read };
            let after: String = pieces[k + 1..].iter().map(|p| p.reading.as_str()).collect();
            total += cache.score(&piece.reading, &piece.text, before, &after);
            read.push_str(&piece.reading);
        }
        total
    }

    /// The composition committed as shown: the whole `accepted`, and each
    /// chunk the model converted and each rewritten stretch too when they
    /// are learned already, with the readings around them.
    pub(super) fn live_lessons(&self, reading: &str, text: &str) -> Vec<Lesson> {
        let ctx = self.truncate_context_for_api();
        let mut lessons = vec![Lesson::new(reading, text, &ctx, "", Evidence::Accepted)];
        // Only when the live conversion is what was committed.
        let live = self.live_text();
        if live.is_empty() || !text.starts_with(&live) {
            return lessons;
        }
        let mut read = String::new();
        for (k, chunk) in self.chunks.iter().enumerate() {
            let left = self.context_before(&read);
            read.push_str(&chunk.reading);
            if !chunk.reading.chars().any(chunk::is_japanese) {
                continue;
            }
            // The chunk itself, when it is not the whole already taught.
            if chunk.reading != reading {
                let right: String = self.chunks[k + 1..]
                    .iter()
                    .map(|c| c.reading.as_str())
                    .collect();
                let converted = self.settle_text(&chunk.converted);
                lessons.push(Lesson::reinforce(&chunk.reading, &converted, &left, &right));
            }
            // A rewritten stretch is accepted as what it was shown as: only
            // the weakest evidence, so a rewrite cannot feed itself.
            for r in &chunk.rewrites {
                lessons.push(Lesson::reinforce(&r.reading, &r.text, &r.left, &r.right));
            }
        }
        lessons
    }

    /// A split conversion committed: each segment whose list was built
    /// against its own head (the text it was cut as), with the readings of
    /// the segments around it, and the whole against what the conversion
    /// opened with. A segment never focused teaches nothing: its pairing
    /// with the reading is a guess.
    pub(super) fn segment_lessons(&mut self, segments: &[Segment]) -> Vec<Lesson> {
        let learnable = |c: &Candidate| c.source.is_none_or(|s| s.is_learnable());
        let ctx = self.truncate_context_for_api();
        let mut lessons = Vec::new();
        let mut read = String::new();
        for (k, segment) in segments.iter().enumerate() {
            let left = self.context_before(&read);
            read.push_str(&segment.reading);
            let right: String = segments[k + 1..]
                .iter()
                .map(|s| s.reading.as_str())
                .collect();
            let (Some(selected), Some(head)) = (
                segment.candidates.selected(),
                segment.candidates.candidates().first(),
            ) else {
                continue;
            };
            if !segment.built || !learnable(selected) {
                continue;
            }
            let r = &segment.reading;
            if selected.text == head.text {
                lessons.push(Lesson::new(
                    r,
                    &selected.text,
                    &left,
                    &right,
                    Evidence::Confirmed,
                ));
            } else if learnable(head) {
                lessons.extend(pick_lessons(r, &selected.text, &head.text, &left, &right));
            } else {
                lessons.push(Lesson::new(
                    r,
                    &selected.text,
                    &left,
                    &right,
                    Evidence::Chosen,
                ));
            }
        }

        // Not when a segment holds something that must not be learned (a
        // date is stale tomorrow): the sentence would carry it.
        let all_learnable = segments
            .iter()
            .all(|s| s.candidates.selected().is_none_or(learnable));
        if all_learnable && let Some(shown) = self.state.shown().cloned() {
            let reading: String = segments.iter().map(|s| s.reading.as_str()).collect();
            let text: String = segments.iter().map(Segment::text).collect();
            if text == shown.text {
                lessons.push(Lesson::new(&reading, &text, &ctx, "", Evidence::Confirmed));
            } else {
                lessons.push(Lesson::new(&reading, &text, &ctx, "", Evidence::Chosen));
                lessons.push(Lesson::new(
                    &shown.reading,
                    &shown.text,
                    &ctx,
                    "",
                    Evidence::Displaced,
                ));
            }
        }
        lessons
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pieces(cut: &str) -> Vec<Piece> {
        cut.split('|')
            .map(|p| {
                let (reading, text) = p.split_once('=').unwrap();
                Piece {
                    reading: reading.to_string(),
                    text: text.to_string(),
                }
            })
            .collect()
    }

    #[test]
    fn a_pick_learns_the_stretch_that_changed() {
        let [chosen, displaced] = pick_lessons("ぶが", "分が", "部が", "しあいは", "わるい");
        assert_eq!(
            (
                chosen.reading.as_str(),
                chosen.text.as_str(),
                chosen.right.as_str()
            ),
            ("ぶ", "分", "がわるい")
        );
        assert_eq!(
            (displaced.text.as_str(), displaced.left.as_str()),
            ("部", "しあいは")
        );
        // Shared kana at the start go to the left.
        let [chosen, _] = pick_lessons("おはし", "お箸", "お橋", "", "");
        assert_eq!(
            (chosen.reading.as_str(), chosen.left.as_str()),
            ("はし", "お")
        );
        // Never all of it.
        let [chosen, _] = pick_lessons("は", "は", "ハ", "", "");
        assert_eq!(chosen.reading, "は");
    }

    #[test]
    fn the_text_of_a_stretch_of_the_reading() {
        let cut = pieces("この=この|しょうぶは=勝負は|ぶが=部が|わるい=悪い");
        assert_eq!(text_of(&cut, 7, 8).as_deref(), Some("部"));
        assert_eq!(text_of(&cut, 7, 9).as_deref(), Some("部が"));
        assert_eq!(text_of(&cut, 0, 7).as_deref(), Some("この勝負は"));
        // Not from inside a piece, nor past what its own kana allow.
        assert_eq!(text_of(&cut, 6, 8), None);
        assert_eq!(text_of(&cut, 9, 10), None);
    }

    #[test]
    fn the_one_stretch_two_cuts_differ_in() {
        let a = pieces("きょうは=今日は|いしゃに=医者に|いく=行く");
        let b = pieces("きょうは=今日は|いしゃに=慰謝に|いく=行く");
        let stretch = single_difference(&a, &b).unwrap();
        assert_eq!(stretch.reading, "いしゃに");
        assert_eq!(stretch.texts, ("医者に".to_string(), "慰謝に".to_string()));
        assert_eq!(stretch.before, "今日は");
    }

    #[test]
    fn edges_the_cuts_do_not_share_widen_the_stretch() {
        let a = pieces("きょう=今日|はいしゃに=歯医者に|いく=行く");
        let b = pieces("きょうは=京は|いしゃに=医者に|いく=行く");
        let stretch = single_difference(&a, &b).unwrap();
        assert_eq!(stretch.reading, "きょうはいしゃに");
        assert_eq!(stretch.texts.1, "京は医者に");
        assert_eq!(stretch.before, "");
    }

    #[test]
    fn two_stretches_or_the_whole_is_none() {
        let a = pieces("きょうは=今日は|いしゃに=医者に|いく=行く");
        let b = pieces("きょうは=京は|いしゃに=医者に|いく=逝く");
        assert_eq!(single_difference(&a, &b), None);
        let c = pieces("きょうは=京は|いしゃに=慰謝に|いく=逝く");
        assert_eq!(single_difference(&a, &c), None);
        assert_eq!(single_difference(&a, &a), None);
    }
}
