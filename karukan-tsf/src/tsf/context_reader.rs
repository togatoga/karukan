//! Context reader — reads surrounding text from the TSF context.
//!
//! Uses a read-only `ITfEditSession` to extract text before and after
//! the cursor position, for contextual kana-kanji conversion.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

/// Maximum number of characters to read in each direction.
const CONTEXT_READ_CHARS: i32 = 50;

/// Result holder for context read — shared between the session and caller.
pub type ContextResult = Rc<RefCell<(String, String)>>;

/// A read-only edit session that reads text around the cursor.
#[implement(ITfEditSession)]
pub struct ContextReadSession {
    context: ITfContext,
    result: ContextResult,
}

impl ContextReadSession {
    pub fn new(context: ITfContext, result: ContextResult) -> Self {
        Self { context, result }
    }
}

impl ITfEditSession_Impl for ContextReadSession_Impl {
    fn DoEditSession(&self, ec: u32) -> Result<()> {
        let (left, right) = read_surrounding_text(ec, &self.context)?;
        let mut result = self.result.borrow_mut();
        result.0 = left;
        result.1 = right;
        Ok(())
    }
}

/// Read surrounding text around the current selection/cursor.
///
/// Returns `(left_text, right_text)` where:
/// - `left_text`: up to 50 characters before the cursor
/// - `right_text`: up to 50 characters after the cursor
fn read_surrounding_text(ec: u32, context: &ITfContext) -> Result<(String, String)> {
    unsafe {
        // Get the current selection
        let mut selections = [TF_SELECTION::default()];
        let mut fetched = 0u32;
        context.GetSelection(ec, TF_DEFAULT_SELECTION, &mut selections, &mut fetched)?;

        if fetched == 0 {
            return Ok((String::new(), String::new()));
        }

        let sel_range = selections[0].range.as_ref().ok_or(E_FAIL)?;

        // Read left context: clone range, shift start backwards
        let left_range = sel_range.Clone()?;
        left_range.Collapse(ec, TF_ANCHOR_START)?;
        let mut shifted = 0i32;
        left_range.ShiftStart(ec, -CONTEXT_READ_CHARS, &mut shifted, std::ptr::null())?;
        let mut left_buf = [0u16; 64];
        let mut left_len = 0u32;
        left_range.GetText(ec, 0, &mut left_buf, &mut left_len)?;
        let left = String::from_utf16_lossy(&left_buf[..left_len as usize]);

        // Read right context: clone range, shift end forwards
        let right_range = sel_range.Clone()?;
        right_range.Collapse(ec, TF_ANCHOR_END)?;
        right_range.ShiftEnd(ec, CONTEXT_READ_CHARS, &mut shifted, std::ptr::null())?;
        let mut right_buf = [0u16; 64];
        let mut right_len = 0u32;
        right_range.GetText(ec, 0, &mut right_buf, &mut right_len)?;
        let right = String::from_utf16_lossy(&right_buf[..right_len as usize]);

        Ok((left, right))
    }
}

/// Request a synchronous read-only edit session to read surrounding context.
///
/// Returns `(left_text, right_text)`. On failure, returns empty strings.
pub fn read_context(context: &ITfContext, client_id: u32) -> (String, String) {
    let result: ContextResult = Rc::new(RefCell::new((String::new(), String::new())));
    let session = ContextReadSession::new(context.clone(), result.clone());

    let edit_session: ITfEditSession = session.into();
    unsafe {
        match context.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READ) {
            Ok(hr) if hr.is_err() => {
                tracing::debug!("Context read session failed: hr={:?}", hr);
                return (String::new(), String::new());
            }
            Err(e) => {
                tracing::debug!("Context read session request failed: {:?}", e);
                return (String::new(), String::new());
            }
            _ => {}
        }
    }

    let result = result.borrow();
    (result.0.clone(), result.1.clone())
}
