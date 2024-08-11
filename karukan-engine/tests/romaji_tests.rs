use karukan_engine::RomajiConverter;

#[test]
fn test_vowels() {
    let mut conv = RomajiConverter::new();

    // Test all vowels
    conv.reset();
    "a".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "あ");

    conv.reset();
    "i".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "い");

    conv.reset();
    "u".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "う");

    conv.reset();
    "e".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "え");

    conv.reset();
    "o".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "お");
}

#[test]
fn test_k_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ka".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "か");

    conv.reset();
    "ki".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "き");

    conv.reset();
    "ku".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "く");

    conv.reset();
    "ke".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "け");

    conv.reset();
    "ko".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "こ");
}

#[test]
fn test_s_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "sa".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "さ");

    conv.reset();
    "shi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "し");

    conv.reset();
    "su".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "す");

    conv.reset();
    "se".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "せ");

    conv.reset();
    "so".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "そ");
}

#[test]
fn test_t_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ta".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "た");

    conv.reset();
    "chi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ち");

    conv.reset();
    "tsu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "つ");

    conv.reset();
    "te".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "て");

    conv.reset();
    "to".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "と");
}

#[test]
fn test_n_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "na".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "な");

    conv.reset();
    "ni".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "に");

    conv.reset();
    "nu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぬ");

    conv.reset();
    "ne".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ね");

    conv.reset();
    "no".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "の");
}

#[test]
fn test_h_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "は");

    conv.reset();
    "hi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ひ");

    conv.reset();
    "fu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ふ");

    conv.reset();
    "he".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "へ");

    conv.reset();
    "ho".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ほ");
}

#[test]
fn test_m_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ma".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ま");

    conv.reset();
    "mi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "み");

    conv.reset();
    "mu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "む");

    conv.reset();
    "me".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "め");

    conv.reset();
    "mo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "も");
}

#[test]
fn test_y_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ya".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "や");

    conv.reset();
    "yu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ゆ");

    conv.reset();
    "yo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "よ");
}

#[test]
fn test_r_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ra".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ら");

    conv.reset();
    "ri".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "り");

    conv.reset();
    "ru".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "る");

    conv.reset();
    "re".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "れ");

    conv.reset();
    "ro".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ろ");
}

#[test]
fn test_w_row() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "wa".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "わ");

    conv.reset();
    "wo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "を");
}

#[test]
fn test_g_row_dakuten() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ga".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "が");

    conv.reset();
    "gi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぎ");

    conv.reset();
    "gu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぐ");

    conv.reset();
    "ge".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "げ");

    conv.reset();
    "go".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ご");
}

#[test]
fn test_z_row_dakuten() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "za".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ざ");

    conv.reset();
    "ji".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "じ");

    conv.reset();
    "zu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ず");

    conv.reset();
    "ze".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぜ");

    conv.reset();
    "zo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぞ");
}

#[test]
fn test_d_row_dakuten() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "da".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "だ");

    conv.reset();
    "de".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "で");

    conv.reset();
    "do".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ど");
}

#[test]
fn test_b_row_dakuten() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "ba".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ば");

    conv.reset();
    "bi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "び");

    conv.reset();
    "bu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぶ");

    conv.reset();
    "be".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "べ");

    conv.reset();
    "bo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぼ");
}

#[test]
fn test_p_row_handakuten() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "pa".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぱ");

    conv.reset();
    "pi".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぴ");

    conv.reset();
    "pu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぷ");

    conv.reset();
    "pe".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぺ");

    conv.reset();
    "po".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぽ");
}

#[test]
fn test_youon_kya_series() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "kya".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "きゃ");

    conv.reset();
    "kyu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "きゅ");

    conv.reset();
    "kyo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "きょ");
}

#[test]
fn test_youon_sha_series() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "sha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "しゃ");

    conv.reset();
    "shu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "しゅ");

    conv.reset();
    "sho".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "しょ");
}

#[test]
fn test_youon_cha_series() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "cha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ちゃ");

    conv.reset();
    "chu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ちゅ");

    conv.reset();
    "cho".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ちょ");
}

