//! IME Engine - the core state machine and input processing
//!
//! This module contains the main `InputMethodEngine` struct that coordinates between
//! the romaji converter, kanji converter, and manages the IME state.

mod chunk;
mod conversion;
mod cursor;
mod display;
mod init;
mod input;
mod input_buffer;
mod mode;
mod strategy;
mod types;

pub use types::*;

use input_buffer::InputBuffer;

#[cfg(test)]
mod tests;

use karukan_engine::{
    Dictionary, KanaKanjiConverter, LearningCache, RewriterChain, RomajiConverter,
};
use tracing::{debug, trace};

use super::candidate::{Candidate, CandidateList};
use super::keycode::{KeyEvent, Keysym};
use super::preedit::Preedit;
use super::state::InputState;
use crate::config::settings::Settings;

/// Source of a conversion candidate
#[derive(Debug, Clone, PartialEq, Eq)]
enum CandidateSource {
    /// User dictionary lookup
    UserDictionary,
    /// Learning cache (user history)
    Learning,
    /// Model inference result
    Model,
    /// System dictionary lookup (also covers reading→symbol lookups via
    /// mozc's symbol.tsv — they're treated as just another dictionary).
    Dictionary,
    /// Rewriter-generated variant (half-width katakana, symbol)
    Rewriter,
    /// Hiragana/katakana fallback
    Fallback,
}

impl CandidateSource {
    fn label(&self) -> &'static str {
        match self {
            CandidateSource::UserDictionary => "\u{1F464} \u{30E6}\u{30FC}\u{30B6}\u{30FC}", // 👤 ユーザー
            CandidateSource::Learning => "\u{1F4DD} \u{5B66}\u{7FD2}", // 📝 学習
            CandidateSource::Model => "\u{1F916} AI",                  // 🤖 AI
            CandidateSource::Dictionary => "\u{1F4DA} \u{8F9E}\u{66F8}", // 📚 辞書
            CandidateSource::Rewriter => "\u{1F504} \u{5909}\u{63DB}", // 🔄 変換
            CandidateSource::Fallback => "",
        }
    }
}

/// A conversion candidate tagged with its source and an optional description.
///
/// Built up internally during candidate construction; later mapped onto the
/// public `Candidate` (where `source.label()` becomes `source_label` and this
/// `description` becomes `description`).
#[derive(Debug, Clone)]
struct AnnotatedCandidate {
    text: String,
    source: CandidateSource,
    /// Override reading (e.g. from prefix_lookup where the full reading differs from input)
    reading: Option<String>,
    /// Per-candidate description (e.g. `三点リーダ` for `…`,
    /// `[全]英大文字` for `ＡＢＣ`). Surfaced as the mozc-style right-side
    /// comment on the candidate; never contains a source label.
    description: Option<String>,
}

impl AnnotatedCandidate {
    fn new(text: impl Into<String>, source: CandidateSource) -> Self {
        Self {
            text: text.into(),
            source,
            reading: None,
            description: None,
        }
    }

    fn with_reading(mut self, reading: Option<String>) -> Self {
        self.reading = reading;
        self
    }

    fn with_description(mut self, description: Option<String>) -> Self {
        self.description = description;
        self
    }
}

/// Resolve a model variant id from settings.
///
/// - `model` is None or empty → default variant from registry
/// - `model` matches a known variant id → that variant
/// - otherwise → error (unknown variant)
pub fn resolve_variant_id(model: Option<&str>) -> anyhow::Result<String> {
    let reg = karukan_engine::kanji::registry();
    match model {
        Some(id) if !id.is_empty() => {
            if reg.find_variant(id).is_some() {
                Ok(id.to_string())
            } else {
                anyhow::bail!("unknown model variant: {}", id)
            }
        }
        _ => Ok(reg.default_model.clone()),
    }
}

/// Keep at most the last `n` characters of `s`.
fn keep_last_chars(s: &str, n: usize) -> String {
    let char_count = s.chars().count();
    if char_count > n {
        s.chars().skip(char_count - n).collect()
    } else {
        s.to_string()
    }
}

/// Keep at most the first `n` characters of `s`.
fn keep_first_chars(s: &str, n: usize) -> String {
    let char_count = s.chars().count();
    if char_count > n {
        s.chars().take(n).collect()
    } else {
        s.to_string()
    }
}

