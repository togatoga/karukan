//! Date rewriter — turn a registered phrase (きょう, あした, いま, …) into
//! date/time candidates rendered from the phrase's format strings.
//!
//! A phrase carries a day offset from today and any number of formats.
//! Formats are plain text with `{TOKEN}` / `{TOKEN:style}` placeholders
//! (`{YEAR}/{MONTH}/{DATE}` → `2026/09/06`); `{{}` is a literal `{`.
//! Numeric tokens are zero-padded by default, `:bare` drops the padding
//! (`9月`), `:kanji` renders kanji numerals (`九月`, `二〇二六`). A format
//! referencing an unknown token, or an era before the table starts, is
//! skipped rather than shown broken. User-facing doc: `docs/date.md`.

use std::collections::BTreeMap;

use chrono::{Datelike, NaiveDate, NaiveDateTime, TimeDelta, Timelike};
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::number::{to_kanji, to_kanji_digits};
use super::{RewriteOutput, Rewriter};

/// The `[date]` section of config.toml.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DateConfig {
    /// Formats shared by every phrase that doesn't bring its own, so the
    /// date phrases are one `offset_days` line each.
    #[serde(default)]
    pub formats: Vec<String>,
    /// Phrases keyed by reading (`[date.phrase."きょう"]`).
    #[serde(default)]
    pub phrase: BTreeMap<String, DatePhrase>,
}

/// One phrase: a day offset from today, and optionally its own formats
/// (omitted → the shared `[date]` list; `[]` disables the phrase).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatePhrase {
    /// Days added to today; negative for the past.
    #[serde(default)]
    pub offset_days: i64,
    pub formats: Option<Vec<String>>,
}

const WEEKDAYS: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];

/// Era name and year for a date; `None` before the table starts.
fn era(date: NaiveDate) -> Option<(&'static str, i32)> {
    const ERAS: [(&str, i32, u32, u32); 5] = [
        ("令和", 2019, 5, 1),
        ("平成", 1989, 1, 8),
        ("昭和", 1926, 12, 25),
        ("大正", 1912, 7, 30),
        ("明治", 1868, 10, 23),
    ];
    for (name, y, m, d) in ERAS {
        let start = NaiveDate::from_ymd_opt(y, m, d).expect("static era table");
        if date >= start {
            return Some((name, date.year() - y + 1));
        }
    }
    None
}

/// Render one placeholder. `None` marks the whole format as unrenderable
/// (unknown token/style, or no era for the date).
fn render_token(token: &str, style: Option<&str>, at: NaiveDateTime) -> Option<String> {
    let kanji = |n: u32| to_kanji(&n.to_string(), false);
    // A BC year (reachable via a huge negative offset_days) has no
    // rendering; the sign would also break to_kanji_digits.
    let year = || u32::try_from(at.year()).ok();
    match (token, style) {
        ("YEAR", None) => Some(format!("{:04}", year()?)),
        ("YEAR", Some("kanji")) => Some(to_kanji_digits(&format!("{:04}", year()?))),
        ("MONTH", None) => Some(format!("{:02}", at.month())),
        ("MONTH", Some("bare")) => Some(at.month().to_string()),
        ("MONTH", Some("kanji")) => kanji(at.month()),
        ("DATE", None) => Some(format!("{:02}", at.day())),
        ("DATE", Some("bare")) => Some(at.day().to_string()),
        ("DATE", Some("kanji")) => kanji(at.day()),
        ("HOUR", None) => Some(format!("{:02}", at.hour())),
        ("HOUR", Some("bare")) => Some(at.hour().to_string()),
        ("HOUR", Some("kanji")) => kanji(at.hour()),
        ("MINUTE", None) => Some(format!("{:02}", at.minute())),
        ("MINUTE", Some("bare")) => Some(at.minute().to_string()),
        ("MINUTE", Some("kanji")) => kanji(at.minute()),
        ("HOUR12", None) => Some((at.hour() % 12).to_string()),
        ("HOUR12", Some("kanji")) => kanji(at.hour() % 12),
        ("AMPM", None) => Some(if at.hour() < 12 { "午前" } else { "午後" }.to_string()),
        ("ERA", None) => Some(era(at.date())?.0.to_string()),
        ("ERA_YEAR", None) => Some(era(at.date())?.1.to_string()),
        ("ERA_YEAR", Some("kanji")) => match era(at.date())?.1 {
            1 => Some("元".to_string()),
            y => to_kanji(&y.to_string(), false),
        },
        ("WEEKDAY", None) => {
            Some(WEEKDAYS[at.weekday().num_days_from_monday() as usize].to_string())
        }
        _ => None,
    }
}

