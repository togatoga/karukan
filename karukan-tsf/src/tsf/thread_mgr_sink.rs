//! ITfThreadMgrEventSink implementation — tracks document focus changes.

use windows::Win32::UI::TextServices::*;
use windows::core::*;

use crate::tsf::text_input_processor::KarukanTextService_Impl;

impl ITfThreadMgrEventSink_Impl for KarukanTextService_Impl {
    /// Called when a document (context) gets focus.
    fn OnInitDocumentMgr(&self, _pdim: Option<&ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    /// Called when a document manager is being removed.
    fn OnUninitDocumentMgr(&self, _pdim: Option<&ITfDocumentMgr>) -> Result<()> {
        Ok(())
    }

    /// Called when focus changes to a new document.
    fn OnSetFocus(
        &self,
        pdimfocus: Option<&ITfDocumentMgr>,
        _pdimprevfocus: Option<&ITfDocumentMgr>,
    ) -> Result<()> {
        let mut inner = self.inner.borrow_mut();

        // Reset engine state when focus changes to avoid stale state
        inner.engine.reset();
        inner.cached_result = None;
        inner.composition = None;

        // Hide candidate window
        if let Some(ref cw) = inner.candidate_window {
            cw.borrow_mut().hide();
        }

        // Read surrounding context from the new document
        if let Some(doc_mgr) = pdimfocus {
            let client_id = inner.client_id;
            // Get the top context from the document manager
            unsafe {
                if let Ok(context) = doc_mgr.GetTop() {
                    let (left, right) =
                        crate::tsf::context_reader::read_context(&context, client_id);
                    inner.engine.set_surrounding_context(&left, &right);
                    return Ok(());
                }
            }
        }

        // Fallback: clear surrounding context
        inner.engine.set_surrounding_context("", "");

        Ok(())
    }

    /// Called when a context is pushed onto the document manager.
    fn OnPushContext(&self, _pic: Option<&ITfContext>) -> Result<()> {
        Ok(())
    }

    /// Called when a context is popped from the document manager.
    fn OnPopContext(&self, _pic: Option<&ITfContext>) -> Result<()> {
        Ok(())
    }
}
