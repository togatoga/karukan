//! Emoji rewriter — surfaces emoji candidates from two input paths.
//!
//! 1. **Hiragana reading lookup** — a typed reading expands to matching
//!    emojis (e.g. `わらい` → `😄`, `🤣`, ...; `ぴえん` → `🥺`). Data
//!    comes from Mozc's `emoji_data.tsv`, ported into `data/emoji.yml`
//!    by `scripts/emoji_porter.py`.
//!
//! 2. **Slack-style `:trigger` lookup** — when the user types `:`
//!    followed by ASCII letters/digits, those letters are matched
//!    against each emoji's `triggers` list using a first-char-anchored
//!    subsequence rule (see [`subseq_match`]):
//!
//!    - `:smile`  → exact match → 😄
//!    - `:sml`    → s + m + l in order in `smile` → 😄
//!    - `:smle`   → s + m + (skip i) + l + e → 😄
//!    - `:mile`   → ✗ first char `m` ≠ first char `s` of `smile`
//!    - `:mlsi`   → ✗ no trigger starts with `m` and contains m..l..s..i
//!
//!    The first-char anchor is what gives the matching a "prefix-like
//!    narrowing" feel (mirrors how Slack/Discord emoji pickers behave):
//!    typing more characters narrows the set, never broadens it.
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
    /// All `(trigger, emoji)` pairs flattened for sequential scan.
    /// Order matches the source-file order of `triggers:` inside each
    /// entry, so the porter's "manual alias first, CLDR second,
    /// romaji last" ordering carries through to candidate ranking
    /// (equal-tier matches fall back to source order).
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

/// Maximum number of target characters that may be skipped between two
/// consecutive matched query characters. With `MAX_GAP = 1`, `:smle`
/// matches `smile` (one skipped `i`) but a sparse query like `:warai`
/// no longer subsequence-matches a long CLDR name like
/// `woman_running_facing_right` where each consumed query char sits
/// many positions apart in the target — which is what we want to keep
/// the matching feel like "narrowing prefix" rather than "loose fuzzy".
const MAX_GAP: usize = 1;

/// True when `query` matches `target` under the Slack-style rule:
///
/// - The first character of `query` must equal the first character of
///   `target` (anchor — keeps typing-more-narrows behavior).
/// - The remaining `query` characters appear in `target` in order
///   (subsequence).
/// - Between any two consecutive matched query characters, at most
///   [`MAX_GAP`] target characters are skipped. Without this bound,
///   long CLDR snake_case names accept almost any short query
///   (`:warai` → `woman_running_facing_right`), drowning the
///   candidate list in unrelated emojis.
///
/// Trailing target characters after the final match are unbounded —
/// `:sm` still matches `smile` even though `ile` is unmatched.
fn subseq_match(query: &str, target: &str) -> bool {
    if query.is_empty() || target.is_empty() {
        return false;
    }
    let q: Vec<char> = query.chars().collect();
    let t: Vec<char> = target.chars().collect();
    if q[0] != t[0] {
        return false;
    }
    let mut qi: usize = 0;
    let mut last_match: Option<usize> = None;
    for (ti, &tc) in t.iter().enumerate() {
        if qi >= q.len() {
            break;
        }
        if tc == q[qi] {
            if let Some(prev) = last_match
                && ti - prev > MAX_GAP + 1
            {
                return false;
            }
            last_match = Some(ti);
            qi += 1;
        }
    }
    qi == q.len()
}