/// One format → one candidate; `None` skips the format.
fn render(format: &str, at: NaiveDateTime) -> Option<RewriteOutput> {
    let mut out = String::new();
    let mut has_date = false;
    let mut has_time = false;
    let mut chars = format.chars();
    while let Some(c) = chars.next() {
        if c != '{' {
            out.push(c);
            continue;
        }
        let mut spec = String::new();
        loop {
            match chars.next() {
                Some('}') => break,
                Some(c) => spec.push(c),
                // Unclosed placeholder: drop the format.
                None => return None,
            }
        }
        if spec == "{" {
            out.push('{');
            continue;
        }
        let (token, style) = spec
            .split_once(':')
            .map_or((spec.as_str(), None), |(t, s)| (t, Some(s)));
        match token {
            "YEAR" | "MONTH" | "DATE" | "ERA" | "ERA_YEAR" | "WEEKDAY" => has_date = true,
            _ => has_time = true,
        }
        out.push_str(&render_token(token, style, at)?);
    }
    let desc = if has_time && !has_date {
        "時刻"
    } else if has_time {
        "日時"
    } else {
        "日付"
    };
    Some((out, Some(desc.to_string())))
}

pub struct DateRewriter {
    config: DateConfig,
}

impl DateRewriter {
    pub fn new(config: DateConfig) -> Self {
        Self { config }
    }

    /// Candidates for `reading` as of `now`; the clock is a parameter so
    /// tests can pin it.
    fn candidates_at(&self, reading: &str, now: NaiveDateTime) -> Vec<RewriteOutput> {
        let Some(phrase) = self.config.phrase.get(reading) else {
            return Vec::new();
        };
        let Some(at) =
            TimeDelta::try_days(phrase.offset_days).and_then(|delta| now.checked_add_signed(delta))
        else {
            return Vec::new();
        };
        phrase
            .formats
            .as_ref()
            .unwrap_or(&self.config.formats)
            .iter()
            .filter_map(|format| {
                let rendered = render(format, at);
                if rendered.is_none() {
                    debug!("date format skipped: {format:?}");
                }
                rendered
            })
            .collect()
    }
}

impl Rewriter for DateRewriter {
    fn name(&self) -> &'static str {
        "date"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        self.candidates_at(candidate, chrono::Local::now().naive_local())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    fn at(y: i32, m: u32, d: u32, h: u32, min: u32) -> NaiveDateTime {
        NaiveDate::from_ymd_opt(y, m, d)
            .unwrap()
            .and_hms_opt(h, min, 0)
            .unwrap()
    }

    fn rewriter(reading: &str, offset_days: i64, formats: &[&str]) -> DateRewriter {
        let mut phrase = BTreeMap::new();
        phrase.insert(
            reading.to_string(),
            DatePhrase {
                offset_days,
                formats: Some(formats.iter().map(|f| f.to_string()).collect()),
            },
        );
        DateRewriter::new(DateConfig {
            formats: Vec::new(),
            phrase,
        })
    }

    #[test]
    fn padded_bare_and_kanji_styles() {
        let r = rewriter(
            "きょう",
            0,
            &[
                "{YEAR}/{MONTH}/{DATE}",
                "{YEAR}年{MONTH:bare}月{DATE:bare}日",
                "{YEAR:kanji}年{MONTH:kanji}月{DATE:kanji}日",
            ],
        );
        let out = r.candidates_at("きょう", at(2026, 9, 6, 15, 30));
        assert_eq!(
            texts(&out),
            vec!["2026/09/06", "2026年9月6日", "二〇二六年九月六日"]
        );
    }

    #[test]
    fn kanji_uses_positional_reading_for_small_numbers() {
        let r = rewriter("きょう", 0, &["{MONTH:kanji}月{DATE:kanji}日"]);
        let out = r.candidates_at("きょう", at(2026, 12, 16, 0, 0));
        assert_eq!(texts(&out), vec!["十二月十六日"]);
    }

    #[test]
    fn era_and_boundary() {
        let r = rewriter(
            "きょう",
            0,
            &["{ERA}{ERA_YEAR}年", "{ERA}{ERA_YEAR:kanji}年"],
        );
        let out = r.candidates_at("きょう", at(2026, 9, 6, 0, 0));
        assert_eq!(texts(&out), vec!["令和8年", "令和八年"]);
        // 令和 starts 2019-05-01; the day before is 平成31, the first year is 元.
        let out = r.candidates_at("きょう", at(2019, 4, 30, 0, 0));
        assert_eq!(texts(&out), vec!["平成31年", "平成三十一年"]);
        let out = r.candidates_at("きょう", at(2019, 5, 1, 0, 0));
        assert_eq!(texts(&out), vec!["令和1年", "令和元年"]);
    }

    #[test]
    fn era_before_meiji_skips_format() {
        let r = rewriter("きょう", 0, &["{ERA}{ERA_YEAR}年", "{YEAR}年"]);
        let out = r.candidates_at("きょう", at(1868, 10, 22, 0, 0));
        assert_eq!(texts(&out), vec!["1868年"]);
    }

    #[test]
    fn weekday() {
        let r = rewriter("きょう", 0, &["{WEEKDAY}曜日"]);
        // 2026-09-06 is a Sunday.
        let out = r.candidates_at("きょう", at(2026, 9, 6, 0, 0));
        assert_eq!(texts(&out), vec!["日曜日"]);
    }

