//! What a commit teaches the learning cache, and how the history is read
//! back into the conversion.
//!
//! A commit is told apart from what was shown as its default
//! ([`Shown`]): picking another candidate is `chosen` for it and
//! `displaced` for the default, keeping the default after looking is
//! `confirmed`, and committing the live conversion as it stood is only
//! `accepted`.

use std::collections::HashMap;
use std::rc::Rc;

use karukan_engine::Evidence;

use super::segment::Piece;
use super::types::Rewrite;
use super::*;
use crate::core::state::{Segment, Shown};

/// How far the history must outscore the model's text before it is
/// replaced: the model's answer is the default, and a little evidence for
/// another is not enough to override it.
const REWRITE_MARGIN: f64 = 0.3;

/// Weight of the word-level evidence drawn out of a sentence-level pick.
const STRETCH_WEIGHT: f64 = 0.5;

/// Something a commit teaches.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Lesson {
    pub reading: String,
    pub text: String,
    /// The text before it, for its context.
    pub preceding: String,
    pub evidence: Evidence,
    pub weight: f64,
}

impl Lesson {
    fn new(reading: &str, text: &str, preceding: &str, evidence: Evidence) -> Self {
        Self {
            reading: reading.to_string(),
            text: text.to_string(),
            preceding: preceding.to_string(),
            evidence,
            weight: 1.0,
        }
    }
}

/// The one stretch of reading where two cuts of the same reading differ.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stretch {
    pub reading: String,
    /// Its text in the first cut and in the second.
    pub texts: (String, String),
    /// The text before it, the same in both.
    pub before: String,
}

/// Where `a` and `b`, two cuts of one reading, differ, when that is one
/// stretch between edges they share and not all of it.
pub(super) fn single_difference(a: &[Piece], b: &[Piece]) -> Option<Stretch> {
    let (mut i, mut j) = (0, 0);
    let mut before = String::new();
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
                reading: group.0.reading,
                texts: (group.0.text.clone(), group.1),
                before: before.clone(),
            });
        }
        before.push_str(&group.0.text);
    }
    found.filter(|_| stretches > 1)
}

/// How many chars `text` ends in that are hiragana and the end of `reading`
/// itself, short of all of `text`.
fn kana_tail(reading: &str, text: &str) -> usize {
    let matching = reading
        .chars()
        .rev()
        .zip(text.chars().rev())
        .take_while(|&(r, t)| r == t && matches!(t, '\u{3041}'..='\u{3096}'))
        .count();
    matching.min(text.chars().count().saturating_sub(1))
}

