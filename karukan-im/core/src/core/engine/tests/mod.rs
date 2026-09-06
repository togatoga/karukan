//! Tests for the IME engine

use karukan_engine::{LearningCache, LearningConfig};

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
mod model_loading;
mod passthrough;
mod pending_romaji;
mod predictive;
mod rewriter;
mod source_filter;
mod strategy;
mod surrounding;
mod width;

/// Engine seeded with a learning entry `reading → surface`, no kanji model.
/// Bypasses `init.rs` (which gates learning on settings + file I/O) and
/// injects a populated `LearningCache` directly.
fn engine_with_learned(reading: &str, surface: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.converters.kanji = None;
    let mut cache = LearningCache::new(LearningConfig::default());
    cache.record(reading, surface);
    engine.learning = Some(cache);
    engine
}

/// Seed the conversion cache so a main-model greedy call for `katakana`
/// with `lctx` returns `texts` — a deterministic stand-in for the model
/// (with no converter loaded every strategy resolves to main greedy, and
/// tests that beam under a real converter pin `StrategyMode::Main`).
fn seed_model_cache(engine: &mut InputMethodEngine, katakana: &str, lctx: &str, texts: &[&str]) {
    engine.conversion_cache.insert(
        crate::core::engine::cache::ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: lctx.to_string(),
            model: crate::core::engine::cache::ModelRole::Main,
            beam_width: 1,
        },
        texts.iter().map(|s| s.to_string()).collect(),
    );
}

/// Build a dictionary from inline JSON, hiding the temp-file plumbing.
fn dict_from_json(json: &str) -> Dictionary {
    use std::io::Write;
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(json.as_bytes()).unwrap();
    tmp.flush().unwrap();
    Dictionary::build_from_json(tmp.path()).unwrap()
}

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

fn press_shift_key(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::new().with_shift(true), true)
}

fn press_ctrl(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(keysym, KeyModifiers::new().with_control(true), true)
}

fn press_alt(keysym: Keysym) -> KeyEvent {
    KeyEvent::new(
        keysym,
        KeyModifiers {
            alt_key: true,
            ..KeyModifiers::new()
        },
        true,
    )
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

/// Engine whose Space setting is the full-width one (the default is half).
fn fullwidth_space_engine() -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        space: crate::config::settings::SpaceStyle::Full,
        ..EngineConfig::default()
    })
}

fn make_live_conversion_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::new();
    engine.live.enabled = true;
    engine
}

/// Simulate an active live-conversion display: install `converted` as the
/// single chunk covering the current reading and mark the display shown.
/// The live text is derived from the chunks, so this is the test analogue
/// of a completed `chunked_auto_suggest`.
fn set_live_text(engine: &mut InputMethodEngine, converted: &str) {
    engine.chunks = vec![ComposingChunk {
        reading: engine.input_buf.reading(),
        converted: converted.to_string(),
    }];
    engine.live.shown = true;
}
