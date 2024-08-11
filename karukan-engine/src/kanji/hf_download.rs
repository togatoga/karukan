//! HuggingFace model download utilities
//!
//! Downloads GGUF models from HuggingFace Hub and caches them locally.
//! Model definitions are loaded from `models.toml` via [`super::model_config`].

use super::error::KanjiError;
use super::model_config::{ModelFamily, VariantConfig, registry};
type Result<T> = super::error::Result<T>;
use hf_hub::{Repo, RepoType, api::sync::ApiBuilder};
use std::path::PathBuf;

/// Download a GGUF model from HuggingFace Hub
///
/// Returns the local path to the downloaded file.
/// The file is cached in the HuggingFace cache directory (~/.cache/huggingface/hub/).
///
/// # Arguments
/// * `repo_id` - HuggingFace repository ID
/// * `filename` - Filename to download
///
/// # Environment Variables
/// * `HF_TOKEN` - HuggingFace API token (required for private repositories)
pub fn download_gguf(repo_id: &str, filename: &str) -> Result<PathBuf> {
    // Check for HF_TOKEN environment variable
    let mut builder = ApiBuilder::new();
    if let Ok(token) = std::env::var("HF_TOKEN") {
        builder = builder.with_token(Some(token));
    }
    let api = builder
        .build()
        .map_err(|e| KanjiError::Download(e.into()))?;

    let repo = api.repo(Repo::new(repo_id.to_string(), RepoType::Model));

    tracing::info!("Downloading {} from {}...", filename, repo_id);

    let path = repo
        .get(filename)
        .map_err(|e| KanjiError::Download(e.into()))?;

    tracing::info!("Downloaded to {:?}", path);

    Ok(path)
}

/// Get local path for a variant, downloading if not cached.
pub fn get_variant_path(family: &ModelFamily, variant: &VariantConfig) -> Result<PathBuf> {
    download_gguf(&family.repo_id, &variant.filename)
}

/// Get the local path to `tokenizer.json` for a model family, downloading if necessary.
pub fn get_tokenizer_path(family: &ModelFamily) -> Result<PathBuf> {
    download_gguf(&family.repo_id, "tokenizer.json")
}

/// Convenience: look up a variant id in the global registry and return its local GGUF path.
pub fn get_path_by_id(variant_id: &str) -> Result<PathBuf> {
    let (family, variant) = registry()
        .find_variant(variant_id)
        .ok_or_else(|| KanjiError::UnknownVariant(variant_id.to_string()))?;
    get_variant_path(family, variant)
}

/// Convenience: look up a variant id and return the tokenizer path.
pub fn get_tokenizer_path_by_id(variant_id: &str) -> Result<PathBuf> {
    let (family, _variant) = registry()
        .find_variant(variant_id)
        .ok_or_else(|| KanjiError::UnknownVariant(variant_id.to_string()))?;
    get_tokenizer_path(family)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_default_variant_exists() {
        let reg = registry();
        let (family, variant) = reg.default_variant().expect("default variant must exist");
        assert!(!family.repo_id.is_empty());
        assert!(!variant.filename.is_empty());
    }

    #[test]
    fn test_find_all_variants() {
        let reg = registry();
        for id in reg.all_variant_ids() {
            assert!(
                reg.find_variant(id).is_some(),
                "find_variant should succeed for '{}'",
                id
            );
        }
    }

    #[test]
    fn test_get_path_by_id_unknown() {
        let result = get_path_by_id("nonexistent-model-id");
        assert!(result.is_err());
    }

    #[test]
    fn test_tokenizer_path_by_id_unknown() {
        let result = get_tokenizer_path_by_id("nonexistent-model-id");
        assert!(result.is_err());
    }
}
