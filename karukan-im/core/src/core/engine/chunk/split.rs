//! Where the chunk boundaries are: the splitting rules, and nothing else.
//!
//! Two phases, like a lexer. [`tokenize`] classifies every character into a
//! [`Token`]; [`group_chunks`] pops them off the queue and packs them into
//! chunks. Both are pure functions of the text and the settings, so the
//! rules can be exercised without an engine.
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
    Letter(char),
}

impl From<char> for Token {
    fn from(c: char) -> Self {
        if is_japanese(c) {
            Self::Japanese(c)
        } else if is_digit(c) {
            Self::Digit(c)
        } else if c.is_alphabetic() {
            Self::Letter(c)
        } else {
            Self::Symbol(c)
        }
    }
}

impl Token {
    /// The character it stands for.
    fn ch(self) -> char {
        match self {
            Self::Japanese(c) | Self::Digit(c) | Self::Symbol(c) | Self::Letter(c) => c,
        }
    }

    fn is_japanese(self) -> bool {
        matches!(self, Self::Japanese(_))
    }
}

/// Classify every character, in order.
fn tokenize(chars: &[char]) -> VecDeque<Token> {
    chars.iter().copied().map(Token::from).collect()
}

/// Whether `chunk` can keep one more `token`. The budgets are counted off
/// the tokens the chunk already holds, so there is no running tally to keep
/// in step with them.
fn accepts(chunk: &[Token], token: Token, limits: ChunkLimits) -> bool {
    if chunk.len() >= limits.chars {
        return false;
    }
    let has_japanese = chunk.iter().any(|t| t.is_japanese());
    let count = |kept: fn(&Token) -> bool| chunk.iter().filter(|t| kept(t)).count();
    match token {
        // Japanese never joins a chunk that is pure passthrough.
        Token::Japanese(_) => has_japanese,
        // A passthrough chunk has nothing to convert, so the budgets do not
        // apply and it grows to the length cap.
        _ if !has_japanese => true,
        // Latin text is passthrough, and an unresolved romaji tail must not
        // reach the model as part of the reading.
        Token::Letter(_) => false,
        Token::Digit(_) => count(|t| matches!(t, Token::Digit(_))) < limits.digits,
        Token::Symbol(_) => count(|t| matches!(t, Token::Symbol(_))) < limits.symbols,
    }
}

/// Split `chars` into the chunk readings: each token goes into the chunk
/// being built, or ends it and starts the next one. `breaks` are the manual
/// boundaries, sorted, so the next one is always at the front.
pub(super) fn group_chunks(chars: &[char], limits: ChunkLimits, breaks: &[usize]) -> Vec<String> {
    let mut queue = tokenize(chars);
    let mut breaks: VecDeque<usize> = breaks.iter().copied().collect();
    let mut chunks: Vec<Vec<Token>> = Vec::new();
    let mut chunk: Vec<Token> = Vec::new();
    let mut pos = 0;
    while let Some(&token) = queue.front() {
        let at_break = breaks.front() == Some(&pos);
        if at_break {
            breaks.pop_front();
        }
        if !chunk.is_empty() && (at_break || !accepts(&chunk, token, limits)) {
            chunks.push(mem::take(&mut chunk));
        }
        chunk.push(queue.pop_front().expect("just peeked"));
        pos += 1;
    }
    if !chunk.is_empty() {
        chunks.push(chunk);
    }
    chunks
        .iter()
        .map(|chunk| chunk.iter().map(|t| t.ch()).collect())
        .collect()
}

#[cfg(test)]
mod tokenize_tests {
    use super::{Token, tokenize};

    /// The classification of each char, one letter per char.
    fn lex(s: &str) -> String {
        tokenize(&s.chars().collect::<Vec<_>>())
            .iter()
            .map(|t| match t {
                Token::Japanese(_) => 'J',
                Token::Digit(_) => 'D',
                Token::Symbol(_) => 'S',
                Token::Letter(_) => 'L',
            })
            .collect()
    }

