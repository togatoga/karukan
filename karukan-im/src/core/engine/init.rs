//! Engine initialization (model loading, dictionary setup)

use anyhow::{Context, Result};
use tracing::debug;

use crate::config::settings::StrategyMode;

use super::*;

/// Create a KanaKanjiConverter from a variant id, optionally setting thread count.
fn create_converter(variant_id: &str, n_threads: u32) -> Result<KanaKanjiConverter> {
    let backend = karukan_engine::Backend::from_variant_id(variant_id)?;
    let mut converter = KanaKanjiConverter::new(backend)?;
    if n_threads > 0 {
        converter.set_n_threads(n_threads);
    }
    Ok(converter)
}

/// Format the n_threads value for debug logging.
fn threads_label(n_threads: u32) -> String {
    if n_threads > 0 {
        n_threads.to_string()
    } else {
        "default".to_string()
    }
}

impl InputMethodEngine {
    /// Full engine initialization from user settings: system dictionary,
    /// user dictionaries, learning cache, and conversion models according
    /// to the configured strategy.
    ///
    /// Shared by the fcitx5 FFI (`karukan_engine_init`) and the stdio
    /// JSON-RPC server (`init` method). In `Adaptive` mode a light-model
    /// failure is non-fatal (beam search is simply unavailable).
    pub fn init_from_settings(&mut self, settings: &Settings) -> Result<()> {
        let strategy = settings.conversion.strategy;
        tracing::info!(
            "Karukan init: model={:?}, light_model={:?}, strategy={:?}",
            settings.conversion.model,
            settings.conversion.light_model,
            strategy,
        );

        self.init_system_dictionary(settings.conversion.dict_path.as_deref());
        self.init_user_dictionaries();
        self.init_learning_cache(settings.learning.enabled, settings.learning.max_entries);

        let n_threads = settings.conversion.n_threads;

        match strategy {
            StrategyMode::Light => {
                // Light mode: load light_model into the main (kanji) slot only
                let light_variant = resolve_variant_id(settings.conversion.light_model.as_deref())
                    .context("invalid light_model settings")?;
                self.init_kanji_converter_with_model(&light_variant, n_threads)
                    .context("failed to initialize light model")?;
                tracing::info!("Light model loaded into main slot: {}", self.model_name());
            }
            StrategyMode::Main => {
                // Main mode: load main model only, no light model
                let main_variant = resolve_variant_id(settings.conversion.model.as_deref())
                    .context("invalid model settings")?;
                self.init_kanji_converter_with_model(&main_variant, n_threads)
                    .context("failed to initialize main model")?;
                tracing::info!("Main model loaded: {}", self.model_name());
            }
            StrategyMode::Adaptive => {
                // Adaptive mode: load both main and light models
                let main_variant = resolve_variant_id(settings.conversion.model.as_deref())
                    .context("invalid model settings")?;
                let light_model = settings.conversion.light_model.clone();
                self.init_kanji_converter_with_model(&main_variant, n_threads)
                    .context("failed to initialize default model")?;
                tracing::info!("Default model loaded: {}", self.model_name());

                // Initialize light model for beam search (non-fatal on failure)
                let light_variant = match resolve_variant_id(light_model.as_deref()) {
                    Ok(id) => id,
                    Err(e) => {
                        tracing::warn!("Invalid light_model settings, using default: {}", e);
                        karukan_engine::kanji::registry().default_model.clone()
                    }
                };
                if let Err(e) = self.init_light_kanji_converter(&light_variant, n_threads) {
                    tracing::warn!(
                        "Failed to initialize beam model (light_model={:?}): {}",
                        light_model,
                        e
                    );
                } else {
                    tracing::info!("Beam model loaded");
                }
            }
        }

        tracing::info!("Karukan init complete: {}", self.model_name());
        Ok(())
    }

    /// Initialize the kanji converter (call this early to avoid latency)
    /// Uses the default model from the registry.
    pub fn init_kanji_converter(&mut self) -> Result<()> {
        let default_id = karukan_engine::kanji::registry().default_model.clone();
        self.init_kanji_converter_with_model(&default_id, 0)
    }

    /// Initialize the kanji converter with a specific variant id
    pub fn init_kanji_converter_with_model(
        &mut self,
        variant_id: &str,
        n_threads: u32,
    ) -> Result<()> {
        if self.converters.kanji.is_none() {
            debug!("Initializing kanji converter with variant: {}", variant_id);
            let converter = create_converter(variant_id, n_threads)?;
            debug!(
                "Kanji converter initialized: {} (n_threads={})",
                converter.model_display_name(),
                threads_label(n_threads)
            );
            self.converters.kanji = Some(converter);
        }
        Ok(())
    }

