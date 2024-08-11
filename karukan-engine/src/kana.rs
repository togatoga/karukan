use unicode_normalization::UnicodeNormalization;

/// Apply NFKC normalization to text.
///
/// This is needed for models whose tokenizer does NOT support full-width ASCII
/// characters in its vocabulary. Without NFKC normalization, characters like
/// `（`, `）`, `！`, `？` are incorrectly tokenized as EOS tokens, causing
/// generation to stop prematurely.
///
/// NFKC normalization converts:
/// - Full-width ASCII → Half-width: `（` → `(`, `！` → `!`, `？` → `?`
/// - Full-width digits → Half-width: `０` → `0`, `１` → `1`
/// - Compatibility characters → Canonical forms
///
/// Note: Hiragana, Katakana, and Kanji are NOT affected by NFKC normalization.
/// The special jinen tokens (U+EE00-U+EE02) in Private Use Area are also preserved.
pub fn normalize_nfkc(text: &str) -> String {
    text.nfkc().collect()
}

/// Convert hiragana to katakana
pub fn hiragana_to_katakana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            // Hiragana range (U+3041-U+3096) -> Katakana (U+30A1-U+30F6)
            '\u{3041}'..='\u{3096}' => std::char::from_u32(c as u32 + 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Convert katakana to hiragana
pub fn katakana_to_hiragana(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            // Katakana range (U+30A1-U+30F6) -> Hiragana (U+3041-U+3096)
            '\u{30A1}'..='\u{30F6}' => std::char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hiragana_to_katakana() {
        assert_eq!(hiragana_to_katakana("あいうえお"), "アイウエオ");
        assert_eq!(hiragana_to_katakana("こんにちは"), "コンニチハ");
        assert_eq!(hiragana_to_katakana("きゃきゅきょ"), "キャキュキョ");
        assert_eq!(hiragana_to_katakana("がぎぐげご"), "ガギグゲゴ");
        assert_eq!(hiragana_to_katakana("ぱぴぷぺぽ"), "パピプペポ");

        // Mixed with non-hiragana should pass through
        assert_eq!(hiragana_to_katakana("abc123"), "abc123");
        assert_eq!(hiragana_to_katakana("あいうabc"), "アイウabc");
    }

    #[test]
    fn test_katakana_to_hiragana() {
        assert_eq!(katakana_to_hiragana("アイウエオ"), "あいうえお");
        assert_eq!(katakana_to_hiragana("コンニチハ"), "こんにちは");
        assert_eq!(katakana_to_hiragana("キャキュキョ"), "きゃきゅきょ");
    }

    #[test]
    fn test_round_trip() {
        let original = "こんにちは";
        let katakana = hiragana_to_katakana(original);
        let back = katakana_to_hiragana(&katakana);
        assert_eq!(original, back);
    }

    #[test]
    fn test_normalize_nfkc() {
        // Full-width ASCII should be converted to half-width
        assert_eq!(normalize_nfkc("（）"), "()");
        assert_eq!(normalize_nfkc("！？"), "!?");
        assert_eq!(normalize_nfkc("Ａｂｃ"), "Abc");
        assert_eq!(normalize_nfkc("０１２３"), "0123");

        // Full-width punctuation
        assert_eq!(normalize_nfkc("、。"), "、。"); // These are NOT full-width ASCII
        assert_eq!(normalize_nfkc("「」"), "「」"); // Japanese brackets preserved

        // Hiragana, Katakana, Kanji should be preserved
        assert_eq!(normalize_nfkc("あいうえお"), "あいうえお");
        assert_eq!(normalize_nfkc("アイウエオ"), "アイウエオ");
        assert_eq!(normalize_nfkc("漢字"), "漢字");

        // Mixed text
        assert_eq!(normalize_nfkc("（カッコ）テスト！"), "(カッコ)テスト!");

        // Special jinen tokens (Private Use Area U+EE00-U+EE02) should be preserved
        assert_eq!(normalize_nfkc("\u{ee00}"), "\u{ee00}");
        assert_eq!(normalize_nfkc("\u{ee01}"), "\u{ee01}");
        assert_eq!(normalize_nfkc("\u{ee02}"), "\u{ee02}");
        assert_eq!(
            normalize_nfkc("\u{ee02}context\u{ee00}input\u{ee01}"),
            "\u{ee02}context\u{ee00}input\u{ee01}"
        );
    }
}