    #[test]
    fn hiragana_katakana_and_kanji_are_one_class() {
        // A word is not classified into pieces at its script changes.
        assert_eq!(lex("私はパンを食べる"), "JJJJJJJJ");
        assert_eq!(lex("ラーメン"), "JJJJ");
    }

    #[test]
    fn every_char_gets_its_class() {
        assert_eq!(lex("あ12いabc"), "JDDJLLL");
    }

    #[test]
    fn marks_are_symbols_including_the_middle_dot() {
        assert_eq!(lex("あ、。"), "JSS");
        // 中黒 sits in the katakana block but is a separator, not Japanese.
        assert_eq!(lex("ア・イ"), "JSJ");
    }

    #[test]
    fn a_token_keeps_its_char() {
        let chars: Vec<char> = "あ1a、".chars().collect();
        let text: String = tokenize(&chars).iter().map(|t| t.ch()).collect();
        assert_eq!(text, "あ1a、");
    }

    #[test]
    fn empty_input_has_no_tokens() {
        assert!(lex("").is_empty());
    }
}

#[cfg(test)]
mod group_chunk_tests {
    use super::{ChunkLimits, group_chunks};

    /// The default per-chunk symbol cap (mirrors `EngineConfig::default` /
    /// default.toml).
    const SYMBOLS: usize = 1;
    /// Digits stay out of the converter (default.toml `chunk_digits = 0`).
    const DIGITS: usize = 0;

    /// Split with the default caps and no manual breaks.
    fn split(s: &str, max: usize) -> Vec<String> {
        split_full(s, max, SYMBOLS, DIGITS, &[])
    }

    fn split_full(
        s: &str,
        max: usize,
        max_symbols: usize,
        max_digits: usize,
        breaks: &[usize],
    ) -> Vec<String> {
        let chars: Vec<char> = s.chars().collect();
        let limits = ChunkLimits {
            chars: max,
            symbols: max_symbols,
            digits: max_digits,
        };
        group_chunks(&chars, limits, breaks).into_iter().collect()
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
            split_full("だい3かい", 10, SYMBOLS, 1, &[]),
            vec!["だい3かい"]
        );
        assert_eq!(split_full("あ12い", 10, SYMBOLS, 2, &[]), vec!["あ12い"]);
        // The budget fills like the symbol one: the digits that fit ride
        // along and the rest form a chunk of their own.
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 0, &[]),
            vec!["あ", "1234"]
        );
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 1, &[]),
            vec!["あ1", "234"]
        );
        assert_eq!(
            split_full("あ1234", 40, SYMBOLS, 2, &[]),
            vec!["あ12", "34"]
        );
    }

    #[test]
    fn letters_never_ride_along() {
        // Latin text is passthrough, and an unresolved romaji tail must not
        // reach the converter as part of the reading.
        assert_eq!(split("あいk", 40), vec!["あい", "k"]);
    }

    #[test]
    fn caps_are_configurable() {
        // Two marks per chunk.
        assert_eq!(
            split_full("あ、い。う", 10, 2, DIGITS, &[]),
            vec!["あ、い。う"]
        );
        // No marks at all: split at every one.
        assert_eq!(split_full("おい、", 10, 0, DIGITS, &[]), vec!["おい", "、"]);
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
            split_full("あいうえ", 40, SYMBOLS, DIGITS, &[2]),
            vec!["あい", "うえ"]
        );
        assert_eq!(
            split_full("あいうえ", 40, SYMBOLS, DIGITS, &[1, 3]),
            vec!["あ", "いう", "え"]
        );
        // A break at 0 or at the very end changes nothing.
        assert_eq!(split_full("あい", 40, SYMBOLS, DIGITS, &[0]), vec!["あい"]);
        assert_eq!(split_full("あい", 40, SYMBOLS, DIGITS, &[2]), vec!["あい"]);
    }

    #[test]
    fn manual_break_splits_a_non_japanese_run() {
        assert_eq!(
            split_full("1234", 40, SYMBOLS, DIGITS, &[2]),
            vec!["12", "34"]
        );
    }

    #[test]
    fn absorbed_symbols_count_against_the_length_cap() {
        assert_eq!(split("あ、いうえ", 3), vec!["あ、い", "うえ"]);
    }
}
