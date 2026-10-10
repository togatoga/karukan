//! Single-kanji rewriter: the reading as one kanji at a time (たか → 高,
//! 嵩, 鷹, …), so a character the model and the dictionaries never
//! offer is still reachable. Behind the reading's own symbols and emoji in
//! the chain, since the list is exhaustive.
//!
//! The data is `data/single_kanji.yml`, hand-maintained, one entry per
//! kanji:
//!
//! ```yaml
//! entries:
//!   - char: 髙
//!     readings: [こう, たか]
//!     description: 高の異体字
//! ```
//!
//! For one reading the entries higher in the file come out first. A kanji
//! listed twice is merged (readings from both, the first description
//! wins). The description is the candidate's annotation; [`description`]
//! lets another source's candidate borrow it when it is the same kanji.

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::Deserialize;

use super::{RewriteOutput, Rewriter};

const SINGLE_KANJI_YAML: &str = include_str!("../../data/single_kanji.yml");

#[derive(Deserialize)]
struct SingleKanjiEntry {
    char: String,
    #[serde(default)]
    readings: Vec<String>,
    #[serde(default)]
    description: Option<String>,
}

#[derive(Deserialize)]
struct SingleKanjiFile {
    #[serde(default)]
    entries: Vec<SingleKanjiEntry>,
}

#[derive(Default)]
struct Table {
    /// reading → kanji in file order.
    by_reading: HashMap<String, Vec<String>>,
    /// kanji → annotation.
    descriptions: HashMap<String, String>,
}

static TABLE: LazyLock<Table> = LazyLock::new(|| {
    let file: SingleKanjiFile =
        serde_yaml::from_str(SINGLE_KANJI_YAML).expect("single_kanji.yml must be valid YAML");
    Table::from(file)
});

impl From<SingleKanjiFile> for Table {
    fn from(file: SingleKanjiFile) -> Self {
        let mut table = Table::default();
        for entry in file.entries {
            for reading in entry.readings {
                let kanji = table.by_reading.entry(reading).or_default();
                if !kanji.contains(&entry.char) {
                    kanji.push(entry.char.clone());
                }
            }
            if let Some(desc) = entry.description {
                table.descriptions.entry(entry.char).or_insert(desc);
            }
        }
        table
    }
}

impl Table {
    /// Every kanji for `reading` (exact match) with its annotation.
    fn rewrite(&self, reading: &str) -> Vec<RewriteOutput> {
        self.by_reading
            .get(reading)
            .into_iter()
            .flatten()
            .map(|kanji| (kanji.clone(), self.descriptions.get(kanji).cloned()))
            .collect()
    }
}

/// Annotation for a kanji (`髙` → `高の異体字`), if the data has one.
pub fn description(text: &str) -> Option<&'static str> {
    TABLE.descriptions.get(text).map(String::as_str)
}

/// Rewriter emitting every single kanji for the typed reading.
#[derive(Default)]
pub struct SingleKanjiRewriter;

impl Rewriter for SingleKanjiRewriter {
    fn name(&self) -> &'static str {
        "single_kanji"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        TABLE.rewrite(candidate)
    }

    fn is_catalog(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    #[test]
    fn reading_lists_its_kanji() {
        let out = texts(&SingleKanjiRewriter.rewrite("たか"));
        assert!(out.len() > 10);
        for kanji in ["高", "鷹", "孝", "嵩"] {
            assert!(out.contains(&kanji.to_string()), "{kanji} missing");
        }
        assert_eq!(texts(&SingleKanjiRewriter.rewrite("あいだ")), ["間"]);
        assert!(SingleKanjiRewriter.rewrite("あいだに").is_empty());
    }

    #[test]
    fn variant_carries_its_note() {
        let out = SingleKanjiRewriter.rewrite("あ");
        assert_eq!(desc(&out, "亜"), None);
        assert_eq!(desc(&out, "亞").as_deref(), Some("亜の旧字体"));
        let taka = SingleKanjiRewriter.rewrite("たか");
        assert_eq!(desc(&taka, "髙").as_deref(), Some("高の異体字"));
        assert_eq!(description("髙"), Some("高の異体字"));
        assert_eq!(description("高"), None);
    }

    fn table(yaml: &str) -> Table {
        Table::from(serde_yaml::from_str::<SingleKanjiFile>(yaml).unwrap())
    }

    #[test]
    fn file_order_is_candidate_order() {
        let t = table(
            "entries:
  - char: 乙
    readings: [あ]
  - char: 甲
    readings: [あ, こう]
  - char: 丙
    readings: [あ]
",
        );
        assert_eq!(texts(&t.rewrite("あ")), ["乙", "甲", "丙"]);
        assert_eq!(texts(&t.rewrite("こう")), ["甲"]);
    }

    #[test]
    fn duplicate_entries_merge() {
        let t = table(
            "entries:
  - char: 高
    readings: [こう, こう]
  - char: 髙
    readings: [こう]
  - char: 高
    readings: [たか]
    description: 旧字体ではない
  - char: 髙
    description: 高の異体字
",
        );
        assert_eq!(texts(&t.rewrite("こう")), ["高", "髙"]);
        assert_eq!(texts(&t.rewrite("たか")), ["高"]);
        assert_eq!(
            desc(&t.rewrite("こう"), "高").as_deref(),
            Some("旧字体ではない")
        );
        assert_eq!(t.descriptions["髙"], "高の異体字");
    }
}
