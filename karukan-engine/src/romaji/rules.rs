use super::trie::TrieNode;

/// Build the conversion rules trie
pub fn build_rules() -> TrieNode {
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
    trie.insert(",", "、");
    trie.insert(".", "。");
    trie.insert("/", "・");
    trie.insert("?", "？");
    trie.insert("!", "！");
    trie.insert("~", "〜");

    // Brackets
    trie.insert("[", "「");
    trie.insert("]", "」");

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
        let trie = build_rules();
        assert_eq!(trie.search_longest("a").output.unwrap(), "あ");
        assert_eq!(trie.search_longest("i").output.unwrap(), "い");
        assert_eq!(trie.search_longest("u").output.unwrap(), "う");
        assert_eq!(trie.search_longest("e").output.unwrap(), "え");
        assert_eq!(trie.search_longest("o").output.unwrap(), "お");
    }

    #[test]
    fn test_k_row() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("ka").output.unwrap(), "か");
        assert_eq!(trie.search_longest("ki").output.unwrap(), "き");
        assert_eq!(trie.search_longest("ku").output.unwrap(), "く");
        assert_eq!(trie.search_longest("ke").output.unwrap(), "け");
        assert_eq!(trie.search_longest("ko").output.unwrap(), "こ");
    }

    #[test]
    fn test_youon() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("kya").output.unwrap(), "きゃ");
        assert_eq!(trie.search_longest("sha").output.unwrap(), "しゃ");
        assert_eq!(trie.search_longest("cha").output.unwrap(), "ちゃ");
        assert_eq!(trie.search_longest("nya").output.unwrap(), "にゃ");
        assert_eq!(trie.search_longest("hya").output.unwrap(), "ひゃ");
        assert_eq!(trie.search_longest("mya").output.unwrap(), "みゃ");
        assert_eq!(trie.search_longest("rya").output.unwrap(), "りゃ");
        assert_eq!(trie.search_longest("gya").output.unwrap(), "ぎゃ");
        assert_eq!(trie.search_longest("ja").output.unwrap(), "じゃ");
        assert_eq!(trie.search_longest("bya").output.unwrap(), "びゃ");
        assert_eq!(trie.search_longest("pya").output.unwrap(), "ぴゃ");
    }

    #[test]
    fn test_small_characters() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("la").output.unwrap(), "ぁ");
        assert_eq!(trie.search_longest("li").output.unwrap(), "ぃ");
        assert_eq!(trie.search_longest("lu").output.unwrap(), "ぅ");
        assert_eq!(trie.search_longest("le").output.unwrap(), "ぇ");
        assert_eq!(trie.search_longest("lo").output.unwrap(), "ぉ");
        assert_eq!(trie.search_longest("lya").output.unwrap(), "ゃ");
        assert_eq!(trie.search_longest("lyu").output.unwrap(), "ゅ");
        assert_eq!(trie.search_longest("lyo").output.unwrap(), "ょ");
        assert_eq!(trie.search_longest("ltu").output.unwrap(), "っ");
    }

    #[test]
    fn test_n_variants() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("nn").output.unwrap(), "ん");
        assert_eq!(trie.search_longest("n'").output.unwrap(), "ん");
        assert_eq!(trie.search_longest("xn").output.unwrap(), "ん");
    }

    #[test]
    fn test_c_row() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("ca").output.unwrap(), "か");
        assert_eq!(trie.search_longest("ci").output.unwrap(), "し");
        assert_eq!(trie.search_longest("cu").output.unwrap(), "く");
        assert_eq!(trie.search_longest("ce").output.unwrap(), "せ");
        assert_eq!(trie.search_longest("co").output.unwrap(), "こ");
        assert_eq!(trie.search_longest("cya").output.unwrap(), "ちゃ");
        assert_eq!(trie.search_longest("cyu").output.unwrap(), "ちゅ");
        assert_eq!(trie.search_longest("cyo").output.unwrap(), "ちょ");
    }

    #[test]
    fn test_q_row() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("qa").output.unwrap(), "くぁ");
        assert_eq!(trie.search_longest("qi").output.unwrap(), "くぃ");
        assert_eq!(trie.search_longest("qu").output.unwrap(), "く");
        assert_eq!(trie.search_longest("qe").output.unwrap(), "くぇ");
        assert_eq!(trie.search_longest("qo").output.unwrap(), "くぉ");
    }

    #[test]
    fn test_kw_gw_series() {
        let trie = build_rules();
        // kw series
        assert_eq!(trie.search_longest("kwa").output.unwrap(), "くぁ");
        assert_eq!(trie.search_longest("kwi").output.unwrap(), "くぃ");
        assert_eq!(trie.search_longest("kwu").output.unwrap(), "くぅ");
        assert_eq!(trie.search_longest("kwe").output.unwrap(), "くぇ");
        assert_eq!(trie.search_longest("kwo").output.unwrap(), "くぉ");
        // gw series
        assert_eq!(trie.search_longest("gwa").output.unwrap(), "ぐぁ");
        assert_eq!(trie.search_longest("gwi").output.unwrap(), "ぐぃ");
        assert_eq!(trie.search_longest("gwu").output.unwrap(), "ぐぅ");
        assert_eq!(trie.search_longest("gwe").output.unwrap(), "ぐぇ");
        assert_eq!(trie.search_longest("gwo").output.unwrap(), "ぐぉ");
    }

    #[test]
    fn test_sw_zw_series() {
        let trie = build_rules();
        // sw series
        assert_eq!(trie.search_longest("swa").output.unwrap(), "すぁ");
        assert_eq!(trie.search_longest("swi").output.unwrap(), "すぃ");
        assert_eq!(trie.search_longest("swu").output.unwrap(), "すぅ");
        assert_eq!(trie.search_longest("swe").output.unwrap(), "すぇ");
        assert_eq!(trie.search_longest("swo").output.unwrap(), "すぉ");
        // zw series
        assert_eq!(trie.search_longest("zwa").output.unwrap(), "ずぁ");
        assert_eq!(trie.search_longest("zwi").output.unwrap(), "ずぃ");
        assert_eq!(trie.search_longest("zwu").output.unwrap(), "ずぅ");
        assert_eq!(trie.search_longest("zwe").output.unwrap(), "ずぇ");
        assert_eq!(trie.search_longest("zwo").output.unwrap(), "ずぉ");
    }

    #[test]
    fn test_th_dh_tw_dw_series() {
        let trie = build_rules();
        // th series
        assert_eq!(trie.search_longest("tha").output.unwrap(), "てゃ");
        assert_eq!(trie.search_longest("thi").output.unwrap(), "てぃ");
        assert_eq!(trie.search_longest("t'i").output.unwrap(), "てぃ");
        assert_eq!(trie.search_longest("thu").output.unwrap(), "てゅ");
        assert_eq!(trie.search_longest("the").output.unwrap(), "てぇ");
        assert_eq!(trie.search_longest("tho").output.unwrap(), "てょ");
        // dh series
        assert_eq!(trie.search_longest("dha").output.unwrap(), "でゃ");
        assert_eq!(trie.search_longest("dhi").output.unwrap(), "でぃ");
        assert_eq!(trie.search_longest("d'i").output.unwrap(), "でぃ");
        assert_eq!(trie.search_longest("dhu").output.unwrap(), "でゅ");
        // tw series
        assert_eq!(trie.search_longest("twa").output.unwrap(), "とぁ");
        assert_eq!(trie.search_longest("twi").output.unwrap(), "とぃ");
        assert_eq!(trie.search_longest("twu").output.unwrap(), "とぅ");
        assert_eq!(trie.search_longest("t'u").output.unwrap(), "とぅ");
        // dw series
        assert_eq!(trie.search_longest("dwa").output.unwrap(), "どぁ");
        assert_eq!(trie.search_longest("dwi").output.unwrap(), "どぃ");
        assert_eq!(trie.search_longest("dwu").output.unwrap(), "どぅ");
        assert_eq!(trie.search_longest("d'u").output.unwrap(), "どぅ");
    }

    #[test]
    fn test_hw_series() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("hwa").output.unwrap(), "ふぁ");
        assert_eq!(trie.search_longest("hwi").output.unwrap(), "ふぃ");
        assert_eq!(trie.search_longest("hwe").output.unwrap(), "ふぇ");
        assert_eq!(trie.search_longest("hwo").output.unwrap(), "ふぉ");
        assert_eq!(trie.search_longest("hwyu").output.unwrap(), "ふゅ");
    }

    #[test]
    fn test_w_row_modern() {
        let trie = build_rules();
        // Modern wi/we should be うぃ/うぇ
        assert_eq!(trie.search_longest("wi").output.unwrap(), "うぃ");
        assert_eq!(trie.search_longest("we").output.unwrap(), "うぇ");
        // Historical wyi/wye should be ゐ/ゑ
        assert_eq!(trie.search_longest("wyi").output.unwrap(), "ゐ");
        assert_eq!(trie.search_longest("wye").output.unwrap(), "ゑ");
    }

    #[test]
    fn test_small_ka_ke() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("xka").output.unwrap(), "ヵ");
        assert_eq!(trie.search_longest("xke").output.unwrap(), "ヶ");
        assert_eq!(trie.search_longest("lka").output.unwrap(), "ヵ");
        assert_eq!(trie.search_longest("lke").output.unwrap(), "ヶ");
    }

    #[test]
    fn test_z_special_symbols() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("z/").output.unwrap(), "・");
        assert_eq!(trie.search_longest("z.").output.unwrap(), "…");
        assert_eq!(trie.search_longest("z,").output.unwrap(), "‥");
        assert_eq!(trie.search_longest("zh").output.unwrap(), "←");
        assert_eq!(trie.search_longest("zj").output.unwrap(), "↓");
        assert_eq!(trie.search_longest("zk").output.unwrap(), "↑");
        assert_eq!(trie.search_longest("zl").output.unwrap(), "→");
        assert_eq!(trie.search_longest("z-").output.unwrap(), "〜");
        assert_eq!(trie.search_longest("z[").output.unwrap(), "『");
        assert_eq!(trie.search_longest("z]").output.unwrap(), "』");
    }

    #[test]
    fn test_brackets_and_punctuation() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("[").output.unwrap(), "「");
        assert_eq!(trie.search_longest("]").output.unwrap(), "」");
        assert_eq!(trie.search_longest(",").output.unwrap(), "、");
        assert_eq!(trie.search_longest(".").output.unwrap(), "。");
        assert_eq!(trie.search_longest("-").output.unwrap(), "ー");
        assert_eq!(trie.search_longest("~").output.unwrap(), "〜");
    }

    #[test]
    fn test_tsu_variants() {
        let trie = build_rules();
        assert_eq!(trie.search_longest("tsa").output.unwrap(), "つぁ");
        assert_eq!(trie.search_longest("tsi").output.unwrap(), "つぃ");
        assert_eq!(trie.search_longest("tse").output.unwrap(), "つぇ");
        assert_eq!(trie.search_longest("tso").output.unwrap(), "つぉ");
    }
}
