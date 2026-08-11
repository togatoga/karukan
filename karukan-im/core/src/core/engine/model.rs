//! Model dispatch and the conversion cache in front of it.
//!
//! Every model call in the engine goes through here: strategy dispatch,
//! the per-computation cache, and the tail-window conversion that Space's
//! mixed list and the AI view share.

use std::collections::HashSet;
use std::time::Instant;

use tracing::debug;

use super::chunk::is_japanese;
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
        let katakana = karukan_engine::hiragana_to_katakana(reading);
        let strategy = self.determine_strategy(reading, num_candidates);

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

    /// the cost stays bounded however long the reading grows. The head of
    /// the list is the whole-reading grid replay — the exact text live
    /// typing displays — so the window's raw char cut (which can land
    /// mid-word) never degrades the visible top-1. Candidates equal to the
    /// raw reading are dropped: an empty result means "no model suggestion".
    pub(super) fn windowed_model_candidates(
        &mut self,
        reading: &str,
        num_candidates: usize,
    ) -> Vec<String> {
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
}
