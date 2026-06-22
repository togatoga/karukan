//! Conversion state handling (candidates, chunks, commit)

use std::collections::HashSet;
use std::time::Instant;

use tracing::debug;

use super::*;

/// Maximum number of learning candidates to show
const MAX_LEARNING_CANDIDATES: usize = 3;

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

/// Number of leading chars shared by `a` and `b`.
fn common_prefix_len(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

/// Number of trailing chars shared by `a` and `b`, capped so it never overlaps
/// the already-counted common prefix of length `prefix_len`.
fn common_suffix_len(a: &[char], b: &[char], prefix_len: usize) -> usize {
    let max = a.len().min(b.len()) - prefix_len;
    let mut n = 0;
    while n < max && a[a.len() - 1 - n] == b[b.len() - 1 - n] {
        n += 1;
    }
    n
}

/// Punctuation that ends a chunk (clause / sentence boundary). A chunk is cut
/// right after a run of these so the model converts whole clauses instead of
/// being split mid-word.
fn is_chunk_break_punct(c: char) -> bool {
    matches!(
        c,
        '。' | '、' | '！' | '？' | '，' | '．' | '…' | '!' | '?' | '.' | ','
    )
}

/// Split `chars` into chunks of at most `max` chars.
///
/// Growing a chunk char by char: while the previous char is *not* punctuation,
/// keep appending to the current chunk as long as it has room (< `max`);
/// once a punctuation run ends (the next char is a non-punctuation char after a
/// punctuation), start a new chunk. Consecutive punctuation stays attached to
/// the chunk while it fits, so runs like `！？` or `。。。` are not scattered.
/// With no punctuation this degrades to fixed `max`-char chunks.
fn punct_chunks(chars: &[char], max: usize) -> Vec<&[char]> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let limit = (start + max).min(chars.len());
        let mut i = start;
        // Grow through ordinary (non-punctuation) chars up to the cap.
        while i < limit && !is_chunk_break_punct(chars[i]) {
            i += 1;
        }
        // Absorb the trailing punctuation run (capped at `max`) so the clause —
        // and consecutive marks — stay together in this chunk.
        while i < limit && is_chunk_break_punct(chars[i]) {
            i += 1;
        }
        out.push(&chars[start..i]);
        start = i;
    }
    out
}

/// How to re-chunk the buffer after an edit, derived purely from the previous
/// chunking and the new text — no engine or model needed (so it is unit
/// tested directly).
///
/// The new buffer is diffed against the old chunking by common character
/// prefix/suffix: whole chunks inside the unchanged prefix/suffix are kept,
/// and only the `mid_start..mid_end` span (in chars of the new text) has to be
/// re-chunked and reconverted.
#[derive(Debug, PartialEq, Eq)]
struct ChunkPlan {
    /// Leading old chunks to reuse verbatim.
    lead_count: usize,
    /// Trailing old chunks to reuse (cached conversion kept).
    trail_count: usize,
    /// Char offset in the new text where the changed span begins (= leading chars).
    mid_start: usize,
    /// Char offset in the new text where the changed span ends (= len - trailing chars).
    mid_end: usize,
}

