//! Tests for the conversion persona (config `persona`): a fixed prefix on
//! the lctx sent to the model.
//!
//! These run without a loaded model: the conversion cache is seeded with the
//! lctx the engine is expected to build, and a hit proves the persona was
//! injected into the model call and the cache key.

use super::*;
use crate::core::engine::EngineConfig;
use crate::core::engine::cache::ConversionCacheKey;

fn persona_engine(persona: &str) -> InputMethodEngine {
    let config = EngineConfig {
        persona: persona.to_string(),
        ..EngineConfig::default()
    };
    InputMethodEngine::with_config(config)
}

/// Seed the conversion cache as if the model had converted `katakana` with
/// `lctx` to `converted`.
fn seed_cache(engine: &mut InputMethodEngine, katakana: &str, lctx: &str, converted: &str) {
    engine.conversion_cache.insert(
        ConversionCacheKey {
            katakana: katakana.to_string(),
            lctx: lctx.to_string(),
            strategy: ConversionStrategy::MainModelOnly,
        },
        vec![converted.to_string()],
    );
}

#[test]
fn test_persona_prefixes_model_lctx() {
    // With a persona configured, the model lctx (and thus the cache key) is
    // 「{persona}{ctx}」 — the seeded entry is only reachable through
    // that exact prefix.
    let mut engine = persona_engine("田中太郎/エンジニア");
    seed_cache(&mut engine, "アイ", "田中太郎/エンジニア", "HIT");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_empty_persona_leaves_lctx_unchanged() {
    let mut engine = persona_engine("");
    seed_cache(&mut engine, "アイ", "", "HIT");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_long_persona_keeps_its_tail() {
    // Only the last 25 chars of an over-long persona reach the lctx.
    let mut engine = persona_engine(&"あ".repeat(30));
    let lctx = "あ".repeat(25);
    seed_cache(&mut engine, "アイ", &lctx, "HIT");
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert_eq!(engine.chunks[0].converted, "HIT");
}

#[test]
fn test_aux_indicator_shows_active_persona() {
    // The mode indicator shows the persona content the model receives, so
    // what is influencing the conversion is visible at a glance.
    let mut engine = persona_engine("太郎");
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(aux.contains("[あ]P:太郎"), "aux was: {aux}");

    let mut engine = persona_engine("");
    let result = engine.process_key(&press('a'));
    let aux = last_aux_text(&result).expect("aux text action");
    assert!(!aux.contains("P:"), "aux was: {aux}");
}

#[test]
fn test_persona_applies_to_every_chunk_lctx() {
    // Chunked live conversion: each chunk's lctx gets the same persona
    // prefix, with the preceding chunks' converted text after it.
    let config = EngineConfig {
        persona: "太郎".to_string(),
        composing_chunk_len: 2,
        ..EngineConfig::default()
    };
    let mut engine = InputMethodEngine::with_config(config);
    seed_cache(&mut engine, "アイ", "太郎", "壱");
    seed_cache(&mut engine, "ウエ", "太郎壱", "弐");
    for k in ['a', 'i', 'u', 'e'] {
        engine.process_key(&press(k));
    }
    let converted: Vec<&str> = engine.chunks.iter().map(|c| c.converted.as_str()).collect();
    assert_eq!(converted, vec!["壱", "弐"]);
}
