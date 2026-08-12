//! Source-filtered candidate views (Ctrl+R / Ctrl+T).
//!
//! Each view queries one candidate source directly rather than filtering
//! the mixed list, which dedups shared texts into the highest-priority
//! source and would hide them from every lower one.

use super::conversion::width_annotation;
use super::*;

/// The source views Ctrl+T and Ctrl+R rotate through, grouped by what the
/// user is looking for: what they taught the IME (learning), what it has
/// looked up (both dictionaries, as one stop), what it guessed (the model),
/// and the mechanical rewrites. The model sits late on purpose — Ctrl+I
/// reaches it in one press from anywhere, so the cycle does not have to
/// keep it near the front.
///
/// Ctrl+R is the reverse one, matching readline's reverse-i-search and the
/// fact that R sits left of T.
///
/// The full list is not a stop: it is what Space already shows, and
/// Esc → Space returns to it. Fallback has no slot of its own — the plain
/// kana ride at the tail of the rewriter view, which sits last so Ctrl+R
/// reaches it in one press from the full list.
const FILTER_CYCLE: [CandidateSource; 4] = [
    CandidateSource::Learning,
    CandidateSource::Dictionary,
    CandidateSource::Model,
    CandidateSource::Rewriter,
];

/// Ctrl+I opens the AI view directly. It is the one view worth a key of
/// its own: the others are a step or two along the cycle and are opened
/// rarely, and every extra chord is one more thing to collide with.
pub(super) fn source_for_key(keysym: Keysym) -> Option<CandidateSource> {
    match keysym {
        Keysym::KEY_I | Keysym::KEY_I_UPPER => Some(CandidateSource::Model),
        _ => None,
    }
}

impl InputMethodEngine {
    /// Show `source`'s view directly, from either state: entering the
    /// conversion first when composing.
    pub(super) fn jump_to_source(&mut self, source: CandidateSource) -> EngineResult {
        match &self.state {
            InputState::Conversion { .. } => self.apply_candidate_filter(source),
            InputState::Composing { .. } => self.start_conversion_with_filter(source),
            InputState::Empty => EngineResult::not_consumed(),
        }
    }

    /// Ctrl+T / Ctrl+R: rotate to the next / previous view in
    /// [`FILTER_CYCLE`], exactly one step per press — an empty source shows
    /// 「候補なし」, never skipped, so the position stays predictable. The
    /// rotation never returns to the full list.
    pub(super) fn cycle_candidate_filter(&mut self, direction: FilterDirection) -> EngineResult {
        if !matches!(self.state, InputState::Conversion { .. }) {
            return EngineResult::not_consumed();
        }
        let current = self.state.filter();
        let len = FILTER_CYCLE.len();
        let forward = direction == FilterDirection::Forward;
        let pos = match current {
            None if forward => 0,
            None => len - 1,
            Some(source) => {
                let pos = FILTER_CYCLE.iter().position(|f| *f == source).unwrap_or(0);
                (if forward { pos + 1 } else { pos + len - 1 }) % len
            }
        };
        self.apply_candidate_filter(FILTER_CYCLE[pos])
    }

    /// Ctrl+T / Ctrl+R while composing: enter the Conversion state and
    /// immediately narrow it one step, so the filtered view opens without
    /// Space.
    pub(super) fn start_filtered_conversion(&mut self, direction: FilterDirection) -> EngineResult {
        if !self.enter_conversion_for_filter() {
            return EngineResult::consumed();
        }
        self.cycle_candidate_filter(direction)
    }

    /// Enter the Conversion state already narrowed to `source` — used when
    /// typing refines a narrowed view, so the view survives the keystroke.
    pub(super) fn start_conversion_with_filter(&mut self, source: CandidateSource) -> EngineResult {
        if !self.enter_conversion_for_filter() {
            return EngineResult::consumed();
        }
        self.apply_candidate_filter(source)
    }

    /// Enter the Conversion state as an empty shell for a filtered view —
    /// no mixed list is built (the view re-queries its source), so no model
    /// inference runs here. Returns false when there is nothing to convert.
    fn enter_conversion_for_filter(&mut self) -> bool {
        let reading = self.input_buf.settled_reading(&self.converters.romaji);
        if reading.is_empty() {
            return false;
        }
        // Left shown, the stale live chunks would survive the commit and
        // render as the next composition's preedit.
        self.live.shown = false;
        self.enter_conversion_state(&reading, CandidateList::new(Vec::new()));
        true
    }

