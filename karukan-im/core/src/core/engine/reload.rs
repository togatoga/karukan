//! Config hot reload: an edited config.toml (or romaji.tsv) takes effect
//! without restarting the IME.
//!
//! The engine checks for itself, at the first key of a composition — the
//! Empty state — so a save shows on the next word typed, in whichever
//! frontend, and a composition never changes rules halfway through. There
//! is no watcher thread and nothing for a frontend to call: the check is a
//! `stat` of the two files, and only a changed one is read.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use tracing::{info, warn};

use crate::config::settings::{CONFIG_FILE, ROMAJI_TABLE};

use super::*;

/// What tells one version of a file from the next without reading it:
/// modification time and length. `None` while the file does not exist.
type FileStamp = Option<(SystemTime, u64)>;

fn stamp_of(path: &Path) -> FileStamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// The watched config directory and what was last read from it.
pub(super) struct ConfigWatch {
    dir: PathBuf,
    /// Stamps of config.toml and romaji.tsv as of the last check that read
    /// them — whether or not what it read was usable, so a broken file is
    /// parsed once per save, not once per keystroke.
    stamps: [FileStamp; 2],
    /// The settings in force: what the next reload is compared against.
    settings: Settings,
}

impl ConfigWatch {
    fn current_stamps(&self) -> [FileStamp; 2] {
        [CONFIG_FILE, ROMAJI_TABLE].map(|name| stamp_of(&self.dir.join(name)))
    }
}

impl InputMethodEngine {
    /// Follow the user's config directory from here on: `settings` is what
    /// this engine was built from, and a later edit to config.toml or
    /// romaji.tsv is applied at the next key that starts a composition.
    /// Frontends call this once; an engine that is never told to watch
    /// (tests, hand-made configs) never reloads.
    pub fn watch_config_file(&mut self, settings: &Settings) {
        if let Some(dir) = Settings::config_dir() {
            self.watch_config_dir(dir, settings.clone());
        }
    }

    pub(super) fn watch_config_dir(&mut self, dir: PathBuf, settings: Settings) {
        let mut watch = ConfigWatch {
            dir,
            stamps: [None; 2],
            settings,
        };
        watch.stamps = watch.current_stamps();
        self.config_watch = Some(watch);
    }

    /// Re-read the watched config if it changed on disk and apply it.
    /// Returns whether new settings were applied.
    ///
    /// A broken config.toml keeps the settings in force (and is logged); a
    /// deleted one means the defaults, as it does at startup.
    pub(super) fn reload_config_if_changed(&mut self) -> bool {
        let Some(watch) = self.config_watch.as_mut() else {
            return false;
        };
        let stamps = watch.current_stamps();
        if stamps == watch.stamps {
            return false;
        }
        watch.stamps = stamps;

        let settings = match Settings::load_from_dir(&watch.dir) {
            Ok(settings) => settings,
            Err(e) => {
                warn!(
                    "config reload: {:?} is invalid, keeping the current settings: {e:#}",
                    watch.dir.join(CONFIG_FILE)
                );
                return false;
            }
        };
        let old = std::mem::replace(&mut watch.settings, settings.clone());
        self.apply_settings(&old, &settings);
        info!("config reloaded");
        true
    }

    /// Swap `new` in for `old`: the engine config with everything derived
    /// from it, and what init loaded — models, system dictionary, learning
    /// cache — wherever the settings no longer ask for the same thing.
    ///
    /// Live conversion and the verbose aux line have runtime toggles
    /// (Ctrl+Shift+L / Ctrl+Shift+V); the configured value only overrides
    /// what the user toggled when it is the value that was edited.
    ///
    /// The conversion cache stays: its key — reading, lctx (persona
    /// included), slot, beam width — covers every setting that reaches the
    /// model, so entries a change invalidates simply stop being hit. Only
    /// new models empty it (`poll_loaded_models`).
    fn apply_settings(&mut self, old: &Settings, new: &Settings) {
        let mut config = EngineConfig::from_settings(new);
        if new.conversion.live_conversion != old.conversion.live_conversion {
            self.live.enabled = config.live_conversion;
        }
        if new.display.verbose == old.display.verbose {
            config.verbose = self.config.verbose;
        }
        self.install_config(config);
        self.reload_models(new);

        if new.conversion.dict_path != old.conversion.dict_path {
            self.dicts.system = None;
            self.init_system_dictionary(new.conversion.dict_path.as_deref());
        }
        // The cache takes its limits at construction, so new ones mean a
        // new cache: what was learned goes to disk and is read back under
        // them (or stays there, when learning was turned off).
        if new.learning != old.learning {
            self.save_learning();
            self.learning = None;
            self.init_learning_cache(&new.learning);
        }
    }
}
