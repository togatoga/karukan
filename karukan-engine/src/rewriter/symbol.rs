//! Symbol rewriter — given a typed symbol or hiragana reading, emit related
//! symbol candidates.
//!
//! [`SymbolRewriter`] handles two complementary jobs:
//!
//! 1. **Variant chain** — a typed symbol expands to related symbols
//!    (e.g. `「` → `『`, `【`, `（`, ...).
//! 2. **Reading lookup** — a hiragana reading expands to matching symbols
//!    (e.g. `かぎかっこ` → `「」`, `『』`), parity with mozc's symbol_rewriter.
//!
//! All data lives in `karukan-engine/data/symbols.yml` (loaded once via
//! `LazyLock`) under three sections:
//!
//! - `descriptions:` — hand-curated overrides for the symbol → description
//!   table. Only entries that are *not* already present (with the same
//!   description) in `entries:` need to live here; today this is just the
//!   single-bracket forms (`「`, `」`, `『`, `』`, ...) which mozc keeps
//!   under non-kana readings only.
//! - `variants:` — hand-curated variant chains driving job (1).
//! - `entries:` — auto-generated from `mozc/src/data/symbol/symbol.tsv` by
//!   `scripts/symbols_porter.py`, driving job (2). Re-running the porter
//!   overwrites only this section. Includes ASCII readings (e.g. `<` →
//!   `〈`/`＜`/`≦`/`←`) for mozc parity.
//!
//! At load time the per-entry `description` from `entries:` is folded into
//! the table behind [`description`], with the curated `descriptions:`
//! section overriding when both define a value for the same symbol.
//!
//! ```yaml
//! descriptions:
//!   。: 句点
//!   …: 三点リーダ
//! variants:
//!   - key: 「
//!     chain: [『, 【, 〔, （, ...]
//! entries:
//!   - char: "「」"
//!     readings: [かっこ, かぎかっこ]
//!     description: かぎ括弧
//! ```

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use crate::kana::{ascii_to_fullwidth_char, fullwidth_to_ascii_char};

use super::{RewriteOutput, Rewriter};

const SYMBOLS_YAML: &str = include_str!("../../data/symbols.yml");

#[derive(Deserialize)]
struct VariantEntry {
    key: String,
    chain: Vec<String>,
}

#[derive(Deserialize)]
struct SymbolEntry {
    char: String,
    readings: Vec<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct SymbolFile {
    descriptions: HashMap<String, String>,
    variants: Vec<VariantEntry>,
    entries: Vec<SymbolEntry>,
}

/// One symbol candidate for a given reading. Internal: the rewriter is the
/// only consumer.
#[derive(Debug, Clone)]
struct SymbolCandidate {
    /// The symbol text (e.g. `「」`).
    char: String,
    /// Description from mozc (e.g. `かぎ括弧`), if any.
    description: Option<String>,
}

struct SymbolTable {
    descriptions: HashMap<String, String>,
    variants: HashMap<String, Vec<String>>,
    /// Reverse index: reading → symbol candidates, in source-file order.
    by_reading: HashMap<String, Vec<SymbolCandidate>>,
}

static SYMBOL_TABLE: LazyLock<SymbolTable> = LazyLock::new(|| {
    let file: SymbolFile =
        serde_yaml::from_str(SYMBOLS_YAML).expect("symbols.yml must be valid YAML");
    let variants = file
        .variants
        .into_iter()
        .map(|e| (e.key, e.chain))
        .collect();

    // Build the char → description table: seed from `entries` (mozc-derived),
    // then let `descriptions` (hand-curated) override. The curated section
    // therefore only needs entries that aren't already in `entries`, or that
    // need a different label than mozc provides (e.g. single-bracket forms
    // like `「` → "始めかぎ括弧" that mozc keeps under non-kana readings and
    // are filtered out of our `entries` section).
    let mut descriptions: HashMap<String, String> = HashMap::new();
    let mut by_reading: HashMap<String, Vec<SymbolCandidate>> = HashMap::new();
    for entry in file.entries {
        if let Some(desc) = &entry.description {
            descriptions
                .entry(entry.char.clone())
                .or_insert_with(|| desc.clone());
        }
        for reading in &entry.readings {
            let bucket = by_reading.entry(reading.clone()).or_default();
            // Dedupe by char within a reading bucket — multiple mozc rows can
            // map the same reading/char pair via different POS values.
            if !bucket.iter().any(|c| c.char == entry.char) {
                bucket.push(SymbolCandidate {
                    char: entry.char.clone(),
                    description: entry.description.clone(),
                });
            }
        }
    }
    descriptions.extend(file.descriptions);

    SymbolTable {
        descriptions,
        variants,
        by_reading,
    }
});

/// Look up the Japanese description for a symbol (e.g. `。` → "句点").
/// Returns `None` if the text isn't a known symbol in the table.
pub fn description(text: &str) -> Option<&'static str> {
    SYMBOL_TABLE.descriptions.get(text).map(|s| s.as_str())
}

