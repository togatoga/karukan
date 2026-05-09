//! Number rewriter — expand a numeric candidate into related styles.
//!
//! For a halfwidth or fullwidth arabic candidate, emit: the width pair
//! (`123` ↔ `１２３`), kanji (`百二十三`), daiji (`壱百弐拾参`), roman
//! (`Ⅰ-Ⅻ` / `ⅰ-ⅻ`), circled (`①-㊿`), and radix (`0x…` / `0…` / `0b…`).
//! Each variant carries the per-style description shown as the candidate's
//! right-side comment.
//!
//! Inputs must be pure decimal digits — mixed text like `20世紀` is left
//! alone.

use super::{RewriteOutput, Rewriter};
use crate::kana::{ascii_to_fullwidth_char, fullwidth_to_ascii_char};

// ---------- tables ----------
//
// `*_DIGITS[d]` is the glyph for digit `d` (0..=9). For kanji/daiji, the
// `d == 0` slot is never read by the algorithm — the all-zero case is
// handled separately via `*_ZERO`.
//
// `*_RANKS[pos]` is the suffix for position `pos` within a 4-digit segment
// (0=ones, 1=tens, 2=hundreds, 3=thousands).
//
// `*_BIG_RANKS[i]` is the suffix between 4-digit segments counted from the
// least-significant side (0=ones segment with no suffix, 1=万, 2=億, …).
// Length 7 caps the kanji form at 10^28 − 1.
//
// `ROMAN_*` and `CIRCLED` are value-indexed: `table[n]` is the glyph for
// value `n`. An empty slot means "no glyph for this value" — e.g. there
// is no roman numeral for 0, so `ROMAN_*[0]` is `""`.

const KANJI_DIGITS: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
const OLD_KANJI_DIGITS: [&str; 10] = ["零", "壱", "弐", "参", "四", "五", "六", "七", "八", "九"];
const KANJI_RANKS: [&str; 4] = ["", "十", "百", "千"];
const OLD_KANJI_RANKS: [&str; 4] = ["", "拾", "百", "阡"];
const KANJI_BIG_RANKS: [&str; 7] = ["", "万", "億", "兆", "京", "垓", "秭"];
const OLD_KANJI_BIG_RANKS: [&str; 7] = ["", "萬", "億", "兆", "京", "垓", "秭"];
const KANJI_ZERO: &str = "〇";
const OLD_KANJI_ZERO: &str = "零";

const ROMAN_CAPITAL: [&str; 13] = [
    "", "Ⅰ", "Ⅱ", "Ⅲ", "Ⅳ", "Ⅴ", "Ⅵ", "Ⅶ", "Ⅷ", "Ⅸ", "Ⅹ", "Ⅺ", "Ⅻ",
];
const ROMAN_SMALL: [&str; 13] = [
    "", "ⅰ", "ⅱ", "ⅲ", "ⅳ", "ⅴ", "ⅵ", "ⅶ", "ⅷ", "ⅸ", "ⅹ", "ⅺ", "ⅻ",
];
const CIRCLED: [&str; 51] = [
    "⓪", "①", "②", "③", "④", "⑤", "⑥", "⑦", "⑧", "⑨", "⑩", "⑪", "⑫", "⑬", "⑭", "⑮", "⑯", "⑰", "⑱",
    "⑲", "⑳", "㉑", "㉒", "㉓", "㉔", "㉕", "㉖", "㉗", "㉘", "㉙", "㉚", "㉛", "㉜", "㉝", "㉞",
    "㉟", "㊱", "㊲", "㊳", "㊴", "㊵", "㊶", "㊷", "㊸", "㊹", "㊺", "㊻", "㊼", "㊽", "㊾", "㊿",
];

