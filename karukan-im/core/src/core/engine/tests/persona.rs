//! Tests for the conversion persona (config `persona`): a fixed prefix on
//! the lctx sent to the model, separated from the context by one space.
//!
//! These run without a loaded model: the conversion cache is seeded with the
//! lctx the engine is expected to build, and a hit proves the persona was
//! injected into the model call and the cache key.

use super::*;

fn persona_engine(persona: &str) -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        persona: persona.to_string(),
        ..EngineConfig::default()
    })
}

fn verbose_persona_engine(persona: &str) -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        persona: persona.to_string(),
        verbose: true,
        ..EngineConfig::default()
    })
}

#[test]
fn test_persona_prefixes_model_lctx() {
    // With a persona configured, the model lctx (and thus the cache key) is
    // 「{persona} {ctx}」 — the seeded entry is only reachable through
    // that exact prefix, space included.
    let mut engine = persona_engine("田中太郎/エンジニア");
    seed_model_cache(&mut engine, "アイ", "田中太郎/エンジニア ", &["HIT"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_empty_persona_leaves_lctx_unchanged() {
    let mut engine = persona_engine("");
    seed_model_cache(&mut engine, "アイ", "", &["HIT"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_long_persona_keeps_its_tail() {
    // Only the last 25 chars of an over-long persona reach the lctx.
    let mut engine = persona_engine(&"あ".repeat(30));
    let lctx = "あ".repeat(25) + " ";
    seed_model_cache(&mut engine, "アイ", &lctx, &["HIT"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_persona_is_nfkc_normalized_and_trimmed_once() {
    // The prompt is NFKC'd before the model reads it, so the persona is
    // normalized up front: full-width ASCII and ideographic spaces fold
    // before the cap and before the cache key / aux indicator see it.
    let mut engine = verbose_persona_engine("\u{3000}Ｐｒｏｇｒａｍｍｉｎｇ ");
    assert_eq!(engine.config.persona, "Programming");
    seed_model_cache(&mut engine, "アイ", "Programming ", &["HIT"]);
    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[あ]P:Programming"), "aux was: {aux}");
}

#[test]
fn test_aux_indicator_shows_persona_only_in_verbose_mode() {
    // The persona is debug information: the quiet aux line leaves it out,
    // and verbose mode shows the content the model receives so what is
    // influencing the conversion is visible at a glance.
    let mut engine = persona_engine("太郎");
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(!aux.contains("P:"), "aux was: {aux}");

    let mut engine = verbose_persona_engine("太郎");
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[あ]P:太郎"), "aux was: {aux}");

    let mut engine = verbose_persona_engine("");
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(!aux.contains("P:"), "aux was: {aux}");
}

#[test]
fn test_verbose_toggle_reveals_persona_in_place() {
    // Ctrl+Shift+V re-renders the aux line on the spot, so the persona
    // appears and disappears with the toggle mid-composition.
    let mut engine = persona_engine("太郎");
    engine.process_key(&press('a'));
    let result = engine.process_key(&press_ctrl_shift(Keysym('V' as u32)));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[あ]P:太郎"), "aux was: {aux}");
    let result = engine.process_key(&press_ctrl_shift(Keysym('V' as u32)));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(!aux.contains("P:"), "aux was: {aux}");
}

#[test]
fn test_persona_applies_to_every_chunk_lctx() {
    // Chunked live conversion: each chunk's lctx gets the same persona
    // prefix, with the preceding chunks' converted text after the space.
    let config = EngineConfig {
        persona: "太郎".to_string(),
        chunk_chars: 2,
        ..EngineConfig::default()
    };
    let mut engine = InputMethodEngine::with_config(config);
    seed_model_cache(&mut engine, "アイ", "太郎 ", &["壱"]);
    seed_model_cache(&mut engine, "ウエ", "太郎 壱", &["弐"]);
    for k in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(k));
    }
    let converted: Vec<&str> = engine.chunks.iter().map(|c| c.converted.as_str()).collect();
    assert_eq!(converted, vec!["壱", "弐"]);
}

#[test]
fn test_persona_alone_when_context_is_only_whitespace() {
    // Blank surrounding text is dropped before the persona is prepended,
    // so the model sees the persona (and its separator) and nothing else.
    let mut engine = persona_engine("太郎");
    engine.set_surrounding_context("    ", "");
    seed_model_cache(&mut engine, "アイ", "太郎 ", &["HIT"]);
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}
