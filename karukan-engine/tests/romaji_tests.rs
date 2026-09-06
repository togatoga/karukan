use karukan_engine::RomajiConverter;

/// Converted text of `raw` (pending excluded).
fn text(raw: &str) -> String {
    RomajiConverter::new().convert(raw).text
}

/// Pending (unresolved trailing input) of `raw`.
fn pending(raw: &str) -> String {
    RomajiConverter::new().convert(raw).pending
}

/// Committed form of `raw`: converted text plus flushed pending.
fn flushed(raw: &str) -> String {
    RomajiConverter::new().convert_flush(raw)
}

fn assert_converts(cases: &[(&str, &str)]) {
    let conv = RomajiConverter::new();
    for (raw, expected) in cases {
        assert_eq!(&conv.convert(raw).text, expected, "input: {raw}");
    }
}

#[test]
fn test_vowels() {
    assert_converts(&[
        ("a", "あ"),
        ("i", "い"),
        ("u", "う"),
        ("e", "え"),
        ("o", "お"),
    ]);
}

#[test]
fn test_k_row() {
    assert_converts(&[
        ("ka", "か"),
        ("ki", "き"),
        ("ku", "く"),
        ("ke", "け"),
        ("ko", "こ"),
    ]);
}

#[test]
fn test_s_row() {
    assert_converts(&[
        ("sa", "さ"),
        ("shi", "し"),
        ("su", "す"),
        ("se", "せ"),
        ("so", "そ"),
    ]);
}

#[test]
fn test_t_row() {
    assert_converts(&[
        ("ta", "た"),
        ("chi", "ち"),
        ("tsu", "つ"),
        ("te", "て"),
        ("to", "と"),
    ]);
}

#[test]
fn test_n_row() {
    assert_converts(&[
        ("na", "な"),
        ("ni", "に"),
        ("nu", "ぬ"),
        ("ne", "ね"),
        ("no", "の"),
    ]);
}

#[test]
fn test_h_row() {
    assert_converts(&[
        ("ha", "は"),
        ("hi", "ひ"),
        ("fu", "ふ"),
        ("he", "へ"),
        ("ho", "ほ"),
    ]);
}

#[test]
fn test_m_row() {
    assert_converts(&[
        ("ma", "ま"),
        ("mi", "み"),
        ("mu", "む"),
        ("me", "め"),
        ("mo", "も"),
    ]);
}

#[test]
fn test_y_row() {
    assert_converts(&[("ya", "や"), ("yu", "ゆ"), ("yo", "よ")]);
}

#[test]
fn test_r_row() {
    assert_converts(&[
        ("ra", "ら"),
        ("ri", "り"),
        ("ru", "る"),
        ("re", "れ"),
        ("ro", "ろ"),
    ]);
}

#[test]
fn test_w_row() {
    assert_converts(&[("wa", "わ"), ("wo", "を")]);
}

#[test]
fn test_g_row_dakuten() {
    assert_converts(&[
        ("ga", "が"),
        ("gi", "ぎ"),
        ("gu", "ぐ"),
        ("ge", "げ"),
        ("go", "ご"),
    ]);
}

#[test]
fn test_z_row_dakuten() {
    assert_converts(&[
        ("za", "ざ"),
        ("ji", "じ"),
        ("zu", "ず"),
        ("ze", "ぜ"),
        ("zo", "ぞ"),
    ]);
}

#[test]
fn test_d_row_dakuten() {
    assert_converts(&[("da", "だ"), ("de", "で"), ("do", "ど")]);
}

#[test]
fn test_b_row_dakuten() {
    assert_converts(&[
        ("ba", "ば"),
        ("bi", "び"),
        ("bu", "ぶ"),
        ("be", "べ"),
        ("bo", "ぼ"),
    ]);
}

#[test]
fn test_p_row_handakuten() {
    assert_converts(&[
        ("pa", "ぱ"),
        ("pi", "ぴ"),
        ("pu", "ぷ"),
        ("pe", "ぺ"),
        ("po", "ぽ"),
    ]);
}

#[test]
fn test_youon_kya_series() {
    assert_converts(&[("kya", "きゃ"), ("kyu", "きゅ"), ("kyo", "きょ")]);
}

#[test]
fn test_youon_sha_series() {
    assert_converts(&[("sha", "しゃ"), ("shu", "しゅ"), ("sho", "しょ")]);
}

#[test]
fn test_youon_cha_series() {
    assert_converts(&[("cha", "ちゃ"), ("chu", "ちゅ"), ("cho", "ちょ")]);
}

#[test]
fn test_youon_nya_series() {
    assert_converts(&[("nya", "にゃ"), ("nyu", "にゅ"), ("nyo", "にょ")]);
}

#[test]
fn test_sokuon() {
    assert_converts(&[
        // kk -> っk
        ("kko", "っこ"),
        // tt -> っt
        ("tte", "って"),
        // pp -> っp
        ("ppa", "っぱ"),
    ]);
}

