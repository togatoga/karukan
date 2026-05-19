//! Emoji rewriter — surfaces emoji candidates from two input paths.
//!
//! 1. **Hiragana reading lookup** — a typed reading expands to matching
//!    emojis (e.g. `わらい` → `😄`, `🤣`, ...; `ぴえん` → `🥺`). Data
//!    comes from Mozc's `emoji_data.tsv`, ported into `data/emoji.yml`
//!    by `scripts/emoji_porter.py`.
//!
//! 2. **Slack-style `:trigger` lookup** — when the user types `:`
//!    followed by ASCII letters/digits, those letters are matched
//!    against each emoji's `triggers` list using **token-chain
//!    fuzzy** matching. Tokens are the `_`/`+`/`-` separated runs.
//!    Rules:
//!
//!    - The query must start at a token's first char (the trigger
//!      head or right after a separator).
//!    - Within a token, chars may be skipped — `:hlo` consumes the
//!      `halo` token by skipping `a`.
//!    - To cross a separator into the next token, the query's
//!      *next unmatched char* must equal that token's first char.
//!      Tokens whose first char doesn't match are skipped wholesale.
//!
//! - `:halo`  → `smiling_face_with_halo`: jump to `halo` token,
//!   consume h-a-l-o → 😇
//! - `:hlo`   → `halo` token consumes h-(skip a)-l-o → 😇
//! - `:smhlo` → `smiling` consumes s-m, then `halo` token
//!   (first char `h` matches next query char), consume
//!   the rest of h-l-o → 😇
//! - `:smle`  → `smile` token consumes s-m-(skip i)-l-e → 😄
//! - `:hal`   → `whale` has no token starting with `h`, reject
//! - `:warai` → `woman` consumes w-a, jumps to `running`'s `r`,
//!   but `running` has no `a` left → reject (so the
//!   old `:warai` → `woman_running_facing_right`
//!   regression stays fixed)
//!
//! Matches are ranked by Levenshtein edit distance against the full
//! trigger, so exact hits beat prefixes beat mid-trigger hits, and
//! within a tier the shorter trigger wins.
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

/// Upper bound on candidates returned per `:` query. Fuzzy matching
/// against ~22k triggers can still yield many hits for short queries
/// like `:s`; we cap the list because the IME UI can't sensibly page
/// through that many. The edit-distance ranking keeps the most
/// relevant ones at the top.
const MAX_TRIGGER_CANDIDATES: usize = 64;

/// Chars that delimit "tokens" inside a snake_case trigger. A fuzzy
/// match must begin at a token boundary — the trigger's leading
/// position or right after one of these — so `:halo` lands on the
/// `halo` token in `smiling_face_with_halo` while `:hal` doesn't
/// fire on the `hal` substring in mid-`whale`.
fn is_word_separator(c: char) -> bool {
    matches!(c, '_' | '+' | '-')
}

/// True iff `query` matches `target` under the token-chain fuzzy
/// rule (see module docs). Tries every position where `target` has
/// `query[0]` at a token start (the trigger head or right after a
/// separator) and runs [`consume_chain`] from there.
fn token_anchored_fuzzy_match(query: &str, target: &str) -> bool {
    if query.is_empty() || target.is_empty() {
        return false;
    }
    let q: Vec<char> = query.chars().collect();
    let t: Vec<char> = target.chars().collect();

    for anchor in 0..t.len() {
        if t[anchor] != q[0] {
            continue;
        }
        let is_token_start = anchor == 0 || is_word_separator(t[anchor - 1]);
        if !is_token_start {
            continue;
        }
        if consume_chain(&q, &t, anchor) {
            return true;
        }
    }
    false
}

/// Walk `t` from `anchor` consuming `q` under the token-chain rule:
///
/// 1. The anchor char (`t[anchor]` == `q[0]`) is consumed up front.
/// 2. Inside the current token, advance through `q` as a subsequence
///    — chars in `t` that don't match the current `q` char are
///    skipped freely.
/// 3. When a separator (`_`/`+`/`-`) is hit, look ahead for the next
///    token whose *first* char equals the next unmatched `q` char.
///    Tokens whose first char doesn't match are skipped wholesale.
///    This is what keeps `:warai` from grabbing chars across
///    unrelated tokens of `woman_running_facing_right` while still
///    letting `:smhlo` chain `smiling` → `halo`.
fn consume_chain(q: &[char], t: &[char], anchor: usize) -> bool {
    let mut qi: usize = 1; // q[0] is consumed by being the anchor.
    let mut ti = anchor + 1;
    loop {
        // Consume within the current token.
        while ti < t.len() && !is_word_separator(t[ti]) {
            if qi >= q.len() {
                return true;
            }
            if t[ti] == q[qi] {
                qi += 1;
            }
            ti += 1;
        }
        if qi >= q.len() {
            return true;
        }
        if ti >= t.len() {
            return false;
        }
        // Skip the separator.
        ti += 1;
        // Hunt for the next token whose first char anchors q[qi].
        loop {
            if ti >= t.len() {
                return false;
            }
            if t[ti] == q[qi] {
                qi += 1;
                ti += 1;
                break;
            }
            // Skip this whole token (and trailing separator).
            while ti < t.len() && !is_word_separator(t[ti]) {
                ti += 1;
            }
            if ti < t.len() {
                ti += 1;
            }
        }
    }
}

