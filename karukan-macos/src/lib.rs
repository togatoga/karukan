//! karukan-macos: macOS Input Method Kit (IMKit) addon for karukan
//!
//! This crate provides a static library that bridges Swift (IMKit) and
//! karukan-im's InputMethodEngine via C FFI.

#[cfg(target_os = "macos")]
pub mod engine_bridge;
#[cfg(target_os = "macos")]
pub mod ffi;
#[cfg(target_os = "macos")]
pub mod keymap;

#[cfg(not(target_os = "macos"))]
pub mod engine_bridge;
#[cfg(not(target_os = "macos"))]
pub mod keymap;
