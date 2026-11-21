//! Config hot reload: an edited config.toml / romaji.tsv is picked up at
//! the next key that starts a composition.

use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

use tempfile::TempDir;

use super::*;
use crate::config::settings::{CONFIG_FILE, ROMAJI_TABLE};

/// Write `content` to `name` in `dir`, stamped `tick` seconds after a fixed
/// epoch so that consecutive writes differ in mtime whatever the
/// filesystem's timestamp granularity.
fn write(dir: &Path, name: &str, content: &str, tick: u64) {
    let path = dir.join(name);
    fs::write(&path, content).unwrap();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000 + tick))
        .unwrap();
}

/// An engine built from `config` in a fresh config directory and watching
/// it, the way a frontend sets one up.
fn engine_watching(config: &str) -> (InputMethodEngine, TempDir) {
    let dir = TempDir::new().unwrap();
    write(dir.path(), CONFIG_FILE, config, 0);
    let settings = Settings::load_from_dir(dir.path()).unwrap();
    let mut engine = InputMethodEngine::with_config(EngineConfig::from_settings(&settings));
    engine.watch_config_dir(dir.path().to_path_buf(), settings);
    (engine, dir)
}

/// The preedit after typing `keys`.
fn preedit_after(engine: &mut InputMethodEngine, keys: &str) -> String {
    for ch in keys.chars() {
        engine.process_key(&press(ch));
    }
    engine.preedit().map(|p| p.text().to_string()).unwrap()
}

/// No live conversion, so the preedit is exactly what the keys typed.
const KANA_ONLY: &str = "[conversion]\nlive_conversion = false\n";

#[test]
fn test_edit_applies_at_the_next_composition() {
    let (mut engine, dir) = engine_watching(KANA_ONLY);
    assert_eq!(preedit_after(&mut engine, "a."), "あ。");

    // Saved mid-composition: this composition keeps the rules it started
    // with, the next one gets the new ones.
    let edited = format!("{KANA_ONLY}persona = \"Ｒｕｓｔ \"\n[symbol]\npunctuation = \"，．\"\n");
    write(dir.path(), CONFIG_FILE, &edited, 1);
    assert_eq!(preedit_after(&mut engine, "."), "あ。。");
    engine.process_key(&KeyEvent::press(Keysym::RETURN));

    assert_eq!(preedit_after(&mut engine, "a."), "あ．");
    // Derived state went through the same door as at startup.
    assert_eq!(engine.config.persona, "Rust");
}

#[test]
fn test_unchanged_config_is_not_reloaded() {
    let (mut engine, dir) = engine_watching(KANA_ONLY);
    assert!(!engine.reload_config_if_changed());

    write(dir.path(), CONFIG_FILE, "[conversion]\nbeam_chars = 7\n", 1);
    assert!(engine.reload_config_if_changed());
    assert_eq!(engine.config.beam_chars, 7);
    assert!(!engine.reload_config_if_changed());
}

#[test]
fn test_runtime_toggles_survive_unrelated_edits() {
    let (mut engine, dir) =
        engine_watching("[conversion]\nlive_conversion = true\n[display]\nverbose = false\n");
    // Ctrl+Shift+L and Ctrl+Shift+V.
    engine.live.enabled = false;
    engine.config.verbose = true;

    write(
        dir.path(),
        CONFIG_FILE,
        "[conversion]\nlive_conversion = true\nbeam_chars = 7\n[display]\nverbose = false\n",
        1,
    );
    assert!(engine.reload_config_if_changed());
    assert!(!engine.live.enabled);
    assert!(engine.config.verbose);

    // Editing the setting itself does apply.
    write(
        dir.path(),
        CONFIG_FILE,
        "[conversion]\nlive_conversion = false\n[display]\nverbose = true\n",
        2,
    );
    assert!(engine.reload_config_if_changed());
    write(
        dir.path(),
        CONFIG_FILE,
        "[conversion]\nlive_conversion = true\n[display]\nverbose = false\n",
        3,
    );
    assert!(engine.reload_config_if_changed());
    assert!(engine.live.enabled);
    assert!(!engine.config.verbose);
}

