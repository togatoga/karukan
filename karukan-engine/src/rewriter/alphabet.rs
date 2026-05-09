//! Alphabet width/case rewriter — for ASCII or full-width alphabetic input,
//! emit the four canonical forms: half-width lower/upper, full-width lower/upper.
//!
//! Examples:
//! - `abc` → `ABC`, `ａｂｃ`, `ＡＢＣ`
//! - `ABC` → `abc`, `ａｂｃ`, `ＡＢＣ`
//! - `Hello` → `hello`, `HELLO`, `ｈｅｌｌｏ`, `ＨＥＬＬＯ`, `Ｈｅｌｌｏ` (… minus self)
//!
//! Each variant carries a description tagged with mozc's width markers
//! (`[半]` / `[全]`) plus 大文字/小文字, e.g. `ＡＢＣ` → `[全]英大文字`.

use crate::kana::{ascii_to_fullwidth_char, fullwidth_to_ascii_char};

use super::{RewriteOutput, Rewriter};

/// Rewriter that produces width/case variants for alphabetic input.
pub struct AlphabetRewriter;

fn is_alpha_char(c: char) -> bool {
    c.is_ascii_alphabetic()
        || ('\u{FF21}'..='\u{FF3A}').contains(&c)  // Ａ-Ｚ
        || ('\u{FF41}'..='\u{FF5A}').contains(&c) // ａ-ｚ
}

fn is_pure_alpha(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_alpha_char)
}

#[derive(Clone, Copy)]
enum Width {
    Half,
    Full,
}

#[derive(Clone, Copy)]
enum Case {
    Lower,
    Upper,
}

/// Build one variant + its mozc-style description.
fn build_variant(original: &str, width: Width, case: Case) -> (String, &'static str) {
    let half: String = original.chars().map(fullwidth_to_ascii_char).collect();
    let cased = match case {
        Case::Lower => half.to_ascii_lowercase(),
        Case::Upper => half.to_ascii_uppercase(),
    };
    let text = match width {
        Width::Half => cased,
        Width::Full => cased.chars().map(ascii_to_fullwidth_char).collect(),
    };
    let desc = match (width, case) {
        (Width::Half, Case::Lower) => "[半]英小文字",
        (Width::Half, Case::Upper) => "[半]英大文字",
        (Width::Full, Case::Lower) => "[全]英小文字",
        (Width::Full, Case::Upper) => "[全]英大文字",
    };
    (text, desc)
}

impl Rewriter for AlphabetRewriter {
    fn name(&self) -> &'static str {
        "alphabet"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        if !is_pure_alpha(candidate) {
            return Vec::new();
        }

        let mut out: Vec<RewriteOutput> = Vec::new();
        for &(width, case) in &[
            (Width::Half, Case::Lower),
            (Width::Half, Case::Upper),
            (Width::Full, Case::Lower),
            (Width::Full, Case::Upper),
        ] {
            let (text, desc) = build_variant(candidate, width, case);
            if text != candidate && !out.iter().any(|(t, _)| t == &text) {
                out.push((text, Some(desc.to_string())));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    #[test]
    fn empty_returns_empty() {
        assert!(AlphabetRewriter.rewrite("").is_empty());
    }

    #[test]
    fn non_alpha_returns_empty() {
        assert!(AlphabetRewriter.rewrite("123").is_empty());
        assert!(AlphabetRewriter.rewrite("abc1").is_empty()); // mixed digits
        assert!(AlphabetRewriter.rewrite("hello world").is_empty()); // space
        assert!(AlphabetRewriter.rewrite("あ").is_empty());
        assert!(AlphabetRewriter.rewrite("競技").is_empty());
    }

    #[test]
    fn lowercase_emits_three_variants() {
        let out = AlphabetRewriter.rewrite("abc");
        let t = texts(&out);
        assert!(t.contains(&"ABC".to_string()));
        assert!(t.contains(&"ａｂｃ".to_string()));
        assert!(t.contains(&"ＡＢＣ".to_string()));
        assert!(!t.iter().any(|s| s == "abc")); // self excluded
    }

    #[test]
    fn uppercase_emits_three_variants() {
        let out = AlphabetRewriter.rewrite("ABC");
        let t = texts(&out);
        assert!(t.contains(&"abc".to_string()));
        assert!(t.contains(&"ａｂｃ".to_string()));
        assert!(t.contains(&"ＡＢＣ".to_string()));
        assert!(!t.iter().any(|s| s == "ABC"));
    }

    #[test]
    fn fullwidth_lowercase_emits_three_variants() {
        let out = AlphabetRewriter.rewrite("ａｂｃ");
        let t = texts(&out);
        assert!(t.contains(&"abc".to_string()));
        assert!(t.contains(&"ABC".to_string()));
        assert!(t.contains(&"ＡＢＣ".to_string()));
        assert!(!t.iter().any(|s| s == "ａｂｃ"));
    }

    #[test]
    fn mixed_case_emits_all_four_canonical_forms() {
        // `AbC` itself isn't one of the four canonical forms, so all four
        // appear as variants.
        let t = texts(&AlphabetRewriter.rewrite("AbC"));
        assert!(t.contains(&"abc".to_string()));
        assert!(t.contains(&"ABC".to_string()));
        assert!(t.contains(&"ａｂｃ".to_string()));
        assert!(t.contains(&"ＡＢＣ".to_string()));
    }

    #[test]
    fn descriptions_match_width_and_case() {
        let out = AlphabetRewriter.rewrite("abc");
        assert_eq!(desc(&out, "ABC"), Some("[半]英大文字".to_string()));
        assert_eq!(desc(&out, "ａｂｃ"), Some("[全]英小文字".to_string()));
        assert_eq!(desc(&out, "ＡＢＣ"), Some("[全]英大文字".to_string()));
    }

    #[test]
    fn single_letter_works() {
        let t = texts(&AlphabetRewriter.rewrite("a"));
        assert!(t.contains(&"A".to_string()));
        assert!(t.contains(&"ａ".to_string()));
        assert!(t.contains(&"Ａ".to_string()));
    }
}
