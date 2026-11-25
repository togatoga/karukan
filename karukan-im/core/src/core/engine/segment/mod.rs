//! Segments (文節): converting a sentence a piece at a time.
//!
//! A conversion starts as one segment covering the whole reading, which is
//! everything it was before segments existed. The arrow keys split it:
//! the text on screen is cut where its segments are ([`split`]), ←/→ move
//! the focus between them, and Shift+←/→ move the focused one's right
//! edge. Each segment is a stretch of the reading with a candidate list of
//! its own, converted with the segments before it as its left context —
//! the same thing live conversion does with its chunks, at boundaries the
//! user can move.
//!
//! Nothing is converted to split: the pieces are cut out of the text
//! already shown, so entering segments never changes what is on screen. A
//! segment's own list is built when it is first focused, and what the
//! model gives for the segment alone joins the list behind the text it
//! already had.
mod split;

use split::{Piece, finest_cut, ranked_cuts, script_cut};

use crate::core::state::Segment;

use super::*;

/// Where the focus goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Focus {
    Left,
    Right,
    First,
    Last,
}

impl InputMethodEngine {
    /// Whether the arrow keys mean segments here. Direct input and the
    /// emoji picker hold one literal string, with nothing to split.
    pub(super) fn segments_apply(&self) -> bool {
        self.mode.current() == InputMode::Hiragana
    }

    /// ←/→/Home/End: move the focus to another segment, wrapping at the
    /// ends. A conversion opens whole, with the list for all of it; the
    /// first of these keys is what splits it, and from the whole → lands
    /// on the second segment and ← on the last, as if the first had been
    /// focused all along.
    pub(super) fn focus_segment(&mut self, to: Focus) -> EngineResult {
        let (segments, focus) = self.segments();
        let last = segments.len() - 1;
        if last == 0 {
            return EngineResult::consumed();
        }
        let next = match to {
            Focus::Left => focus.checked_sub(1).unwrap_or(last),
            Focus::Right if focus == last => 0,
            Focus::Right => focus + 1,
            Focus::First => 0,
            Focus::Last => last,
        };
        self.put_segments(segments, next);
        self.render_conversion()
    }

    /// Ctrl+S while composing: Space and Ctrl+S in one press. What is cut
    /// is the text that was on screen — the live conversion — not whatever
    /// heads Space's list: the history comes first there, and may be a
    /// different text, or a longer phrase predicted from what was typed,
    /// which would replace what the user was looking at and meant to fix.
    /// The history's rows are still in the list for the whole. A text that
    /// does not come apart is left as that plain conversion.
    pub(super) fn convert_and_split(&mut self) -> EngineResult {
        let shown = self.live_text_with_pending();
        let converted = self.start_conversion(LearningLookup::Use);
        if !matches!(self.state, InputState::Conversion { .. }) {
            return converted;
        }
        if let Some(list) = self.state.candidates_mut()
            && let Some(at) = list.candidates().iter().position(|c| c.text == shown)
        {
            list.set_cursor(at);
        }
        self.split_conversion();
        self.render_conversion()
    }

