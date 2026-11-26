//! F6–F10 transliteration: the composition in one script, selected in a
//! conversion that lists every transliteration.

use super::*;

fn committed(result: &EngineResult) -> Option<&str> {
    result.actions.iter().find_map(|a| match a {
        EngineAction::Commit(text) => Some(text.as_str()),
        _ => None,
    })
}

#[test]
fn each_key_selects_its_form() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kyouha");

    for (key, expected) in [
        (Keysym::F7, "キョウハ"),
        (Keysym::F8, "ｷｮｳﾊ"),
        (Keysym::F9, "ｋｙｏｕｈａ"),
        (Keysym::F10, "kyouha"),
        (Keysym::F6, "きょうは"),
    ] {
        assert!(engine.process_key(&press_key(key)).consumed);
        assert!(matches!(engine.state(), InputState::Conversion { .. }));
        assert_eq!(
            engine.preedit().map(|p| p.text()),
            Some(expected),
            "{key:?}"
        );
    }
}

#[test]
fn alphanumerics_are_the_keystrokes_as_typed() {
    let mut engine = InputMethodEngine::new();
    // し as `si` and ん as `nn`: a kana→romaji table would answer `shinbun`
    type_keys(&mut engine, "sinnbunn1k");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("しんぶん1k"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("sinnbunn1k"));
}

#[test]
fn repeated_alphanumeric_key_cycles_case() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kai");

    let mut seen = Vec::new();
    for _ in 0..4 {
        engine.process_key(&press_key(Keysym::F10));
        seen.push(engine.preedit().map(|p| p.text().to_owned()));
    }
    assert_eq!(
        seen,
        ["kai", "KAI", "Kai", "kai"].map(|s| Some(s.to_owned()))
    );

    engine.process_key(&press_key(Keysym::F9));
    assert_eq!(
        engine.preedit().map(|p| p.text()),
        Some("ｋａｉ"),
        "another key starts at its own first form"
    );
}

#[test]
fn escape_returns_to_the_composition() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kak");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("カk"));

    engine.process_key(&press_key(Keysym::ESCAPE));
    assert!(matches!(engine.state(), InputState::Composing { .. }));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("かk"));
}

#[test]
fn kana_forms_are_learned_alphanumerics_are_not() {
    let mut engine = engine_with_learned("ん", "ん");

    type_keys(&mut engine, "kai");
    engine.process_key(&press_key(Keysym::F10));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result), Some("kai"));
    assert!(
        engine
            .learning_cache()
            .unwrap()
            .lookup("かい", "", "")
            .is_empty()
    );

    type_keys(&mut engine, "kai");
    engine.process_key(&press_key(Keysym::F7));
    let result = engine.process_key(&press_key(Keysym::RETURN));
    assert_eq!(committed(&result), Some("カイ"));
    assert_eq!(learned(&engine, "かい"), ["カイ"]);
}

#[test]
fn empty_state_passes_function_keys_through() {
    let mut engine = InputMethodEngine::new();
    assert!(!engine.process_key(&press_key(Keysym::F7)).consumed);
}

#[test]
fn backspace_inside_a_rule_drops_its_keys() {
    // `kyo` cannot be divided between き and ょ, so what is left of the
    // rule stays kana: F10 shows no keys that were never typed.
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kyo");
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("き"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("き"));
}

#[test]
fn backspace_on_a_rule_head_keys_what_is_left() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kyoka");
    for _ in 0..3 {
        engine.process_key(&press_key(Keysym::LEFT));
    }
    // Caret between き and ょ: delete き, keep ょ.
    engine.process_key(&press_key(Keysym::RIGHT));
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("ょか"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("ょka"));
}

#[test]
fn untouched_rules_keep_their_keys() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "sikkari");
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("しっか"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("sikka"));
}

#[test]
fn typing_inside_a_rule_keeps_keys_in_place() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kyo");
    // Caret between き and ょ, then type あ there.
    engine.process_key(&press_key(Keysym::LEFT));
    type_keys(&mut engine, "a");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きあょ"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きaょ"));
}

// --- Review follow-ups ------------------------------------------------------

