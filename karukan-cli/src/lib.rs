//! Shared helpers for the karukan-cli binaries.

use anyhow::{Context, Result};
use karukan_engine::kanji::LlamaCppModel;
use karukan_im::config::Settings;
use std::path::Path;

/// Load a model from a direct GGUF file path or by `[models]` key from the
/// user's config.toml (falling back to the built-in defaults).
///
/// When `gguf` is `Some`, loads that file directly (`tokenizer_json` is
/// required); otherwise resolves `model_key` through the config's `[models]`
/// table.
pub fn load_llama_model(
    gguf: Option<&Path>,
    tokenizer_json: Option<&Path>,
    model_key: &str,
    n_ctx: u32,
) -> Result<LlamaCppModel> {
    if let Some(gguf_path) = gguf {
        let tok_path = tokenizer_json.ok_or_else(|| {
            anyhow::anyhow!("--tokenizer-json is required when loading a GGUF file path")
        })?;
        eprintln!("Loading GGUF from {}...", gguf_path.display());
        return LlamaCppModel::from_file_with_n_ctx(gguf_path, tok_path, n_ctx)
            .with_context(|| format!("Failed to load GGUF from {}", gguf_path.display()));
    }

    eprintln!("Resolving model '{}'...", model_key);
    let source = Settings::load()?.model_source(model_key)?;
    let (gguf_path, tok_path) = source.resolve()?;
    eprintln!("Model path: {}", gguf_path.display());
    eprintln!("Tokenizer: {}", tok_path.display());
    Ok(LlamaCppModel::from_file_with_n_ctx(
        &gguf_path, &tok_path, n_ctx,
    )?)
}
