//! Input state machine
//!
//! Defines the states of the IME and transitions between them.

use super::candidate::{Candidate, CandidateList, CandidateSource};
use super::preedit::Preedit;

/// One segment (文節) of a conversion that is not the focused one: a
/// stretch of the reading with its own candidate list, kept so that the
/// choice made there survives the focus moving away and back.
#[derive(Debug, Clone)]
pub struct Segment {
    /// The stretch of the reading this segment converts
    pub reading: String,
    /// Its candidates. Until the segment is first focused this is only the
    /// text it had when the conversion was split (`built` is false): the
    /// full list costs a model call, paid when the user asks for it.
    pub candidates: CandidateList,
    /// The source filter that was active on it
    pub filter: Option<CandidateSource>,
    /// Whether `candidates` is the full list for `reading`
    pub built: bool,
}

impl Segment {
    /// A segment not yet focused, showing `text` (the reading itself when
    /// there is no conversion to show).
    pub fn unbuilt(reading: impl Into<String>, text: Option<String>) -> Self {
        // No reading on the row: this text was cut out of a longer
        // conversion, so the pair is a guess and must not be learned.
        let candidates = text
            .map(|text| Candidate {
                text,
                reading: None,
                source: Some(CandidateSource::Model),
                description: None,
            })
            .into_iter()
            .collect();
        Self {
            reading: reading.into(),
            candidates: CandidateList::new(candidates),
            filter: None,
            built: false,
        }
    }

    /// What the segment shows and commits.
    pub fn text(&self) -> &str {
        self.candidates.selected_text().unwrap_or(&self.reading)
    }
}

/// What a conversion showed as its default before anything was picked:
/// the live conversion on screen when Space was pressed, or else the head
/// of the list. A pick is told apart from it when the commit is learned.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Shown {
    pub reading: String,
    pub text: String,
}

/// The current state of the IME
#[derive(Debug, Clone, Default)]
pub enum InputState {
    /// No input, waiting for user to type
    #[default]
    Empty,

    /// Composing mode - building preedit text (kana, alphabet, or an emoji query)
    Composing {
        /// The preedit string being composed
        preedit: Preedit,
    },

    /// Conversion mode - selecting from candidates.
    ///
    /// `candidates`, `reading` and `filter` describe the focused segment.
    /// A conversion starts as one segment covering the whole reading, with
    /// `left` and `right` empty; splitting it (the arrow keys) moves the
    /// other segments there, in order, around the focused one.
    Conversion {
        /// The preedit string showing conversion result
        preedit: Preedit,
        /// List of conversion candidates (possibly source-filtered)
        candidates: CandidateList,
        /// The (settled) reading the conversion was built from
        reading: String,
        /// Active Ctrl+R source filter; `None` shows the full list
        filter: Option<CandidateSource>,
        /// Segments before the focused one
        left: Vec<Segment>,
        /// Segments after the focused one
        right: Vec<Segment>,
        /// The default the whole conversion opened with
        shown: Shown,
    },
}

impl InputState {
    /// Check if the engine is in the Empty (idle) state
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Get the current preedit if any
    pub fn preedit(&self) -> Option<&Preedit> {
        match self {
            Self::Empty => None,
            Self::Composing { preedit, .. } => Some(preedit),
            Self::Conversion { preedit, .. } => Some(preedit),
        }
    }

    /// Get mutable reference to preedit
    pub fn preedit_mut(&mut self) -> Option<&mut Preedit> {
        match self {
            Self::Empty => None,
            Self::Composing { preedit, .. } => Some(preedit),
            Self::Conversion { preedit, .. } => Some(preedit),
        }
    }

    /// Get the active source filter in conversion state
    pub fn filter(&self) -> Option<CandidateSource> {
        match self {
            Self::Conversion { filter, .. } => *filter,
            _ => None,
        }
    }

    /// The reading a conversion was built from, if in the Conversion state.
    pub fn reading(&self) -> Option<&str> {
        match self {
            Self::Conversion { reading, .. } => Some(reading),
            _ => None,
        }
    }

    /// Whether the conversion has been split into more than one segment.
    pub fn is_segmented(&self) -> bool {
        match self {
            Self::Conversion { left, right, .. } => !left.is_empty() || !right.is_empty(),
            _ => false,
        }
    }

    /// The segments around the focused one: those before it, those after.
    pub fn neighbours(&self) -> (&[Segment], &[Segment]) {
        match self {
            Self::Conversion { left, right, .. } => (left, right),
            _ => (&[], &[]),
        }
    }

    /// The default the conversion opened with.
    pub fn shown(&self) -> Option<&Shown> {
        match self {
            Self::Conversion { shown, .. } => Some(shown),
            _ => None,
        }
    }

    /// Get candidates in conversion state
    pub fn candidates(&self) -> Option<&CandidateList> {
        match self {
            Self::Conversion { candidates, .. } => Some(candidates),
            _ => None,
        }
    }

    /// Get mutable reference to candidates
    pub fn candidates_mut(&mut self) -> Option<&mut CandidateList> {
        match self {
            Self::Conversion { candidates, .. } => Some(candidates),
            _ => None,
        }
    }
}