    /// Ctrl+S: cut the conversion into segments, the first one focused,
    /// and pressed again cut it the next way, round and round. There are
    /// three ways and no more: the dictionary's longest words
    /// ([`Self::cut`], what the arrow keys split into), the script alone
    /// ([`script_cut`]), which cuts where the dictionary has one long
    /// word, and the dictionary's shortest words ([`finest_cut`]), the
    /// extreme. A way that cuts as an earlier one does is left out. After
    /// the last way comes the whole again, one segment over everything,
    /// so the key is also the way back out of segments.
    ///
    /// What is cut is the text on screen, so choices already made in a
    /// segment carry over as text.
    pub(super) fn split_conversion(&mut self) -> EngineResult {
        let (segments, _) = self.segments();
        let whole = !self.state.is_segmented();
        let (reading, text) = match self.state.candidates().and_then(CandidateList::selected) {
            // A whole conversion that the arrow keys would not split
            // either (a command, a date) has nothing to cut.
            Some(c) if whole && !self.splits(c) => return EngineResult::consumed(),
            // A predicted phrase reads longer than what was typed.
            Some(c) if whole => (
                c.reading
                    .clone()
                    .unwrap_or_else(|| segments[0].reading.clone()),
                c.text.clone(),
            ),
            _ => (
                segments.iter().map(|s| s.reading.as_str()).collect(),
                segments.iter().map(Segment::text).collect(),
            ),
        };
        let candidates = {
            let lookup = self.word_lookup(&text);
            [
                ("辞書(最長)", self.cut(&reading, &text)),
                ("字種", script_cut(&reading, &text, &lookup)),
                ("辞書(最短)", finest_cut(&reading, &text, &lookup)),
            ]
        };
        let mut ways: Vec<(&'static str, Vec<Piece>)> = Vec::new();
        for (name, cut) in candidates {
            // A way that cuts nothing, or as an earlier one does, is none.
            if cut.len() > 1 && ways.iter().all(|(_, earlier)| *earlier != cut) {
                ways.push((name, cut));
            }
        }
        // From the whole, the first way; from one of them, the next, and
        // from the last the whole again; and the first from segments that
        // are none of them (one was resized).
        let shown = ways.iter().position(|(_, cut)| {
            !whole
                && cut
                    .iter()
                    .map(|p| p.reading.as_str())
                    .eq(segments.iter().map(|s| s.reading.as_str()))
        });
        let next = match shown {
            Some(at) if at + 1 == ways.len() => {
                // Back to one segment over everything, with what is on
                // screen at the head of its list.
                self.put_segments(vec![Segment::unbuilt(reading, Some(text))], 0);
                self.segment_cut = None;
                return self.render_conversion();
            }
            Some(at) => at + 1,
            None if ways.is_empty() => return EngineResult::consumed(),
            None => 0,
        };
        let (name, pieces) = ways[next].clone();
        self.put_segments(pieces.into_iter().map(Segment::from).collect(), 0);
        // Named only when there is another way to switch to.
        self.segment_cut = (ways.len() > 1).then_some(name);
        self.render_conversion()
    }

    /// `text`, the conversion of `reading`, cut into segments: the
    /// dictionary's longest match — the fewest segments, but two at least,
    /// so a text the dictionary has as one word is cut into its parts
    /// ([`ranked_cuts`]). One piece when nothing cuts it.
    fn cut(&self, reading: &str, text: &str) -> Vec<Piece> {
        let whole = || {
            vec![Piece {
                reading: reading.to_string(),
                text: text.to_string(),
            }]
        };
        ranked_cuts(reading, text, &self.word_lookup(text), 1)
            .pop()
            .unwrap_or_else(whole)
    }

    /// Shift+←/→: move the focused segment's right edge by one character of
    /// the reading. The focused segment is converted again from its new
    /// reading, and so is everything after it, which is then split afresh;
    /// the segments before it are left as they are.
    pub(super) fn resize_segment(&mut self, grow: bool) -> EngineResult {
        let (segments, focus) = self.segments();
        // The segments stop being any of the ways of cutting.
        self.segment_cut = None;
        let mut focused: Vec<char> = segments[focus].reading.chars().collect();
        let mut rest: Vec<char> = segments[focus + 1..]
            .iter()
            .flat_map(|s| s.reading.chars())
            .collect();
        if grow {
            if rest.is_empty() {
                return EngineResult::consumed();
            }
            focused.push(rest.remove(0));
        } else {
            if focused.len() <= 1 {
                return EngineResult::consumed();
            }
            rest.insert(0, focused.pop().expect("more than one char"));
        }

        let mut resized: Vec<Segment> = segments[..focus].to_vec();
        resized.push(Segment::unbuilt(focused.iter().collect::<String>(), None));
        // The rest rides along as kana while the focused segment is built,
        // so that list is built as a segment's (its reading fixed).
        if !rest.is_empty() {
            resized.push(Segment::unbuilt(rest.iter().collect::<String>(), None));
        }
        self.put_segments(resized, focus);

        if !rest.is_empty() {
            let pieces = self.convert_rest(&rest);
            if let InputState::Conversion { right, .. } = &mut self.state {
                *right = pieces.into_iter().map(Segment::from).collect();
            }
        }
        self.render_conversion()
    }

    /// Ctrl+digit or a click picked a candidate for the focused segment:
    /// show it and move on to the next segment, if there is one.
    pub(super) fn after_segment_pick(&mut self) -> EngineResult {
        if self.state.neighbours().1.is_empty() {
            return self.render_conversion();
        }
        self.focus_segment(Focus::Right)
    }

    /// Replace the focused segment's list (a transliteration, a rebuilt
    /// list) and show the result. The filter goes: the new list is not a
    /// source's view.
    pub(super) fn replace_focused_candidates(&mut self, list: CandidateList) -> EngineResult {
        if let InputState::Conversion {
            candidates, filter, ..
        } = &mut self.state
        {
            *candidates = list;
            *filter = None;
        }
        self.render_conversion()
    }

    /// The preedit of the conversion as it stands: every segment's text in
    /// order. Unsplit, the whole is highlighted as it always was; split,
    /// only the focused segment is, and the caret sits at its start so the
    /// candidate window follows the focus.
    pub(super) fn conversion_preedit(&self) -> Preedit {
        let focused = self
            .state
            .candidates()
            .and_then(CandidateList::selected_text)
            .or(self.state.reading())
            .unwrap_or_default();
        if !self.state.is_segmented() {
            return Preedit::with_text_highlighted(focused);
        }
        use crate::core::preedit::{AttributeType, PreeditSegment};
        let (left, right) = self.state.neighbours();
        let underlined = |s: &Segment| PreeditSegment::new(s.text(), AttributeType::Underline);
        let caret = left.iter().map(|s| s.text().chars().count()).sum();
        let segments = left
            .iter()
            .map(underlined)
            .chain([PreeditSegment::highlighted(focused)])
            .chain(right.iter().map(underlined))
            .collect();
        Preedit::from_segments(segments, caret)
    }

    /// The candidate list for a segment's `reading`, with `head` — the text
    /// the segment already shows — first, so focusing a segment never
    /// changes it. When the list has that text it is that row (and commits
    /// as what its source says); when it has not, the text stays as it was
    /// cut, a row that teaches nothing.
    pub(super) fn build_segment_list(
        &mut self,
        reading: &str,
        head: Option<&str>,
    ) -> CandidateList {
        let mut rows: Vec<Candidate> = self
            .build_conversion_candidates(
                reading,
                reading,
                "",
                self.config.num_candidates,
                LearningLookup::Use,
            )
            .into_iter()
            .map(|ac| ac.into_candidate(reading))
            .collect();
        if let Some(head) = head {
            let row = match rows.iter().position(|c| c.text == head) {
                Some(at) => rows.remove(at),
                None => Segment::unbuilt(reading, Some(head.to_string()))
                    .candidates
                    .selected()
                    .cloned()
                    .expect("built with a text"),
            };
            rows.insert(0, row);
        }
        self.settle_candidates(rows)
    }

    /// The conversion as a list of segments and the index of the focused
    /// one. A conversion not yet split is split here, from the text it
    /// shows; when that text does not come apart (one word, a row that is
    /// not this reading's conversion) it is the one segment it was.
    fn segments(&self) -> (Vec<Segment>, usize) {
        let InputState::Conversion {
            candidates,
            reading,
            filter,
            left,
            right,
            ..
        } = &self.state
        else {
            return (vec![Segment::unbuilt("", None)], 0);
        };
        let focused = Segment {
            reading: reading.clone(),
            candidates: candidates.clone(),
            filter: *filter,
            built: true,
        };
        if self.state.is_segmented() {
            let segments = left
                .iter()
                .cloned()
                .chain([focused])
                .chain(right.iter().cloned());
            return (segments.collect(), left.len());
        }

        // A row carries the reading it converts: a phrase predicted from
        // what was typed reads longer than that, and is cut against its
        // own reading.
        let pieces = match candidates.selected() {
            Some(c) if self.splits(c) => self.cut(c.reading.as_deref().unwrap_or(reading), &c.text),
            _ => Vec::new(),
        };
        if pieces.len() < 2 {
            return (vec![focused], 0);
        }
        (pieces.into_iter().map(Segment::from).collect(), 0)
    }

    /// Whether the selected row is its reading converted, and so can be
    /// cut against it: not a row whose text was never the reading's (a
    /// command, a date, the keystrokes).
    fn splits(&self, selected: &Candidate) -> bool {
        matches!(
            selected.source,
            None | Some(
                CandidateSource::Model
                    | CandidateSource::Learning
                    | CandidateSource::UserDictionary
                    | CandidateSource::Dictionary
                    | CandidateSource::Fallback
                    | CandidateSource::Rewriter
            )
        )
    }

    /// Install `segments` with the one at `focus` focused, building its
    /// list if this is the first time it is.
    fn put_segments(&mut self, mut segments: Vec<Segment>, focus: usize) {
        let right = segments.split_off(focus + 1);
        let focused = segments.pop().expect("focus is in range");
        self.state = InputState::Conversion {
            preedit: Preedit::new(),
            candidates: focused.candidates.clone(),
            reading: focused.reading.clone(),
            filter: focused.filter,
            left: segments,
            right,
        };
        if !focused.built {
            // After the state is in place: the list is built with the
            // segments before this one as its context.
            let head = focused.candidates.selected_text().map(str::to_string);
            let list = self.build_segment_list(&focused.reading, head.as_deref());
            if let Some(candidates) = self.state.candidates_mut() {
                *candidates = list;
            }
        }
    }

    /// Show the conversion as the state has it: preedit, the focused
    /// segment's candidates, and the aux line.
    fn render_conversion(&mut self) -> EngineResult {
        let preedit = self.conversion_preedit();
        if let Some(p) = self.state.preedit_mut() {
            *p = preedit.clone();
        }
        let (Some(candidates), Some(reading)) = (self.state.candidates(), self.state.reading())
        else {
            return EngineResult::consumed();
        };
        let aux = self.format_aux_conversion(reading, candidates);
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Convert what follows the focused segment and split it: the model's
    /// top answer on the chunk grid, with everything up to and including
    /// the focused segment as its left context.
    fn convert_rest(&mut self, rest: &[char]) -> Vec<Piece> {
        let reading: String = rest.iter().collect();
        // The composition's manual breaks are positions in the whole
        // reading; they mean nothing inside this stretch of it.
        let breaks = std::mem::take(&mut self.chunk_breaks);
        let ctx = self.segment_ctx(true);
        let converted = self.convert_on_chunk_grid(rest, &ctx);
        self.chunk_breaks = breaks;
        let text = self.settle_text(&converted);
        self.cut(&reading, &text)
    }

    /// The left context of a segment's conversion: the editor's text, then
    /// the segments before the focused one — and the focused one too, for
    /// what comes after it.
    pub(super) fn segment_ctx(&self, through_focused: bool) -> String {
        let mut before: String = self
            .state
            .neighbours()
            .0
            .iter()
            .map(Segment::text)
            .collect();
        if through_focused {
            before.push_str(self.conversion_preedit_focused());
        }
        self.lctx_for(&self.truncate_context_for_api(), &before)
    }

    /// The segments as one line with the focused one bracketed, for the
    /// aux line: 「今日は[医者に]行く」. The preedit marks the focus with
    /// its underline, which not every application draws (a terminal shows
    /// all of it alike); this line is drawn by the IME itself. Only the
    /// neighbours on either side are spelled out, so a long sentence does
    /// not widen the window.
    pub(super) fn segment_outline(&self) -> String {
        let (left, right) = self.state.neighbours();
        let before = match left {
            [] => String::new(),
            [only] => only.text().to_string(),
            [.., last] => format!("…{}", last.text()),
        };
        let after = match right {
            [] => String::new(),
            [only] => only.text().to_string(),
            [first, ..] => format!("{}…", first.text()),
        };
        format!("{before}[{}]{after}", self.conversion_preedit_focused())
    }

    /// The focused segment's text.
    fn conversion_preedit_focused(&self) -> &str {
        self.state
            .candidates()
            .and_then(CandidateList::selected_text)
            .or(self.state.reading())
            .unwrap_or_default()
    }

    /// The dictionary words that start a reading and are written in `text`
    /// — what [`ranked_cuts`] pins the text to the reading with. Both
    /// dictionaries; a word `text` does not contain cannot be one of its
    /// words, and dropping it here keeps the common readings (こう has
    /// hundreds of surfaces) from being copied for nothing.
    fn word_lookup<'a>(&'a self, text: &'a str) -> impl Fn(&str) -> Vec<(usize, String)> + 'a {
        move |rest: &str| {
            [self.dicts.user.as_ref(), self.dicts.system.as_ref()]
                .into_iter()
                .flatten()
                .flat_map(|dict| dict.common_prefix_search(rest))
                .flat_map(|hit| {
                    let taken = hit.reading.chars().count();
                    hit.candidates
                        .iter()
                        .filter(|c| text.contains(c.surface.as_str()))
                        .map(move |c| (taken, c.surface.clone()))
                })
                .collect()
        }
    }
}

impl From<Piece> for Segment {
    fn from(piece: Piece) -> Self {
        Segment::unbuilt(piece.reading, Some(piece.text))
    }
}
