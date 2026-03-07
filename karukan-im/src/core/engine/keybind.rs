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

        // SKK mode does not use Henkan/Muhenkan keys — pass them through
        if matches!(key.keysym, Keysym::HENKAN | Keysym::MUHENKAN) {
            return Some(EngineResult::not_consumed());
        }

        // Ctrl+j → enter hiragana mode (from any mode)
        if key.modifiers.control_key
            && !key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_J || key.keysym == Keysym::KEY_J_UPPER)
        {
            return Some(self.skk_enter_hiragana());
        }

        // In alphabet mode, only Ctrl+J is handled (above). All other keys pass through.
        if self.input_mode == InputMode::Alphabet {
            return None;
        }

        // Ctrl+q → half-width katakana (composing: commit, empty: mode switch)
        if key.modifiers.control_key
            && !key.modifiers.shift_key
            && (key.keysym == Keysym::KEY_Q || key.keysym == Keysym::KEY_Q_UPPER)
        {
            return Some(self.skk_enter_halfwidth_katakana());
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

    /// l key: commit composing text and switch to alphabet mode
    fn skk_enter_alphabet(&mut self) -> EngineResult {
        let mut result = EngineResult::consumed();

        if matches!(self.state, InputState::Composing { .. }) {
            self.flush_romaji_to_composed();
            let reading = self.input_buf.text.clone();
            let text = if self.input_mode == InputMode::Katakana {
                Self::hiragana_to_katakana(&reading)
            } else if self.input_mode == InputMode::HalfWidthKatakana {
                karukan_engine::kana::hiragana_to_halfwidth_katakana(&reading)
            } else if !self.live.text.is_empty() {
                self.live.text.clone()
            } else {
                reading.clone()
            };
            if !text.is_empty() {
                self.record_learning(&reading, &text);
                result = result.with_action(EngineAction::Commit(text));
            }
            self.converters.romaji.reset();
            self.input_buf.clear();
            self.live.text.clear();
            self.state = InputState::Empty;
            result = result
                .with_action(EngineAction::UpdatePreedit(Preedit::new()))
                .with_action(EngineAction::HideAuxText);
        }

        self.input_mode = InputMode::Alphabet;
        let aux = self.format_aux_composing();
        result.with_action(EngineAction::UpdateAuxText(aux))
    }

    /// q key: in composing state, convert to katakana and commit immediately;
    /// in empty state, toggle between hiragana and katakana mode.
    fn skk_toggle_katakana(&mut self) -> EngineResult {
        // Composing中はカタカナに変換して即確定
        if matches!(self.state, InputState::Composing { .. }) {
            return self.skk_commit_as_katakana();
        }

        // Empty状態ではモード切替
        let new_mode = if self.input_mode == InputMode::Katakana {
            InputMode::Hiragana
        } else {
            InputMode::Katakana
        };
        self.input_mode = new_mode;
        self.live.text.clear();
        let aux = self.format_aux_composing();
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Convert composing text to katakana and commit immediately
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

    /// Convert composing text to half-width katakana and commit immediately
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

    /// Ctrl+q: in composing state, convert to half-width katakana and commit;
    /// in empty state, switch to half-width katakana mode.
    fn skk_enter_halfwidth_katakana(&mut self) -> EngineResult {
        // Composing中は半角カタカナに変換して即確定
        if matches!(self.state, InputState::Composing { .. }) {
            return self.skk_commit_as_halfwidth_katakana();
        }

        // Empty状態ではモード切替
        if self.input_mode == InputMode::HalfWidthKatakana {
            return EngineResult::consumed();
        }
        if self.input_mode == InputMode::Katakana {
            self.bake_katakana();
        }
        self.input_mode = InputMode::HalfWidthKatakana;
        self.live.text.clear();
        let aux = self.format_aux_composing();
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }
}
