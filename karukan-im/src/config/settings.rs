//! Settings configuration
//!
//! Manages user-configurable settings for the IME.
//! Default values are defined in `config/default.toml`.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

/// Default configuration TOML embedded from config/default.toml
const DEFAULT_CONFIG_TOML: &str = include_str!("../../config/default.toml");

/// Configuration settings for the IME
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    /// Conversion settings
    pub conversion: ConversionSettings,
    /// Learning cache settings
    pub learning: LearningSettings,
}

/// Conversion strategy mode
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StrategyMode {
    /// Adaptive: dynamically switch between main and light models based on latency
    #[default]
    Adaptive,
    /// Light: use light_model only (loaded into main slot, beam search on Space)
    Light,
    /// Main: use main model only (no light model loaded)
    Main,
}

/// Conversion-related settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversionSettings {
    /// Conversion strategy mode (adaptive, light, main)
    #[serde(default)]
    pub strategy: StrategyMode,
    /// Number of candidates to show on Space conversion
    pub num_candidates: usize,
    /// Use surrounding text (text left of cursor) as context for conversion
    pub use_context: bool,
    /// Maximum number of surrounding text characters passed to the conversion API
    pub max_context_length: usize,
    /// Path to dictionary binary file (optional, defaults to data_dir/dict.bin)
    pub dict_path: Option<String>,
    /// Model variant id (optional, defaults to registry default)
    pub model: Option<String>,
    /// Beam search model variant id (used on Space conversion, default model if unset)
    pub light_model: Option<String>,
    /// Token count threshold for beam search (at or below → beam, above → greedy)
    pub short_input_threshold: usize,
    /// Beam width for short input
    pub beam_width: usize,
    /// Maximum acceptable latency in milliseconds for auto-suggest (0 = disabled)
    /// When a main model conversion exceeds this, the engine adaptively switches to light_model
    pub max_latency_ms: u64,
    /// Number of threads for llama.cpp inference (0 = all cores, llama.cpp default)
    pub n_threads: u32,
}

/// Learning cache settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningSettings {
    /// Whether learning is enabled
    pub enabled: bool,
    /// Maximum number of total entries in the learning cache
    pub max_entries: usize,
}

impl Default for Settings {
    fn default() -> Self {
        toml::from_str(DEFAULT_CONFIG_TOML).expect("embedded default.toml must be valid")
    }
}

/// Recursively merge `overlay` TOML values on top of `base`.
fn merge_toml(base: &mut toml::Value, overlay: &toml::Value) {
    match (base, overlay) {
        (toml::Value::Table(base_table), toml::Value::Table(overlay_table)) => {
            for (key, value) in overlay_table {
                if let Some(base_value) = base_table.get_mut(key) {
                    merge_toml(base_value, value);
                } else {
                    base_table.insert(key.clone(), value.clone());
                }
            }
        }
        (base, _) => {
            *base = overlay.clone();
        }
    }
}

/// Parse user TOML content merged on top of default.toml.
fn parse_with_defaults(user_content: &str) -> Result<Settings> {
    let mut base: toml::Value = toml::from_str(DEFAULT_CONFIG_TOML)?;
    let user: toml::Value = toml::from_str(user_content)?;
    merge_toml(&mut base, &user);
    let settings: Settings = base.try_into()?;
    Ok(settings)
}

/// Get the project directories for karukan-im.
fn project_dirs() -> Option<ProjectDirs> {
    ProjectDirs::from("com", "karukan", "karukan-im")
}

impl Settings {
    /// Get the data directory path
    pub fn data_dir() -> Option<PathBuf> {
        project_dirs().map(|dirs| dirs.data_dir().to_path_buf())
    }

    /// Get the configuration directory path
    pub fn config_dir() -> Option<PathBuf> {
        project_dirs().map(|dirs| dirs.config_dir().to_path_buf())
    }

    /// Get the configuration file path
    pub fn config_file() -> Option<PathBuf> {
        Self::config_dir().map(|dir| dir.join("config.toml"))
    }

    /// Get the user dictionary directory path.
    ///
    /// All files in this directory are automatically loaded as user dictionaries.
    /// Default: `~/.local/share/karukan-im/user_dicts/`
    pub fn user_dict_dir() -> Option<PathBuf> {
        Self::data_dir().map(|dir| dir.join("user_dicts"))
    }

    /// Get the learning cache file path.
    ///
    /// Default: `~/.local/share/karukan-im/learning.tsv`
    pub fn learning_file() -> Option<PathBuf> {
        Self::data_dir().map(|dir| dir.join("learning.tsv"))
    }

    /// Load settings from the default configuration file.
    /// Falls back to embedded default.toml if the config file does not exist.
    pub fn load() -> Result<Self> {
        let Some(config_file) = Self::config_file() else {
            warn!("Could not determine config directory, using defaults");
            return Ok(Self::default());
        };

        if !config_file.exists() {
            debug!("Config file not found, using defaults");
            return Ok(Self::default());
        }

        debug!("Loading config from {:?}", config_file);
        let content = fs::read_to_string(&config_file)?;
        parse_with_defaults(&content)
    }

    /// Load settings from a specific file, merged on top of defaults.
    pub fn load_from(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        parse_with_defaults(&content)
    }

    /// Save settings to the default configuration file
    pub fn save(&self) -> Result<()> {
        let Some(config_file) = Self::config_file() else {
            anyhow::bail!("Could not determine config directory");
        };

        // Create config directory if it doesn't exist
        if let Some(parent) = config_file.parent() {
            fs::create_dir_all(parent)?;
        }

        debug!("Saving config to {:?}", config_file);
        let content = toml::to_string_pretty(self)?;
        fs::write(&config_file, content)?;
        Ok(())
    }

    /// Save settings to a specific file
    pub fn save_to(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_default_settings() {
        let settings = Settings::default();
        assert_eq!(settings.conversion.num_candidates, 9);
        assert!(settings.conversion.use_context);
        assert_eq!(settings.conversion.max_context_length, 20);
    }

    #[test]
    fn test_serialize_deserialize() {
        let settings = Settings::default();
        let toml_str = toml::to_string(&settings).unwrap();
        let loaded: Settings = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            loaded.conversion.num_candidates,
            settings.conversion.num_candidates
        );
    }

    #[test]
    fn test_load_from_file() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[conversion]
num_candidates = 5
use_context = false
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings.conversion.num_candidates, 5);
        assert!(!settings.conversion.use_context);
    }

    #[test]
    fn test_user_dict_dir() {
        let dir = Settings::user_dict_dir();
        // Should return Some on systems with a home directory
        if let Some(dir) = dir {
            assert!(dir.ends_with("user_dicts"));
        }
    }

    #[test]
    fn test_partial_config() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[conversion]
num_candidates = 3
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings.conversion.num_candidates, 3);
        // Should use default for unspecified values
        assert!(settings.conversion.use_context);
        assert_eq!(settings.conversion.max_context_length, 20);
    }

    #[test]
    fn test_strategy_default_when_unspecified() {
        // When strategy is not specified, it should default to Adaptive
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[conversion]
num_candidates = 5
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings.conversion.strategy, StrategyMode::Adaptive);
    }

    #[test]
    fn test_strategy_light() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[conversion]
strategy = "light"
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings.conversion.strategy, StrategyMode::Light);
    }

    #[test]
    fn test_strategy_main() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"
[conversion]
strategy = "main"
"#
        )
        .unwrap();

        let path = file.path().to_path_buf();
        let settings = Settings::load_from(&path).unwrap();
        assert_eq!(settings.conversion.strategy, StrategyMode::Main);
    }
}
