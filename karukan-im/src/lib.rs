//! karukan-im: A Japanese Input Method Engine for Linux and macOS
//!
//! This crate provides a Japanese IME engine consumed by two frontends:
//! the fcitx5 addon on Linux (via the C FFI in [`ffi`]) and the macOS
//! Swift/InputMethodKit frontend (via the stdio JSON-RPC server in
//! [`server`]). It uses karukan-engine for romaji-to-hiragana and
//! hiragana-to-kanji conversion.

pub mod config;
pub mod core;
pub mod ffi;
pub mod server;

pub use core::engine::{EngineAction, EngineResult, InputMethodEngine};
pub use core::keycode::{KeyEvent, KeyModifiers, Keysym};
pub use core::state::InputState;
