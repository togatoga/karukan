//! Preedit string management
//!
//! Handles the composition string (preedit) that is displayed while the user
//! is typing and before text is committed.

/// Attribute type for preedit text styling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeType {
    /// Normal underline for uncommitted preedit text
    Underline,
    /// Double underline for the currently selected segment
    UnderlineDouble,
    /// Highlight for text being converted
    Highlight,
    /// Reverse video for selected candidate
    Reverse,
}

/// A text attribute with range
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreeditAttribute {
    /// Start position (character index)
    pub start: usize,
    /// End position (character index, exclusive)
    pub end: usize,
    /// Attribute type
    pub attr_type: AttributeType,
}

impl PreeditAttribute {
    pub fn new(start: usize, end: usize, attr_type: AttributeType) -> Self {
        Self {
            start,
            end,
            attr_type,
        }
    }

    /// Create an underline attribute for the entire range
    pub fn underline(start: usize, end: usize) -> Self {
        Self::new(start, end, AttributeType::Underline)
    }
}

/// A segment within the preedit text
#[derive(Debug, Clone)]
pub struct PreeditSegment {
    /// The text content of this segment
    pub text: String,
    /// The attribute type for this segment
    pub attr_type: AttributeType,
}

impl PreeditSegment {
    pub fn new(text: impl Into<String>, attr_type: AttributeType) -> Self {
        Self {
            text: text.into(),
            attr_type,
        }
    }

    pub fn highlighted(text: impl Into<String>) -> Self {
        Self::new(text, AttributeType::Highlight)
    }
}

/// Preedit string with cursor position and attributes
#[derive(Debug, Clone, Default)]
pub struct Preedit {
    /// The preedit text
    text: String,
    /// Caret (cursor) position in characters
    caret: usize,
    /// Text attributes for styling
    attributes: Vec<PreeditAttribute>,
}

impl Preedit {
    /// Create a new empty preedit
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a preedit with the given text
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        Self {
            text,
            caret: len,
            attributes: Vec::new(),
        }
    }

    /// Create a preedit with text and underline the entire text
    pub fn with_text_underlined(text: impl Into<String>) -> Self {
        let text = text.into();
        let len = text.chars().count();
        Self {
            attributes: vec![PreeditAttribute::underline(0, len)],
            text,
            caret: len,
        }
    }

    /// Get the preedit text
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Get the caret position (in characters)
    pub fn caret(&self) -> usize {
        self.caret
    }

    /// Get the text attributes
    pub fn attributes(&self) -> &[PreeditAttribute] {
        &self.attributes
    }

    /// Check if the preedit is empty
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Get the length in characters
    pub fn len(&self) -> usize {
        self.text.chars().count()
    }

    /// Set the caret position
    pub fn set_caret(&mut self, caret: usize) {
        let len = self.len();
        self.caret = caret.min(len);
    }

    /// Set attributes
    pub fn set_attributes(&mut self, attributes: Vec<PreeditAttribute>) {
        self.attributes = attributes;
    }

    /// Clear the preedit
    pub fn clear(&mut self) {
        self.text.clear();
        self.caret = 0;
        self.attributes.clear();
    }

    /// Create a preedit from segments
    pub fn from_segments(segments: Vec<PreeditSegment>, caret: usize) -> Self {
        let text: String = segments.iter().map(|s| s.text.as_str()).collect();
        let attributes: Vec<_> = segments
            .iter()
            .scan(0usize, |pos, seg| {
                let start = *pos;
                *pos += seg.text.chars().count();
                Some(PreeditAttribute::new(start, *pos, seg.attr_type))
            })
            .collect();
        let len = text.chars().count();
        Self {
            text,
            caret: caret.min(len),
            attributes,
        }
    }
}
