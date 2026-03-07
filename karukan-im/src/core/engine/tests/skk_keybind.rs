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

    // Press 'l' to switch to alphabet — composing text should be committed
    let result = engine.process_key(&press('l'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Alphabet);
    // State should be Empty after commit
    assert!(matches!(engine.state(), InputState::Empty));
    // Should have a Commit action with the composed text
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "あ"))
    );
}

#[test]
fn test_skk_alphabet_mode_keys_pass_through() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Alphabet;

    // q should pass through in alphabet mode
    let result = engine.process_key(&press('q'));
    assert!(!result.consumed);

    // Ctrl+q should pass through in alphabet mode
    let result = engine.process_key(&press_ctrl(Keysym::KEY_Q));
    assert!(!result.consumed);

    // Only Ctrl+j should be consumed
    let result = engine.process_key(&press_ctrl(Keysym::KEY_J));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn test_skk_q_in_composing_commits_katakana() {
    let mut engine = make_skk_engine();

    // Type "a" to get "あ" in composing state
    engine.process_key(&press('a'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    // Press 'q' — should commit as katakana and return to hiragana
    let result = engine.process_key(&press('q'));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "ア"))
    );
}

#[test]
fn test_skk_l_in_composing_katakana_commits_katakana() {
    let mut engine = make_skk_engine();
    engine.input_mode = InputMode::Katakana;

    // Type "a" to get composing state (hiragana internally)
    engine.process_key(&press('a'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    // Press 'l' — should commit as katakana then switch to alphabet
    let result = engine.process_key(&press('l'));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Alphabet);
    assert!(matches!(engine.state(), InputState::Empty));
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "ア"))
    );
}

#[test]
fn test_skk_ctrl_q_in_composing_commits_halfwidth_katakana() {
    let mut engine = make_skk_engine();

    // Type "a" to get "あ" in composing state
    engine.process_key(&press('a'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    // Press Ctrl+q — should commit as half-width katakana and return to hiragana
    let result = engine.process_key(&press_ctrl(Keysym::KEY_Q));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "ｱ"))
    );
}

#[test]
fn test_skk_ctrl_q_in_empty_switches_mode() {
    let mut engine = make_skk_engine();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    // Ctrl+q in empty state switches to half-width katakana mode
    let result = engine.process_key(&press_ctrl(Keysym::KEY_Q));
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::HalfWidthKatakana);
    assert!(matches!(engine.state(), InputState::Empty));
}
