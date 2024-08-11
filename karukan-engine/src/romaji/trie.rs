use std::collections::HashMap;

/// Result of a trie search operation
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResult<'a> {
    /// Number of characters matched from the input
    pub matched_len: usize,
    /// The output string if a complete match was found
    pub output: Option<&'a str>,
    /// Whether there are longer potential matches beyond the current match
    pub has_continuation: bool,
}

/// A node in the romaji conversion trie
#[derive(Debug, Clone)]
pub struct TrieNode {
    /// Output hiragana if this is a terminal node
    pub output: Option<String>,
    /// Child nodes
    pub children: HashMap<char, TrieNode>,
}

impl TrieNode {
    /// Create a new empty trie node
    pub fn new() -> Self {
        Self {
            output: None,
            children: HashMap::new(),
        }
    }

    /// Insert a romaji -> hiragana rule into the trie
    pub fn insert(&mut self, romaji: &str, hiragana: &str) {
        let mut node = self;
        for ch in romaji.chars() {
            node = node.children.entry(ch).or_default();
        }
        node.output = Some(hiragana.to_string());
    }

    /// Search for the longest matching prefix in the trie
    pub fn search_longest(&self, input: &str) -> SearchResult<'_> {
        let mut node = self;
        let mut last_match: Option<(usize, &str)> = None;
        let mut has_continuation = false;

        for (idx, ch) in input.chars().enumerate() {
            if let Some(child) = node.children.get(&ch) {
                node = child;
                if let Some(ref output) = node.output {
                    last_match = Some((idx + 1, output.as_str()));
                }
                has_continuation = !node.children.is_empty();
            } else {
                break;
            }
        }

        match last_match {
            Some((len, output)) => SearchResult {
                matched_len: len,
                output: Some(output),
                has_continuation,
            },
            None => SearchResult {
                matched_len: 0,
                output: None,
                has_continuation: !self.children.is_empty(),
            },
        }
    }
}

impl Default for TrieNode {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trie_basic() {
        let mut trie = TrieNode::new();
        trie.insert("ka", "か");
        trie.insert("ki", "き");

        let result = trie.search_longest("ka");
        assert_eq!(result.matched_len, 2);
        assert_eq!(result.output.unwrap(), "か");

        let result = trie.search_longest("ki");
        assert_eq!(result.matched_len, 2);
        assert_eq!(result.output.unwrap(), "き");
    }

    #[test]
    fn test_trie_longest_match() {
        let mut trie = TrieNode::new();
        trie.insert("k", "k");
        trie.insert("ka", "か");
        trie.insert("kya", "きゃ");

        let result = trie.search_longest("kya");
        assert_eq!(result.matched_len, 3);
        assert_eq!(result.output.unwrap(), "きゃ");
        assert!(!result.has_continuation);
    }

    #[test]
    fn test_trie_continuation() {
        let mut trie = TrieNode::new();
        trie.insert("ka", "か");
        trie.insert("kan", "かん");

        let result = trie.search_longest("ka");
        assert_eq!(result.matched_len, 2);
        assert_eq!(result.output.unwrap(), "か");
        assert!(result.has_continuation); // "kan" is a longer match
    }
}
