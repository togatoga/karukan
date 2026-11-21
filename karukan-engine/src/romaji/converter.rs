use std::sync::Arc;

use super::chain::ChainTable;
use super::rules::build_rules;
use super::style::SymbolStyle;
use super::trie::TrieNode;
use crate::width::WidthRules;

/// Result of converting a raw input string.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// Converted output: hiragana plus passed-through characters
    pub text: String,
    /// Unresolved trailing input that may still extend to a longer rule
    pub pending: String,
    /// `text` split by the rule that wrote it, each piece with the keys
    /// that fired it (`kyo` → きょ, `k` → っ in `kka`). Concatenated, the
    /// texts are `text` and the keys are the consumed input, lowercased as
    /// the rules match it (`Ka` → か ← `ka`).
    pub spans: Vec<Span>,
}

/// One fired rule (or one passed-through key): what it wrote and the
/// keys that wrote it, lowercased.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub keys: String,
}

/// A chain table's text as one span keyed by itself: the table rewrites
/// its own outputs and keeps its live tail as display text, so the keys
/// behind it are not known, and F9/F10 leave it as written rather than
/// show keys never typed.
fn text_as_keys(text: &str) -> Vec<Span> {
    if text.is_empty() {
        return Vec::new();
    }
    vec![Span {
        text: text.to_string(),
        keys: text.to_string(),
    }]
}

/// Stateless romaji-to-hiragana converter.
///
/// Holds the rule trie and the width rules; each call derives its result
/// from the full raw input, so the caller owns all editing state.
#[derive(Debug)]
pub struct RomajiConverter {
    trie: TrieNode,
    width: WidthRules,
    /// A user table replacing the built-in rules (see [`ChainTable`]).
    /// Its outputs are taken verbatim: the width rules do not apply.
    chain: Option<Arc<ChainTable>>,
}

impl RomajiConverter {
    /// Create a new converter with default rules
    pub fn new() -> Self {
        Self::with_rules(SymbolStyle::default(), WidthRules::default())
    }

    /// Create a converter whose `,` `.` `/` `[` `]` keys type `style`, and
    /// whose output settles at `width`.
    pub fn with_rules(style: SymbolStyle, width: WidthRules) -> Self {
        Self {
            trie: build_rules(style),
            width,
            chain: None,
        }
    }

    /// Create a converter driven by a user chain table instead of the
    /// built-in rules.
    pub fn with_chain_table(table: Arc<ChainTable>, width: WidthRules) -> Self {
        Self {
            chain: Some(table),
            ..Self::with_rules(SymbolStyle::default(), width)
        }
    }

    /// The form a character settles at: the configured width, except that
    /// a chain table's outputs are kept as written.
    pub fn settle_char(&self, c: char) -> char {
        if self.chain.is_some() {
            c
        } else {
            self.width.apply(c)
        }
    }

    /// The width a character settles at once it is no longer a live
    /// keystroke. Applied by the caller, which is what knows when that is.
    pub fn width(&self) -> &WidthRules {
        &self.width
    }

    /// Convert `raw` left to right. `pending` holds the trailing input that
    /// may still combine with future keys (e.g. `k`, `ky`, a lone `n`).
    ///
    /// Contract: rule outputs never contain ASCII, so any ASCII character
    /// in `text` is an input character that passed through unchanged
    /// (guarded by `rule_outputs_are_never_ascii`).
    pub fn convert(&self, raw: &str) -> Converted {
        if let Some(chain) = &self.chain {
            // `raw` is the previous live tail plus, last, the new keystroke.
            let mut buf = String::new();
            let mut chars = raw.chars();
            let last = chars.next_back();
            buf.extend(chars);
            if let Some(ch) = last {
                chain.feed(&mut buf, ch);
            }
            let (text, pending) = chain.split_live(&buf);
            return Converted {
                text: text.to_string(),
                pending: pending.to_string(),
                spans: text_as_keys(text),
            };
        }
        let mut state = ConvertState::new(&self.trie);
        for key in raw.chars() {
            state.push(key);
        }
        Converted {
            text: state.spans.iter().map(|s| s.text.as_str()).collect(),
            pending: state.pending.into_iter().collect(),
            spans: state.spans,
        }
    }

