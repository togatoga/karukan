//! Ctrl+K is the katakana conversion (F7), not a mode: the composition
//! opens as a conversion with katakana selected, and the next word is
//! typed in hiragana again.

use super::*;

fn type_aiueo(engine: &mut InputMethodEngine) {
    for ch in ['a', 'i', 'u', 'e', 'o'] {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");
}

#[test]
fn test_ctrl_k_converts_to_katakana() {
    let mut engine = InputMethodEngine::new();
    type_aiueo(&mut engine);

    let result = engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert!(result.consumed);
    assert!(
        !result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(_))),
        "Ctrl+K must not commit"
    );
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    let enter = engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        enter
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if t == "アイウエオ"))
    );

    // No mode was left behind: the next word is hiragana.
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn test_ctrl_k_uppercase_keysym_converts_to_katakana() {
    // Some environments fold Shift into an uppercase keysym.
    let mut engine = InputMethodEngine::new();
    type_aiueo(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_K_UPPER));
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");
}

#[test]
fn test_ctrl_k_with_empty_input_passes_through() {
    let mut engine = InputMethodEngine::new();
    let result = engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert!(!result.consumed);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_ctrl_k_during_conversion_selects_katakana() {
    let mut engine = InputMethodEngine::new();
    type_aiueo(&mut engine);
    engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));

    engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert_eq!(engine.preedit().unwrap().text(), "アイウエオ");
}

#[test]
fn test_escape_after_ctrl_k_returns_to_hiragana() {
    let mut engine = InputMethodEngine::new();
    type_aiueo(&mut engine);
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あいうえお");
}
