//! Katakana form rewriter — produces full-width and half-width katakana variants.
//!
//! Only applies to candidates that consist entirely of hiragana or full-width
//! katakana (i.e. the reading itself / pure-kana fallbacks). Model output
//! candidates that mix kanji with kana are NOT rewritten — converting
//! `愛してる` into `愛ｼﾃﾙ` would be nonsense.
//!
//! For a pure-kana candidate, this rewriter emits:
//! - The full-width katakana form (only if the candidate has hiragana)
//! - The half-width katakana form (only if it differs from the candidate)
//!
//! Variants identical to the original or to each other are not emitted.
//!
//! Each emitted variant carries a mozc-style width annotation
//! (`[全]カタカナ` / `[半]カタカナ`) so the candidate window can label them.

use crate::kana::{hiragana_to_katakana, katakana_to_half_width};

use super::{RewriteOutput, Rewriter};

/// Annotation shown on full-width katakana variants.
const FULL_KATAKANA_DESC: &str = "[全]カタカナ";

/// Annotation shown on half-width katakana variants.
const HALF_KATAKANA_DESC: &str = "[半]カタカナ";

/// Rewriter that produces full-width and half-width katakana variants.
pub struct HalfWidthKatakanaRewriter;

fn contains_hiragana(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{3041}'..='\u{3096}'))
}

/// True if every character is in the hiragana or katakana block (including
/// the prolonged sound mark, sokuon, dakuten/handakuten, and small kana).
fn is_pure_kana(text: &str) -> bool {
    text.chars()
        .all(|c| matches!(c, '\u{3041}'..='\u{3096}' | '\u{30A0}'..='\u{30FF}'))
}

impl Rewriter for HalfWidthKatakanaRewriter {
    fn name(&self) -> &'static str {
        "katakana_form"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        if candidate.is_empty() || !is_pure_kana(candidate) {
            return Vec::new();
        }

        let mut out: Vec<RewriteOutput> = Vec::new();

        // Full-width katakana (only if candidate contains hiragana)
        let full_kata = if contains_hiragana(candidate) {
            let v = hiragana_to_katakana(candidate);
            if v != candidate {
                out.push((v.clone(), Some(FULL_KATAKANA_DESC.to_string())));
                v
            } else {
                candidate.to_string()
            }
        } else {
            candidate.to_string()
        };

        // Half-width katakana
        let half = katakana_to_half_width(&full_kata);
        if half != candidate && !out.iter().any(|(s, _)| s == &half) {
            out.push((half, Some(HALF_KATAKANA_DESC.to_string())));
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    #[test]
    fn empty_input_returns_empty() {
        let r = HalfWidthKatakanaRewriter;
        assert!(r.rewrite("").is_empty());
    }

    #[test]
    fn pure_kanji_returns_empty() {
        let r = HalfWidthKatakanaRewriter;
        assert!(r.rewrite("競技").is_empty());
    }

    #[test]
    fn pure_ascii_returns_empty() {
        let r = HalfWidthKatakanaRewriter;
        assert!(r.rewrite("abc").is_empty());
    }

    #[test]
    fn hiragana_emits_full_and_half() {
        let r = HalfWidthKatakanaRewriter;
        assert_eq!(
            texts(&r.rewrite("あいう")),
            vec!["アイウ".to_string(), "ｱｲｳ".to_string()]
        );
    }

    #[test]
    fn full_katakana_emits_half_only() {
        let r = HalfWidthKatakanaRewriter;
        assert_eq!(texts(&r.rewrite("アイウ")), vec!["ｱｲｳ".to_string()]);
    }

    #[test]
    fn voiced_dakuten_expands() {
        let r = HalfWidthKatakanaRewriter;
        assert_eq!(
            texts(&r.rewrite("がっこう")),
            vec!["ガッコウ".to_string(), "ｶﾞｯｺｳ".to_string()]
        );
    }

    #[test]
    fn mixed_kanji_kana_returns_empty() {
        let r = HalfWidthKatakanaRewriter;
        // Mixed kanji + kana inputs (typical model output) must NOT be rewritten:
        // turning `愛してる` into `愛ｼﾃﾙ` is nonsense.
        assert!(r.rewrite("競技プログラミング").is_empty());
        assert!(r.rewrite("愛してる").is_empty());
        assert!(r.rewrite("漢字あ").is_empty());
    }

    #[test]
    fn does_not_emit_self() {
        let r = HalfWidthKatakanaRewriter;
        let out = r.rewrite("ｱｲｳ");
        assert!(!out.iter().any(|(s, _)| s == "ｱｲｳ"));
    }

    #[test]
    fn descriptions_match_width_form() {
        // Mozc-style width annotations: full-width katakana → `[全]カタカナ`,
        // half-width katakana → `[半]カタカナ`.
        let r = HalfWidthKatakanaRewriter;
        let out = r.rewrite("あいう");
        assert_eq!(desc(&out, "アイウ"), Some(FULL_KATAKANA_DESC.to_string()));
        assert_eq!(desc(&out, "ｱｲｳ"), Some(HALF_KATAKANA_DESC.to_string()));

        let out = r.rewrite("アイウ");
        assert_eq!(desc(&out, "ｱｲｳ"), Some(HALF_KATAKANA_DESC.to_string()));
    }
}
