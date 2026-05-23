//! Tests for emoji-shortcode input mode (`:smile` → 😄).

use super::*;

/// Press `:` (keysym 0x003A, sent with Shift on a US layout, but
/// fcitx5 normally resolves it as the literal keysym).
fn press_colon() -> KeyEvent {
    KeyEvent::press(Keysym(b':' as u32))
}

/// Texts from the most recent ShowCandidates action — auto-suggest
/// surfaces candidates here rather than parking them in
/// `engine.state()`, so this is the canonical way to inspect them
/// during the Composing phase.
fn auto_suggest_texts(result: &crate::core::engine::EngineResult) -> Vec<String> {
    use crate::core::engine::EngineAction;
    result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::ShowCandidates(list) => {
                Some(list.candidates().iter().map(|c| c.text.clone()).collect())
            }
            _ => None,
        })
        .unwrap_or_default()
}

#[test]
fn typing_colon_in_empty_enters_emoji_mode() {
    let mut engine = InputMethodEngine::new();
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    let result = engine.process_key(&press_colon());
    assert!(result.consumed);
    assert_eq!(engine.input_mode, InputMode::Emoji);
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    // Preedit shows the literal `:` rather than any kana — emoji mode
    // is supposed to bypass romaji conversion.
    assert_eq!(engine.preedit().unwrap().text(), ":");
}

#[test]
fn ascii_after_colon_stays_literal() {
    // Confirms the user's spec: `:pien` must remain `:pien`, not get
    // romaji-converted into hiragana while the user is still typing.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['p', 'i', 'e', 'n'] {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_mode, InputMode::Emoji);
    assert_eq!(engine.preedit().unwrap().text(), ":pien");
}

#[test]
fn emoji_mode_shows_candidates_via_rewriter() {
    // After enough chars to anchor a shortcode (`:smile`), the
    // EmojiRewriter should be surfacing emoji candidates through the
    // auto-suggest path — inspected via the most recent
    // `ShowCandidates` action since composing-phase candidates aren't
    // parked in `engine.state()`.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    engine.process_key(&press('s'));
    engine.process_key(&press('m'));
    engine.process_key(&press('i'));
    engine.process_key(&press('l'));
    let last = engine.process_key(&press('e'));

    let texts = auto_suggest_texts(&last);
    assert!(
        texts.iter().any(|t| t == "😄"),
        "expected 😄 in candidates, got {:?}",
        texts
    );
}

#[test]
fn escape_commits_literal_and_exits_emoji_mode() {
    // Slack-style escape: pressing ESC in emoji mode dismisses the
    // picker AND commits whatever the user typed as plain text. Two
    // reasons:
    //   * The typed `:smile` shouldn't silently vanish — that would
    //     be surprising; users expect what they typed to land somewhere.
    //   * It gives a deliberate way to commit `:smile` literally even
    //     when an emoji match exists, which Enter alone can't do
    //     (Enter on a match commits the emoji).
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_mode, InputMode::Emoji);

    let result = engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(commit_text(&result).as_deref(), Some(":smile"));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn committing_emoji_resets_to_hiragana() {
    // Selecting an emoji candidate then committing must also drop emoji
    // mode so the user's next keypress lands in normal kana input.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    // Space starts conversion → first candidate selected → Return commits.
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert!(matches!(engine.state(), InputState::Empty));
}

/// Extract the text from the first `Commit` action in `result`, if any.
fn commit_text(result: &crate::core::engine::EngineResult) -> Option<String> {
    use crate::core::engine::EngineAction;
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(t) => Some(t.clone()),
        _ => None,
    })
}

#[test]
fn enter_on_emoji_query_commits_emoji_not_literal() {
    // The headline fix: pressing Enter on `:smile` from the Composing
    // phase must commit 😄, not the literal text `:smile`. Otherwise
    // emoji mode is useless — the user would have to explicitly enter
    // the conversion list (Space/Down) before every commit.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&result).as_deref(), Some("😄"));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
}

#[test]
fn conversion_emoji_first_not_literal() {
    // After Space in emoji mode, the conversion candidate list should
    // surface 😄 ahead of the literal `:smile`. Previously the
    // hiragana/katakana fallback (the literal `:smile` text) sat above
    // rewriter candidates and `:smile` became the default selection —
    // exactly the noise the user wants suppressed for an explicit
    // emoji-mode session.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    let result = engine.process_key(&press_key(Keysym::SPACE));

    // Selected text on entering Conversion comes from the first
    // candidate; assert it's the emoji, not the literal.
    let candidates = engine
        .candidates()
        .expect("expected Conversion candidate list");
    let first = candidates
        .candidates()
        .first()
        .expect("expected at least one candidate")
        .text
        .as_str();
    assert_eq!(first, "😄", "expected 😄 as first candidate, got {}", first);

    // The conversion preedit (sent via UpdatePreedit) should also be
    // the emoji, not `:smile`, since the first candidate is selected
    // by default on entering Conversion.
    use crate::core::engine::EngineAction;
    let preedit_text = result
        .actions
        .iter()
        .find_map(|a| match a {
            EngineAction::UpdatePreedit(p) => Some(p.text().to_string()),
            _ => None,
        })
        .unwrap_or_default();
    assert_eq!(preedit_text, "😄");
}

