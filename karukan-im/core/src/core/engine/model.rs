//! Model dispatch and the conversion cache in front of it.
//!
//! Every model call in the engine goes through here: strategy dispatch,
//! the per-computation cache, and the split conversion that Space's
//! mixed list and the AI view share.

use std::collections::HashSet;
use std::time::Instant;

use tracing::debug;

use super::*;

impl InputMethodEngine {
    /// Kana-kanji conversion via the model(s). Every model call goes through
    /// the conversion cache, so re-running unchanged chunks is free.
    /// `api_context` is the left context fed to the model.
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
        let strategy = self.determine_strategy(reading, num_candidates);
        let katakana = karukan_engine::hiragana_to_katakana(reading);

        debug!(
            "convert: reading=\"{}\" api_context=\"{}\" candidates={} strategy={:?}",
            reading, api_context, num_candidates, strategy
        );

        let start = Instant::now();
        // Each arm yields the candidates plus the main model's *greedy*
        // latency when it actually ran — the only measurement the adaptive
        // gate may act on.
        let (candidates, main_ms) = match &strategy {
            ConversionStrategy::ParallelBeam { beam_width } => {
                self.run_parallel_beam(&katakana, api_context, *beam_width)
            }
            ConversionStrategy::LightModelOnly => (
                self.cached_convert(ModelRole::Light, 1, &katakana, api_context)
                    .0,
                None,
            ),
            ConversionStrategy::LightModelBeam { beam_width } => (
                self.cached_convert(ModelRole::Light, *beam_width, &katakana, api_context)
                    .0,
                None,
            ),
            ConversionStrategy::MainModelOnly => {
                self.cached_convert(ModelRole::Main, 1, &katakana, api_context)
            }
            ConversionStrategy::MainModelBeam { beam_width } => (
                self.cached_convert(ModelRole::Main, *beam_width, &katakana, api_context)
                    .0,
                None,
            ),
        };

        self.metrics.conversion_ms = start.elapsed().as_millis() as u64;
        if let Some(ms) = main_ms {
            self.update_adaptive_model_flag(ms);
        }
        self.metrics.model_name = self.model_name_for(&strategy);

