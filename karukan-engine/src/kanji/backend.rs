//! Backend interface for kanji conversion using llama.cpp

use super::error::KanjiError;
use super::hf_download::download_gguf;
use super::llamacpp::LlamaCppModel;
use super::{CONTEXT_TOKEN, INPUT_START_TOKEN, OUTPUT_START_TOKEN};
use crate::kana::{hiragana_to_katakana, normalize_nfkc};
use std::path::{Path, PathBuf};

type Result<T> = super::error::Result<T>;

/// Where a conversion model's GGUF comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModelSource {
    /// A HuggingFace repo; `tokenizer.json` comes from the same repo.
    Hf { repo: String, filename: String },
    /// A local GGUF file; `tokenizer.json` must sit in the same directory.
    Path(PathBuf),
}

impl ModelSource {
    /// Resolve to local `(gguf, tokenizer.json)` paths. HuggingFace files
    /// are served cache-first and downloaded on a cache miss.
    pub fn resolve(&self) -> Result<(PathBuf, PathBuf)> {
        match self {
            ModelSource::Hf { repo, filename } => Ok((
                download_gguf(repo, filename)?,
                download_gguf(repo, "tokenizer.json")?,
            )),
            ModelSource::Path(path) => {
                if !path.is_file() {
                    return Err(KanjiError::ModelNotFound(path.clone()));
                }
                let tokenizer = path.with_file_name("tokenizer.json");
                if !tokenizer.is_file() {
                    return Err(KanjiError::TokenizerNotFound(tokenizer));
                }
                Ok((path.clone(), tokenizer))
            }
        }
    }

    /// Display name: the GGUF filename stem.
    pub fn display_name(&self) -> String {
        let name = match self {
            ModelSource::Hf { filename, .. } => Path::new(filename),
            ModelSource::Path(path) => path.as_path(),
        };
        name.file_stem()
            .unwrap_or(name.as_os_str())
            .to_string_lossy()
            .into_owned()
    }
}

/// Configuration for kanji conversion
#[derive(Debug, Clone)]
pub struct ConversionConfig {
    /// Maximum number of new tokens to generate
    pub max_new_tokens: usize,
}

impl Default for ConversionConfig {
    fn default() -> Self {
        Self { max_new_tokens: 50 }
    }
}

/// Build a prompt in jinen format.
///
/// The prompt is NFKC-normalized: jinen models are trained on NFKC text and
/// full-width ASCII in the context degrades accuracy. The special tokens
/// (U+EE00–U+EE02) are unaffected by NFKC.
pub fn build_jinen_prompt(katakana: &str, context: &str) -> String {
    normalize_nfkc(&format!(
        "{}{}{}{}{}",
        CONTEXT_TOKEN, context, INPUT_START_TOKEN, katakana, OUTPUT_START_TOKEN
    ))
}

/// Clean model output by trimming whitespace.
///
/// Special tokens (BOS/EOS) are handled at the decode level via
/// `skip_special_tokens` rather than string replacement.
pub fn clean_model_output(text: &str) -> String {
    text.trim().to_string()
}

/// Inference backend configuration (llama.cpp GGUF format with external tokenizer)
#[derive(Debug, Clone)]
pub struct Backend {
    gguf_path: String,
    tokenizer_json_path: String,
    /// Display name for the model (the GGUF filename stem)
    display_name: String,
}

impl Backend {
    /// Resolve a model source into a loadable backend.
    pub fn from_source(source: &ModelSource) -> Result<Self> {
        let (gguf, tokenizer) = source.resolve()?;
        Ok(Backend {
            gguf_path: gguf.to_string_lossy().into_owned(),
            tokenizer_json_path: tokenizer.to_string_lossy().into_owned(),
            display_name: source.display_name(),
        })
    }
}

/// Kanji converter using llama.cpp backend
pub struct KanaKanjiConverter {
    model: LlamaCppModel,
    config: ConversionConfig,
    display_name: String,
}

impl KanaKanjiConverter {
    /// Create a new converter with the specified backend
    pub fn new(backend: Backend) -> Result<Self> {
        Self::with_config(backend, ConversionConfig::default())
    }

    /// Create a new converter with the specified backend and configuration
    pub fn with_config(backend: Backend, config: ConversionConfig) -> Result<Self> {
        let model = LlamaCppModel::from_file(&backend.gguf_path, &backend.tokenizer_json_path)?;
        Ok(KanaKanjiConverter {
            model,
            config,
            display_name: backend.display_name,
        })
    }

    /// Set the number of threads for inference (0 = default).
    pub fn set_n_threads(&mut self, n: u32) {
        self.model.set_n_threads(n);
    }

    /// Convert hiragana to kanji candidates
    ///
    /// # Arguments
    /// * `reading` - Input reading in hiragana
    /// * `context` - Left context (previously converted text)
    /// * `num_candidates` - Number of candidates to generate
    ///
    /// # Returns
    /// Vector of conversion candidates
    pub fn convert(
        &self,
        reading: &str,
        context: &str,
        num_candidates: usize,
    ) -> Result<Vec<String>> {
        // Convert hiragana to katakana (model expects katakana input)
        let katakana = hiragana_to_katakana(reading);

        // Build prompt in jinen format
        let prompt = build_jinen_prompt(&katakana, context);

        // Tokenize
        let tokens = self.model.tokenize(&prompt)?;
        let eos = Some(self.model.eos_token_id().0);

        let mut candidates = Vec::with_capacity(num_candidates);

        if num_candidates == 1 {
            // Single candidate: use greedy decoding (faster)
            let output_tokens = self
                .model
                .generate(&tokens, self.config.max_new_tokens, eos)?;
            let generated = &output_tokens[tokens.len()..];
            let text = self.model.decode(generated, true)?;
            let clean = clean_model_output(&text);

            if !clean.is_empty() {
                candidates.push(clean);
            }
        } else {
            // Multiple candidates: use beam search
            let results = self.model.generate_beam_search(
                &tokens,
                self.config.max_new_tokens,
                eos,
                num_candidates,
            )?;

            for (output_tokens, _score) in results {
                let text = self.model.decode(&output_tokens, true)?;
                let clean = clean_model_output(&text);

                if !clean.is_empty() && !candidates.contains(&clean) {
                    candidates.push(clean);
                }
            }
        }

        // If no candidates, return the original reading
        if candidates.is_empty() {
            candidates.push(reading.to_string());
        }

        Ok(candidates)
    }

