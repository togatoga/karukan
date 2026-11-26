//! ばーじょん converts to the running build, and the version stays out of
//! the learning cache: recorded once, it would outlive the next upgrade.

use super::*;

/// Engine in Composing state on `reading`, no kanji model.
fn version_engine(reading: &str) -> InputMethodEngine {
    let mut engine = InputMethodEngine::with_config(EngineConfig::default());
    engine.input_buf.insert(reading);
    engine.state = InputState::Composing {
        preedit: Preedit::new(),
    };
    engine.converters.kanji = None;
    engine
}

fn version_text() -> String {
    format!("Karukan-{}", karukan_engine::VERSION)
}

#[test]
fn version_reading_surfaces_the_build() {
    for reading in ["ばーじょん", "ゔぁーじょん"] {
        let mut engine = version_engine(reading);
        let candidates =
            engine.build_conversion_candidates(reading, reading, "", 9, LearningLookup::Use);
        let version = candidates
            .iter()
            .find(|c| c.source == CandidateSource::Version)
            .unwrap_or_else(|| panic!("no version candidate for {reading}: {candidates:?}"));
        assert_eq!(version.text, version_text());
        assert_eq!(version.description.as_deref(), Some("バージョン"));
    }
}

#[test]
fn other_readings_carry_no_version() {
    let mut engine = version_engine("ばーじょ");
    let candidates =
        engine.build_conversion_candidates("ばーじょ", "ばーじょ", "", 9, LearningLookup::Use);
    assert!(
        !candidates
            .iter()
            .any(|c| c.source == CandidateSource::Version),
        "version leaked for a prefix: {candidates:?}"
    );
}

#[test]
fn version_heads_the_rewriter_view() {
    let engine = version_engine("ばーじょん");
    let view = engine.lookup_rewriter_variants("ばーじょん");
    assert_eq!(view.first().map(|c| c.text.clone()), Some(version_text()));
}

#[test]
fn committing_the_version_records_no_learning() {
    let mut engine = version_engine("ばーじょん");
    engine.learning = Some(LearningCache::new(LearningConfig::default()));
    let version = engine
        .lookup_rewriter_variants("ばーじょん")
        .into_iter()
        .find(|c| c.source == Some(CandidateSource::Version))
        .expect("version candidate");
    engine.state = InputState::Conversion {
        preedit: Preedit::new(),
        candidates: CandidateList::new(vec![version]),
        reading: "ばーじょん".to_string(),
        filter: None,
        left: Vec::new(),
        right: Vec::new(),
        shown: Default::default(),
    };

    let result = engine.process_key(&press_key(Keysym::RETURN));

    assert!(
        result
            .actions
            .iter()
            .any(|a| matches!(a, EngineAction::Commit(t) if *t == version_text())),
        "version did not commit: {result:?}"
    );
    assert!(
        engine
            .learning
            .as_ref()
            .unwrap()
            .lookup("ばーじょん", "")
            .is_empty(),
        "version commit leaked into the learning cache"
    );
}
