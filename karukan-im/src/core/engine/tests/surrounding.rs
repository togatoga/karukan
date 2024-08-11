use super::*;

// --- Surrounding Text Tests ---

#[test]
fn test_surrounding_text_sets_context() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 50;

    engine.set_surrounding_context("エディタの文章", "");
    assert_eq!(
        engine.surrounding_context.as_ref().unwrap().left.as_deref(),
        Some("エディタの文章")
    );
}

#[test]
fn test_surrounding_text_overwrites_context() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 50;

    // First, set some internal context (without surrounding text)
    engine.surrounding_context = Some(SurroundingContext {
        left: Some("古い内部文脈".to_string()),
        right: None,
    });

    // Now set surrounding text - should completely overwrite
    engine.set_surrounding_context("エディタからの新しい文脈", "");

    let left = engine
        .surrounding_context
        .as_ref()
        .unwrap()
        .left
        .as_deref()
        .unwrap();
    assert_eq!(left, "エディタからの新しい文脈");
    assert!(!left.contains("古い"));
}

#[test]
fn test_surrounding_text_multiple_updates() {
    let mut engine = InputMethodEngine::new();

    // Simulate multiple key events with surrounding text updates
    engine.set_surrounding_context("最初の文脈", "");
    assert_eq!(
        engine.surrounding_context.as_ref().unwrap().left.as_deref(),
        Some("最初の文脈")
    );

    // User types, editor updates surrounding text
    engine.set_surrounding_context("最初の文脈あ", "");
    assert_eq!(
        engine.surrounding_context.as_ref().unwrap().left.as_deref(),
        Some("最初の文脈あ")
    );

    // User commits, editor updates again
    engine.set_surrounding_context("最初の文脈あい", "");
    let left = engine
        .surrounding_context
        .as_ref()
        .unwrap()
        .left
        .as_deref()
        .unwrap();
    assert_eq!(left, "最初の文脈あい");

    // No garbage from internal tracking
    assert!(!left.contains("古い"));
}

#[test]
fn test_surrounding_text_respects_max_length() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 10;

    // Use a string longer than max_api_context_len
    let long_text = "あ".repeat(20);
    engine.set_surrounding_context(&long_text, "");

    // Should be truncated to last 10 chars
    assert_eq!(
        engine
            .surrounding_context
            .as_ref()
            .unwrap()
            .left
            .as_ref()
            .unwrap()
            .chars()
            .count(),
        10
    );
}

#[test]
fn test_reset_clears_all_state() {
    let mut engine = InputMethodEngine::new();

    // Set up various state
    engine.surrounding_context = Some(SurroundingContext {
        left: Some("文脈テキスト".to_string()),
        right: Some("右側テキスト".to_string()),
    });

    // Type something to change state
    engine.process_key(&press('a'));

    // Reset
    engine.reset();

    // State should be cleared, but surrounding_context is intentionally preserved
    // (it is set once at activate time and persists through the session)
    assert!(engine.surrounding_context.is_some());
    assert!(matches!(engine.state(), InputState::Empty));
}

// --- Candidate Merge Tests ---

#[test]
fn test_merge_candidates_dedup_no_duplicates() {
    let primary = vec!["今日".to_string(), "京".to_string()];
    let secondary = vec!["恭".to_string(), "強".to_string()];

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 5);

    assert_eq!(result, vec!["今日", "京", "恭", "強"]);
}

#[test]
fn test_merge_candidates_dedup_with_duplicates() {
    let primary = vec!["今日".to_string(), "京".to_string()];
    let secondary = vec!["京".to_string(), "強".to_string()]; // "京" is duplicate

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 5);

    // "京" should appear only once (from primary)
    assert_eq!(result, vec!["今日", "京", "強"]);
}

#[test]
fn test_merge_candidates_dedup_respects_max() {
    let primary = vec!["a".to_string(), "b".to_string(), "c".to_string()];
    let secondary = vec!["d".to_string(), "e".to_string()];

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 3);

    // Should only return 3 candidates
    assert_eq!(result.len(), 3);
    assert_eq!(result, vec!["a", "b", "c"]);
}

