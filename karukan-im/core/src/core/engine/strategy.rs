//! Conversion strategy determination and adaptive model selection

use crate::config::settings::StrategyMode;

use super::*;

/// Pure function to determine conversion strategy from the reading length
/// (in chars), adaptive flag, and configuration.
///
/// This is separated from `InputMethodEngine` to enable unit testing without model instances.
///
/// `adaptive_use_light_model` is set by the engine when the main model's last
/// conversion exceeded `max_latency_ms`. It is reset when a new word begins.
pub(super) fn determine_conversion_strategy(
    reading_chars: usize,
    num_candidates: usize,
    has_light_model: bool,
    adaptive_use_light_model: bool,
    config: &EngineConfig,
) -> ConversionStrategy {
    match config.strategy {
        StrategyMode::Adaptive => determine_adaptive_strategy(
            reading_chars,
            num_candidates,
            has_light_model,
            adaptive_use_light_model,
            config,
        ),
        StrategyMode::Light => {
            // Light mode: light model is loaded into the main slot.
            // Auto-suggest → MainModelOnly (greedy), Space → MainModelBeam (beam search)
            if num_candidates == 1 {
                ConversionStrategy::MainModelOnly
            } else {
                ConversionStrategy::MainModelBeam {
                    beam_width: num_candidates.min(config.beam_width),
                }
            }
        }
        StrategyMode::Main => {
            // Main mode: always use main model greedy only
            ConversionStrategy::MainModelOnly
        }
    }
}

/// Adaptive strategy: dynamically switch between main and light models based on latency.
fn determine_adaptive_strategy(
    reading_chars: usize,
    num_candidates: usize,
    has_light_model: bool,
    adaptive_use_light_model: bool,
    config: &EngineConfig,
) -> ConversionStrategy {
    if !has_light_model {
        return ConversionStrategy::MainModelOnly;
    }

    if num_candidates == 1 {
        // Auto-suggest: adapt based on measured latency
        if adaptive_use_light_model {
            ConversionStrategy::LightModelOnly
        } else {
            ConversionStrategy::MainModelOnly
        }
    } else {
        // Explicit conversion (Space key)
        if adaptive_use_light_model {
            // Main model was too slow — beam on the light model alone
            // (the light half of ParallelBeam), keeping the candidate
            // count through the downgrade
            ConversionStrategy::LightModelBeam {
                beam_width: num_candidates.min(config.beam_width),
            }
        } else if reading_chars <= config.chunk_chars {
            // Fits one chunk: parallel beam search. Explicit conversion
            // splits its reading on the chunk grid first, so this is the
            // normal path.
            ConversionStrategy::ParallelBeam {
                beam_width: num_candidates.min(config.beam_width),
            }
        } else {
            // Backstop for a beam request wider than the window (no
            // caller does this today): light model, greedy.
            ConversionStrategy::LightModelOnly
        }
    }
}

impl InputMethodEngine {
    /// Determine the conversion strategy based on the reading length (in
    /// chars), adaptive latency flag, and configuration.
    ///
    /// Char-based on purpose: `chunk_chars` is the unit the beam span is
    /// bounded by, so a span always qualifies for the beam here.
    pub(super) fn determine_strategy(
        &self,
        reading: &str,
        num_candidates: usize,
    ) -> ConversionStrategy {
        let has_light_model = self.converters.light_kanji.is_some();
        if self.converters.kanji.is_none() {
            return ConversionStrategy::MainModelOnly;
        }

        determine_conversion_strategy(
            reading.chars().count(),
            num_candidates,
            has_light_model,
            self.metrics.adaptive_use_light_model,
            &self.config,
        )
    }

    /// Update the adaptive switching flag from a measured main-model greedy
    /// latency. Only an actual main greedy run may feed this: a beam's
    /// one-shot cost must not read as "the main model is too slow" and
    /// downgrade the rest of the word (flipping every cache key with it).
    pub(super) fn update_adaptive_model_flag(&mut self, main_ms: u64) {
        // Only Adaptive mode uses the adaptive flag
        if self.config.strategy != StrategyMode::Adaptive {
            return;
        }
        if self.config.max_latency_ms == 0 || self.converters.light_kanji.is_none() {
            return;
        }
        self.metrics.adaptive_use_light_model = main_ms > self.config.max_latency_ms;
    }
}
