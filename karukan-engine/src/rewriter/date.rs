//! Date rewriter — turns relative-day readings into calendar dates.
//!
//! When the user types a reading such as `きょう` / `あした` / `きのう`, this
//! rewriter emits the corresponding calendar date rendered with one or more
//! configurable `strftime`-style formats (e.g. `2026-07-13`). Each variant is
//! annotated with the day label (`今日`, `明日`, …) so the candidate window can
//! show what the date refers to.
//!
//! Unlike the other rewriters, `DateRewriter` ignores the *content* of the
//! incoming candidate beyond matching it against a fixed set of readings, and
//! derives its output from the current date instead. The current date is read
//! from an injectable [`Clock`] on every `rewrite()` call — the macOS
//! `karukan-imserver` is a long-lived daemon, so capturing the date once at
//! construction would go stale. Tests inject a fixed clock for determinism.

use std::sync::Arc;

use chrono::format::{Item, StrftimeItems};
use chrono::{Duration, Local, NaiveDate};

use super::{RewriteOutput, Rewriter};

/// Default date formats emitted when the user does not configure their own:
/// an ISO-style `2026-07-13` and a Japanese `2026年7月13日`.
pub const DEFAULT_DATE_FORMATS: &[&str] = &["%Y-%m-%d", "%Y年%-m月%-d日"];

/// Readings recognized by the date rewriter, paired with the day offset from
/// today and the label shown as the candidate annotation.
const READINGS: &[(&str, i64, &str)] = &[
    ("きょう", 0, "今日"),
    ("きのう", -1, "昨日"),
    ("おととい", -2, "一昨日"),
    ("あした", 1, "明日"),
    ("あす", 1, "明日"),
    ("あさって", 2, "明後日"),
];

/// Source of the current local date. Injectable so tests are deterministic.
pub trait Clock: Send + Sync {
    /// Today's date in the local timezone.
    fn today(&self) -> NaiveDate;
}

/// Reads the current date from the system's local clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn today(&self) -> NaiveDate {
        Local::now().date_naive()
    }
}

/// Rewriter that maps relative-day readings to formatted calendar dates.
pub struct DateRewriter {
    clock: Arc<dyn Clock>,
    formats: Vec<String>,
}

impl DateRewriter {
    /// Build a rewriter using the system clock and the given `strftime` formats.
    pub fn new(formats: Vec<String>) -> Self {
        Self {
            clock: Arc::new(SystemClock),
            formats,
        }
    }

    /// Build a rewriter with an explicit clock (used by tests for determinism).
    pub fn with_clock(clock: Arc<dyn Clock>, formats: Vec<String>) -> Self {
        Self { clock, formats }
    }
}

/// Look up the day offset and label for a reading, if it is a date reading.
fn match_reading(reading: &str) -> Option<(i64, &'static str)> {
    READINGS
        .iter()
        .find(|(r, _, _)| *r == reading)
        .map(|(_, offset, label)| (*offset, *label))
}

/// Render `date` with a `strftime` format, returning `None` for an invalid
/// format string instead of panicking (formats come from user config).
fn format_date(date: NaiveDate, fmt: &str) -> Option<String> {
    let items: Vec<Item> = StrftimeItems::new(fmt).collect();
    if items.iter().any(|it| matches!(it, Item::Error)) {
        return None;
    }
    // Format via midnight so any valid specifier (incl. time fields) resolves
    // rather than failing to render.
    let dt = date.and_hms_opt(0, 0, 0)?;
    Some(dt.format_with_items(items.iter()).to_string())
}

impl Rewriter for DateRewriter {
    fn name(&self) -> &'static str {
        "date"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        let Some((offset, label)) = match_reading(candidate) else {
            return Vec::new();
        };
        let Some(date) = self
            .clock
            .today()
            .checked_add_signed(Duration::days(offset))
        else {
            return Vec::new();
        };
        self.formats
            .iter()
            .filter_map(|fmt| format_date(date, fmt))
            .map(|text| (text, Some(label.to_string())))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    struct FixedClock(NaiveDate);
    impl Clock for FixedClock {
        fn today(&self) -> NaiveDate {
            self.0
        }
    }

    fn rewriter_on(date: (i32, u32, u32), formats: &[&str]) -> DateRewriter {
        let clock = Arc::new(FixedClock(
            NaiveDate::from_ymd_opt(date.0, date.1, date.2).unwrap(),
        ));
        DateRewriter::with_clock(clock, formats.iter().map(|s| s.to_string()).collect())
    }

    #[test]
    fn non_date_reading_returns_empty() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d"]);
        assert!(r.rewrite("あいう").is_empty());
        assert!(r.rewrite("").is_empty());
    }

    #[test]
    fn kyou_is_today() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d"]);
        assert_eq!(texts(&r.rewrite("きょう")), vec!["2026-07-13".to_string()]);
    }

    #[test]
    fn kinou_is_yesterday_ashita_is_tomorrow() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d"]);
        assert_eq!(texts(&r.rewrite("きのう")), vec!["2026-07-12".to_string()]);
        assert_eq!(texts(&r.rewrite("あした")), vec!["2026-07-14".to_string()]);
        // `あす` is an alias for tomorrow.
        assert_eq!(texts(&r.rewrite("あす")), vec!["2026-07-14".to_string()]);
    }

    #[test]
    fn ototoi_and_asatte_are_two_day_offsets() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d"]);
        assert_eq!(
            texts(&r.rewrite("おととい")),
            vec!["2026-07-11".to_string()]
        );
        assert_eq!(
            texts(&r.rewrite("あさって")),
            vec!["2026-07-15".to_string()]
        );
    }

    #[test]
    fn offsets_cross_month_boundaries() {
        let r = rewriter_on((2026, 7, 1), &["%Y-%m-%d"]);
        assert_eq!(texts(&r.rewrite("きのう")), vec!["2026-06-30".to_string()]);
    }

    #[test]
    fn multiple_formats_each_emit_a_variant() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d", "%Y年%-m月%-d日"]);
        assert_eq!(
            texts(&r.rewrite("きょう")),
            vec!["2026-07-13".to_string(), "2026年7月13日".to_string()]
        );
    }

    #[test]
    fn label_is_attached_as_description() {
        let r = rewriter_on((2026, 7, 13), &["%Y-%m-%d"]);
        let out = r.rewrite("きょう");
        assert_eq!(desc(&out, "2026-07-13"), Some("今日".to_string()));
        let out = r.rewrite("あした");
        assert_eq!(desc(&out, "2026-07-14"), Some("明日".to_string()));
    }

    #[test]
    fn empty_formats_emit_nothing() {
        let r = rewriter_on((2026, 7, 13), &[]);
        assert!(r.rewrite("きょう").is_empty());
    }

    #[test]
    fn invalid_format_is_skipped_not_panicked() {
        // `%Q` is not a valid strftime specifier; it must be dropped rather
        // than crash the engine, and valid formats still come through.
        let r = rewriter_on((2026, 7, 13), &["%Q", "%Y-%m-%d"]);
        assert_eq!(texts(&r.rewrite("きょう")), vec!["2026-07-13".to_string()]);
    }
}
