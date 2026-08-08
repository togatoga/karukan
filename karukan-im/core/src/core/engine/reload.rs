//! Config hot-reload: re-read config.toml and apply the tunable subset.
//!
//! No file-watching thread — in line with fcitx5 (explicit DBus reload),
//! mozc (IPC push), and azooKey (per-request reads), none of which watch
//! files. Frontends call [`InputMethodEngine::reload_config_if_changed`] at
//! cheap explicit moments instead: the fcitx5 addon's `activate()` /
//! `reloadConfig()` and the macOS server's `reload_config` RPC (fired on
//! focus). An mtime guard makes the no-change case a single stat.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tracing::{info, warn};

use super::*;

/// The watched config file plus its state at the last (re)load.
pub(super) struct ConfigWatch {
    path: PathBuf,
    /// mtime at the last (re)load; `None` while the file does not exist.
    mtime: Option<SystemTime>,
    /// Settings as of the last (re)load, for restart-required diffs.
    settings: Settings,
}

fn mtime_of(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

/// Settings that are wired up at engine init (models, dictionaries, thread
/// pool, learning cache) and therefore not hot-swapped: a change is reported
/// as restart-required instead.
fn restart_required_diff(old: &Settings, new: &Settings) -> Vec<&'static str> {
    let (o, n) = (&old.conversion, &new.conversion);
    let mut out = Vec::new();
    if o.model != n.model {
        out.push("model");
    }
    if o.light_model != n.light_model {
        out.push("light_model");
    }
    if o.n_threads != n.n_threads {
        out.push("n_threads");
    }
    if o.dict_path != n.dict_path {
        out.push("dict_path");
    }
    if old.learning != new.learning {
        out.push("[learning]");
    }
    out
}

impl InputMethodEngine {
    /// Start watching the default config.toml for
    /// [`Self::reload_config_if_changed`]. Called once by the frontends;
    /// without it reloads are disabled (tests, custom setups).
    pub fn watch_config_file(&mut self) {
        if let Some(path) = Settings::config_file() {
            self.watch_config_file_at(path);
        }
    }

    pub(super) fn watch_config_file_at(&mut self, path: PathBuf) {
        self.config_watch = Some(ConfigWatch {
            mtime: mtime_of(&path),
            settings: Settings::load_from(&path).unwrap_or_default(),
            path,
        });
    }

    /// Re-read the watched config file if it changed on disk and apply the
    /// hot-reloadable settings — everything in [`EngineConfig`] (persona,
    /// chunking, beam/strategy, context lengths, …). Returns true when a
    /// reload was applied.
    ///
    /// The conversion cache needs no clearing: its key (reading, lctx,
    /// strategy) already covers every reloadable input to the model, so
    /// changed settings simply stop hitting the old entries.
    pub fn reload_config_if_changed(&mut self) -> bool {
        let Some(watch) = self.config_watch.as_mut() else {
            return false;
        };
        let mtime = mtime_of(&watch.path);
        if mtime == watch.mtime {
            return false;
        }
        // Remember the mtime before parsing: a broken file is parsed once
        // per edit, not once per focus change.
        watch.mtime = mtime;
        // A deleted file falls back to the defaults; a broken one keeps the
        // current settings.
        let loaded = if watch.path.exists() {
            Settings::load_from(&watch.path)
        } else {
            Ok(Settings::default())
        };
        let settings = match loaded {
            Ok(settings) => settings,
            Err(e) => {
                warn!(
                    "config reload: {:?} is invalid, keeping current settings: {:#}",
                    watch.path, e
                );
                return false;
            }
        };
        let restart = restart_required_diff(&watch.settings, &settings);
        let path = watch.path.clone();
        self.apply_reloaded(&settings);
        self.config_watch.as_mut().unwrap().settings = settings;
        for field in restart {
            info!("config reload: change to `{field}` requires a restart");
        }
        info!("config reloaded from {path:?}");
        true
    }

    /// Swap in the hot-reloadable subset: everything in [`EngineConfig`].
    /// The runtime live-conversion toggle (Ctrl+Shift+L) survives unrelated
    /// edits — the startup flag is pushed to `live.enabled` only when its
    /// configured value actually changed.
    fn apply_reloaded(&mut self, settings: &Settings) {
        let new = EngineConfig::from_settings(settings);
        if new.live_conversion != self.config.live_conversion {
            self.live.enabled = new.live_conversion;
        }
        self.config = new;
    }
}
