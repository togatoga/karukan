//! Which symbol a key types: the `,` `.` `/` `[` `]` keys have more than
//! one conventional output, and each IME lets the user pick.
//!
//! The style only decides *which* symbol; how wide it settles is
//! [`crate::width`]. Every output here is full-width so the converter keeps
//! its "rule outputs are never ASCII" contract, and the width rules fold
//! them to `,` `.` `/` `[` `]` when asked.

use serde::{Deserialize, Serialize};

/// What the `,` and `.` keys type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PunctuationStyle {
    #[default]
    #[serde(rename = "、。")]
    KutenTouten,
    #[serde(rename = "，．")]
    CommaPeriod,
    #[serde(rename = "、．")]
    ToutenPeriod,
    #[serde(rename = "，。")]
    CommaKuten,
}

impl PunctuationStyle {
    /// The `,` key's output.
    pub fn comma(self) -> &'static str {
        match self {
            Self::KutenTouten | Self::ToutenPeriod => "、",
            Self::CommaPeriod | Self::CommaKuten => "，",
        }
    }

    /// The `.` key's output.
    pub fn period(self) -> &'static str {
        match self {
            Self::KutenTouten | Self::CommaKuten => "。",
            Self::CommaPeriod | Self::ToutenPeriod => "．",
        }
    }
}

/// What the `[` and `]` keys type.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketStyle {
    #[default]
    #[serde(rename = "「」")]
    Corner,
    #[serde(rename = "[]")]
    Square,
}

impl BracketStyle {
    /// The `[` key's output.
    pub fn open(self) -> &'static str {
        match self {
            Self::Corner => "「",
            Self::Square => "［",
        }
    }

    /// The `]` key's output.
    pub fn close(self) -> &'static str {
        match self {
            Self::Corner => "」",
            Self::Square => "］",
        }
    }
}

/// What the `/` key types.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SlashStyle {
    #[default]
    #[serde(rename = "・")]
    MiddleDot,
    #[serde(rename = "/")]
    Slash,
}

impl SlashStyle {
    /// The `/` key's output.
    pub fn slash(self) -> &'static str {
        match self {
            Self::MiddleDot => "・",
            Self::Slash => "／",
        }
    }
}

/// The symbol style as a whole, one field per configurable key.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolStyle {
    pub punctuation: PunctuationStyle,
    pub bracket: BracketStyle,
    pub slash: SlashStyle,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_style_types_its_own_pair() {
        for (style, comma, period) in [
            (PunctuationStyle::KutenTouten, "、", "。"),
            (PunctuationStyle::CommaPeriod, "，", "．"),
            (PunctuationStyle::ToutenPeriod, "、", "．"),
            (PunctuationStyle::CommaKuten, "，", "。"),
        ] {
            assert_eq!(
                (style.comma(), style.period()),
                (comma, period),
                "{style:?}"
            );
        }
        let default = SymbolStyle::default();
        assert_eq!(default.punctuation, PunctuationStyle::KutenTouten);
        assert_eq!(
            (default.bracket.open(), default.slash.slash()),
            ("「", "・")
        );
    }

    #[test]
    fn outputs_are_never_ascii() {
        // The converter's contract: a rule output is always non-ASCII, so
        // the width rules — not the trie — decide half or full.
        let outputs = [
            PunctuationStyle::CommaPeriod.comma(),
            PunctuationStyle::CommaPeriod.period(),
            BracketStyle::Square.open(),
            BracketStyle::Square.close(),
            SlashStyle::Slash.slash(),
        ];
        for output in outputs {
            assert!(
                output.chars().all(|c| !c.is_ascii()),
                "{output} must not be ASCII"
            );
        }
    }
}