/// Rewriter that returns related symbols for a typed symbol or hiragana
/// reading. See the module docstring for the two lookup paths.
#[derive(Default)]
pub struct SymbolRewriter;

impl SymbolRewriter {
    pub fn new() -> Self {
        Self
    }
}

/// True if every character is an ASCII digit or full-width digit (and the
/// string is non-empty). Used to gate digit-only width-pair generation so
/// arbitrary candidates with stray digits aren't expanded.
fn is_pure_digit(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(&c))
}

/// For a digit-only string, return the all-full-width and all-half-width forms
/// that differ from the input. Supports arbitrary length (e.g. `123` → `１２３`,
/// `１２` → `12`).
fn digit_width_variants(candidate: &str) -> Vec<String> {
    if !is_pure_digit(candidate) {
        return Vec::new();
    }
    let full: String = candidate.chars().map(ascii_to_fullwidth_char).collect();
    let half: String = candidate.chars().map(fullwidth_to_ascii_char).collect();
    let mut out = Vec::new();
    if full != candidate {
        out.push(full);
    }
    if half != candidate && !out.iter().any(|s| s == &half) {
        out.push(half);
    }
    out
}

impl Rewriter for SymbolRewriter {
    fn name(&self) -> &'static str {
        "symbol"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        if candidate.is_empty() {
            return Vec::new();
        }
        let mut out: Vec<RewriteOutput> = Vec::new();
        let push_unique = |s: String, desc: Option<String>, out: &mut Vec<RewriteOutput>| {
            if s != candidate && !out.iter().any(|(t, _)| t == &s) {
                // Prefer the description that was supplied (e.g. mozc's
                // per-row description for a reading→symbol entry); fall
                // back to the global symbol description table for variant
                // chains that don't carry their own.
                let final_desc = desc.or_else(|| SYMBOL_TABLE.descriptions.get(&s).cloned());
                out.push((s, final_desc));
            }
        };

        // Variant chain: typed symbol → related symbols
        // (e.g. `「` → `『`, `【`, `（`, ...).
        if let Some(chain) = SYMBOL_TABLE.variants.get(candidate) {
            for v in chain {
                push_unique(v.clone(), None, &mut out);
            }
        }

        // Reading lookup: hiragana reading → symbol candidates from mozc's
        // symbol.tsv (e.g. `かぎかっこ` → `「」`, `『』`, ...).
        if let Some(syms) = SYMBOL_TABLE.by_reading.get(candidate) {
            for sym in syms {
                push_unique(sym.char.clone(), sym.description.clone(), &mut out);
            }
        }

        for v in digit_width_variants(candidate) {
            push_unique(v, None, &mut out);
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    #[test]
    fn open_kagi_returns_other_open_brackets() {
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("「"));
        assert!(out.contains(&"『".to_string()));
        assert!(out.contains(&"【".to_string()));
        assert!(out.contains(&"（".to_string()));
        assert!(out.contains(&"〔".to_string()));
    }