/// Levenshtein edit distance between `a` and `b` (in unicode chars).
/// Used to rank substring-matching triggers: the lower the distance,
/// the closer the trigger is to the typed query (exact match → 0,
/// `query` as a prefix → `|target| - |query|`, deep middle-substring
/// hits → larger because of the leading insertions). Compared to
/// pure length-sort it gives slightly better intuition for typos and
/// for picking the trigger whose match starts earliest in the name.
fn edit_distance(a: &[char], b: &[char]) -> usize {
    if a.is_empty() {
        return b.len();
    }
    if b.is_empty() {
        return a.len();
    }
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    let mut curr: Vec<usize> = vec![0; b.len() + 1];
    for i in 1..=a.len() {
        curr[0] = i;
        for j in 1..=b.len() {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (curr[j - 1] + 1).min(prev[j] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[b.len()]
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
        // Match rule mirrors Slack's emoji picker: a trigger is a
        // candidate iff `query` is a substring of it. Ranking is by
        // Levenshtein edit distance ascending, so:
        //
        //   - exact `:smile` → trigger `smile`  → distance 0
        //   - prefix `:sm`   → trigger `smile`  → distance 3
        //   - middle `:halo` → trigger `smiling_face_with_halo`
        //                                       → distance 18
        //
        // The shortest trigger whose string most-closely matches the
        // query wins, which gives the intuitive "I typed it exactly"
        // > "I typed a prefix" > "the word lives deep inside a CLDR
        // name" ordering without us having to enumerate tiers.
        // Substring-matching against ~22k triggers can balloon for
        // short queries (`:s` hits thousands), so we cap the visible
        // list with [`MAX_TRIGGER_CANDIDATES`].
        if let Some(stripped) = candidate.strip_prefix(TRIGGER_PREFIX)
            && is_trigger_chars(stripped)
        {
            let query_chars: Vec<char> = stripped.chars().collect();
            let mut scored: Vec<(usize, &str, &str)> = Vec::new();
            for (trig, emoji) in &EMOJI_TABLE.triggers {
                if !token_anchored_fuzzy_match(stripped, trig) {
                    continue;
                }
                let trig_chars: Vec<char> = trig.chars().collect();
                let dist = edit_distance(&query_chars, &trig_chars);
                scored.push((dist, trig.as_str(), emoji.as_str()));
            }
            // Stable sort: edit distance asc. Equal-distance ties
            // fall back to emoji.yml's source order, which already
            // places manual aliases ahead of CLDR ahead of romaji.
            scored.sort_by_key(|&(dist, _, _)| dist);
            for (_, trig, emoji) in scored.into_iter().take(MAX_TRIGGER_CANDIDATES) {
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

    // ---------- :trigger lookup ----------

    #[test]
    fn trigger_word_boundary_anchor_halo_surfaces_innocent() {
        // `:halo` consumes the `halo` token in `smiling_face_with_halo`.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":halo"));
        assert!(
            out.contains(&"😇".to_string()),
            "expected 😇 from :halo, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_fuzzy_within_token_hlo_matches_halo() {
        // Headline fuzzy case: `:hlo` skips the `a` inside the `halo`
        // token and still reaches 😇. Demonstrates that within a
        // token the user's chars can drop letters.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":hlo"));
        assert!(
            out.contains(&"😇".to_string()),
            "expected 😇 from :hlo (fuzzy within `halo` token), got {:?}",
            out
        );
    }

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
    fn trigger_typo_within_token_is_accepted() {
        // `:smle` skips the `i` inside the `smile` token. With the
        // head-anchored fuzzy rule this matches.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":smle"));
        assert!(
            out.contains(&"😄".to_string()),
            "expected 😄 from :smle (fuzzy within `smile` token), got {:?}",
            out
        );
    }

    #[test]
    fn trigger_mid_token_substring_is_rejected() {
        // `:hal` only appears mid-token in `whale` (no token starts
        // with `h`), so the head-anchored fuzzy rule rejects it.
        // This is the fix for the previous substring-based behavior
        // where `:hal` would surface 🐋 ahead of any halo-related
        // emoji.
        assert!(!token_anchored_fuzzy_match("hal", "whale"));
    }

    #[test]
    fn trigger_out_of_order_does_not_match() {
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":mlsi"));
        assert!(
            !out.contains(&"😄".to_string()),
            "did NOT expect 😄 from :mlsi, got {:?}",
            out
        );
    }

    #[test]
    fn trigger_chains_tokens_when_next_first_char_matches() {
        // `:smhlo` chains `smiling` and `halo`: s-m consumed in
        // `smiling`, then `halo`'s leading `h` anchors the jump and
        // l-o finish off in `halo`.
        let r = EmojiRewriter::new();
        let out = texts(&r.rewrite(":smhlo"));
        assert!(
            out.contains(&"😇".to_string()),
            "expected 😇 from :smhlo (token chain smiling→halo), got {:?}",
            out
        );
    }

    #[test]
    fn trigger_rejects_when_chain_cannot_complete() {
        // The chain rule rejects queries that would require pulling
        // chars across tokens without each new token's first char
        // matching the next unmatched query char. `:warai` against
        // `woman_running_facing_right` makes it to `running` (via
        // `r`) but then needs `a-i` and `running` has neither, and
        // no later token starts with `a` → reject. This keeps the
        // old `:warai` regression fixed.
        assert!(!token_anchored_fuzzy_match(
            "warai",
            "woman_running_facing_right"
        ));
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
