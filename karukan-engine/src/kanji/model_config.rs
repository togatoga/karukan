//! TOML-based model configuration
//!
//! All supported GGUF models are defined in `models.toml` at the crate root.
//! This module deserializes that file and provides a global registry for lookup.

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// Top-level config parsed from `models.toml`
#[derive(Debug, Deserialize)]
pub struct ModelRegistry {
    /// Default variant id (e.g. "jinen-v1-xsmall-q5")
    pub default_model: String,
    /// Model families keyed by short name (e.g. "jinen-v1-xsmall")
    pub models: HashMap<String, ModelFamily>,
}

/// A model family (one HuggingFace repo, multiple quantisation variants)
#[derive(Debug, Deserialize)]
pub struct ModelFamily {
    pub repo_id: String,
    pub display_name: String,
    #[serde(default)]
    pub pre_tokenizer_override: Option<String>,
    /// Quantisation variants keyed by short name (e.g. "q5", "f16")
    pub variants: HashMap<String, VariantConfig>,
}

/// A single downloadable GGUF variant
#[derive(Debug, Deserialize)]
pub struct VariantConfig {
    /// Unique variant id (e.g. "jinen-v1-xsmall-q5")
    pub id: String,
    /// GGUF filename in the HuggingFace repo
    pub filename: String,
    /// Human-readable name shown in UI
    pub display_name: String,
}

static REGISTRY: OnceLock<ModelRegistry> = OnceLock::new();

/// Return the global model registry, parsed once from the embedded `models.toml`.
pub fn registry() -> &'static ModelRegistry {
    REGISTRY.get_or_init(|| {
        let toml_str = include_str!("../../models.toml");
        toml::from_str(toml_str).expect("Failed to parse models.toml")
    })
}

impl ModelRegistry {
    /// Look up a variant by its unique id (e.g. "jinen-v1-xsmall-q5").
    ///
    /// Returns `(family, variant)` if found.
    pub fn find_variant(&self, variant_id: &str) -> Option<(&ModelFamily, &VariantConfig)> {
        for family in self.models.values() {
            for variant in family.variants.values() {
                if variant.id == variant_id {
                    return Some((family, variant));
                }
            }
        }
        None
    }

    /// Return the default `(family, variant)` pair.
    pub fn default_variant(&self) -> Option<(&ModelFamily, &VariantConfig)> {
        self.find_variant(&self.default_model)
    }

    /// All variant ids across every model family.
    pub fn all_variant_ids(&self) -> Vec<&str> {
        let mut ids = Vec::new();
        for family in self.models.values() {
            for variant in family.variants.values() {
                ids.push(variant.id.as_str());
            }
        }
        ids
    }

    /// Iterate over all `(family, variant)` pairs.
    pub fn iter_variants(&self) -> impl Iterator<Item = (&ModelFamily, &VariantConfig)> {
        self.models
            .values()
            .flat_map(|f| f.variants.values().map(move |v| (f, v)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_registry() {
        let reg = registry();
        assert_eq!(reg.default_model, "jinen-v1-small-q5");
        assert_eq!(reg.models.len(), 2, "Expected exactly 2 model families");
    }

    #[test]
    fn test_find_variant() {
        let reg = registry();
        let (family, variant) = reg
            .find_variant("jinen-v1-xsmall-q5")
            .expect("variant not found");
        assert_eq!(family.repo_id, "togatogah/jinen-v1-xsmall.gguf");
        assert_eq!(variant.filename, "jinen-v1-xsmall-Q5_K_M.gguf");
    }

    #[test]
    fn test_find_variant_small() {
        let reg = registry();
        let (family, variant) = reg
            .find_variant("jinen-v1-small-q5")
            .expect("variant not found");
        assert_eq!(family.repo_id, "togatogah/jinen-v1-small.gguf");
        assert_eq!(variant.filename, "jinen-v1-small-Q5_K_M.gguf");
    }

    #[test]
    fn test_default_variant() {
        let reg = registry();
        let (family, variant) = reg.default_variant().expect("default not found");
        assert_eq!(variant.id, "jinen-v1-small-q5");
        assert_eq!(family.repo_id, "togatogah/jinen-v1-small.gguf");
    }

    #[test]
    fn test_all_variant_ids() {
        let reg = registry();
        let ids = reg.all_variant_ids();
        assert_eq!(
            ids.len(),
            2,
            "Expected exactly 2 variants, got {}",
            ids.len()
        );
        assert!(ids.contains(&"jinen-v1-xsmall-q5"));
        assert!(ids.contains(&"jinen-v1-small-q5"));
    }

    #[test]
    fn test_iter_variants() {
        let reg = registry();
        let count = reg.iter_variants().count();
        assert_eq!(count, 2, "Expected exactly 2 variants, got {}", count);
    }

    #[test]
    fn test_unknown_variant_returns_none() {
        let reg = registry();
        assert!(reg.find_variant("nonexistent-model").is_none());
    }

    #[test]
    fn test_variant_ids_unique() {
        let reg = registry();
        let ids = reg.all_variant_ids();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(ids.len(), unique.len(), "Duplicate variant ids found");
    }
}
