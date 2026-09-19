//! Engine initialization (model loading, dictionary setup)

use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::debug;

use crate::config::settings::StrategyMode;

use super::*;

/// Converters produced by the background model-loading thread, handed to the
/// engine through the `model_loading` channel.
pub(super) struct LoadedConverters {
    pub kanji: KanaKanjiConverter,
    pub light_kanji: Option<KanaKanjiConverter>,
}

/// What a model load needs from settings, kept so a failed attempt can be
/// retried without them.
#[derive(Clone)]
pub(super) struct ModelLoadSpec {
    pub(super) strategy: StrategyMode,
    pub(super) model: Option<String>,
    pub(super) light_model: Option<String>,
    pub(super) n_threads: u32,
}

/// Why a load attempt failed, as far as retrying is concerned.
pub(super) enum LoadFailure {
    /// Config or model-file errors a retry cannot fix (unknown variant,
    /// broken GGUF): give up for the session.
    Permanent,
    /// Download errors: the network may come back, so retry with backoff.
    Transient,
}

/// Background model-loading state, driven by `poll_loaded_models`.
pub(super) enum ModelLoading {
    /// Nothing in flight: models installed, never requested, or given up.
    Idle,
    /// A loader thread is running.
    Loading {
        rx: mpsc::Receiver<Result<LoadedConverters, LoadFailure>>,
        spec: ModelLoadSpec,
        attempts: u32,
    },
    /// The last attempt failed on the network; the first key event at or
    /// after `retry_at` retries, so a session that started offline picks
    /// the model up once connectivity returns.
    Failed {
        spec: ModelLoadSpec,
        attempts: u32,
        retry_at: Instant,
    },
}

/// Delay before retry number `attempts + 1`: 30s doubling up to 10 min.
/// Retries piggyback on key events, so this is a floor, not a schedule.
fn retry_backoff(attempts: u32) -> Duration {
    const BASE: Duration = Duration::from_secs(30);
    const MAX: Duration = Duration::from_secs(600);
    (BASE * (1 << (attempts.saturating_sub(1)).min(5))).min(MAX)
}

/// Transient iff the chain contains a `KanjiError::Download`.
fn classify_failure(e: &anyhow::Error) -> LoadFailure {
    let is_download = e.chain().any(|c| {
        matches!(
            c.downcast_ref::<karukan_engine::kanji::KanjiError>(),
            Some(karukan_engine::kanji::KanjiError::Download(_))
        )
    });
    if is_download {
        LoadFailure::Transient
    } else {
        LoadFailure::Permanent
    }
}

/// Create a KanaKanjiConverter from a variant id, optionally setting thread count.
fn create_converter(variant_id: &str, n_threads: u32) -> Result<KanaKanjiConverter> {
    let backend = karukan_engine::Backend::from_variant_id(variant_id)?;
    let mut converter = KanaKanjiConverter::new(backend)?;
    if n_threads > 0 {
        converter.set_n_threads(n_threads);
    }
    Ok(converter)
}

/// Load the conversion models for a strategy. Runs on the background
/// loading thread — it may block on a model download, which must stay off
/// the key-event thread. In `Adaptive` mode a light-model failure is
/// non-fatal (beam search is simply unavailable).
fn load_converters(spec: &ModelLoadSpec) -> Result<LoadedConverters> {
    let (model, light_model) = (spec.model.as_deref(), spec.light_model.as_deref());
    let n_threads = spec.n_threads;
    let (kanji, light_kanji) = match spec.strategy {
        StrategyMode::Light => {
            let variant =
                resolve_variant_id(light_model).context("invalid light_model settings")?;
            let converter = create_converter(&variant, n_threads)
                .context("failed to initialize light model")?;
            tracing::info!(
                "Light model loaded into main slot: {}",
                converter.model_display_name()
            );
            (converter, None)
        }
        StrategyMode::Main => {
            let variant = resolve_variant_id(model).context("invalid model settings")?;
            let converter =
                create_converter(&variant, n_threads).context("failed to initialize main model")?;
            tracing::info!("Main model loaded: {}", converter.model_display_name());
            (converter, None)
        }
        StrategyMode::Adaptive => {
            let variant = resolve_variant_id(model).context("invalid model settings")?;
            let main = create_converter(&variant, n_threads)
                .context("failed to initialize default model")?;
            tracing::info!("Default model loaded: {}", main.model_display_name());

            let light_variant = match resolve_variant_id(light_model) {
                Ok(id) => id,
                Err(e) => {
                    tracing::warn!("Invalid light_model settings, using default: {}", e);
                    karukan_engine::kanji::registry().default_model.clone()
                }
            };
            let light = match create_converter(&light_variant, n_threads) {
                Ok(converter) => {
                    tracing::info!("Beam model loaded");
                    Some(converter)
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to initialize beam model (light_model={:?}): {}",
                        light_model,
                        e
                    );
                    None
                }
            };
            (main, light)
        }
    };
    Ok(LoadedConverters { kanji, light_kanji })
}

impl InputMethodEngine {
    /// Full engine initialization from user settings: system dictionary,
    /// user dictionaries, learning cache, and conversion models according
    /// to the configured strategy.
    ///
    /// Shared by the fcitx5 FFI (`karukan_engine_init`) and the stdio
    /// JSON-RPC server (`init` method). Dictionaries and the learning cache
    /// load synchronously (local files, fast); the models load on a
    /// background thread because resolving them can touch the network.
    /// Until they arrive (or if loading fails) the engine runs with what it
    /// has: romaji conversion, dictionaries, learning cache, rewriters.
    pub fn init_from_settings(&mut self, settings: &Settings) -> Result<()> {
        tracing::info!(
            "Karukan init: model={:?}, light_model={:?}, strategy={:?}",
            settings.conversion.model,
            settings.conversion.light_model,
            settings.conversion.strategy,
        );

        self.init_system_dictionary(settings.conversion.dict_path.as_deref());
        self.init_user_dictionaries();
        self.init_learning_cache(
            settings.learning.enabled,
            LearningConfig {
                max_entries: settings.learning.max_entries,
                max_surface_chars: settings.learning.max_surface_chars,
            },
        );

        self.spawn_model_loading(settings);
        Ok(())
    }