#[test]
fn test_youon_nya_series() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "nya".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にゃ");

    conv.reset();
    "nyu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にゅ");

    conv.reset();
    "nyo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にょ");
}

#[test]
fn test_sokuon() {
    let mut conv = RomajiConverter::new();

    // kk -> っk
    conv.reset();
    "kko".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "っこ");

    // tt -> っt
    conv.reset();
    "tte".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "って");

    // pp -> っp
    conv.reset();
    "ppa".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "っぱ");
}

#[test]
fn test_n_variants() {
    let mut conv = RomajiConverter::new();

    // nn -> immediately converts to ん
    conv.reset();
    "nn".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ん"); // Immediately converted
    assert_eq!(conv.buffer(), ""); // Buffer is empty

    // n' -> ん
    conv.reset();
    "n'".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ん");
}

#[test]
fn test_small_characters() {
    let mut conv = RomajiConverter::new();

    conv.reset();
    "la".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぁ");

    conv.reset();
    "li".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぃ");

    conv.reset();
    "lu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぅ");

    conv.reset();
    "ltu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "っ");
}

#[test]
fn test_real_words() {
    let mut conv = RomajiConverter::new();

    // With IME-style nn rule: "konnichiha" -> こ + nn->ん + i->い + chiha->ちは = "こんいちは"
    // To get "こんにちは", use "konnnichiha" (3 n's)
    conv.reset();
    "konnichiha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "こんいちは");

    // Correct way to type "こんにちは" with IME-style nn rule
    conv.reset();
    "konnnichiha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "こんにちは");

    conv.reset();
    "arigatou".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ありがとう");

    conv.reset();
    "gakkou".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "がっこう");

    conv.reset();
    "nihongo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にほんご");

    conv.reset();
    "kitte".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "きって");

    // With IME-style nn rule: "annindouhu" -> あ + nn->ん + i->い + n before d->ん + douhu->どうふ
    conv.reset();
    "annindouhu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "あんいんどうふ");

    // To get "あんにんどうふ" (almond jelly), use "annninndouhu"
    // (nnn -> ん+n remaining, ni -> に, nn before d -> ん)
    conv.reset();
    "annninndouhu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "あんにんどうふ"); // "annin doufu" - almond jelly

    // "karukan" (single n at end) -> "かるかn" after flush
    conv.reset();
    "karukan".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "かるか");
    assert_eq!(conv.buffer(), "n"); // Trailing 'n' buffered (ambiguous)
    conv.flush();
    assert_eq!(conv.output(), "かるかn"); // Ambiguous 'n' outputs as-is

    // "karukann" (nn at end) -> "かるかん" immediately (nn converts right away)
    conv.reset();
    "karukann".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "かるかん"); // nn -> ん immediately
    assert_eq!(conv.buffer(), ""); // Buffer is empty

    // Test multiple input styles for same output
    conv.reset();
    "narezzi".chars().for_each(|c| {
        conv.push(c);
    });
    let result1 = conv.output().to_string();
    assert_eq!(result1, "なれっじ");

    conv.reset();
    "nareltuzi".chars().for_each(|c| {
        conv.push(c);
    });
    let result2 = conv.output().to_string();
    assert_eq!(result2, "なれっじ");

    // Both should produce the same output
    assert_eq!(result1, result2);
}

