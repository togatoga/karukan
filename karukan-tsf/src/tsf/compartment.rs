//! Compartment helpers — keyboard open/close state synchronization.
//!
//! Provides utility functions to read and write the
//! `GUID_COMPARTMENT_KEYBOARD_OPENCLOSE` compartment, which tracks
//! whether the IME is "open" (active) or "closed" (passthrough).

use windows::Win32::UI::TextServices::*;
use windows::core::*;

/// Get the keyboard open/close compartment from the thread manager.
pub fn get_keyboard_openclose_compartment(
    thread_mgr: &ITfThreadMgr,
    client_id: u32,
) -> Result<ITfCompartment> {
    unsafe {
        let compartment_mgr: ITfCompartmentMgr = thread_mgr.cast()?;
        let compartment = compartment_mgr.GetCompartment(&GUID_COMPARTMENT_KEYBOARD_OPENCLOSE)?;
        let _ = client_id; // Used by callers for SetValue
        Ok(compartment)
    }
}

/// Read the current keyboard open/close state.
/// Returns `true` if the IME is open (active), `false` if closed.
pub fn get_openclose_state(thread_mgr: &ITfThreadMgr, client_id: u32) -> Result<bool> {
    let compartment = get_keyboard_openclose_compartment(thread_mgr, client_id)?;
    unsafe {
        let value = compartment.GetValue()?;
        // VT_I4 with value 0 = closed, nonzero = open
        let i = i32::try_from(&value).unwrap_or(0);
        Ok(i != 0)
    }
}

/// Set the keyboard open/close state.
pub fn set_openclose_state(thread_mgr: &ITfThreadMgr, client_id: u32, open: bool) -> Result<()> {
    let compartment = get_keyboard_openclose_compartment(thread_mgr, client_id)?;
    unsafe {
        let value = VARIANT::from(if open { 1i32 } else { 0i32 });
        compartment.SetValue(client_id, &value)?;
    }
    Ok(())
}

/// Advise an `ITfCompartmentEventSink` on the keyboard open/close compartment.
/// Returns the advise cookie for later `UnadviseSink`.
pub fn advise_keyboard_openclose(
    thread_mgr: &ITfThreadMgr,
    client_id: u32,
    sink: &ITfCompartmentEventSink,
) -> Result<u32> {
    let compartment = get_keyboard_openclose_compartment(thread_mgr, client_id)?;
    unsafe {
        let source: ITfSource = compartment.cast()?;
        source.AdviseSink(&ITfCompartmentEventSink::IID, sink)
    }
}

/// Unadvise a previously advised compartment event sink.
pub fn unadvise_keyboard_openclose(
    thread_mgr: &ITfThreadMgr,
    client_id: u32,
    cookie: u32,
) -> Result<()> {
    let compartment = get_keyboard_openclose_compartment(thread_mgr, client_id)?;
    unsafe {
        let source: ITfSource = compartment.cast()?;
        source.UnadviseSink(cookie)?;
    }
    Ok(())
}
