//! Background model-loading state machine: failure handling and the
//! key-event-driven retry with backoff.

use std::sync::mpsc;
use std::time::{Duration, Instant};

use super::*;
use crate::config::settings::StrategyMode;
use crate::core::engine::init::{LoadFailure, LoadedConverters, ModelLoadSpec, ModelLoading};

/// A spec whose load fails locally (unknown variant): the loader thread
/// never touches the network, so tests stay offline-safe.
fn unknown_model_spec() -> ModelLoadSpec {
    ModelLoadSpec {
        strategy: StrategyMode::Main,
        model: Some("no-such-model-variant".to_string()),
        light_model: None,
        n_threads: 0,
    }
}

fn loading_with(result: Result<LoadedConverters, LoadFailure>) -> ModelLoading {
    let (tx, rx) = mpsc::channel();
    tx.send(result).unwrap();
    ModelLoading::Loading {
        rx,
        spec: unknown_model_spec(),
        attempts: 0,
    }
}

#[test]
fn transient_failure_schedules_retry() {
    let mut engine = InputMethodEngine::new();
    engine.model_loading = loading_with(Err(LoadFailure::Transient));

    engine.poll_loaded_models();

    match &engine.model_loading {
        ModelLoading::Failed {
            attempts, retry_at, ..
        } => {
            assert_eq!(*attempts, 1);
            assert!(*retry_at > Instant::now());
        }
        _ => panic!("expected Failed after a transient failure"),
    }
}

#[test]
fn permanent_failure_gives_up() {
    let mut engine = InputMethodEngine::new();
    engine.model_loading = loading_with(Err(LoadFailure::Permanent));

    engine.poll_loaded_models();

    assert!(matches!(engine.model_loading, ModelLoading::Idle));
}

#[test]
fn dead_loader_gives_up() {
    let mut engine = InputMethodEngine::new();
    let (tx, rx) = mpsc::channel::<Result<LoadedConverters, LoadFailure>>();
    drop(tx);
    engine.model_loading = ModelLoading::Loading {
        rx,
        spec: unknown_model_spec(),
        attempts: 0,
    };

    engine.poll_loaded_models();

    assert!(matches!(engine.model_loading, ModelLoading::Idle));
}

#[test]
fn retry_waits_for_backoff() {
    let mut engine = InputMethodEngine::new();
    engine.model_loading = ModelLoading::Failed {
        spec: unknown_model_spec(),
        attempts: 1,
        retry_at: Instant::now() + Duration::from_secs(3600),
    };

    engine.poll_loaded_models();

    assert!(matches!(
        engine.model_loading,
        ModelLoading::Failed { attempts: 1, .. }
    ));
}

#[test]
fn retry_fires_after_backoff() {
    let mut engine = InputMethodEngine::new();
    engine.model_loading = ModelLoading::Failed {
        spec: unknown_model_spec(),
        attempts: 1,
        retry_at: Instant::now(),
    };

    engine.poll_loaded_models();
    assert!(matches!(
        engine.model_loading,
        ModelLoading::Loading { attempts: 1, .. }
    ));

    // The unknown variant fails locally and permanently; poll until the
    // loader reports it and the engine gives up for the session.
    let deadline = Instant::now() + Duration::from_secs(10);
    while !matches!(engine.model_loading, ModelLoading::Idle) {
        assert!(Instant::now() < deadline, "loader thread did not finish");
        std::thread::sleep(Duration::from_millis(10));
        engine.poll_loaded_models();
    }
    assert!(engine.converters.kanji.is_none());
}

#[test]
fn model_name_reports_loading_only_while_loading() {
    let mut engine = InputMethodEngine::new();
    let (_tx, rx) = mpsc::channel::<Result<LoadedConverters, LoadFailure>>();
    engine.model_loading = ModelLoading::Loading {
        rx,
        spec: unknown_model_spec(),
        attempts: 0,
    };
    assert_eq!(engine.model_name(), "loading");

    engine.model_loading = ModelLoading::Failed {
        spec: unknown_model_spec(),
        attempts: 1,
        retry_at: Instant::now() + Duration::from_secs(3600),
    };
    assert_eq!(engine.model_name(), "unknown");
}
