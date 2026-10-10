//! Candidate rewriters: produce additional variants from existing conversion candidates.
//!
//! Algorithms, tables and descriptions in this module are Rust re-implementations
//! of Mozc's rewriter logic (`NumberRewriter`, `SymbolRewriter`, `NumberUtil`,
//! width/case helpers). See `THIRD_PARTY_LICENSES` at the repo root for the
//! upstream BSD-3-Clause notice.
//!
//! A `Rewriter` takes a single string, the typed reading, and returns zero
//! or more `(variant, description)` pairs. The description is shown as the
//! candidate's annotation in the candidate window (e.g. `…` -> "三点リーダ");
//! rewriters that don't have a meaningful description return `None`.
//!
//! Rewriters do not run the kana-kanji model. They either transform the
//! reading mechanically (full-width katakana to half-width, a bracket to
//! its relatives) or look it up in a table of their own (a symbol or emoji
//! by name, the single kanji).
//!
//! The chain (`RewriterChain`) applies all registered rewriters to the input
//! candidate list and returns flat variants. Caller (IMEEngine) is responsible
//! for deduplication and ordering.

mod alphabet;
mod date;
mod emoji;
mod half_katakana;
mod number;
mod single_kanji;
mod symbol;

pub use alphabet::AlphabetRewriter;
pub use date::{DateConfig, DatePhrase, DateRewriter};
pub use emoji::EmojiRewriter;
pub use half_katakana::HalfWidthKatakanaRewriter;
pub use number::NumberRewriter;
pub use single_kanji::{SingleKanjiRewriter, description as single_kanji_description};
pub use symbol::{SymbolRewriter, description};

use crate::kana::is_digit;

/// True iff every character is a halfwidth (`0-9`) or fullwidth (`０-９`)
/// decimal digit, and the string is non-empty. Shared gate for digit-only
/// rewriting so arbitrary candidates with stray digits aren't expanded.
pub(crate) fn is_pure_digit(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_digit)
}

/// One result of rewriting: the variant text and an optional description used
/// as the candidate's annotation (e.g. `三点リーダ` for `…`).
pub type RewriteOutput = (String, Option<String>);

/// A rewriter takes one candidate and produces variants.
pub trait Rewriter: Send + Sync {
    /// Stable identifier for logging/debugging.
    fn name(&self) -> &'static str;

    /// Produce variants from a single candidate. The original candidate is not
    /// included in the result. Each result is paired with an optional
    /// description used as the candidate annotation.
    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput>;

    /// True when the rows are a catalog whose order is only a default, so
    /// the caller may rank them (the single kanji, by use). The others
    /// emit a fixed order that is part of their meaning.
    fn is_catalog(&self) -> bool {
        false
    }
}

/// A chain of rewriters applied in registration order.
#[derive(Default)]
pub struct RewriterChain {
    rewriters: Vec<Box<dyn Rewriter>>,
}

impl RewriterChain {
    /// Create an empty chain.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a rewriter at the end of the chain.
    pub fn add(&mut self, rewriter: Box<dyn Rewriter>) {
        self.rewriters.push(rewriter);
    }

    /// Build the default chain used by the IME. The order is the order of
    /// the rows: first what the reading itself names (a symbol, a number
    /// form, an emoji), then the single kanji, and last the mechanical
    /// width and case variants of the text.
    pub fn default_chain() -> Self {
        let mut chain = Self::new();
        chain.add(Box::new(SymbolRewriter));
        chain.add(Box::new(NumberRewriter));
        chain.add(Box::new(EmojiRewriter));
        chain.add(Box::new(SingleKanjiRewriter));
        chain.add(Box::new(HalfWidthKatakanaRewriter));
        chain.add(Box::new(AlphabetRewriter));
        chain
    }

    /// Apply all rewriters to each candidate. Returns variants in registration
    /// order. Caller must deduplicate and merge with the original candidate
    /// list.
    pub fn rewrite_all(&self, candidates: &[String]) -> Vec<RewriteOutput> {
        let mut out = Vec::new();
        for cand in candidates {
            for rewriter in &self.rewriters {
                out.extend(rewriter.rewrite(cand));
            }
        }
        out
    }

    /// Apply all rewriters to one reading, letting `rank` reorder each
    /// catalog rewriter's rows before they are joined. The rest keep their
    /// own order, so a ranking never moves a row across rewriters.
    pub fn rewrite_ranked(
        &self,
        reading: &str,
        mut rank: impl FnMut(&mut [RewriteOutput]),
    ) -> Vec<RewriteOutput> {
        let mut out = Vec::new();
        for rewriter in &self.rewriters {
            let mut rows = rewriter.rewrite(reading);
            if rewriter.is_catalog() {
                rank(&mut rows);
            }
            out.extend(rows);
        }
        out
    }
}

/// Shared test helpers for inspecting `RewriteOutput` slices.
///
/// Pulled out of the per-rewriter test modules so each one doesn't have to
/// re-implement the same `(text, description)` accessors.
#[cfg(test)]
pub(crate) mod test_util {
    use super::RewriteOutput;

    /// Collect just the variant texts from a `rewrite()` result.
    pub fn texts(out: &[RewriteOutput]) -> Vec<String> {
        out.iter().map(|(t, _)| t.clone()).collect()
    }

    /// Find the description attached to a specific variant text.
    pub fn desc(out: &[RewriteOutput], text: &str) -> Option<String> {
        out.iter()
            .find(|(t, _)| t == text)
            .and_then(|(_, d)| d.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct UpcaseRewriter;
    impl Rewriter for UpcaseRewriter {
        fn name(&self) -> &'static str {
            "upcase"
        }
        fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
            vec![(candidate.to_uppercase(), None)]
        }
    }

    #[test]
    fn empty_chain_returns_empty() {
        let chain = RewriterChain::new();
        let out = chain.rewrite_all(&["a".to_string(), "b".to_string()]);
        assert!(out.is_empty());
    }

    /// Emits a fixed list and declares it a catalog.
    struct CatalogRewriter;
    impl Rewriter for CatalogRewriter {
        fn name(&self) -> &'static str {
            "catalog"
        }
        fn rewrite(&self, _: &str) -> Vec<RewriteOutput> {
            vec![("乙".to_string(), None), ("甲".to_string(), None)]
        }
        fn is_catalog(&self) -> bool {
            true
        }
    }

    #[test]
    fn ranking_touches_only_catalog_rows() {
        let mut chain = RewriterChain::new();
        chain.add(Box::new(UpcaseRewriter));
        chain.add(Box::new(CatalogRewriter));
        // The ranker reverses whatever it is given: only the catalog's rows
        // come back reversed, and they stay behind the upcase row.
        let out = chain.rewrite_ranked("ab", |rows| rows.reverse());
        let texts: Vec<&str> = out.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(texts, ["AB", "甲", "乙"]);
    }

    #[test]
    fn chain_applies_all_rewriters() {
        let mut chain = RewriterChain::new();
        chain.add(Box::new(UpcaseRewriter));
        let out = chain.rewrite_all(&["abc".to_string(), "def".to_string()]);
        assert_eq!(
            out,
            vec![("ABC".to_string(), None), ("DEF".to_string(), None),]
        );
    }
}