#[test]
fn conversion_unknown_emoji_shows_no_literal() {
    // Slack-style mental model: the emoji picker shows emojis or
    // nothing — never the literal `:qqqq` query. When the user's
    // input doesn't match any emoji, the candidate list must NOT
    // include the literal buffer text. Enter still commits the
    // literal via `commit_composing` as an escape hatch (covered by
    // `enter_on_unknown_emoji_query_commits_literal`).
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['q', 'q', 'q', 'q'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    let texts: Vec<String> = engine
        .candidates()
        .map(|list| list.candidates().iter().map(|c| c.text.clone()).collect())
        .unwrap_or_default();
    assert!(
        !texts.iter().any(|t| t == ":qqqq"),
        "did NOT expect :qqqq literal in emoji-mode candidates, got {:?}",
        texts
    );
}

#[test]
fn enter_on_unknown_emoji_query_commits_literal() {
    // `:qqqq` has no emoji match — falling back to the literal buffer
    // text is the only sensible thing to do so the user sees what they
    // typed and can correct it.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['q', 'q', 'q', 'q'] {
        engine.process_key(&press(ch));
    }
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(commit_text(&result).as_deref(), Some(":qqqq"));
}

#[test]
fn typing_kiniku_surfaces_muscle_via_silent_n() {
    // Regression: user-reported that `:kiniku` didn't surface 💪
    // even though the natural finger-pattern for きんにく (筋肉) is
    // `kiniku` — the leading `n` of `niku` swallows the ん. The
    // porter now emits both the silent-ん form (`kiniku`) and the
    // strict double-n form (`kinniku`) as triggers, so both queries
    // must reach 💪.
    for query in ["kiniku", "kinniku"] {
        let mut engine = InputMethodEngine::new();
        engine.process_key(&press_colon());
        for ch in query.chars() {
            engine.process_key(&press(ch));
        }
        let last_show = engine
            .process_key(&press_key(Keysym::SPACE))
            .actions
            .into_iter()
            .find_map(|a| match a {
                crate::core::engine::EngineAction::ShowCandidates(list) => Some(list),
                _ => None,
            })
            .or_else(|| engine.candidates().cloned())
            .unwrap_or_else(|| panic!("no candidate list after :{}", query));
        let texts: Vec<String> = last_show
            .candidates()
            .iter()
            .map(|c| c.text.clone())
            .collect();
        assert!(
            texts.contains(&"💪".to_string()),
            "expected 💪 from :{}, got {:?}",
            query,
            texts
        );
    }
}

#[test]
fn backspacing_to_empty_exits_emoji_mode() {
    // Regression: deleting back through the leading `:` left the
    // engine in Emoji mode even though the buffer was empty. The
    // next typed char would then be inserted literally (e.g. `a`
    // staying as `a` instead of romaji-converting to `あ`) and the
    // aux text would keep showing `[☺]`. After erasing back to
    // empty, the session is over and the mode should drop back to
    // Hiragana.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_mode, InputMode::Emoji);

    for _ in 0..6 {
        engine.process_key(&press_key(Keysym::BACKSPACE));
    }
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_mode, InputMode::Hiragana);

    // And the very next keypress should behave like normal kana
    // input — `a` becomes `あ`, not literal `a`.
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "あ");
}

#[test]
fn backspacing_to_empty_restores_pre_emoji_katakana_mode() {
    // Same exit path as the empty-backspace case, but verifies the
    // pre-emoji mode is *restored* rather than forced back to Hiragana.
    // A user typing in Katakana who pops into emoji and bails out
    // should land back in Katakana.
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Katakana;

    engine.process_key(&press_colon());
    assert_eq!(engine.input_mode, InputMode::Emoji);
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }

    for _ in 0..6 {
        engine.process_key(&press_key(Keysym::BACKSPACE));
    }
    assert!(matches!(engine.state(), InputState::Empty));
    assert_eq!(engine.input_mode, InputMode::Katakana);
}

#[test]
fn commit_emoji_restores_pre_emoji_katakana_mode() {
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Katakana;

    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(engine.input_mode, InputMode::Katakana);
}

#[test]
fn escape_emoji_restores_pre_emoji_katakana_mode() {
    let mut engine = InputMethodEngine::new();
    engine.input_mode = InputMode::Katakana;

    engine.process_key(&press_colon());
    for ch in ['s', 'm', 'i', 'l', 'e'] {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(engine.input_mode, InputMode::Katakana);
}

#[test]
fn colon_in_hiragana_does_not_enter_emoji_when_already_composing() {
    // A `:` typed in the middle of an existing hiragana composition is
    // just punctuation, not an emoji trigger — emoji mode only starts
    // from Empty state.
    let mut engine = InputMethodEngine::new();
    engine.process_key(&press('a'));
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    assert!(matches!(engine.state(), InputState::Composing { .. }));

    engine.process_key(&press_colon());
    assert_eq!(engine.input_mode, InputMode::Hiragana);
    // The `:` should have been absorbed by the existing composition,
    // not have triggered emoji mode.
    assert!(engine.preedit().unwrap().text().contains('あ'));
}