    /// Set `filter` and rebuild the window from it. With no candidates the
    /// preedit falls back to the reading and the aux says 「候補なし」.
    pub(super) fn apply_candidate_filter(&mut self, next: CandidateSource) -> EngineResult {
        let Some(reading) = self.state.reading().map(str::to_string) else {
            return EngineResult::not_consumed();
        };
        let view = self.source_view(next, &reading);
        let list = self.settle_candidates(view);
        let selected = list.selected_text().unwrap_or(&reading).to_string();
        let preedit = Preedit::with_text_highlighted(&selected);
        // The aux leads with what the user typed, tail included — typing
        // refines the view in place, and the selected candidate's own
        // reading would otherwise be the only thing on the line, leaving no
        // sign of what is being typed. A predictive entry, whose reading
        // runs past the query, shows both: committing it records under the
        // longer one.
        let (base, pending) = self.live_query_split(&reading);
        let typed = format!("{base}{pending}");
        let aux_reading = match list.selected().and_then(|c| c.reading.as_deref()) {
            Some(full) if full != typed => format!("{typed} → {full}"),
            _ => typed,
        };
        if let InputState::Conversion {
            filter,
            candidates,
            preedit: state_preedit,
            ..
        } = &mut self.state
        {
            *filter = Some(next);
            *candidates = list.clone();
            *state_preedit = preedit.clone();
        }
        debug!("candidate filter → {:?}", next);
        // After the state assignment: the aux header reads the active filter.
        let aux = self.format_aux_conversion_with_page(&aux_reading, Some(&list));
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(list))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Candidates for the view narrowed to `source`. Each view queries its
    /// own source instead of filtering the mixed list — the list's dedup
    /// folds shared texts into the highest-priority source, which would
    /// hide them from every lower source's view.
    fn source_view(&mut self, source: CandidateSource, reading: &str) -> Vec<Candidate> {
        // Learning/dictionary views predict on the live base + romaji tail;
        // model and rewriter cannot consume a tail and use the settled
        // `reading` — the exact text Enter commits.
        let (base, pending) = self.live_query_split(reading);
        match source {
            CandidateSource::Learning => self.lookup_learning_history(&base, &pending),
            // One dictionary view over both dictionaries: usually the user
            // just wants to look the reading up, not to pick which book it
            // comes from. `search_dictionaries` puts their own entries first
            // and dedups by surface, and each candidate still carries the
            // dictionary it came from, so 👤 and 📚 stay distinguishable in
            // the aux without a stop of their own.
            //
            // A paged dictionary browser wants everything: uncapped, and
            // predictive from the first char (no flood guard).
            CandidateSource::UserDictionary | CandidateSource::Dictionary => self
                .search_dictionaries(&base, &pending, usize::MAX, usize::MAX, 1, None)
                .into_iter()
                .map(|ac| ac.into_candidate(&base))
                .collect(),
            CandidateSource::Model => self.model_source_view(reading),
            // Rewriter variants regenerate from the reading; the plain kana
            // pair rides at the tail (lowest priority).
            CandidateSource::Rewriter => {
                let mut view = self.lookup_rewriter_variants(reading);
                // Emoji mode shows emojis and nothing else — no literal
                // `:query` pair at the tail.
                if self.mode.current() == InputMode::Emoji {
                    return view;
                }
                let mut kana = vec![reading.to_string()];
                let katakana = karukan_engine::hiragana_to_katakana(reading);
                if katakana != kana[0] {
                    kana.push(katakana);
                }
                for text in kana {
                    if view.iter().any(|c| c.text == text) {
                        continue;
                    }
                    view.push(Candidate {
                        description: width_annotation(&text).map(str::to_string),
                        text,
                        reading: Some(reading.to_string()),
                        source: Some(CandidateSource::Fallback),
                    });
                }
                view
            }
            // Fallback has no slot in the cycle; nothing to show.
            CandidateSource::Fallback => Vec::new(),
        }
    }

    /// Model candidates for the narrowed AI view — the same split
    /// conversion as the mixed list, so right after Space this is normally
    /// a pure cache replay of the list's model rows.
    fn model_source_view(&mut self, reading: &str) -> Vec<Candidate> {
        self.model_candidates(reading, self.config.num_candidates)
            .into_iter()
            .map(|text| Candidate {
                text,
                reading: Some(reading.to_string()),
                source: Some(CandidateSource::Model),
                description: None,
            })
            .collect()
    }

    /// Base reading + unresolved romaji tail for live-narrowing queries.
    /// The split only predicts correctly while the caret sits at the end of
    /// the composition (あk|い settles to あkい, never あいか…); otherwise
    /// fall back to the settled `reading` with no tail.
    fn live_query_split(&self, reading: &str) -> (String, String) {
        if self.input_buf.cursor() == self.input_buf.char_count() {
            (self.input_buf.reading(), self.input_buf.pending())
        } else {
            (reading.to_string(), String::new())
        }
    }
}
