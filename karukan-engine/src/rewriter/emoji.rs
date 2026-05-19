//! Emoji rewriter — surfaces emoji candidates from two input paths.
//!
//! 1. **Hiragana reading lookup** — a typed reading expands to matching
//!    emojis (e.g. `わらい` → `😄`, `🤣`, ...; `ぴえん` → `🥺`). Data
//!    comes from Mozc's `emoji_data.tsv`, ported into `data/emoji.yml`
//!    by `scripts/emoji_porter.py`.
//!
//! 2. **Slack-style `:trigger` lookup** — when the user types `:`
//!    followed by ASCII letters/digits, those letters are matched
//!    against each emoji's `triggers` list as a subsequence (walking
//!    the trigger left-to-right, each query char must appear in
//!    order with arbitrary chars allowed in between).
//!
//!    Ranking borrows peco's fuzzy-finder heuristic:
//!
//!    1. **Longest contiguous run** of adjacent matched chars in
//!       the trigger (descending). `:hlo` against `smiling_face_with_halo`
//!       gets a run of 2 (l-o adjacent inside `halo`) while
//!       `:hlo` against `helicopter` only gets a run of 1
//!       (h, l, o are scattered) — so 😇 outranks 🚁.
//!    2. **Earliest match position** in the trigger (ascending).
//!       Among equal-longest hits, the one that starts earlier wins.
//!    3. **Trigger length** (ascending). Final tiebreaker: shorter
//!       triggers win, so `:smile` lands on `smile` ahead of
//!       `smiley`.
//!
//!    For each trigger we try every position where the first query
//!    char appears, greedily extend to a full subseq match, and keep
//!    the best-scoring placement — so `:halo` lands on the `halo`
//!    occurrence of `h` inside `smiling_face_with_halo`, not the
//!    earlier one in `with`.
//!
//! `triggers:` in `data/emoji.yml` is a unified list of every ASCII
//! string a user might type after `:` to surface this emoji. The
//! porter assembles it from three sources:
//!
//!   * curated manual aliases (`smile`, `heart`, `+1`)
//!   * the CLDR snake_case name (`grinning_face_with_smiling_eyes`)
//!   * romaji forms derived from each hiragana reading, including
//!     Hepburn + Kunrei variants and the silent-ん form so
//!     `:pien`/`:kiniku`/`:kinniku` all reach their respective emoji.
//!
//! Because every romaji form is precomputed in the data file, the
//! runtime needs only one lookup table — there's no live romaji-to-
//! hiragana conversion path, no `hiragana_to_romaji` reverse table,
//! and no description-rendering logic specific to the romaji path.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use serde::Deserialize;

use super::{RewriteOutput, Rewriter};

const EMOJI_YAML: &str = include_str!("../../data/emoji.yml");

/// Mozc-style annotation prefix for emoji candidates. Mirrors mozc's
/// `kEmoji` constant so candidates show as e.g. `絵文字 笑顔` in the
/// candidate window.
const EMOJI_LABEL: &str = "絵文字";

/// Prefix that triggers Slack-style trigger lookup. The runtime only
/// consults the trigger table when the input begins with this char.
const TRIGGER_PREFIX: char = ':';

#[derive(Deserialize)]
struct EmojiEntry {
    char: String,
    #[serde(default)]
    readings: Vec<String>,
    #[serde(default)]
    triggers: Vec<String>,
}

#[derive(Deserialize)]
struct EmojiFile {
    #[serde(default)]
    descriptions: HashMap<String, String>,
    #[serde(default)]
    entries: Vec<EmojiEntry>,
}

struct EmojiTable {
    /// emoji → Japanese description (e.g. `😄` → `笑顔`).
    descriptions: HashMap<String, String>,
    /// hiragana reading → emoji list, in source-file order.
    by_reading: HashMap<String, Vec<String>>,
    /// `(trigger, emoji)` pairs flattened for sequential scan. The
    /// porter emits triggers in "manual alias → CLDR → romaji"
    /// order, so equal-distance ties at ranking time fall back to
    /// that more idiomatic ordering.
    triggers: Vec<(String, String)>,
}

