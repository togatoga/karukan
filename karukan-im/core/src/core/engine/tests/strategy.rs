use super::super::strategy::determine_conversion_strategy;
use super::*;

// --- ConversionStrategy tests ---

/// Helper to create a config with specific thresholds
fn strategy_config(short_input_threshold: usize, beam_width: usize) -> EngineConfig {
    EngineConfig {
        short_input_threshold,
        beam_width,
        num_candidates: 9,
        ..EngineConfig::default()
    }
}

/// Default test config: short_input_threshold=10, beam_width=3, max_latency_ms=100
fn default_strategy_config() -> EngineConfig {
    strategy_config(10, 3)
}

// --- No sub model: always MainModelOnly ---

#[test]
fn strategy_no_light_model_returns_main_model_only() {
    let config = default_strategy_config();
    // Without light model, always MainModelOnly regardless of other params
    assert_eq!(
        determine_conversion_strategy(5, 1, false, false, &config),
        ConversionStrategy::MainModelOnly,
    );
    assert_eq!(
        determine_conversion_strategy(5, 9, false, false, &config),
        ConversionStrategy::MainModelOnly,
    );
    assert_eq!(
        determine_conversion_strategy(50, 9, false, true, &config),
        ConversionStrategy::MainModelOnly,
    );
}

// --- Auto-suggest (num_candidates == 1) ---

#[test]
fn strategy_auto_suggest_adaptive_false_returns_main_model() {
    let config = default_strategy_config();
    // adaptive=false → MainModelOnly
    assert_eq!(
        determine_conversion_strategy(5, 1, true, false, &config),
        ConversionStrategy::MainModelOnly,
    );
}

#[test]
fn strategy_auto_suggest_adaptive_true_returns_light_model() {
    let config = default_strategy_config();
    // adaptive=true → LightModelOnly (main model was too slow)
    assert_eq!(
        determine_conversion_strategy(5, 1, true, true, &config),
        ConversionStrategy::LightModelOnly,
    );
}

#[test]
fn strategy_auto_suggest_adaptive_true_even_short_input() {
    let config = default_strategy_config();
    // Even with very short input, adaptive=true → LightModelOnly
    assert_eq!(
        determine_conversion_strategy(1, 1, true, true, &config),
        ConversionStrategy::LightModelOnly,
    );
}

// --- Explicit conversion (num_candidates > 1) ---

#[test]
fn strategy_explicit_adaptive_true_returns_light_model() {
    let config = default_strategy_config();
    // adaptive=true → LightModelOnly (main model was too slow)
    assert_eq!(
        determine_conversion_strategy(5, 9, true, true, &config),
        ConversionStrategy::LightModelOnly,
    );
}

#[test]
fn strategy_explicit_short_reading_returns_parallel_beam() {
    let config = default_strategy_config();
    // adaptive=false, reading_tokens=5 <= 10 → ParallelBeam
    assert_eq!(
        determine_conversion_strategy(5, 9, true, false, &config),
        ConversionStrategy::ParallelBeam { beam_width: 3 },
    );
}

#[test]
fn strategy_explicit_long_reading_returns_light_model() {
    let config = default_strategy_config();
    // adaptive=false, reading_tokens=15 > 10 → LightModelOnly
    assert_eq!(
        determine_conversion_strategy(15, 9, true, false, &config),
        ConversionStrategy::LightModelOnly,
    );
}

#[test]
fn strategy_explicit_reading_boundary_at_threshold() {
    let config = default_strategy_config();
    // reading_tokens == threshold → ParallelBeam (<=)
    assert_eq!(
        determine_conversion_strategy(10, 9, true, false, &config),
        ConversionStrategy::ParallelBeam { beam_width: 3 },
    );
    // reading_tokens == threshold + 1 → LightModelOnly
    assert_eq!(
        determine_conversion_strategy(11, 9, true, false, &config),
        ConversionStrategy::LightModelOnly,
    );
}

// --- beam_width capping ---

#[test]
fn strategy_beam_width_capped_by_num_candidates() {
    let config = strategy_config(10, 5);
    // num_candidates=2 < beam_width=5 → beam_width = min(2, 5) = 2
    assert_eq!(
        determine_conversion_strategy(5, 2, true, false, &config),
        ConversionStrategy::ParallelBeam { beam_width: 2 },
    );
}

#[test]
fn strategy_beam_width_capped_by_beam_width() {
    let config = strategy_config(10, 3);
    // num_candidates=9 > beam_width=3 → beam_width = min(9, 3) = 3
    assert_eq!(
        determine_conversion_strategy(5, 9, true, false, &config),
        ConversionStrategy::ParallelBeam { beam_width: 3 },
    );
}

// --- Adaptive latency-based model switching ---

#[test]
fn strategy_adaptive_flag_overrides_short_input_for_explicit() {
    let config = default_strategy_config();
    // Short reading but adaptive=true → LightModelOnly (not ParallelBeam)
    assert_eq!(
        determine_conversion_strategy(3, 9, true, true, &config),
        ConversionStrategy::LightModelOnly,
    );
}

#[test]
fn strategy_adaptive_false_long_reading_still_uses_light() {
    let config = default_strategy_config();
    // Long reading, adaptive=false → LightModelOnly (proactive, too long for beam)
    assert_eq!(
        determine_conversion_strategy(20, 9, true, false, &config),
        ConversionStrategy::LightModelOnly,
    );
}

// --- Engine-level adaptive flag behavior ---

#[test]
fn test_adaptive_flag_initial_state() {
    let engine = InputMethodEngine::new();
    assert!(!engine.metrics.adaptive_use_light_model);
}

#[test]
fn test_adaptive_flag_reset_on_engine_reset() {
    let mut engine = InputMethodEngine::new();
    engine.metrics.adaptive_use_light_model = true;
    engine.reset();
    assert!(!engine.metrics.adaptive_use_light_model);
}

#[test]
fn test_adaptive_flag_reset_on_new_word() {
    let mut engine = InputMethodEngine::new();
    engine.metrics.adaptive_use_light_model = true;

    // Process a key in Empty state → flag should be reset
    engine.process_key(&press('a'));
    assert!(!engine.metrics.adaptive_use_light_model);
}

#[test]
fn test_adaptive_flag_persists_during_input() {
    let mut engine = InputMethodEngine::new();

    // Start typing (flag reset on first key from Empty)
    engine.process_key(&press('a'));
    assert!(!engine.metrics.adaptive_use_light_model);

    // Manually set the flag (simulating slow conversion)
    engine.metrics.adaptive_use_light_model = true;

    // Continue typing — flag should persist (not in Empty state)
    engine.process_key(&press('i'));
    assert!(engine.metrics.adaptive_use_light_model);
}

#[test]
fn test_adaptive_flag_reset_after_commit_and_new_input() {
    let mut engine = InputMethodEngine::new();

    // Type and set flag
    engine.process_key(&press('a'));
    engine.metrics.adaptive_use_light_model = true;

    // Commit
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(matches!(engine.state(), InputState::Empty));
    // Flag is still true (reset happens on next key in Empty state)
    assert!(engine.metrics.adaptive_use_light_model);

    // Start new word → flag reset
    engine.process_key(&press('k'));
    assert!(!engine.metrics.adaptive_use_light_model);
}

#[test]
fn test_config_default_max_latency_ms() {
    let config = EngineConfig::default();
    assert_eq!(config.max_latency_ms, 100);
}
