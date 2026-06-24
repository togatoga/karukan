//! Type definitions for the IME engine

use karukan_engine::{Dictionary, KanaKanjiConverter, RewriterChain, RomajiConverter};

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
    /// Maximum reading length (chars) converted by the model in a single call.
    /// The composing buffer is split into chunks of at most this many chars so
    /// live-conversion latency stays bounded for long input. See
    /// [`ComposingChunk`] and `chunked_auto_suggest`.
    pub composing_chunk_len: usize,
    /// Token count threshold for beam search (at or below → beam, above → greedy)
    pub short_input_threshold: usize,
    /// Beam width for short input
    pub beam_width: usize,
    /// Maximum acceptable latency in milliseconds for auto-suggest (0 = disabled)
    /// When a main model conversion exceeds this, the engine adaptively switches to light_model
    pub max_latency_ms: u64,
    /// Conversion strategy mode (adaptive, light, main)
    pub strategy: StrategyMode,
    /// Whether live conversion is enabled at engine startup
    pub live_conversion: bool,
}

impl EngineConfig {
    /// Build an engine config from user settings (config.toml).
    /// Shared by the fcitx5 FFI and the stdio JSON-RPC server.
    pub fn from_settings(settings: &crate::config::Settings) -> Self {
        Self {
            num_candidates: settings.conversion.num_candidates,
            display_context_len: 10,
            max_api_context_len: if settings.conversion.use_context {
                settings.conversion.max_context_length
            } else {
                0
            },
            composing_chunk_len: settings.conversion.composing_chunk_len,
            short_input_threshold: settings.conversion.short_input_threshold,
            beam_width: settings.conversion.beam_width,
            max_latency_ms: settings.conversion.max_latency_ms,
            strategy: settings.conversion.strategy,
            live_conversion: settings.conversion.live_conversion,
        }
    }
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            num_candidates: 3, // Space conversion: beam search with 3 candidates
            display_context_len: 10,
            max_api_context_len: 10,
            composing_chunk_len: 30,
            short_input_threshold: 10,
            beam_width: 3,
            max_latency_ms: 100,
            strategy: StrategyMode::default(),
            live_conversion: false,
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
    /// Candidate rewriters (half-width katakana, symbol variants)
    pub rewriters: RewriterChain,
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
    /// Emoji shortcode mode — entered by typing `:` from Empty state.
    /// Behaves like [`InputMode::Alphabet`] (ASCII inserted directly,
    /// no romaji conversion) but auto-exits back to [`InputMode::Hiragana`]
    /// on commit/cancel so the next word lands in kana mode without the
    /// user having to toggle anything. The `EmojiRewriter` picks up the
    /// `:`-prefixed input from the candidate-build pipeline and surfaces
    /// emoji candidates as the user types.
    Emoji,
}

/// One internal chunk of the composing buffer (at most
/// `EngineConfig::composing_chunk_len` reading chars) together with its cached
/// model conversion.
///
/// Chunks are an internal optimization only — the user always sees the
/// concatenation of every chunk's `converted` text as one continuous preedit;
/// there are no visible bunsetsu boundaries. Splitting the reading bounds each
/// model call to N chars so live-conversion latency stays flat for long input.
///
/// The left context (lctx) a chunk was converted with is *not* stored: it is
/// just the editor surrounding text plus the `converted` text of the preceding
/// chunks, so it is derived on demand via `chunk_lctx` instead of duplicated
/// here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(in crate::core) struct ComposingChunk {
    /// Hiragana reading for this chunk (≤ N chars).
    pub reading: String,
    /// Model conversion of `reading` — this chunk's slice of the live preedit.
    /// Falls back to `reading` when the model yields nothing.
    pub converted: String,
}

/// Live conversion state: enabled flag and current converted text
#[derive(Debug, Clone, Default)]
pub(in crate::core) struct LiveConversion {
    /// Whether live conversion is enabled (toggled via Ctrl+Shift+L)
    pub enabled: bool,
    /// Converted text (non-empty when live conversion produced a result)
    pub text: String,
}

impl LiveConversion {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            text: String::new(),
        }
    }
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
    /// Conversion time of the current call in milliseconds (inference only);
    /// reset to 0 at the start of each key/selection so it never carries
    /// over from a previous keystroke
    pub conversion_ms: u64,
    /// Last process_key time in milliseconds (input to result, end-to-end)
    pub process_key_ms: u64,
    /// Display name of the model used for the last conversion
    pub model_name: String,
    /// Adaptive flag: set when the main model exceeded max_latency_ms
    /// Reset when a new word begins (Empty state)
    pub adaptive_use_light_model: bool,
}
