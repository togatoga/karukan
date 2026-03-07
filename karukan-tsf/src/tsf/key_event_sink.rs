//! ITfKeyEventSink implementation — handles keyboard input for TSF.

use std::cell::RefCell;
use std::rc::Rc;

use windows::Win32::Foundation::*;
use windows::Win32::UI::TextServices::*;
use windows::core::*;

use karukan_im::EngineAction;

use crate::globals::GUID_PRESERVED_KEY_ONOFF;
use crate::tsf::edit_session::ActionEditSession;
use crate::tsf::lang_bar::KarukanLangBarButton;
use crate::tsf::text_input_processor::KarukanTextService_Impl;

#[allow(clippy::not_unsafe_ptr_arg_deref)]
impl ITfKeyEventSink_Impl for KarukanTextService_Impl {
    /// Called when focus changes.
    fn OnSetFocus(&self, _fforeground: BOOL) -> Result<()> {
        Ok(())
    }

    /// Called before OnKeyDown — determines if the key will be consumed.
    ///
    /// We process the key through the engine here and cache the result.
    /// If consumed, TSF will call OnKeyDown where we apply the cached actions.
    fn OnTestKeyDown(
        &self,
        _pic: Option<&ITfContext>,
        wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        // If IME is disabled, don't consume any keys
        let inner = self.inner.borrow();
        if !inner.enabled {
            return Ok(FALSE);
        }
        drop(inner);

        let vk = wparam.0 as u32;
        let (shift, control, alt, win) = get_modifier_state();
        let unicode_char = vk_to_unicode(vk, shift);

        let mut inner = self.inner.borrow_mut();
        let result = inner
            .engine
            .process_key(vk, unicode_char, shift, control, alt, win, true);

        let consumed = result.consumed;
        inner.cached_result = Some(result);

        Ok(BOOL::from(consumed))
    }

    /// Called when a key is pressed and OnTestKeyDown returned TRUE.
    ///
    /// Applies the cached EngineResult actions via an EditSession.
    fn OnKeyDown(
        &self,
        pic: Option<&ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        let mut inner = self.inner.borrow_mut();

        if let Some(result) = inner.cached_result.take() {
            if result.consumed
                && !result.actions.is_empty()
                && let Some(context) = pic
            {
                drop(inner); // Release borrow before edit session
                apply_engine_actions(self, context, &result.actions)?;

                // Check for mode changes and update language bar
                update_lang_bar_if_mode_changed(self);

                return Ok(TRUE);
            }
            Ok(BOOL::from(result.consumed))
        } else {
            // No cached result — should not happen, but handle gracefully
            Ok(FALSE)
        }
    }

    /// Called before OnKeyUp — we generally don't consume key-up events.
    fn OnTestKeyUp(
        &self,
        _pic: Option<&ITfContext>,
        _wparam: WPARAM,
        _lparam: LPARAM,
    ) -> Result<BOOL> {
        Ok(FALSE)
    }

    /// Called when a key is released — we generally don't consume key-up events.
    fn OnKeyUp(&self, _pic: Option<&ITfContext>, _wparam: WPARAM, _lparam: LPARAM) -> Result<BOOL> {
        Ok(FALSE)
    }

    /// Called for preserved keys (e.g., Hankaku/Zenkaku toggle).
    fn OnPreservedKey(&self, _pic: Option<&ITfContext>, rguid: *const GUID) -> Result<BOOL> {
        unsafe {
            if rguid.is_null() {
                return Ok(FALSE);
            }

            if *rguid == GUID_PRESERVED_KEY_ONOFF {
                let mut inner = self.inner.borrow_mut();
                inner.enabled = !inner.enabled;
                if !inner.enabled {
                    // Commit pending text and reset when turning IME off
                    let _committed = inner.engine.commit();
                    inner.engine.reset();
                    inner.composition = None;
                    // Hide candidate window
                    if let Some(ref cw) = inner.candidate_window {
                        cw.borrow_mut().hide();
                    }
                }

                // Sync compartment state
                if let Some(ref thread_mgr) = inner.thread_mgr {
                    let _ = crate::tsf::compartment::set_openclose_state(
                        thread_mgr,
                        inner.client_id,
                        inner.enabled,
                    );
                }

                // Update language bar
                if let Some(ref item) = inner.lang_bar_item {
                    let button: &KarukanLangBarButton = windows::core::AsImpl::as_impl(item);
                    button.update_mode(inner.engine.input_mode(), inner.enabled);
                }

                tracing::debug!("IME toggle: enabled={}", inner.enabled);
                Ok(TRUE)
            } else {
                Ok(FALSE)
            }
        }
    }
}

