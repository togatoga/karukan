//! Numeric keypad handling (issue #51): KP_* keysyms either behave like
//! their main-row equivalents (default) or bypass the IME entirely when
//! `[input] numpad_direct_input` is on.

use super::*;

fn engine_with_numpad_direct_input() -> InputMethodEngine {
    InputMethodEngine::with_config(EngineConfig {
        numpad_direct_input: true,
        ..Default::default()
    })
}

#[test]
fn test_keypad_digit_behaves_like_main_row_by_default() {
    let mut engine = InputMethodEngine::new();

    let result = engine.process_key(&press_key(Keysym(Keysym::KP_0.0 + 5)));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "5");
}

#[test]
fn test_keypad_operators_behave_like_main_row_by_default() {
    // KP_Divide must be indistinguishable from the main-row `/`, whatever
    // the configured slash style happens to be.
    let mut main_row = InputMethodEngine::new();
    let mut keypad = InputMethodEngine::new();

    let main_result = main_row.process_key(&press('/'));
    let kp_result = keypad.process_key(&press_key(Keysym::KP_DIVIDE));

    assert_eq!(kp_result.consumed, main_result.consumed);
    assert!(matches!(keypad.state(), InputState::Composing { .. }));
    assert_eq!(
        keypad.preedit().unwrap().text(),
        main_row.preedit().unwrap().text()
    );
}

#[test]
fn test_keypad_enter_commits_like_return() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    let result = engine.process_key(&press_key(Keysym::KP_ENTER));
    assert!(result.consumed);
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "あ"));
    assert!(has_commit);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_keypad_enter_acts_as_return_even_with_direct_input() {
    let mut engine = engine_with_numpad_direct_input();

    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Even with numpad_direct_input on, keypad Enter stays the confirm key.
    let result = engine.process_key(&press_key(Keysym::KP_ENTER));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_keypad_passes_through_with_direct_input() {
    let mut engine = engine_with_numpad_direct_input();

    let result = engine.process_key(&press_key(Keysym(Keysym::KP_0.0 + 5)));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));

    // Operators and the decimal point pass through too.
    for keysym in [
        Keysym::KP_DIVIDE,
        Keysym::KP_MULTIPLY,
        Keysym::KP_SUBTRACT,
        Keysym::KP_ADD,
        Keysym::KP_EQUAL,
        Keysym::KP_DECIMAL,
    ] {
        let result = engine.process_key(&press_key(keysym));
        assert!(
            !result.consumed,
            "keysym 0x{:04x} should pass through",
            keysym.0
        );
        assert!(matches!(engine.state(), InputState::Empty));
    }
}

#[test]
fn test_keypad_passes_through_mid_composition_with_direct_input() {
    let mut engine = engine_with_numpad_direct_input();

    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // The numpad key goes straight to the app; the composition is untouched.
    let result = engine.process_key(&press_key(Keysym(Keysym::KP_0.0 + 1)));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_main_row_unaffected_by_direct_input() {
    let mut engine = engine_with_numpad_direct_input();

    // Main-row keys keep going through the IME even with the option on.
    let result = engine.process_key(&press('5'));
    assert!(result.consumed);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "5");
}
