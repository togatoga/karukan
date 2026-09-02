//! Where the chunk boundaries are: the splitting rules, and nothing else.
//!
//! Like a lexer: every character is classified into a [`Token`], and
//! [`group_chunks`] walks them once, packing them into chunks. Pure
//! functions of the text and the settings, so the rules can be exercised
//! without an engine.
//!
//! Why the text is split at all is `docs/chunking.md`; what the engine does
//! with the chunks is the parent module.
use std::collections::VecDeque;
use std::mem;

use karukan_engine::kana::is_digit;

/// Whether `c` is "Japanese": hiragana, katakana (incl. `ー`), or kanji.
/// Everything else — digits, letters, symbols, all punctuation — is not, and
/// only a chunk containing Japanese reaches the model. The 中黒 `・` sits in
/// the katakana block but is special-cased as a separator symbol, so it
/// counts against the absorption budget like any other mark.
pub(crate) fn is_japanese(c: char) -> bool {
    // 中黒 (・): a katakana-block separator, treated as a non-Japanese symbol.
    if c == '\u{30FB}' {
        return false;
    }
    matches!(c,
        '\u{3040}'..='\u{309F}'   // hiragana
        | '\u{30A0}'..='\u{30FF}' // katakana (incl. ー U+30FC)
        | '\u{3400}'..='\u{9FFF}' // CJK ideographs (kanji)
    )
}

/// Per-chunk limits, taken from the engine config.
#[derive(Debug, Clone, Copy)]
pub(super) struct ChunkLimits {
    /// Hard cap on a chunk's length in chars.
    pub(super) chars: usize,
    /// Marks a chunk containing Japanese may keep.
    pub(super) symbols: usize,
    /// Digits a chunk containing Japanese may keep.
    pub(super) digits: usize,
    /// Alphabet chars a chunk containing Japanese may keep.
    pub(super) alphabets: usize,
}

/// One character, classified. Only Japanese is converted; the rest is
/// passed through to the preedit as typed.
#[derive(Debug, Clone, Copy)]
enum Token {
    /// Hiragana, katakana, kanji: the only text the model ever sees.
    Japanese(char),
    Digit(char),
    /// Punctuation and every other mark.
    Symbol(char),
    Alphabet(char),
}

impl From<char> for Token {
    fn from(c: char) -> Self {
        if is_japanese(c) {
            Self::Japanese(c)
        } else if is_digit(c) {
            Self::Digit(c)
        } else if c.is_alphabetic() {
            Self::Alphabet(c)
        } else {
            Self::Symbol(c)
        }
    }
}

impl Token {
    /// The character it stands for.
    fn ch(self) -> char {
        match self {
            Self::Japanese(c) | Self::Digit(c) | Self::Symbol(c) | Self::Alphabet(c) => c,
        }
    }

    fn is_japanese(self) -> bool {
        matches!(self, Self::Japanese(_))
    }

    /// Same variant, whatever the char.
    fn same_kind(self, other: Self) -> bool {
        mem::discriminant(&self) == mem::discriminant(&other)
    }

    /// How many of this kind a chunk containing Japanese may keep. Japanese
    /// itself has no budget: it is what the budgets are spent alongside.
    fn budget(self, limits: ChunkLimits) -> Option<usize> {
        match self {
            Self::Japanese(_) => None,
            Self::Symbol(_) => Some(limits.symbols),
            Self::Digit(_) => Some(limits.digits),
            // The alphabet budget also covers the unfired romaji tail, which
            // would otherwise reach the model as part of the reading.
            Self::Alphabet(_) => Some(limits.alphabets),
        }
    }
}

/// Whether `token` still fits the chunk being built. Everything the rules
/// need is counted off the tokens the chunk already holds, so there is no
/// running tally to keep in step with them.
fn fits(token: Token, chunk: &[Token], limits: ChunkLimits) -> bool {
    if chunk.len() >= limits.chars {
        return false;
    }
    let has_japanese = chunk.iter().any(|t| t.is_japanese());
    match token.budget(limits) {
        // Japanese never joins a chunk that is pure passthrough: that chunk
        // is not converted, so the Japanese in it would never be either.
        None => has_japanese,
        // A passthrough chunk has nothing to convert, so the budgets do not
        // apply to it and it grows to the length cap. This is what keeps a
        // run of digits or marks together instead of one per chunk.
        Some(_) if !has_japanese => true,
        Some(budget) => chunk.iter().filter(|t| t.same_kind(token)).count() < budget,
    }
}