    /// Get a human-readable model name for display
    pub fn model_display_name(&self) -> &str {
        &self.display_name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hf_source(repo: &str, filename: &str) -> ModelSource {
        ModelSource::Hf {
            repo: repo.to_string(),
            filename: filename.to_string(),
        }
    }

    #[test]
    fn test_from_source_missing_gguf() {
        let source = ModelSource::Path(PathBuf::from("/nonexistent/model.gguf"));
        let err = Backend::from_source(&source).unwrap_err();
        assert!(matches!(err, KanjiError::ModelNotFound(_)), "{err}");
    }

    #[test]
    fn test_from_source_missing_tokenizer() {
        let dir = tempfile::tempdir().unwrap();
        let gguf = dir.path().join("model.gguf");
        std::fs::write(&gguf, b"gguf").unwrap();

        let err = Backend::from_source(&ModelSource::Path(gguf)).unwrap_err();
        let expected = dir.path().join("tokenizer.json");
        match err {
            KanjiError::TokenizerNotFound(path) => assert_eq!(path, expected),
            other => panic!("expected TokenizerNotFound, got {other}"),
        }
    }

    #[test]
    fn test_from_source_local_path() {
        let dir = tempfile::tempdir().unwrap();
        let gguf = dir.path().join("my-model.gguf");
        std::fs::write(&gguf, b"gguf").unwrap();
        std::fs::write(dir.path().join("tokenizer.json"), b"{}").unwrap();

        let backend = Backend::from_source(&ModelSource::Path(gguf)).unwrap();
        assert_eq!(backend.display_name, "my-model");
    }

    #[test]
    fn test_display_name_is_filename_stem() {
        let source = hf_source("owner/repo.gguf", "jinen-v2-small-Q5_K_M.gguf");
        assert_eq!(source.display_name(), "jinen-v2-small-Q5_K_M");
    }

    #[test]

    fn test_default_model_conversion() {
        let source = hf_source(
            "togatogah/jinen-v2-small.gguf",
            "jinen-v2-small-Q5_K_M.gguf",
        );
        let backend = Backend::from_source(&source).expect("Failed to load default model");
        let converter = KanaKanjiConverter::new(backend).expect("Failed to create converter");

        let result = converter.convert("かんじ", "", 1);
        assert!(result.is_ok(), "Conversion failed: {:?}", result.err());

        let candidates = result.unwrap();
        assert!(!candidates.is_empty(), "No candidates returned");

        let output = &candidates[0];
        assert!(
            !output.contains("ã"),
            "Output contains mojibake: '{}'",
            output
        );
    }

    #[test]

    fn test_xsmall_special_tokens() {
        use super::super::{CONTEXT_TOKEN, INPUT_START_TOKEN, OUTPUT_START_TOKEN};
        let source = hf_source(
            "togatogah/jinen-v1-xsmall.gguf",
            "jinen-v1-xsmall-Q5_K_M.gguf",
        );
        let (path, tok_path) = source.resolve().expect("Failed to download model");
        let model = LlamaCppModel::from_file(&path, &tok_path).expect("Failed to load model");

        let prompt = build_jinen_prompt("テスト", "");
        let tokens = model.tokenize(&prompt).expect("Failed to tokenize");

        let mut found_context = false;
        let mut found_input_start = false;
        let mut found_output_start = false;

        for token in &tokens {
            let display = model.decode_token_for_display(*token);
            if display.contains(CONTEXT_TOKEN) {
                found_context = true;
            }
            if display.contains(INPUT_START_TOKEN) {
                found_input_start = true;
            }
            if display.contains(OUTPUT_START_TOKEN) {
                found_output_start = true;
            }
        }

        assert!(found_context, "CONTEXT token (U+EE02) not found");
        assert!(found_input_start, "INPUT_START token (U+EE00) not found");
        assert!(found_output_start, "OUTPUT_START token (U+EE01) not found");
    }

    #[test]

    fn test_xsmall_conversion() {
        let source = hf_source(
            "togatogah/jinen-v1-xsmall.gguf",
            "jinen-v1-xsmall-Q5_K_M.gguf",
        );
        let backend = Backend::from_source(&source).expect("Failed to download GGUF");
        let converter = KanaKanjiConverter::new(backend).expect("Failed to create converter");

        let result = converter.convert("かんじ", "", 1);
        assert!(result.is_ok(), "Conversion failed: {:?}", result.err());

        let candidates = result.unwrap();
        assert!(!candidates.is_empty(), "No candidates returned");

        let output = &candidates[0];
        assert!(
            !output.contains("ã"),
            "Output contains mojibake (GPT-2 byte encoding leak): '{}'",
            output
        );
    }
}
