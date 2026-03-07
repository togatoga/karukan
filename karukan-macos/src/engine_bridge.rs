//! Engine bridge: wraps karukan-im's InputMethodEngine for macOS IMKit integration.
//!
//! This module provides a higher-level wrapper that handles initialization
//! and translates between macOS key events and the engine's KeyEvent type.

use anyhow::Result;
use tracing::debug;

use karukan_im::config::Settings;
use karukan_im::core::engine::resolve_variant_id;
use karukan_im::{EngineResult, InputMethodEngine, KeyEvent};

use crate::keymap;

/// Configuration for the engine bridge, derived from karukan-im Settings.
pub struct BridgeConfig {
    pub settings: Settings,
}

impl Default for BridgeConfig {
    fn default() -> Self {
        Self {
            settings: Settings::load().unwrap_or_default(),
        }
    }
}

/// Bridge between macOS IMKit and karukan-im's InputMethodEngine.
///
/// Uses InputMethodEngine directly (Rust-to-Rust, no FFI overhead).
pub struct EngineBridge {
    engine: InputMethodEngine,
    settings: Settings,
    initialized: bool,
}

impl EngineBridge {
    /// Create a new engine bridge with default settings.
    pub fn new() -> Self {
        let config = BridgeConfig::default();
        Self::with_config(config)
    }

    /// Create a new engine bridge with explicit configuration.
    pub fn with_config(config: BridgeConfig) -> Self {
        use karukan_im::core::engine::EngineConfig;

        let settings = config.settings;
        let engine_config = EngineConfig {
            num_candidates: settings.conversion.num_candidates,
            display_context_len: 10,
            max_api_context_len: if settings.conversion.use_context {
                settings.conversion.max_context_length
            } else {
                0
            },
            short_input_threshold: settings.conversion.short_input_threshold,
            beam_width: settings.conversion.beam_width,
            max_latency_ms: settings.conversion.max_latency_ms,
            strategy: settings.conversion.strategy,
            keybinding_profile: settings.keybinding.profile,
        };
        let engine = InputMethodEngine::with_config(engine_config);

        Self {
            engine,
            settings,
            initialized: false,
        }
    }

    /// Initialize models, dictionaries, and learning cache.
    ///
    /// This should be called during IMKit activateServer. It may take several seconds
    /// on first run (model download from HuggingFace).
    pub fn initialize(&mut self) -> Result<()> {
        if self.initialized {
            return Ok(());
        }

        debug!("EngineBridge: initializing models and dictionaries");

        // Initialize main kanji converter
        let variant_id = resolve_variant_id(self.settings.conversion.model.as_deref())?;
        self.engine
            .init_kanji_converter_with_model(&variant_id, self.settings.conversion.n_threads)?;

        // Initialize light model if configured
        if let Some(light_model) = &self.settings.conversion.light_model {
            let light_id = resolve_variant_id(Some(light_model))?;
            self.engine
                .init_light_kanji_converter(&light_id, self.settings.conversion.n_threads)?;
        }

        // Initialize system dictionary
        self.engine
            .init_system_dictionary(self.settings.conversion.dict_path.as_deref());

        // Initialize user dictionaries
        self.engine.init_user_dictionaries();

        // Initialize learning cache
        self.engine.init_learning_cache(
            self.settings.learning.enabled,
            self.settings.learning.max_entries,
        );

        self.initialized = true;
        debug!("EngineBridge: initialization complete");
        Ok(())
    }

    /// Process a macOS key event through the engine.
    ///
    /// Converts macOS Carbon key code + modifiers to karukan KeyEvent, then delegates
    /// to InputMethodEngine::process_key.
    #[allow(clippy::too_many_arguments)]
    pub fn process_key(
        &mut self,
        keycode: u16,
        unicode_char: Option<char>,
        shift: bool,
        control: bool,
        option: bool,
        command: bool,
        is_press: bool,
    ) -> EngineResult {
        let key_event = keymap::create_key_event(
            keycode,
            unicode_char,
            shift,
            control,
            option,
            command,
            is_press,
        );
        self.engine.process_key(&key_event)
    }

    /// Process a pre-built KeyEvent through the engine.
    pub fn process_key_event(&mut self, key: &KeyEvent) -> EngineResult {
        self.engine.process_key(key)
    }

    /// Commit any pending input text.
    pub fn commit(&mut self) -> String {
        self.engine.commit()
    }

    /// Reset the engine state.
    pub fn reset(&mut self) {
        self.engine.reset();
    }

    /// Save the learning cache to disk.
    pub fn save_learning(&mut self) {
        self.engine.save_learning();
    }

    /// Set surrounding text context for contextual conversion.
    pub fn set_surrounding_context(&mut self, left: &str, right: &str) {
        self.engine.set_surrounding_context(left, right);
    }

    /// Get the current input mode.
    pub fn input_mode(&self) -> karukan_im::InputMode {
        self.engine.input_mode()
    }

    /// Get access to the underlying engine for inspection.
    pub fn engine(&self) -> &InputMethodEngine {
        &self.engine
    }

    /// Check if the engine has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }
}

impl Default for EngineBridge {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bridge_creation() {
        let bridge = EngineBridge::new();
        assert!(!bridge.is_initialized());
    }

    #[test]
    fn test_bridge_process_key_before_init() {
        let mut bridge = EngineBridge::new();
        // Should work even before initialization (no model = no conversion, but romaji works)
        // kVK_ANSI_A = 0x00, unicode 'a'
        let result = bridge.process_key(0x00, Some('a'), false, false, false, false, true);
        // 'a' in Empty state should start composing
        assert!(result.consumed);
    }

    #[test]
    fn test_bridge_escape_resets() {
        let mut bridge = EngineBridge::new();
        // Type 'a' to enter composing state
        bridge.process_key(0x00, Some('a'), false, false, false, false, true);
        // Press Escape (kVK_Escape = 0x35) to reset
        let result = bridge.process_key(0x35, None, false, false, false, false, true);
        assert!(result.consumed);
    }

    #[test]
    fn test_bridge_commit() {
        let mut bridge = EngineBridge::new();
        // Type some text (kVK_ANSI_A = 0x00)
        bridge.process_key(0x00, Some('a'), false, false, false, false, true);
        // Commit
        let text = bridge.commit();
        assert!(!text.is_empty());
    }

    #[test]
    fn test_bridge_reset_after_commit() {
        let mut bridge = EngineBridge::new();
        // Type 'a' to enter composing state
        bridge.process_key(0x00, Some('a'), false, false, false, false, true);
        // Commit + reset
        let text = bridge.commit();
        assert!(!text.is_empty());
        bridge.reset();
        // After reset, typing 'a' again should start fresh composing
        let result = bridge.process_key(0x00, Some('a'), false, false, false, false, true);
        assert!(result.consumed);
    }
}
