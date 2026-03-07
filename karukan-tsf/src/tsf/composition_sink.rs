//! ITfCompositionSink implementation — handles composition lifecycle events.

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::tsf::text_input_processor::KarukanTextService;

impl ITfCompositionSink_Impl for KarukanTextService_Impl {
    /// Called when a composition is terminated (by the application or TSF).
    ///
    /// This can happen when the application forcefully ends the composition
    /// (e.g., when focus changes). We need to clean up the engine state.
    fn OnCompositionTerminated(
        &self,
        _ecwrite: u32,
        _pcomposition: Option<&ITfComposition>,
    ) -> Result<()> {
        let mut inner = self.inner.borrow_mut();

        // The composition was terminated externally — commit any pending text
        let committed = inner.engine.commit();
        if !committed.is_empty() {
            tracing::debug!("Composition terminated, committed: {}", committed);
        }

        // Clear the composition reference
        inner.composition = None;

        // Hide candidate window
        if let Some(ref cw) = inner.candidate_window {
            cw.borrow_mut().hide();
        }

        Ok(())
    }
}