static EMOJI_TABLE: LazyLock<EmojiTable> = LazyLock::new(|| {
    let file: EmojiFile = serde_yaml::from_str(EMOJI_YAML).expect("emoji.yml must be valid YAML");

    let mut by_reading: HashMap<String, Vec<String>> = HashMap::new();
    let mut triggers: Vec<(String, String)> = Vec::new();
    for entry in file.entries {
        for reading in &entry.readings {
            let bucket = by_reading.entry(reading.clone()).or_default();
            if !bucket.iter().any(|c| c == &entry.char) {
                bucket.push(entry.char.clone());
            }
        }
        for trig in &entry.triggers {
            triggers.push((trig.clone(), entry.char.clone()));
        }
    }

    EmojiTable {
        descriptions: file.descriptions,
        by_reading,
        triggers,
    }
});

/// Sort key for a trigger's best fuzzy placement. Plain ascending
/// `min()` selects the most relevant: longer contiguous runs come
/// first (via [`Reverse`]), then earlier start positions, then
/// shorter triggers.
type MatchScore = (Reverse<usize>, usize, usize);

/// Peco-style score for `query` against `target`. Returns `None`
/// when `query` is not a subsequence of `target` from any starting
/// position. Otherwise tries every position where `target[i] ==
/// query[0]`, greedily completes the subseq from there, and keeps
/// the lowest-sorting [`MatchScore`].
///
/// Both inputs are taken as byte slices because triggers are all
/// ASCII (see [`scripts/emoji_porter.py`]) — no need to pay the
/// `Vec<char>` allocation per call.
fn best_match_score(query: &[u8], target: &[u8]) -> Option<MatchScore> {
    if query.is_empty() {
        return None;
    }
    let target_len = target.len();
    (0..target_len)
        .filter(|&start| target[start] == query[0])
        .filter_map(|start| {
            longest_run_from(query, target, start)
                .map(|longest| (Reverse(longest), start, target_len))
        })
        .min()
}

/// Greedy subseq match from `target[start]` (which is the caller's
/// guarantee to equal `query[0]`). Returns the longest run of
/// adjacent matched positions if all of `query` is consumed, else
/// `None`.
fn longest_run_from(query: &[u8], target: &[u8], start: usize) -> Option<usize> {
    let mut qi = 1; // query[0] consumed by the anchor itself.
    let mut prev = start;
    let mut longest = 1usize;
    let mut run = 1usize;

    for (ti, &tc) in target.iter().enumerate().skip(start + 1) {
        if qi >= query.len() {
            break;
        }
        if tc != query[qi] {
            continue;
        }
        run = if ti == prev + 1 { run + 1 } else { 1 };
        longest = longest.max(run);
        prev = ti;
        qi += 1;
    }

    (qi == query.len()).then_some(longest)
}

/// Format the per-candidate description: `絵文字` alone, or
/// `絵文字 <description>` when one is registered for the emoji.
fn format_description(emoji: &str) -> String {
    match EMOJI_TABLE.descriptions.get(emoji) {
        Some(d) if !d.is_empty() => format!("{} {}", EMOJI_LABEL, d),
        _ => EMOJI_LABEL.to_string(),
    }
}

/// Like [`format_description`] but with the matched `:trigger`
/// prepended, so users can see *what* they're hitting as they type a
/// partial query — `:s` → 😄 shows `:smile 笑顔`, telling the user
/// "this is what your partial input completes to", not just "this is
/// an emoji". The trigger is the full target (e.g. `smile`), not the
/// partial query, since the user already sees their own input in the
/// preedit.
fn format_trigger_description(emoji: &str, matched_trigger: &str) -> String {
    let base = format_description(emoji);
    format!("{}{} {}", TRIGGER_PREFIX, matched_trigger, base)
}

