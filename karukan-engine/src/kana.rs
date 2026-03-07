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

/// Convert hiragana to half-width katakana (半角カタカナ)
///
/// Dakuten (゛) and handakuten (゜) are decomposed into two code points.
/// For example: が → ｶﾞ (U+FF76 + U+FF9E)
pub fn hiragana_to_halfwidth_katakana(text: &str) -> String {
    let mut result = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            'あ' => result.push('ｱ'),
            'い' => result.push('ｲ'),
            'う' => result.push('ｳ'),
            'え' => result.push('ｴ'),
            'お' => result.push('ｵ'),
            'か' => result.push('ｶ'),
            'き' => result.push('ｷ'),
            'く' => result.push('ｸ'),
            'け' => result.push('ｹ'),
            'こ' => result.push('ｺ'),
            'さ' => result.push('ｻ'),
            'し' => result.push('ｼ'),
            'す' => result.push('ｽ'),
            'せ' => result.push('ｾ'),
            'そ' => result.push('ｿ'),
            'た' => result.push('ﾀ'),
            'ち' => result.push('ﾁ'),
            'つ' => result.push('ﾂ'),
            'て' => result.push('ﾃ'),
            'と' => result.push('ﾄ'),
            'な' => result.push('ﾅ'),
            'に' => result.push('ﾆ'),
            'ぬ' => result.push('ﾇ'),
            'ね' => result.push('ﾈ'),
            'の' => result.push('ﾉ'),
            'は' => result.push('ﾊ'),
            'ひ' => result.push('ﾋ'),
            'ふ' => result.push('ﾌ'),
            'へ' => result.push('ﾍ'),
            'ほ' => result.push('ﾎ'),
            'ま' => result.push('ﾏ'),
            'み' => result.push('ﾐ'),
            'む' => result.push('ﾑ'),
            'め' => result.push('ﾒ'),
            'も' => result.push('ﾓ'),
            'や' => result.push('ﾔ'),
            'ゆ' => result.push('ﾕ'),
            'よ' => result.push('ﾖ'),
            'ら' => result.push('ﾗ'),
            'り' => result.push('ﾘ'),
            'る' => result.push('ﾙ'),
            'れ' => result.push('ﾚ'),
            'ろ' => result.push('ﾛ'),
            'わ' => result.push('ﾜ'),
            'を' => result.push('ｦ'),
            'ん' => result.push('ﾝ'),
            // Dakuten (voiced) kana → base + ﾞ
            'が' => result.push_str("ｶﾞ"),
            'ぎ' => result.push_str("ｷﾞ"),
            'ぐ' => result.push_str("ｸﾞ"),
            'げ' => result.push_str("ｹﾞ"),
            'ご' => result.push_str("ｺﾞ"),
            'ざ' => result.push_str("ｻﾞ"),
            'じ' => result.push_str("ｼﾞ"),
            'ず' => result.push_str("ｽﾞ"),
            'ぜ' => result.push_str("ｾﾞ"),
            'ぞ' => result.push_str("ｿﾞ"),
            'だ' => result.push_str("ﾀﾞ"),
            'ぢ' => result.push_str("ﾁﾞ"),
            'づ' => result.push_str("ﾂﾞ"),
            'で' => result.push_str("ﾃﾞ"),
            'ど' => result.push_str("ﾄﾞ"),
            'ば' => result.push_str("ﾊﾞ"),
            'び' => result.push_str("ﾋﾞ"),
            'ぶ' => result.push_str("ﾌﾞ"),
            'べ' => result.push_str("ﾍﾞ"),
            'ぼ' => result.push_str("ﾎﾞ"),
            'ゔ' => result.push_str("ｳﾞ"),
            // Handakuten (semi-voiced) kana → base + ﾟ
            'ぱ' => result.push_str("ﾊﾟ"),
            'ぴ' => result.push_str("ﾋﾟ"),
            'ぷ' => result.push_str("ﾌﾟ"),
            'ぺ' => result.push_str("ﾍﾟ"),
            'ぽ' => result.push_str("ﾎﾟ"),
            // Small kana
            'ぁ' => result.push('ｧ'),
            'ぃ' => result.push('ｨ'),
            'ぅ' => result.push('ｩ'),
            'ぇ' => result.push('ｪ'),
            'ぉ' => result.push('ｫ'),
            'っ' => result.push('ｯ'),
            'ゃ' => result.push('ｬ'),
            'ゅ' => result.push('ｭ'),
            'ょ' => result.push('ｮ'),
            // Punctuation
            '。' => result.push('｡'),
            '、' => result.push('､'),
            '・' => result.push('･'),
            'ー' => result.push('ｰ'),
            '「' => result.push('｢'),
            '」' => result.push('｣'),
            // Pass through anything else
            _ => result.push(c),
        }
    }
    result
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
    fn test_hiragana_to_halfwidth_katakana() {
        // Basic vowels
        assert_eq!(hiragana_to_halfwidth_katakana("あいうえお"), "ｱｲｳｴｵ");
        // Basic consonants
        assert_eq!(hiragana_to_halfwidth_katakana("かきくけこ"), "ｶｷｸｹｺ");
        // Dakuten (voiced) — two code points each
        assert_eq!(hiragana_to_halfwidth_katakana("がぎぐげご"), "ｶﾞｷﾞｸﾞｹﾞｺﾞ");
        // Handakuten (semi-voiced)
        assert_eq!(hiragana_to_halfwidth_katakana("ぱぴぷぺぽ"), "ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ");
        // Small kana
        assert_eq!(
            hiragana_to_halfwidth_katakana("ぁぃぅぇぉっゃゅょ"),
            "ｧｨｩｪｫｯｬｭｮ"
        );
        // Special
        assert_eq!(hiragana_to_halfwidth_katakana("ん"), "ﾝ");
        assert_eq!(hiragana_to_halfwidth_katakana("ー"), "ｰ");
        // Mixed text passes through non-hiragana
        assert_eq!(hiragana_to_halfwidth_katakana("abc123"), "abc123");
        assert_eq!(hiragana_to_halfwidth_katakana("あいうabc"), "ｱｲｳabc");
        // Punctuation
        assert_eq!(hiragana_to_halfwidth_katakana("。、"), "｡､");
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
