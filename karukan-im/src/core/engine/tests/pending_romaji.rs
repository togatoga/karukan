//! Pending-romaji editing scenarios, table-driven.
//!
//! Each case is a list of (keys, expected) steps: the keys are sent one
//! keystroke at a time, then the preedit is asserted. Key tokens:
//! `←` Left, `⌫` Backspace, `⇥` End, `␛` Escape, `␣` Space, `↹` Tab,
//! `変` HENKAN
//! (mode toggle to hiragana); an ASCII uppercase letter is typed with
//! Shift; anything else is a plain key. An expected value of `"きょ@2"`
//! also asserts the caret, and `"∅"` asserts the Empty state.

use super::*;

fn preedit_text(engine: &InputMethodEngine) -> String {
    engine.preedit().unwrap().text().to_string()
}

fn send(engine: &mut InputMethodEngine, keys: &str) {
    for ch in keys.chars() {
        let key = match ch {
            '←' => press_key(Keysym::LEFT),
            '⌫' => press_key(Keysym::BACKSPACE),
            '⇥' => press_key(Keysym::END),
            '␛' => press_key(Keysym::ESCAPE),
            '␣' => press_key(Keysym::SPACE),
            '↹' => press_key(Keysym::TAB),
            '変' => press_key(Keysym::HENKAN),
            c if c.is_ascii_uppercase() => press_shift(c),
            c => press(c),
        };
        engine.process_key(&key);
    }
}

fn run_scenario(name: &str, steps: &[(&str, &str)]) {
    let mut engine = InputMethodEngine::new();
    for (keys, expected) in steps {
        send(&mut engine, keys);
        if *expected == "∅" {
            assert!(
                matches!(engine.state(), InputState::Empty),
                "{name}: expected Empty state after {keys:?}"
            );
            continue;
        }
        let (text, caret) = match expected.split_once('@') {
            Some((text, caret)) => (text, Some(caret.parse::<usize>().expect("caret number"))),
            None => (*expected, None),
        };
        assert_eq!(preedit_text(&engine), text, "{name}: after {keys:?}");
        if let Some(caret) = caret {
            assert_eq!(
                engine.preedit().unwrap().caret(),
                caret,
                "{name}: caret after {keys:?}"
            );
        }
    }
}

