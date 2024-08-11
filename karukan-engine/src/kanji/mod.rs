//! Kanji conversion using llama.cpp GGUF inference

mod backend;
pub mod error;
pub mod hf_download;
pub mod llamacpp;
pub mod model_config;

pub use backend::{
    Backend, ConversionConfig, KanaKanjiConverter, build_jinen_prompt, clean_model_output,
};
pub use error::KanjiError;
pub use hf_download::{
    download_gguf, get_path_by_id, get_tokenizer_path, get_tokenizer_path_by_id, get_variant_path,
};
pub use llama_cpp_2::token::LlamaToken;
pub use llamacpp::{LlamaCppModel, NllScorer};
pub use model_config::{ModelFamily, ModelRegistry, VariantConfig, registry};

/// Special tokens for jinen format
pub const CONTEXT_TOKEN: char = '\u{ee02}';
pub const INPUT_START_TOKEN: char = '\u{ee00}';
pub const OUTPUT_START_TOKEN: char = '\u{ee01}';

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jinen_format() {
        let context = "今日は";
        let katakana = "コンニチハ";
        let prompt = format!(
            "{}{}{}{}{}",
            CONTEXT_TOKEN, context, INPUT_START_TOKEN, katakana, OUTPUT_START_TOKEN
        );

        assert!(prompt.contains('\u{ee02}'));
        assert!(prompt.contains('\u{ee00}'));
        assert!(prompt.contains('\u{ee01}'));
        assert_eq!(prompt, "\u{ee02}今日は\u{ee00}コンニチハ\u{ee01}");
    }
}
