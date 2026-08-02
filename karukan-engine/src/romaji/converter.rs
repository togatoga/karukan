use super::rules::build_rules;
use super::trie::TrieNode;

/// Result of converting a raw input string.
#[derive(Debug, Clone, PartialEq)]
pub struct Converted {
    /// Converted output: hiragana plus passed-through characters
    pub text: String,
    /// Unresolved trailing input that may still extend to a longer rule
    pub pending: String,
}

/// Events that can occur during conversion
#[derive(Debug, Clone, PartialEq)]
enum ConversionEvent {
    /// Characters were converted to hiragana
    Converted(String),
    /// Character added to buffer, waiting for more input
    Buffered,
    /// No conversion rule found, character passed through
    PassThrough(char),
}

/// Stateless romaji-to-hiragana converter.
///
/// Holds only the rule trie; each call derives its result from the full raw
/// input, so the caller owns all editing state.
#[derive(Debug)]
pub struct RomajiConverter {
    trie: TrieNode,
}

impl RomajiConverter {
    /// Create a new converter with default rules
    pub fn new() -> Self {
        Self {
            trie: build_rules(),
        }
    }

    /// Convert `raw` left to right. `pending` holds the trailing input that
    /// may still combine with future keys (e.g. `k`, `ky`, a lone `n`).
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

    /// Convert then flush the leftover: the committed form of `raw`.
    pub fn convert_flush(&self, raw: &str) -> String {
        let Converted { mut text, pending } = self.convert(raw);
        text.push_str(&self.flush_pending(&pending));
        text
    }

    /// Whether `s` is a strict prefix of some conversion rule (e.g. `k`,
    /// `ky`, `n`) — i.e. more input could still complete a conversion.
    pub fn is_rule_prefix(&self, s: &str) -> bool {
        let mut node = &self.trie;
        for ch in s.chars() {
            match node.children.get(&ch) {
                Some(child) => node = child,
                None => return false,
            }
        }
        !node.children.is_empty()
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
    fn push(&mut self, ch: char) -> ConversionEvent {
        // Handle uppercase by converting to lowercase
        let ch = ch.to_ascii_lowercase();

        // Add to buffer
        self.buffer.push(ch);

        // Try to convert
        self.try_convert()
    }

    /// Convert with the given hiragana and recursively process any remaining buffer.
    /// Returns a Converted event combining the hiragana with any further conversions.
    fn convert_with_remainder(&mut self, hiragana: String) -> ConversionEvent {
        if !self.buffer.is_empty()
            && let ConversionEvent::Converted(next) = self.try_convert()
        {
            return ConversionEvent::Converted(format!("{}{}", hiragana, next));
        }
        ConversionEvent::Converted(hiragana)
    }

    /// Try to convert the current buffer
    fn try_convert(&mut self) -> ConversionEvent {
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
            return self.convert_with_remainder("ん".to_string());
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
                return self.convert_with_remainder("ん".to_string());
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
                return ConversionEvent::Converted("っ".to_string());
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
                    return ConversionEvent::Converted(hiragana.to_string());
                }
                // Otherwise, wait for more input
                return ConversionEvent::Buffered;
            } else {
                // Convert and keep remainder in buffer
                self.output.push_str(hiragana);
                self.buffer.drain(..search.matched_len);
                return self.convert_with_remainder(hiragana.to_string());
            }
        } else if search.matched_len == 0 {
            // No match at all
            // Check if the first character could start a valid conversion
            let Some(first_char) = self.buffer.chars().next() else {
                return ConversionEvent::Buffered;
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
                    return ConversionEvent::Buffered;
                }
            }

            // First character doesn't start any rule, or buffer is not on valid path
            let first_search = self.trie.search_longest(&first_char.to_string());

            if let Some(hiragana) = first_search.output {
                // First character has a valid conversion, use it
                self.output.push_str(hiragana);
                self.buffer.drain(..first_search.matched_len);
                return self.convert_with_remainder(hiragana.to_string());
            } else {
                // No possible match, pass through the first character
                self.buffer.remove(0);
                self.output.push(first_char);

                // Try to convert remainder after pass-through
                if !self.buffer.is_empty() {
                    let next_event = self.try_convert();
                    match next_event {
                        ConversionEvent::Converted(_) | ConversionEvent::PassThrough(_) => {
                            return next_event;
                        }
                        _ => {}
                    }
                }

                return ConversionEvent::PassThrough(first_char);
            }
        }

        ConversionEvent::Buffered
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
    fn test_is_rule_prefix() {
        let c = RomajiConverter::new();
        assert!(c.is_rule_prefix("k"));
        assert!(c.is_rule_prefix("ky"));
        assert!(c.is_rule_prefix("n"));
        assert!(!c.is_rule_prefix("1"));
        assert!(!c.is_rule_prefix("こ"));
        assert!(!c.is_rule_prefix("yk"));
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
