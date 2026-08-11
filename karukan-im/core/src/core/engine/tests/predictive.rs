//! Predictive dictionary lookup: readings extending the typed prefix.

use super::*;

#[test]
fn predictive_dict_candidates_carry_full_reading() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[{"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]}]"#,
    ));

    let candidates = engine.lookup_dict_candidates("わせ");
    let waseda = candidates
        .iter()
        .find(|c| c.text == "早稲田")
        .expect("predictive candidate");
    assert_eq!(waseda.reading.as_deref(), Some("わせだ"));
}

#[test]
fn predictive_needs_two_typed_chars() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[{"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]}]"#,
    ));

    assert!(
        engine
            .lookup_dict_candidates("わ")
            .iter()
            .all(|c| c.text != "早稲田")
    );
}

#[test]
fn exact_matches_stay_ahead_of_predictive() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[
            {"reading":"わせ","candidates":[{"surface":"和瀬","score":5000.0}]},
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":100.0}]}
        ]"#,
    ));

    let candidates = engine.lookup_dict_candidates("わせ");
    let texts: Vec<&str> = candidates.iter().map(|c| c.text.as_str()).collect();
    assert_eq!(texts, ["和瀬", "早稲田"]);
    assert_eq!(candidates[0].reading.as_deref(), Some("わせ"));
    assert_eq!(candidates[1].reading.as_deref(), Some("わせだ"));
}

/// The user's scenario: typing わせ shows every completion, but the
/// moment `d` is typed the pending narrows prediction to だ/で/ど…
/// readings — ワセリン disappears, 早稲田 stays.
#[test]
fn pending_romaji_narrows_predictive_candidates() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]},
            {"reading":"わせりん","candidates":[{"surface":"ワセリン","score":100.0}]}
        ]"#,
    ));

    for ch in "wase".chars() {
        engine.process_key(&press(ch));
    }
    let texts: Vec<String> = engine
        .lookup_dict_candidates(&engine.input_buf.reading())
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert!(texts.contains(&"早稲田".to_string()));
    assert!(texts.contains(&"ワセリン".to_string()));

    engine.process_key(&press('d'));
    assert_eq!(engine.input_buf.pending(), "d");
    let texts: Vec<String> = engine
        .lookup_dict_candidates(&engine.input_buf.reading())
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert!(texts.contains(&"早稲田".to_string()));
    assert!(!texts.contains(&"ワセリン".to_string()));
}

/// A tail that cannot become kana (`yk`) suppresses prediction entirely.
#[test]
fn dead_romaji_tail_suppresses_prediction() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[{"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]}]"#,
    ));

    for ch in "waseyk".chars() {
        engine.process_key(&press(ch));
    }
    assert_eq!(engine.input_buf.pending(), "yk");
    let texts: Vec<String> = engine
        .lookup_dict_candidates(&engine.input_buf.reading())
        .into_iter()
        .map(|c| c.text)
        .collect();
    assert!(!texts.contains(&"早稲田".to_string()));
}

/// The conversion list gets the full ranked predictive set (paged); the
/// composing suggestion list stays capped at 3.
#[test]
fn conversion_list_gets_all_predictive_candidates() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]},
            {"reading":"わせだし","candidates":[{"surface":"早稲田市","score":2000.0}]},
            {"reading":"わせだだいがく","candidates":[{"surface":"早稲田大学","score":3000.0}]},
            {"reading":"わせだまえ","candidates":[{"surface":"早稲田前","score":4000.0}]},
            {"reading":"わせだえき","candidates":[{"surface":"早稲田駅","score":5000.0}]}
        ]"#,
    ));

    let conversion: Vec<String> = engine
        .build_conversion_candidates("わせ", "わせ", "", 1, LearningLookup::Use)
        .into_iter()
        .map(|c| c.text)
        .collect();
    for surface in ["早稲田", "早稲田市", "早稲田大学", "早稲田前", "早稲田駅"]
    {
        assert!(
            conversion.contains(&surface.to_string()),
            "missing {surface}"
        );
    }

    let suggestions = engine.lookup_dict_candidates("わせ");
    assert!(suggestions.len() <= 3);
}

/// The narrowed suggestion stays selectable: わせd + Space rebuilds the
/// conversion list with the same narrowing, so 早稲田 is in the
/// selectable candidates (and ワセリン is not).
#[test]
fn conversion_with_pending_keeps_narrowed_candidates() {
    let mut engine = InputMethodEngine::new();
    engine.dicts.system = Some(dict_from_json(
        r#"[
            {"reading":"わせだ","candidates":[{"surface":"早稲田","score":1000.0}]},
            {"reading":"わせりん","candidates":[{"surface":"ワセリン","score":100.0}]}
        ]"#,
    ));

    for ch in "wased".chars() {
        engine.process_key(&press(ch));
    }
    engine.process_key(&press_key(Keysym::SPACE));

    let candidates = engine.candidates().expect("conversion candidates");
    let texts: Vec<&str> = candidates
        .candidates()
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(texts.contains(&"早稲田"), "早稲田 selectable: {texts:?}");
    assert!(!texts.contains(&"ワセリン"), "ワセリン excluded: {texts:?}");

    let waseda = candidates
        .candidates()
        .iter()
        .find(|c| c.text == "早稲田")
        .unwrap();
    assert_eq!(waseda.reading.as_deref(), Some("わせだ"));
}
