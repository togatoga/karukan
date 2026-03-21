//! Core IME functionality
//!
//! This module contains the core state machine and input processing logic.

pub mod candidate;
pub mod engine;
pub mod keycode;
#[cfg(target_os = "macos")]
pub mod keymap_mac;
pub mod preedit;
pub mod state;
