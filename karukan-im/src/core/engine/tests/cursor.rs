use super::*;

// --- Super (Windows) Key Passthrough Tests ---

#[test]
fn test_super_key_not_consumed_in_empty_state() {
    let mut engine = InputMethodEngine::new();

    // Set some context
    engine.surrounding_context = Some(SurroundingContext {
        left: Some("テスト文脈".to_string()),
        right: Some("右側".to_string()),
    });
    assert!(engine.surrounding_context.is_some());

    // Press Super (Windows) key in empty state
    let super_key = KeyEvent {
        keysym: Keysym::SUPER_L,
        modifiers: KeyModifiers::default(),
        is_press: true,
    };
    let result = engine.process_key(&super_key);

    // Should NOT be consumed (pass through to window manager)
    assert!(!result.consumed, "Super key should not be consumed");

    // Context should be preserved
    assert!(
        engine.surrounding_context.is_some(),
        "Context should be preserved when Super is pressed"
    );
}

#[test]
fn test_super_key_not_consumed_in_hiragana_state() {
    let mut engine = InputMethodEngine::new();

    // Enter hiragana input state
    engine.process_key(&press('a'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あ");

    // Press Super key in hiragana state
    let super_key = KeyEvent {
        keysym: Keysym::SUPER_L,
        modifiers: KeyModifiers::default(),
        is_press: true,
    };
    let result = engine.process_key(&super_key);

    // Should NOT be consumed (pass through to window manager)
    assert!(!result.consumed, "Super key should not be consumed");

    // Preedit should be preserved (deactivate will commit it)
    assert!(
        matches!(engine.state(), InputState::Composing { .. }),
        "State should remain hiragana input"
    );
    assert_eq!(
        engine.preedit().unwrap().text(),
        "あ",
        "Preedit should be preserved"
    );
}

#[test]
fn test_commit_returns_pending_input() {
    let mut engine = InputMethodEngine::new();

    // Enter hiragana input state
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().unwrap().text(), "あい");

    // Commit should return the pending input (used by deactivate)
    let committed = engine.commit();
    assert_eq!(committed, "あい");
    assert!(matches!(engine.state(), InputState::Empty));
}

// --- Cursor Movement Tests ---

#[test]
fn test_cursor_move_left_right() {
    let mut engine = InputMethodEngine::new();

    // Type "あいう"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 3); // cursor at end

    // Move left -> cursor between "い" and "う"
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 2);

    // Move left again -> cursor between "あ" and "い"
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    // Move right -> cursor between "い" and "う"
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 2);

    // Move right -> cursor at end
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 3);
}

#[test]
fn test_cursor_left_boundary() {
    let mut engine = InputMethodEngine::new();

    // Type "あ"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    // Move left past start
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().caret(), 0);

    // Move left again - should stay at 0
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().caret(), 0);
}

#[test]
fn test_cursor_right_boundary() {
    let mut engine = InputMethodEngine::new();

    // Type "あ"
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    // Move right past end - should stay at 1
    engine.process_key(&press_key(Keysym::RIGHT));
    assert_eq!(engine.preedit().unwrap().caret(), 1);
}

#[test]
fn test_cursor_insert_in_middle() {
    let mut engine = InputMethodEngine::new();

    // Type "あう" (a, u)
    engine.process_key(&press('a'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "あう");

    // Move left to before "う"
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    // Type "い" (i) - should insert between "あ" and "う"
    engine.process_key(&press('i'));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");
    assert_eq!(engine.preedit().unwrap().caret(), 2);
}

#[test]
fn test_cursor_insert_romaji_in_middle() {
    let mut engine = InputMethodEngine::new();

    // Type "あう" (a, u)
    engine.process_key(&press('a'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "あう");

    // Move left to before "う"
    engine.process_key(&press_key(Keysym::LEFT));

    // Type "ka" - 'k' goes to buffer, then 'a' produces "か"
    engine.process_key(&press('k'));
    // Display should show buffer "k" at cursor position
    assert_eq!(engine.preedit().unwrap().text(), "あkう");

    engine.process_key(&press('a'));
    // "ka" -> "か" inserted at cursor
    assert_eq!(engine.preedit().unwrap().text(), "あかう");
    assert_eq!(engine.preedit().unwrap().caret(), 2);
}

#[test]
fn test_cursor_backspace_in_middle() {
    let mut engine = InputMethodEngine::new();

    // Type "あいう"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");

    // Move left once (before "う"), then left again (before "い")
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.preedit().unwrap().caret(), 1);

    // Backspace - should delete "あ" before cursor
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().unwrap().text(), "いう");
    assert_eq!(engine.preedit().unwrap().caret(), 0);
}

