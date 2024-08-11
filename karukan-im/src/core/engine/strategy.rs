//! Conversion strategy determination and adaptive model selection

use tracing::debug;

use crate::config::settings::StrategyMode;

use super::*;

/// Pure function to determine conversion strategy from token counts, adaptive flag,
/// and configuration.
///
/// This is separated from `InputMethodEngine` to enable unit testing without model instances.
///
/// `adaptive_use_light_model` is set by the engine when the main model's last
/// conversion exceeded `max_latency_ms`. It is reset when a new word begins.
pub(super) fn determine_conversion_strategy(
    reading_tokens: usize,
    num_candidates: usize,
    has_light_model: bool,
    adaptive_use_light_model: bool,
    config: &EngineConfig,
) -> ConversionStrategy {
    match config.strategy {
        StrategyMode::Adaptive => determine_adaptive_strategy(
            reading_tokens,
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
    reading_tokens: usize,
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
            // Main model was too slow — use light model only
            ConversionStrategy::LightModelOnly
        } else if reading_tokens <= config.short_input_threshold {
            // Short input + main model is fast enough: parallel beam search
            ConversionStrategy::ParallelBeam {
                beam_width: num_candidates.min(config.beam_width),
            }
        } else {
            // Long input: proactively use light model
            ConversionStrategy::LightModelOnly
        }
    }
}

impl InputMethodEngine {
    /// Determine the conversion strategy based on input token counts, adaptive latency
    /// flag, and configuration.
    ///
    /// Counts tokens using the main model's tokenizer and delegates to
    /// `determine_conversion_strategy` for the actual decision logic.
    pub(super) fn determine_strategy(
        &self,
        reading: &str,
        num_candidates: usize,
    ) -> ConversionStrategy {
        let has_light_model = self.converters.light_kanji.is_some();
        let katakana = karukan_engine::kana::hiragana_to_katakana(reading);

        // Count tokens using main model's tokenizer
        let Some(converter) = &self.converters.kanji else {
            return ConversionStrategy::MainModelOnly;
        };

        let reading_tokens = match converter.count_input_tokens(&katakana) {
            Ok(n) => n,
            Err(e) => {
                debug!(
                    "Failed to count reading tokens: {}, fallback to MainModelOnly",
                    e
                );
                return ConversionStrategy::MainModelOnly;
            }
        };

        determine_conversion_strategy(
            reading_tokens,
            num_candidates,
            has_light_model,
            self.metrics.adaptive_use_light_model,
            &self.config,
        )
    }

    /// Update the adaptive model switching flag based on the strategy used and
    /// measured latency. Only updates when the main model was involved.
    pub(super) fn update_adaptive_model_flag(&mut self, strategy: &ConversionStrategy) {
        // Only Adaptive mode uses the adaptive flag
        if self.config.strategy != StrategyMode::Adaptive {
            return;
        }
        if self.config.max_latency_ms == 0 || self.converters.light_kanji.is_none() {
            return;
        }
        match strategy {
            ConversionStrategy::MainModelOnly | ConversionStrategy::ParallelBeam { .. } => {
                self.metrics.adaptive_use_light_model =
                    self.metrics.conversion_ms > self.config.max_latency_ms;
            }
            ConversionStrategy::LightModelOnly | ConversionStrategy::MainModelBeam { .. } => {
                // Don't update — light model latency doesn't reflect main model speed
            }
        }
    }
}