    /// Initialize the light model for beam search (generates multiple candidates on Space conversion)
    pub fn init_light_kanji_converter(&mut self, variant_id: &str, n_threads: u32) -> Result<()> {
        if self.converters.light_kanji.is_none() {
            debug!(
                "Initializing light kanji converter with variant: {}",
                variant_id
            );
            let converter = create_converter(variant_id, n_threads)?;
            debug!(
                "Light kanji converter initialized: {} (n_threads={})",
                converter.model_display_name(),
                threads_label(n_threads)
            );
            self.converters.light_kanji = Some(converter);
        }
        Ok(())
    }

    /// Initialize the system dictionary for candidate lookup
    ///
    /// Uses `dict_path` from settings if specified, otherwise defaults to `data_dir/dict.bin`.
    /// If the file doesn't exist, the engine continues without a dictionary.
    pub fn init_system_dictionary(&mut self, dict_path: Option<&str>) {
        if self.dicts.system.is_some() {
            return;
        }

        let path = if let Some(p) = dict_path {
            std::path::PathBuf::from(p)
        } else if let Some(data_dir) = Settings::data_dir() {
            data_dir.join("dict.bin")
        } else {
            debug!("Could not determine data directory for system dictionary");
            return;
        };

        if !path.exists() {
            debug!("System dictionary not found at {:?}, skipping", path);
            return;
        }

        match Dictionary::load(&path) {
            Ok(dict) => {
                debug!("System dictionary loaded from {:?}", path);
                self.dicts.system = Some(dict);
            }
            Err(e) => {
                debug!("Failed to load system dictionary from {:?}: {}", path, e);
            }
        }
    }

    /// Initialize the learning cache from disk.
    ///
    /// Loads `~/.local/share/karukan-im/learning.tsv` if it exists.
    /// If the file doesn't exist, creates an empty in-memory cache.
    pub fn init_learning_cache(&mut self, enabled: bool, max_entries: usize) {
        if !enabled || self.learning.is_some() {
            return;
        }

        let Some(path) = Settings::learning_file() else {
            debug!("Could not determine learning cache path");
            self.learning = Some(LearningCache::new(max_entries));
            return;
        };

        if path.exists() {
            match LearningCache::load(&path, max_entries) {
                Ok(cache) => {
                    debug!(
                        "Learning cache loaded from {:?} ({} entries)",
                        path,
                        cache.entry_count()
                    );
                    self.learning = Some(cache);
                }
                Err(e) => {
                    debug!("Failed to load learning cache from {:?}: {}", path, e);
                    self.learning = Some(LearningCache::new(max_entries));
                }
            }
        } else {
            debug!("Learning cache not found at {:?}, starting empty", path);
            self.learning = Some(LearningCache::new(max_entries));
        }
    }

    /// Initialize user dictionaries by scanning the user dictionary directory.
    ///
    /// All files in the directory are loaded with `Dictionary::load_auto()`
    /// (auto-detects KRKN binary or Mozc TSV). Files are loaded in sorted
    /// order; earlier files have higher priority after merging.
    ///
    /// Default directory: `~/.local/share/karukan-im/user_dicts/`
    pub fn init_user_dictionaries(&mut self) {
        if self.dicts.user.is_some() {
            return;
        }

        let Some(dir) = Settings::user_dict_dir() else {
            debug!("Could not determine user dictionary directory");
            return;
        };

        if !dir.exists() {
            debug!(
                "User dictionary directory {:?} does not exist, skipping",
                dir
            );
            return;
        }

        let Ok(entries) = std::fs::read_dir(&dir) else {
            debug!("Failed to read user dictionary directory {:?}", dir);
            return;
        };
        let mut paths: Vec<std::path::PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();

        if paths.is_empty() {
            debug!("No files in user dictionary directory {:?}", dir);
            return;
        }

        // Sort for deterministic load order (alphabetical)
        paths.sort();

        let mut dicts = Vec::new();
        for path in &paths {
            match Dictionary::load_auto(path) {
                Ok(dict) => {
                    debug!("User dictionary loaded from {:?}", path);
                    dicts.push(dict);
                }
                Err(e) => {
                    debug!("Failed to load user dictionary from {:?}: {}", path, e);
                }
            }
        }

        if dicts.is_empty() {
            return;
        }

        match Dictionary::merge(dicts) {
            Ok(Some(merged)) => {
                debug!(
                    "User dictionaries merged successfully ({} files from {:?})",
                    paths.len(),
                    dir
                );
                self.dicts.user = Some(merged);
            }
            Ok(None) => {}
            Err(e) => {
                debug!("Failed to merge user dictionaries: {}", e);
            }
        }
    }
}
