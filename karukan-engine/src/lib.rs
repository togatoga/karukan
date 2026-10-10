pub mod dict;
pub mod kana;
pub mod kanji;
pub mod learning;
pub mod rewriter;
pub mod romaji;
pub mod width;

pub use dict::{Candidate as DictCandidate, DictEntry, Dictionary, LookupResult, PredictiveMatch};
pub use kana::{
    contains_kana, hiragana_to_katakana, is_pure_full_katakana, is_pure_hiragana,
    katakana_to_hiragana, normalize_nfkc,
};
pub use kanji::{KanaKanjiConverter, ModelSource};
pub use learning::{LearningCache, LearningConfig};
pub use rewriter::{
    AlphabetRewriter, DateConfig, DatePhrase, DateRewriter, EmojiRewriter,
    HalfWidthKatakanaRewriter, RewriteOutput, Rewriter, RewriterChain, SingleKanjiRewriter,
    SymbolRewriter, description as symbol_description, single_kanji_description,
};
pub use romaji::{
    BracketStyle, Converted, PunctuationStyle, RomajiConverter, SlashStyle, SymbolStyle,
};
pub use width::{Width, WidthRules};