/// The main IME engine
pub struct InputMethodEngine {
    /// Current input state
    state: InputState,
    /// Converters (romaji, kanji, light kanji)
    converters: Converters,
    /// Surrounding text context from the editor (text around cursor)
    surrounding_context: Option<SurroundingContext>,
    /// Engine configuration
    config: EngineConfig,
    /// Conversion timing and adaptive model metrics
    metrics: ConversionMetrics,
    /// Current input mode plus the mode to come back to when a temporary
    /// mode (Emoji, Alphabet) ends — see [`ModeState`]
    mode: ModeState,
    /// Composed input buffer (hiragana text, cursor position)
    input_buf: InputBuffer,
    /// Live conversion state
    live: LiveConversion,
    /// Internal chunking of the composing buffer used by
    /// `chunked_auto_suggest`: a cache of the per-chunk model conversions.
    /// Re-chunking diffs the new buffer against this by common prefix/suffix so
    /// a keystroke only reconverts the chunk it touched, not the whole buffer.
    /// Empty when not composing.
    chunks: Vec<ComposingChunk>,
    /// Dictionaries (system, user)
    dicts: Dictionaries,
    /// Learning cache (user conversion history)
    learning: Option<LearningCache>,
}

impl InputMethodEngine {
    /// Create a new IME engine
    pub fn new() -> Self {
        Self {
            state: InputState::Empty,
            converters: Converters {
                romaji: RomajiConverter::new(),
                kanji: None,
                light_kanji: None,
                rewriters: RewriterChain::default_chain(),
            },
            surrounding_context: None,
            config: EngineConfig::default(),
            metrics: ConversionMetrics::default(),
            mode: ModeState::default(),
            input_buf: InputBuffer::new(),
            live: LiveConversion::default(),
            chunks: Vec::new(),
            dicts: Dictionaries::default(),
            learning: None,
        }
    }

    /// Create with configuration
    pub fn with_config(config: EngineConfig) -> Self {
        Self {
            live: LiveConversion::new(config.live_conversion),
            config,
            ..Self::new()
        }
    }

    /// Conversion (inference) time of the last `process_key` /
    /// `select_candidate_on_page` call in milliseconds; 0 when that call
    /// ran no conversion.
    pub fn last_conversion_ms(&self) -> u64 {
        self.metrics.conversion_ms
    }

    /// Get last process_key time in milliseconds (input to result, end-to-end)
    pub fn last_process_key_ms(&self) -> u64 {
        self.metrics.process_key_ms
    }

    /// Get the model name being used
    pub fn model_name(&self) -> String {
        let main = self
            .converters
            .kanji
            .as_ref()
            .map(|c| c.model_display_name());
        let sub = self
            .converters
            .light_kanji
            .as_ref()
            .map(|c| c.model_display_name());
        match (main, sub) {
            (Some(m), Some(s)) => format!("{}+{}", m, s),
            (Some(m), None) => m.to_string(),
            _ => "unknown".to_string(),
        }
    }

    /// Get the current state
    pub fn state(&self) -> &InputState {
        &self.state
    }

    /// Get the current preedit
    pub fn preedit(&self) -> Option<&Preedit> {
        self.state.preedit()
    }

    /// Get the current candidates
    pub fn candidates(&self) -> Option<&CandidateList> {
        self.state.candidates()
    }

    /// Reset the engine state
    /// Note: surrounding_context is intentionally NOT cleared here.
    /// It is set once at activate() time and should persist through
    /// the session. fcitx5 may send reset events between activate
    /// and the first keyEvent, which would wipe the context.
    pub fn reset(&mut self) {
        self.state = InputState::Empty;
        self.converters.romaji.reset();
        self.mode = ModeState::default();
        self.input_buf.clear();
        self.live.text.clear();
        self.chunks.clear();
        self.metrics = ConversionMetrics::default();
    }

    /// If the display is empty, reset to Empty state and return the result.
    /// Returns None if display is not empty (caller should continue normally).
    fn try_reset_if_empty(&mut self) -> Option<EngineResult> {
        if self.build_input_display().is_empty() {
            self.state = InputState::Empty;
            self.input_buf.clear();
            // Erasing the whole buffer ends the composition: drop the live
            // conversion text and the chunk cache so neither leaks into the
            // next composing session (build_composing_preedit would otherwise
            // render a stale live.text, and the chunk cache would be diffed
            // against a buffer it no longer matches).
            self.live.text.clear();
            self.chunks.clear();
            // Temporary modes (Emoji, Alphabet) are per-composition:
            // erasing back to an empty buffer ends the session, so restore
            // the mode the user was in before entering it (a Katakana-mode
            // user lands back in Katakana, and the next keypress doesn't
            // get treated as a literal emoji-query char).
            self.mode.exit_temporary();
            Some(
                EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                    .with_action(EngineAction::HideCandidates)
                    .with_action(EngineAction::HideAuxText),
            )
        } else {
            None
        }
    }

