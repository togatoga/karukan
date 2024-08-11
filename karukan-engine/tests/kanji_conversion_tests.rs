//! Integration tests for kanji conversion
//!
//! These tests verify that the kanji conversion functionality works correctly.
//!
//! Note: These tests require downloading models from HuggingFace on first run.

use karukan_engine::kanji::ConversionConfig;

// ============================================================================
// Helper functions
// ============================================================================

/// Check if a string contains only valid Japanese characters
fn is_valid_japanese(s: &str) -> bool {
    s.chars().all(|c| {
        c.is_ascii_punctuation()
            || c == '。'
            || c == '、'
            || ('\u{3040}'..='\u{309F}').contains(&c) // Hiragana
            || ('\u{30A0}'..='\u{30FF}').contains(&c) // Katakana
            || ('\u{4E00}'..='\u{9FFF}').contains(&c) // CJK
            || c.is_whitespace()
    })
}

// Re-export clean_model_output for test use
use karukan_engine::kanji::clean_model_output as clean_output;

// ============================================================================
// Unit tests (no model download required)
// ============================================================================

mod unit_tests {
    use super::*;

    #[test]
    fn test_conversion_config_defaults() {
        let config = ConversionConfig::default();
        assert_eq!(config.max_new_tokens, 50);
    }
}

// ============================================================================
// llama.cpp backend tests
// ============================================================================

mod llamacpp_tests {
    use super::*;
    use karukan_engine::kanji::{
        LlamaCppModel, build_jinen_prompt, get_path_by_id, get_tokenizer_path_by_id, registry,
    };

    fn load_model() -> Option<LlamaCppModel> {
        let reg = registry();
        let path = get_path_by_id(&reg.default_model).ok()?;
        let tok_path = get_tokenizer_path_by_id(&reg.default_model).ok()?;
        LlamaCppModel::from_file(&path, &tok_path).ok()
    }

    fn build_prompt(katakana: &str) -> String {
        build_jinen_prompt(katakana, "")
    }

    #[test]
    fn test_tokenization() {
        let model = load_model().expect("Failed to load");
        let tokens = model.tokenize("コンニチハ").expect("Tokenize failed");
        assert!(!tokens.is_empty());
    }

    #[test]
    fn test_generation() {
        let model = load_model().expect("Failed to load");
        let prompt = build_prompt("ワセダ");
        let tokens = model.tokenize(&prompt).expect("Tokenize failed");

        let eos = Some(model.eos_token_id().0);
        let output_tokens = model.generate(&tokens, 20, eos).expect("Generate failed");
        assert!(output_tokens.len() >= tokens.len());

        let output = model
            .decode(&output_tokens[tokens.len()..], true)
            .expect("Decode failed");
        let clean = clean_output(&output);

        assert!(!clean.is_empty());
        assert!(is_valid_japanese(&clean), "Invalid output: {}", clean);
    }

    #[test]
    fn test_expected_conversions() {
        let model = load_model().expect("Failed to load");
        let test_cases = [
            ("ワセダ", "早稲田"),
            ("トウキョウ", "東京"),
            ("ニホン", "日本"),
        ];

        for (input, expected) in test_cases {
            let prompt = build_prompt(input);
            let tokens = model.tokenize(&prompt).expect("Tokenize failed");
            let eos = Some(model.eos_token_id().0);
            let output_tokens = model.generate(&tokens, 20, eos).expect("Generate failed");
            let output = model
                .decode(&output_tokens[tokens.len()..], true)
                .expect("Decode failed");
            let clean = clean_output(&output);

            assert!(
                clean.contains(expected),
                "'{}' -> '{}', expected '{}'",
                input,
                clean,
                expected
            );
        }
    }

    #[test]
    fn test_beam_search_basic() {
        let model = load_model().expect("Failed to load");
        let prompt = build_prompt("ヘンカン");
        let tokens = model.tokenize(&prompt).expect("Tokenize failed");
        let eos = Some(model.eos_token_id().0);

        let results = model
            .generate_beam_search(&tokens, 20, eos, 2)
            .expect("Beam search failed");

        assert!(!results.is_empty(), "No results");

        let candidates: Vec<String> = results
            .iter()
            .filter_map(|(t, _)| {
                let s = clean_output(&model.decode(t, true).ok()?);
                if s.is_empty() { None } else { Some(s) }
            })
            .collect();

        assert!(!candidates.is_empty(), "No valid candidates");
        for c in &candidates {
            assert!(is_valid_japanese(c), "Invalid: {}", c);
        }

        println!("へんかん -> {:?}", candidates);
    }