const DESC_NUMBER: &str = "数字";
const DESC_KANJI: &str = "漢数字";
const DESC_OLD_KANJI: &str = "大字";
const DESC_ROMAN_CAPITAL: &str = "ローマ数字(大文字)";
const DESC_ROMAN_SMALL: &str = "ローマ数字(小文字)";
const DESC_CIRCLED: &str = "丸数字";
const DESC_HEX: &str = "16進数";
const DESC_OCT: &str = "8進数";
const DESC_BIN: &str = "2進数";

// ---------- input gates ----------

/// True iff every character is a halfwidth (`0-9`) or fullwidth (`０-９`)
/// decimal digit, and the string is non-empty.
fn is_pure_digit(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || ('\u{FF10}'..='\u{FF19}').contains(&c))
}

fn to_halfwidth(text: &str) -> String {
    text.chars().map(fullwidth_to_ascii_char).collect()
}

fn to_fullwidth(text: &str) -> String {
    text.chars().map(ascii_to_fullwidth_char).collect()
}

// ---------- kanji rendering ----------

/// Split a digit string into 4-digit big-rank segments, least-significant
/// first: `"1234567890"` → `["7890", "3456", "12"]`.
fn split_big_ranks(digits: &str) -> Vec<&str> {
    let mut segments = Vec::new();
    let bytes = digits.as_bytes();
    let mut end = bytes.len();
    while end > 0 {
        let start = end.saturating_sub(4);
        segments.push(std::str::from_utf8(&bytes[start..end]).expect("ASCII digits"));
        end = start;
    }
    segments
}

/// Render one 4-digit segment as positional kanji (`"1234"` → `"千二百三十四"`).
/// Standard style drops a leading `一` before 十百千; daiji always keeps `壱`.
fn render_kanji_segment(segment: &str, digits: &[&str], ranks: &[&str], old_kanji: bool) -> String {
    let len = segment.len();
    let mut out = String::new();
    for (i, b) in segment.bytes().enumerate() {
        let d = (b - b'0') as usize;
        if d == 0 {
            continue;
        }
        let pos = len - 1 - i;
        let keep_one = old_kanji || pos == 0 || d != 1;
        if keep_one {
            out.push_str(digits[d]);
        }
        out.push_str(ranks[pos]);
    }
    out
}

/// Full kanji form. All-zero input picks the style-specific glyph:
/// `〇` for standard, `零` for daiji. Returns `None` if the input has more
/// big-rank segments than the table can name.
fn to_kanji(digits: &str, old_kanji: bool) -> Option<String> {
    let trimmed = digits.trim_start_matches('0');
    let (digit_table, rank_table, big_rank_table, zero): (&[&str], &[&str], &[&str], &str) =
        if old_kanji {
            (
                &OLD_KANJI_DIGITS,
                &OLD_KANJI_RANKS,
                &OLD_KANJI_BIG_RANKS,
                OLD_KANJI_ZERO,
            )
        } else {
            (&KANJI_DIGITS, &KANJI_RANKS, &KANJI_BIG_RANKS, KANJI_ZERO)
        };
    if trimmed.is_empty() {
        return Some(zero.to_string());
    }
    let segments = split_big_ranks(trimmed);
    if segments.len() > big_rank_table.len() {
        return None;
    }
    let mut out = String::new();
    for (i, seg) in segments.iter().enumerate().rev() {
        let body = render_kanji_segment(seg, digit_table, rank_table, old_kanji);
        if !body.is_empty() {
            out.push_str(&body);
            out.push_str(big_rank_table[i]);
        }
    }
    Some(if out.is_empty() {
        zero.to_string()
    } else {
        out
    })
}

/// Value-indexed lookup. Returns the index iff `table[n]` exists and is
/// non-empty (empty slots are treated as "no glyph for this value").
fn lookup_index(n: u64, table: &[&str]) -> Option<usize> {
    let i = n as usize;
    (i < table.len() && !table[i].is_empty()).then_some(i)
}

// ---------- rewriter ----------

#[derive(Default)]
pub struct NumberRewriter;

impl NumberRewriter {
    pub fn new() -> Self {
        Self
    }
}