impl ChunkPlan {
    /// Diff `old_text` (the concatenated readings of the previous chunks,
    /// whose individual char lengths are `old_lens`) against the new `text`.
    fn compute(old_lens: &[usize], old_text: &[char], text: &[char], chunk_len: usize) -> Self {
        let cp = common_prefix_len(old_text, text);
        let cs = common_suffix_len(old_text, text, cp);

        // Leading whole chunks that lie entirely inside the unchanged prefix.
        let mut lead_count = 0;
        let mut lead_chars = 0;
        while lead_count < old_lens.len() && lead_chars + old_lens[lead_count] <= cp {
            lead_chars += old_lens[lead_count];
            lead_count += 1;
        }
        // Reopen the last leading chunk when it sits right at the edit and is
        // not yet full, so an append/edit merges into it instead of spawning a
        // stray short chunk (keeps forward typing at one growing chunk). But not
        // when that chunk ends in punctuation: a completed clause should stay
        // put and the new char should start a fresh chunk.
        if lead_count > 0
            && lead_chars == cp
            && cp < text.len()
            && old_lens[lead_count - 1] < chunk_len
            && !is_chunk_break_punct(old_text[lead_chars - 1])
        {
            lead_count -= 1;
            lead_chars -= old_lens[lead_count];
        }

        // Trailing whole chunks inside the unchanged suffix, without crossing
        // into the leading region.
        let mut trail_count = 0;
        let mut trail_chars = 0;
        while trail_count < old_lens.len() - lead_count {
            let idx = old_lens.len() - 1 - trail_count;
            if trail_chars + old_lens[idx] <= cs {
                trail_chars += old_lens[idx];
                trail_count += 1;
            } else {
                break;
            }
        }

        Self {
            lead_count,
            trail_count,
            mid_start: lead_chars,
            mid_end: text.len() - trail_chars,
        }
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
    /// Run kana-kanji conversion for a reading via llama.cpp model.
    ///
    /// Determines the conversion strategy (main model, light model, or parallel beam),
    /// dispatches to the appropriate model(s), measures latency, and records which model was used.
    ///
    /// Skips the model entirely when the reading has no hiragana/katakana — the
    /// model is trained on kana → kanji and hallucinates garbage (e.g. `「` → `w`)
    /// for symbol- or alphabet-only inputs. Rule-based variants from
    /// `SymbolRewriter` cover those cases instead.
    ///
    /// `api_context` is the left context (lctx) fed to the model. Callers pass
    /// `truncate_context_for_api()` for a whole-buffer conversion, or — for
    /// chunked live conversion — the converted text of the preceding chunks.
    fn run_kana_kanji_conversion(
        &mut self,
        reading: &str,
        api_context: &str,
        num_candidates: usize,
    ) -> Vec<String> {
        if !karukan_engine::contains_kana(reading) {
            return vec![];
        }
        let Some(converter) = self.converters.kanji.as_ref() else {
            return vec![];
        };
        let katakana = karukan_engine::hiragana_to_katakana(reading);
        let main_model_name = converter.model_display_name().to_string();

        let strategy = self.determine_strategy(reading, num_candidates);
        debug!(
            "convert: reading=\"{}\" api_context=\"{}\" candidates={} strategy={:?}",
            reading, api_context, num_candidates, strategy
        );

        let start = Instant::now();

        let candidates = match &strategy {
            ConversionStrategy::ParallelBeam { beam_width } => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                let bw = *beam_width;
                let (default_top1, light_candidates) = std::thread::scope(|s| {
                    let h_default = s.spawn(|| {
                        converter
                            .convert(&katakana, api_context, 1)
                            .unwrap_or_default()
                    });
                    let h_beam = s.spawn(|| {
                        light_converter
                            .convert(&katakana, api_context, bw)
                            .unwrap_or_default()
                    });
                    (
                        h_default.join().unwrap_or_default(),
                        h_beam.join().unwrap_or_default(),
                    )
                });
                Self::merge_candidates_dedup(default_top1, light_candidates, bw)
            }
            ConversionStrategy::LightModelOnly => {
                let Some(light_converter) = self.converters.light_kanji.as_ref() else {
                    return vec![];
                };
                light_converter
                    .convert(&katakana, api_context, 1)
                    .unwrap_or_default()
            }
            ConversionStrategy::MainModelOnly => converter
                .convert(&katakana, api_context, 1)
                .unwrap_or_default(),
            ConversionStrategy::MainModelBeam { beam_width } => converter
                .convert(&katakana, api_context, *beam_width)
                .unwrap_or_default(),
        };

        self.metrics.conversion_ms = start.elapsed().as_millis() as u64;
        self.update_adaptive_model_flag(&strategy);

        self.metrics.model_name = match &strategy {
            ConversionStrategy::ParallelBeam { .. } => {
                let light_name = self
                    .converters
                    .light_kanji
                    .as_ref()
                    .map(|c| c.model_display_name().to_string())
                    .unwrap_or_default();
                format!("{}+{}", main_model_name, light_name)
            }
            ConversionStrategy::LightModelOnly => self
                .converters
                .light_kanji
                .as_ref()
                .map(|c| c.model_display_name().to_string())
                .unwrap_or(main_model_name),
            ConversionStrategy::MainModelOnly | ConversionStrategy::MainModelBeam { .. } => {
                main_model_name
            }
        };

        candidates
    }