    /// Update state to Composing with current preedit and romaji buffer, returning the preedit.
    /// Automatically uses live conversion display when `live.text` is non-empty.
    fn set_composing_state(&mut self) -> Preedit {
        let romaji_buffer = self.converters.romaji.buffer().to_string();
        let preedit = self.build_composing_preedit();
        self.state = InputState::Composing {
            preedit: preedit.clone(),
            romaji_buffer,
        };
        preedit
    }

    /// Convert hiragana in input_buf to katakana permanently.
    /// Called when leaving Katakana mode so the preedit doesn't revert.
    fn bake_katakana(&mut self) {
        if !self.input_buf.text.is_empty() {
            self.input_buf.text = karukan_engine::hiragana_to_katakana(&self.input_buf.text);
        }
    }

    /// Flush the romaji buffer and insert result at cursor position
    fn flush_romaji_to_composed(&mut self) {
        if self.converters.romaji.buffer().is_empty() {
            return;
        }
        let prev_output_len = self.converters.romaji.output().chars().count();
        let _flushed = self.converters.romaji.flush();
        // flush() appends converted buffer to output internally
        let new_from_flush: String = self
            .converters
            .romaji
            .output()
            .chars()
            .skip(prev_output_len)
            .collect();
        if !new_from_flush.is_empty() {
            self.input_buf.insert(&new_from_flush);
        }
    }

    /// Set surrounding context from the full text plus a cursor offset in
    /// Unicode scalar values (the unit both fcitx5 and the JSON-RPC
    /// protocol deliver). Splits at the cursor and delegates to
    /// [`Self::set_surrounding_context`].
    pub fn set_surrounding_text_at(&mut self, text: &str, cursor_chars: usize) {
        let byte_offset = text
            .char_indices()
            .nth(cursor_chars)
            .map(|(i, _)| i)
            .unwrap_or(text.len());
        let (left, right) = text.split_at(byte_offset);
        self.set_surrounding_context(left, right);
    }

    /// Set both left and right context from surrounding text (from editor)
    /// left_context: text before cursor
    /// right_context: text after cursor
    pub fn set_surrounding_context(&mut self, left_context: &str, right_context: &str) {
        debug!(
            "set_surrounding_context: left=\"{}\" right=\"{}\"",
            left_context, right_context
        );

        // Strip to current line: left = text after last newline.
        // If cursor is right after a newline, left context is empty.
        let left_context = match left_context.rsplit_once('\n') {
            Some((_, after)) => after,
            None => left_context,
        };
        let right_context = right_context
            .split_once('\n')
            .map_or(right_context, |(before, _)| before);

        if left_context.is_empty() && right_context.is_empty() {
            self.surrounding_context = None;
            return;
        }

        // Truncate left context to max length (keep end)
        let left = if left_context.is_empty() {
            None
        } else {
            Some(keep_last_chars(
                left_context,
                self.config.max_api_context_len,
            ))
        };

        // Truncate right context to max length (keep beginning)
        let right = if right_context.is_empty() {
            None
        } else {
            Some(keep_first_chars(
                right_context,
                self.config.max_api_context_len,
            ))
        };

        self.surrounding_context = Some(SurroundingContext { left, right });
    }