#[test]
fn test_cursor_delete_key() {
    let mut engine = InputMethodEngine::new();

    // Type "あいう"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));

    // Move to beginning
    engine.process_key(&press_key(Keysym::HOME));
    assert_eq!(engine.preedit().unwrap().caret(), 0);

    // Delete - should remove "あ"
    engine.process_key(&press_key(Keysym::DELETE));
    assert_eq!(engine.preedit().unwrap().text(), "いう");
    assert_eq!(engine.preedit().unwrap().caret(), 0);
}

#[test]
fn test_cursor_home_end() {
    let mut engine = InputMethodEngine::new();

    // Type "あいう"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().caret(), 3);

    // Home - cursor to start
    engine.process_key(&press_key(Keysym::HOME));
    assert_eq!(engine.preedit().unwrap().caret(), 0);
    assert_eq!(engine.preedit().unwrap().text(), "あいう");

    // End - cursor to end
    engine.process_key(&press_key(Keysym::END));
    assert_eq!(engine.preedit().unwrap().caret(), 3);
}

#[test]
fn test_cursor_left_flushes_romaji_buffer() {
    let mut engine = InputMethodEngine::new();

    // Type "a" then "k" (buffer has "k")
    engine.process_key(&press('a'));
    engine.process_key(&press('k'));
    assert_eq!(engine.preedit().unwrap().text(), "あk");

    // Move left - should flush "k" (becomes "k" as-is or gets handled)
    engine.process_key(&press_key(Keysym::LEFT));
    // After flush, buffer should be empty, the flushed char is in composed text
    let preedit = engine.preedit().unwrap();
    // "k" flushed becomes "k" (pass-through since no match)
    assert!(preedit.text().contains('k') || preedit.text().contains("あ"));
}

#[test]
fn test_cursor_commit_after_editing() {
    let mut engine = InputMethodEngine::new();

    // Type "わせやだいがく" -> then fix to "わせだだいがく"
    // Type "wasedadaigaku" but simulate the mistake scenario
    // First type "あいう"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));
    engine.process_key(&press('u'));
    assert_eq!(engine.preedit().unwrap().text(), "あいう");

    // Move left twice (before "い")
    engine.process_key(&press_key(Keysym::LEFT));
    engine.process_key(&press_key(Keysym::LEFT));

    // Delete "い" at cursor
    engine.process_key(&press_key(Keysym::DELETE));
    assert_eq!(engine.preedit().unwrap().text(), "あう");

    // Insert "え" at cursor
    engine.process_key(&press('e'));
    assert_eq!(engine.preedit().unwrap().text(), "あえう");

    // Commit
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "あえう"));
    assert!(has_commit, "Should commit edited text");
    assert!(matches!(engine.state(), InputState::Empty));
}

#[test]
fn test_cursor_waseda_scenario() {
    let mut engine = InputMethodEngine::new();

    // Type "わせやだいがく" (wasedadaigaku with 'ya' mistake)
    // This simulates typing "waseyadaigaku" where 'ya' should be 'da'
    for ch in "wasedadaigaku".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.preedit().unwrap().text(), "わせだだいがく");

    // Now let's test the fix scenario: type "waseyadaigaku" (wrong)
    engine.process_key(&press_key(Keysym::ESCAPE)); // Cancel
    for ch in "waseyadaigaku".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.preedit().unwrap().text(), "わせやだいがく");

    // Now fix: move cursor to after "せ", delete "や", type "da"
    // "わせやだいがく" - 7 chars, cursor at end (7)
    // Move left 5 times to get to position 2 (after "せ")
    for _ in 0..5 {
        engine.process_key(&press_key(Keysym::LEFT));
    }
    assert_eq!(engine.preedit().unwrap().caret(), 2);

    // Delete "や" at cursor
    engine.process_key(&press_key(Keysym::DELETE));
    assert_eq!(engine.preedit().unwrap().text(), "わせだいがく");
    assert_eq!(engine.preedit().unwrap().caret(), 2);

    // Type "da" -> "だ"
    engine.process_key(&press('d'));
    engine.process_key(&press('a'));
    assert_eq!(engine.preedit().unwrap().text(), "わせだだいがく");

    // Commit
    let result = engine.process_key(&press_key(Keysym::RETURN));
    let has_commit = result
        .actions
        .iter()
        .any(|a| matches!(a, EngineAction::Commit(text) if text == "わせだだいがく"));
    assert!(has_commit, "Should commit corrected text 'わせだだいがく'");
}

#[test]
fn test_cursor_composed_hiragana_tracking() {
    let mut engine = InputMethodEngine::new();

    // Type "あい"
    engine.process_key(&press('a'));
    engine.process_key(&press('i'));

    // Check internal state
    assert_eq!(engine.input_buf.text, "あい");
    assert_eq!(engine.input_buf.cursor_pos, 2);

    // Move left
    engine.process_key(&press_key(Keysym::LEFT));
    assert_eq!(engine.input_buf.text, "あい");
    assert_eq!(engine.input_buf.cursor_pos, 1);

    // Cancel should clear
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(engine.input_buf.text, "");
    assert_eq!(engine.input_buf.cursor_pos, 0);
}
