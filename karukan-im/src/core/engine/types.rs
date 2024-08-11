//! Type definitions for the IME engine

use karukan_engine::{Dictionary, KanaKanjiConverter, RomajiConverter};

use crate::config::settings::StrategyMode;

use super::super::candidate::CandidateList;
use super::super::preedit::Preedit;

/// Action to be performed by the framework/UI layer
#[derive(Debug, Clone)]
pub enum EngineAction {
    /// Update the preedit display
    UpdatePreedit(Preedit),
    /// Show the candidate window with candidates
    ShowCandidates(CandidateList),
    /// Hide the candidate window
    HideCandidates,
    /// Commit text to the application
    Commit(String),
    /// Update auxiliary text (e.g., reading hint, mode indicator)
    UpdateAuxText(String),
    /// Hide auxiliary text
    HideAuxText,
}

/// Result of processing a key event
#[derive(Debug, Clone, Default)]
pub struct EngineResult {
    /// Whether the key was consumed by the IME
    pub consumed: bool,
    /// Actions to perform
    pub actions: Vec<EngineAction>,
}

impl EngineResult {
    pub fn consumed() -> Self {
        Self {
            consumed: true,
            actions: Vec::new(),
        }
    }

    pub fn not_consumed() -> Self {
        Self {
            consumed: false,
            actions: Vec::new(),
        }
    }

    pub fn with_action(mut self, action: EngineAction) -> Self {
        self.actions.push(action);
        self
    }
}

/// Surrounding text context from the editor (text around the cursor)
#[derive(Debug, Clone)]
pub(in crate::core) struct SurroundingContext {
    /// Text before the cursor (None if empty)
    pub left: Option<String>,
    /// Text after the cursor (None if empty)
    pub right: Option<String>,
}

/// Configuration for the IME engine
#[derive(Debug, Clone)]
pub struct EngineConfig {
    /// Number of conversion candidates for explicit conversion (Space key)
    pub num_candidates: usize,
    /// Maximum context length to display
    pub display_context_len: usize,
    /// Maximum context length for API calls (to avoid overflow)
    pub max_api_context_len: usize,
    /// Token count threshold for beam search (at or below → beam, above → greedy)
    pub short_input_threshold: usize,
    /// Beam width for short input
    pub beam_width: usize,
    /// Maximum acceptable latency in milliseconds for auto-suggest (0 = disabled)
    /// When a main model conversion exceeds this, the engine adaptively switches to light_model
    pub max_latency_ms: u64,
    /// Conversion strategy mode (adaptive, light, main)
    pub strategy: StrategyMode,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            num_candidates: 3, // Space conversion: beam search with 3 candidates
            display_context_len: 10,
            max_api_context_len: 10,
            short_input_threshold: 10,
            beam_width: 3,
            max_latency_ms: 100,
            strategy: StrategyMode::default(),
        }
    }
}

/// Converter bundle: romaji → hiragana, kana → kanji (main + light)
pub(in crate::core) struct Converters {
    /// Romaji to hiragana converter
    pub romaji: RomajiConverter,
    /// Kanji converter (lazy loaded)
    pub kanji: Option<KanaKanjiConverter>,
    /// Light model for beam search
    pub light_kanji: Option<KanaKanjiConverter>,
}

/// Input mode for the IME engine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputMode {
    /// Hiragana mode (default) — romaji is converted to hiragana
    Hiragana,
    /// Katakana mode — preedit displays katakana instead of hiragana
    Katakana,
    /// Alphabet (direct input) mode — characters bypass romaji conversion
    Alphabet,
}

/// Live conversion state: enabled flag and current converted text
#[derive(Debug, Clone, Default)]
pub(in crate::core) struct LiveConversion {
    /// Whether live conversion is enabled (toggled via Ctrl+Shift+L)
    pub enabled: bool,
    /// Converted text (non-empty when live conversion produced a result)
    pub text: String,
}

/// Dictionary store: system, user, and future cache dictionaries
#[derive(Default)]
pub(in crate::core) struct Dictionaries {
    /// System dictionary for yada double-array trie lookup
    pub system: Option<Dictionary>,
    /// User dictionary (merged from user_dict_paths)
    pub user: Option<Dictionary>,
}

/// Conversion model dispatch strategy based on input length
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core) enum ConversionStrategy {
    /// Short input: main model greedy + light model beam search (parallel)
    ParallelBeam { beam_width: usize },
    /// Long input: light model greedy only (skip slow main model)
    LightModelOnly,
    /// No light model: main model greedy only
    MainModelOnly,
    /// Main model beam search (used in Light strategy mode where light model occupies main slot)
    MainModelBeam { beam_width: usize },
}

/// Timing and adaptive model selection metrics for conversion
#[derive(Debug, Clone, Default)]
pub(in crate::core) struct ConversionMetrics {
    /// Last conversion time in milliseconds (inference only)
    pub conversion_ms: u64,
    /// Last process_key time in milliseconds (input to result, end-to-end)
    pub process_key_ms: u64,
    /// Display name of the model used for the last conversion
    pub model_name: String,
    /// Adaptive flag: set when the main model exceeded max_latency_ms
    /// Reset when a new word begins (Empty state)
    pub adaptive_use_light_model: bool,
}
