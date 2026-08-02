//! Pending-romaji editing tests: backspace over the raw segment must let
//! remaining keystrokes re-combine, while settled text never reverts.

use super::*;

fn preedit_text(engine: &InputMethodEngine) -> String {
    engine.preedit().unwrap().text().to_string()
}

/// The original bug: `ykt` → BS → `o` used to produce 「ykお」 because the
/// passed-through `k` was settled the moment `t` arrived. Keeping the whole
/// consonant run pending lets `o` re-derive `yko` → 「yこ」.
#[test]
fn test_ykt_backspace_then_o() {
    let mut engine = InputMethodEngine::new();

    for ch in "ykt".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "ykt");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "yk");

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "yこ");
}

/// A consonant stranded behind settled text stays at its position — it is
/// part of the reading, not the aux/live tail, so it never teleports to
/// the end of the display.
#[test]
fn test_stranded_consonant_stays_in_place() {
    let mut engine = InputMethodEngine::new();

    for ch in "y1k".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "y1k");
    // Only the k (adjacent to the cursor) is being typed; y is stranded
    assert_eq!(engine.input_buf.reading(), "y1");
    assert_eq!(engine.input_buf.pending(), "k");

    engine.process_key(&press('a'));
    assert_eq!(preedit_text(&engine), "y1か");
    assert_eq!(engine.input_buf.reading(), "y1か");
    assert_eq!(engine.input_buf.pending(), "");
}

/// Same scenario with live conversion on: the preedit is built from
/// live text + pending, which must not move the stranded y to the end.
#[test]
fn test_stranded_consonant_stays_in_place_live() {
    let mut engine = make_live_conversion_engine();

    for ch in "y1a".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "y1あ");
}

/// Deleting a converted element re-exposes the live consonants before it,
/// one conversion at a time: `ytko` → BS → `o` → BS → `o` → BS → `o`.
#[test]
fn test_ytko_backspace_recombines_stepwise() {
    let mut engine = InputMethodEngine::new();

    for ch in "ytko".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "ytこ");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "yt");

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "yと");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "y");

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "よ");
}

/// A multi-key rule prefix survives deletion of the element after it:
/// `kyt` → BS → `o` re-combines to きょ.
#[test]
fn test_kyt_backspace_then_o() {
    let mut engine = InputMethodEngine::new();

    for ch in "kyt".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "kyt");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "ky");

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "きょ");
}

/// Settled text never reverts: っ stays っ after the pending `k` is erased.
#[test]
fn test_kk_backspace_keeps_sokuon() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('k'));
    engine.process_key(&press('k'));
    assert_eq!(preedit_text(&engine), "っk");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "っ");

    engine.process_key(&press('a'));
    assert_eq!(preedit_text(&engine), "っあ");
}

/// ん from the n-before-consonant rule stays settled after backspace.
#[test]
fn test_nk_backspace_keeps_n() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('n'));
    engine.process_key(&press('k'));
    assert_eq!(preedit_text(&engine), "んk");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "ん");

    engine.process_key(&press('a'));
    assert_eq!(preedit_text(&engine), "んあ");
}

/// Backspace on fully converted text removes one display character.
#[test]
fn test_kyo_backspace_then_o() {
    let mut engine = InputMethodEngine::new();

    for ch in "kyo".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "きょ");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "き");

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "きお");
}

/// Pending keystrokes pop one at a time; erasing everything resets the state.
#[test]
fn test_kan_backspace_to_empty() {
    let mut engine = InputMethodEngine::new();

    for ch in "kan".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(preedit_text(&engine), "かn");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(preedit_text(&engine), "か");

    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(matches!(engine.state(), InputState::Empty));
}

/// A consonant run stays fully erasable key by key.
#[test]
fn test_consonant_run_backspace_to_empty() {
    let mut engine = InputMethodEngine::new();

    for ch in "ykt".chars() {
        engine.process_key(&press(ch));
    }
    for expected in ["yk", "y"] {
        engine.process_key(&press_key(Keysym::BACKSPACE));
        assert_eq!(preedit_text(&engine), expected);
    }
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert!(matches!(engine.state(), InputState::Empty));
}

/// Returning from conversion to composing keeps the reading editable
/// (no stale pending romaji).
#[test]
fn test_conversion_escape_then_continue_typing() {
    let mut engine = InputMethodEngine::new();

    for ch in "kyo".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(preedit_text(&engine), "きょ");

    engine.process_key(&press('u'));
    assert_eq!(preedit_text(&engine), "きょう");
}

/// Cursor movement keeps unevaluated romaji live: coming back and typing a
/// vowel still combines.
#[test]
fn test_cursor_move_keeps_pending_live() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('a'));
    engine.process_key(&press('k'));
    assert_eq!(preedit_text(&engine), "あk");

    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(preedit_text(&engine), "あk");
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    engine.process_key(&press_key(Keysym::END));
    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "あこ");
}

/// Typing before a Direct element combines with the live romaji to its
/// left: `k`, `y`, `Shift+K`, ←, `o` → 「きょK」. Nothing combines across
/// the cursor, so the `K` stays untouched.
#[test]
fn test_type_before_direct_element_combines() {
    let mut engine = InputMethodEngine::new();

    engine.process_key(&press('k'));
    engine.process_key(&press('y'));
    engine.process_key(&press_shift('K'));
    assert_eq!(preedit_text(&engine), "kyK");

    // Moving ends the temporary alphabet word; `ky` is still live
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().caret(), 2);

    engine.process_key(&press('o'));
    assert_eq!(preedit_text(&engine), "きょK");
    assert_eq!(engine.preedit().unwrap().caret(), 2);
}
