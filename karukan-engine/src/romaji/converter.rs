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
}

/// Stateless romaji-to-hiragana converter.
///
/// Holds the rule trie and the width rules; each call derives its result
/// from the full raw input, so the caller owns all editing state.
#[derive(Debug)]
pub struct RomajiConverter {
    trie: TrieNode,
    width: WidthRules,
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
        let mut scratch = Scratch {
            trie: &self.trie,
            buffer: String::new(),
            output: String::new(),
        };
        for ch in raw.chars() {
            scratch.push(ch);
        }
        Converted {
            text: scratch.output,
            pending: scratch.buffer,
        }
    }

    /// Force-convert leftover pending input (`ltu` → っ); characters with no
    /// rule pass through literally (a trailing `n` stays `n`).
    pub fn flush_pending(&self, pending: &str) -> String {
        let mut buffer = pending.to_string();
        let mut result = String::new();

        while !buffer.is_empty() {
            let search = self.trie.search_longest(&buffer);
            if let Some(h) = search.output {
                result.push_str(h);
                buffer.drain(..search.matched_len);
            } else {
                result.push(buffer.remove(0));
            }
        }

        result
    }

    /// Convert then flush the leftover: the committed form of `raw`, at the
    /// configured width. [`Self::convert`] leaves the width alone because
    /// part of its output is still live keystrokes; here everything settles.
    pub fn convert_flush(&self, raw: &str) -> String {
        let Converted { mut text, pending } = self.convert(raw);
        text.push_str(&self.flush_pending(&pending));
        self.width.apply_str(&text)
    }

    /// Whether `ch` can begin a conversion rule (`k`, `y`, `n` — a later
    /// keystroke could still complete a conversion with it; `1` cannot).
    pub fn starts_rule(&self, ch: char) -> bool {
        self.trie.children.contains_key(&ch)
    }

    /// Kana the pending romaji can still become: the outputs of every rule
    /// whose key extends `pending` (`d` → だ/ぢ/づ/で/ど/ぢゃ…; `n` includes
    /// ん via `nn`/`n'`). Empty when `pending` is empty or cannot reach any
    /// rule (`yk`). Used to narrow predictive dictionary lookups while a
    /// romaji tail is being typed.
    pub fn pending_expansions(&self, pending: &str) -> Vec<String> {
        if pending.is_empty() {
            return Vec::new();
        }
        let Some(node) = pending
            .chars()
            .try_fold(&self.trie, |node, ch| node.children.get(&ch))
        else {
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

/// Working state for one `convert` call.
struct Scratch<'a> {
    trie: &'a TrieNode,
    buffer: String,
    output: String,
}

impl Scratch<'_> {
    /// Push a character and attempt conversion
    fn push(&mut self, ch: char) {
        // Handle uppercase by converting to lowercase
        let ch = ch.to_ascii_lowercase();

        // Add to buffer
        self.buffer.push(ch);

        // Try to convert
        self.try_convert();
    }

    /// Recursively process the buffer left after a conversion.
    fn convert_remainder(&mut self) {
        if !self.buffer.is_empty() {
            self.try_convert();
        }
    }

    /// Try to convert the current buffer
    fn try_convert(&mut self) {
        // Special case: "nn" + another character
        // "nn" is ALWAYS treated as a single ん, regardless of what follows.
        // This matches IME behavior where "nn" is the deliberate way to enter ん.
        // Examples:
        // - "nna" -> "んa" (nn -> ん, a continues in buffer)
        // - "nni" -> "んi" (nn -> ん, i continues in buffer)
        // - "nnk" -> "んk" (nn -> ん, k continues in buffer)
        let chars: Vec<char> = self.buffer.chars().collect();
        let char_count = chars.len();
        if char_count >= 3 && chars[0] == 'n' && chars[1] == 'n' {
            // "nn" is always a single ん, rest is processed separately
            self.buffer.drain(..2);
            self.output.push('ん');
            return self.convert_remainder();
        }

        // Special case: 'n' before consonant -> ん
        if char_count >= 2 {
            let last = chars[char_count - 1];
            let second_last = chars[char_count - 2];

            // N before consonant rule: 'n' + consonant (including 'n') -> ん + consonant
            // Exception: exactly "nn" (length 2) should wait for next char
            if second_last == 'n'
                && !matches!(last, 'a' | 'i' | 'u' | 'e' | 'o' | 'y' | '\'')
                && !(char_count == 2 && last == 'n')
            // Exclude exactly "nn"
            {
                // Convert the 'n' at position len-2 to 'ん'
                // Keep everything before that position plus the last character
                let prefix: String = chars.iter().take(char_count - 2).collect();
                self.buffer = format!("{}{}", prefix, last);
                self.output.push('ん');
                return self.convert_remainder();
            }

            // Double consonant rule: same consonant twice (except 'n') -> っ + consonant.
            // Only when the pair is the whole buffer; with a longer prefix
            // (`ty` + `y`) decomposition below keeps the prefix alive, so
            // `tyy` becomes tっ+y instead of silently dropping the t.
            if char_count == 2
                && last == second_last
                && !matches!(last, 'a' | 'i' | 'u' | 'e' | 'o' | 'n')
            {
                // Convert to sokuon and keep the last consonant
                self.buffer = last.to_string();
                self.output.push('っ');
                return;
            }
        }

        // Search for longest match
        let search = self.trie.search_longest(&self.buffer);

        if let Some(hiragana) = search.output {
            // Found a match
            if search.has_continuation && search.matched_len == self.buffer.len() {
                // This is a valid conversion, but there might be longer matches
                // Wait for more input unless it's "n'" or "nn"
                if self.buffer == "n'" || self.buffer == "nn" {
                    // Special case: always convert n' and nn immediately
                    self.output.push_str(hiragana);
                    self.buffer.clear();
                }
                // Otherwise, wait for more input
            } else {
                // Convert and keep remainder in buffer
                self.output.push_str(hiragana);
                self.buffer.drain(..search.matched_len);
                self.convert_remainder();
            }
        } else if search.matched_len == 0 {
            // No match at all
            // Check if the first character could start a valid conversion
            let Some(first_char) = self.buffer.chars().next() else {
                return;
            };
            let first_char_has_children = self.trie.children.contains_key(&first_char);

            if first_char_has_children {
                // Check if the current buffer could still lead to a match
                // by walking the trie to see if we're on a valid path
                let mut node = self.trie;
                let mut on_valid_path = true;
                for ch in self.buffer.chars() {
                    if let Some(child) = node.children.get(&ch) {
                        node = child;
                    } else {
                        on_valid_path = false;
                        break;
                    }
                }

                if on_valid_path {
                    // We're on a valid path in the trie, keep buffering
                    return;
                }
            }

            // First character doesn't start any rule, or buffer is not on valid path
            let first_search = self.trie.search_longest(&first_char.to_string());

            if let Some(hiragana) = first_search.output {
                // First character has a valid conversion, use it
                self.output.push_str(hiragana);
                self.buffer.drain(..first_search.matched_len);
                self.convert_remainder();
            } else {
                // No possible match, pass through the first character
                self.buffer.remove(0);
                self.output.push(first_char);

                // Try to convert remainder after pass-through
                self.convert_remainder();
            }
        }
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
}
