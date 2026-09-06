//! Integration tests for the date rewriter: config.toml phrases surface as
//! `CandidateSource::Date` candidates and stay out of the learning cache.
//! The clock is real here, so assertions check shape (`2026/09/06`-like),
//! not exact dates.

use karukan_engine::{DateConfig, DatePhrase};

use super::*;
use crate::config::Settings;

/// Engine carrying `date` config, in Composing state, no kanji model.
fn date_engine(date: DateConfig, reading: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig {
        date,
        ..EngineConfig::default()
    });
    engine.input_buf.insert(reading);
    engine.state = InputState::Composing {
        preedit: Preedit::new(),
    };
    engine.converters.kanji = None;
    engine
}

fn slash_date(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() == 10
        && b[4] == b'/'
        && b[7] == b'/'
        && b.iter()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7) || c.is_ascii_digit())
}

#[test]
fn registered_phrase_surfaces_date_candidates() {
    let mut engine = date_engine(
        DateConfig {
            formats: Vec::new(),
            phrases: vec![DatePhrase {
                reading: "きょう".to_string(),
                offset_days: 0,
                formats: Some(vec!["{YEAR}/{MONTH}/{DATE}".to_string()]),
            }],
        },
        "きょう",
    );
    let candidates =
        engine.build_conversion_candidates("きょう", "きょう", "", 9, LearningLookup::Use);
    let date = candidates
        .iter()
        .find(|c| c.source == CandidateSource::Date && slash_date(&c.text))
        .unwrap_or_else(|| panic!("no date candidate: {:?}", candidates));
    assert_eq!(date.description.as_deref(), Some("日付"));
}

#[test]
fn default_settings_carry_the_shipped_phrases() {
    let settings = Settings::default();
    for reading in [
        "きょう",
        "きのう",
        "おととい",
        "あした",
        "あす",
        "あさって",
        "しあさって",
        "いま",
        "にちじ",
    ] {
        assert!(
            settings.date.phrases.iter().any(|p| p.reading == reading),
            "default.toml lacks date phrase {reading}"
        );
    }

    // The shipped きょう formats render through the whole engine path.
    let config = EngineConfig::from_settings(&settings);
    let mut engine = date_engine(config.date, "きょう");
    let candidates =
        engine.build_conversion_candidates("きょう", "きょう", "", 9, LearningLookup::Use);
    assert!(
        candidates
            .iter()
            .any(|c| c.source == CandidateSource::Date && slash_date(&c.text)),
        "no date candidate from default config: {candidates:?}"
    );
}

#[test]
fn unregistered_reading_gets_no_date_candidates() {
    let settings = Settings::default();
    let mut engine = date_engine(settings.date.clone(), "こんにちは");
    let candidates =
        engine.build_conversion_candidates("こんにちは", "こんにちは", "", 9, LearningLookup::Use);
    assert!(
        !candidates.iter().any(|c| c.source == CandidateSource::Date),
        "date candidate leaked for a plain word: {candidates:?}"
    );
}

#[test]
fn committing_a_date_candidate_records_no_learning() {
    // A recorded date would resurface later as a stale date at learning
    // priority, so Enter on a date candidate must record nothing.
    let settings = Settings::default();
    let mut engine = date_engine(settings.date.clone(), "きょう");
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    let date = engine
        .lookup_rewriter_variants("きょう")
        .into_iter()
        .find(|c| c.source == Some(CandidateSource::Date))
        .expect("date candidate");
    let surface = date.text.clone();
    engine.state = InputState::Conversion {
        preedit: Preedit::new(),
        candidates: CandidateList::new(vec![date]),
        reading: "きょう".to_string(),
        filter: None,
    };

    let result = engine.process_key(&press_key(Keysym::RETURN));

    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if *t == surface)),
        "date candidate did not commit: {result:?}"
    );
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("きょう")
            .is_empty(),
        "date commit leaked into the learning cache"
    );
}
