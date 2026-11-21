//! Mode switching (live conversion)

use tracing::debug;

use super::*;

impl InputMethodEngine {
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

        if matches!(self.state, InputState::Composing { .. }) {
            if self.live.enabled {
                let mut result = self.refresh_input_state();
                result.actions.push(aux);
                return result;
            }
            if self.live.shown {
                self.live.shown = false;
                let preedit = self.set_composing_state();
                return EngineResult::consumed()
                    .with_action(EngineAction::UpdatePreedit(preedit))
                    .with_action(aux);
            }
        }

        EngineResult::consumed().with_action(aux)
    }
}