#[test]
fn test_merge_candidates_dedup_primary_priority() {
    // Same candidate appears in both, primary should win (appear first)
    let primary = vec!["漢字".to_string()];
    let secondary = vec!["漢字".to_string(), "感じ".to_string()];

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 5);

    // "漢字" should appear only once, "感じ" added from secondary
    assert_eq!(result, vec!["漢字", "感じ"]);
}

#[test]
fn test_merge_candidates_dedup_empty_primary() {
    let primary = vec![];
    let secondary = vec!["a".to_string(), "b".to_string()];

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 5);

    assert_eq!(result, vec!["a", "b"]);
}

#[test]
fn test_merge_candidates_dedup_empty_secondary() {
    let primary = vec!["a".to_string(), "b".to_string()];
    let secondary = vec![];

    let result = InputMethodEngine::merge_candidates_dedup(primary, secondary, 5);

    assert_eq!(result, vec!["a", "b"]);
}

// --- Surrounding Context (Left/Right) Tests ---

#[test]
fn test_set_surrounding_context_both() {
    let mut engine = InputMethodEngine::new();

    engine.set_surrounding_context("左側テキスト", "右側テキスト");

    let ctx = engine.surrounding_context.as_ref().unwrap();
    assert_eq!(ctx.left.as_deref(), Some("左側テキスト"));
    assert_eq!(ctx.right.as_deref(), Some("右側テキスト"));
}

#[test]
fn test_set_surrounding_context_left_only() {
    let mut engine = InputMethodEngine::new();

    engine.set_surrounding_context("左側のみ", "");

    let ctx = engine.surrounding_context.as_ref().unwrap();
    assert_eq!(ctx.left.as_deref(), Some("左側のみ"));
    assert!(ctx.right.is_none());
}

#[test]
fn test_set_surrounding_context_right_only() {
    let mut engine = InputMethodEngine::new();

    engine.set_surrounding_context("", "右側のみ");

    let ctx = engine.surrounding_context.as_ref().unwrap();
    assert!(ctx.left.is_none());
    assert_eq!(ctx.right.as_deref(), Some("右側のみ"));
}

#[test]
fn test_set_surrounding_context_truncation() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 5;

    // Use strings longer than max_api_context_len
    engine.set_surrounding_context("左側が長すぎるテキスト", "右側が長すぎるテキスト");

    let ctx = engine.surrounding_context.as_ref().unwrap();

    // Left: keep last 5 chars
    let left = ctx.left.as_ref().unwrap();
    assert_eq!(left.chars().count(), 5);
    assert!(left.contains("テキスト")); // last part

    // Right: keep first 5 chars
    let right = ctx.right.as_ref().unwrap();
    assert_eq!(right.chars().count(), 5);
    assert!(right.contains("右側が")); // first part
}

#[test]
fn test_display_context_lctx_rctx_format() {
    let mut engine = InputMethodEngine::new();
    engine.config.display_context_len = 10;

    // Both contexts
    engine.set_surrounding_context("左側", "右側");
    let display = engine.display_context();
    assert!(display.contains("lctx:"));
    assert!(display.contains("rctx:"));
    assert!(display.contains("左側"));
    assert!(display.contains("右側"));

    // Left only
    engine.set_surrounding_context("左側のみ", "");
    let display = engine.display_context();
    assert!(display.contains("lctx:"));
    assert!(!display.contains("rctx:"));

    // Right only
    engine.set_surrounding_context("", "右側のみ");
    let display = engine.display_context();
    assert!(!display.contains("lctx:"));
    assert!(display.contains("rctx:"));

    // Empty (both empty → None)
    engine.set_surrounding_context("", "");
    assert!(engine.surrounding_context.is_none());
    let display = engine.display_context();
    assert!(display.is_empty());
}

#[test]
fn test_display_context_truncation() {
    let mut engine = InputMethodEngine::new();
    engine.config.max_api_context_len = 50;
    engine.config.display_context_len = 5;

    engine.set_surrounding_context("とても長い左側テキスト", "とても長い右側テキスト");

    let ctx = engine.display_context();
    // Left context: truncated with "..." prefix, right context: truncated with "..." suffix
    assert!(ctx.contains("lctx: ..."));
    assert!(ctx.contains("rctx: "));
    assert!(ctx.ends_with("..."));
}