impl Rewriter for NumberRewriter {
    fn name(&self) -> &'static str {
        "number"
    }

    fn rewrite(&self, candidate: &str) -> Vec<RewriteOutput> {
        if !is_pure_digit(candidate) {
            return Vec::new();
        }
        let half = to_halfwidth(candidate);
        let full = to_fullwidth(candidate);
        let n = half.parse::<u64>().ok();

        let mut out: Vec<RewriteOutput> = Vec::new();
        let mut push = |text: String, desc: &'static str| {
            if text != candidate && !out.iter().any(|(t, _)| t == &text) {
                out.push((text, Some(desc.to_string())));
            }
        };

        // width pair (数字)
        push(half.clone(), DESC_NUMBER);
        push(full, DESC_NUMBER);

        // kanji (漢数字)
        if let Some(s) = to_kanji(&half, false) {
            push(s, DESC_KANJI);
        }

        // old kanji / daiji (大字)
        if let Some(s) = to_kanji(&half, true) {
            push(s, DESC_OLD_KANJI);
        }

        // roman (Ⅰ-Ⅻ / ⅰ-ⅻ), 1..=12
        if let Some(n) = n
            && let Some(i) = lookup_index(n, &ROMAN_CAPITAL)
        {
            push(ROMAN_CAPITAL[i].to_string(), DESC_ROMAN_CAPITAL);
            push(ROMAN_SMALL[i].to_string(), DESC_ROMAN_SMALL);
        }

        // circled (⓪-㊿), 0..=50
        if let Some(n) = n
            && let Some(i) = lookup_index(n, &CIRCLED)
        {
            push(CIRCLED[i].to_string(), DESC_CIRCLED);
        }

        // radix — gated on u64 fit
        if let Some(n) = n {
            push(format!("0x{:x}", n), DESC_HEX);
            push(format!("0{:o}", n), DESC_OCT);
            push(format!("0b{:b}", n), DESC_BIN);
        }

        out
    }
}