/// Rewriter that surfaces emoji candidates from hiragana readings and
/// from Slack-style `:trigger` queries.
#[derive(Default)]
pub struct EmojiRewriter;

impl EmojiRewriter {
    pub fn new() -> Self {
        Self
    }
}

impl Rewriter for EmojiRewriter {
    fn name(&self) -> &'static str {
        "emoji"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        if candidate.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<RewriteOutput> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        let mut push_with_desc = |emoji: &str, desc: String, out: &mut Vec<RewriteOutput>| {
            if seen.insert(emoji.to_string()) {
                out.push((emoji.to_string(), Some(desc)));
            }
        };

        // Path 1: Slack-style `:trigger` lookup. Score each trigger
        // with peco's fuzzy heuristic (see `best_match_score`), sort
        // ascending, cap at MAX_TRIGGER_CANDIDATES to keep short
        // queries (`:s`) from dumping thousands of hits. Equal-score
        // ties keep emoji.yml's source order (manual alias → CLDR →
        // romaji), which already favors idiomatic triggers.
        if let Some(stripped) = candidate.strip_prefix(TRIGGER_PREFIX)
            && !stripped.is_empty()
        {
            let query = stripped.as_bytes();
            let mut scored: Vec<(MatchScore, &str, &str)> = Vec::new();
            for (trig, emoji) in &EMOJI_TABLE.triggers {
                if let Some(score) = best_match_score(query, trig.as_bytes()) {
                    scored.push((score, trig.as_str(), emoji.as_str()));
                }
            }
            scored.sort_by_key(|&(score, _, _)| score);
            for (_, trig, emoji) in scored {
                let desc = format_trigger_description(emoji, trig);
                push_with_desc(emoji, desc, &mut out);
            }
        }

        // Path 2: Hiragana reading lookup. Exact-match on the typed
        // kana against Mozc's reading table. Skipped when the input
        // is already a `:trigger` form (path 1 covered it).
        if !candidate.starts_with(TRIGGER_PREFIX)
            && let Some(emojis) = EMOJI_TABLE.by_reading.get(candidate)
        {
            for emoji in emojis {
                let desc = format_description(emoji);
                push_with_desc(emoji, desc, &mut out);
            }
        }

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    fn rewriter() -> EmojiRewriter {
        EmojiRewriter::new()
    }

    fn assert_surfaces(query: &str, emoji: &str) {
        let out = texts(&rewriter().rewrite(query));
        assert!(
            out.contains(&emoji.to_string()),
            "expected {} from `{}`, got {:?}",
            emoji,
            query,
            out
        );
    }

    fn assert_does_not_surface(query: &str, emoji: &str) {
        let out = texts(&rewriter().rewrite(query));
        assert!(
            !out.contains(&emoji.to_string()),
            "did NOT expect {} from `{}`, got {:?}",
            emoji,
            query,
            out
        );
    }

    // ---------- :trigger subseq matching ----------

    #[test]
    fn trigger_exact_match() {
        assert_surfaces(":smile", "😄");
    }

    #[test]
    fn trigger_subseq_matches_deep_substring() {
        // `:halo` matches `smiling_face_with_halo` via h-a-l-o subseq.
        assert_surfaces(":halo", "😇");
    }

    #[test]
    fn trigger_subseq_skips_chars_inside_a_word() {
        // `:hlo` skips `a` inside the trailing `halo` to reach 😇.
        assert_surfaces(":hlo", "😇");
    }

    #[test]
    fn trigger_subseq_skips_separator_chars_too() {
        // `:smhlo` walks s-m from `smiling`, hops over `_face_with_`,
        // then picks up h-l-o in `halo`.
        assert_surfaces(":smhlo", "😇");
    }