    /// Auto-suggest over the composing buffer, split into chunks of at most
    /// `config.composing_chunk_len` reading characters so each model call
    /// stays bounded for long input.
    ///
    /// Re-chunking is *incremental* and content-anchored: the new buffer is
    /// diffed against the previous chunking (`self.chunks`) by common
    /// character prefix/suffix. Chunks that fall entirely in the unchanged
    /// prefix are reused as-is, chunks entirely in the unchanged suffix keep
    /// their cached conversion, and only the changed middle span is re-chunked
    /// and re-run through the model. So a keystroke at the end reconverts only
    /// the final chunk, and an edit/deletion in the middle reconverts only the
    /// chunk(s) it touched — not everything downstream.
    ///
    /// Trade-off: a middle edit changes the left context of the chunks to its
    /// right, but those suffix chunks are *not* reconverted (that is the whole
    /// point — bounded cost). Their displayed conversion stays as last computed
    /// until they are themselves edited or the text is committed. Each chunk's
    /// left context is still the editor surrounding text plus the converted text
    /// of all preceding chunks, truncated to `max_api_context_len`.
    ///
    /// Returns the concatenated conversion of the whole buffer, or `None` when
    /// it equals the raw reading (no useful model suggestion).
    ///
    /// Note: for input no longer than one chunk (the common case, default
    /// N=40) this produces exactly one model call over the whole buffer, i.e.
    /// identical behavior to a whole-buffer conversion.
    pub(super) fn chunked_auto_suggest(&mut self) -> Option<String> {
        let full_reading = self.input_buf.text.clone();
        if full_reading.is_empty() {
            self.chunks.clear();
            return None;
        }
        self.ensure_kanji_converter();

        let chunk_len = self.chunk_len();
        let text: Vec<char> = full_reading.chars().collect();
        let base_ctx = self.truncate_context_for_api();

        // Previous chunking (covers the pre-edit text). Move it out so the
        // model calls below don't conflict with borrowing `self.chunks`.
        let mut old = std::mem::take(&mut self.chunks);
        let old_lens: Vec<usize> = old.iter().map(|s| s.reading.chars().count()).collect();
        let old_text: Vec<char> = old.iter().flat_map(|s| s.reading.chars()).collect();

        let plan = ChunkPlan::compute(&old_lens, &old_text, &text, chunk_len);

        let mut chunks: Vec<ComposingChunk> = Vec::with_capacity(old.len() + 1);
        let mut combined = String::new();

        // 1. Reused leading chunks — reading + converted still valid (their left
        //    context is unchanged because everything before them is unchanged).
        for chunk in old.drain(..plan.lead_count) {
            combined.push_str(&chunk.converted);
            chunks.push(chunk);
        }
        // `old` now starts at the first non-leading chunk; the trailing
        // chunks to keep are its last `trail_count` entries.
        let trail_start = old.len() - plan.trail_count;

        // 2. Changed middle span: re-chunk into <= N chars and reconvert. Each
        //    chunk's left context is the surrounding text plus everything
        //    converted so far, truncated (the tail wins, so the nearest left
        //    chunk dominates).
        let middle = &text[plan.mid_start..plan.mid_end];
        for chunk in punct_chunks(middle, chunk_len) {
            let reading: String = chunk.iter().collect();
            let lctx = self.truncate_context(&format!("{base_ctx}{combined}"));
            let converted = self.convert_chunk(&reading, &lctx);
            combined.push_str(&converted);
            chunks.push(ComposingChunk { reading, converted });
        }

        // 3. Reused trailing chunks — cached conversion kept (the left context
        //    it was converted with may have drifted, but we don't reconvert).
        for chunk in old.drain(trail_start..) {
            combined.push_str(&chunk.converted);
            chunks.push(chunk);
        }

        let reconverted = chunks.len() - plan.lead_count - plan.trail_count;
        self.chunks = chunks;
        self.log_chunk_state("convert");
        debug!(
            "chunked_auto_suggest: reused {} leading + {} trailing chunk(s), reconverted {} middle chunk(s)",
            plan.lead_count, plan.trail_count, reconverted
        );

        (combined != full_reading).then_some(combined)
    }

    /// Configured maximum chunk length in chars, clamped to at least 1.
    fn chunk_len(&self) -> usize {
        self.config.composing_chunk_len.max(1)
    }

    /// Left context for the chunk at `index`: the editor surrounding text plus
    /// the converted text of every preceding chunk, truncated to the context
    /// budget. Derived on demand (the chunk doesn't store it) — it is just "the
    /// value of the chunks to the left".
    pub(super) fn chunk_lctx(&self, index: usize) -> String {
        let base = self.truncate_context_for_api();
        let preceding: String = self.chunks[..index.min(self.chunks.len())]
            .iter()
            .map(|c| c.converted.as_str())
            .collect();
        self.truncate_context(&format!("{base}{preceding}"))
    }

