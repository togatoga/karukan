//! Which width a character comes out at: half (`(`, `?`, `1`) or full
//! (`（`, `？`, `１`).
//!
//! Three [`Group`]s, each with its own [`Width`]: the kana symbols, the
//! ASCII ones, digits. Characters in no group (kana, kanji, the ideographic
//! space) are left alone, and so are letters — see [`WidthRules::width`].
//!
//! `。、「」・` are a group of their own because they are the symbols with
//! no ASCII counterpart: their only half-width forms (`｡､｢｣･`) are the ones
//! that go with half-width katakana. Keeping them apart is what lets "every
//! symbol half-width" stop short of 「こんにちは｡」, and what lets `？！` sit
//! with the ASCII symbols they are the full-width forms of.
//!
//! A group holds both forms of each character it covers, so applying the
//! rules twice is the same as applying them once.

use serde::{Deserialize, Serialize};

use crate::kana::{ascii_to_fullwidth_char, fullwidth_to_ascii_char, is_digit};

/// The width a group comes out at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Width {
    /// The half-width form (`。` → `｡`, `（` → `(`).
    Half,
    /// The full-width form (`｡` → `。`, `(` → `（`).
    Full,
}

/// A set of characters sharing one width setting.
///
/// The split is by what a user asks for, not by what a character is:
/// symbols, digits and letters are the three answers people give
/// separately. Splitting further would be answering a question nobody asks
/// — whether `(` and `@` deserve different widths.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Group {
    /// 。、「」・ — the symbols with no ASCII form, only a half-width
    /// katakana one (`｡､｢｣･`)
    KanaSymbol,
    /// Every ASCII symbol and its full-width twin: `?!` `,.` `(){}[]`
    /// `<>=+-/*` `"'` `:;` `~` `#%&@$^_|` `` ` `` `\`
    AsciiSymbol,
    /// 0-9
    Digit,
    /// A-Za-z. Not configurable: see [`WidthRules::width`].
    Alphabet,
}

/// The pairs arithmetic cannot produce. Every other character in
/// [`Group::AsciiSymbol`] is its ASCII original plus [`FULLWIDTH_OFFSET`],
/// which [`forms`] computes: these five kana symbols have unrelated code
/// points, and `~` types the wave dash `〜` rather than the full-width
/// tilde the offset would give (`～` still folds to `~` through it).
#[rustfmt::skip]
const PAIRS: &[(Group, char, char)] = &[
    (Group::KanaSymbol,  '｡', '。'),
    (Group::KanaSymbol,  '､', '、'),
    (Group::KanaSymbol,  '｢', '「'),
    (Group::KanaSymbol,  '｣', '」'),
    (Group::KanaSymbol,  '･', '・'),
    (Group::AsciiSymbol, '~', '〜'),
];

/// Full-width forms sit this far above their ASCII originals: `(` U+0028 →
/// `（` U+FF08.
const FULLWIDTH_OFFSET: u32 = 0xFEE0;

/// The group `c` belongs to, with its half and full forms — `None` for
/// characters no group covers (kana, kanji, spaces).
fn forms(c: char) -> Option<(Group, char, char)> {
    // Digits and letters are ranges, so their pair is computed rather than
    // listed. Both helpers are no-ops on the form `c` is already in.
    let ascii_pair = |group| {
        Some((
            group,
            fullwidth_to_ascii_char(c),
            ascii_to_fullwidth_char(c),
        ))
    };
    if is_digit(c) {
        return ascii_pair(Group::Digit);
    }
    if c.is_ascii_alphabetic() || matches!(c, 'Ａ'..='Ｚ' | 'ａ'..='ｚ') {
        return ascii_pair(Group::Alphabet);
    }
    if let Some((group, half, full)) = PAIRS
        .iter()
        .find(|(_, half, full)| c == *half || c == *full)
    {
        return Some((*group, *half, *full));
    }
    // Everything else is ASCII punctuation, or the full-width form sitting
    // at the fixed offset above it.
    let half = if c.is_ascii_punctuation() {
        c
    } else {
        char::from_u32((c as u32).wrapping_sub(FULLWIDTH_OFFSET))
            .filter(char::is_ascii_punctuation)?
    };
    let full = char::from_u32(half as u32 + FULLWIDTH_OFFSET)?;
    Some((Group::AsciiSymbol, half, full))
}

/// Whether `c` has both a half-width and a full-width form — false for kana,
/// kanji and anything else no group covers.
pub fn has_width_pair(c: char) -> bool {
    forms(c).is_some()
}

/// Every character at its half-width form (`（１）` → `(1)`). Characters
/// without one are left alone.
pub fn to_half_width(text: &str) -> String {
    text.chars()
        .map(|c| forms(c).map_or(c, |(_, half, _)| half))
        .collect()
}

/// Every character at its full-width form (`(1)` → `（１）`). Characters
/// without one are left alone.
pub fn to_full_width(text: &str) -> String {
    text.chars()
        .map(|c| forms(c).map_or(c, |(_, _, full)| full))
        .collect()
}

/// The width each group comes out at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct WidthRules {
    pub kana_symbol: Width,
    pub ascii_symbol: Width,
    pub digit: Width,
}

