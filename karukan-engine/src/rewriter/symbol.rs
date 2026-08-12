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

use super::{RewriteOutput, Rewriter, is_pure_digit};
use crate::width::{has_width_pair, to_full_width, to_half_width};

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

/// The whole candidate at one width and the other (`(1)` → `（１）`, `123`
/// → `１２３`), each with its width marker (`全` / `半`) and the label to
/// use when the variant is new (`記号` / `数字`).
///
/// Only for candidates made entirely of characters that *have* both forms,
/// so a word with one mark in it (`きょう。`) is not offered as 「きょう｡」.
/// Pure letters are left to [`super::AlphabetRewriter`], which covers case
/// as well as width.
fn width_variants(candidate: &str) -> Vec<(String, &'static str, &'static str)> {
    let convertible =
        !candidate.is_empty() && candidate.chars().all(has_width_pair) && !is_pure_alpha(candidate);
    if !convertible {
        return Vec::new();
    }
    let kind = if is_pure_digit(candidate) {
        "数字"
    } else {
        "記号"
    };
    [
        (to_full_width(candidate), "全"),
        (to_half_width(candidate), "半"),
    ]
    .into_iter()
    .filter(|(text, _)| text != candidate)
    .map(|(text, marker)| (text, marker, kind))
    .collect()
}

/// True for a non-empty run of ASCII or full-width letters.
fn is_pure_alpha(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphabetic() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ'))
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

        // The width pair annotates in place when an earlier step already
        // produced the same text — a variant chain reaches `｡` and `＠`
        // before this does — so neither half of the information is lost:
        // `＠` reads 「[全]アットマーク」, `｡` 「[半]記号」.
        for (text, marker, kind) in width_variants(candidate) {
            match out.iter_mut().find(|(t, _)| *t == text) {
                Some((_, desc)) => {
                    let label = desc.as_deref().unwrap_or(kind);
                    *desc = Some(format!("[{marker}]{label}"));
                }
                None => out.push((text, Some(format!("[{marker}]{kind}")))),
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
    fn pure_letters_are_left_to_the_alphabet_rewriter() {
        // That one covers case as well as width, so it owns `abc`.
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("abc"));
        assert!(!out.iter().any(|s| s == "ａｂｃ"));
    }

    #[test]
    fn a_mixed_run_offers_both_widths() {
        // What the engine passes in is the reading as displayed, which
        // under the default settings is itself mixed (symbols full, digits
        // half): both patterns have to come back from it.
        let r = SymbolRewriter::new();
        let out = r.rewrite("＜＞1234");
        assert_eq!(desc(&out, "＜＞１２３４"), Some("[全]記号".to_string()));
        assert_eq!(desc(&out, "<>1234"), Some("[半]記号".to_string()));

        // A form equal to the input is not a variant and is left out.
        let out = r.rewrite("（ａ１）");
        assert!(!texts(&out).contains(&"（ａ１）".to_string()));
        assert_eq!(desc(&out, "(a1)"), Some("[半]記号".to_string()));
    }

    #[test]
    fn a_word_with_a_mark_in_it_is_not_converted() {
        // 「きょう。」 must not be offered as 「きょう｡」: the marks around a
        // word are not what the user is choosing a width for.
        let r = SymbolRewriter::new();
        let out = texts(&r.rewrite("きょう。"));
        assert!(!out.iter().any(|s| s == "きょう｡"));
    }

    #[test]
    fn width_variants_are_annotated() {
        let r = SymbolRewriter::new();
        let out = r.rewrite("123");
        assert_eq!(desc(&out, "１２３"), Some("[全]数字".to_string()));
        let out = r.rewrite("１２３");
        assert_eq!(desc(&out, "123"), Some("[半]数字".to_string()));
    }

    #[test]
    fn a_variant_the_chain_also_produces_still_gets_its_width_label() {
        // `｡` comes from 。's variant chain first; the width pair reaches it
        // second and supplies the label the chain has none for.
        let r = SymbolRewriter::new();
        let out = r.rewrite("。");
        assert_eq!(desc(&out, "｡"), Some("[半]記号".to_string()));
        // An entry the width pair does not reach keeps its own label.
        assert_eq!(desc(&out, "．"), Some("ピリオド".to_string()));

        let out = r.rewrite("@");
        // A name the table already carries is kept, with the width marker
        // in front of it.
        assert_eq!(desc(&out, "＠"), Some("[全]アットマーク".to_string()));
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