#[test]
fn test_n_variants() {
    // nn -> immediately converts to ん
    assert_eq!(text("nn"), "ん");
    assert_eq!(pending("nn"), "");

    // n' -> ん
    assert_eq!(text("n'"), "ん");
}

#[test]
fn test_small_characters() {
    assert_converts(&[("la", "ぁ"), ("li", "ぃ"), ("lu", "ぅ"), ("ltu", "っ")]);
}

#[test]
fn test_real_words() {
    // With IME-style nn rule: "konnichiha" -> こ + nn->ん + i->い + chiha->ちは = "こんいちは"
    // To get "こんにちは", use "konnnichiha" (3 n's)
    assert_converts(&[
        ("konnichiha", "こんいちは"),
        ("konnnichiha", "こんにちは"),
        ("arigatou", "ありがとう"),
        ("gakkou", "がっこう"),
        ("nihongo", "にほんご"),
        ("kitte", "きって"),
        // "annindouhu" -> あ + nn->ん + i->い + n before d->ん + douhu->どうふ
        ("annindouhu", "あんいんどうふ"),
        // To get "あんにんどうふ" (almond jelly), use "annninndouhu"
        // (nnn -> ん+n remaining, ni -> に, nn before d -> ん)
        ("annninndouhu", "あんにんどうふ"),
    ]);

    // "karukan" (single n at end): trailing 'n' stays pending (ambiguous),
    // and flushing outputs it as-is
    assert_eq!(text("karukan"), "かるか");
    assert_eq!(pending("karukan"), "n");
    assert_eq!(flushed("karukan"), "かるかn");

    // "karukann" (nn at end) -> "かるかん" immediately (nn converts right away)
    assert_eq!(text("karukann"), "かるかん");
    assert_eq!(pending("karukann"), "");

    // Multiple input styles for the same output
    assert_eq!(text("narezzi"), "なれっじ");
    assert_eq!(text("nareltuzi"), "なれっじ");
}

#[test]
fn test_nn_edge_cases() {
    // Standalone "nn" should immediately convert to ん
    assert_eq!(text("nn"), "ん", "nn should immediately convert to ん");
    assert_eq!(pending("nn"), "", "pending should be empty after nn");

    // "nnn" - first "nn" -> ん immediately, then "n" pending
    assert_eq!(text("nnn"), "ん");
    assert_eq!(pending("nnn"), "n");

    // "nnnn" - first "nn" -> ん, then second "nn" -> ん
    assert_eq!(text("nnnn"), "んん");
    assert_eq!(pending("nnnn"), "");

    assert_converts(&[
        // "nni" should be "んい" (nn -> ん, i -> い)
        ("nni", "んい"),
        // "nna" should be "んあ" (nn -> ん, a -> あ)
        ("nna", "んあ"),
        // "nnka" should be "んか" (nn is explicit ん, ka->か)
        ("nnka", "んか"),
        // "kannna" should be "かんな" (ka->か, nn->ん when followed by consonant n, na->な)
        ("kannna", "かんな"),
        // Word ending in "nn": nn converts immediately to ん
        ("karukann", "かるかん"),
        // ny* patterns should be yōon, not n + vowel
        ("nya", "にゃ"),
        ("nyo", "にょ"),
        ("nyu", "にゅ"),
        // "nn" is ALWAYS ん in IME style, so "nnyo" = ん + yo = んよ
        ("nnyo", "んよ"),
        // To get こんにゃく, you need "konnnyaku"
        // (3 n's: nn->ん when followed by consonant n, nya->にゃ)
        ("konnnyaku", "こんにゃく"),
        // "annyo" = あ + nn->ん + yo->よ = あんよ
        ("annyo", "あんよ"),
        ("konnnichiha", "こんにちは"),
    ]);
}

#[test]
fn test_sentences() {
    assert_converts(&[
        ("watashihagennkidesu", "わたしはげんきです"),
        ("kyouhaiitennkidesu", "きょうはいいてんきです"),
        ("toukyouhashibuyaku", "とうきょうはしぶやく"),
        (
            "nihonngowobennkyoushiteimasu",
            "にほんごをべんきょうしています",
        ),
    ]);
}

#[test]
fn test_zenninn() {
    // zenninn -> ぜんいん ("nn" converts immediately, no flush needed)
    assert_eq!(text("zenninn"), "ぜんいん");
    assert_eq!(pending("zenninn"), "");
}

#[test]
fn test_zenninn_kanji_conversion() {
    use karukan_engine::{Backend, KanaKanjiConverter, ModelSource};
    let hiragana = text("zenninn");
    println!("Hiragana: {}", hiragana);

    // Now try kanji conversion
    let source = ModelSource::Hf {
        repo: "togatogah/jinen-v2-small.gguf".to_string(),
        filename: "jinen-v2-small-Q5_K_M.gguf".to_string(),
    };
    let backend = Backend::from_source(&source).expect("Failed to load backend");
    let kanji_conv = KanaKanjiConverter::new(backend).expect("Failed to create converter");
    let result = kanji_conv.convert(&hiragana, "", 1);
    println!("Kanji result: {:?}", result);
}