#[test]
fn test_editing_scenarios() {
    let cases: &[(&str, &[(&str, &str)])] = &[
        // The original bug: the passed-through consonants stay live, so
        // erasing the pending `t` lets `o` re-combine with the `k`
        (
            "ykt_backspace_then_o",
            &[("ykt", "ykt"), ("⌫", "yk"), ("o", "yこ")],
        ),
        // Deleting a converted element re-exposes the live consonants
        // before it, one conversion at a time
        (
            "ytko_backspace_recombines_stepwise",
            &[
                ("ytko", "ytこ"),
                ("⌫", "yt"),
                ("o", "yと"),
                ("⌫", "y"),
                ("o", "よ"),
            ],
        ),
        // A multi-key rule prefix survives deletion of the element after it
        (
            "kyt_backspace_then_o",
            &[("kyt", "kyt"), ("⌫", "ky"), ("o", "きょ")],
        ),
        // Settled text never reverts
        (
            "kk_backspace_keeps_sokuon",
            &[("kk", "っk"), ("⌫", "っ"), ("a", "っあ")],
        ),
        (
            "nk_backspace_keeps_n",
            &[("nk", "んk"), ("⌫", "ん"), ("a", "んあ")],
        ),
        (
            "kyo_backspace_then_o",
            &[("kyo", "きょ"), ("⌫", "き"), ("o", "きお")],
        ),
        // Erasing everything resets the state
        (
            "kan_backspace_to_empty",
            &[("kan", "かn"), ("⌫", "か"), ("⌫", "∅")],
        ),
        (
            "consonant_run_backspace_to_empty",
            &[("ykt", "ykt"), ("⌫", "yk"), ("⌫", "y"), ("⌫", "∅")],
        ),
        // Stranded consonants stay live across cursor movement and combine
        // when typing returns next to them
        (
            "cursor_move_keeps_pending_live",
            &[("ak", "あk"), ("←", "あk@1"), ("⇥o", "あこ")],
        ),
        (
            "stranded_consonants_combine_after_cursor_return",
            &[("ky123", "ky123"), ("←←←", "ky123@2"), ("o", "きょ123@2")],
        ),
        (
            "ky123ni_cursor_return_combines",
            &[
                ("ky123ni", "ky123に"),
                ("←←←←", "ky123に@2"),
                ("o", "きょ123に@2"),
            ],
        ),
        // Evaluation never crosses the caret: elements right of it stay put
        (
            "type_before_direct_element_combines",
            &[("kyK", "kyK"), ("←", "kyK@2"), ("o", "きょK@2")],
        ),
        (
            "combine_before_converted_and_direct",
            &[("ky1K", "ky1K"), ("←←", "ky1K@2"), ("o", "きょ1K@2")],
        ),
        // Mode transitions never touch the element array
        (
            "mode_toggle_back_keeps_live_romaji",
            &[("ky1K", "ky1K"), ("変", "ky1K"), ("←←o", "きょ1K")],
        ),
        (
            "mode_toggle_after_backspace_combines",
            &[("kyK", "kyK"), ("⌫", "ky"), ("変o", "きょ")],
        ),
        // Deleting the separator between two live runs evaluates the
        // joined keystrokes, matching fresh typing of the remainder
        (
            "delete_separator_evaluates_joined_run",
            &[("ty1y", "ty1y"), ("←⌫", "tっy@2"), ("⇥o", "tっよ")],
        ),
        (
            "delete_separator_fires_sokuon",
            &[("yt1t", "yt1t"), ("←⌫", "yっt@2"), ("⇥o", "yっと")],
        ),
        // A doubled consonant after a rule prefix keeps the prefix alive
        (
            "prefixed_double_consonant_keeps_prefix",
            &[("tyy", "tっy"), ("⌫", "tっ"), ("⌫", "t"), ("a", "た")],
        ),
        // Returning from conversion keeps the reading editable
        (
            "conversion_escape_then_continue_typing",
            &[("kyo", "きょ"), ("␣␛", "きょ"), ("u", "きょう")],
        ),
        // Cancelling a conversion restores the live romaji: the pending
        // `d` still combines afterwards
        (
            "space_escape_keeps_pending_live",
            &[
                ("keioud", "けいおうd"),
                ("␣␛", "けいおうd"),
                ("a", "けいおうだ"),
            ],
        ),
        (
            "tab_escape_keeps_pending_live",
            &[
                ("keioud", "けいおうd"),
                ("↹␛", "けいおうd"),
                ("a", "けいおうだ"),
            ],
        ),
        // No candidates (emoji query with no match): Space keeps composing
        // with the caret at the end, so the next key appends
        (
            "emoji_no_match_space_keeps_appending",
            &[(":qqqq", ":qqqq"), ("␣", ":qqqq"), ("a", ":qqqqa")],
        ),
    ];

    for (name, steps) in cases {
        run_scenario(name, steps);
    }
}

/// A consonant stranded behind settled text stays at its position — it is
/// part of the reading, not the aux/live tail, so it never teleports to
/// the end of the display.
#[test]
fn test_stranded_consonant_stays_in_place() {
    let mut engine = InputMethodEngine::new();

    send(&mut engine, "y1k");
    assert_eq!(preedit_text(&engine), "y1k");
    // Only the k (adjacent to the caret) is being typed; y is stranded
    assert_eq!(engine.input_buf.reading(), "y1");
    assert_eq!(engine.input_buf.pending(), "k");

    send(&mut engine, "a");
    assert_eq!(preedit_text(&engine), "y1か");
    assert_eq!(engine.input_buf.reading(), "y1か");
    assert_eq!(engine.input_buf.pending(), "");
}

/// Same scenario with live conversion on: the preedit is built from
/// live text + pending, which must not move the stranded y to the end.
#[test]
fn test_stranded_consonant_stays_in_place_live() {
    let mut engine = make_live_conversion_engine();

    send(&mut engine, "y1a");
    assert_eq!(preedit_text(&engine), "y1あ");
}
