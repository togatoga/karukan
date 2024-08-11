//! Candidate list management
//!
//! Handles the list of conversion candidates with pagination support.

/// A single conversion candidate
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    /// The converted text
    pub text: String,
    /// The original reading (hiragana)
    pub reading: Option<String>,
    /// Optional annotation (e.g., word type, dictionary info)
    pub annotation: Option<String>,
    /// Unique index from the conversion engine
    pub index: usize,
}

impl Candidate {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            reading: None,
            annotation: None,
            index: 0,
        }
    }

    pub fn with_reading(text: impl Into<String>, reading: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            reading: Some(reading.into()),
            annotation: None,
            index: 0,
        }
    }

    pub fn with_index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }
}

impl From<String> for Candidate {
    fn from(text: String) -> Self {
        Self::new(text)
    }
}

impl From<&str> for Candidate {
    fn from(text: &str) -> Self {
        Self::new(text)
    }
}

/// A list of candidates with pagination and selection support
#[derive(Debug, Clone)]
pub struct CandidateList {
    /// All candidates
    candidates: Vec<Candidate>,
    /// Currently selected candidate index
    cursor: usize,
    /// Number of candidates per page
    page_size: usize,
}

impl CandidateList {
    /// Default page size for candidate display
    pub const DEFAULT_PAGE_SIZE: usize = 9;

    /// Create a new candidate list
    pub fn new(candidates: Vec<Candidate>) -> Self {
        Self {
            candidates,
            cursor: 0,
            page_size: Self::DEFAULT_PAGE_SIZE,
        }
    }

    /// Create a candidate list from strings
    pub fn from_strings(strings: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let candidates = strings
            .into_iter()
            .enumerate()
            .map(|(i, s)| Candidate::new(s).with_index(i))
            .collect();
        Self::new(candidates)
    }

    /// Create a candidate list from strings with reading annotation
    pub fn from_strings_with_reading(
        strings: impl IntoIterator<Item = impl Into<String>>,
        reading: impl Into<String>,
    ) -> Self {
        let reading = reading.into();
        let candidates = strings
            .into_iter()
            .enumerate()
            .map(|(i, s)| Candidate::with_reading(s, &reading).with_index(i))
            .collect();
        Self::new(candidates)
    }

    /// Get all candidates
    pub fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Get the number of candidates
    pub fn len(&self) -> usize {
        self.candidates.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.candidates.is_empty()
    }

    /// Get the current cursor position
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Get the page size
    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Get the current page number (0-indexed)
    pub fn current_page(&self) -> usize {
        if self.page_size == 0 {
            0
        } else {
            self.cursor / self.page_size
        }
    }

    /// Get the total number of pages
    pub fn total_pages(&self) -> usize {
        if self.page_size == 0 || self.candidates.is_empty() {
            0
        } else {
            self.candidates.len().div_ceil(self.page_size)
        }
    }

    /// Get the start index of the current page
    pub fn page_start(&self) -> usize {
        self.current_page() * self.page_size
    }

    /// Get the candidates for the current page
    pub fn page_candidates(&self) -> &[Candidate] {
        let start = self.page_start();
        let end = (start + self.page_size).min(self.candidates.len());
        &self.candidates[start..end]
    }

    /// Get the cursor position within the current page (0-indexed)
    pub fn page_cursor(&self) -> usize {
        self.cursor - self.page_start()
    }

    /// Get the currently selected candidate
    pub fn selected(&self) -> Option<&Candidate> {
        self.candidates.get(self.cursor)
    }

    /// Get the currently selected text
    pub fn selected_text(&self) -> Option<&str> {
        self.selected().map(|c| c.text.as_str())
    }

    /// Move to the next candidate
    pub fn move_next(&mut self) -> bool {
        if self.cursor + 1 < self.candidates.len() {
            self.cursor += 1;
            true
        } else if !self.candidates.is_empty() {
            // Wrap to beginning
            self.cursor = 0;
            true
        } else {
            false
        }
    }