/// True iff every char in `s` is a legal Slack-style trigger char
/// (lowercase ASCII letter, digit, `_`, `+`, `-`).
fn is_trigger_chars(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '+' | '-'))
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

        // 1. Slack-style :trigger lookup. Only fires when the input
        //    starts with `:` and the suffix is plausibly a trigger
        //    fragment (no kana, no uppercase, no symbols beyond the
        //    handful Slack permits).
        //
        // Matches are scored to make incremental typing feel like
        // narrowing rather than a static list reshuffling:
        //
        //   - Exact alias hits (`:smile` → `smile` trigger) come first
        //     so `:smile` surfaces 😄 (not 😃 via `smiley` subseq).
        //   - Prefix hits (`:sm` → `smile`) come next so typing more
        //     characters keeps the same emojis pinned to the top.
        //   - Looser subsequence hits trail behind.
        //   - Within a tier, shorter triggers win — `:s` should
        //     prioritize 😄 (`smile`, 5 chars) over a sea of
        //     `*_dark_skin_tone` variants 30+ chars long that just
        //     happen to start with `s`.
        if let Some(stripped) = candidate.strip_prefix(TRIGGER_PREFIX)
            && is_trigger_chars(stripped)
        {
            let mut scored: Vec<(u8, usize, &str, &str)> = Vec::new();
            for (trig, emoji) in &EMOJI_TABLE.triggers {
                if !subseq_match(stripped, trig) {
                    continue;
                }
                let tier: u8 = if trig == stripped {
                    0
                } else if trig.starts_with(stripped) {
                    1
                } else {
                    2
                };
                scored.push((tier, trig.len(), trig.as_str(), emoji.as_str()));
            }
            // Stable sort: tier asc, then trigger length asc.
            // Equal-key entries fall back to emoji.yml's source order.
            scored.sort_by_key(|&(tier, len, _, _)| (tier, len));
            for (_, _, trig, emoji) in scored {
                let desc = format_trigger_description(emoji, trig);
                push_with_desc(emoji, desc, &mut out);
            }
        }

        // 2. Hiragana reading lookup (mozc-parity path). Skipped when
        //    the input is already in trigger form so we don't double-
        //    surface candidates that just match by happenstance.
        //    Annotation here is the plain `絵文字 <desc>` form — the
        //    user typed the hiragana reading directly, so there's no
        //    extra trigger to disambiguate.
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

    // ---------- subseq_match ----------

    #[test]
    fn subseq_first_char_must_match() {
        assert!(subseq_match("smile", "smile"));
        assert!(subseq_match("sml", "smile"));
        assert!(subseq_match("smle", "smile"));
        assert!(!subseq_match("mile", "smile"));
        assert!(!subseq_match("mlsi", "smile"));
    }

    #[test]
    fn subseq_rejects_loose_match_in_long_target() {
        assert!(!subseq_match("warai", "woman_running_facing_right"));
        assert!(!subseq_match("pien", "pleading_face"));
    }

    #[test]
    fn subseq_allows_skip_of_one_char() {
        assert!(subseq_match("smle", "smile"));
        assert!(subseq_match("smie", "smile"));
        assert!(!subseq_match("sle", "smile"));
    }

    #[test]
    fn subseq_handles_empty_inputs() {
        assert!(!subseq_match("", ""));
        assert!(!subseq_match("", "smile"));
        assert!(!subseq_match("smile", ""));
    }

    #[test]
    fn subseq_does_not_revisit_target_chars() {
        assert!(!subseq_match("ss", "smile"));
    }

    // ---------- :trigger lookup ----------

    #[test]
    fn trigger_exact_match_smile() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":smile"));
        assert!(
            out.contains(&"😄".to_string()),
            "expected 😄 from :smile, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_subsequence_smle_matches_smile() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":smle"));
        assert!(
            out.contains(&"😄".to_string()),
            "expected 😄 from :smle, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_mlsi_rejects_smile() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":mlsi"));
        assert!(
            !out.contains(&"😄".to_string()),
            "did NOT expect 😄 from :mlsi, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_heart_returns_red_heart() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":heart"));
        assert!(
            out.contains(&"❤\u{fe0f}".to_string()) || out.contains(&"❤".to_string()),
            "expected ❤ from :heart, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_plus_one_accepts_punctuation() {
        // `+1` is Slack's classic trigger for 👍; the porter quotes
        // it so the YAML loader returns it as a string. The rewriter
        // must accept `+` inside the trigger body.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":+1"));
        assert!(
            out.contains(&"👍".to_string()),
            "expected 👍 from :+1, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_rejects_uppercase() {
        let r = EmojiRewriter::new();
        let out = r.rewrite(":SMILE");
        assert!(
            out.is_empty(),
            "expected no match for :SMILE, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_carries_emoji_description() {
        // Description must include both the matched `:trigger` (so
        // the user knows what their partial input completes to) and
        // the `絵文字` label that signals the candidate's category.
        let r = EmojiRewriter::new();
        let out = r.rewrite(":smile");
        let d = desc(&out, "😄").expect("😄 should have a description");
        assert!(
            d.contains(":smile") && d.contains(EMOJI_LABEL),
            "description should contain both `:smile` and `絵文字`, got `{}`",
            d
        );
    }

    // ---------- hiragana reading lookup ----------

    #[test]
    fn hiragana_pien_surfaces_pleading_face() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite("ぴえん"));
        assert!(
            out.contains(&"🥺".to_string()),
            "expected 🥺 from ぴえん, got {:?}",
            out
        );
    }

    #[test]
    fn hiragana_unrelated_reading_returns_empty() {
        let r = EmojiRewriter::new();
        let out = r.rewrite("きょうとし");
        assert!(out.is_empty(), "expected no match, got {:?}", texts(&out));
    }

    #[test]
    fn hiragana_multiple_readings_for_same_emoji() {
        let r = EmojiRewriter::new();
        assert!(texts(&r.rewrite("おねがい")).contains(&"🥺".to_string()));
        assert!(texts(&r.rewrite("ぴえん")).contains(&"🥺".to_string()));
    }

    // ---------- guardrails ----------

    #[test]
    fn empty_input_returns_empty() {
        let r = EmojiRewriter::new();
        assert!(r.rewrite("").is_empty());
    }

    #[test]
    fn colon_alone_returns_nothing() {
        let r = EmojiRewriter::new();
        assert!(r.rewrite(":").is_empty());
    }

    // ---------- precomputed romaji triggers ----------

    #[test]
    fn romaji_pien_surfaces_pleading_face() {
        // The romaji path now comes from precomputed `triggers:` in
        // emoji.yml rather than a runtime romaji-to-hiragana
        // conversion. `:pien` should land directly on 🥺.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":pien"));
        assert!(
            out.contains(&"🥺".to_string()),
            "expected 🥺 from :pien, got {:?}",
            out
        );
    }

    #[test]
    fn romaji_pie_prefix_surfaces_pleading_face() {
        // `:pie` is a prefix of trigger `pien`, so the prefix tier
        // should still surface 🥺 with `:pien` shown in the desc.
        let r = EmojiRewriter::new();
        let out = r.rewrite(":pie");
        assert!(
            texts(&out).contains(&"🥺".to_string()),
            "expected 🥺 from :pie (prefix), got {:?}",
            texts(&out)
        );
        let d = desc(&out, "🥺").expect("🥺 should have a description");
        assert!(
            d.contains(":pien"),
            "expected :pien in description, got `{}`",
            d
        );
    }

    #[test]
    fn romaji_kiniku_and_kinniku_both_surface_muscle() {
        // The user-reported bug: `:kiniku` should reach 💪 because
        // people mentally split きんにく as "ki-n-niku" but their
        // fingers type `kiniku` (the leading `n` of `niku` absorbs
        // the ん). The porter emits both the silent-ん form
        // (`kiniku`) and the explicit double-n form (`kinniku`), and
        // either should land on 💪.
        let r = EmojiRewriter::new();
        for query in [":kiniku", ":kinniku"] {
            let out = texts(&r.rewrite(query));
            assert!(
                out.contains(&"💪".to_string()),
                "expected 💪 from {}, got {:?}",
                query,
                out
            );
        }
    }

    #[test]
    fn romaji_warai_surfaces_smiling_face() {
        // Mozc registers `わらい` (笑い) as a reading for 😁 and 😂.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":warai"));
        assert!(
            out.contains(&"😁".to_string()) || out.contains(&"😂".to_string()),
            "expected 😁 or 😂 from :warai, got {:?}",
            out
        );
    }

    #[test]
    fn romaji_garbage_yields_no_match() {
        let r = EmojiRewriter::new();
        let out = r.rewrite(":xyzqq");
        assert!(
            out.is_empty(),
            "expected no match for :xyzqq, got {:?}",
            out
        );
    }

    #[test]
    fn dedupes_emoji_across_multiple_matching_triggers() {
        // 😄 has multiple aliases (smile, happy, grinning_face...).
        // `:smile` may subseq-match more than one alias, but the
        // emoji should only appear once.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":smile"));
        let count = out.iter().filter(|t| *t == "😄").count();
        assert_eq!(count, 1, "😄 should appear once, got {:?}", out);
    }
}
