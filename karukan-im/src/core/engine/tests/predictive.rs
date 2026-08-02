//! Predictive dictionary lookup: readings extending the typed prefix.

use std::io::Write;

use super::*;

fn dict_from_json(json: &str) -> Dictionary {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    tmp.write_all(json.as_bytes()).unwrap();
    tmp.flush().unwrap();
    Dictionary::build_from_json(tmp.path()).unwrap()
}

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