/// Split `chars` into the chunk readings: each token goes into the chunk
/// being built, or ends it and starts the next one. `breaks` are the manual
/// boundaries, sorted, so the next one is always at the front of the queue.
pub(super) fn group_chunks(chars: &[char], limits: ChunkLimits, breaks: &[usize]) -> Vec<String> {
    let mut breaks: VecDeque<usize> = breaks.iter().copied().collect();
    let mut chunks: Vec<Vec<Token>> = Vec::new();
    let mut chunk: Vec<Token> = Vec::new();
    for (pos, token) in chars.iter().copied().map(Token::from).enumerate() {
        let at_break = breaks.front() == Some(&pos);
        if at_break {
            breaks.pop_front();
        }
        if !chunk.is_empty() && (at_break || !fits(token, &chunk, limits)) {
            chunks.push(mem::take(&mut chunk));
        }
        chunk.push(token);
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
        .into_iter()
        .map(|chunk| chunk.into_iter().map(Token::ch).collect())
        .collect()
}

#[cfg(test)]
mod group_chunk_tests {
    use super::{ChunkLimits, group_chunks};

    /// The default per-chunk symbol cap (mirrors `EngineConfig::default` /
    /// default.toml).
    const SYMBOLS: usize = 1;
    /// Digits stay out of the converter (default.toml `chunk_digits = 0`).
    const DIGITS: usize = 0;
    /// Alphabet chars stay out too (default.toml `chunk_alphabets = 0`).
    const ALPHABETS: usize = 0;

    /// Split with the default caps and no manual breaks.
    fn split(s: &str, max: usize) -> Vec<String> {
        split_full(s, max, SYMBOLS, DIGITS, ALPHABETS, &[])
    }

    fn split_full(
        s: &str,
        max: usize,
        max_symbols: usize,
        max_digits: usize,
        max_alphabets: usize,
        breaks: &[usize],
    ) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        let limits = ChunkLimits {
            chars: max,
            symbols: max_symbols,
            digits: max_digits,
            alphabets: max_alphabets,
        };
        group_chunks(&chars, limits, breaks).into_iter().collect()
    }

    #[test]
    fn empty_input_has_no_chunks() {
        assert!(split("", 40).is_empty());
    }

    #[test]
    fn japanese_run_splits_by_length_cap() {
        assert_eq!(split("あいうえお", 2), vec!["あい", "うえ", "お"]);
    }

    #[test]
    fn long_japanese_run_hard_breaks() {
        assert_eq!(split("あいうえお", 3), vec!["あいう", "えお"]);
    }

    #[test]
    fn a_run_of_marks_after_japanese_forms_one_chunk() {
        // The first mark rides along; the rest have no Japanese in front of
        // them, so they grow into a single verbatim chunk instead of
        // splitting one by one.
        assert_eq!(split("ア、、、、、", 40), vec!["ア、", "、、、、"]);
    }

    #[test]
    fn a_mark_rides_along_with_the_japanese_around_it() {
        // The mark stays inline while the chunk has budget left, so 「おい、」
        // keeps converting as one unit instead of freezing 「おい」 (as 老)
        // the moment the mark is typed.
        assert_eq!(split("おい、", 10), vec!["おい、"]);
        assert_eq!(split("あ、いう", 10), vec!["あ、いう"]);
        assert_eq!(split("いいね！すごい", 10), vec!["いいね！すごい"]);
        assert_eq!(split("きごう〜", 10), vec!["きごう〜"]);
    }

    #[test]
    fn mark_past_the_cap_forces_a_new_chunk() {
        // One mark per chunk by default: the second opens a new chunk even
        // directly after Japanese, which is roughly one clause each.
        assert_eq!(split("あ、い。う", 10), vec!["あ、い", "。", "う"]);
        assert_eq!(
            split("おい、おまえだよ。まて、こら", 20),
            vec!["おい、おまえだよ", "。", "まて、こら"]
        );
        // The kept mark stays put, so the left chunk is not reshaped.
        assert_eq!(split("すごい！？", 10), vec!["すごい！", "？"]);
        assert_eq!(split("は、じ。", 10), vec!["は、じ", "。"]);
    }

    #[test]
    fn digits_ride_along_when_allowed() {
        // Raising the digit budget lets short runs go through the converter
        // with the text around them.
        assert_eq!(
            split_full("だい3かい", 10, SYMBOLS, 1, ALPHABETS, &[]),
            vec!["だい3かい"]
        );
        assert_eq!(
            split_full("あ12い", 10, SYMBOLS, 2, ALPHABETS, &[]),
            vec!["あ12い"]
        );
        // The budget fills like the symbol one: the digits that fit ride
        // along and the rest form a chunk of their own.
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 0, ALPHABETS, &[]),
            vec!["あ", "1234"]
        );
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 1, ALPHABETS, &[]),
            vec!["あ1", "234"]
        );
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 2, ALPHABETS, &[]),
            vec!["あ12", "34"]
        );
    }

    #[test]
    fn alphabets_never_ride_along_by_default() {
        // Latin text is passthrough, and so is the unfired romaji tail.
        assert_eq!(split("あいk", 40), vec!["あい", "k"]);
        // Raising the budget lets it ride, like the other two.
        assert_eq!(
            split_full("これはRustです", 40, SYMBOLS, DIGITS, 4, &[]),
            vec!["これはRustです"]
        );
        assert_eq!(
            split_full("あいkかき", 40, SYMBOLS, DIGITS, 1, &[]),
            vec!["あいkかき"]
        );
        // Only after Japanese, though: a chunk still never starts with the
        // text it absorbs.
        assert_eq!(
            split_full("Rustで", 40, SYMBOLS, DIGITS, 4, &[]),
            vec!["Rust", "で"]
        );
    }

    #[test]
    fn caps_are_configurable() {
        // Two marks per chunk.
        assert_eq!(
            split_full("あ、い。う", 10, 2, DIGITS, ALPHABETS, &[]),
            vec!["あ、い。う"]
        );
        // No marks at all: split at every one.
        assert_eq!(
            split_full("おい、", 10, 0, DIGITS, ALPHABETS, &[]),
            vec!["おい", "、"]
        );
    }

    #[test]
    fn chunk_with_no_japanese_is_exempt_from_the_cap() {
        // A chunk never *starts* with absorbed symbols: with no Japanese in
        // front of them, digits/symbols form a verbatim chunk of their own,
        // growing to the length cap regardless of how many symbols it holds.
        assert_eq!(split("123あ", 40), vec!["123", "あ"]);
        assert_eq!(split("1233413！！〜〜", 40), vec!["1233413！！〜〜"]);
        assert_eq!(split("iPhone15", 40), vec!["iPhone15"]);
    }

    #[test]
    fn non_japanese_run_is_capped_at_max() {
        assert_eq!(split("abcdef", 2), vec!["ab", "cd", "ef"]);
    }

    #[test]
    fn katakana_word_with_prolonged_mark_stays_together() {
        // `ー` (U+30FC) lives in the katakana block, so a katakana word is one
        // Japanese chunk and is never split off as a symbol.
        assert_eq!(split("スーパーマーケット", 40), vec!["スーパーマーケット"]);
    }

    #[test]
    fn middle_dot_counts_toward_the_symbol_cap() {
        // 中黒 ・ (U+30FB) sits in the katakana block but is special-cased as
        // a symbol: it is absorbed like any other mark and counts against the
        // cap.
        assert_eq!(split("ジョン・スミス", 40), vec!["ジョン・スミス"]);
        assert_eq!(split("あ・い・う・え", 40), vec!["あ・い", "・", "う・え"]);
    }

    #[test]
    fn manual_breaks_force_boundaries() {
        assert_eq!(
            split_full("あいうえ", 40, SYMBOLS, DIGITS, ALPHABETS, &[2]),
            vec!["あい", "うえ"]
        );
        assert_eq!(
            split_full("あいうえ", 40, SYMBOLS, DIGITS, ALPHABETS, &[1, 3]),
            vec!["あ", "いう", "え"]
        );
        // A break at 0 or at the very end changes nothing.
        assert_eq!(
            split_full("あい", 40, SYMBOLS, DIGITS, ALPHABETS, &[0]),
            vec!["あい"]
        );
        assert_eq!(
            split_full("あい", 40, SYMBOLS, DIGITS, ALPHABETS, &[2]),
            vec!["あい"]
        );
    }

    #[test]
    fn manual_break_splits_a_non_japanese_run() {
        assert_eq!(
            split_full("1234", 40, SYMBOLS, DIGITS, ALPHABETS, &[2]),
            vec!["12", "34"]
        );
    }

    #[test]
    fn absorbed_symbols_count_against_the_length_cap() {
        assert_eq!(split("あ、いうえ", 3), vec!["あ、い", "うえ"]);
    }
}
