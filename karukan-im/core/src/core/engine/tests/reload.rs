//! Tests for config hot-reload (`reload_config_if_changed`).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::*;
use crate::config::settings::Settings;
use crate::core::engine::EngineConfig;

/// Unique temp config path per test.
fn temp_config(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("karukan-reload-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir.join("config.toml")
}

/// Write `content` with a strictly increasing mtime, so consecutive writes
/// within the filesystem's timestamp granularity still register as changes.
fn write_config(path: &Path, content: &str, bump: u64) {
    fs::write(path, content).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000 + bump))
        .unwrap();
}

/// Engine configured from the file at `path`, watching it for reloads.
fn engine_watching(path: &Path) -> InputMethodEngine {
    let settings = Settings::load_from(path).unwrap();
    let mut engine = InputMethodEngine::with_config(EngineConfig::from_settings(&settings));
    engine.watch_config_file_at(path.to_path_buf());
    engine
}

#[test]
fn test_reload_applies_changed_settings() {
    let path = temp_config("apply");
    write_config(&path, "[conversion]\npersona = \"A\"\n", 1);
    let mut engine = engine_watching(&path);
    assert!(!engine.reload_config_if_changed()); // unchanged → single stat

    write_config(
        &path,
        "[conversion]\npersona = \"B\"\ncomposing_chunk_len = 7\n",
        2,
    );
    assert!(engine.reload_config_if_changed());
    assert_eq!(engine.config.persona, "B");
    assert_eq!(engine.config.composing_chunk_len, 7);
    assert!(!engine.reload_config_if_changed()); // applied once per edit
}

#[test]
fn test_runtime_live_toggle_survives_unrelated_edit() {
    let path = temp_config("live");
    write_config(&path, "[conversion]\nlive_conversion = true\n", 1);
    let mut engine = engine_watching(&path);
    assert!(engine.live.enabled);

    // User toggles live conversion off at runtime (Ctrl+Shift+L); an edit
    // that does not touch live_conversion must not stomp the toggle.
    engine.live.enabled = false;
    write_config(
        &path,
        "[conversion]\nlive_conversion = true\npersona = \"B\"\n",
        2,
    );
    assert!(engine.reload_config_if_changed());
    assert!(!engine.live.enabled);

    // A change to the configured value itself does apply.
    write_config(&path, "[conversion]\nlive_conversion = false\n", 3);
    assert!(engine.reload_config_if_changed());
    assert!(!engine.live.enabled);
    write_config(&path, "[conversion]\nlive_conversion = true\n", 4);
    assert!(engine.reload_config_if_changed());
    assert!(engine.live.enabled);
}

#[test]
fn test_broken_config_keeps_current_settings() {
    let path = temp_config("broken");
    write_config(&path, "[conversion]\npersona = \"A\"\n", 1);
    let mut engine = engine_watching(&path);

    write_config(&path, "not toml [", 2);
    assert!(!engine.reload_config_if_changed());
    assert_eq!(engine.config.persona, "A");
    // The broken file is parsed once per edit, not once per check.
    assert!(!engine.reload_config_if_changed());

    write_config(&path, "[conversion]\npersona = \"C\"\n", 3);
    assert!(engine.reload_config_if_changed());
    assert_eq!(engine.config.persona, "C");
}

#[test]
fn test_unwatched_engine_never_reloads() {
    let mut engine = InputMethodEngine::new();
    assert!(!engine.reload_config_if_changed());
}