    /// Move to the previous candidate
    pub fn move_prev(&mut self) -> bool {
        if self.cursor > 0 {
            self.cursor -= 1;
            true
        } else if !self.candidates.is_empty() {
            // Wrap to end
            self.cursor = self.candidates.len() - 1;
            true
        } else {
            false
        }
    }

    /// Move to the next page
    pub fn next_page(&mut self) -> bool {
        if self.candidates.is_empty() {
            return false;
        }

        let next_page_start = self.page_start() + self.page_size;
        if next_page_start < self.candidates.len() {
            self.cursor = next_page_start;
            true
        } else {
            // Wrap to first page
            self.cursor = 0;
            true
        }
    }

    /// Move to the previous page
    pub fn prev_page(&mut self) -> bool {
        if self.candidates.is_empty() {
            return false;
        }

        let current_page = self.current_page();
        if current_page > 0 {
            self.cursor = (current_page - 1) * self.page_size;
            true
        } else {
            // Wrap to last page
            let last_page = self.total_pages().saturating_sub(1);
            self.cursor = last_page * self.page_size;
            true
        }
    }

    /// Select a candidate by index within the current page (1-9)
    pub fn select_on_page(&mut self, page_index: usize) -> Option<&Candidate> {
        if page_index == 0 || page_index > self.page_size {
            return None;
        }

        let absolute_index = self.page_start() + page_index - 1;
        if absolute_index < self.candidates.len() {
            self.cursor = absolute_index;
            self.selected()
        } else {
            None
        }
    }

    /// Select a candidate by absolute index
    pub fn select(&mut self, index: usize) -> Option<&Candidate> {
        if index < self.candidates.len() {
            self.cursor = index;
            self.selected()
        } else {
            None
        }
    }

    /// Reset cursor to beginning
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Update the candidate list with new candidates
    pub fn update(&mut self, candidates: Vec<Candidate>) {
        self.candidates = candidates;
        self.cursor = 0;
    }
}

impl Default for CandidateList {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_candidate_list_basic() {
        let candidates = CandidateList::from_strings(["今日", "京", "恭"]);
        assert_eq!(candidates.len(), 3);
        assert_eq!(candidates.selected_text(), Some("今日"));
    }

    #[test]
    fn test_candidate_list_navigation() {
        let mut candidates = CandidateList::from_strings(["a", "b", "c"]);

        assert!(candidates.move_next());
        assert_eq!(candidates.selected_text(), Some("b"));

        assert!(candidates.move_next());
        assert_eq!(candidates.selected_text(), Some("c"));

        // Wrap around
        assert!(candidates.move_next());
        assert_eq!(candidates.selected_text(), Some("a"));

        // Wrap back
        assert!(candidates.move_prev());
        assert_eq!(candidates.selected_text(), Some("c"));
    }

    #[test]
    fn test_candidate_list_pagination() {
        // Default page_size is 9, so 20 items = 3 pages (9+9+2)
        let items: Vec<_> = (1..=20).map(|i| format!("item{}", i)).collect();
        let mut candidates = CandidateList::from_strings(items);

        assert_eq!(candidates.total_pages(), 3);
        assert_eq!(candidates.current_page(), 0);
        assert_eq!(candidates.page_candidates().len(), 9);

        candidates.next_page();
        assert_eq!(candidates.current_page(), 1);
        assert_eq!(candidates.page_start(), 9);

        candidates.next_page();
        assert_eq!(candidates.current_page(), 2);
        assert_eq!(candidates.page_candidates().len(), 2);

        // Wrap to first page
        candidates.next_page();
        assert_eq!(candidates.current_page(), 0);
    }

    #[test]
    fn test_candidate_list_select_on_page() {
        let items: Vec<_> = (1..=20).map(|i| format!("item{}", i)).collect();
        let mut candidates = CandidateList::from_strings(items);

        // Select item 3 on first page
        candidates.select_on_page(3);
        assert_eq!(candidates.selected_text(), Some("item3"));

        // Move to second page and select item 2
        candidates.next_page();
        candidates.select_on_page(2);
        assert_eq!(candidates.selected_text(), Some("item11")); // 9 + 2 = 11
    }
}