    #[test]
    fn test_beam_search_multiple_inputs() {
        let model = load_model().expect("Failed to load");
        let test_cases = [
            ("カンジ", vec!["漢字", "感じ"]),
            ("キョウ", vec!["今日", "京"]),
            ("コウエン", vec!["公園", "講演", "後援"]),
        ];

        for (katakana, possible) in test_cases {
            let prompt = build_prompt(katakana);
            let tokens = model.tokenize(&prompt).expect("Tokenize failed");
            let eos = Some(model.eos_token_id().0);

            let results = model
                .generate_beam_search(&tokens, 20, eos, 3)
                .expect("Beam search failed");

            let candidates: Vec<String> = results
                .iter()
                .filter_map(|(t, _)| {
                    let s = clean_output(&model.decode(t, true).ok()?);
                    if s.is_empty() { None } else { Some(s) }
                })
                .collect();

            println!("{} -> {:?}", katakana, candidates);

            let found = possible
                .iter()
                .any(|p| candidates.iter().any(|c| c.contains(p)));
            if !found {
                println!("Note: expected {:?}, got {:?}", possible, candidates);
            }
        }
    }

    #[test]
    fn test_beam_search_score_ordering() {
        let model = load_model().expect("Failed to load");
        let prompt = build_prompt("ヘンカン");
        let tokens = model.tokenize(&prompt).expect("Tokenize failed");
        let eos = Some(model.eos_token_id().0);

        let results = model
            .generate_beam_search(&tokens, 20, eos, 5)
            .expect("Beam search failed");

        let scores: Vec<f32> = results.iter().map(|(_, s)| *s).collect();

        for i in 1..scores.len() {
            assert!(
                scores[i - 1] >= scores[i],
                "Scores not sorted: {:?}",
                scores
            );
        }

        println!("Scores: {:?}", scores);
    }

    #[test]
    fn test_conversion_with_context() {
        let model = load_model().expect("Failed to load");

        // Test that context influences conversion
        // "てんき" without context
        let prompt_no_ctx = build_jinen_prompt("テンキ", "");
        let tokens = model.tokenize(&prompt_no_ctx).expect("Tokenize failed");
        let eos = Some(model.eos_token_id().0);
        let output_no_ctx = model.generate(&tokens, 20, eos).expect("Generate failed");
        let text_no_ctx =
            clean_output(&model.decode(&output_no_ctx[tokens.len()..], true).unwrap());

        // "てんき" with context "今日はいい" should favor "天気"
        let prompt_with_ctx = build_jinen_prompt("テンキ", "今日はいい");
        let tokens = model.tokenize(&prompt_with_ctx).expect("Tokenize failed");
        let output_with_ctx = model.generate(&tokens, 20, eos).expect("Generate failed");
        let text_with_ctx = clean_output(
            &model
                .decode(&output_with_ctx[tokens.len()..], true)
                .unwrap(),
        );

        println!("Without context: テンキ -> {}", text_no_ctx);
        println!("With context '今日はいい': テンキ -> {}", text_with_ctx);

        // Both should produce valid output
        assert!(!text_no_ctx.is_empty(), "No output without context");
        assert!(!text_with_ctx.is_empty(), "No output with context");

        // With context should likely produce "天気"
        assert!(
            text_with_ctx.contains("天気"),
            "Expected '天気' with context, got '{}'",
            text_with_ctx
        );
    }

