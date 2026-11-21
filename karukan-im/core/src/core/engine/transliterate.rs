//! F6–F10: the composition rewritten in one script, without the model.

use karukan_engine::kana::{half_width_katakana_to_full, katakana_to_half_width};
use karukan_engine::width::{to_full_width, to_half_width};
use karukan_engine::{hiragana_to_katakana, katakana_to_hiragana};

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Transliteration {
    Hiragana,
    FullKatakana,
    HalfKatakana,
    FullAlphanumeric,
    HalfAlphanumeric,
}

impl Transliteration {
    const ALL: [Self; 5] = [
        Self::Hiragana,
        Self::FullKatakana,
        Self::HalfKatakana,
        Self::FullAlphanumeric,
        Self::HalfAlphanumeric,
    ];

    /// F6–F10 bare, and Ctrl+K for katakana (either keysym case, since
    /// some environments fold Shift into an uppercase keysym).
    pub(super) fn for_key(key: &KeyEvent) -> Option<Self> {
        let m = &key.modifiers;
        if m.control_key
            && !m.alt_key
            && !m.super_key
            && matches!(key.keysym, Keysym::KEY_K | Keysym::KEY_K_UPPER)
        {
            return Some(Self::FullKatakana);
        }
        if m.any() {
            return None;
        }
        match key.keysym {
            Keysym::F6 => Some(Self::Hiragana),
            Keysym::F7 => Some(Self::FullKatakana),
            Keysym::F8 => Some(Self::HalfKatakana),
            Keysym::F9 => Some(Self::FullAlphanumeric),
            Keysym::F10 => Some(Self::HalfAlphanumeric),
            _ => None,
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Hiragana => "[全]ひらがな",
            Self::FullKatakana => "[全]カタカナ",
            Self::HalfKatakana => "[半]カタカナ",
            Self::FullAlphanumeric => "[全]英数",
            Self::HalfAlphanumeric => "[半]英数",
        }
    }

    /// A row of this form.
    fn candidate(self, text: String, reading: &str) -> Candidate {
        Candidate {
            text,
            reading: Some(reading.to_string()),
            source: Some(self.source()),
            description: Some(self.description().to_string()),
        }
    }

    fn source(self) -> CandidateSource {
        match self {
            Self::Hiragana | Self::FullKatakana | Self::HalfKatakana => CandidateSource::Fallback,
            Self::FullAlphanumeric | Self::HalfAlphanumeric => CandidateSource::Transliteration,
        }
    }

    /// The forms this key steps through: the alphanumerics cycle their
    /// case (as typed → lower → UPPER → Capitalized), the kana have one.
    fn variants(self, reading: &str, keystrokes: &str) -> Vec<String> {
        // A table may write half-width katakana (`ｶﾞ`): every kana form
        // starts from full-width kana.
        let kana = || half_width_katakana_to_full(reading);
        let mut variants = match self {
            Self::Hiragana => vec![katakana_to_hiragana(&kana())],
            Self::FullKatakana => vec![hiragana_to_katakana(&kana())],
            Self::HalfKatakana => vec![katakana_to_half_width(&hiragana_to_katakana(&kana()))],
            Self::FullAlphanumeric => case_variants(keystrokes)
                .iter()
                .map(|v| to_full_width(v).replace(' ', "\u{3000}"))
                .collect(),
            Self::HalfAlphanumeric => {
                case_variants(&to_half_width(keystrokes).replace('\u{3000}', " "))
            }
        };
        let mut seen = HashSet::new();
        variants.retain(|v| !v.is_empty() && seen.insert(v.clone()));
        variants
    }
}

fn case_variants(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut chars = lower.chars();
    let capitalized = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    vec![
        text.to_string(),
        lower.clone(),
        text.to_uppercase(),
        capitalized,
    ]
}

impl InputMethodEngine {
    /// Open a conversion whose list is the transliterations of the
    /// composition, with `form` selected. Pressing the same key again while
    /// that list is shown steps to the key's next variant.
    pub(super) fn transliterate(&mut self, form: Transliteration) -> EngineResult {
        let reading = self.input_buf.settled_reading(&self.converters.romaji);
        if reading.is_empty() {
            return EngineResult::consumed();
        }
        let keystrokes = self.input_buf.keystrokes();
        let mut rows: Vec<Candidate> = Transliteration::ALL
            .into_iter()
            .flat_map(|t| {
                t.variants(&reading, &keystrokes)
                    .into_iter()
                    .map(|text| t.candidate(text, &reading))
                    .collect::<Vec<_>>()
            })
            .collect();

        // A repeat is this same key pressed on its own row of this same
        // list: only then does the case step on. The row carries the form
        // that was pressed (stamped below), so F6 then F10 on `k`, whose
        // forms all read `k`, starts F10 at the typed case.
        let variants = form.variants(&reading, &keystrokes);
        let mut seen = HashSet::new();
        let texts: Vec<&str> = rows
            .iter()
            .map(|c| c.text.as_str())
            .filter(|t| seen.insert(*t))
            .collect();
        let repeated = self
            .state
            .candidates()
            .filter(|shown| {
                shown
                    .candidates()
                    .iter()
                    .map(|c| c.text.as_str())
                    .eq(texts.iter().copied())
            })
            .and_then(CandidateList::selected)
            .filter(|c| {
                c.source == Some(form.source())
                    && c.description.as_deref() == Some(form.description())
            });
        let step = repeated
            .and_then(|c| variants.iter().position(|v| *v == c.text))
            .map_or(0, |i| (i + 1) % variants.len());
        let Some(target) = variants.get(step).cloned() else {
            return EngineResult::consumed();
        };

        // Forms can share a surface (`Linux` reads the same in every form)
        // and the list keeps one row per surface, the first. Give that row
        // the pressed form, so F9/F10 commit a non-learnable row and F6–F8
        // a learnable one whatever the surfaces happen to share.
        if let Some(row) = rows.iter_mut().find(|c| c.text == target) {
            row.source = Some(form.source());
            row.description = Some(form.description().to_string());
        }
        let mut candidates = self.settle_candidates(rows);
        if let Some(index) = candidates
            .candidates()
            .iter()
            .position(|c| c.text == target)
        {
            candidates.set_cursor(index);
        }
        // Like Space: the list replaces the live display, so cancelling
        // returns to the kana, not to the live conversion.
        self.live.shown = false;
        self.enter_conversion_state(&reading, candidates)
    }
}
