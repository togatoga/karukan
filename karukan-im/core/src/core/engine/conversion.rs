//! Conversion state handling: building the mixed candidate list, key
//! handling, and commit. Model dispatch lives in the sibling `model`
//! module, the Ctrl+R source views in `filter`, live chunking in `chunk`.

use std::collections::HashSet;

use tracing::debug;

use super::filter::source_for_key;
use super::*;

/// Maximum number of learning candidates to show
const MAX_LEARNING_CANDIDATES: usize = 3;

/// Max predictive (prefix-extending) dictionary candidates in the
/// composing suggestion list. The conversion list is uncapped — the full
/// ranked set goes into the paged candidate window.
const MAX_PREDICTIVE_SUGGESTIONS: usize = 3;

/// Min typed characters before predictive dictionary lookup kicks in — a
/// single key would flood the list from a large dictionary
const MIN_PREDICTIVE_PREFIX_CHARS: usize = 2;

/// How the unresolved romaji tail constrains the predictive lookup.
enum TailConstraint {
    /// No tail: prediction is unconstrained
    Unconstrained,
    /// The tail can still become these kana: narrow to them (`d` → だ/で…)
    Narrow(Vec<String>),
    /// The tail can no longer become kana (`yk`): no reading extends it
    Dead,
}

/// Mozc-style width/script annotation for a pure-kana candidate, or `None`
/// if the text mixes scripts or contains kanji/punctuation. Used to label
/// `あ` / `ア` / `ｱ` candidates in the conversion list.
pub(super) fn width_annotation(text: &str) -> Option<&'static str> {
    if karukan_engine::is_pure_hiragana(text) {
        Some("[全]ひらがな")
    } else if karukan_engine::is_pure_full_katakana(text) {
        Some("[全]カタカナ")
    } else {
        None
    }
}

/// The composition split at the caret for a conversion, as
/// [`InputMethodEngine::split_composition_at_caret`] resolved it.
pub(super) struct ConversionRange {
    /// The reading to convert: up to the caret when the caret splits the
    /// composition, the whole composition otherwise.
    pub reading: String,
    /// Dictionary lookup base and the unresolved romaji tail narrowing it.
    pub base: String,
    pub pending: String,
    /// Whether the caret actually split the composition (the rest is in
    /// `conversion_tail`).
    pub split: bool,
}

/// Helper for building a deduplicated list of conversion candidates.
///
/// Two push paths exist: [`push`] dedups by text (skips duplicates), and
/// [`push_force`] always inserts (used for learning candidates that should
/// appear at the top even if a later source re-emits the same text).
struct CandidateBuilder {
    candidates: Vec<AnnotatedCandidate>,
    seen: HashSet<String>,
}

impl CandidateBuilder {
    fn new() -> Self {
        Self {
            candidates: Vec::new(),
            seen: HashSet::new(),
        }
    }

    /// Push a candidate if its text hasn't been seen yet.
    fn push(&mut self, ac: AnnotatedCandidate) {
        if self.seen.insert(ac.text.clone()) {
            self.candidates.push(ac);
        }
    }

    /// Push a candidate unconditionally, marking its text as seen so later
    /// dedup'd inserts skip it. Use only for sources that should win over
    /// duplicates from later steps (e.g. learning cache).
    fn push_force(&mut self, ac: AnnotatedCandidate) {
        self.seen.insert(ac.text.clone());
        self.candidates.push(ac);
    }

    fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    fn into_candidates(self) -> Vec<AnnotatedCandidate> {
        self.candidates
    }
}

impl InputMethodEngine {
    /// Resolve what a conversion starting now covers, and prepare the
    /// engine for it: stash everything right of the caret in
    /// `conversion_tail`, and shrink the composition to the converted
    /// range.
    ///
    /// Every path that enters the Conversion state from a composition goes
    /// through here, so Space and the source-filter keys (Ctrl+I / Ctrl+T /
    /// Ctrl+R) cover the same range: what the caret bounds.
    pub(super) fn split_composition_at_caret(&mut self) -> ConversionRange {
        // Resolve the reading without touching the composition: pending
        // romaji stays live so cancelling the conversion returns to an
        // editable buffer (けいおうd → Tab → Esc → `a` → けいおうだ)
        let full_reading = self.input_buf.settled_reading(&self.converters.romaji);
        let cursor = self.input_buf.settled_cursor(&self.converters.romaji);
        let total_len = full_reading.chars().count();

        // If cursor is in the middle, convert only up to cursor position;
        // the rest becomes the unconverted tail.
        let (reading, tail) = if cursor > 0 && cursor < total_len {
            let r: String = full_reading.chars().take(cursor).collect();
            let t: String = full_reading.chars().skip(cursor).collect();
            (r, Some(t))
        } else {
            (full_reading, None)
        };
        // The unresolved romaji tail keeps narrowing the predictive
        // dictionary lookup, so a suggestion visible while typing (わせd →
        // 早稲田) stays selectable in the conversion list. A cursor split has
        // no live tail of its own: the reading stops at the caret.
        let (base, pending) = if tail.is_some() {
            (reading.clone(), String::new())
        } else {
            (self.input_buf.reading(), self.input_buf.pending())
        };

        // A cursor split shrinks the composition to the converted range, so
        // the buffer has to be rebuilt from the reading; without one it is
        // left untouched and Escape comes back to it still editable.
        if tail.is_some() {
            self.input_buf.set_text(&reading);
        }
        let split = tail.is_some();
        self.conversion_tail = tail;

        ConversionRange {
            reading,
            base,
            pending,
            split,
        }
    }