    /// Handle mode toggle keys (Right Alt/Super/Meta/Hyper and the JIS 変換
    /// key): one-way non-Hiragana → Hiragana.
    /// Returns `Some(result)` if the key was handled, `None` if not a mode toggle key.
    fn handle_mode_toggle_key(&mut self, key: &KeyEvent) -> Option<EngineResult> {
        if !key.keysym.is_mode_toggle_key() {
            return None;
        }
        // 変換 is an ordinary key, not a modifier: a modified chord
        // (Ctrl+変換 etc.) may be an app or fcitx5 shortcut, so only the
        // bare press acts as the toggle. The right-modifier keysyms are
        // exempt — their events routinely carry their own modifier state.
        if key.keysym == Keysym::HENKAN && key.modifiers.any() {
            return None;
        }
        // While a conversion is in flight (candidate window open) the
        // toggle is inert: switching modes here would katakana-bake the
        // conversion *reading* (not the preedit) and defeat the Emoji-mode
        // learning guard — the commit path checks the current mode to
        // decide whether the reading is safe to record in the kana-keyed
        // learning cache. Resolve the conversion first, then toggle.
        if matches!(self.state, InputState::Conversion { .. }) {
            return Some(EngineResult::not_consumed());
        }
        // Only consume the key when actually switching; otherwise pass through
        // so the system can properly track modifier state.
        if key.is_press && self.mode.current() != InputMode::Hiragana {
            // Bake katakana before switching so preedit doesn't revert
            if self.mode.current() == InputMode::Katakana {
                self.bake_katakana();
            }
            self.mode.set(InputMode::Hiragana);
            self.flush_romaji_to_composed();
            let aux = self.format_aux_composing();
            if matches!(self.state, InputState::Composing { .. }) {
                let preedit = self.set_composing_state();
                return Some(
                    EngineResult::consumed()
                        .with_action(EngineAction::UpdatePreedit(preedit))
                        .with_action(EngineAction::UpdateAuxText(aux)),
                );
            }
            return Some(EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux)));
        }
        Some(EngineResult::not_consumed())
    }

    /// Process a key event
    pub fn process_key(&mut self, key: &KeyEvent) -> EngineResult {
        // Log modifier key events for debugging key mapping issues
        if key.keysym.is_modifier() {
            debug!(
                "modifier key: keysym=0x{:04x} press={} modifiers={:?}",
                key.keysym.0, key.is_press, key.modifiers
            );
        }

        // Right Alt/Super/Meta/Hyper: one-way non-Hiragana → Hiragana switch
        if let Some(result) = self.handle_mode_toggle_key(key) {
            return result;
        }

        // Modifier-only keys (Shift, Ctrl, Alt_L, Super_L, etc.): pass through
        if key.keysym.is_modifier() {
            return EngineResult::not_consumed();
        }

        // Only process key presses
        if !key.is_press {
            return EngineResult::not_consumed();
        }

        // Ctrl+Shift+L: toggle live conversion (works in all states)
        if key.modifiers.control_key
            && key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_L || key.keysym == Keysym::KEY_L_UPPER)
        {
            return self.toggle_live_conversion();
        }

        // Reset adaptive model flag when starting a new word (first key in Empty state)
        if matches!(self.state, InputState::Empty) {
            self.metrics.adaptive_use_light_model = false;
        }

        trace!(
            "Processing key: {:?} in state: {:?}",
            key.keysym, self.state
        );

        let start = std::time::Instant::now();
        // conversion_ms reports this key only: 0 unless a conversion runs below
        self.metrics.conversion_ms = 0;

        let shift_active = key.modifiers.shift_key;

        let result = match &self.state {
            InputState::Empty => self.process_key_empty(key, shift_active),
            InputState::Composing { .. } => self.process_key_composing(key, shift_active),
            InputState::Conversion { .. } => self.process_key_conversion(key),
        };

        self.metrics.process_key_ms = start.elapsed().as_millis() as u64;

        result
    }

    /// Commit any pending input and return the text
    pub fn commit(&mut self) -> String {
        match &self.state {
            InputState::Empty => String::new(),
            InputState::Composing { .. } => {
                // Flush romaji buffer into composed_hiragana
                self.flush_romaji_to_composed();
                let reading = self.input_buf.text.clone();
                let text = if !self.live.text.is_empty() {
                    self.live.text.clone()
                } else {
                    reading.clone()
                };
                // Record live conversion result in learning cache
                self.record_learning(&reading, &text);
                self.converters.romaji.reset();
                self.input_buf.clear();
                self.live.text.clear();
                self.state = InputState::Empty;
                self.surrounding_context = None;
                text
            }
            InputState::Conversion { .. } => {
                let (text, reading) = self
                    .selected_conversion_info()
                    .expect("state is Conversion");
                // Record conversion result in learning cache
                if let Some(reading) = &reading {
                    self.record_learning(reading, &text);
                }
                self.input_buf.clear();
                self.state = InputState::Empty;
                self.surrounding_context = None;
                text
            }
        }
    }

    /// Commit any pending input as an [`EngineResult`], emitting the same
    /// UI cleanup actions as the key-driven commit path (Enter), so
    /// frontends don't have to pair [`Self::commit`] with manual
    /// preedit/candidate-window teardown.
    pub fn commit_result(&mut self) -> EngineResult {
        let text = self.commit();
        let mut result = EngineResult::consumed();
        if !text.is_empty() {
            result = result.with_action(EngineAction::Commit(text));
        } else {
            result = result.with_action(EngineAction::UpdatePreedit(Preedit::new()));
        }
        result
            .with_action(EngineAction::HideCandidates)
            .with_action(EngineAction::HideAuxText)
    }

    /// Save the learning cache to disk if it has unsaved changes.
    pub fn save_learning(&mut self) {
        if let Some(cache) = &mut self.learning
            && cache.is_dirty()
            && let Some(path) = Settings::learning_file()
        {
            if let Err(e) = cache.save(&path) {
                debug!("Failed to save learning cache: {}", e);
            } else {
                debug!("Learning cache saved to {:?}", path);
            }
        }
    }
}

impl Default for InputMethodEngine {
    fn default() -> Self {
        Self::new()
    }
}
