//! `[display] candidate_window`: whether the candidate window opens while
//! typing or only once a conversion starts.

use super::*;
use crate::config::settings::CandidateWindow;

const AI_DICT: &str = r#"[{"reading":"あい","candidates":[{"surface":"愛","score":1.0}]}]"#;

/// `candidate_window = "conversion"`, live conversion on.
fn conversion_window_engine() -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        live_conversion: true,
        candidate_window: CandidateWindow::Conversion,
        ..EngineConfig::default()
    });
    engine.dicts.user = Some(dict_from_json(AI_DICT));
    engine
}

fn has(result: &EngineResult, action: impl Fn(&EngineAction) -> bool) -> bool {
    result.actions.iter().any(action)
}

fn shows_candidates(result: &EngineResult) -> bool {
    has(result, |a| matches!(a, EngineAction::ShowCandidates(_)))
}

fn opens_window(result: &EngineResult) -> bool {
    shows_candidates(result) || last_aux_text(result).is_some()
}

fn closes_window(result: &EngineResult) -> bool {
    has(result, |a| matches!(a, EngineAction::HideCandidates))
        && has(result, |a| matches!(a, EngineAction::HideAuxText))
}

#[test]
fn test_typing_opens_no_window() {
    let mut engine = conversion_window_engine();
    let result = engine.process_key(&press('a'));
    assert!(!opens_window(&result), "{:?}", result.actions);
    let result = engine.process_key(&press('i'));
    assert!(closes_window(&result), "{:?}", result.actions);
    assert!(!opens_window(&result), "{:?}", result.actions);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あい");
}

#[test]
fn test_caret_move_opens_no_window() {
    // Renders other than typing go through the same gate.
    let mut engine = conversion_window_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_key(Keysym::LEFT));
    assert!(!opens_window(&result), "{:?}", result.actions);
    assert_eq!(engine.input_buf.cursor(), 1);
}

#[test]
fn test_shortcuts_open_no_window() {
    // The state-independent shortcuts re-render the composition and
    // return before the state dispatch; they go through the gate too.
    let mut engine = conversion_window_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let verbose = engine.process_key(&press_ctrl_shift(Keysym::KEY_V));
    assert!(!opens_window(&verbose), "verbose: {:?}", verbose.actions);
    let live_off = engine.process_key(&press_ctrl_shift(Keysym::KEY_L));
    assert!(!opens_window(&live_off), "live off: {:?}", live_off.actions);
    let live_on = engine.process_key(&press_ctrl_shift(Keysym::KEY_L));
    assert!(!opens_window(&live_on), "live on: {:?}", live_on.actions);
    engine.process_key(&press_ctrl(Keysym::KEY_K));
    assert_eq!(engine.mode.current(), InputMode::Katakana);
    let back = engine.process_key(&press_key(Keysym::HENKAN));
    assert_eq!(engine.mode.current(), InputMode::Hiragana);
    assert!(!opens_window(&back), "mode toggle: {:?}", back.actions);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn test_hidden_candidates_cannot_be_selected() {
    let mut engine = conversion_window_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_ctrl(Keysym::KEY_1));
    assert!(result.consumed);
    assert!(!has(&result, |a| matches!(a, EngineAction::Commit(_))));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
}

#[test]
fn test_space_opens_the_conversion_window() {
    let mut engine = conversion_window_engine();
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(matches!(engine.state(), InputState::Conversion { .. }));
    assert!(shows_candidates(&result), "{:?}", result.actions);
    assert!(last_aux_text(&result).is_some());
    // Escape drops back to typing, and the window closes with it.
    let result = engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert!(closes_window(&result), "{:?}", result.actions);
    assert!(!opens_window(&result), "{:?}", result.actions);
}

#[test]
fn test_live_conversion_off_changes_nothing() {
    // Without live conversion the preedit stays kana and Space converts,
    // the classic shape: the window still waits for the conversion.
    let mut engine = conversion_window_engine();
    engine.process_key(&press_ctrl_shift(Keysym::KEY_L));
    assert!(!engine.live.enabled);
    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));
    assert!(!opens_window(&result), "{:?}", result.actions);
    assert_eq!(engine.preedit().unwrap().text(), "あい");
    let result = engine.process_key(&press_key(Keysym::SPACE));
    assert!(shows_candidates(&result), "{:?}", result.actions);
}

#[test]
fn test_emoji_picker_still_shows() {
    let mut engine = conversion_window_engine();
    engine.process_key(&press(':'));
    let mut result = EngineResult::default();
    for ch in "smile".chars() {
        result = engine.process_key(&press(ch));
    }
    assert_eq!(engine.mode.current(), InputMode::Emoji);
    assert!(shows_candidates(&result), "{:?}", result.actions);
}

#[test]
fn test_window_opens_while_typing_by_default() {
    let mut engine = make_live_conversion_engine();
    engine.dicts.user = Some(dict_from_json(AI_DICT));
    engine.process_key(&press('a'));
    let result = engine.process_key(&press('i'));
    assert!(shows_candidates(&result), "{:?}", result.actions);
    assert!(last_aux_text(&result).is_some());
}
