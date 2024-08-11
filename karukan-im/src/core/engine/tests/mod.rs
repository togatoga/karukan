//! Tests for the IME engine

use super::*;
use crate::core::keycode::KeyModifiers;

mod alphabet;
mod basic;
mod candidates;
mod conversion;
mod cursor;
mod katakana;
mod live_conversion;
mod mode_toggle;
mod passthrough;
mod strategy;
mod surrounding;

fn press(ch: char) -> KeyEvent {
    KeyEvent::press(Keysym(ch as u32))
}

fn press_key(keysym: Keysym) -> KeyEvent {
    KeyEvent::press(keysym)
}

fn release_key(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::default(), false)
}

fn press_shift(ch: char) -> KeyEvent {
    KeyEvent::new(
        Keysym(ch as u32),
        KeyModifiers::new().with_shift(true),
        true,
    )
}

fn press_ctrl(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::new().with_control(true), true)
}

fn press_ctrl_shift(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(
        keysym,
        KeyModifiers::new().with_control(true).with_shift(true),
        true,
    )
}

fn make_live_conversion_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.live.enabled = true;
    engine
}