    /// Best-effort lazy init of the kanji converter. Chunking proceeds even
    /// on failure so `self.chunks` always mirrors the current buffer (which
    /// chunk the cursor is in, etc.); `run_kana_kanji_conversion` handles a
    /// missing converter by yielding nothing, and each chunk falls back to its
    /// own reading.
    fn ensure_kanji_converter(&mut self) {
        if self.converters.kanji.is_none()
            && let Err(e) = self.init_kanji_converter()
        {
            debug!("Failed to initialize kanji converter: {}", e);
        }
    }

    /// Model conversion of one chunk's `reading` given `lctx`, falling back to
    /// the reading itself when the model yields nothing.
    fn convert_chunk(&mut self, reading: &str, lctx: &str) -> String {
        self.run_kana_kanji_conversion(reading, lctx, 1)
            .into_iter()
            .next()
            .unwrap_or_else(|| reading.to_string())
    }

    /// Index of the chunk the cursor currently sits in, found by walking the
    /// actual chunk lengths (chunks are variable-length once punctuation
    /// splitting is in play, so a fixed `cursor / chunk_len` is wrong). This is
    /// the chunk a character insert/delete at the cursor lands in. Returns 0 for
    /// an empty buffer or a cursor at the very start.
    pub(super) fn current_chunk_index(&self) -> usize {
        let pos = self.input_buf.cursor_pos.saturating_sub(1);
        let mut end = 0;
        for (i, chunk) in self.chunks.iter().enumerate() {
            end += chunk.reading.chars().count();
            if pos < end {
                return i;
            }
        }
        self.chunks.len().saturating_sub(1)
    }

    /// Emit a debug line describing the current chunking: how many chunks
    /// exist and which one — and how long — the cursor currently sits in. `at`
    /// labels the call site (e.g. `"convert"` after re-chunking, `"cursor"`
    /// after a caret move) so the log shows chunk changes on cursor movement,
    /// not just on conversion.
    pub(super) fn log_chunk_state(&self, at: &str) {
        let current = self.current_chunk_index();
        let current_len = self
            .chunks
            .get(current)
            .map(|chunk| chunk.reading.chars().count())
            .unwrap_or(0);
        debug!(
            "chunks [{}]: {} chunk(s); cursor at pos {} in chunk {} ({} char(s))",
            at,
            self.chunks.len(),
            self.input_buf.cursor_pos,
            current,
            current_len
        );
    }

    /// Start kanji conversion for the current input buffer.
    ///
    /// Called when DOWN/TAB/SPACE is pressed: flushes any pending romaji,
    /// resolves the reading, runs `build_conversion_candidates`, and
    /// transitions into the Conversion state. The previous live-conversion
    /// result is preserved as the first model candidate so the user sees
    /// the same text they had been looking at during input.
    ///
    /// `skip_learning` is set by the Tab path to omit learning-cache
    /// candidates (Space/Down keep the default learning-included behavior).
    pub(super) fn start_conversion(&mut self, skip_learning: bool) -> EngineResult {
        // Flush any remaining romaji into composed_hiragana
        self.flush_romaji_to_composed();

        let reading = self.input_buf.text.clone();

        // Save auto-suggest/live conversion result before clearing state.
        // This ensures the candidate that was displayed during input is preserved
        // in the conversion candidate list even if the re-inference uses a different strategy.
        let prev_suggest_text = std::mem::take(&mut self.live.text);

        self.converters.romaji.reset();
        self.input_buf.cursor_pos = 0;

        if reading.is_empty() {
            return EngineResult::consumed();
        }

        // Get candidates from kanji converter (use full num_candidates for explicit conversion)
        let mut candidates =
            self.build_conversion_candidates(&reading, self.config.num_candidates, skip_learning);

        // If the previous auto-suggest result is not in the new candidates, insert it at the top
        // so it doesn't disappear when the conversion strategy changes.
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
            // No candidates, stay in hiragana mode
            let preedit = Preedit::with_text_underlined(&reading);
            self.state = InputState::Composing {
                preedit: preedit.clone(),
                romaji_buffer: String::new(),
            };
            return EngineResult::consumed().with_action(EngineAction::UpdatePreedit(preedit));
        }