#[test]
fn test_broken_config_keeps_current_settings() {
    let (mut engine, dir) = engine_watching("[conversion]\nbeam_chars = 5\n");

    write(dir.path(), CONFIG_FILE, "not toml [", 1);
    assert!(!engine.reload_config_if_changed());
    assert_eq!(engine.config.beam_chars, 5);

    write(dir.path(), CONFIG_FILE, "[conversion]\nbeam_chars = 7\n", 2);
    assert!(engine.reload_config_if_changed());
    assert_eq!(engine.config.beam_chars, 7);
}

#[test]
fn test_deleted_config_falls_back_to_defaults() {
    let (mut engine, dir) = engine_watching("[conversion]\nbeam_chars = 5\n");
    fs::remove_file(dir.path().join(CONFIG_FILE)).unwrap();
    assert!(engine.reload_config_if_changed());
    assert_eq!(
        engine.config.beam_chars,
        Settings::default().conversion.beam_chars
    );
}

#[test]
fn test_romaji_table_is_watched_too() {
    let (mut engine, dir) = engine_watching(KANA_ONLY);
    assert_eq!(preedit_after(&mut engine, "q"), "q");
    engine.process_key(&KeyEvent::press(Keysym::RETURN));

    write(dir.path(), ROMAJI_TABLE, "q\tくぁ\n", 1);
    assert_eq!(preedit_after(&mut engine, "q"), "くぁ");
    engine.process_key(&KeyEvent::press(Keysym::RETURN));

    // Removing the table brings the built-in rules back.
    fs::remove_file(dir.path().join(ROMAJI_TABLE)).unwrap();
    assert_eq!(preedit_after(&mut engine, "ka"), "か");
}

#[test]
fn test_model_change_loads_again_only_after_init() {
    // A local path that does not exist: the load fails at once, offline.
    let config = |model: &str| {
        format!(
            "[conversion]\nstrategy = \"main\"\nmodel = \"m\"\n[models]\nm = \"/nonexistent/{model}.gguf\"\n"
        )
    };

    // Never initialised: a reload leaves the models alone.
    let (mut engine, dir) = engine_watching(&config("a"));
    write(dir.path(), CONFIG_FILE, &config("b"), 1);
    assert!(engine.reload_config_if_changed());
    assert!(engine.model_loading.is_none());

    // Initialised: other models are asked for, the same ones are not.
    let (mut engine, dir) = engine_watching(&config("a"));
    let settings = Settings::load_from_dir(dir.path()).unwrap();
    engine.spawn_model_loading(&settings);
    engine.model_loading = None;

    write(
        dir.path(),
        CONFIG_FILE,
        &(config("a") + "beam_chars = 7\n"),
        1,
    );
    assert!(engine.reload_config_if_changed());
    assert!(engine.model_loading.is_none());

    write(dir.path(), CONFIG_FILE, &config("b"), 2);
    assert!(engine.reload_config_if_changed());
    assert!(engine.model_loading.is_some());
}

#[test]
fn test_dictionary_and_learning_follow_the_config() {
    let (mut engine, dir) = engine_watching(KANA_ONLY);
    engine.dicts.system = Some(std::sync::Arc::new(dict_from_json(
        r#"[{"reading":"あ","candidates":[{"surface":"亜","score":0.0}]}]"#,
    )));
    engine.learning = Some(LearningCache::new(LearningConfig::default()));

    // Another dictionary path is loaded in place of the current one (here:
    // none there, so none loaded); learning turned off drops the cache.
    let edited = format!(
        "{KANA_ONLY}dict_path = \"{}\"\n[learning]\nenabled = false\n",
        dir.path().join("missing.bin").display()
    );
    write(dir.path(), CONFIG_FILE, &edited, 1);
    assert!(engine.reload_config_if_changed());
    assert!(engine.dicts.system.is_none());
    assert!(engine.learning.is_none());
}

#[test]
fn test_unwatched_engine_never_reloads() {
    let mut engine = InputMethodEngine::new();
    assert!(!engine.reload_config_if_changed());
}