/// `s` without its last `n` chars.
fn drop_last(s: &str, n: usize) -> String {
    let count = s.chars().count();
    s.chars().take(count - n.min(count)).collect()
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

    /// Record `lessons`. Nothing in emoji mode: the buffer is a `:query`,
    /// not a kana reading.
    pub(super) fn learn(&mut self, lessons: &[Lesson]) {
        if self.mode.current() == InputMode::Emoji {
            return;
        }
        if let Some(cache) = &mut self.learning {
            for l in lessons {
                cache.record(&l.reading, &l.text, &l.preceding, l.evidence, l.weight);
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

    /// `text` for `reading` was committed where `shown` was the default:
    /// `confirmed` when it is the default, else `chosen` for it and
    /// `displaced` for the default. When both read the same and differ in
    /// one stretch of it, that stretch is taught the same at half weight,
    /// so a pick made over a sentence teaches the word it changed.
    pub(super) fn choice_lessons(
        &mut self,
        shown: &Shown,
        reading: &str,
        text: &str,
        preceding: &str,
    ) -> Vec<Lesson> {
        if text == shown.text {
            return vec![Lesson::new(reading, text, preceding, Evidence::Confirmed)];
        }
        let mut lessons = vec![
            Lesson::new(reading, text, preceding, Evidence::Chosen),
            Lesson::new(&shown.reading, &shown.text, preceding, Evidence::Displaced),
        ];
        if shown.reading == reading {
            let default = self.align(reading, &shown.text);
            let picked = self.align(reading, text);
            if let Some(stretch) = single_difference(&default, &picked) {
                let before = format!("{preceding}{}", stretch.before);
                for (text, evidence) in [
                    (&stretch.texts.1, Evidence::Chosen),
                    (&stretch.texts.0, Evidence::Displaced),
                ] {
                    lessons.push(Lesson {
                        weight: STRETCH_WEIGHT,
                        ..Lesson::new(&stretch.reading, text, &before, evidence)
                    });
                }
            }
        }
        lessons
    }

    /// The model's `converted` for a chunk's `reading` after `lctx`, with
    /// what the history prefers put in its place: the whole chunk when the
    /// history has its reading, else runs of its segments whose reading it
    /// has, longest first. Only whole segments, so a word is never cut
    /// into; and nothing after a replacement is converted again.
    pub(super) fn rewrite_from_history(
        &mut self,
        reading: &str,
        converted: String,
        lctx: &str,
    ) -> (String, Vec<Rewrite>) {
        if self.mode.current() != InputMode::Hiragana {
            return (converted, Vec::new());
        }
        let Some(cache) = &self.learning else {
            return (converted, Vec::new());
        };
        // Most chunks have nothing learned in them: found without cutting.
        let chars: Vec<char> = reading.chars().collect();
        let learned_inside = (0..chars.len()).any(|i| {
            (i + 2..=chars.len())
                .any(|j| cache.contains_reading(&chars[i..j].iter().collect::<String>()))
        });
        if !learned_inside {
            return (converted, Vec::new());
        }
        if let Some(text) = self.preferred(reading, &converted, lctx) {
            let rewrite = Rewrite {
                reading: reading.to_string(),
                text: text.clone(),
                preceding: lctx.to_string(),
            };
            return (text, vec![rewrite]);
        }

        let pieces = self.align(reading, &converted);
        let mut out = String::new();
        let mut rewrites = Vec::new();
        let mut i = 0;
        while i < pieces.len() {
            let before = format!("{lctx}{out}");
            let found = (i + 1..=pieces.len())
                .rev()
                .filter(|&j| j - i < pieces.len())
                .find_map(|j| {
                    let run = &pieces[i..j];
                    let reading: String = run.iter().map(|p| p.reading.as_str()).collect();
                    let text: String = run.iter().map(|p| p.text.as_str()).collect();
                    // A segment carries its particle (アンソロピックの), and
                    // a word is learned without one: the kana the text ends
                    // in are the reading's own, so the word ends before them.
                    let tail = kana_tail(&reading, &text);
                    std::iter::once(0)
                        .chain((tail > 0).then_some(tail))
                        .find_map(|tail| {
                            let reading: String = drop_last(&reading, tail);
                            let text: String = drop_last(&text, tail);
                            let preferred = self.preferred(&reading, &text, &before)?;
                            Some((j, reading, preferred, tail))
                        })
                });
            match found {
                Some((j, reading, text, tail)) => {
                    out.push_str(&text);
                    let kana: String = pieces[j - 1].text.chars().rev().take(tail).collect();
                    out.extend(kana.chars().rev());
                    rewrites.push(Rewrite {
                        reading,
                        text,
                        preceding: before,
                    });
                    i = j;
                }
                None => {
                    out.push_str(&pieces[i].text);
                    i += 1;
                }
            }
        }
        if rewrites.is_empty() {
            return (converted, rewrites);
        }
        (out, rewrites)
    }

    /// What the history prefers to `text` for `reading` after `preceding`:
    /// its best surface, when that outscores `text` by
    /// [`REWRITE_MARGIN`]. Never for a single char of reading, where one
    /// kana is too little to tell words apart by.
    fn preferred(&self, reading: &str, text: &str, preceding: &str) -> Option<String> {
        let cache = self.learning.as_ref()?;
        if reading.chars().count() < 2 || !cache.contains_reading(reading) {
            return None;
        }
        let text = self.settle_text(text);
        let (best, score) = cache.lookup(reading, preceding).into_iter().next()?;
        (best != text && score > cache.score(reading, &text, preceding) + REWRITE_MARGIN)
            .then_some(best)
    }

    /// The text before what is being converted, for the history's
    /// contexts: the segments before the focused one when split.
    pub(super) fn learning_context(&self) -> String {
        if self.state.is_segmented() {
            self.segment_ctx(false)
        } else {
            self.truncate_context_for_api()
        }
    }

    /// Order the model's alternatives by what the history says of their
    /// pieces: each is cut against `reading` and the scores of its
    /// segments summed, each after the text before it. The head stays: it
    /// is what the live conversion showed.
    pub(super) fn rank_model_candidates(&mut self, reading: &str, candidates: &mut [String]) {
        if candidates.len() < 3 || self.learning.is_none() {
            return;
        }
        let ctx = self.learning_context();
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
        let Some(cache) = &self.learning else {
            return 0.0;
        };
        let mut before = preceding.to_string();
        let mut total = 0.0;
        for piece in pieces.iter() {
            total += cache.score(&piece.reading, &piece.text, &before);
            before.push_str(&piece.text);
        }
        total
    }

    /// The composition committed as shown: the whole, and each chunk the
    /// model converted, `accepted`.
    pub(super) fn live_lessons(&self, reading: &str, text: &str) -> Vec<Lesson> {
        let ctx = self.truncate_context_for_api();
        let mut lessons = vec![Lesson::new(reading, text, &ctx, Evidence::Accepted)];
        // Only when the live conversion is what was committed.
        let live = self.live_text();
        if live.is_empty() || !text.starts_with(&live) {
            return lessons;
        }
        for (i, chunk) in self.chunks.iter().enumerate() {
            if !chunk.reading.chars().any(chunk::is_japanese) {
                continue;
            }
            // The chunk itself, when it is not the whole already taught.
            if chunk.reading != reading {
                let converted = self.settle_text(&chunk.converted);
                let lctx = self.chunk_lctx(i);
                lessons.push(Lesson::new(
                    &chunk.reading,
                    &converted,
                    &lctx,
                    Evidence::Accepted,
                ));
            }
            // A rewritten stretch is accepted as what it was shown as: only
            // the weakest evidence, so a rewrite cannot feed itself.
            for r in chunk.rewrites.iter().filter(|r| r.reading != chunk.reading) {
                lessons.push(Lesson::new(
                    &r.reading,
                    &r.text,
                    &r.preceding,
                    Evidence::Accepted,
                ));
            }
        }
        lessons
    }

    /// A split conversion committed: each segment whose list was built
    /// against its own head (the text it was cut as), and the whole against
    /// what the conversion opened with. A segment never focused teaches
    /// nothing: its pairing with the reading is a guess.
    pub(super) fn segment_lessons(&mut self, segments: &[Segment]) -> Vec<Lesson> {
        let learnable = |c: &Candidate| c.source.is_none_or(|s| s.is_learnable());
        let ctx = self.truncate_context_for_api();
        let mut lessons = Vec::new();
        let mut before = ctx.clone();
        for segment in segments {
            let preceding = before.clone();
            before.push_str(segment.text());
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
                    &preceding,
                    Evidence::Confirmed,
                ));
                continue;
            }
            lessons.push(Lesson::new(r, &selected.text, &preceding, Evidence::Chosen));
            if learnable(head) {
                lessons.push(Lesson::new(r, &head.text, &preceding, Evidence::Displaced));
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
                lessons.push(Lesson::new(&reading, &text, &ctx, Evidence::Confirmed));
            } else {
                lessons.push(Lesson::new(&reading, &text, &ctx, Evidence::Chosen));
                lessons.push(Lesson::new(
                    &shown.reading,
                    &shown.text,
                    &ctx,
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