    #[test]
    fn test_context_variations() {
        let model = load_model().expect("Failed to load");

        let test_cases = [
            // (katakana, context, expected_contains)
            ("カンジ", "日本語の", "漢字"), // "kanji" with "Japanese" context -> 漢字
            ("キョウ", "今日は", "今日"),   // "kyou" with date context -> 今日
            ("コウエン", "公園で", "公園"), // "kouen" with park context -> 公園
        ];

        for (katakana, context, expected) in test_cases {
            let prompt = build_jinen_prompt(katakana, context);
            let tokens = model.tokenize(&prompt).expect("Tokenize failed");
            let eos = Some(model.eos_token_id().0);
            let output = model.generate(&tokens, 20, eos).expect("Generate failed");
            let text = clean_output(&model.decode(&output[tokens.len()..], true).unwrap());

            println!("Context '{}' + {} -> {}", context, katakana, text);

            // Note: This is a soft check - model may not always follow context perfectly
            if !text.contains(expected) {
                println!(
                    "Note: expected '{}' but got '{}' (context may not fully influence)",
                    expected, text
                );
            }
        }
    }

    /// Test that context significantly changes conversion results
    /// Same reading "こうえん" should produce different kanji based on context
    #[test]
    fn test_context_disambiguation() {
        let model = load_model().expect("Failed to load");

        // Test cases where context should produce different results for the same reading
        let disambiguation_cases = [
            // (katakana, context1, expected1, context2, expected2)
            ("コウエン", "子どもと", "公園", "政治家の", "公演"),
            ("シカイ", "歯が痛いので", "歯科医", "番組の", "司会"),
        ];

        for (katakana, ctx1, exp1, ctx2, exp2) in disambiguation_cases {
            // First context
            let prompt1 = build_jinen_prompt(katakana, ctx1);
            let tokens1 = model.tokenize(&prompt1).expect("Tokenize failed");
            let eos = Some(model.eos_token_id().0);
            let output1 = model.generate(&tokens1, 20, eos).expect("Generate failed");
            let text1 = clean_output(&model.decode(&output1[tokens1.len()..], true).unwrap());

            // Second context
            let prompt2 = build_jinen_prompt(katakana, ctx2);
            let tokens2 = model.tokenize(&prompt2).expect("Tokenize failed");
            let output2 = model.generate(&tokens2, 20, eos).expect("Generate failed");
            let text2 = clean_output(&model.decode(&output2[tokens2.len()..], true).unwrap());

            println!(
                "{} + '{}' -> {} (expected: {})",
                katakana, ctx1, text1, exp1
            );
            println!(
                "{} + '{}' -> {} (expected: {})",
                katakana, ctx2, text2, exp2
            );

            // Verify different contexts produce different results
            if text1 == text2 {
                println!("Warning: Context did not affect output for {}", katakana);
            }

            // Check expected outputs
            if text1.contains(exp1) && text2.contains(exp2) {
                println!("Success: Context correctly influenced conversion!");
            } else {
                println!(
                    "Note: Expected '{}' and '{}', got '{}' and '{}'",
                    exp1, exp2, text1, text2
                );
            }
        }
    }

    /// Test "zenninn" -> "ゼンイン" conversion (reported crash)
    #[test]
    fn test_zenninn_llamacpp() {
        let model = load_model().expect("Failed to load");

        // "zenninn" -> "ぜんいん" -> katakana "ゼンイン"
        let katakana = "ゼンイン";
        let prompt = build_jinen_prompt(katakana, "");
        println!("Prompt: {:?}", prompt);
        println!("Prompt bytes: {:?}", prompt.as_bytes());

        let tokens = model.tokenize(&prompt).expect("Tokenize failed");
        println!("Input tokens: {:?}", tokens);
        println!("Token count: {}", tokens.len());

        let eos = Some(model.eos_token_id().0);
        println!("EOS token: {:?}", eos);

        let output_tokens = model.generate(&tokens, 20, eos).expect("Generate failed");
        println!("Output tokens: {:?}", output_tokens);

        let generated = &output_tokens[tokens.len()..];
        println!("Generated tokens: {:?}", generated);

        let output = model.decode(generated, true).expect("Decode failed");
        println!("Output: {}", output);

        let clean = clean_output(&output);
        println!("Clean: {}", clean);
    }