#[test]
fn test_nn_edge_cases() {
    let mut conv = RomajiConverter::new();

    // Test 1: Standalone "nn" should immediately convert to ん
    conv.reset();
    "nn".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ん", "nn should immediately convert to ん");
    assert_eq!(
        conv.buffer(),
        "",
        "buffer should be empty after nn conversion"
    );

    // Test 2: "nnn" - first "nn" -> ん immediately, then "n" buffered
    conv.reset();
    "nnn".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ん");
    assert_eq!(conv.buffer(), "n"); // Remaining "n"

    // Test 3: "nnnn" - first "nn" -> ん, then second "nn" -> ん
    conv.reset();
    "nnnn".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "んん");
    assert_eq!(conv.buffer(), "");

    // Test 4: "nni" should be "んい" (nn -> ん, i -> い)
    conv.reset();
    "nni".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "んい");
    assert_eq!(conv.buffer(), "");

    // Test 5: "nna" should be "んあ" (nn -> ん, a -> あ)
    conv.reset();
    "nna".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "んあ");

    // Test 6: "nnka" should be "んか" (nn is explicit ん, ka->か)
    conv.reset();
    "nnka".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "んか");

    // Test 7: "kannna" should be "かんな" (ka->か, nn->ん when followed by consonant n, na->な)
    conv.reset();
    "kannna".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "かんな");

    // Test 8: Word ending in "nn" - e.g., "karukann"
    // "nn" converts immediately to ん
    conv.reset();
    "karukann".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "かるかん"); // nn -> ん immediately
    assert_eq!(conv.buffer(), ""); // Buffer is empty

    // Test 9: ny* patterns should be yōon, not n + vowel
    conv.reset();
    "nya".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にゃ", "nya should be にゃ, not んや");

    conv.reset();
    "nyo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にょ", "nyo should be にょ, not んよ");

    conv.reset();
    "nyu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にゅ", "nyu should be にゅ, not んゆ");

    // Test 10: Complex patterns with nn (nn is ALWAYS ん in IME style)
    // With the new rule: "nn" is always ん, so "nnyo" = ん + yo = んよ
    conv.reset();
    "nnyo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(
        conv.output(),
        "んよ",
        "nnyo should be んよ (nn->ん, yo->よ)"
    );

    // To get こんにゃく, you need "konnnyaku" (3 n's: nn->ん when followed by consonant n, nya->にゃ)
    conv.reset();
    "konnnyaku".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(
        conv.output(),
        "こんにゃく",
        "konnnyaku should be こんにゃく"
    );

    // With the new rule: "annyo" = あ + nn->ん + yo->よ = あんよ
    conv.reset();
    "annyo".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(
        conv.output(),
        "あんよ",
        "annyo should be あんよ (a->あ, nn->ん, yo->よ)"
    );

    conv.reset();
    "konnnichiha".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(
        conv.output(),
        "こんにちは",
        "konnnichiha should be こんにちは (nn->ん when followed by consonant n, nichiha->にちは)"
    );
}

#[test]
fn test_sentences() {
    let mut conv = RomajiConverter::new();

    // watashihagennkidesu -> わたしはげんきです
    conv.reset();
    "watashihagennkidesu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "わたしはげんきです");

    // kyouhaiitennkidesu -> きょうはいいてんきです
    conv.reset();
    "kyouhaiitennkidesu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "きょうはいいてんきです");

    // toukyouhashibuyaku -> とうきょうはしぶやく
    conv.reset();
    "toukyouhashibuyaku".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "とうきょうはしぶやく");

    // nihonngowobenkyoushiteimasu -> にほんごをべんきょうしています
    conv.reset();
    "nihonngowobennkyoushiteimasu".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "にほんごをべんきょうしています");
}

#[test]
fn test_zenninn() {
    let mut conv = RomajiConverter::new();

    // zenninn -> ぜんいん
    // "nn" converts immediately to ん, no flush needed
    conv.reset();
    "zenninn".chars().for_each(|c| {
        conv.push(c);
    });
    assert_eq!(conv.output(), "ぜんいん"); // Converts immediately
    assert_eq!(conv.buffer(), ""); // Buffer is empty
}

#[test]
fn test_zenninn_crash() {
    let mut conv = RomajiConverter::new();
    let input = "zenninn";
    for ch in input.chars() {
        conv.push(ch);
    }
    println!("Output: {}, Buffer: {}", conv.output(), conv.buffer());
}

#[test]
fn test_zenninn_kanji_conversion() {
    use karukan_engine::{Backend, KanaKanjiConverter};
    let mut conv = RomajiConverter::new();
    let input = "zenninn";
    for ch in input.chars() {
        conv.push(ch);
    }
    let hiragana = conv.output();
    println!("Hiragana: {}", hiragana);

    // Now try kanji conversion
    let backend = Backend::from_variant_id("jinen-v1-small-q5").expect("Failed to load backend");
    let kanji_conv = KanaKanjiConverter::new(backend).expect("Failed to create converter");
    let result = kanji_conv.convert(hiragana, "", 1);
    println!("Kanji result: {:?}", result);
}
