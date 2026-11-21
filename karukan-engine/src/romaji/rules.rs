use super::style::SymbolStyle;
use super::trie::TrieNode;

/// Build the conversion rules trie in `style`: the `,` `.` `/` `[` `]` keys
/// take their output from it, every other rule is fixed.
pub fn build_rules(style: SymbolStyle) -> TrieNode {
    let mut trie = TrieNode::new();

    // Vowels
    trie.insert("a", "あ");
    trie.insert("i", "い");
    trie.insert("u", "う");
    trie.insert("e", "え");
    trie.insert("o", "お");

    // K-row
    trie.insert("ka", "か");
    trie.insert("ki", "き");
    trie.insert("ku", "く");
    trie.insert("ke", "け");
    trie.insert("ko", "こ");
    trie.insert("kya", "きゃ");
    trie.insert("kyi", "きぃ");
    trie.insert("kyu", "きゅ");
    trie.insert("kye", "きぇ");
    trie.insert("kyo", "きょ");
    // kw series (くぁ行)
    trie.insert("kwa", "くぁ");
    trie.insert("kwi", "くぃ");
    trie.insert("kwu", "くぅ");
    trie.insert("kwe", "くぇ");
    trie.insert("kwo", "くぉ");

    // C-row (alternative K and CH sounds)
    trie.insert("ca", "か");
    trie.insert("ci", "し");
    trie.insert("cu", "く");
    trie.insert("ce", "せ");
    trie.insert("co", "こ");

    // Q-row (くぁ行 alternative)
    trie.insert("qa", "くぁ");
    trie.insert("qi", "くぃ");
    trie.insert("qu", "く");
    trie.insert("qe", "くぇ");
    trie.insert("qo", "くぉ");

    // G-row (dakuten)
    trie.insert("ga", "が");
    trie.insert("gi", "ぎ");
    trie.insert("gu", "ぐ");
    trie.insert("ge", "げ");
    trie.insert("go", "ご");
    trie.insert("gya", "ぎゃ");
    trie.insert("gyi", "ぎぃ");
    trie.insert("gyu", "ぎゅ");
    trie.insert("gye", "ぎぇ");
    trie.insert("gyo", "ぎょ");
    // gw series (ぐぁ行)
    trie.insert("gwa", "ぐぁ");
    trie.insert("gwi", "ぐぃ");
    trie.insert("gwu", "ぐぅ");
    trie.insert("gwe", "ぐぇ");
    trie.insert("gwo", "ぐぉ");

    // S-row
    trie.insert("sa", "さ");
    trie.insert("si", "し");
    trie.insert("su", "す");
    trie.insert("se", "せ");
    trie.insert("so", "そ");
    trie.insert("shi", "し");
    trie.insert("sha", "しゃ");
    trie.insert("shu", "しゅ");
    trie.insert("she", "しぇ");
    trie.insert("sho", "しょ");
    trie.insert("sya", "しゃ");
    trie.insert("syi", "しぃ");
    trie.insert("syu", "しゅ");
    trie.insert("sye", "しぇ");
    trie.insert("syo", "しょ");
    // sw series (すぁ行)
    trie.insert("swa", "すぁ");
    trie.insert("swi", "すぃ");
    trie.insert("swu", "すぅ");
    trie.insert("swe", "すぇ");
    trie.insert("swo", "すぉ");

    // Z-row (dakuten)
    trie.insert("za", "ざ");
    trie.insert("zi", "じ");
    trie.insert("zu", "ず");
    trie.insert("ze", "ぜ");
    trie.insert("zo", "ぞ");
    trie.insert("ji", "じ");
    trie.insert("ja", "じゃ");
    trie.insert("ju", "じゅ");
    trie.insert("je", "じぇ");
    trie.insert("jo", "じょ");
    trie.insert("zya", "じゃ");
    trie.insert("zyi", "じぃ");
    trie.insert("zyu", "じゅ");
    trie.insert("zye", "じぇ");
    trie.insert("zyo", "じょ");
    trie.insert("jya", "じゃ");
    trie.insert("jyi", "じぃ");
    trie.insert("jyu", "じゅ");
    trie.insert("jye", "じぇ");
    trie.insert("jyo", "じょ");
    // zw series (ずぁ行)
    trie.insert("zwa", "ずぁ");
    trie.insert("zwi", "ずぃ");
    trie.insert("zwu", "ずぅ");
    trie.insert("zwe", "ずぇ");
    trie.insert("zwo", "ずぉ");

    // T-row
    trie.insert("ta", "た");
    trie.insert("ti", "ち");
    trie.insert("tu", "つ");
    trie.insert("te", "て");
    trie.insert("to", "と");
    trie.insert("chi", "ち");
    trie.insert("tsu", "つ");
    trie.insert("cha", "ちゃ");
    trie.insert("chu", "ちゅ");
    trie.insert("che", "ちぇ");
    trie.insert("cho", "ちょ");
    trie.insert("tya", "ちゃ");
    trie.insert("tyi", "ちぃ");
    trie.insert("tyu", "ちゅ");
    trie.insert("tye", "ちぇ");
    trie.insert("tyo", "ちょ");
    trie.insert("cya", "ちゃ");
    trie.insert("cyi", "ちぃ");
    trie.insert("cyu", "ちゅ");
    trie.insert("cye", "ちぇ");
    trie.insert("cyo", "ちょ");
    trie.insert("tsa", "つぁ");
    trie.insert("tsi", "つぃ");
    trie.insert("tse", "つぇ");
    trie.insert("tso", "つぉ");
    // th series (てゃ行)
    trie.insert("tha", "てゃ");
    trie.insert("thi", "てぃ");
    trie.insert("t'i", "てぃ");
    trie.insert("thu", "てゅ");
    trie.insert("the", "てぇ");
    trie.insert("tho", "てょ");
    trie.insert("t'yu", "てゅ");
    // tw series (とぁ行)
    trie.insert("twa", "とぁ");
    trie.insert("twi", "とぃ");
    trie.insert("twu", "とぅ");
    trie.insert("twe", "とぇ");
    trie.insert("two", "とぉ");
    trie.insert("t'u", "とぅ");

    // D-row (dakuten)
    trie.insert("da", "だ");
    trie.insert("di", "ぢ");
    trie.insert("du", "づ");
    trie.insert("de", "で");
    trie.insert("do", "ど");
    trie.insert("dya", "ぢゃ");
    trie.insert("dyi", "ぢぃ");
    trie.insert("dyu", "ぢゅ");
    trie.insert("dye", "ぢぇ");
    trie.insert("dyo", "ぢょ");
    // dh series (でゃ行)
    trie.insert("dha", "でゃ");
    trie.insert("dhi", "でぃ");
    trie.insert("d'i", "でぃ");
    trie.insert("dhu", "でゅ");
    trie.insert("dhe", "でぇ");
    trie.insert("dho", "でょ");
    trie.insert("d'yu", "でゅ");
    // dw series (どぁ行)
    trie.insert("dwa", "どぁ");
    trie.insert("dwi", "どぃ");
    trie.insert("dwu", "どぅ");
    trie.insert("dwe", "どぇ");
    trie.insert("dwo", "どぉ");
    trie.insert("d'u", "どぅ");

    // N-row
    trie.insert("na", "な");
    trie.insert("ni", "に");
    trie.insert("nu", "ぬ");
    trie.insert("ne", "ね");
    trie.insert("no", "の");
    trie.insert("nya", "にゃ");
    trie.insert("nyi", "にぃ");
    trie.insert("nyu", "にゅ");
    trie.insert("nye", "にぇ");
    trie.insert("nyo", "にょ");
    trie.insert("nn", "ん");
    trie.insert("n'", "ん");
    trie.insert("xn", "ん");

    // H-row
    trie.insert("ha", "は");
    trie.insert("hi", "ひ");
    trie.insert("hu", "ふ");
    trie.insert("he", "へ");
    trie.insert("ho", "ほ");
    trie.insert("fu", "ふ");
    trie.insert("hya", "ひゃ");
    trie.insert("hyi", "ひぃ");
    trie.insert("hyu", "ひゅ");
    trie.insert("hye", "ひぇ");
    trie.insert("hyo", "ひょ");
    trie.insert("fa", "ふぁ");
    trie.insert("fi", "ふぃ");
    trie.insert("fe", "ふぇ");
    trie.insert("fo", "ふぉ");
    trie.insert("fya", "ふゃ");
    trie.insert("fyu", "ふゅ");
    trie.insert("fyo", "ふょ");
    // hw series (ふぁ行 alternative)
    trie.insert("hwa", "ふぁ");
    trie.insert("hwi", "ふぃ");
    trie.insert("hwe", "ふぇ");
    trie.insert("hwo", "ふぉ");
    trie.insert("hwyu", "ふゅ");

    // B-row (dakuten)
    trie.insert("ba", "ば");
    trie.insert("bi", "び");
    trie.insert("bu", "ぶ");
    trie.insert("be", "べ");
    trie.insert("bo", "ぼ");
    trie.insert("bya", "びゃ");
    trie.insert("byi", "びぃ");
    trie.insert("byu", "びゅ");
    trie.insert("bye", "びぇ");
    trie.insert("byo", "びょ");

    // P-row (handakuten)
    trie.insert("pa", "ぱ");
    trie.insert("pi", "ぴ");
    trie.insert("pu", "ぷ");
    trie.insert("pe", "ぺ");
    trie.insert("po", "ぽ");
    trie.insert("pya", "ぴゃ");
    trie.insert("pyi", "ぴぃ");
    trie.insert("pyu", "ぴゅ");
    trie.insert("pye", "ぴぇ");
    trie.insert("pyo", "ぴょ");

    // M-row
    trie.insert("ma", "ま");
    trie.insert("mi", "み");
    trie.insert("mu", "む");
    trie.insert("me", "め");
    trie.insert("mo", "も");
    trie.insert("mya", "みゃ");
    trie.insert("myi", "みぃ");
    trie.insert("myu", "みゅ");
    trie.insert("mye", "みぇ");
    trie.insert("myo", "みょ");

    // Y-row
    trie.insert("ya", "や");
    trie.insert("yi", "い");
    trie.insert("yu", "ゆ");
    trie.insert("ye", "いぇ");
    trie.insert("yo", "よ");

    // R-row
    trie.insert("ra", "ら");
    trie.insert("ri", "り");
    trie.insert("ru", "る");
    trie.insert("re", "れ");
    trie.insert("ro", "ろ");
    trie.insert("rya", "りゃ");
    trie.insert("ryi", "りぃ");
    trie.insert("ryu", "りゅ");
    trie.insert("rye", "りぇ");
    trie.insert("ryo", "りょ");

    // W-row
    trie.insert("wa", "わ");
    trie.insert("wi", "うぃ");
    trie.insert("wu", "う");
    trie.insert("we", "うぇ");
    trie.insert("wo", "を");
    trie.insert("wha", "うぁ");
    trie.insert("whi", "うぃ");
    trie.insert("whu", "う");
    trie.insert("whe", "うぇ");
    trie.insert("who", "うぉ");
    // Historical kana (ゐ, ゑ)
    trie.insert("wyi", "ゐ");
    trie.insert("wye", "ゑ");

    // V-row
    trie.insert("va", "ゔぁ");
    trie.insert("vi", "ゔぃ");
    trie.insert("vu", "ゔ");
    trie.insert("ve", "ゔぇ");
    trie.insert("vo", "ゔぉ");
    trie.insert("vya", "ゔゃ");
    trie.insert("vyi", "ゔぃ");
    trie.insert("vyu", "ゔゅ");
    trie.insert("vye", "ゔぇ");
    trie.insert("vyo", "ゔょ");

    // Small characters (la, li, lu, le, lo series)
    trie.insert("la", "ぁ");
    trie.insert("li", "ぃ");
    trie.insert("lu", "ぅ");
    trie.insert("le", "ぇ");
    trie.insert("lo", "ぉ");
    trie.insert("lya", "ゃ");
    trie.insert("lyi", "ぃ");
    trie.insert("lyu", "ゅ");
    trie.insert("lye", "ぇ");
    trie.insert("lyo", "ょ");
    trie.insert("ltu", "っ");
    trie.insert("ltsu", "っ");
    trie.insert("lwa", "ゎ");
    trie.insert("lka", "ヵ");
    trie.insert("lke", "ヶ");

    // Alternative small characters (x series)
    trie.insert("xa", "ぁ");
    trie.insert("xi", "ぃ");
    trie.insert("xu", "ぅ");
    trie.insert("xe", "ぇ");
    trie.insert("xo", "ぉ");
    trie.insert("xya", "ゃ");
    trie.insert("xyi", "ぃ");
    trie.insert("xyu", "ゅ");
    trie.insert("xye", "ぇ");
    trie.insert("xyo", "ょ");
    trie.insert("xtu", "っ");
    trie.insert("xtsu", "っ");
    trie.insert("xwa", "ゎ");
    trie.insert("xka", "ヵ");
    trie.insert("xke", "ヶ");

    // Long vowel mark
    trie.insert("-", "ー");

    // Punctuation and symbols
    trie.insert(",", style.punctuation.comma());
    trie.insert(".", style.punctuation.period());
    trie.insert("/", style.slash.slash());
    trie.insert("?", "？");
    trie.insert("!", "！");
    trie.insert("~", "〜");

    // Brackets
    trie.insert("[", style.bracket.open());
    trie.insert("]", style.bracket.close());

    // Z-special symbols (Google Japanese Input style)
    trie.insert("z/", "・");
    trie.insert("z.", "…");
    trie.insert("z,", "‥");
    trie.insert("zh", "←");
    trie.insert("zj", "↓");
    trie.insert("zk", "↑");
    trie.insert("zl", "→");
    trie.insert("z-", "〜");
    trie.insert("z[", "『");
    trie.insert("z]", "』");

    trie
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_vowels() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("a".chars()).output.unwrap(), "あ");
        assert_eq!(trie.search_longest("i".chars()).output.unwrap(), "い");
        assert_eq!(trie.search_longest("u".chars()).output.unwrap(), "う");
        assert_eq!(trie.search_longest("e".chars()).output.unwrap(), "え");
        assert_eq!(trie.search_longest("o".chars()).output.unwrap(), "お");
    }

    #[test]
    fn test_k_row() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("ka".chars()).output.unwrap(), "か");
        assert_eq!(trie.search_longest("ki".chars()).output.unwrap(), "き");
        assert_eq!(trie.search_longest("ku".chars()).output.unwrap(), "く");
        assert_eq!(trie.search_longest("ke".chars()).output.unwrap(), "け");
        assert_eq!(trie.search_longest("ko".chars()).output.unwrap(), "こ");
    }

    #[test]
    fn test_youon() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("kya".chars()).output.unwrap(), "きゃ");
        assert_eq!(trie.search_longest("sha".chars()).output.unwrap(), "しゃ");
        assert_eq!(trie.search_longest("cha".chars()).output.unwrap(), "ちゃ");
        assert_eq!(trie.search_longest("nya".chars()).output.unwrap(), "にゃ");
        assert_eq!(trie.search_longest("hya".chars()).output.unwrap(), "ひゃ");
        assert_eq!(trie.search_longest("mya".chars()).output.unwrap(), "みゃ");
        assert_eq!(trie.search_longest("rya".chars()).output.unwrap(), "りゃ");
        assert_eq!(trie.search_longest("gya".chars()).output.unwrap(), "ぎゃ");
        assert_eq!(trie.search_longest("ja".chars()).output.unwrap(), "じゃ");
        assert_eq!(trie.search_longest("bya".chars()).output.unwrap(), "びゃ");
        assert_eq!(trie.search_longest("pya".chars()).output.unwrap(), "ぴゃ");
    }

    #[test]
    fn test_small_characters() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("la".chars()).output.unwrap(), "ぁ");
        assert_eq!(trie.search_longest("li".chars()).output.unwrap(), "ぃ");
        assert_eq!(trie.search_longest("lu".chars()).output.unwrap(), "ぅ");
        assert_eq!(trie.search_longest("le".chars()).output.unwrap(), "ぇ");
        assert_eq!(trie.search_longest("lo".chars()).output.unwrap(), "ぉ");
        assert_eq!(trie.search_longest("lya".chars()).output.unwrap(), "ゃ");
        assert_eq!(trie.search_longest("lyu".chars()).output.unwrap(), "ゅ");
        assert_eq!(trie.search_longest("lyo".chars()).output.unwrap(), "ょ");
        assert_eq!(trie.search_longest("ltu".chars()).output.unwrap(), "っ");
    }

    #[test]
    fn test_n_variants() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("nn".chars()).output.unwrap(), "ん");
        assert_eq!(trie.search_longest("n'".chars()).output.unwrap(), "ん");
        assert_eq!(trie.search_longest("xn".chars()).output.unwrap(), "ん");
    }

    #[test]
    fn test_c_row() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("ca".chars()).output.unwrap(), "か");
        assert_eq!(trie.search_longest("ci".chars()).output.unwrap(), "し");
        assert_eq!(trie.search_longest("cu".chars()).output.unwrap(), "く");
        assert_eq!(trie.search_longest("ce".chars()).output.unwrap(), "せ");
        assert_eq!(trie.search_longest("co".chars()).output.unwrap(), "こ");
        assert_eq!(trie.search_longest("cya".chars()).output.unwrap(), "ちゃ");
        assert_eq!(trie.search_longest("cyu".chars()).output.unwrap(), "ちゅ");
        assert_eq!(trie.search_longest("cyo".chars()).output.unwrap(), "ちょ");
    }

    #[test]
    fn test_q_row() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("qa".chars()).output.unwrap(), "くぁ");
        assert_eq!(trie.search_longest("qi".chars()).output.unwrap(), "くぃ");
        assert_eq!(trie.search_longest("qu".chars()).output.unwrap(), "く");
        assert_eq!(trie.search_longest("qe".chars()).output.unwrap(), "くぇ");
        assert_eq!(trie.search_longest("qo".chars()).output.unwrap(), "くぉ");
    }

    #[test]
    fn test_kw_gw_series() {
        let trie = build_rules(SymbolStyle::default());
        // kw series
        assert_eq!(trie.search_longest("kwa".chars()).output.unwrap(), "くぁ");
        assert_eq!(trie.search_longest("kwi".chars()).output.unwrap(), "くぃ");
        assert_eq!(trie.search_longest("kwu".chars()).output.unwrap(), "くぅ");
        assert_eq!(trie.search_longest("kwe".chars()).output.unwrap(), "くぇ");
        assert_eq!(trie.search_longest("kwo".chars()).output.unwrap(), "くぉ");
        // gw series
        assert_eq!(trie.search_longest("gwa".chars()).output.unwrap(), "ぐぁ");
        assert_eq!(trie.search_longest("gwi".chars()).output.unwrap(), "ぐぃ");
        assert_eq!(trie.search_longest("gwu".chars()).output.unwrap(), "ぐぅ");
        assert_eq!(trie.search_longest("gwe".chars()).output.unwrap(), "ぐぇ");
        assert_eq!(trie.search_longest("gwo".chars()).output.unwrap(), "ぐぉ");
    }

    #[test]
    fn test_sw_zw_series() {
        let trie = build_rules(SymbolStyle::default());
        // sw series
        assert_eq!(trie.search_longest("swa".chars()).output.unwrap(), "すぁ");
        assert_eq!(trie.search_longest("swi".chars()).output.unwrap(), "すぃ");
        assert_eq!(trie.search_longest("swu".chars()).output.unwrap(), "すぅ");
        assert_eq!(trie.search_longest("swe".chars()).output.unwrap(), "すぇ");
        assert_eq!(trie.search_longest("swo".chars()).output.unwrap(), "すぉ");
        // zw series
        assert_eq!(trie.search_longest("zwa".chars()).output.unwrap(), "ずぁ");
        assert_eq!(trie.search_longest("zwi".chars()).output.unwrap(), "ずぃ");
        assert_eq!(trie.search_longest("zwu".chars()).output.unwrap(), "ずぅ");
        assert_eq!(trie.search_longest("zwe".chars()).output.unwrap(), "ずぇ");
        assert_eq!(trie.search_longest("zwo".chars()).output.unwrap(), "ずぉ");
    }

    #[test]
    fn test_th_dh_tw_dw_series() {
        let trie = build_rules(SymbolStyle::default());
        // th series
        assert_eq!(trie.search_longest("tha".chars()).output.unwrap(), "てゃ");
        assert_eq!(trie.search_longest("thi".chars()).output.unwrap(), "てぃ");
        assert_eq!(trie.search_longest("t'i".chars()).output.unwrap(), "てぃ");
        assert_eq!(trie.search_longest("thu".chars()).output.unwrap(), "てゅ");
        assert_eq!(trie.search_longest("the".chars()).output.unwrap(), "てぇ");
        assert_eq!(trie.search_longest("tho".chars()).output.unwrap(), "てょ");
        // dh series
        assert_eq!(trie.search_longest("dha".chars()).output.unwrap(), "でゃ");
        assert_eq!(trie.search_longest("dhi".chars()).output.unwrap(), "でぃ");
        assert_eq!(trie.search_longest("d'i".chars()).output.unwrap(), "でぃ");
        assert_eq!(trie.search_longest("dhu".chars()).output.unwrap(), "でゅ");
        // tw series
        assert_eq!(trie.search_longest("twa".chars()).output.unwrap(), "とぁ");
        assert_eq!(trie.search_longest("twi".chars()).output.unwrap(), "とぃ");
        assert_eq!(trie.search_longest("twu".chars()).output.unwrap(), "とぅ");
        assert_eq!(trie.search_longest("t'u".chars()).output.unwrap(), "とぅ");
        // dw series
        assert_eq!(trie.search_longest("dwa".chars()).output.unwrap(), "どぁ");
        assert_eq!(trie.search_longest("dwi".chars()).output.unwrap(), "どぃ");
        assert_eq!(trie.search_longest("dwu".chars()).output.unwrap(), "どぅ");
        assert_eq!(trie.search_longest("d'u".chars()).output.unwrap(), "どぅ");
    }

    #[test]
    fn test_hw_series() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("hwa".chars()).output.unwrap(), "ふぁ");
        assert_eq!(trie.search_longest("hwi".chars()).output.unwrap(), "ふぃ");
        assert_eq!(trie.search_longest("hwe".chars()).output.unwrap(), "ふぇ");
        assert_eq!(trie.search_longest("hwo".chars()).output.unwrap(), "ふぉ");
        assert_eq!(trie.search_longest("hwyu".chars()).output.unwrap(), "ふゅ");
    }

    #[test]
    fn test_w_row_modern() {
        let trie = build_rules(SymbolStyle::default());
        // Modern wi/we should be うぃ/うぇ
        assert_eq!(trie.search_longest("wi".chars()).output.unwrap(), "うぃ");
        assert_eq!(trie.search_longest("we".chars()).output.unwrap(), "うぇ");
        // Historical wyi/wye should be ゐ/ゑ
        assert_eq!(trie.search_longest("wyi".chars()).output.unwrap(), "ゐ");
        assert_eq!(trie.search_longest("wye".chars()).output.unwrap(), "ゑ");
    }

    #[test]
    fn test_small_ka_ke() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("xka".chars()).output.unwrap(), "ヵ");
        assert_eq!(trie.search_longest("xke".chars()).output.unwrap(), "ヶ");
        assert_eq!(trie.search_longest("lka".chars()).output.unwrap(), "ヵ");
        assert_eq!(trie.search_longest("lke".chars()).output.unwrap(), "ヶ");
    }

    #[test]
    fn test_z_special_symbols() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("z/".chars()).output.unwrap(), "・");
        assert_eq!(trie.search_longest("z.".chars()).output.unwrap(), "…");
        assert_eq!(trie.search_longest("z,".chars()).output.unwrap(), "‥");
        assert_eq!(trie.search_longest("zh".chars()).output.unwrap(), "←");
        assert_eq!(trie.search_longest("zj".chars()).output.unwrap(), "↓");
        assert_eq!(trie.search_longest("zk".chars()).output.unwrap(), "↑");
        assert_eq!(trie.search_longest("zl".chars()).output.unwrap(), "→");
        assert_eq!(trie.search_longest("z-".chars()).output.unwrap(), "〜");
        assert_eq!(trie.search_longest("z[".chars()).output.unwrap(), "『");
        assert_eq!(trie.search_longest("z]".chars()).output.unwrap(), "』");
    }

    #[test]
    fn test_brackets_and_punctuation() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("[".chars()).output.unwrap(), "「");
        assert_eq!(trie.search_longest("]".chars()).output.unwrap(), "」");
        assert_eq!(trie.search_longest(",".chars()).output.unwrap(), "、");
        assert_eq!(trie.search_longest(".".chars()).output.unwrap(), "。");
        assert_eq!(trie.search_longest("-".chars()).output.unwrap(), "ー");
        assert_eq!(trie.search_longest("~".chars()).output.unwrap(), "〜");
    }

    #[test]
    fn test_tsu_variants() {
        let trie = build_rules(SymbolStyle::default());
        assert_eq!(trie.search_longest("tsa".chars()).output.unwrap(), "つぁ");
        assert_eq!(trie.search_longest("tsi".chars()).output.unwrap(), "つぃ");
        assert_eq!(trie.search_longest("tse".chars()).output.unwrap(), "つぇ");
        assert_eq!(trie.search_longest("tso".chars()).output.unwrap(), "つぉ");
    }
}
