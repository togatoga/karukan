//! Candidate window for displaying conversion candidates.
//!
//! Uses a Win32 layered window with Direct2D/DirectWrite rendering
//! for proper Japanese text display.

#[cfg(target_os = "windows")]
pub mod window;
