use super::*;
use crate::config::settings::KeybindingProfile;

fn make_skk_engine() -> InputMethodEngine {
    let config = EngineConfig {
        keybinding_profile: KeybindingProfile::Skk,
        ..EngineConfig::default()
    };
    InputMethodEngine::with_config(config)
}

#[test]
fn test_skk_l_in_empty_hiragana_switches_to_alphabet() {
    let mut engine = make_skk_engine();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press('l'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Alphabet);
}

#[test]
fn test_skk_l_in_empty_alphabet_passes_through() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Alphabet;

    let result = engine.process_key(&press('l'));
    // In SKK alphabet mode, all keys pass through to the application
    assert!(!result.consumed);
    assert_eq!(engine.input_mode, InputMode::Alphabet);
}

#[test]
fn test_skk_q_toggle_hiragana_to_katakana() {
    let mut engine = make_skk_engine();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press('q'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Katakana);
}

#[test]
fn test_skk_q_toggle_katakana_to_hiragana() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Katakana;

    let result = engine.process_key(&press('q'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_ctrl_j_enters_hiragana_from_alphabet() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Alphabet;

    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_ctrl_j_enters_hiragana_from_katakana() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Katakana;

    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_ctrl_j_noop_in_hiragana() {
    let mut engine = make_skk_engine();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_ctrl_q_enters_halfwidth_katakana() {
    let mut engine = make_skk_engine();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press_ctrl(Keysym::KEY_Q));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::HalfWidthKatakana);
}

#[test]
fn test_default_profile_l_passes_through() {
    // With default profile, 'l' should NOT be intercepted by SKK
    let mut engine = InputMethodEngine::new();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press('l'));
    // 'l' starts romaji composing in default mode (not SKK intercept)
    assert!(result.consumed);
    // Should remain in Hiragana mode (romaji 'l' buffer)
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_default_profile_q_passes_through() {
    // With default profile, 'q' should NOT be intercepted by SKK
    let mut engine = InputMethodEngine::new();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press('q'));
    // 'q' is a passthrough character in romaji, auto-commits
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_l_in_composing_switches_to_alphabet() {
    let mut engine = make_skk_engine();

    // Type "a" to enter composing state
    engine.process_key(&press('a'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    // Press 'l' to switch to alphabet
    let result = engine.process_key(&press('l'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Alphabet);
    // Preedit should be preserved (not committed)
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn test_skk_halfwidth_katakana_display() {
    let mut engine = make_skk_engine();

    // Type "a" to get "あ"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Switch to half-width katakana via Ctrl+q
    engine.process_key(&press_ctrl(Keysym::KEY_Q));
    assert_eq!(engine.input_mode, InputMode::HalfWidthKatakana);
    // Preedit should show half-width katakana
    assert_eq!(engine.preedit().unwrap().text(), "ｱ");
}
