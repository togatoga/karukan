//! SKK-style keybinding handling
//!
//! All SKK-specific keybinding logic is isolated in this module.
//! Existing input.rs and mode.rs are not modified with SKK-specific branches.

use crate::config::settings::KeybindingProfile;

use super::*;

impl InputMethodEngine {
    /// SKK keybinding pre-processing. Returns Some(result) if the key was consumed,
    /// None if SKK mode is inactive or the key is not an SKK binding.
    pub(super) fn handle_skk_keybind(&mut self, key: &KeyEvent) -> Option<EngineResult> {
        if self.config.keybinding_profile != KeybindingProfile::Skk {
            return None;
        }

        // Only handle key presses
        if !key.is_press {
            return None;
        }

        // Ctrl+j → enter hiragana mode (from any mode)
        if key.modifiers.control_key
            && !key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_J || key.keysym == Keysym::KEY_J_UPPER)
        {
            return Some(self.skk_enter_hiragana());
        }

        // Shift+Ctrl+q → convert to half-width katakana and commit (must be before Ctrl+q)
        if key.modifiers.control_key
            && key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_Q || key.keysym == Keysym::KEY_Q_UPPER)
            && matches!(self.state, InputState::Composing { .. })
        {
            return Some(self.skk_commit_as_halfwidth_katakana());
        }

        // Ctrl+q → enter half-width katakana mode
        if key.modifiers.control_key
            && !key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_Q || key.keysym == Keysym::KEY_Q_UPPER)
        {
            return Some(self.skk_enter_halfwidth_katakana());
        }

        // Shift+q (in kana mode, composing) → convert to katakana and commit
        if key.modifiers.shift_key
            && !key.modifiers.control_key
            && (key.keysym == Keysym::KEY_Q || key.keysym == Keysym::KEY_Q_UPPER)
            && matches!(
                self.input_mode,
                InputMode::Hiragana | InputMode::Katakana | InputMode::HalfWidthKatakana
            )
            && matches!(self.state, InputState::Composing { .. })
        {
            return Some(self.skk_commit_as_katakana());
        }

        // l (no modifiers, in kana mode) → enter alphabet mode
        if key.keysym == Keysym::KEY_L
            && key.modifiers.is_empty()
            && matches!(
                self.input_mode,
                InputMode::Hiragana | InputMode::Katakana | InputMode::HalfWidthKatakana
            )
        {
            return Some(self.skk_enter_alphabet());
        }

        // q (no modifiers, in kana mode) → toggle katakana
        if key.keysym == Keysym::KEY_Q
            && key.modifiers.is_empty()
            && matches!(
                self.input_mode,
                InputMode::Hiragana | InputMode::Katakana | InputMode::HalfWidthKatakana
            )
        {
            return Some(self.skk_toggle_katakana());
        }

        None
    }

    /// Ctrl+j: switch to hiragana mode from any mode
    fn skk_enter_hiragana(&mut self) -> EngineResult {
        if self.input_mode == InputMode::Hiragana {
            return EngineResult::consumed();
        }

        // Bake current mode text before switching
        if self.input_mode == InputMode::Katakana {
            self.bake_katakana();
        }
        if self.input_mode == InputMode::HalfWidthKatakana {
            self.bake_halfwidth_katakana();
        }

        self.input_mode = InputMode::Hiragana;
        self.flush_romaji_to_composed();
        let aux = self.format_aux_composing();

        if matches!(self.state, InputState::Composing { .. }) {
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(aux));
        }
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }

    /// l key: switch to alphabet mode (keep preedit, don't commit)
    fn skk_enter_alphabet(&mut self) -> EngineResult {
        if self.input_mode == InputMode::Katakana {
            self.bake_katakana();
        }
        if self.input_mode == InputMode::HalfWidthKatakana {
            self.bake_halfwidth_katakana();
        }

        self.input_mode = InputMode::Alphabet;
        self.flush_romaji_to_composed();
        self.live.text.clear();
        let aux = self.format_aux_composing();

        if matches!(self.state, InputState::Composing { .. }) {
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(aux));
        }
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }

    /// q key: toggle between hiragana and katakana (bidirectional)
    fn skk_toggle_katakana(&mut self) -> EngineResult {
        let new_mode = if self.input_mode == InputMode::Katakana {
            // Katakana → Hiragana: bake katakana first
            self.bake_katakana();
            InputMode::Hiragana
        } else {
            // Hiragana/HalfWidthKatakana → Katakana
            if self.input_mode == InputMode::HalfWidthKatakana {
                self.bake_halfwidth_katakana();
            }
            InputMode::Katakana
        };

        self.input_mode = new_mode;
        self.live.text.clear();

        let aux = self.format_aux_composing();

        if matches!(self.state, InputState::Composing { .. }) {
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(aux));
        }
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Shift+q: convert composing text to katakana and commit immediately
    fn skk_commit_as_katakana(&mut self) -> EngineResult {
        self.flush_romaji_to_composed();
        let reading = self.input_buf.text.clone();
        let text = Self::hiragana_to_katakana(&reading);

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;
        self.input_mode = InputMode::Hiragana;

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::Commit(text))
            .with_action(EngineAction::HideAuxText)
    }

    /// Shift+Ctrl+q: convert composing text to half-width katakana and commit immediately
    fn skk_commit_as_halfwidth_katakana(&mut self) -> EngineResult {
        self.flush_romaji_to_composed();
        let reading = self.input_buf.text.clone();
        let text = karukan_engine::kana::hiragana_to_halfwidth_katakana(&reading);

        self.converters.romaji.reset();
        self.input_buf.clear();
        self.live.text.clear();
        self.state = InputState::Empty;
        self.input_mode = InputMode::Hiragana;

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(Preedit::new()))
            .with_action(EngineAction::Commit(text))
            .with_action(EngineAction::HideAuxText)
    }

    /// Ctrl+q: enter half-width katakana mode
    fn skk_enter_halfwidth_katakana(&mut self) -> EngineResult {
        if self.input_mode == InputMode::HalfWidthKatakana {
            return EngineResult::consumed();
        }

        if self.input_mode == InputMode::Katakana {
            self.bake_katakana();
        }

        self.input_mode = InputMode::HalfWidthKatakana;
        self.live.text.clear();

        let aux = self.format_aux_composing();

        if matches!(self.state, InputState::Composing { .. }) {
            let preedit = self.set_composing_state();
            return EngineResult::consumed()
                .with_action(EngineAction::UpdatePreedit(preedit))
                .with_action(EngineAction::UpdateAuxText(aux));
        }
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }
}