    /// Force-convert leftover pending input (`ltu` → っ); characters with no
    /// rule pass through literally (a trailing `n` stays `n`).
    pub fn flush_pending(&self, pending: &str) -> String {
        self.flush_spans(pending)
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    /// [`Self::flush_pending`], split by the rule that wrote each piece.
    pub fn flush_spans(&self, pending: &str) -> Vec<Span> {
        if self.chain.is_some() {
            return text_as_keys(pending);
        }
        let mut state = ConvertState::new(&self.trie);
        state.pending = pending.chars().collect();
        state.flush();
        state.spans
    }

    /// Convert then flush the leftover: the committed form of `raw`, at the
    /// configured width. [`Self::convert`] leaves the width alone because
    /// part of its output is still live keystrokes; here everything settles.
    pub fn convert_flush(&self, raw: &str) -> String {
        self.convert_flush_spans(raw)
            .into_iter()
            .map(|s| s.text)
            .collect()
    }

    /// [`Self::convert_flush`], split by the rule that wrote each piece.
    pub fn convert_flush_spans(&self, raw: &str) -> Vec<Span> {
        if self.chain.is_some() {
            // Runs reaching here are already evaluated live tails.
            return text_as_keys(raw);
        }
        let Converted { pending, spans, .. } = self.convert(raw);
        spans
            .into_iter()
            .chain(self.flush_spans(&pending))
            .map(|s| Span {
                text: self.width.apply_str(&s.text),
                keys: s.keys,
            })
            .collect()
    }

    /// Whether `ch` can begin a conversion rule (`k`, `y`, `n` — a later
    /// keystroke could still complete a conversion with it; `1` cannot).
    pub fn starts_rule(&self, ch: char) -> bool {
        // Chain-table text is settled as soon as it leaves the live tail.
        if self.chain.is_some() {
            return false;
        }
        self.trie.children.contains_key(&ch)
    }

    /// Kana the pending romaji can still become: the outputs of every rule
    /// whose key extends `pending` (`d` → だ/ぢ/づ/で/ど/ぢゃ…; `n` includes
    /// ん via `nn`/`n'`). Empty when `pending` is empty or cannot reach any
    /// rule (`yk`). Used to narrow predictive dictionary lookups while a
    /// romaji tail is being typed.
    pub fn pending_expansions(&self, pending: &str) -> Vec<String> {
        if let Some(chain) = &self.chain {
            return chain.expansions(pending);
        }
        if pending.is_empty() {
            return Vec::new();
        }
        let Some(node) = self.trie.walk(pending.chars()) else {
            return Vec::new();
        };
        node.outputs()
            .map(str::to_string)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

impl Default for RomajiConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Keys typed so far and the rules they fired, while a run of keys is
/// converted.
///
/// Each key is added to `pending`, then rules fire for as long as the
/// pending keys decide one:
///
/// 1. `n` before a key that cannot continue it is ん (`nk` → ん + `k`),
///    and a doubled consonant letter is っ (`kk` → っ + `k`; `''` is
///    not). These two look at the next key, so no rule table entry can
///    express them.
/// 2. While the pending keys could still grow into a longer rule (`k`,
///    `ky`), nothing fires: the next key decides.
/// 3. Otherwise the longest rule they start with fires (`kya` → きゃ), or,
///    when none does, the first key passes through as itself (`1`).
///
/// Positions and lengths count characters, never bytes.
struct ConvertState<'a> {
    trie: &'a TrieNode,
    /// Keys not yet consumed by a rule.
    pending: Vec<char>,
    /// The rules fired so far, in order.
    spans: Vec<Span>,
}

impl<'a> ConvertState<'a> {
    fn new(trie: &'a TrieNode) -> Self {
        Self {
            trie,
            pending: Vec::new(),
            spans: Vec::new(),
        }
    }

    /// Add one key, then fire every rule it decides.
    fn push(&mut self, key: char) {
        self.pending.push(key.to_ascii_lowercase());
        while self.fire_next() {}
    }

    /// Fire rules for whatever is pending without waiting for more keys:
    /// the input has ended (`ky` settles as `k` + `y`).
    fn flush(&mut self) {
        while !self.pending.is_empty() {
            self.fire_longest();
        }
    }

    /// Fire the next rule the pending keys decide. False when there is
    /// nothing to fire yet.
    fn fire_next(&mut self) -> bool {
        if self.pending.is_empty() {
            return false;
        }
        if let Some(text) = self.next_key_rule() {
            self.fire(text, 1);
            return true;
        }
        if self.is_rule_prefix() {
            return false;
        }
        self.fire_longest();
        true
    }

    /// Step 1: ん for `n` before a key that cannot continue it, っ for a
    /// doubled consonant. Either way the first key is consumed and the
    /// second stays for the next rule.
    fn next_key_rule(&self) -> Option<&'static str> {
        match self.pending[..] {
            ['n', next] if !matches!(next, 'a' | 'i' | 'u' | 'e' | 'o' | 'y' | '\'' | 'n') => {
                Some("ん")
            }
            [first, second]
                if first == second
                    && first.is_ascii_alphabetic()
                    && !matches!(first, 'a' | 'i' | 'u' | 'e' | 'o' | 'n') =>
            {
                Some("っ")
            }
            _ => None,
        }
    }

    /// Step 2: the pending keys are the start of a longer rule (`k` of
    /// `ka`, `ky` of `kya`).
    fn is_rule_prefix(&self) -> bool {
        self.trie
            .walk(self.pending.iter().copied())
            .is_some_and(|node| !node.children.is_empty())
    }

    /// Step 3: fire the longest rule the pending keys start with, or pass
    /// the first key through when none matches.
    fn fire_longest(&mut self) {
        let trie = self.trie;
        let rule = trie.search_longest(self.pending.iter().copied());
        match rule.output {
            Some(text) => self.fire(text, rule.matched_len),
            None => {
                let key = self.pending[0].to_string();
                self.fire(&key, 1);
            }
        }
    }

    /// The first `count` pending keys wrote `text`: consume them and
    /// record the span.
    fn fire(&mut self, text: &str, count: usize) {
        let keys = self.pending.drain(..count).collect();
        self.spans.push(Span {
            text: text.to_string(),
            keys,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conv(raw: &str) -> (String, String) {
        let c = RomajiConverter::new();
        let r = c.convert(raw);
        (r.text, r.pending)
    }

    #[test]
    fn test_basic_conversion() {
        assert_eq!(conv("ka"), ("か".to_string(), "".to_string()));
    }

    #[test]
    fn test_buffering() {
        assert_eq!(conv("k"), ("".to_string(), "k".to_string()));
    }

    #[test]
    fn test_sokuon() {
        assert_eq!(conv("kk"), ("っ".to_string(), "k".to_string()));
        assert_eq!(conv("kka"), ("っか".to_string(), "".to_string()));
    }

    #[test]
    fn test_sokuon_after_rule_prefix_keeps_prefix() {
        // `ty` + `y`: the pair fires but the prefix must survive
        assert_eq!(conv("tyy"), ("tっ".to_string(), "y".to_string()));
        assert_eq!(conv("tyyu"), ("tっゆ".to_string(), "".to_string()));
        assert_eq!(conv("kyy"), ("kっ".to_string(), "y".to_string()));
        assert_eq!(conv("tss"), ("tっ".to_string(), "s".to_string()));
    }

    #[test]
    fn test_n_context() {
        assert_eq!(conv("n"), ("".to_string(), "n".to_string()));
        assert_eq!(conv("na"), ("な".to_string(), "".to_string()));
    }

    #[test]
    fn test_nn() {
        // "nn" converts immediately to ん
        assert_eq!(conv("nn"), ("ん".to_string(), "".to_string()));
        assert_eq!(conv("nni"), ("んい".to_string(), "".to_string()));
        assert_eq!(conv("nna"), ("んあ".to_string(), "".to_string()));
        assert_eq!(conv("nnk"), ("ん".to_string(), "k".to_string()));
    }

    #[test]
    fn test_youon() {
        assert_eq!(conv("kya"), ("きゃ".to_string(), "".to_string()));
    }

    #[test]
    fn test_flush() {
        let c = RomajiConverter::new();
        assert_eq!(c.flush_pending("k"), "k");
        assert_eq!(c.flush_pending("ltu"), "っ");
        assert_eq!(c.convert_flush("k"), "k");
        assert_eq!(c.convert_flush("kan"), "かn");
    }

    #[test]
    fn rule_outputs_are_never_ascii() {
        // Callers tell passed-through keystrokes (ASCII) apart from rule
        // output by this property, so no rule may output an ASCII char.
        // The configurable outputs are guarded in `style.rs`.
        fn walk(node: &TrieNode, check: &mut impl FnMut(&str)) {
            if let Some(output) = &node.output {
                check(output);
            }
            for child in node.children.values() {
                walk(child, check);
            }
        }
        let c = RomajiConverter::new();
        walk(&c.trie, &mut |output| {
            assert!(
                output.chars().all(|ch| !ch.is_ascii()),
                "rule output contains ASCII: {output:?}"
            );
        });
    }

    #[test]
    fn symbol_style_picks_the_output() {
        use super::super::style::{BracketStyle, PunctuationStyle, SlashStyle};
        let c = RomajiConverter::with_rules(
            SymbolStyle {
                punctuation: PunctuationStyle::CommaPeriod,
                bracket: BracketStyle::Square,
                slash: SlashStyle::Slash,
            },
            WidthRules::default(),
        );
        assert_eq!(c.convert_flush("a,b.").as_str(), "あ，b．");
        assert_eq!(c.convert_flush("[a]").as_str(), "［あ］");
        assert_eq!(c.convert_flush("a/b").as_str(), "あ／b");
    }

    #[test]
    fn test_pending_expansions() {
        let c = RomajiConverter::new();

        let d = c.pending_expansions("d");
        for kana in ["だ", "ぢ", "づ", "で", "ど", "ぢゃ"] {
            assert!(d.iter().any(|s| s == kana), "missing {kana}");
        }
        assert!(!d.iter().any(|s| s == "か"));

        // ん is reachable from a lone n via nn / n'
        assert!(c.pending_expansions("n").iter().any(|s| s == "ん"));

        let ky = c.pending_expansions("ky");
        assert!(ky.iter().any(|s| s == "きょ"));
        assert!(!ky.iter().any(|s| s == "か"));

        assert!(c.pending_expansions("").is_empty());
        assert!(c.pending_expansions("yk").is_empty());
    }

    #[test]
    fn test_starts_rule() {
        let c = RomajiConverter::new();
        assert!(c.starts_rule('k'));
        assert!(c.starts_rule('y'));
        assert!(c.starts_rule('n'));
        assert!(!c.starts_rule('1'));
        assert!(!c.starts_rule('こ'));
    }

    #[test]
    fn test_full_sentence() {
        // IME style: "nn" is always ん, so こんにちは requires 3 n's: "konnnichiha"
        // (ko -> こ, nn -> ん, ni -> に, chi -> ち, ha -> は)
        assert_eq!(conv("konnnichiha").0, "こんにちは");
    }

    #[test]
    fn test_punctuation_passthrough() {
        assert_eq!(
            conv("kokohadoko?watashihadare?"),
            ("ここはどこ？わたしはだれ？".to_string(), "".to_string())
        );
    }

    #[test]
    fn test_mixed_punctuation() {
        // 'c' stays pending because it could start 'ca', 'chi', etc.
        assert_eq!(conv("a!b?c"), ("あ！b？".to_string(), "c".to_string()));
        let c = RomajiConverter::new();
        assert_eq!(c.convert_flush("a!b?c"), "あ！b？c");
    }

    #[test]
    fn test_watashiha() {
        assert_eq!(
            conv("kokohadoko?watashiha?"),
            ("ここはどこ？わたしは？".to_string(), "".to_string())
        );
    }

    #[test]
    fn test_punctuation_then_youon() {
        // 'c' must stay pending after '?' until 'ya' completes 'cya'
        assert_eq!(conv("a?b?cya"), ("あ？b？ちゃ".to_string(), "".to_string()));
    }

    /// Type `keys` the way the IME's input buffer does: each evaluation
    /// sees the previous live tail plus one new keystroke.
    fn type_chain(c: &RomajiConverter, keys: &str) -> (String, String) {
        let (mut settled, mut pending) = (String::new(), String::new());
        for key in keys.chars() {
            let r = c.convert(&format!("{pending}{key}"));
            settled.extend(r.text.chars().map(|ch| c.settle_char(ch)));
            pending = r.pending;
        }
        (settled, pending)
    }

    fn chain_converter() -> RomajiConverter {
        let tsv = "d\tｄ\nｄh\tか\nｄu\tきゃ\nきゃu\tきゃあ\nf\tｆ\nｆk\tし\nn\tん\n\
                   v\tｖ\nｖ;\tで\nでｆd\tですか\n.\t。\n1\t1\n1.\t1.\n";
        RomajiConverter::with_chain_table(
            Arc::new(ChainTable::parse_tsv(tsv)),
            WidthRules::default(),
        )
    }

    #[test]
    fn test_chain_table_typing() {
        let c = chain_converter();
        assert_eq!(type_chain(&c, "fkdh"), ("しか".to_string(), "".to_string()));
        assert_eq!(
            type_chain(&c, "duu"),
            ("きゃあ".to_string(), "".to_string())
        );
        // で is shown at once but stays live: でｆd could still follow
        assert_eq!(type_chain(&c, "v;"), ("".to_string(), "で".to_string()));
        assert_eq!(
            type_chain(&c, "v;fd"),
            ("ですか".to_string(), "".to_string())
        );
        assert_eq!(type_chain(&c, "v;fk"), ("でし".to_string(), "".to_string()));
    }

    #[test]
    fn test_chain_table_skips_builtin_special_cases() {
        let c = chain_converter();
        // No n-before-consonant or doubled-consonant handling
        assert_eq!(type_chain(&c, "nfk"), ("んし".to_string(), "".to_string()));
        assert_eq!(type_chain(&c, "ddh"), ("ｄか".to_string(), "".to_string()));
        // Outputs are taken verbatim, ASCII included, without width folding
        assert_eq!(type_chain(&c, "1."), ("1.".to_string(), "".to_string()));
        assert_eq!(type_chain(&c, "."), ("。".to_string(), "".to_string()));
        assert!(!c.starts_rule('d'));
        assert_eq!(c.convert_flush("ｄ"), "ｄ");
    }
}

#[cfg(test)]
mod span_tests {
    use super::*;

    fn spans(raw: &str) -> Vec<(String, String)> {
        RomajiConverter::new()
            .convert(raw)
            .spans
            .into_iter()
            .map(|s| (s.text, s.keys))
            .collect()
    }

    fn pair(text: &str, keys: &str) -> (String, String) {
        (text.to_string(), keys.to_string())
    }

    #[test]
    fn each_rule_carries_its_keys() {
        assert_eq!(spans("kyoha"), [pair("きょ", "kyo"), pair("は", "ha")]);
        assert_eq!(
            spans("sinnbunn"),
            [
                pair("し", "si"),
                pair("ん", "nn"),
                pair("ぶ", "bu"),
                pair("ん", "nn")
            ]
        );
    }

    #[test]
    fn context_rules_take_the_key_they_consumed() {
        // っ is the first k of `kk`, ん the n of `nk`.
        assert_eq!(spans("kka"), [pair("っ", "k"), pair("か", "ka")]);
        assert_eq!(
            spans("kanka"),
            [pair("か", "ka"), pair("ん", "n"), pair("か", "ka")]
        );
    }

    #[test]
    fn doubled_apostrophe_is_not_a_sokuon() {
        // `t'` starts `t'i`; when it does not complete, `''` passes through
        // like any other keys, as it does after `k`.
        let c = RomajiConverter::new();
        assert_eq!(
            c.convert_flush("t''"),
            c.convert_flush("t").to_string() + "＇＇"
        );
        assert_eq!(c.convert("t''").text, "t''");
        assert_eq!(c.convert("k''").text, "k''");
        assert_eq!(c.convert("tta").text, "った");
    }

    #[test]
    fn passthrough_keys_are_their_own_span() {
        assert_eq!(spans("a1k"), [pair("あ", "a"), pair("1", "1")]);
    }

    #[test]
    fn spans_concatenate_to_text_and_consumed_input() {
        let c = RomajiConverter::new();
        for raw in [
            "kyouhaiitenkidesune",
            "tyottomatte",
            "a,b.c",
            "nnn",
            "xtu",
            "yk1",
            // Keys outside ASCII take more than one byte: positions count
            // characters, so they must not shift what is cut around them.
            "k¥a",
            "¥kyo",
            "nｋa",
            "ka€n",
        ] {
            let out = c.convert(raw);
            let text: String = out.spans.iter().map(|s| s.text.as_str()).collect();
            let keys: String = out.spans.iter().map(|s| s.keys.as_str()).collect();
            assert_eq!(text, out.text, "{raw}");
            assert_eq!(format!("{keys}{}", out.pending), raw, "{raw}");
        }
    }

    #[test]
    fn keys_are_recorded_lowercased() {
        // Rules match case-insensitively, and the keys are kept as matched.
        // The IME lowercases every key before converting, so nothing it
        // types is lost; an uppercase key starts alphabet input instead.
        assert_eq!(spans("Ka"), [pair("か", "ka")]);
        assert_eq!(spans("KYo"), [pair("きょ", "kyo")]);
    }

    #[test]
    fn flush_spans_settle_what_is_left() {
        // What `convert` leaves pending is the start of a rule that never
        // finished (`ky` of `kya`), so it settles key by key.
        let c = RomajiConverter::new();
        let flushed = |pending: &str| -> Vec<_> {
            c.flush_spans(pending)
                .into_iter()
                .map(|s| (s.text, s.keys))
                .collect()
        };
        assert_eq!(c.convert("kaky").pending, "ky");
        assert_eq!(flushed("ky"), [pair("k", "k"), pair("y", "y")]);
        assert_eq!(flushed("n"), [pair("n", "n")]);
        assert!(flushed("").is_empty());
    }
}
