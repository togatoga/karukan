//! Mode switching (katakana, alphabet, live conversion)

use tracing::debug;

use super::*;

impl InputMethodEngine {
    /// Enter katakana mode (Ctrl+k)
    /// One-way switch to Katakana; use Right Super to return to Hiragana.
    pub(super) fn enter_katakana_mode(&mut self) -> EngineResult {
        // Already in katakana mode: nothing to do
        if self.input_mode == InputMode::Katakana {
            return EngineResult::consumed();
        }

        self.input_mode = InputMode::Katakana;
        // Clear live conversion text so katakana mode takes priority on commit
        self.live.text.clear();

        let romaji_buffer = self.converters.romaji.buffer().to_string();

        if self.input_buf.text.is_empty() && romaji_buffer.is_empty() {
            return EngineResult::consumed();
        }

        let preedit = self.set_composing_state();

        // Update aux text to show mode
        let aux = format!("{} Karukan ({})", self.mode_indicator(), self.model_name());

        EngineResult::consumed()
            .with_action(EngineAction::UpdatePreedit(preedit))
            .with_action(EngineAction::UpdateAuxText(aux))
    }

    /// Toggle live conversion mode via Ctrl+Shift+L.
    ///
    /// When toggled ON during Composing, immediately convert the current
    /// input buffer so the user doesn't have to type another key to see the
    /// live result. When toggled OFF, drop any stale converted text so the
    /// preedit reverts to hiragana right away.
    pub(super) fn toggle_live_conversion(&mut self) -> EngineResult {
        self.live.enabled = !self.live.enabled;
        let mode = if self.live.enabled { "ON" } else { "OFF" };
        debug!("Live conversion toggled: {}", mode);
        let aux = EngineAction::UpdateAuxText(format!("ライブ変換: {}", mode));

        if matches!(self.state, InputState::Composing { .. })
            && self.input_mode != InputMode::Katakana
        {
            if self.live.enabled {
                let mut result = self.refresh_input_state();
                result.actions.push(aux);
                return result;
            }
            if !self.live.text.is_empty() {
                self.live.text.clear();
                let preedit = self.set_composing_state();
                return EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(preedit))
                    .with_action(aux);
            }
        }

        EngineResult::consumed().with_action(aux)
    }
}