    /// Start kanji conversion for the current input buffer.
    ///
    /// Resolves the reading, runs `build_conversion_candidates`, and
    /// transitions into the Conversion state. The previous live-conversion
    /// result is preserved as the first model candidate so the user sees the
    /// same text they had been looking at during input.
    pub(super) fn start_conversion(&mut self, lookup: LearningLookup) -> EngineResult {
        let ConversionRange {
            reading,
            base,
            pending,
            split,
        } = self.split_composition_at_caret();

        // Predictive candidates only make sense at the end of the buffer:
        // anywhere else the conversion is bounded by the cursor, and a
        // prefix match's surface would duplicate the tail's characters on
        // commit (e.g. `あい|さつ` converting to `挨拶` → `挨拶さつ`).
        let lookup = if lookup == LearningLookup::Use && split {
            LearningLookup::Exact
        } else {
            lookup
        };

        // Snapshot the live-conversion text before clearing it, so the
        // displayed candidate survives even if re-inference diverges.
        let prev_suggest_text = self.live_text_with_pending();
        self.live.shown = false;

        if reading.is_empty() {
            return EngineResult::consumed();
        }

        // Get candidates from kanji converter (use full num_candidates for explicit conversion)
        let mut candidates = self.build_conversion_candidates(
            &reading,
            &base,
            &pending,
            self.config.num_candidates,
            lookup,
        );

        let seen: HashSet<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
        if !prev_suggest_text.is_empty()
            && prev_suggest_text != reading
            && !seen.contains(prev_suggest_text.as_str())
        {
            candidates.insert(
                0,
                AnnotatedCandidate::new(prev_suggest_text, CandidateSource::Model),
            );
        }

        if candidates.is_empty() {
            // No candidates: stay composing, untouched (emoji queries with
            // no match land here)
            let preedit = self.set_composing_state();
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        let candidate_list = self.to_conversion_candidate_list(candidates, &reading);
        self.enter_conversion_state(&reading, candidate_list)
    }

    /// Map builder output to the public [`CandidateList`] shown in the
    /// conversion window, settled at the configured width.
    fn to_conversion_candidate_list(
        &self,
        candidates: Vec<AnnotatedCandidate>,
        reading: &str,
    ) -> CandidateList {
        self.settle_candidates(
            candidates
                .into_iter()
                .map(|ac| ac.into_candidate(reading))
                .collect(),
        )
    }

    /// Build the Conversion-state preedit: confirmed segments (underlined),
    /// the currently selected candidate (highlighted), already-converted
    /// upcoming segments (underlined), then the unconverted tail
    /// (underlined) if any. Caret sits right after the highlighted segment.
    pub(super) fn build_conversion_preedit(&self, selected_text: &str) -> Preedit {
        let mut segments: Vec<PreeditSegment> = self
            .confirmed_segments
            .iter()
            .map(|seg| PreeditSegment::underlined(&seg.text))
            .collect();
        let confirmed_len: usize = segments.iter().map(|s| s.text.chars().count()).sum();

        segments.push(PreeditSegment::highlighted(selected_text));
        for seg in &self.upcoming_segments {
            segments.push(PreeditSegment::underlined(&seg.text));
        }
        if let Some(tail) = &self.conversion_tail {
            segments.push(PreeditSegment::underlined(tail));
        }
        let caret = confirmed_len + selected_text.chars().count();

        Preedit::from_segments(segments, caret)
    }

    /// Transition to Conversion state with the given reading and candidate list.
    ///
    /// Sets up the preedit (highlighted selected text + underlined tail if any),
    /// updates the state, and returns an EngineResult with preedit, candidates,
    /// and aux text actions.
    pub(super) fn enter_conversion_state(
        &mut self,
        reading: &str,
        candidates: CandidateList,
    ) -> EngineResult {
        let selected_text = candidates.selected_text().unwrap_or(reading).to_string();
        let preedit = self.build_conversion_preedit(&selected_text);

        self.state = InputState::Conversion {
            preedit: preedit.clone(),
            candidates: candidates.clone(),
            reading: reading.to_string(),
            // A fresh conversion always starts unfiltered
            filter: None,
        };

        // After the state assignment: the aux header reads the active filter.
        let aux = self.format_aux_conversion_with_page(reading, Some(&candidates));

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Dictionary candidates for a reading: user dict first, then system,
    /// exact matches then predictive (prefix-extending) ones, deduped.
    ///
    /// `pending` narrows the predictive lookup to readings the romaji tail
    /// can still become (わせ + `d` keeps わせだ…, drops わせり…);
    /// `predictive_limit` caps those results. `only` restricts search and
    /// dedup to one dictionary, so shared surfaces stay visible per view.
    pub(super) fn search_dictionaries(
        &self,
        reading: &str,
        pending: &str,
        limit: usize,
        predictive_limit: usize,
        min_prefix_chars: usize,
        only: Option<CandidateSource>,
    ) -> Vec<AnnotatedCandidate> {
        let dicts = [
            (self.dicts.user.as_ref(), CandidateSource::UserDictionary),
            (self.dicts.system.as_ref(), CandidateSource::Dictionary),
        ]
        .into_iter()
        .filter(|(_, source)| only.is_none_or(|o| o == *source))
        .collect::<Vec<_>>();
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        // Exact matches, user dictionary first — only when no romaji tail
        // is pending (an exact hit on the base would ignore the typed
        // tail). Candidates are sorted by score at build/load time
        for &(dict, source) in &dicts {
            if !pending.is_empty() {
                break;
            }
            let Some(result) = dict.and_then(|d| d.exact_match_search(reading)) else {
                continue;
            };
            for cand in result.candidates {
                if candidates.len() >= limit {
                    break;
                }
                if seen.insert(cand.surface.clone()) {
                    candidates.push(AnnotatedCandidate::new(cand.surface.clone(), source));
                }
            }
        }

        // Predictive: dictionary readings extending the typed prefix,
        // mirroring the learning cache's prefix lookup. The full reading
        // rides on the candidate so selecting it commits and records under
        // the right key.
        let constraint = self.tail_constraint(pending);
        if reading.chars().count() >= min_prefix_chars
            && !matches!(constraint, TailConstraint::Dead)
        {
            let mut budget = predictive_limit;
            for &(dict, source) in &dicts {
                if budget == 0 {
                    break;
                }
                let Some(dict) = dict else { continue };
                let matches = match &constraint {
                    TailConstraint::Unconstrained => dict.predictive_search(reading, budget),
                    TailConstraint::Narrow(expansions) => {
                        dict.predictive_search_expanded(reading, expansions, budget)
                    }
                    TailConstraint::Dead => unreachable!("checked above"),
                };
                for m in matches {
                    if budget == 0 || candidates.len() >= limit {
                        break;
                    }
                    if seen.insert(m.candidate.surface.clone()) {
                        budget -= 1;
                        candidates.push(
                            AnnotatedCandidate::new(m.candidate.surface.clone(), source)
                                .with_reading(Some(m.reading.to_string())),
                        );
                    }
                }
            }
        }

        candidates
    }

    /// Classify the unresolved romaji tail for predictive lookup.
    fn tail_constraint(&self, pending: &str) -> TailConstraint {
        if pending.is_empty() {
            return TailConstraint::Unconstrained;
        }
        let expansions = self.converters.romaji.pending_expansions(pending);
        if expansions.is_empty() {
            TailConstraint::Dead
        } else {
            TailConstraint::Narrow(expansions)
        }
    }

    /// Build the mixed candidate list, deduped in priority order:
    /// Learning → User Dictionary → Model → System Dictionary → Fallback.
    ///
    /// `base`/`pending` split the reading for the dictionary lookup: while
    /// a romaji tail is unresolved the predictive search stays narrowed to
    /// it (base わせ + `d` → わせだ…), with no tail they equal `reading`/"".
    ///
    /// `lookup` decides which learning candidates step (1) includes — see
    /// [`LearningLookup`]. Segment range navigation always passes
    /// [`LearningLookup::Exact`]: auto-selecting a longer prediction there
    /// would silently commit characters the user never typed.
    pub(super) fn build_conversion_candidates(
        &mut self,
        reading: &str,
        base: &str,
        pending: &str,
        num_candidates: usize,
        lookup: LearningLookup,
    ) -> Vec<AnnotatedCandidate> {
        // No converter (still loading in the background, or loading failed)
        // just means no model candidates: symbol-only and early keystrokes
        // still get dictionary/rewriter/fallback candidates. Loading here
        // synchronously would block the key-event thread on the download.
        let candidates = self.model_candidates(reading, num_candidates);

        let hiragana = reading.to_string();
        let katakana = karukan_engine::hiragana_to_katakana(reading);

        // Priority: Learning → User Dictionary → Model → System Dictionary → Fallback
        let mut builder = CandidateBuilder::new();

        // 1. Learning cache candidates (highest priority).
        //    Force-inserted so they win against duplicate text from later sources.
        let learning_candidates = match lookup {
            LearningLookup::Use => self.lookup_learning_candidates(reading),
            LearningLookup::Exact => self.lookup_learning_candidates_exact(reading),
            LearningLookup::Skip => Vec::new(),
        };
        for c in learning_candidates {
            // Exact matches have reading == input reading; use None to avoid redundancy
            let cand_reading = c.reading.filter(|r| r != reading);
            builder.push_force(
                AnnotatedCandidate::new(c.text, CandidateSource::Learning)
                    .with_reading(cand_reading),
            );
        }

        // 2. User dictionary candidates (system dictionary follows the model
        //    in step 4, so the two are split here).
        let (user_dict, system_dict): (Vec<_>, Vec<_>) = self
            .search_dictionaries(
                base,
                pending,
                usize::MAX,
                usize::MAX,
                MIN_PREDICTIVE_PREFIX_CHARS,
                None,
            )
            .into_iter()
            .partition(|ac| ac.source == CandidateSource::UserDictionary);
        for ac in user_dict {
            builder.push(ac);
        }

        // 3. Model inference results
        if candidates.is_empty() {
            // No literal fallback in emoji mode: `:smile` must not outrank
            // the 😄 surfaced by the rewriter step below.
            if builder.is_empty() && self.mode.current() != InputMode::Emoji {
                builder.push(AnnotatedCandidate::new(
                    hiragana.clone(),
                    CandidateSource::Fallback,
                ));
            }
        } else {
            for text in candidates {
                builder.push(AnnotatedCandidate::new(text, CandidateSource::Model));
            }
        }

        // 4. System dictionary candidates
        for ac in system_dict {
            builder.push(ac);
        }

        // 5/6. Hiragana/katakana fallback + rewriter variants. Emoji mode
        // shows rewriter (emoji) candidates only — no kana pair, like an
        // emoji picker; Enter in Composing still commits the literal query.
        if self.mode.current() != InputMode::Emoji {
            builder.push(AnnotatedCandidate::new(hiragana, CandidateSource::Fallback));
            builder.push(AnnotatedCandidate::new(katakana, CandidateSource::Fallback));
        }
        // Rewriters run on the typed reading only; running them on other
        // sources' candidates would emit variants nobody asked for.
        for (variant, description) in self.rewriter_variants(reading) {
            builder.push(
                AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                    .with_description(description),
            );
        }

        // 7. Back-fill descriptions. Symbol names are Fallback-only —
        //    model/dict/learning candidates must not inherit labels like
        //    「金 = 部首」 — while width annotations (`[全]カタカナ`) apply to
        //    any pure-kana candidate that still has none.
        for c in &mut builder.candidates {
            if c.description.is_some() {
                continue;
            }
            let symbol = (c.source == CandidateSource::Fallback)
                .then(|| karukan_engine::symbol_description(&c.text))
                .flatten();
            c.description = symbol
                .or_else(|| width_annotation(&c.text))
                .map(str::to_string);
        }

        builder.into_candidates()
    }

    /// Look up learning cache candidates for a reading (exact + prefix match, max 3).
    ///
    /// Returns candidates from the learning cache suitable for auto-suggest display.
    pub(super) fn lookup_learning_candidates(&self, reading: &str) -> Vec<Candidate> {
        self.lookup_learning(reading, "", MAX_LEARNING_CANDIDATES, true)
    }

    /// Like [`lookup_learning_candidates`] but restricted to exact matches
    /// (`reading` == the cached full reading).
    ///
    /// Used by segment range navigation (shrink/expand/advance/return): a
    /// predictive (prefix) match's surface corresponds to a *longer* reading
    /// than what's currently selected, so auto-selecting it as the default
    /// candidate would silently commit characters the user never typed when
    /// the segment is confirmed.
    pub(super) fn lookup_learning_candidates_exact(&self, reading: &str) -> Vec<Candidate> {
        self.lookup_learning(reading, "", MAX_LEARNING_CANDIDATES, false)
    }

    /// Full learning history for `reading` (exact + prefix, uncapped),
    /// narrowed by the unresolved romaji tail like the dictionary lookup —
    /// an exact hit on the base must not swallow the typed tail.
    pub(super) fn lookup_learning_history(&self, reading: &str, pending: &str) -> Vec<Candidate> {
        self.lookup_learning(reading, pending, usize::MAX, true)
    }

    fn lookup_learning(
        &self,
        reading: &str,
        pending: &str,
        max: usize,
        include_predictive: bool,
    ) -> Vec<Candidate> {
        let Some(cache) = &self.learning else {
            return vec![];
        };
        let constraint = self.tail_constraint(pending);
        if matches!(constraint, TailConstraint::Dead) {
            return vec![];
        }
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut seen = HashSet::new();

        // Exact match — only when no romaji tail is pending (an exact hit
        // on the base would ignore the typed tail)
        if pending.is_empty() {
            for (surface, _score) in cache.lookup(reading) {
                if candidates.len() >= max {
                    break;
                }
                if seen.insert(surface.clone()) {
                    candidates.push(Candidate {
                        text: surface,
                        reading: Some(reading.to_string()),
                        source: Some(CandidateSource::Learning),
                        description: None,
                    });
                }
            }
        }

        if !include_predictive {
            return candidates;
        }

        // Prefix match (predictive), narrowed to the kana the tail can
        // still become — mirrors the dictionary's expanded search
        for (full_reading, surface, _score) in cache.prefix_lookup(reading) {
            if candidates.len() >= max {
                break;
            }
            if full_reading == reading {
                continue;
            }
            if let TailConstraint::Narrow(expansions) = &constraint {
                let rest = full_reading.strip_prefix(reading).unwrap_or(&full_reading);
                if !expansions.iter().any(|e| rest.starts_with(e.as_str())) {
                    continue;
                }
            }
            if seen.insert(surface.clone()) {
                candidates.push(Candidate {
                    text: surface,
                    reading: Some(full_reading),
                    source: Some(CandidateSource::Learning),
                    description: None,
                });
            }
        }

        candidates
    }

    /// Dictionary candidates for the composing suggestion list (one page).
    pub(super) fn lookup_dict_candidates(&self, reading: &str) -> Vec<Candidate> {
        let pending = self.input_buf.pending();
        self.search_dictionaries(
            reading,
            &pending,
            CandidateList::DEFAULT_PAGE_SIZE,
            MAX_PREDICTIVE_SUGGESTIONS,
            MIN_PREDICTIVE_PREFIX_CHARS,
            None,
        )
        .into_iter()
        .map(|ac| ac.into_candidate(reading))
        .collect()
    }

    /// Rewriter variants for `reading`, as `(text, description)` pairs.
    ///
    /// In emoji mode only the emoji rewriter runs: `:smile` is a query, and
    /// another rewriter's width variant (`：ｓｍｉｌｅ`) would head the
    /// picker and be what Enter commits.
    pub(super) fn rewriter_variants(&self, reading: &str) -> Vec<RewriteOutput> {
        if self.mode.current() == InputMode::Emoji {
            return EmojiRewriter.rewrite(reading);
        }
        self.converters
            .rewriters
            .rewrite_all(&[reading.to_string()])
    }

    /// Build rule-based rewriter variants for the reading itself (e.g. for
    /// symbol input `「` → `『`, `【`, `（`, ...). Used in the auto-suggest path
    /// so users see mozc-style symbol variants without pressing Space first.
    pub(super) fn lookup_rewriter_variants(&self, reading: &str) -> Vec<Candidate> {
        self.rewriter_variants(reading)
            .into_iter()
            .map(|(text, description)| Candidate {
                text,
                reading: Some(reading.to_string()),
                source: Some(CandidateSource::Rewriter),
                description,
            })
            .collect()
    }

    pub(super) fn process_key_conversion(
        &mut self,
        key: &KeyEvent,
        shift_active: bool,
    ) -> EngineResult {
        // Alt chords pass through before any binding matches: Alt+Tab must
        // navigate and Alt+Return must not commit.
        if key.modifiers.alt_key {
            return EngineResult::not_consumed();
        }
        match key.keysym {
            Keysym::RETURN => self.commit_conversion(),
            Keysym::ESCAPE => self.cancel_conversion(),
            // Tab stays next-candidate and Shift+Tab (ISO_Left_Tab on
            // X11) prev-candidate for mozc-compatible muscle memory.
            Keysym::ISO_LEFT_TAB => self.prev_candidate(),
            Keysym::TAB if key.modifiers.shift_key => self.prev_candidate(),
            Keysym::SPACE | Keysym::DOWN | Keysym::TAB => self.next_candidate(),
            Keysym::UP => self.prev_candidate(),
            Keysym::PAGE_DOWN => self.next_candidate_page(),
            Keysym::PAGE_UP => self.prev_candidate_page(),
            // Ctrl+Backspace / Ctrl+Delete: delete the selected learning
            // candidate (the Mac "delete" key is Backspace). A non-learning
            // selection consumes the chord as a no-op.
            Keysym::DELETE | Keysym::BACKSPACE if key.modifiers.control_key => {
                if self.selected_is_deletable() {
                    self.delete_selected_candidate_from_history()
                } else {
                    EngineResult::consumed()
                }
            }
            // Inside a narrowed view Backspace shrinks the reading and
            // stays in the view — the mirror of typing-refine, so the list
            // re-expands as the query shrinks. Without a filter it returns
            // to the composition as before.
            Keysym::BACKSPACE if self.state.filter().is_some() => {
                self.refine_through_composing(key, shift_active)
            }
            // Backspace cancels back to the composition, like Escape.
            Keysym::BACKSPACE => self.cancel_conversion(),
            Keysym::LEFT if key.modifiers.shift_key => self.shrink_conversion_range(),
            Keysym::RIGHT if key.modifiers.shift_key => self.expand_conversion_range(),
            // The arrow keys walk the segments (fork-only segment
            // navigation); Home/End are the way back to caret editing, the
            // same way a caret move ends the live-conversion display while
            // composing. Delegated to the composing handler so the two
            // states cannot drift apart.
            Keysym::RIGHT => self.advance_to_next_segment(),
            Keysym::LEFT => self.return_to_prev_segment(),
            Keysym::HOME | Keysym::END => {
                self.in_composing(false, |e| e.process_key_composing(key, shift_active))
            }
            _ => {
                // Ctrl+N / Ctrl+P: emacs-style candidate navigation
                if key.modifiers.control_key {
                    match key.keysym {
                        Keysym::KEY_N | Keysym::KEY_N_UPPER => return self.next_candidate(),
                        Keysym::KEY_P | Keysym::KEY_P_UPPER => return self.prev_candidate(),
                        // Ctrl+R / Ctrl+T: cycle the source filter. Both
                        // keysym cases — some environments fold Shift into
                        // an uppercase keysym; direction must not change.
                        Keysym::KEY_R | Keysym::KEY_R_UPPER => {
                            return self.cycle_candidate_filter(FilterDirection::Backward);
                        }
                        Keysym::KEY_T | Keysym::KEY_T_UPPER => {
                            return self.cycle_candidate_filter(FilterDirection::Forward);
                        }
                        // Ctrl+J: split at the caret and rebuild, so the
                        // alternatives cover only the text after the break.
                        Keysym::KEY_J | Keysym::KEY_J_UPPER => {
                            return self.rebreak_conversion();
                        }
                        // Ctrl+A/B/E/F: the same caret moves as while
                        // composing, dropping back to editing like the
                        // arrow keys above.
                        Keysym::KEY_A
                        | Keysym::KEY_A_UPPER
                        | Keysym::KEY_B
                        | Keysym::KEY_B_UPPER
                        | Keysym::KEY_E
                        | Keysym::KEY_E_UPPER
                        | Keysym::KEY_F
                        | Keysym::KEY_F_UPPER => {
                            return self.in_composing(false, |e| {
                                e.process_key_composing(key, shift_active)
                            });
                        }
                        _ => {}
                    }

                    // Ctrl+Y/U/I/O: jump straight to one source's view.
                    if let Some(source) = source_for_key(key.keysym) {
                        return self.jump_to_source(source);
                    }

                    // Ctrl+1..9: select and commit that candidate. Bare
                    // digits refine below like any printable character, so
                    // typing numbers never conflicts with selection.
                    if let Some(digit) = key.keysym.digit_value() {
                        return self.select_shown_candidate(digit);
                    }
                }

                // A printable character refines instead of committing:
                // the reading grows and the suggestion rewrites in place,
                // keeping any active source filter.
                if key.to_char().is_some() && !key.modifiers.control_key {
                    return self.refine_through_composing(key, shift_active);
                }

                // Everything else is consumed as a no-op — leaked chords
                // would let the app act on them mid-conversion (e.g. a
                // browser reloading on Ctrl+R).
                EngineResult::consumed()
            }
        }
    }

    /// Feed a refining keystroke (printable char, Backspace) through the
    /// composing path, then re-enter the conversion with the previous
    /// source filter if one was active. With a filter the composing render
    /// is discarded, so its auto-suggest inference is suppressed.
    fn refine_through_composing(&mut self, key: &KeyEvent, shift_active: bool) -> EngineResult {
        let filter = self.state.filter();
        let result = self.in_composing(filter.is_some(), |engine| {
            engine.process_key_composing(key, shift_active)
        });
        if let Some(source) = filter
            && matches!(self.state, InputState::Composing { .. })
        {
            return self.start_conversion_with_filter(source);
        }
        result
    }

    /// Insert a chunk break at the caret without leaving the conversion.
    /// The span is the last chunk, so breaking narrows what the beam
    /// covers; the rebuilt list keeps the active source filter. The
    /// intermediate composing render is discarded, so its auto-suggest
    /// inference is suppressed.
    fn rebreak_conversion(&mut self) -> EngineResult {
        let filter = self.state.filter();
        self.in_composing(true, |engine| engine.insert_chunk_break());
        match filter {
            Some(source) => self.start_conversion_with_filter(source),
            None => self.start_conversion(LearningLookup::Use),
        }
    }

    /// Drop back to the untouched composition and run `edit` there. Set
    /// `discard_render` when the caller rebuilds the conversion afterwards:
    /// the composing render is thrown away, so its auto-suggest inference
    /// would be pure waste. The flag lives and dies inside this call, so no
    /// other path can inherit it.
    fn in_composing<R>(&mut self, discard_render: bool, edit: impl FnOnce(&mut Self) -> R) -> R {
        self.restore_composition_from_segments();
        self.set_composing_state();
        self.suppress_suggest = discard_render;
        let out = edit(self);
        self.suppress_suggest = false;
        out
    }

    /// Get selected text and reading from conversion state, or None if not in conversion
    pub(super) fn selected_conversion_info(&self) -> Option<(String, Option<String>)> {
        match &self.state {
            InputState::Conversion {
                candidates,
                reading,
                ..
            } => {
                // An empty (source-filtered) view displays the raw reading
                // as its preedit, so that is what committing produces —
                // never an empty commit that would eat the composition.
                let text = candidates.selected_text().unwrap_or(reading).to_string();
                let reading = candidates.selected().and_then(|c| c.reading.clone());
                Some((text, reading))
            }
            _ => None,
        }
    }

    /// Record a selection in the learning cache. No-op in emoji mode — the
    /// buffer is a `:query`, not a kana reading, and would corrupt the
    /// kana-keyed cache.
    pub(super) fn record_learning(&mut self, reading: &str, surface: &str) {
        if self.mode.current() == InputMode::Emoji {
            return;
        }
        if let Some(cache) = &mut self.learning {
            cache.record(reading, surface);
        }
    }

    /// Record learning for all `confirmed_segments` and `upcoming_segments`
    /// (from segment navigation), then build and return the full commit
    /// text — confirmed segments, `current_text`, then upcoming segments,
    /// concatenated in order — clearing both segment lists in the process.
    ///
    /// Skips recording when in Emoji mode: shortcode readings (e.g.
    /// `:smile`) aren't hiragana keys and would corrupt the kana-keyed
    /// learning cache.
    fn finalize_confirmed_segments(&mut self, current_text: &str) -> String {
        if self.mode.current() != InputMode::Emoji {
            let pairs: Vec<_> = self
                .confirmed_segments
                .iter()
                .chain(self.upcoming_segments.iter())
                .map(|seg| (seg.reading.clone(), seg.text.clone()))
                .collect();
            for (r, t) in &pairs {
                self.record_learning(r, t);
            }
        }

        let mut commit_text = String::new();
        for seg in &self.confirmed_segments {
            commit_text.push_str(&seg.text);
        }
        commit_text.push_str(current_text);
        for seg in &self.upcoming_segments {
            commit_text.push_str(&seg.text);
        }
        self.confirmed_segments.clear();
        self.upcoming_segments.clear();
        commit_text
    }

    /// Commit the current conversion (and any confirmed segments).
    fn commit_conversion(&mut self) -> EngineResult {
        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        if text.is_empty() && self.confirmed_segments.is_empty() {
            return EngineResult::consumed();
        }

        // A tail (unconverted portion) means the session isn't over: commit
        // the converted part and return to Composing with the tail.
        if let Some(tail) = self.conversion_tail.take() {
            return self.commit_and_resume_tail(&text, &reading, tail);
        }

        let commit_text = self.finish_conversion(&text, &reading);

        EngineResult::consumed()
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
            .with_action(EngineAction::Commit(commit_text))
    }

    /// Record learning for the selected text and any confirmed/upcoming
    /// segments, build and return the full commit text, and reset to the
    /// Empty state.
    ///
    /// Skips learning when the buffer is a `:shortcode` query — the
    /// reading would be e.g. `:smile`, which isn't a hiragana key
    /// and would corrupt the kana-keyed learning cache.
    pub(super) fn finish_conversion(&mut self, text: &str, reading: &Option<String>) -> String {
        if self.mode.current() != InputMode::Emoji
            && let Some(reading) = reading
        {
            self.record_learning(reading, text);
        }
        let commit_text = self.finalize_confirmed_segments(text);

        // Teardown lives in one place so nothing added to the composition
        // (chunk breaks, shown suggestions, live display) is forgotten here.
        self.end_composition();
        commit_text
    }

    /// Commit the converted segments and re-enter Composing with the
    /// unconverted `tail` reading (partial conversion via cursor split or
    /// range shrink). Learning is recorded like a normal finish, but the
    /// engine resumes a composing session instead of resetting to Empty.
    fn commit_and_resume_tail(
        &mut self,
        text: &str,
        reading: &Option<String>,
        tail: String,
    ) -> EngineResult {
        if self.mode.current() != InputMode::Emoji
            && let Some(reading) = reading
        {
            self.record_learning(reading, text);
        }
        let commit_text = self.finalize_confirmed_segments(text);

        self.state = InputState::Empty;
        // The tail is resumed from its reading alone: the keystrokes that
        // produced it were consumed by the conversion that just committed.
        self.input_buf.set_text(&tail);
        self.live.shown = false;
        self.chunks.clear();

        let preedit = self.set_composing_state();
        let mut result = EngineResult::consumed()
            .with_action(EngineAction::Commit(commit_text))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()));

        let refresh = self.refresh_input_state();
        result.actions.extend(refresh.actions);
        result
    }

