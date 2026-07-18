//! Tests for the IME engine

use super::*;
use crate::core::keycode::KeyModifiers;

mod alphabet;
mod basic;
mod candidates;
mod chunks;
mod conversion;
mod cursor;
mod emoji;
mod katakana;
mod learning;
mod live_conversion;
mod mode_toggle;
mod passthrough;
mod rewriter;
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

fn press_ctrl_alt(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(
        keysym,
        KeyModifiers {
            alt_key: true,
            ..KeyModifiers::new().with_control(true)
        },
        true,
    )
}

fn press_ctrl_shift(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(
        keysym,
        KeyModifiers::new().with_control(true).with_shift(true),
        true,
    )
}

/// Last UpdateAuxText emitted by an engine result, if any.
fn last_aux_text(result: &EngineResult) -> Option<String> {
    result.actions.iter().rev().find_map(|a| match a {
        EngineAction::UpdateAuxText(text) => Some(text.clone()),
        _ => None,
    })
}

fn make_live_conversion_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.live.enabled = true;
    engine
}