fn chain_engine() -> InputMethodEngine {
    let table = karukan_engine::ChainTable::parse_tsv("d\tｄ\nｄu\tきゃ\nきゃu\tきゃあ\n");
    InputMethodEngine::with_config(EngineConfig {
        romaji_table: Some(std::sync::Arc::new(table)),
        ..EngineConfig::default()
    })
}

#[test]
fn chain_table_text_stays_as_written() {
    // A chain table's live tail is display text, so the keys behind a
    // chain run are not known: F10 leaves it as written instead of mixing
    // in the display (`きゃu`).
    let mut engine = chain_engine();
    type_keys(&mut engine, "duu");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きゃあ"));
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きゃあ"));
}

#[test]
fn switching_forms_starts_at_the_typed_case() {
    // `k` reads the same in every form, so F6 then F10 must not be taken
    // for a repeated F10.
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "k");
    engine.process_key(&press_key(Keysym::F6));
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("k"));
    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("K"));
}

#[test]
fn shared_surface_takes_the_pressed_forms_source() {
    // Direct input reads the same as kana and alphanumeric: the row F10
    // selects must still be the non-learnable alphanumeric one.
    let mut engine = InputMethodEngine::new();
    engine.learning = Some(std::sync::Arc::new(std::sync::Mutex::new(
        LearningCache::new(LearningConfig::default()),
    )));
    engine.process_key(&press_shift('L'));
    type_keys(&mut engine, "inux");
    engine.process_key(&press_key(Keysym::F10));
    let InputState::Conversion { candidates, .. } = engine.state() else {
        panic!("not converting");
    };
    let selected = candidates.selected().expect("selected");
    assert_eq!(selected.text, "Linux");
    assert_eq!(selected.source, Some(CandidateSource::Transliteration));
    assert_eq!(selected.description.as_deref(), Some("[半]英数"));
    engine.process_key(&press_key(Keysym::RETURN));
    assert!(
        engine
            .learning_cache()
            .unwrap()
            .lookup("Linux", "", "")
            .is_empty()
    );
}

#[test]
fn cancelling_returns_to_the_kana_not_the_live_text() {
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "ai");
    set_live_text(&mut engine, "愛");
    engine.process_key(&press_key(Keysym::F7));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("アイ"));
    engine.process_key(&press_key(Keysym::ESCAPE));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("あい"));
}

#[test]
fn a_rule_back_together_gives_its_keys_again() {
    // Typing inside 「きょ」 breaks the rule; deleting what was typed puts
    // it back together, and its keys count again.
    let mut engine = InputMethodEngine::new();
    type_keys(&mut engine, "kyo");
    engine.process_key(&press_key(Keysym::LEFT));
    type_keys(&mut engine, "a");
    engine.process_key(&press_key(Keysym::BACKSPACE));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("きょ"));

    engine.process_key(&press_key(Keysym::F10));
    assert_eq!(engine.preedit().map(|p| p.text()), Some("kyo"));
}

#[test]
fn equal_rules_side_by_side_stay_apart() {
    // Two rules that wrote the same text from the same keys are still two
    // rules: か|か and きょ|きょ each give their own keys.
    for keys in ["kaka", "kyokyo"] {
        let mut engine = InputMethodEngine::new();
        type_keys(&mut engine, keys);
        engine.process_key(&press_key(Keysym::F10));
        assert_eq!(engine.preedit().map(|p| p.text()), Some(keys));
    }
}

#[test]
fn half_width_kana_take_every_kana_form() {
    // A table may write half-width katakana; F6/F7/F8 still convert it,
    // and the half-width kana symbols keep the width they were given.
    let table = karukan_engine::ChainTable::parse_tsv("ga\tｶﾞ\n.\t｡\n");
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        romaji_table: Some(std::sync::Arc::new(table)),
        ..EngineConfig::default()
    });
    type_keys(&mut engine, "ga.");
    assert_eq!(engine.preedit().map(|p| p.text()), Some("ｶﾞ｡"));
    for (key, expected) in [(Keysym::F6, "が｡"), (Keysym::F7, "ガ｡"), (Keysym::F8, "ｶﾞ｡")]
    {
        engine.process_key(&press_key(key));
        assert_eq!(
            engine.preedit().map(|p| p.text()),
            Some(expected),
            "{key:?}"
        );
    }
}