    /// Whether the selected candidate can be removed from the learning
    /// history. False when nothing is selected, so the delete chord stays
    /// inert outside the case it is meant for.
    fn selected_is_deletable(&self) -> bool {
        self.state
            .candidates()
            .and_then(|c| c.selected())
            .is_some_and(Candidate::is_deletable)
    }

    /// Delete the selected learning candidate from the history
    /// (Ctrl+Backspace / Ctrl+Delete); the caller guards deletability
    /// ([`Self::selected_is_deletable`]).
    ///
    /// Removes the entry and its prefix twins
    /// ([`LearningCache::remove_suggestion`]), then rebuilds the conversion
    /// rather than dropping the row in place: dedup hid any
    /// model/dictionary/fallback copy of the same surface behind the learning
    /// entry, and only a rebuild brings it back.
    fn delete_selected_candidate_from_history(&mut self) -> EngineResult {
        let Some(surface) = self
            .state
            .candidates()
            .and_then(|c| c.selected())
            .map(|c| c.text.clone())
        else {
            return EngineResult::consumed();
        };
        // Remove by the typed reading: every entry surfacing this row has
        // it as a prefix, so the row and its twins clear together.
        let Some(reading) = self.state.reading().map(str::to_string) else {
            return EngineResult::consumed();
        };
        let removed = self
            .learning
            .as_mut()
            .is_some_and(|cache| cache.remove_suggestion(&reading, &surface));
        if !removed {
            return EngineResult::consumed();
        }
        debug!("deleted learning entry: {} -> {}", reading, surface);

        // Keep the filter and cursor so consecutive deletes stay in the
        // narrowed view and chew through the list top-down.
        let prev_filter = self.state.filter();
        let prev_cursor = self.state.candidates().map(|c| c.cursor()).unwrap_or(0);

        let candidates = self.build_conversion_candidates(
            &reading,
            &reading,
            "",
            self.config.num_candidates,
            LearningLookup::Exact,
        );
        if candidates.is_empty() {
            return self.cancel_conversion();
        }
        let candidate_list = self.to_conversion_candidate_list(candidates, &reading);
        let mut result = self.enter_conversion_state(&reading, candidate_list);

        if let Some(source) = prev_filter {
            result = self.apply_candidate_filter(source);
        }
        if self.state.candidates().is_some_and(|c| !c.is_empty()) {
            return self.navigate_candidate(|c| {
                c.set_cursor(prev_cursor);
                true
            });
        }
        result
    }
    /// Piece the composition back together when a partial conversion split
    /// the reading across several places (confirmed/upcoming segments, an
    /// unconverted tail). Without a split the composition was never touched
    /// and is left exactly as it is, pending romaji still live.
    ///
    /// Every path that drops from Conversion back into the composition goes
    /// through here — cancelling, and the caret/refine keys that edit there —
    /// so confirmed text can never linger in a field nothing displays.
    fn restore_composition_from_segments(&mut self) {
        let split = !self.confirmed_segments.is_empty()
            || !self.upcoming_segments.is_empty()
            || self.conversion_tail.is_some();
        if !split {
            return;
        }

        let mut reading = String::new();
        for seg in &self.confirmed_segments {
            reading.push_str(&seg.reading);
        }
        self.confirmed_segments.clear();
        reading.push_str(&self.input_buf.display());
        for seg in &self.upcoming_segments {
            reading.push_str(&seg.reading);
        }
        self.upcoming_segments.clear();
        // The caret goes back where the conversion ended, not to the end of
        // the reassembled reading: the tail was never part of what was
        // being converted, and typing next must land at the boundary the
        // user was working at (あ|い stays あ|い, so `k` gives あk|い).
        let caret = reading.chars().count();
        if let Some(tail) = self.conversion_tail.take() {
            reading.push_str(&tail);
        }
        // The reading is all that survives a conversion, so the raw
        // keystrokes are gone from here on.
        self.input_buf.set_text(&reading);
        self.input_buf.set_cursor(caret);
    }

