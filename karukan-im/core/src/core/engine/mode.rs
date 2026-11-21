//! Mode switching (live conversion, verbose display)

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

    /// Ctrl+Shift+V: turn the aux line's debug details on or off. The next
    /// render picks it up, so no state has to be rebuilt here.
    pub(super) fn toggle_verbose(&mut self) -> EngineResult {
        self.config.verbose = !self.config.verbose;
        let mode = if self.config.verbose { "ON" } else { "OFF" };
        debug!("Verbose display toggled: {}", mode);
        // Re-render the line the user is looking at, so the change shows now
        // rather than on the next keystroke. Nothing is being typed in the
        // Empty state, so there the toggle reports itself instead.
        let aux = match &self.state {
            InputState::Conversion {
                reading,
                candidates,
                ..
            } => self.format_aux_conversion(reading, candidates),
            InputState::Composing { .. } => self.format_aux_suggest(),
            InputState::Empty => format!("詳細表示: {mode}"),
        };
        EngineResult::consumed().with_action(EngineAction::UpdateAuxText(aux))
    }
}
