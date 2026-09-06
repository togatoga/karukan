//! Mode switching (katakana, alphabet, live conversion)

use tracing::debug;

use super::*;

impl InputMethodEngine {
    /// Enter katakana mode (Ctrl+k)
    /// One-way switch to Katakana; a mode toggle key (Right Super, JIS 変換,
    /// macOS かな/right-⌘ tap) returns to Hiragana.
    pub(super) fn enter_katakana_mode(&mut self) -> EngineResult {
        // Already in katakana mode: nothing to do
        if self.mode.current() == InputMode::Katakana {
            return EngineResult::consumed();
        }

        self.mode.set(InputMode::Katakana);
        // Drop the live display so katakana mode takes priority on commit
        self.live.shown = false;

        if self.input_buf.is_empty() {
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
            && self.mode.current() != InputMode::Katakana
        {
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