    #[test]
    fn time_tokens() {
        let r = rewriter(
            "いま",
            0,
            &[
                "{HOUR}:{MINUTE}",
                "{HOUR:bare}時{MINUTE:bare}分",
                "{AMPM}{HOUR12}時{MINUTE:bare}分",
            ],
        );
        let out = r.candidates_at("いま", at(2026, 9, 6, 5, 8));
        assert_eq!(texts(&out), vec!["05:08", "5時8分", "午前5時8分"]);
        // Midnight is 午前0時, noon is 午後0時.
        let out = r.candidates_at("いま", at(2026, 9, 6, 0, 5));
        assert_eq!(texts(&out)[2], "午前0時5分");
        let out = r.candidates_at("いま", at(2026, 9, 6, 12, 30));
        assert_eq!(texts(&out)[2], "午後0時30分");
    }

    #[test]
    fn offset_crosses_month_and_year() {
        let r = rewriter("あした", 1, &["{YEAR}/{MONTH}/{DATE}"]);
        let out = r.candidates_at("あした", at(2026, 12, 31, 0, 0));
        assert_eq!(texts(&out), vec!["2027/01/01"]);
        let r = rewriter("きのう", -1, &["{YEAR}/{MONTH}/{DATE}"]);
        let out = r.candidates_at("きのう", at(2026, 1, 1, 0, 0));
        assert_eq!(texts(&out), vec!["2025/12/31"]);
    }

    #[test]
    fn literal_brace_and_plain_text() {
        let r = rewriter("きょう", 0, &["{{}{YEAR}}"]);
        let out = r.candidates_at("きょう", at(2026, 9, 6, 0, 0));
        assert_eq!(texts(&out), vec!["{2026}"]);
    }

    #[test]
    fn broken_formats_are_skipped() {
        let r = rewriter(
            "きょう",
            0,
            &["{FOO}", "{YEAR:bare}", "{YEAR", "{YEAR}/{MONTH}/{DATE}"],
        );
        let out = r.candidates_at("きょう", at(2026, 9, 6, 0, 0));
        assert_eq!(texts(&out), vec!["2026/09/06"]);
    }

    #[test]
    fn descriptions_derive_from_tokens() {
        let r = rewriter(
            "にちじ",
            0,
            &[
                "{YEAR}/{MONTH}/{DATE}",
                "{HOUR}:{MINUTE}",
                "{YEAR}/{MONTH}/{DATE} {HOUR}:{MINUTE}",
            ],
        );
        let out = r.candidates_at("にちじ", at(2026, 9, 6, 15, 30));
        assert_eq!(desc(&out, "2026/09/06"), Some("日付".to_string()));
        assert_eq!(desc(&out, "15:30"), Some("時刻".to_string()));
        assert_eq!(desc(&out, "2026/09/06 15:30"), Some("日時".to_string()));
    }

    #[test]
    fn unregistered_reading_and_empty_formats() {
        let r = rewriter("きょう", 0, &["{YEAR}"]);
        assert!(r.candidates_at("あした", at(2026, 9, 6, 0, 0)).is_empty());
        let r = rewriter("きょう", 0, &[]);
        assert!(r.candidates_at("きょう", at(2026, 9, 6, 0, 0)).is_empty());
    }

    #[test]
    fn phrase_without_formats_uses_the_shared_list() {
        let mut phrase = BTreeMap::new();
        phrase.insert(
            "あした".to_string(),
            DatePhrase {
                offset_days: 1,
                formats: None,
            },
        );
        let r = DateRewriter::new(DateConfig {
            formats: vec!["{YEAR}/{MONTH}/{DATE}".to_string()],
            phrase,
        });
        let out = r.candidates_at("あした", at(2026, 9, 5, 0, 0));
        assert_eq!(texts(&out), vec!["2026/09/06"]);
        // An explicit empty list disables the phrase instead of falling back.
        let mut phrase = BTreeMap::new();
        phrase.insert(
            "あした".to_string(),
            DatePhrase {
                offset_days: 1,
                formats: Some(Vec::new()),
            },
        );
        let r = DateRewriter::new(DateConfig {
            formats: vec!["{YEAR}".to_string()],
            phrase,
        });
        assert!(r.candidates_at("あした", at(2026, 9, 5, 0, 0)).is_empty());
    }

    #[test]
    fn absurd_offset_is_dropped() {
        let r = rewriter("きょう", i64::MAX, &["{YEAR}"]);
        assert!(r.candidates_at("きょう", at(2026, 9, 6, 0, 0)).is_empty());
    }

    #[test]
    fn bc_year_skips_year_formats() {
        // A negative offset_days can land in a proleptic BC year; the year
        // formats are skipped (kanji digits would panic on the sign) while
        // year-free formats still render.
        let r = rewriter(
            "きょう",
            0,
            &["{YEAR}/{MONTH}/{DATE}", "{YEAR:kanji}年", "{MONTH:bare}月"],
        );
        let out = r.candidates_at("きょう", at(-1, 1, 2, 0, 0));
        assert_eq!(texts(&out), vec!["1月"]);
    }
}
