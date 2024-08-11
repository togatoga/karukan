pub mod dict;
pub mod kana;
pub mod kanji;
pub mod learning;
pub mod romaji;

pub use dict::{Candidate as DictCandidate, DictEntry, Dictionary, LookupResult};
pub use kana::{hiragana_to_katakana, katakana_to_hiragana, normalize_nfkc};
pub use kanji::{Backend, KanaKanjiConverter};
pub use learning::LearningCache;
pub use romaji::{BackspaceResult, ConversionEvent, RomajiConverter};