/// Kana input comes out full-width, digits excepted: nothing here
/// remembers the width last picked, so a full-width default would be one
/// the user cannot take back, and digits are wanted half-width more often
/// than not.
impl Default for WidthRules {
    fn default() -> Self {
        Self {
            kana_symbol: Width::Full,
            ascii_symbol: Width::Full,
            digit: Width::Half,
        }
    }
}

impl WidthRules {
    /// The width configured for `group`, or `None` for a group that is
    /// never folded.
    ///
    /// Letters are the one such group. In kana input a latin character is
    /// either an unfired keystroke (the `d` in 「わせだd」, which is a
    /// keystroke and not text) or a dictionary surface, and neither is
    /// something to widen behind the user's back. The candidate list offers
    /// `ＡＢＣ` when they want it.
    fn width(&self, group: Group) -> Option<Width> {
        match group {
            Group::KanaSymbol => Some(self.kana_symbol),
            Group::AsciiSymbol => Some(self.ascii_symbol),
            Group::Digit => Some(self.digit),
            Group::Alphabet => None,
        }
    }

    /// The form `c` comes out at.
    pub fn apply(&self, c: char) -> char {
        let Some((group, half, full)) = forms(c) else {
            return c;
        };
        match self.width(group) {
            Some(Width::Half) => half,
            Some(Width::Full) => full,
            None => c,
        }
    }

    /// Every character of `text` at the width its group settles as.
    pub fn apply_str(&self, text: &str) -> String {
        text.chars().map(|c| self.apply(c)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every group at one width. `all(Width::Half)` is the "記号はすべて
    /// 半角" setting.
    fn all(width: Width) -> WidthRules {
        WidthRules {
            kana_symbol: width,
            ascii_symbol: width,
            digit: width,
        }
    }

    fn all_half() -> WidthRules {
        all(Width::Half)
    }

    fn all_full() -> WidthRules {
        all(Width::Full)
    }

    #[test]
    fn the_default_is_full_width_kana_input_with_half_width_digits() {
        let rules = WidthRules::default();
        assert_eq!(rules.apply_str("。、「」・"), "。、「」・");
        assert_eq!(rules.apply_str("()[]?!"), "（）［］？！");
        assert_eq!(rules.apply_str("１２３"), "123");
        assert_eq!(rules.apply_str("abcあア亜"), "abcあア亜");
    }

    #[test]
    fn half_and_full_fold_each_group() {
        let half = all_half();
        assert_eq!(half.apply_str("？！"), "?!");
        assert_eq!(half.apply_str("（）［］"), "()[]");
        assert_eq!(half.apply_str("＠：；"), "@:;");
        assert_eq!(half.apply_str("１２３"), "123");

        let full = all_full();
        assert_eq!(full.apply_str("()[]{}"), "（）［］｛｝");
        assert_eq!(full.apply_str("@#%"), "＠＃％");
        assert_eq!(full.apply_str("123"), "１２３");

        // `~` types 〜 (U+301C); the full-width tilde ～ (U+FF5E) other
        // sources produce belongs to the same group and folds with it.
        assert_eq!(full.apply_str("~"), "〜");
        assert_eq!(half.apply_str("〜～"), "~~");
    }

    #[test]
    fn the_kana_symbols_are_their_own_group() {
        // 「記号は半角」 must not reach them: that is what the split is for.
        let symbols_only = WidthRules {
            ascii_symbol: Width::Half,
            ..WidthRules::default()
        };
        assert_eq!(symbols_only.apply_str("。、「」・？"), "。、「」・?");
        // Asking for them explicitly still gives the half-width katakana
        // forms.
        let marks_too = WidthRules {
            kana_symbol: Width::Half,
            ..symbols_only
        };
        assert_eq!(marks_too.apply_str("。、「」・？"), "｡､｢｣･?");
    }

    #[test]
    fn groups_are_independent() {
        // Full-width digits next to half-width symbols and letters.
        let rules = WidthRules {
            kana_symbol: Width::Full,
            ascii_symbol: Width::Half,
            digit: Width::Full,
        };
        assert_eq!(rules.apply_str("。"), "。");
        assert_eq!(rules.apply_str("（1）"), "(１)");
    }

    #[test]
    fn applying_twice_changes_nothing() {
        for rules in [all_half(), all_full(), WidthRules::default()] {
            for text in ["。、「」・()[]?!~123abc@", "（）［］？！〜１２３ＡＢＣ＠"]
            {
                let once = rules.apply_str(text);
                assert_eq!(rules.apply_str(&once), once, "{text} is not idempotent");
            }
        }
    }

    #[test]
    fn whole_string_conversions_cover_every_group() {
        assert_eq!(to_half_width("（１）ＡＢ？。"), "(1)AB?｡");
        assert_eq!(to_full_width("(1)AB?｡"), "（１）ＡＢ？。");
        // Kana and kanji have no pair and ride through untouched.
        assert_eq!(to_half_width("あ漢ア"), "あ漢ア");
        assert!(has_width_pair('。'));
        assert!(has_width_pair('1'));
        assert!(!has_width_pair('あ'));
        assert!(!has_width_pair('漢'));
    }
}