        // Map AnnotatedCandidate → public Candidate. The two annotation
        // slots are kept disjoint so descriptions never duplicate between the
        // aux text and the candidate's right-side comment:
        //   - `source_label` ← source.label() only (e.g. `🤖 AI`, `📚 辞書`)
        //   - `description`  ← the per-candidate description only
        //                      (e.g. `三点リーダ`, `[全]英大文字`)
        let candidate_list = CandidateList::new(
            candidates
                .into_iter()
                .map(|ac| {
                    let cand_reading = ac.reading.unwrap_or_else(|| reading.clone());
                    let label = ac.source.label();
                    Candidate {
                        text: ac.text,
                        reading: Some(cand_reading),
                        source_label: (!label.is_empty()).then(|| label.to_string()),
                        description: ac.description,
                    }
                })
                .collect(),
        );
        self.enter_conversion_state(&reading, candidate_list)
    }

    /// Transition to Conversion state with the given reading and candidate list.
    ///
    /// Sets up the preedit (highlighted selected text), updates the state, and
    /// returns an EngineResult with preedit, candidates, and aux text actions.
    fn enter_conversion_state(&mut self, reading: &str, candidates: CandidateList) -> EngineResult {
        let selected_text = candidates.selected_text().unwrap_or(reading).to_string();

        let preedit = Preedit::from_segments(
            vec![PreeditSegment::highlighted(&selected_text)],
            selected_text.chars().count(),
        );

        self.state = InputState::Conversion {
            preedit: preedit.clone(),
            candidates: candidates.clone(),
        };

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::ShowCandidates(candidates.clone()))
            .with_action(EngineAction::UpdateAuxText(
                self.format_aux_conversion_with_page(reading, Some(&candidates)),
            ))
    }

    /// Search user and system dictionaries for candidates matching a reading.
    ///
    /// User dictionary results come first (higher priority), then system dictionary
    /// results sorted by score. Duplicates are removed via HashSet.
    fn search_dictionaries(&self, reading: &str, limit: usize) -> Vec<AnnotatedCandidate> {
        let mut candidates = Vec::new();
        let mut seen = HashSet::new();

        // User dictionary (higher priority)
        if let Some(dict) = &self.dicts.user
            && let Some(result) = dict.exact_match_search(reading)
        {
            for cand in result.candidates {
                if candidates.len() >= limit {
                    break;
                }
                if seen.insert(cand.surface.clone()) {
                    candidates.push(AnnotatedCandidate::new(
                        cand.surface.clone(),
                        CandidateSource::UserDictionary,
                    ));
                }
            }
        }

        // System dictionary (sorted by score)
        if let Some(dict) = &self.dicts.system
            && let Some(result) = dict.exact_match_search(reading)
        {
            let mut dict_candidates: Vec<_> = result.candidates.to_vec();
            dict_candidates.sort_by(|a, b| a.score.total_cmp(&b.score));
            for cand in dict_candidates {
                if candidates.len() >= limit {
                    break;
                }
                if seen.insert(cand.surface.clone()) {
                    candidates.push(AnnotatedCandidate::new(
                        cand.surface,
                        CandidateSource::Dictionary,
                    ));
                }
            }
        }

        candidates
    }

    /// Build conversion candidates for a reading from multiple sources.
    ///
    /// Combines learning cache, dictionaries, and model inference results
    /// with deduplication. Uses dynamic candidate count based on input token
    /// count for performance.
    ///
    /// Priority: Learning → User Dictionary → Model → System Dictionary → Fallback
    ///
    /// `skip_learning` suppresses the learning-cache step (1). Used by the Tab
    /// key path so users can escape a noisy learning history without losing
    /// access to dictionary/model candidates.
    pub(super) fn build_conversion_candidates(
        &mut self,
        reading: &str,
        num_candidates: usize,
        skip_learning: bool,
    ) -> Vec<AnnotatedCandidate> {
        // Try to initialize the kanji converter, but don't bail out if it
        // fails — symbol-only inputs (e.g. `。。。`) don't need the model and
        // we still want to produce dictionary, rewriter, and fallback candidates.
        // run_kana_kanji_conversion handles the converter-missing case.
        if self.converters.kanji.is_none()
            && let Err(e) = self.init_kanji_converter()
        {
            debug!("Failed to initialize kanji converter: {}", e);
        }

        let api_context = self.truncate_context_for_api();
        let candidates = self.run_kana_kanji_conversion(reading, &api_context, num_candidates);

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
        let dict_results = self.search_dictionaries(reading, usize::MAX);
        // Insert user dictionary entries at the top (after learning)
        for ac in &dict_results {
            if ac.source == CandidateSource::UserDictionary {
                builder.push(ac.clone());
            }
        }

        // 3. Model inference results
        if candidates.is_empty() {
            // In emoji mode, defer the literal-fallback decision until
            // after rewriters have run — otherwise `:smile` would be
            // pinned to the top of the candidate list as a Fallback
            // and outrank the 😄 we surface in step 5/6.
            if builder.is_empty() && self.input_mode != InputMode::Emoji {
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

        // 5/6. Hiragana/katakana fallback + rewriter variants.
        //
        // In emoji mode we surface ONLY the rewriter (i.e. EmojiRewriter)
        // candidates — Slack's emoji picker shows emojis and nothing
        // else, and that's the mental model the user wants here.
        // No literal `:smile` / `:xyz` fallback in the candidate list:
        // if nothing matches, the picker is just empty. (Enter on a
        // no-match query in Composing still commits the buffer
        // literal via `commit_composing`; that's the escape hatch.)
        // Non-emoji modes keep the original order so existing IME
        // behavior is untouched.
        let rewriter_variants = self
            .converters
            .rewriters
            .rewrite_all(&[reading.to_string()]);
        if self.input_mode == InputMode::Emoji {
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        } else {
            builder.push(AnnotatedCandidate::new(hiragana, CandidateSource::Fallback));
            builder.push(AnnotatedCandidate::new(katakana, CandidateSource::Fallback));
            // Rewriters operate on the user's typed input (the reading
            // itself). Running them on dictionary/model/fallback
            // candidates produces unrelated noise (e.g. a dictionary
            // entry of `,` for some reading would generate `、`/`，`
            // variants the user never asked for; a learning entry `アト`
            // pulled by prefix lookup on `あ` would emit `ｱﾄ`).
            for (variant, description) in rewriter_variants {
                builder.push(
                    AnnotatedCandidate::new(variant, CandidateSource::Rewriter)
                        .with_description(description),
                );
            }
        }

        // 7. Enrich Fallback candidates whose text is a known symbol with
        //    its description (mirrors the relevant slice of mozc's
        //    `AddDescForCurrentCandidates`). Restricted to Fallback so the
        //    AI/Dict/Learning paths don't pick up unwanted labels — e.g.
        //    the model returning `金` for `きん` should NOT inherit mozc's
        //    "部首" annotation. Typed-symbol input still gets annotated:
        //    pressing `「` produces a Fallback candidate `「`, which here
        //    picks up "始めかぎ括弧".
        for c in &mut builder.candidates {
            if c.source == CandidateSource::Fallback
                && c.description.is_none()
                && let Some(desc) = karukan_engine::symbol_description(&c.text)
            {
                c.description = Some(desc.to_string());
            }
        }

        // 8. Attach mozc-style width annotations (`[全]ひらがな`,
        //    `[全]カタカナ`, `[半]カタカナ`) to any pure-kana candidate that
        //    still has no description. This catches `あ`/`ア` candidates that
        //    arrived via the Model or Fallback paths and were deduped against
        //    the rewriter's already-labelled variants.
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
        let Some(cache) = &self.learning else {
            return vec![];
        };
        let mut candidates: Vec<Candidate> = Vec::new();
        let mut seen = HashSet::new();
        let label = CandidateSource::Learning.label().to_string();

        // Exact match
        for (surface, _score) in cache.lookup(reading) {
            if candidates.len() >= MAX_LEARNING_CANDIDATES {
                break;
            }
            if seen.insert(surface.clone()) {
                candidates.push(Candidate {
                    text: surface,
                    reading: Some(reading.to_string()),
                    source_label: Some(label.clone()),
                    description: None,
                });
            }
        }

        // Prefix match (predictive)
        for (full_reading, surface, _score) in cache.prefix_lookup(reading) {
            if candidates.len() >= MAX_LEARNING_CANDIDATES {
                break;
            }
            if full_reading == reading {
                continue;
            }
            if seen.insert(surface.clone()) {
                candidates.push(Candidate {
                    text: surface,
                    reading: Some(full_reading),
                    source_label: Some(label.clone()),
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
        self.search_dictionaries(reading, CandidateList::DEFAULT_PAGE_SIZE)
            .into_iter()
            .map(|ac| Candidate {
                text: ac.text,
                reading: Some(reading.to_string()),
                source_label: Some(ac.source.label().to_string()),
                description: None,
            })
            .collect()
    }

    /// Build rule-based rewriter variants for the reading itself (e.g. for
    /// symbol input `「` → `『`, `【`, `（`, ...). Used in the auto-suggest path
    /// so users see mozc-style symbol variants without pressing Space first.
    pub(super) fn lookup_rewriter_variants(&self, reading: &str) -> Vec<Candidate> {
        let source_label = CandidateSource::Rewriter.label().to_string();
        self.converters
            .rewriters
            .rewrite_all(&[reading.to_string()])
            .into_iter()
            .map(|(text, description)| Candidate {
                text,
                reading: Some(reading.to_string()),
                source_label: Some(source_label.clone()),
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
    pub(super) fn process_key_conversion(&mut self, key: &KeyEvent) -> EngineResult {
        match key.keysym {
            Keysym::RETURN => self.commit_conversion(),
            Keysym::ESCAPE => self.cancel_conversion(),
            Keysym::SPACE | Keysym::DOWN | Keysym::TAB => self.next_candidate(),
            Keysym::UP => self.prev_candidate(),
            Keysym::PAGE_DOWN => self.next_candidate_page(),
            Keysym::PAGE_UP => self.prev_candidate_page(),
            Keysym::BACKSPACE => self.backspace_conversion(),
            _ => {
                // Ctrl+N / Ctrl+P: emacs-style candidate navigation
                if key.modifiers.control_key && !key.modifiers.alt_key {
                    match key.keysym {
                        Keysym::KEY_N | Keysym::KEY_N_UPPER => return self.next_candidate(),
                        Keysym::KEY_P | Keysym::KEY_P_UPPER => return self.prev_candidate(),
                        _ => {}
                    }
                }

                // Check for digit selection (1-9)
                if let Some(digit) = key.keysym.digit_value() {
                    return self.select_candidate_by_digit(digit);
                }

                // Any printable character: commit current conversion and start new input
                if let Some(ch) = key.to_char()
                    && !key.modifiers.control_key
                    && !key.modifiers.alt_key
                {
                    return self.commit_conversion_and_continue(ch);
                }

                EngineResult::not_consumed()
            }
        }
    }

    /// Get selected text and reading from conversion state, or None if not in conversion
    fn selected_conversion_info(&self) -> Option<(String, Option<String>)> {
        match &self.state {
            InputState::Conversion { candidates, .. } => {
                let text = candidates.selected_text().unwrap_or("").to_string();
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

    /// Commit the current conversion
    fn commit_conversion(&mut self) -> EngineResult {
        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        if text.is_empty() {
            return EngineResult::consumed();
        }

        // Skip learning when the buffer is a `:shortcode` query — the
        // reading would be e.g. `:smile`, which isn't a hiragana key
        // and would corrupt the kana-keyed learning cache.
        if self.input_mode != InputMode::Emoji
            && let Some(reading) = &reading
        {
            self.record_learning(reading, &text);
        }

        self.state = InputState::Empty;
        self.input_buf.text.clear();
        self.exit_emoji_mode();

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
            .with_action(EngineAction::Commit(text))
    }

    /// Commit current conversion and then process a new character as fresh input
    fn commit_conversion_and_continue(&mut self, ch: char) -> EngineResult {
        let Some((text, reading)) = self.selected_conversion_info() else {
            return EngineResult::not_consumed();
        };

        if self.input_mode != InputMode::Emoji
            && let Some(reading) = &reading
        {
            self.record_learning(reading, &text);
        }

        self.state = InputState::Empty;
        self.input_buf.text.clear();
        self.exit_emoji_mode();

        // Start new input with the character
        let new_input_result = self.start_input(ch);

        // Combine: commit first, then new input actions
        let mut result = EngineResult::consumed()
            .with_action(EngineAction::Commit(text))
            .with_action(EngineAction::HideCandidates);
        result.actions.extend(new_input_result.actions);
        result
    }

    /// Cancel conversion and return to hiragana
    pub(super) fn cancel_conversion(&mut self) -> EngineResult {
        if !matches!(self.state, InputState::Conversion { .. }) {
            return EngineResult::not_consumed();
        }
        let reading = self.input_buf.text.clone();

        if reading.is_empty() {
            self.state = InputState::Empty;
            self.input_buf.clear();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideCandidates)
                .with_action(EngineAction::HideAuxText);
        }

        // Set up composed_hiragana with the reading
        self.input_buf.text = reading.clone();
        self.input_buf.cursor_pos = self.input_buf.text.chars().count();

        // Reset romaji converter and set output to reading
        self.converters.romaji.reset();
        // We need to push each character to rebuild the state
        for ch in reading.chars() {
            self.converters.romaji.push(ch);
        }

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
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
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
        let mut preedit = Preedit::with_text(selected_text);
        preedit.set_attributes(vec![PreeditAttribute::new(
            0,
            selected_text.chars().count(),
            AttributeType::Highlight,
        )]);

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

#[cfg(test)]
mod punct_chunk_tests {
    use super::punct_chunks;

    fn split(s: &str, max: usize) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        punct_chunks(&chars, max)
            .into_iter()
            .map(|c| c.iter().collect())
            .collect()
    }

    #[test]
    fn no_punctuation_falls_back_to_fixed_chunks() {
        assert_eq!(split("あいうえお", 2), vec!["あい", "うえ", "お"]);
    }

    #[test]
    fn breaks_after_a_comma() {
        assert_eq!(split("あ、いう", 10), vec!["あ、", "いう"]);
    }

    #[test]
    fn consecutive_punctuation_stays_together() {
        assert_eq!(split("あ！？い", 10), vec!["あ！？", "い"]);
    }

    #[test]
    fn punctuation_run_spills_when_over_cap() {
        // Only what fits stays attached; the rest spills to the next chunk.
        assert_eq!(split("。。。", 2), vec!["。。", "。"]);
    }

    #[test]
    fn clause_longer_than_cap_hard_breaks() {
        assert_eq!(split("あいうえお、", 3), vec!["あいう", "えお、"]);
    }

    #[test]
    fn multiple_clauses_each_become_a_chunk() {
        assert_eq!(split("あ、い。う", 10), vec!["あ、", "い。", "う"]);
    }
}

#[cfg(test)]
mod plan_tests {
    use super::ChunkPlan;

    /// Build a `ChunkPlan` from chunk char-lengths and the new text. The old
    /// text is reconstructed as `0..old_len` filler chars, and the new text as
    /// `new` — only the diff positions matter, so distinct chars suffice.
    fn plan(old_lens: &[usize], old_text: &str, new_text: &str, chunk_len: usize) -> ChunkPlan {
        let old: Vec<char> = old_text.chars().collect();
        let new: Vec<char> = new_text.chars().collect();
        assert_eq!(
            old.len(),
            old_lens.iter().sum::<usize>(),
            "old_lens vs old_text"
        );
        ChunkPlan::compute(old_lens, &old, &new, chunk_len)
    }

    #[test]
    fn fresh_buffer_reconverts_everything() {
        // No previous chunking → whole buffer is the changed middle.
        let p = plan(&[], "", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 0,
                trail_count: 0,
                mid_start: 0,
                mid_end: 4
            }
        );
    }

    #[test]
    fn append_after_full_chunk_reuses_all_leading() {
        // [ab][cd] + "e": both full chunks reused, only "e" is middle.
        let p = plan(&[2, 2], "abcd", "abcde", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 2,
                trail_count: 0,
                mid_start: 4,
                mid_end: 5
            }
        );
    }

    #[test]
    fn append_after_nonfull_chunk_reopens_it() {
        // [ab][c] + "d": the non-full last chunk is reopened so "cd" merges.
        let p = plan(&[2, 1], "abc", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 1,
                trail_count: 0,
                mid_start: 2,
                mid_end: 4
            }
        );
    }

    #[test]
    fn middle_insert_reuses_both_neighbors() {
        // [ab][cd][ef], insert X at pos 3 → only the middle chunk is rebuilt.
        let p = plan(&[2, 2, 2], "abcdef", "abcXdef", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 1,
                trail_count: 1,
                mid_start: 2,
                mid_end: 5
            }
        );
    }

    #[test]
    fn delete_leading_char_keeps_suffix() {
        // [ab][cd], delete 'a' → "bcd": "cd" stays as a reused suffix chunk.
        let p = plan(&[2, 2], "abcd", "bcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 0,
                trail_count: 1,
                mid_start: 0,
                mid_end: 1
            }
        );
    }

    #[test]
    fn unchanged_text_reconverts_nothing() {
        // Same text (e.g. a refresh with no edit) → empty middle, all reused.
        let p = plan(&[2, 2], "abcd", "abcd", 2);
        assert_eq!(
            p,
            ChunkPlan {
                lead_count: 2,
                trail_count: 0,
                mid_start: 4,
                mid_end: 4
            }
        );
    }
}