        candidates
    }

    /// One model computation, served from the cache when possible. Returns
    /// the candidates and the inference time — `None` when nothing ran (a
    /// cache hit, or no such model loaded), so a caller can tell a
    /// measurement from a replay.
    ///
    /// Empty results are not cached: they usually mean a conversion error,
    /// and pinning one would keep replaying the failure.
    fn cached_convert(
        &mut self,
        model: ModelRole,
        beam_width: usize,
        katakana: &str,
        lctx: &str,
    ) -> (Vec<String>, Option<u64>) {
        // The lookup comes before the converter check: a hit needs no model.
        if let Some(candidates) = self.cached_result(model, beam_width, katakana, lctx) {
            debug!("convert: cache hit {:?} beam={}", model, beam_width);
            return (candidates, None);
        }
        let Some(converter) = self.converter_for(model) else {
            return (Vec::new(), None);
        };
        let key = Self::cache_key(model, beam_width, katakana, lctx);
        let start = Instant::now();
        let candidates = converter
            .convert(katakana, lctx, beam_width)
            .unwrap_or_default();
        let elapsed = start.elapsed().as_millis() as u64;
        if !candidates.is_empty() {
            self.conversion_cache.insert(key, candidates.clone());
        }
        (candidates, Some(elapsed))
    }

    /// Cached result for a computation, if any.
    ///
    /// A light-model request also accepts the main model's entry for the same
    /// reading and beam width: the main model is the better of the two, so
    /// substituting it can only improve the result, and it costs no
    /// inference. This is what keeps a latency downgrade from re-running
    /// every chunk the main model had already converted — backspacing
    /// through a word after the downgrade stays free. Never the reverse: a
    /// main-model request must not be served a light-model result.
    pub(super) fn cached_result(
        &mut self,
        model: ModelRole,
        beam_width: usize,
        katakana: &str,
        lctx: &str,
    ) -> Option<Vec<String>> {
        let key = Self::cache_key(model, beam_width, katakana, lctx);
        if let Some(candidates) = self.conversion_cache.get(&key) {
            return Some(candidates);
        }
        if model == ModelRole::Light {
            let main_key = Self::cache_key(ModelRole::Main, beam_width, katakana, lctx);
            return self.conversion_cache.get(&main_key);
        }
        None
    }

    fn cache_key(
        model: ModelRole,
        beam_width: usize,
        katakana: &str,
        lctx: &str,
    ) -> ConversionCacheKey {
        ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: lctx.to_string(),
            model,
            beam_width,
        }
    }

    fn converter_for(&self, model: ModelRole) -> Option<&KanaKanjiConverter> {
        match model {
            ModelRole::Main => self.converters.kanji.as_ref(),
            ModelRole::Light => self.converters.light_kanji.as_ref(),
        }
    }

    /// ParallelBeam: main greedy and light beam at the same time, merged.
    /// Both halves are ordinary cached computations, so each is served from
    /// the cache when live typing or another strategy already ran it, and
    /// only the missing halves are spawned. Returns the merged candidates
    /// and the main half's latency (`None` when it didn't run).
    fn run_parallel_beam(
        &mut self,
        katakana: &str,
        lctx: &str,
        beam_width: usize,
    ) -> (Vec<String>, Option<u64>) {
        let main_key = Self::cache_key(ModelRole::Main, 1, katakana, lctx);
        let light_key = Self::cache_key(ModelRole::Light, beam_width, katakana, lctx);
        let cached_main = self.cached_result(ModelRole::Main, 1, katakana, lctx);
        let cached_light = self.cached_result(ModelRole::Light, beam_width, katakana, lctx);
        let (Some(main_converter), Some(light_converter)) = (
            self.converter_for(ModelRole::Main),
            self.converter_for(ModelRole::Light),
        ) else {
            return (Vec::new(), None);
        };

        let (computed_main, computed_light) = std::thread::scope(|s| {
            let h_main = cached_main.is_none().then(|| {
                s.spawn(|| {
                    let start = Instant::now();
                    let result = main_converter
                        .convert(katakana, lctx, 1)
                        .unwrap_or_default();
                    (result, start.elapsed().as_millis() as u64)
                })
            });
            let h_light = cached_light.is_none().then(|| {
                s.spawn(|| {
                    light_converter
                        .convert(katakana, lctx, beam_width)
                        .unwrap_or_default()
                })
            });
            (
                h_main.map(|h| h.join().unwrap_or_default()),
                h_light.map(|h| h.join().unwrap_or_default()),
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
        let light = match computed_light {
            Some(result) => {
                if !result.is_empty() {
                    self.conversion_cache.insert(light_key, result.clone());
                }
                result
            }
            None => cached_light.unwrap_or_default(),
        };

        (
            Self::merge_candidates_dedup(main_top1, light, beam_width),
            main_ms,
        )
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

    /// Merge two candidate lists, primary first, dropping duplicates.
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

    /// the cost stays bounded however long the reading grows. An empty
    /// result means "the model produced nothing"; a candidate equal to the
    /// reading is a real answer (kana-only words convert to themselves).
    pub(super) fn model_candidates(&mut self, reading: &str, num_candidates: usize) -> Vec<String> {
        if !karukan_engine::contains_kana(reading) {
            return Vec::new();
        }
        let base_ctx = self.truncate_context_for_api();
        let chars: Vec<char> = reading.chars().collect();
        let span_start = self.beam_span_start(&chars);
        let prefix = self.convert_on_chunk_grid(&chars[..span_start], &base_ctx);

        // Nothing to beam (the reading ends outside Japanese): the grid
        // conversion is the only candidate.
        if span_start >= chars.len() {
            return if prefix == reading {
                // Nothing converted, so there is no model answer here.
                Vec::new()
            } else {
                vec![prefix]
            };
        }

        // The span is the last chunk, so its main-model greedy is exactly
        // what the whole-reading grid would compute for it: `prefix` plus
        // that greedy IS the head, no separate pass. ParallelBeam runs it
        // alongside the light beam and puts it first, so the head costs no
        // extra wall time instead of a serial conversion before the beam.
        let span: String = chars[span_start..].iter().collect();
        let lctx = self.lctx_for(&base_ctx, &prefix);
        // An empty beam means the model produced nothing (unavailable, or a
        // conversion error), which is what "no model suggestion" means to
        // the callers. A beam that returns the reading unchanged is a real
        // answer — words that stay in kana (きゃりーぱみゅぱみゅ) convert to
        // themselves — so it rides like any other candidate.
        self.run_kana_kanji_conversion(&span, &lctx, num_candidates)
            .into_iter()
            .map(|tail| format!("{prefix}{tail}"))
            .collect()
    }

    /// Start of the beam span: the trailing Japanese chunks fitting
    /// `beam_chars`, at least the last one. Only a grid boundary will do —
    /// cutting anywhere else leaves a prefix live conversion never
    /// converted, which costs an extra inference and shows a seam the user
    /// never saw, and could feed a digit run to the model.
    pub(super) fn beam_span_start(&self, chars: &[char]) -> usize {
        self.trailing_chunks_start(chars, self.config.beam_chars)
    }
}
