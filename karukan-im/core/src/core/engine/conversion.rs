//! Conversion state handling (candidates, commit). The live-conversion
//! chunking lives in the sibling `chunk` module.

use std::collections::HashSet;
use std::time::Instant;

use tracing::debug;

use super::chunk::is_japanese;
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

/// Ctrl+R/T rotate through the source views in the mixed list's priority
/// order — the full list is not a stop (it is what Space already shows;
/// Esc → Space returns to it). Fallback has no slot of its own — the
/// plain kana ride at the tail of the rewriter view, which sits last so
/// Ctrl+T reaches it in one press from the full list.
const FILTER_CYCLE: [CandidateSource; 5] = [
    CandidateSource::Learning,
    CandidateSource::UserDictionary,
    CandidateSource::Model,
    CandidateSource::Dictionary,
    CandidateSource::Rewriter,
];

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
fn width_annotation(text: &str) -> Option<&'static str> {
    if karukan_engine::is_pure_hiragana(text) {
        Some("[全]ひらがな")
    } else if karukan_engine::is_pure_full_katakana(text) {
        Some("[全]カタカナ")
    } else {
        None
    }
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
    /// Kana-kanji conversion via the model(s), cached by (katakana reading,
    /// lctx, strategy) — a cache hit skips inference, so re-running unchanged
    /// chunks is free. `api_context` is the left context fed to the model.
    ///
    /// Kana-free readings skip the model entirely: it hallucinates on
    /// symbol/alphabet-only input (rewriters cover those).
    pub(super) fn run_kana_kanji_conversion(
        &mut self,
        reading: &str,
        api_context: &str,
        num_candidates: usize,
    ) -> Vec<String> {
        if !karukan_engine::contains_kana(reading) {
            return vec![];
        }
        let katakana = karukan_engine::hiragana_to_katakana(reading);

        let strategy = self.determine_strategy(reading, num_candidates);

        // Cache lookup comes before the converter check: a hit needs no model.
        let key = ConversionCacheKey {
            katakana: katakana.clone(),
            lctx: api_context.to_string(),
            strategy: strategy.clone(),
        };
        if let Some(candidates) = self.conversion_cache.get(&key) {
            debug!(
                "convert: cache hit reading=\"{}\" api_context=\"{}\" strategy={:?}",
                reading, api_context, strategy
            );
            // conversion_ms stays 0 (no inference ran) and the adaptive flag
            // is left untouched — a cache hit says nothing about model speed.
            self.metrics.model_name = self.model_name_for(&strategy);
            return candidates;
        }

        debug!(
            "convert: reading=\"{}\" api_context=\"{}\" candidates={} strategy={:?}",
            reading, api_context, num_candidates, strategy
        );

        let start = Instant::now();

        let Some(converter) = self.converters.kanji.as_ref() else {
            return vec![];
        };
        // Each arm returns (candidates, main-model greedy latency); only a
        // measured main greedy run feeds the adaptive gate below.
        let (candidates, main_ms) = match &strategy {
            ConversionStrategy::ParallelBeam { beam_width } => {
                let Some(result) = self.run_parallel_beam(&katakana, api_context, *beam_width)
                else {
                    return vec![];
                };
                result
            }
            ConversionStrategy::LightModelOnly => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                let candidates = light_converter
                    .convert(&katakana, api_context, 1)
                    .unwrap_or_default();
                (candidates, None)
            }
            ConversionStrategy::LightModelBeam { beam_width } => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                let candidates = light_converter
                    .convert(&katakana, api_context, *beam_width)
                    .unwrap_or_default();
                (candidates, None)
            }
            ConversionStrategy::MainModelOnly => {
                let main_start = Instant::now();
                let candidates = converter
                    .convert(&katakana, api_context, 1)
                    .unwrap_or_default();
                (candidates, Some(main_start.elapsed().as_millis() as u64))
            }
            ConversionStrategy::MainModelBeam { beam_width } => {
                let candidates = converter
                    .convert(&katakana, api_context, *beam_width)
                    .unwrap_or_default();
                (candidates, None)
            }
        };

        self.metrics.conversion_ms = start.elapsed().as_millis() as u64;
        if let Some(ms) = main_ms {
            self.update_adaptive_model_flag(ms);
        }
        self.metrics.model_name = self.model_name_for(&strategy);

        // Don't cache empty results: they usually mean a conversion error,
        // and pinning one would keep replaying the failure.
        if !candidates.is_empty() {
            self.conversion_cache.insert(key, candidates.clone());
        }

        candidates
    }

    /// ParallelBeam: main greedy and light beam in parallel. The main half
    /// is an ordinary MainModelOnly conversion and shares that cache slot,
    /// so it runs at most once per (reading, lctx) across all callers.
    /// Returns the merged candidates and the main half's measured latency
    /// (None when it was cache-served). None when a converter is missing.
    fn run_parallel_beam(
        &mut self,
        katakana: &str,
        api_context: &str,
        beam_width: usize,
    ) -> Option<(Vec<String>, Option<u64>)> {
        let converter = self.converters.kanji.as_ref()?;
        let light_converter = self.converters.light_kanji.as_ref()?;
        let main_key = ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: api_context.to_string(),
            strategy: ConversionStrategy::MainModelOnly,
        };
        let cached_main = self.conversion_cache.get(&main_key);
        let (computed_main, light_candidates) = std::thread::scope(|s| {
            let h_default = cached_main.is_none().then(|| {
                s.spawn(|| {
                    let main_start = Instant::now();
                    let result = converter
                        .convert(katakana, api_context, 1)
                        .unwrap_or_default();
                    (result, main_start.elapsed().as_millis() as u64)
                })
            });
            let h_beam = s.spawn(|| {
                light_converter
                    .convert(katakana, api_context, beam_width)
                    .unwrap_or_default()
            });
            (
                h_default.map(|h| h.join().unwrap_or_default()),
                h_beam.join().unwrap_or_default(),
            )
        });
        let (main_top1, main_ms) = match computed_main {
            Some((result, elapsed)) => {
                if !result.is_empty() {
                    self.conversion_cache.insert(main_key, result.clone());
                }
                (result, Some(elapsed))
            }
            None => (cached_main.unwrap_or_default(), None),
        };
        Some((
            Self::merge_candidates_dedup(main_top1, light_candidates, beam_width),
            main_ms,
        ))
    }

    /// Display name of the model(s) a strategy dispatches to.
    fn model_name_for(&self, strategy: &ConversionStrategy) -> String {
        let main = self
            .converters
            .kanji
            .as_ref()
            .map(|c| c.model_display_name().to_string())
            .unwrap_or_default();
        let light = self
            .converters
            .light_kanji
            .as_ref()
            .map(|c| c.model_display_name().to_string());
        match strategy {
            ConversionStrategy::ParallelBeam { .. } => {
                format!("{}+{}", main, light.unwrap_or_default())
            }
            ConversionStrategy::LightModelOnly | ConversionStrategy::LightModelBeam { .. } => {
                light.unwrap_or(main)
            }
            ConversionStrategy::MainModelOnly | ConversionStrategy::MainModelBeam { .. } => main,
        }
    }

    /// Start kanji conversion for the current buffer (Space/Down/Tab).
    /// `skip_learning` (the Tab path) omits learning-cache candidates.
    pub(super) fn start_conversion(&mut self, skip_learning: bool) -> EngineResult {
        // Resolve the reading without touching the composition, so Esc
        // returns to an editable buffer with the romaji tail still live.
        let reading = self.input_buf.settled_reading(&self.converters.romaji);
        // The unresolved tail keeps narrowing the predictive dictionary
        // lookup (わせd → 早稲田 stays selectable).
        let base = self.input_buf.reading();
        let pending = self.input_buf.pending();

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
            skip_learning,
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

        let candidate_list = Self::to_conversion_candidate_list(candidates, &reading);
        self.enter_conversion_state(&reading, candidate_list)
    }

    /// Map builder output (`AnnotatedCandidate`) to the public
    /// [`CandidateList`] shown in the conversion window. Candidates that don't
    /// carry their own reading fall back to `reading`. The source rides along
    /// as-is; its presentation (aux label, deletability) is derived on read.
    fn to_conversion_candidate_list(
        candidates: Vec<AnnotatedCandidate>,
        reading: &str,
    ) -> CandidateList {
        CandidateList::new(
            candidates
                .into_iter()
                .map(|ac| Candidate {
                    reading: Some(ac.reading.unwrap_or_else(|| reading.to_string())),
                    text: ac.text,
                    source: Some(ac.source),
                    description: ac.description,
                })
                .collect(),
        )
    }

    /// Transition to Conversion state with the given reading and candidate list.
    ///
    /// Sets up the preedit (highlighted selected text), updates the state, and
    /// returns an EngineResult with preedit, candidates, and aux text actions.
    fn enter_conversion_state(&mut self, reading: &str, candidates: CandidateList) -> EngineResult {
        let selected_text = candidates.selected_text().unwrap_or(reading).to_string();

        let preedit = Preedit::with_text_highlighted(&selected_text);

        self.state = InputState::Conversion {
            preedit: preedit.clone(),
            candidates: candidates.clone(),
            reading: reading.to_string(),
            // A fresh conversion always starts unfiltered
            filter: None,
        };

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(reading, Some(&candidates)),
            ))
    }

    /// Dictionary candidates for a reading: user dict first, then system,
    /// exact matches then predictive (prefix-extending) ones, deduped.
    ///
    /// `pending` narrows the predictive lookup to readings the romaji tail
    /// can still become (わせ + `d` keeps わせだ…, drops わせり…);
    /// `predictive_limit` caps those results. `only` restricts search and
    /// dedup to one dictionary, so shared surfaces stay visible per view.
    fn search_dictionaries(
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
    /// `base`/`pending` split the reading for the dictionary lookup (the
    /// unresolved romaji tail narrows prediction); `skip_learning` (Tab)
    /// omits the learning step.
    pub(super) fn build_conversion_candidates(
        &mut self,
        reading: &str,
        base: &str,
        pending: &str,
        num_candidates: usize,
        skip_learning: bool,
    ) -> Vec<AnnotatedCandidate> {
        // Init failure is not fatal: symbol-only inputs don't need the
        // model and still get dictionary/rewriter/fallback candidates.
        if self.converters.kanji.is_none()
            && let Err(e) = self.init_kanji_converter()
        {
            debug!("Failed to initialize kanji converter: {}", e);
        }

        let candidates = self.windowed_model_candidates(reading, num_candidates);

        let hiragana = reading.to_string();
        let katakana = karukan_engine::hiragana_to_katakana(reading);

        // Priority: Learning → User Dictionary → Model → System Dictionary → Fallback
        let mut builder = CandidateBuilder::new();

        // 1. Learning cache candidates (highest priority).
        //    Force-inserted so they win against duplicate text from later sources.
        //    Skipped when the caller asks for a learning-free conversion (Tab key).
        if !skip_learning {
            for c in self.lookup_learning_candidates(reading) {
                // Exact matches have reading == input reading; use None to avoid redundancy
                let cand_reading = c.reading.filter(|r| r != reading);
                builder.push_force(
                    AnnotatedCandidate::new(c.text, CandidateSource::Learning)
                        .with_reading(cand_reading),
                );
            }
        }

        // 2. Dictionary candidates (user dict first, then system dict)
        let dict_results = self.search_dictionaries(
            base,
            pending,
            usize::MAX,
            usize::MAX,
            MIN_PREDICTIVE_PREFIX_CHARS,
            None,
        );
        // Insert user dictionary entries at the top (after learning)
        for ac in &dict_results {
            if ac.source == CandidateSource::UserDictionary {
                builder.push(ac.clone());
            }
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

        // 4. System dictionary candidates (from search_dictionaries result)
        for ac in dict_results {
            if ac.source == CandidateSource::Dictionary {
                builder.push(ac);
            }
        }

        // 5/6. Hiragana/katakana fallback + rewriter variants. Emoji mode
        // shows rewriter (emoji) candidates only — no kana pair, like an
        // emoji picker; Enter in Composing still commits the literal query.
        let rewriter_variants = self
            .converters
            .rewriters
            .rewrite_all(&[reading.to_string()]);
        if self.mode.current() == InputMode::Emoji {
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        } else {
            builder.push(AnnotatedCandidate::new(hiragana, CandidateSource::Fallback));
            builder.push(AnnotatedCandidate::new(katakana, CandidateSource::Fallback));
            // Rewriters run on the typed reading only; running them on other
            // sources' candidates would emit variants nobody asked for.
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        }

        // 7. Symbol descriptions, Fallback only — model/dict/learning
        //    candidates must not inherit labels like 「金 = 部首」.
        for c in &mut builder.candidates {
            if c.source == CandidateSource::Fallback
                && c.description.is_none()
                && let Some(desc) = karukan_engine::symbol_description(&c.text)
            {
                c.description = Some(desc.to_string());
            }
        }

        // 8. Width annotations (`[全]カタカナ` etc.) for pure-kana
        //    candidates that still have no description.
        for c in &mut builder.candidates {
            if c.description.is_none()
                && let Some(desc) = width_annotation(&c.text)
            {
                c.description = Some(desc.to_string());
            }
        }

        builder.into_candidates()
    }

    /// Look up learning cache candidates for a reading (exact + prefix match, max 3).
    ///
    /// Returns candidates from the learning cache suitable for auto-suggest display.
    pub(super) fn lookup_learning_candidates(&self, reading: &str) -> Vec<Candidate> {
        self.lookup_learning(reading, "", MAX_LEARNING_CANDIDATES)
    }

    /// Full learning history for `reading` (exact + prefix, uncapped),
    /// narrowed by the unresolved romaji tail like the dictionary lookup —
    /// an exact hit on the base must not swallow the typed tail.
    fn lookup_learning_history(&self, reading: &str, pending: &str) -> Vec<Candidate> {
        self.lookup_learning(reading, pending, usize::MAX)
    }

    fn lookup_learning(&self, reading: &str, pending: &str, max: usize) -> Vec<Candidate> {
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

    /// Look up dictionary candidates for a reading (1 page, for live conversion display)
    ///
    /// Searches user dictionary first, then system dictionary.
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
        .map(|ac| Candidate {
            text: ac.text,
            // Predictive results carry their own (longer) reading
            reading: ac.reading.or_else(|| Some(reading.to_string())),
            source: Some(ac.source),
            description: None,
        })
        .collect()
    }

    /// Build rule-based rewriter variants for the reading itself (e.g. for
    /// symbol input `「` → `『`, `【`, `（`, ...). Used in the auto-suggest path
    /// so users see mozc-style symbol variants without pressing Space first.
    pub(super) fn lookup_rewriter_variants(&self, reading: &str) -> Vec<Candidate> {
        self.converters
            .rewriters
            .rewrite_all(&[reading.to_string()])
            .into_iter()
            .map(|(text, description)| Candidate {
                text,
                reading: Some(reading.to_string()),
                source: Some(CandidateSource::Rewriter),
                description,
            })
            .collect()
    }

    /// Merge two candidate lists with deduplication
    /// Primary candidates come first, then secondary candidates that aren't duplicates
    pub(super) fn merge_candidates_dedup(
        primary: Vec<String>,
        secondary: Vec<String>,
        max_candidates: usize,
    ) -> Vec<String> {
        let mut seen = HashSet::new();
        primary
            .into_iter()
            .chain(secondary)
            .filter(|c| seen.insert(c.clone()))
            .take(max_candidates)
            .collect()
    }

    /// Process key in conversion state
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
            Keysym::BACKSPACE => self.backspace_conversion(),
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
                            return self.cycle_candidate_filter(true);
                        }
                        Keysym::KEY_T | Keysym::KEY_T_UPPER => {
                            return self.cycle_candidate_filter(false);
                        }
                        _ => {}
                    }
                }

                // Check for digit selection (1-9)
                if let Some(digit) = key.keysym.digit_value() {
                    return self.select_candidate_by_digit(digit);
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
        self.set_composing_state();
        self.suppress_suggest = filter.is_some();
        let result = self.process_key_composing(key, shift_active);
        self.suppress_suggest = false;
        if let Some(source) = filter
            && matches!(self.state, InputState::Composing { .. })
        {
            return self.start_conversion_with_filter(source);
        }
        result
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

    /// Record a conversion selection in the learning cache.
    pub(super) fn record_learning(&mut self, reading: &str, surface: &str) {
        if let Some(cache) = &mut self.learning {
            cache.record(reading, surface);
        }
    }

    /// Record the committed conversion in the learning cache and reset to Empty state.
    ///
    /// Skips learning when the buffer is a `:shortcode` query — the
    /// reading would be e.g. `:smile`, which isn't a hiragana key
    /// and would corrupt the kana-keyed learning cache.
    fn finish_conversion(&mut self, text: &str, reading: &Option<String>) {
        if self.mode.current() != InputMode::Emoji
            && let Some(reading) = reading
        {
            self.record_learning(reading, text);
        }

        self.state = InputState::Empty;
        self.input_buf.clear();
        self.mode.exit_temporary();
    }

    /// Commit the current conversion
    fn commit_conversion(&mut self) -> EngineResult {
        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        if text.is_empty() {
            return EngineResult::consumed();
        }

        self.finish_conversion(&text, &reading);

        EngineResult::consumed()
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
            .with_action(EngineAction::Commit(text))
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

    /// Delete the selected learning candidate and its prefix twins from the
    /// history, then rebuild the conversion in place — dedup hid any other
    /// source's copy of the surface, and only a rebuild brings it back.
    /// The caller guards deletability ([`Self::selected_is_deletable`]).
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
        let InputState::Conversion { reading, .. } = &self.state else {
            return EngineResult::consumed();
        };
        let reading = reading.clone();
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
            false,
        );
        if candidates.is_empty() {
            return self.cancel_conversion();
        }
        let candidate_list = Self::to_conversion_candidate_list(candidates, &reading);
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

    /// Ctrl+R / Ctrl+T: rotate to the next / previous view in
    /// [`FILTER_CYCLE`], exactly one step per press — an empty source shows
    /// 「候補なし」, never skipped, so the position stays predictable. The
    /// rotation never returns to the full list.
    fn cycle_candidate_filter(&mut self, forward: bool) -> EngineResult {
        let current = match &self.state {
            InputState::Conversion { filter, .. } => *filter,
            _ => return EngineResult::not_consumed(),
        };
        let len = FILTER_CYCLE.len();
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

    /// Ctrl+R while composing: enter the Conversion state and immediately
    /// narrow it one step, so the filtered view opens without Space.
    pub(super) fn start_filtered_conversion(&mut self, forward: bool) -> EngineResult {
        if !self.enter_conversion_for_filter() {
            return EngineResult::consumed();
        }
        self.cycle_candidate_filter(forward)
    }

    /// Enter the Conversion state already narrowed to `source` — used when
    /// typing refines a narrowed view, so the view survives the keystroke.
    fn start_conversion_with_filter(&mut self, source: CandidateSource) -> EngineResult {
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
    fn apply_candidate_filter(&mut self, next: CandidateSource) -> EngineResult {
        let reading = match &self.state {
            InputState::Conversion { reading, .. } => reading.clone(),
            _ => return EngineResult::not_consumed(),
        };
        let list = CandidateList::new(self.source_view(next, &reading));
        let selected = list.selected_text().unwrap_or(&reading).to_string();
        let preedit = Preedit::with_text_highlighted(&selected);
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
        // Like candidate navigation, the aux shows the selected candidate's
        // own reading (predictive entries carry a longer one), falling back
        // to the base reading for an empty view.
        let aux_reading = list
            .selected()
            .and_then(|c| c.reading.clone())
            .unwrap_or_else(|| reading.clone());
        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(list.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(&aux_reading, Some(&list)),
            ))
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
            // A paged dictionary browser wants everything: uncapped, and
            // predictive from the first char (no flood guard).
            source @ (CandidateSource::UserDictionary | CandidateSource::Dictionary) => self
                .search_dictionaries(&base, &pending, usize::MAX, usize::MAX, 1, Some(source))
                .into_iter()
                .map(|ac| Candidate {
                    text: ac.text,
                    reading: ac.reading.or_else(|| Some(base.clone())),
                    source: Some(source),
                    description: ac.description,
                })
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

    /// Model candidates for the narrowed AI view — the same tail-window
    /// conversion as the mixed list, so right after Space this is normally
    /// a pure cache replay of the list's model rows.
    fn model_source_view(&mut self, reading: &str) -> Vec<Candidate> {
        self.windowed_model_candidates(reading, self.config.num_candidates)
            .into_iter()
            .map(|text| Candidate {
                text,
                reading: Some(reading.to_string()),
                source: Some(CandidateSource::Model),
                description: None,
            })
            .collect()
    }

    /// Model conversion shared by Space's mixed list and the AI view: the
    /// `num_candidates` beam runs only over a tail window
    /// ([`Self::beam_window_start`]); the part before it converts top-1 on
    /// the live-conversion chunk grid and prefixes every beam result, so
    /// the cost stays bounded however long the reading grows. The head of
    /// the list is the whole-reading grid replay — the exact text live
    /// typing displays — so the window's raw char cut (which can land
    /// mid-word) never degrades the visible top-1. Candidates equal to the
    /// raw reading are dropped: an empty result means "no model suggestion".
    fn windowed_model_candidates(&mut self, reading: &str, num_candidates: usize) -> Vec<String> {
        if !karukan_engine::contains_kana(reading) {
            return Vec::new();
        }
        let base_ctx = self.truncate_context_for_api();
        let chars: Vec<char> = reading.chars().collect();
        let window_start = self.beam_window_start(&chars);

        // The head must run before the window beam: a slow beam may flip
        // the adaptive flag, which changes the replay's cache keys — the
        // head would miss the entries typing just filled and re-convert to
        // a text the user never saw.
        let live_top1 = self.convert_on_chunk_grid(&chars, &base_ctx);

        let prefix = self.convert_on_chunk_grid(&chars[..window_start], &base_ctx);
        let window: String = chars[window_start..].iter().collect();

        // An empty window (the reading ends in a non-Japanese run) leaves
        // just the converted prefix as the single candidate.
        let tails = if window.is_empty() {
            vec![String::new()]
        } else {
            let lctx = self.lctx_for(&base_ctx, &prefix);
            let beam = self.run_kana_kanji_conversion(&window, &lctx, num_candidates);
            if beam.is_empty() { vec![window] } else { beam }
        };

        let prefixed = tails
            .into_iter()
            .map(|tail| format!("{prefix}{tail}"))
            .collect();
        let mut merged = Self::merge_candidates_dedup(vec![live_top1], prefixed, usize::MAX);
        merged.retain(|text| text != reading);
        merged
    }

    /// Start of the beam window: the final Japanese run, never crossing a
    /// chunk boundary, capped at `beam_window_len` chars (the strategy's
    /// beam gate uses the same unit, so the window always qualifies for the
    /// beam) and at the live-conversion chunk length.
    fn beam_window_start(&self, chars: &[char]) -> usize {
        let run_start = chars
            .iter()
            .rposition(|c| !is_japanese(*c))
            .map_or(0, |i| i + 1);
        let cap = self.config.beam_window_len.min(self.chunk_len());
        run_start.max(chars.len().saturating_sub(cap))
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

    /// Cancel conversion and return to hiragana
    pub(super) fn cancel_conversion(&mut self) -> EngineResult {
        if !matches!(self.state, InputState::Conversion { .. }) {
            return EngineResult::not_consumed();
        }

        if self.input_buf.is_empty() {
            self.state = InputState::Empty;
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        // The composition was left untouched when the conversion started:
        // just come back to it, pending romaji still live
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
        self.update_conversion_preedit(&selected_text, &candidates)
    }

    /// Select next candidate
    fn next_candidate(&mut self) -> EngineResult {
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
        let result = self.select_candidate_by_digit(page_index + 1);
        self.metrics.process_key_ms = start.elapsed().as_millis() as u64;
        result
    }

    /// Select candidate by digit (1-9)
    fn select_candidate_by_digit(&mut self, digit: usize) -> EngineResult {
        let (selected_text, reading) = {
            let candidates = match self.state.candidates_mut() {
                Some(c) => c,
                None => return EngineResult::not_consumed(),
            };

            if candidates.select_on_page(digit).is_none() {
                return EngineResult::consumed();
            }

            let text = candidates.selected_text().unwrap_or("").to_string();
            let reading = candidates.selected().and_then(|c| c.reading.clone());
            (text, reading)
        };

        // Record learning before committing
        if let Some(reading) = &reading {
            self.record_learning(reading, &selected_text);
        }

        // Commit immediately after digit selection

        self.state = InputState::Empty;

        EngineResult::consumed()
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
            .with_action(EngineAction::Commit(selected_text))
    }

    /// Update preedit after candidate selection change
    fn update_conversion_preedit(
        &mut self,
        selected_text: &str,
        candidates: &CandidateList,
    ) -> EngineResult {
        let preedit = Preedit::with_text_highlighted(selected_text);

        if let Some(p) = self.state.preedit_mut() {
            *p = preedit.clone();
        }

        let reading = candidates
            .selected()
            .and_then(|c| c.reading.as_deref())
            .unwrap_or("");

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(reading, Some(candidates)),
            ))
    }

    /// Handle backspace in conversion mode
    fn backspace_conversion(&mut self) -> EngineResult {
        // Return to hiragana mode with the reading
        self.cancel_conversion()
    }
}
