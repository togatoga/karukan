pub mod dict;
pub mod kana;
pub mod kanji;
pub mod learning;
pub mod rewriter;
pub mod romaji;

pub use dict::{Candidate as DictCandidate, DictEntry, Dictionary, LookupResult};
pub use kana::{
    contains_kana, hiragana_to_katakana, is_pure_full_katakana, is_pure_hiragana,
    katakana_to_hiragana, normalize_nfkc,
};
pub use kanji::{Backend, KanaKanjiConverter};
pub use learning::LearningCache;
pub use rewriter::{
    AlphabetRewriter, EmojiRewriter, HalfWidthKatakanaRewriter, RewriteOutput, Rewriter,
    RewriterChain, SymbolRewriter, description as symbol_description,
};
pub use romaji::{BackspaceResult, ConversionEvent, RomajiConverter};