    #[test]
    fn trigger_skip_inside_word_still_matches() {
        // `:smle` skips the `i` inside `smile`.
        assert_surfaces(":smle", "😄");
    }

    #[test]
    fn trigger_out_of_order_rejects() {
        // m-l-s-i is not a subseq of s-m-i-l-e.
        assert_does_not_surface(":mlsi", "😄");
    }

    #[test]
    fn trigger_uppercase_rejects() {
        // All triggers are ASCII lowercase, so `:SMILE` simply has
        // no subseq match anywhere — no special-case needed.
        let out = rewriter().rewrite(":SMILE");
        assert!(
            out.is_empty(),
            "expected no match for :SMILE, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_accepts_plus_in_body() {
        // `+1` is the classic Slack alias for 👍.
        assert_surfaces(":+1", "👍");
    }

    // ---------- description annotation ----------

    #[test]
    fn trigger_description_carries_matched_trigger_and_label() {
        let out = rewriter().rewrite(":smile");
        let d = desc(&out, "😄").expect("😄 should have a description");
        assert!(
            d.contains(":smile") && d.contains(EMOJI_LABEL),
            "expected `:smile` and `絵文字` in description, got `{}`",
            d
        );
    }

    #[test]
    fn trigger_description_shows_full_trigger_for_partial_query() {
        // `:pie` matches the romaji trigger `pien`; the description
        // shows the full `:pien` so the user sees what they're
        // completing to.
        let out = rewriter().rewrite(":pie");
        let d = desc(&out, "🥺").expect("🥺 should have a description");
        assert!(
            d.contains(":pien"),
            "expected `:pien` in description, got `{}`",
            d
        );
    }

    // ---------- hiragana reading lookup ----------

    #[test]
    fn hiragana_reading_surfaces_emoji() {
        assert_surfaces("ぴえん", "🥺");
        assert_surfaces("おねがい", "🥺");
    }

    #[test]
    fn hiragana_unrelated_reading_rejects() {
        let out = rewriter().rewrite("きょうとし");
        assert!(out.is_empty(), "expected no match, got {:?}", texts(&out));
    }

    // ---------- precomputed romaji triggers ----------

    #[test]
    fn romaji_trigger_surfaces_emoji() {
        // Direct romaji of a Mozc reading reaches the emoji because
        // the porter emitted it into the trigger table.
        assert_surfaces(":pien", "🥺");
        let out = texts(&rewriter().rewrite(":warai"));
        assert!(
            out.contains(&"😁".to_string()) || out.contains(&"😂".to_string()),
            "expected 😁 or 😂 from :warai, got {:?}",
            out
        );
    }

    #[test]
    fn romaji_silent_n_variant_surfaces_emoji() {
        // The porter emits both `kiniku` (silent ん) and `kinniku`
        // (explicit double-n) for the reading `きんにく`, so users
        // typing either spelling reach 💪.
        assert_surfaces(":kiniku", "💪");
        assert_surfaces(":kinniku", "💪");
    }

    // ---------- guardrails ----------

    #[test]
    fn empty_input_returns_empty() {
        assert!(rewriter().rewrite("").is_empty());
    }

    #[test]
    fn colon_alone_returns_empty() {
        assert!(rewriter().rewrite(":").is_empty());
    }

    #[test]
    fn unmatched_trigger_query_returns_empty() {
        let out = rewriter().rewrite(":xyzqq");
        assert!(
            out.is_empty(),
            "expected no match for :xyzqq, got {:?}",
            out
        );
    }

    #[test]
    fn duplicate_triggers_for_same_emoji_dedupe() {
        // 😄 carries multiple triggers (`smile`, `happy`,
        // `grinning_face_with_smiling_eyes`); even when several
        // match, the emoji surfaces only once.
        let out = texts(&rewriter().rewrite(":smile"));
        let count = out.iter().filter(|t| *t == "😄").count();
        assert_eq!(count, 1, "😄 should appear once, got {:?}", out);
    }
}