// ---------- tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rewriter::test_util::{desc, texts};

    #[test]
    fn empty_or_non_digit_returns_empty() {
        let r = NumberRewriter::new();
        assert!(r.rewrite("").is_empty());
        assert!(r.rewrite("abc").is_empty());
        assert!(r.rewrite("あ").is_empty());
        assert!(r.rewrite("1a").is_empty());
        assert!(r.rewrite("1.5").is_empty()); // decimals not supported
    }

    #[test]
    fn width_pair() {
        let r = NumberRewriter::new();
        assert!(texts(&r.rewrite("123")).contains(&"１２３".to_string()));
        assert!(texts(&r.rewrite("１２３")).contains(&"123".to_string()));
    }

    #[test]
    fn kanji_basic() {
        let r = NumberRewriter::new();
        // standard kanji 〇 vs daiji 零 for all-zero input
        let t = texts(&r.rewrite("0"));
        assert!(t.contains(&"〇".to_string()));
        assert!(t.contains(&"零".to_string()));
        assert!(texts(&r.rewrite("1")).contains(&"一".to_string()));
        // leading `一` suppressed before 十百千
        assert!(texts(&r.rewrite("10")).contains(&"十".to_string()));
        assert!(texts(&r.rewrite("11")).contains(&"十一".to_string()));
        assert!(texts(&r.rewrite("100")).contains(&"百".to_string()));
    }

    #[test]
    fn kanji_big_ranks() {
        let r = NumberRewriter::new();
        assert!(texts(&r.rewrite("10000")).contains(&"一万".to_string()));
        assert!(texts(&r.rewrite("1234")).contains(&"千二百三十四".to_string()));
        assert!(
            texts(&r.rewrite("12345678")).contains(&"千二百三十四万五千六百七十八".to_string())
        );
        // inner zero ranks are skipped (no `〇万` for 10001)
        assert!(texts(&r.rewrite("10001")).contains(&"一万一".to_string()));
    }

    #[test]
    fn zero_glyph_per_style() {
        let out = NumberRewriter::new().rewrite("0");
        assert_eq!(desc(&out, "〇"), Some("漢数字".to_string()));
        assert_eq!(desc(&out, "零"), Some("大字".to_string()));
        for input in ["00", "000", "0000"] {
            let t = texts(&NumberRewriter::new().rewrite(input));
            assert!(t.contains(&"〇".to_string()));
            assert!(t.contains(&"零".to_string()));
        }
    }

    #[test]
    fn old_kanji_keeps_leading_one() {
        let r = NumberRewriter::new();
        assert!(texts(&r.rewrite("10")).contains(&"壱拾".to_string()));
        assert!(texts(&r.rewrite("100")).contains(&"壱百".to_string()));
        assert!(texts(&r.rewrite("1000")).contains(&"壱阡".to_string()));
        assert!(texts(&r.rewrite("10000")).contains(&"壱萬".to_string()));
    }

    #[test]
    fn roman_only_for_1_through_12() {
        let r = NumberRewriter::new();
        assert!(texts(&r.rewrite("1")).contains(&"Ⅰ".to_string()));
        assert!(texts(&r.rewrite("1")).contains(&"ⅰ".to_string()));
        assert!(texts(&r.rewrite("12")).contains(&"Ⅻ".to_string()));
        let t = texts(&r.rewrite("13"));
        assert!(!t.iter().any(|s| s == "Ⅻ" || s == "ⅻ"));
    }

    #[test]
    fn circled_for_0_through_50() {
        let r = NumberRewriter::new();
        assert!(texts(&r.rewrite("0")).contains(&"⓪".to_string()));
        assert!(texts(&r.rewrite("1")).contains(&"①".to_string()));
        assert!(texts(&r.rewrite("20")).contains(&"⑳".to_string()));
        assert!(texts(&r.rewrite("50")).contains(&"㊿".to_string()));
        assert!(!texts(&r.rewrite("51")).iter().any(|s| s == "㊿"));
    }

    #[test]
    fn radix() {
        let r = NumberRewriter::new();
        let t = texts(&r.rewrite("1234"));
        assert!(t.contains(&"0x4d2".to_string()));
        assert!(t.contains(&"02322".to_string()));
        assert!(t.contains(&"0b10011010010".to_string()));
    }

    #[test]
    fn descriptions_match_mozc() {
        let r = NumberRewriter::new();
        let out = r.rewrite("1234");
        assert_eq!(desc(&out, "千二百三十四"), Some("漢数字".to_string()));
        assert_eq!(desc(&out, "壱阡弐百参拾四"), Some("大字".to_string()));
        assert_eq!(desc(&out, "0x4d2"), Some("16進数".to_string()));
        assert_eq!(desc(&out, "02322"), Some("8進数".to_string()));
        assert_eq!(desc(&out, "0b10011010010"), Some("2進数".to_string()));

        let out = r.rewrite("12");
        assert_eq!(desc(&out, "Ⅻ"), Some("ローマ数字(大文字)".to_string()));
        assert_eq!(desc(&out, "ⅻ"), Some("ローマ数字(小文字)".to_string()));
        assert_eq!(desc(&out, "⑫"), Some("丸数字".to_string()));
    }

    #[test]
    fn does_not_emit_self() {
        let r = NumberRewriter::new();
        for input in ["1", "123", "1234", "12345"] {
            let t = texts(&r.rewrite(input));
            assert!(
                !t.iter().any(|s| s == input),
                "{} in its own variants",
                input
            );
        }
    }

    #[test]
    fn very_large_number_skips_radix() {
        let r = NumberRewriter::new();
        let t = texts(&r.rewrite("99999999999999999999")); // 20 digits, > u64::MAX
        assert!(!t.iter().any(|s| s.starts_with("0x")));
        assert!(!t.iter().any(|s| s.starts_with("0b")));
    }
}