    pub(super) fn cancel_conversion(&mut self) -> EngineResult {
        if !matches!(self.state, InputState::Conversion { .. }) {
            return EngineResult::not_consumed();
        }
        self.restore_composition_from_segments();

        if self.input_buf.is_empty() {
            self.state = InputState::Empty;
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        // Nothing else to rebuild: the composition is either the one the
        // conversion started from (pending romaji still live) or the one
        // just reassembled from the segments above.
        let preedit = self.set_composing_state();

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::UpdateAuxText(self.format_aux_composing()))
    }

    /// Navigate candidates with the given operation, then update preedit
    fn navigate_candidate(&mut self, op: impl FnOnce(&mut CandidateList) -> bool) -> EngineResult {
        let (selected_text, candidates) = {
            let Some(candidates) = self.state.candidates_mut() else {
                return EngineResult::not_consumed();
            };
            // Nothing to navigate in an empty (source-filtered) view; keep
            // the reading preedit instead of blanking it.
            if candidates.is_empty() {
                return EngineResult::consumed();
            }
            op(candidates);
            let text = candidates.selected_text().unwrap_or("").to_string();
            (text, candidates.clone())
        };
        self.update_conversion_preedit(&selected_text, candidates)
    }

    /// Select next candidate
    pub(super) fn next_candidate(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::move_next)
    }

    /// Select previous candidate
    fn prev_candidate(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::move_prev)
    }

    /// Go to next candidate page
    fn next_candidate_page(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::next_page)
    }

    /// Go to previous candidate page
    fn prev_candidate_page(&mut self) -> EngineResult {
        self.navigate_candidate(CandidateList::prev_page)
    }

    /// Select and commit the candidate at `page_index` (0-based) within the
    /// current page, like pressing the digit key `page_index + 1`. Not
    /// consumed unless a candidate list is active (Conversion state).
    pub fn select_candidate_on_page(&mut self, page_index: usize) -> EngineResult {
        let start = std::time::Instant::now();
        self.metrics.conversion_ms = 0;
        let result = self.select_shown_candidate(page_index + 1);
        self.metrics.process_key_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Update preedit after candidate selection change
    fn update_conversion_preedit(
        &mut self,
        selected_text: &str,
        candidates: CandidateList,
    ) -> EngineResult {
        let preedit = self.build_conversion_preedit(selected_text);

        if let Some(p) = self.state.preedit_mut() {
            *p = preedit.clone();
        }

        let reading = candidates
            .selected()
            .and_then(|c| c.reading.clone())
            .unwrap_or_default();
        let aux = self.format_aux_conversion_with_page(&reading, Some(&candidates));

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Convert the given reading directly (without cursor-split or romaji flush).
    ///
    /// Used by segment navigation (Left/Right arrows) where `conversion_tail`
    /// is already set by the caller and must not be overwritten.
    fn convert_reading(&mut self, reading: &str) -> EngineResult {
        self.convert_reading_preselect(reading, None)
    }

    /// Like [`convert_reading`], but re-selects `preselect` as the default
    /// candidate when given. Used when re-entering a segment the user
    /// already converted (Left/Right navigation) so their previous choice
    /// survives the round trip; if the rebuilt candidate list no longer
    /// contains it, it is inserted at the top.
    fn convert_reading_preselect(
        &mut self,
        reading: &str,
        preselect: Option<&str>,
    ) -> EngineResult {
        self.input_buf.set_text(reading);
        self.input_buf.set_cursor(0);

        if reading.is_empty() {
            return EngineResult::consumed();
        }

        let mut candidates = self.build_conversion_candidates(
            reading,
            reading,
            "",
            self.config.num_candidates,
            LearningLookup::Exact,
        );

        if let Some(preferred) = preselect
            && !candidates.iter().any(|c| c.text == preferred)
        {
            candidates.insert(
                0,
                AnnotatedCandidate::new(preferred, CandidateSource::Model),
            );
        }

        if candidates.is_empty() {
            let preedit = Preedit::with_text_underlined(reading);
            self.state = InputState::Composing {
                preedit: preedit.clone(),
            };
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        let mut candidate_list = self.to_conversion_candidate_list(candidates, reading);
        if let Some(preferred) = preselect
            && let Some(idx) = candidate_list
                .candidates()
                .iter()
                .position(|c| c.text == preferred)
        {
            candidate_list.select(idx);
        }
        self.enter_conversion_state(reading, candidate_list)
    }

    /// Confirm current segment and move right (Right arrow): re-enter the
    /// next already-converted segment if one exists (created by a previous
    /// Left), otherwise start converting the raw tail.
    fn advance_to_next_segment(&mut self) -> EngineResult {
        let has_upcoming = !self.upcoming_segments.is_empty();
        let has_tail = self.conversion_tail.as_ref().is_some_and(|t| !t.is_empty());
        if !has_upcoming && !has_tail {
            return EngineResult::consumed();
        }

        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        let seg_reading = reading.unwrap_or_else(|| self.input_buf.display());
        self.confirmed_segments.push(ConvertedSegment {
            text,
            reading: seg_reading,
        });

        if has_upcoming {
            let next = self.upcoming_segments.remove(0);
            self.convert_reading_preselect(&next.reading, Some(&next.text))
        } else {
            let tail = self.conversion_tail.take().unwrap_or_default();
            self.convert_reading(&tail)
        }
    }

    /// Go back to the previous segment (Left arrow). The current segment's
    /// conversion is kept in `upcoming_segments` (not dissolved back to raw
    /// kana) so it survives the round trip.
    fn return_to_prev_segment(&mut self) -> EngineResult {
        let Some(prev) = self.confirmed_segments.pop() else {
            return EngineResult::consumed();
        };

        if let Some((text, reading)) = self.selected_conversion_info() {
            let seg_reading = reading.unwrap_or_else(|| self.input_buf.display());
            self.upcoming_segments.insert(
                0,
                ConvertedSegment {
                    text,
                    reading: seg_reading,
                },
            );
        }

        self.convert_reading_preselect(&prev.reading, Some(&prev.text))
    }

    /// Dissolve `upcoming_segments` back into the raw `conversion_tail`
    /// (readings only). Called before Shift+Left/Right range adjustments:
    /// moving the segment boundary invalidates the downstream conversions,
    /// so they revert to kana and get re-converted on demand.
    fn dissolve_upcoming_into_tail(&mut self) {
        if self.upcoming_segments.is_empty() {
            return;
        }
        let mut readings = String::new();
        for seg in &self.upcoming_segments {
            readings.push_str(&seg.reading);
        }
        self.upcoming_segments.clear();
        if let Some(tail) = &self.conversion_tail {
            readings.push_str(tail);
        }
        self.conversion_tail = Some(readings);
    }

    /// Shrink the conversion range by 1 character (Shift+Left).
    ///
    /// Moves the last character of the current reading into the front of
    /// `conversion_tail`, then re-runs conversion on the shortened reading.
    fn shrink_conversion_range(&mut self) -> EngineResult {
        let reading = self.input_buf.display();
        let char_count = reading.chars().count();
        if char_count <= 1 {
            return EngineResult::consumed();
        }
        self.dissolve_upcoming_into_tail();

        let new_reading: String = reading.chars().take(char_count - 1).collect();
        let moved_char: String = reading.chars().skip(char_count - 1).collect();

        let tail = self.conversion_tail.take().unwrap_or_default();
        self.conversion_tail = Some(format!("{}{}", moved_char, tail));
        self.convert_reading(&new_reading)
    }

    /// Expand the conversion range by 1 character (Shift+Right).
    ///
    /// Takes the first character from `conversion_tail` and appends it to the
    /// current reading, then re-runs conversion on the extended reading.
    fn expand_conversion_range(&mut self) -> EngineResult {
        self.dissolve_upcoming_into_tail();
        let tail = match self.conversion_tail.as_ref() {
            Some(t) if !t.is_empty() => t.clone(),
            _ => return EngineResult::consumed(),
        };

        let first_char: String = tail.chars().take(1).collect();
        let remaining: String = tail.chars().skip(1).collect();

        if remaining.is_empty() {
            self.conversion_tail = None;
        } else {
            self.conversion_tail = Some(remaining);
        }

        let new_reading = format!("{}{}", self.input_buf.display(), first_char);
        self.convert_reading(&new_reading)
    }
}