    #[test]
    fn close_kagi_returns_other_close_brackets() {
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("」"));
        assert!(out.contains(&"』".to_string()));
        assert!(out.contains(&"】".to_string()));
        assert!(out.contains(&"）".to_string()));
    }

    #[test]
    fn comma_returns_variants() {
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("、"));
        assert!(out.contains(&"，".to_string()));
        assert!(out.contains(&",".to_string()));
    }

    #[test]
    fn unknown_returns_empty() {
        // Arbitrary words / empty input have neither variant chains nor
        // reading entries in the symbol table.
        let r = SymbolRewriter::new();
        assert!(r.rewrite("競技").is_empty());
        assert!(r.rewrite("").is_empty());
        // (Single-kana readings like `あ` *do* hit reading entries from
        // mozc's symbol.tsv — see kakko/kagikakko tests below — so they're
        // intentionally not asserted to be empty here.)
    }

    #[test]
    fn no_self_in_variants() {
        let r = SymbolRewriter::new();
        for key in SYMBOL_TABLE.variants.keys() {
            let out = texts(&r.rewrite(key));
            assert!(
                !out.iter().any(|s| s == key),
                "{} should not include self",
                key
            );
        }
    }

    #[test]
    fn single_digit_emits_width_pair() {
        let r = SymbolRewriter::new();
        assert!(texts(&r.rewrite("1")).contains(&"１".to_string()));
        assert!(texts(&r.rewrite("１")).contains(&"1".to_string()));
        assert!(texts(&r.rewrite("0")).contains(&"０".to_string()));
        assert!(texts(&r.rewrite("9")).contains(&"９".to_string()));
    }

    #[test]
    fn multi_digit_emits_width_pair() {
        let r = SymbolRewriter::new();
        assert!(texts(&r.rewrite("123")).contains(&"１２３".to_string()));
        assert!(texts(&r.rewrite("１２３")).contains(&"123".to_string()));
        assert!(texts(&r.rewrite("2026")).contains(&"２０２６".to_string()));
    }

    #[test]
    fn mixed_or_non_digit_no_width_pair() {
        let r = SymbolRewriter::new();
        // Digits mixed with other chars are NOT expanded by digit_width_variants.
        let out = texts(&r.rewrite("a1"));
        assert!(!out.iter().any(|s| s == "ａ１"));
        // Pure ASCII letters also not expanded (digit-only gate).
        let out = texts(&r.rewrite("abc"));
        assert!(!out.iter().any(|s| s == "ａｂｃ"));
    }

    #[test]
    fn double_quote_returns_variants() {
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("\""));
        assert!(out.contains(&"”".to_string()));
        assert!(out.contains(&"“".to_string()));
        let out = texts(&r.rewrite("”"));
        assert!(out.contains(&"\"".to_string()));
        assert!(out.contains(&"“".to_string()));
    }

    #[test]
    fn repeated_dots_emit_ellipsis() {
        let r = SymbolRewriter::new();
        assert!(texts(&r.rewrite("。。。")).contains(&"…".to_string()));
        assert!(texts(&r.rewrite("...")).contains(&"…".to_string()));
        assert!(texts(&r.rewrite("・・・")).contains(&"…".to_string()));
        assert!(texts(&r.rewrite("。。")).contains(&"‥".to_string()));
        assert!(texts(&r.rewrite("..")).contains(&"‥".to_string()));
        let out = texts(&r.rewrite("…"));
        assert!(out.contains(&"。。。".to_string()));
        assert!(out.contains(&"...".to_string()));
    }

    #[test]
    fn paired_brackets_emit_other_pairs() {
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("「」"));
        assert!(out.contains(&"『』".to_string()));
        assert!(out.contains(&"【】".to_string()));
        assert!(out.contains(&"（）".to_string()));
        let out = texts(&r.rewrite("()"));
        assert!(out.contains(&"「」".to_string()));
    }

    #[test]
    fn ascii_symbol_pair_expands() {
        let r = SymbolRewriter::new();
        assert!(texts(&r.rewrite("@")).contains(&"＠".to_string()));
        assert!(texts(&r.rewrite("＠")).contains(&"@".to_string()));
    }

    // ---------- description tests ----------

    #[test]
    fn description_returns_mozc_label() {
        // These come from mozc's symbol.tsv via data/symbols.yml.
        assert_eq!(description("。"), Some("句点"));
        assert_eq!(description("、"), Some("読点"));
        assert_eq!(description("…"), Some("三点リーダ"));
        assert_eq!(description("‥"), Some("二点リーダ"));
        assert_eq!(description("「"), Some("始めかぎ括弧"));
        assert_eq!(description("『』"), Some("二重かぎ括弧"));
    }

    #[test]
    fn description_returns_none_for_unknown() {
        assert_eq!(description("あ"), None);
        assert_eq!(description("競技"), None);
        assert_eq!(description(""), None);
    }

    #[test]
    fn rewriter_attaches_description_to_known_variants() {
        let r = SymbolRewriter::new();
        let out = r.rewrite("。。。");
        // `…` should come back with its description.
        assert_eq!(desc(&out, "…"), Some("三点リーダ".to_string()));
    }

    #[test]
    fn rewriter_returns_none_for_undescribed_variants() {
        let r = SymbolRewriter::new();
        let out = r.rewrite("。");
        // `.` (ASCII period) has no mozc description in our YAML — must be None,
        // not a stale label from a different symbol.
        assert!(desc(&out, ".").is_none());
    }

    // ---------- reading → symbol lookup (from mozc's symbol.tsv) ----------

    #[test]
    fn kagikakko_reading_emits_paired_brackets() {
        // Typing the reading `かぎかっこ` should surface `「」` and `『』`
        // as candidates (mozc symbol.tsv parity).
        let r = SymbolRewriter::new();
        let out = r.rewrite("かぎかっこ");
        let texts: Vec<String> = out.iter().map(|(t, _)| t.clone()).collect();
        assert!(
            texts.contains(&"「」".to_string()),
            "「」 should appear for reading かぎかっこ, got: {:?}",
            texts
        );
        assert!(
            texts.contains(&"『』".to_string()),
            "『』 should appear for reading かぎかっこ, got: {:?}",
            texts
        );
        // The mozc-sourced description rides along on the candidate.
        assert_eq!(desc(&out, "「」"), Some("かぎ括弧".to_string()));
    }

    #[test]
    fn kakko_reading_emits_many_bracket_pairs() {
        // The broader `かっこ` reading covers many bracket variants in
        // mozc's symbol.tsv.
        let r = SymbolRewriter::new();
        let out = r.rewrite("かっこ");
        let texts: Vec<String> = out.iter().map(|(t, _)| t.clone()).collect();
        for expected in ["「」", "『』", "（）", "【】"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "{} should appear for reading かっこ, got: {:?}",
                expected,
                texts
            );
        }
    }

    #[test]
    fn ascii_reading_lookup_matches_mozc() {
        // Mozc parity: typing the literal ASCII reading `<` should surface
        // every symbol that lists `<` as a reading in symbol.tsv — the
        // angle bracket, less-than-or-equal, triangles, etc. This was the
        // case the earlier porter intentionally filtered out; regenerating
        // entries: from symbol.tsv brings it back.
        let r = SymbolRewriter::new();
        let out = r.rewrite("<");
        let texts: Vec<String> = out.iter().map(|(t, _)| t.clone()).collect();
        for expected in ["〈", "‹", "＜", "≦", "◁", "◀"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "{} should appear for ASCII reading <, got: {:?}",
                expected,
                texts
            );
        }
    }

    #[test]
    fn multichar_ascii_reading_lookup_matches_mozc() {
        // `<<` (paired less-than) is a multi-char ASCII reading in mozc's
        // symbol.tsv mapping to `《`, `«`, `≪`. Typing `<<` should surface
        // these without going through the hiragana lookup.
        let r = SymbolRewriter::new();
        let out = r.rewrite("<<");
        let texts: Vec<String> = out.iter().map(|(t, _)| t.clone()).collect();
        for expected in ["《", "«", "≪"] {
            assert!(
                texts.iter().any(|t| t == expected),
                "{} should appear for ASCII reading <<, got: {:?}",
                expected,
                texts
            );
        }
    }

    #[test]
    fn unknown_reading_yields_no_symbol_lookup() {
        // A plain Japanese word with no symbol-table reading should not
        // produce symbol candidates from this path.
        let r = SymbolRewriter::new();
        let out = r.rewrite("きょう");
        let texts: Vec<String> = out.iter().map(|(t, _)| t.clone()).collect();
        // None of these should appear from a plain word reading.
        for unexpected in ["「」", "『』", "（）"] {
            assert!(
                !texts.iter().any(|t| t == unexpected),
                "{} should not appear for reading きょう, got: {:?}",
                unexpected,
                texts
            );
        }
    }
}
