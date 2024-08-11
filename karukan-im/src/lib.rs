//! karukan-im: A Japanese Input Method Engine for Linux
//!
//! This crate provides a Japanese IME that integrates with fcitx5 framework.
//! It uses karukan-engine for romaji-to-hiragana and hiragana-to-kanji conversion.

pub mod config;
pub mod core;
pub mod ffi;

pub use core::engine::{EngineAction, EngineResult, InputMethodEngine};
pub use core::keycode::{KeyEvent, KeyModifiers, Keysym};
pub use core::state::InputState;