    /// Load the conversion models on a background thread; never blocks.
    fn spawn_model_loading(&mut self, settings: &Settings) {
        if self.converters.kanji.is_some() || !matches!(self.model_loading, ModelLoading::Idle) {
            return;
        }
        self.spawn_load(
            ModelLoadSpec {
                strategy: settings.conversion.strategy,
                model: settings.conversion.model.clone(),
                light_model: settings.conversion.light_model.clone(),
                n_threads: settings.conversion.n_threads,
            },
            0,
        );
    }

    /// Start a loader thread for `spec`; `attempts` counts failures so far.
    fn spawn_load(&mut self, spec: ModelLoadSpec, attempts: u32) {
        let (tx, rx) = mpsc::channel();
        let thread_spec = spec.clone();
        let spawned = std::thread::Builder::new()
            .name("karukan-model-load".to_string())
            .spawn(move || {
                let result = load_converters(&thread_spec).map_err(|e| {
                    let failure = classify_failure(&e);
                    match failure {
                        LoadFailure::Transient => {
                            tracing::warn!("model loading failed, will retry: {e:#}")
                        }
                        LoadFailure::Permanent => {
                            tracing::error!("model loading failed, continuing without model: {e:#}")
                        }
                    }
                    failure
                });
                // A dead receiver just means the engine was dropped.
                drop(tx.send(result));
            });
        match spawned {
            Ok(_) => self.model_loading = ModelLoading::Loading { rx, spec, attempts },
            Err(e) => {
                tracing::error!("failed to spawn model loading thread: {e}");
                self.model_loading = ModelLoading::Idle;
            }
        }
    }

    /// Drive the background loading state; non-blocking, called at the top
    /// of `process_key`. Failures were already logged by the loader thread.
    pub(super) fn poll_loaded_models(&mut self) {
        match std::mem::replace(&mut self.model_loading, ModelLoading::Idle) {
            ModelLoading::Idle => {}
            ModelLoading::Loading { rx, spec, attempts } => match rx.try_recv() {
                Ok(Ok(loaded)) => {
                    self.converters.kanji = Some(loaded.kanji);
                    self.converters.light_kanji = loaded.light_kanji;
                    tracing::info!("Karukan init complete: {}", self.model_name());
                }
                Ok(Err(LoadFailure::Transient)) => {
                    let attempts = attempts + 1;
                    self.model_loading = ModelLoading::Failed {
                        spec,
                        attempts,
                        retry_at: Instant::now() + retry_backoff(attempts),
                    };
                }
                Ok(Err(LoadFailure::Permanent)) => {}
                Err(mpsc::TryRecvError::Empty) => {
                    self.model_loading = ModelLoading::Loading { rx, spec, attempts };
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    tracing::warn!("model loader thread died; continuing without model");
                }
            },
            ModelLoading::Failed {
                spec,
                attempts,
                retry_at,
            } => {
                if retry_at <= Instant::now() {
                    tracing::info!("retrying model load (attempt {})", attempts + 1);
                    self.spawn_load(spec, attempts);
                } else {
                    self.model_loading = ModelLoading::Failed {
                        spec,
                        attempts,
                        retry_at,
                    };
                }
            }
        }
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
    /// `config.max_surface_chars` caps the surface length `record` accepts;
    /// entries already on disk are loaded regardless (they can be removed
    /// with Ctrl+Delete or by eviction).
    pub fn init_learning_cache(&mut self, enabled: bool, config: LearningConfig) {
        if !enabled || self.learning.is_some() {
            return;
        }

        let cache = match Settings::learning_file() {
            Some(path) if path.exists() => match LearningCache::load(&path, config) {
                Ok(cache) => {
                    debug!(
                        "Learning cache loaded from {:?} ({} entries)",
                        path,
                        cache.entry_count()
                    );
                    cache
                }
                Err(e) => {
                    debug!("Failed to load learning cache from {:?}: {}", path, e);
                    LearningCache::new(config)
                }
            },
            Some(path) => {
                debug!("Learning cache not found at {:?}, starting empty", path);
                LearningCache::new(config)
            }
            None => {
                debug!("Could not determine learning cache path");
                LearningCache::new(config)
            }
        };
        self.learning = Some(cache);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_doubles_and_caps() {
        assert_eq!(retry_backoff(1), Duration::from_secs(30));
        assert_eq!(retry_backoff(2), Duration::from_secs(60));
        assert_eq!(retry_backoff(5), Duration::from_secs(480));
        assert_eq!(retry_backoff(6), Duration::from_secs(600));
        assert_eq!(retry_backoff(100), Duration::from_secs(600));
    }

    #[test]
    fn download_failures_are_transient_others_permanent() {
        let download = anyhow::Error::from(karukan_engine::kanji::KanjiError::Download(
            "connection refused".into(),
        ))
        .context("failed to initialize main model");
        assert!(matches!(
            classify_failure(&download),
            LoadFailure::Transient
        ));

        let unknown = anyhow::anyhow!("unknown model variant: foo");
        assert!(matches!(classify_failure(&unknown), LoadFailure::Permanent));
    }
}