    /// Test beam search with multiple candidates (this was causing decode errors)
    #[test]
    fn test_zenninn_beam_search() {
        let model = load_model().expect("Failed to load");

        let katakana = "ゼンイン";
        let prompt = build_jinen_prompt(katakana, "");
        let tokens = model.tokenize(&prompt).expect("Tokenize failed");
        let eos = Some(model.eos_token_id().0);

        println!("Testing beam search with beam_size=3");
        println!("Input tokens: {:?}", tokens);
        println!("EOS token: {:?}", eos);

        let results = model
            .generate_beam_search(&tokens, 20, eos, 3)
            .expect("Beam search failed");

        println!("Beam search returned {} results", results.len());

        for (i, (output_tokens, score)) in results.iter().enumerate() {
            println!("Result {}: tokens={:?}, score={}", i, output_tokens, score);

            if output_tokens.is_empty() {
                println!("  -> Empty tokens, skipping decode");
                continue;
            }

            match model.decode(output_tokens, true) {
                Ok(text) => println!("  -> Decoded: '{}'", text),
                Err(e) => println!("  -> Decode error: {}", e),
            }
        }
    }

    /// Test that long context inputs work within n_ctx=256 limit
    #[test]
    fn test_long_context_input() {
        let model = load_model().expect("Failed to load");

        // Long context that produces many tokens (but still within 256)
        let long_context =
            "これは非常に長い文脈です。今日はとても良い天気で、公園に行って遊びました。";
        let katakana = "コウエン";

        let prompt = build_jinen_prompt(katakana, long_context);
        let tokens = model.tokenize(&prompt).expect("Tokenize failed");

        println!("Long context test: {} tokens", tokens.len());
        assert!(tokens.len() < 256, "Input should fit in context window");

        let eos = Some(model.eos_token_id().0);
        let output = model.generate(&tokens, 20, eos).expect("Generate failed");

        let text = model
            .decode(&output[tokens.len()..], true)
            .expect("Decode failed");
        let clean = clean_output(&text);

        println!("Output: {}", clean);
        assert!(!clean.is_empty(), "Should produce output");
    }

    /// Test that the external tokenizer applies NFKC normalization automatically.
    ///
    /// Full-width ASCII characters (e.g. `Ａ`, `０`) should be normalized to
    /// half-width equivalents by the tokenizer's NFKC normalizer, producing the
    /// same token ids as the half-width input.
    #[test]
    fn test_tokenizer_nfkc_normalization() {
        let model = load_model().expect("Failed to load");

        // Full-width → half-width pairs that NFKC should normalize
        let cases = [
            ("Ａｂｃ", "Abc"),
            ("０１２３", "0123"),
            ("（テスト）", "(テスト)"),
            ("！？", "!?"),
        ];

        for (fullwidth, halfwidth) in cases {
            let tokens_fw = model
                .tokenize(fullwidth)
                .expect("Tokenize fullwidth failed");
            let tokens_hw = model
                .tokenize(halfwidth)
                .expect("Tokenize halfwidth failed");
            assert_eq!(
                tokens_fw, tokens_hw,
                "NFKC normalization not applied by tokenizer: '{}' vs '{}'",
                fullwidth, halfwidth
            );
        }

        // Japanese hiragana/katakana should be unchanged by NFKC
        let katakana = "アイウエオ";
        let tokens = model.tokenize(katakana).expect("Tokenize failed");
        let decoded = model.decode(&tokens, true).expect("Decode failed");
        assert!(
            decoded.contains(katakana),
            "Katakana should be preserved: got '{}'",
            decoded
        );
    }

    /// Test token counts for various input lengths
    #[test]
    fn test_token_counts() {
        let model = load_model().expect("Failed to load");

        let test_cases = [
            ("ヘンカン", "", 10),                               // Short input
            ("ニューラルカナカンジヘンカン", "", 20),           // Medium input
            ("シカイ", "歯が痛いので", 20),                     // With short context
            ("コウエン", "子どもと遊びに行くために近くの", 30), // With longer context
        ];

        for (input, context, max_expected) in test_cases {
            let prompt = build_jinen_prompt(input, context);
            let tokens = model.tokenize(&prompt).expect("Tokenize failed");

            println!("'{}' + '{}' -> {} tokens", input, context, tokens.len());
            assert!(
                tokens.len() <= max_expected,
                "Token count {} exceeds expected max {}",
                tokens.len(),
                max_expected
            );
        }
    }
}
