//! Chained conversion tables in the azooKey / hazkey TSV format.
//!
//! Each line is `key<TAB>value`. Unlike the built-in trie, a key is matched
//! against the tail of the composed text, not against raw keystrokes, so a
//! rule's output can take part in a later rule: with `d → ｄ` and
//! `ｄh → か`, typing `dh` yields か, and with `きゃu → きゃあ` the u
//! rewrites an already shown きゃ. Only the tail that some key could still
//! extend stays live; everything before it settles.
//!
//! `{lbracket}` / `{rbracket}` stand for `{` / `}`, as in azooKey.

use std::collections::{BTreeSet, HashMap, HashSet};

#[derive(Debug, Clone, Default)]
pub struct ChainTable {
    rules: HashMap<String, String>,
    /// Proper prefixes of rule keys: a composed tail equal to one of these
    /// may still be rewritten by a later keystroke.
    prefixes: HashSet<String>,
    max_key_chars: usize,
}

impl ChainTable {
    /// Parse a TSV table. Lines without a tab, and empty keys, are ignored.
    pub fn parse_tsv(text: &str) -> Self {
        let mut table = Self::default();
        for line in text.lines() {
            let line = line.trim_end_matches('\r');
            let mut cols = line.split('\t');
            let (Some(key), Some(value)) = (cols.next(), cols.next()) else {
                continue;
            };
            if !key.is_empty() {
                table.insert(unescape(key), unescape(value));
            }
        }
        table
    }

    fn insert(&mut self, key: String, value: String) {
        let chars: Vec<char> = key.chars().collect();
        for len in 1..chars.len() {
            self.prefixes.insert(chars[..len].iter().collect());
        }
        self.max_key_chars = self.max_key_chars.max(chars.len());
        self.rules.insert(key, value);
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Append one keystroke to `buf`, then rewrite the longest rule key
    /// that the buffer now ends with.
    pub fn feed(&self, buf: &mut String, ch: char) {
        buf.push(ch);
        let starts: Vec<usize> = buf.char_indices().map(|(i, _)| i).collect();
        let n = starts.len();
        for len in (1..=self.max_key_chars.min(n)).rev() {
            let at = starts[n - len];
            if let Some(value) = self.rules.get(&buf[at..]) {
                buf.truncate(at);
                buf.push_str(value);
                return;
            }
        }
    }

    /// Split composed text into the settled part and the live tail: the
    /// longest suffix that is a proper prefix of some key.
    pub fn split_live<'a>(&self, buf: &'a str) -> (&'a str, &'a str) {
        let starts: Vec<usize> = buf.char_indices().map(|(i, _)| i).collect();
        let max = self.max_key_chars.saturating_sub(1).min(starts.len());
        for len in (1..=max).rev() {
            let at = starts[starts.len() - len];
            if self.prefixes.contains(&buf[at..]) {
                return buf.split_at(at);
            }
        }
        (buf, "")
    }

    /// Outputs of every rule a live tail could still turn into.
    pub fn expansions(&self, pending: &str) -> Vec<String> {
        if pending.is_empty() {
            return Vec::new();
        }
        self.rules
            .iter()
            .filter(|(key, _)| key.len() > pending.len() && key.starts_with(pending))
            .map(|(_, value)| value.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

fn unescape(s: &str) -> String {
    s.replace("{lbracket}", "{").replace("{rbracket}", "}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> ChainTable {
        ChainTable::parse_tsv(
            "d\tｄ\nｄh\tか\nｄu\tきゃ\nきゃu\tきゃあ\nv\tｖ\nｖ;\tで\nf\tｆ\nでｆd\tですか\n{lbracket}\t『\n",
        )
    }

    fn run(t: &ChainTable, keys: &str) -> String {
        let mut buf = String::new();
        keys.chars().for_each(|c| t.feed(&mut buf, c));
        buf
    }

    #[test]
    fn chains_through_outputs() {
        let t = table();
        assert_eq!(run(&t, "dh"), "か");
        assert_eq!(run(&t, "du"), "きゃ");
        assert_eq!(run(&t, "duu"), "きゃあ");
        assert_eq!(run(&t, "v;fd"), "ですか");
        assert_eq!(run(&t, "{"), "『");
    }

    #[test]
    fn live_tail() {
        let t = table();
        assert_eq!(t.split_live("かｄ"), ("か", "ｄ"));
        assert_eq!(t.split_live("かきゃ"), ("か", "きゃ"));
        assert_eq!(t.split_live("かで"), ("か", "で"));
        assert_eq!(t.split_live("かあ"), ("かあ", ""));
    }
}