/// Get the current modifier key state using GetKeyState.
#[cfg(target_os = "windows")]
fn get_modifier_state() -> (bool, bool, bool, bool) {
    use windows::Win32::UI::Input::KeyboardAndMouse::GetKeyState;

    unsafe {
        let shift = GetKeyState(0x10) < 0; // VK_SHIFT
        let control = GetKeyState(0x11) < 0; // VK_CONTROL
        let alt = GetKeyState(0x12) < 0; // VK_MENU
        let win = GetKeyState(0x5B) < 0 || GetKeyState(0x5C) < 0; // VK_LWIN | VK_RWIN
        (shift, control, alt, win)
    }
}

#[cfg(not(target_os = "windows"))]
fn get_modifier_state() -> (bool, bool, bool, bool) {
    (false, false, false, false)
}

/// Convert a VK code to a Unicode character using ToUnicode.
#[cfg(target_os = "windows")]
fn vk_to_unicode(vk: u32, _shift: bool) -> Option<char> {
    use windows::Win32::UI::Input::KeyboardAndMouse::*;

    unsafe {
        let scan_code = MapVirtualKeyW(vk, MAP_VIRTUAL_KEY_TYPE(0)); // MAPVK_VK_TO_VSC
        let mut keyboard_state = [0u8; 256];
        GetKeyboardState(&mut keyboard_state).ok()?;

        let mut buf = [0u16; 4];
        let result = ToUnicode(vk, scan_code, Some(&keyboard_state), &mut buf, 0);
        if result == 1 {
            char::from_u32(buf[0] as u32)
        } else {
            None
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn vk_to_unicode(_vk: u32, _shift: bool) -> Option<char> {
    None
}

/// Apply engine actions to the TSF context via a synchronous EditSession.
///
/// Creates an `ActionEditSession`, requests a synchronous edit session from TSF,
/// and updates the composition reference in the service after completion.
#[cfg(target_os = "windows")]
fn apply_engine_actions(
    service: &KarukanTextService_Impl,
    context: &ITfContext,
    actions: &[EngineAction],
) -> Result<()> {
    // Extract needed values from inner state
    let (composition_snapshot, client_id, candidate_window, atom_input, atom_converted) = {
        let inner = service.inner.borrow();
        (
            inner.composition.clone(),
            inner.client_id,
            inner.candidate_window.clone(),
            inner.display_attr_atom_input,
            inner.display_attr_atom_converted,
        )
    };

    // Shared cell for composition lifecycle tracking across the edit session
    let composition_cell = Rc::new(RefCell::new(composition_snapshot));

    // Get the ITfCompositionSink from our service (for StartComposition)
    let composition_sink: ITfCompositionSink = service.cast_interface()?;

    let session = ActionEditSession::new(
        context.clone(),
        actions.to_vec(),
        composition_cell.clone(),
        client_id,
        composition_sink,
        candidate_window,
        atom_input,
        atom_converted,
    );

    // Request synchronous read-write edit session
    let edit_session: ITfEditSession = session.into();
    unsafe {
        let hr =
            context.RequestEditSession(client_id, &edit_session, TF_ES_SYNC | TF_ES_READWRITE)?;
        // Check the edit session result
        if hr.is_err() {
            tracing::warn!("Edit session failed: {:?}", hr);
        }
    }

    // Update composition in inner state after the edit session completes
    let mut inner = service.inner.borrow_mut();
    inner.composition = composition_cell.borrow().clone();

    Ok(())
}

/// Check if the engine's input mode changed since the last key event,
/// and update the language bar if so.
#[cfg(target_os = "windows")]
fn update_lang_bar_if_mode_changed(service: &KarukanTextService_Impl) {
    let mut inner = service.inner.borrow_mut();
    let current_mode = inner.engine.input_mode();
    if current_mode != inner.prev_input_mode {
        inner.prev_input_mode = current_mode;
        if let Some(ref item) = inner.lang_bar_item {
            let button: &KarukanLangBarButton = unsafe { windows::core::AsImpl::as_impl(item) };
            button.update_mode(current_mode, inner.enabled);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn update_lang_bar_if_mode_changed(_service: &KarukanTextService_Impl) {}

#[cfg(not(target_os = "windows"))]
fn apply_engine_actions(
    _service: &KarukanTextService_Impl,
    _context: &ITfContext,
    actions: &[EngineAction],
) -> Result<()> {
    for action in actions {
        tracing::debug!("TSF action (stub): {:?}", action);
    }
    Ok(())
}
