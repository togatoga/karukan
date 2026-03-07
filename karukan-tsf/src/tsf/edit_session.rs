//! ITfEditSession implementation — applies engine actions to the TSF context.
//!
//! TSF requires all text modifications to happen within an EditSession callback.
//! This module provides the ActionEditSession that processes EngineActions
//! (preedit updates, commits, candidate display) via TSF APIs.

#[cfg(target_os = "windows")]
use std::cell::RefCell;
#[cfg(target_os = "windows")]
use std::rc::Rc;

#[cfg(target_os = "windows")]
use windows::Win32::Foundation::*;
#[cfg(target_os = "windows")]
use windows::Win32::UI::TextServices::*;
#[cfg(target_os = "windows")]
use windows::core::*;

#[cfg(target_os = "windows")]
use karukan_im::EngineAction;
#[cfg(target_os = "windows")]
use karukan_im::core::candidate::CandidateList;
#[cfg(target_os = "windows")]
use karukan_im::core::preedit::{AttributeType, Preedit};

#[cfg(target_os = "windows")]
use crate::candidate::window::CandidateWindow;

/// An edit session that applies a list of EngineActions to the TSF context.
///
/// The `composition` field is wrapped in `Rc<RefCell<>>` so that composition
/// lifecycle changes (start/end) made during DoEditSession are visible to
/// the caller after the synchronous edit session completes.
#[cfg(target_os = "windows")]
#[implement(ITfEditSession)]
pub struct ActionEditSession {
    context: ITfContext,
    actions: Vec<EngineAction>,
    /// Shared composition reference — updated during DoEditSession.
    composition: Rc<RefCell<Option<ITfComposition>>>,
    _client_id: u32,
    /// Composition sink for StartComposition callback.
    composition_sink: ITfCompositionSink,
    /// Candidate window reference for show/hide.
    candidate_window: Option<Rc<RefCell<CandidateWindow>>>,
    /// Display attribute atom for input (composing) text.
    display_attr_atom_input: u32,
    /// Display attribute atom for converted text.
    display_attr_atom_converted: u32,
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
impl ActionEditSession {
    pub fn new(
        context: ITfContext,
        actions: Vec<EngineAction>,
        composition: Rc<RefCell<Option<ITfComposition>>>,
        client_id: u32,
        composition_sink: ITfCompositionSink,
        candidate_window: Option<Rc<RefCell<CandidateWindow>>>,
        display_attr_atom_input: u32,
        display_attr_atom_converted: u32,
    ) -> Self {
        Self {
            context,
            actions,
            composition,
            _client_id: client_id,
            composition_sink,
            candidate_window,
            display_attr_atom_input,
            display_attr_atom_converted,
        }
    }
}

#[cfg(target_os = "windows")]
impl ITfEditSession_Impl for ActionEditSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        for action in &self.actions {
            match action {
                EngineAction::UpdatePreedit(preedit) => {
                    self.update_preedit(ec, preedit)?;
                }
                EngineAction::Commit(text) => {
                    self.commit_text(ec, text)?;
                }
                EngineAction::ShowCandidates(candidates) => {
                    self.show_candidates(ec, candidates)?;
                }
                EngineAction::HideCandidates => {
                    self.hide_candidates();
                }
                EngineAction::UpdateAuxText(text) => {
                    tracing::debug!("EditSession: UpdateAuxText: {}", text);
                }
                EngineAction::HideAuxText => {
                    tracing::debug!("EditSession: HideAuxText");
                }
            }
        }
        Ok(())
    }
}

// --- Composition lifecycle and text operations ---

#[cfg(target_os = "windows")]
impl ActionEditSession {
    /// Ensure a composition exists. If not, start one at the current insertion point.
    fn ensure_composition(&self, ec: u32) -> Result<()> {
        let mut comp = self.composition.borrow_mut();
        if comp.is_some() {
            return Ok(());
        }

        unsafe {
            // Get the insertion point range
            let insert_at_sel: ITfInsertAtSelection = self.context.cast()?;
            let range = insert_at_sel.InsertTextAtSelection(ec, TF_IAS_QUERYONLY, &[])?;

            // Start a new composition at the insertion point
            let ctx_comp: ITfContextComposition = self.context.cast()?;
            let new_comp = ctx_comp.StartComposition(ec, &range, &self.composition_sink)?;
            *comp = Some(new_comp);
        }

        Ok(())
    }

    /// End the current composition (if any) without committing text.
    fn end_composition(&self, ec: u32) -> Result<()> {
        let mut comp = self.composition.borrow_mut();
        if let Some(composition) = comp.take() {
            unsafe {
                composition.EndComposition(ec)?;
            }
        }
        Ok(())
    }

