//! Mode switching (katakana)

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
}
