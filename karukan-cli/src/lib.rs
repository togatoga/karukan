//! Shared helpers for the karukan-cli binaries.

use anyhow::{Context, Result};
use karukan_engine::kanji::{LlamaCppModel, get_path_by_id, get_tokenizer_path_by_id};
use std::path::Path;

/// Load a model from a direct GGUF file path or by registry variant id.
///
/// When `gguf` is `Some`, loads that file directly (`tokenizer_json` is
/// required); otherwise downloads/loads the registry variant `model_id`.
pub fn load_llama_model(
    gguf: Option<&Path>,
    tokenizer_json: Option<&Path>,
    model_id: &str,
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

    eprintln!("Downloading/loading model variant: {} ...", model_id);
    let gguf_path = get_path_by_id(model_id)?;
    let tok_path = get_tokenizer_path_by_id(model_id)?;
    eprintln!("Model path: {}", gguf_path.display());
    eprintln!("Tokenizer: {}", tok_path.display());
    Ok(LlamaCppModel::from_file_with_n_ctx(
        &gguf_path, &tok_path, n_ctx,
    )?)
}