    /// Update the preedit (composition) text in the TSF context.
    ///
    /// - If preedit is empty and composition exists, end the composition.
    /// - If preedit is non-empty and no composition, start one.
    /// - Update the composition range text and apply display attributes.
    fn update_preedit(&self, ec: u32, preedit: &Preedit) -> Result<()> {
        if preedit.is_empty() {
            return self.end_composition(ec);
        }

        self.ensure_composition(ec)?;

        let comp = self.composition.borrow();
        if let Some(ref composition) = *comp {
            unsafe {
                let range = composition.GetRange()?;
                let text_wide: Vec<u16> = preedit.text().encode_utf16().collect();
                range.SetText(ec, 0, &text_wide)?;

                // Apply display attributes (underline, highlight, etc.)
                self.apply_display_attributes(ec, &range, preedit)?;

                // Set caret position within the composition
                let caret_chars = preedit.caret();
                let caret_utf16: i32 = preedit
                    .text()
                    .chars()
                    .take(caret_chars)
                    .map(|c| c.len_utf16() as i32)
                    .sum();
                let caret_range = range.Clone()?;
                caret_range.Collapse(ec, TF_ANCHOR_START)?;
                let mut shifted = 0i32;
                caret_range.ShiftEnd(ec, caret_utf16, &mut shifted, std::ptr::null())?;
                caret_range.Collapse(ec, TF_ANCHOR_END)?;

                let sel = TF_SELECTION {
                    range: core::mem::ManuallyDrop::new(Some(caret_range)),
                    style: TF_SELECTIONSTYLE {
                        ase: TF_AE_NONE,
                        fInterimChar: FALSE,
                    },
                };
                self.context.SetSelection(ec, &[sel])?;
            }
        }

        Ok(())
    }

    /// Commit text to the application and end the composition.
    fn commit_text(&self, ec: u32, text: &str) -> Result<()> {
        {
            let comp = self.composition.borrow();
            if let Some(ref composition) = *comp {
                unsafe {
                    let range = composition.GetRange()?;
                    let text_wide: Vec<u16> = text.encode_utf16().collect();
                    range.SetText(ec, 0, &text_wide)?;
                }
            }
        }
        self.end_composition(ec)?;
        Ok(())
    }
}

// --- Display attributes ---

#[cfg(target_os = "windows")]
impl ActionEditSession {
    /// Apply display attributes from Preedit to the composition range.
    ///
    /// Maps Preedit AttributeType to the registered display attribute atoms
    /// (input underline or converted highlight) and sets them via ITfProperty.
    unsafe fn apply_display_attributes(
        &self,
        ec: u32,
        composition_range: &ITfRange,
        preedit: &Preedit,
    ) -> Result<()> {
        if preedit.attributes().is_empty() {
            return Ok(());
        }

        let prop: ITfProperty = unsafe { self.context.GetProperty(&GUID_PROP_ATTRIBUTE)? };

        for attr in preedit.attributes() {
            let atom = match attr.attr_type {
                AttributeType::Underline | AttributeType::UnderlineDouble => {
                    self.display_attr_atom_input
                }
                AttributeType::Highlight | AttributeType::Reverse => {
                    self.display_attr_atom_converted
                }
            };

            // Skip if atom was not registered (0 = invalid)
            if atom == 0 {
                continue;
            }

            // Create a sub-range for this attribute
            let attr_range = unsafe { composition_range.Clone()? };
            let mut shifted = 0i32;

            // Collapse to the start of the composition
            unsafe { attr_range.Collapse(ec, TF_ANCHOR_START)? };

            // Expand to cover [attr.start, attr.end)
            unsafe { attr_range.ShiftEnd(ec, attr.end as i32, &mut shifted, std::ptr::null())? };
            unsafe {
                attr_range.ShiftStart(ec, attr.start as i32, &mut shifted, std::ptr::null())?
            };

            // Set the display attribute property
            let variant = VARIANT::from(atom as i32);
            unsafe { prop.SetValue(ec, &attr_range, &variant)? };
        }

        Ok(())
    }
}

// --- Candidate window ---

#[cfg(target_os = "windows")]
impl ActionEditSession {
    /// Show the candidate window positioned at the composition text.
    fn show_candidates(&self, ec: u32, candidates: &CandidateList) -> Result<()> {
        let cw = match self.candidate_window {
            Some(ref cw) => cw,
            None => return Ok(()),
        };

        // Get the caret position for window placement
        let comp = self.composition.borrow();
        if let Some(ref composition) = *comp {
            unsafe {
                let range = composition.GetRange()?;
                let context_view = self.context.GetActiveView()?;
                let mut rect = RECT::default();
                let mut clipped = BOOL::default();
                context_view.GetTextExt(ec, &range, &mut rect, &mut clipped)?;

                let page_candidates: Vec<String> = candidates
                    .page_candidates()
                    .iter()
                    .map(|c| c.text.clone())
                    .collect();

                let mut window = cw.borrow_mut();
                window.show(&page_candidates, candidates.page_cursor());
                window.move_to(rect.left, rect.bottom);
            }
        }

        Ok(())
    }

    /// Hide the candidate window.
    fn hide_candidates(&self) {
        if let Some(ref cw) = self.candidate_window {
            cw.borrow_mut().hide();
        }
    }
}
