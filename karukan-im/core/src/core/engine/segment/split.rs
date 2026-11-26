//! Where the segment (文節) boundaries are, found in the model's own answer.
//!
//! There is no morphological analysis in this IME — no connection costs,
//! no word costs, no lattice — and the model turns a reading into text
//! without saying which part came from where. What there is, is a
//! dictionary, and two strings that are the same sentence: the kana that
//! were typed and the text they became. Finding the segments is a matching
//! problem between the two, and the answer is the dictionary's longest
//! match.
//!
//! The text is covered, left to right, by four kinds of step, each taking
//! a stretch of the reading with it:
//!
//! - a dictionary word: its reading at this place in the reading, its
//!   surface at this place in the text;
//! - a character that is the reading's own, and can only be that one (a
//!   kana the dictionary did not cover, a digit typed as a digit);
//! - kanji the dictionary has no word for, read as their single-kanji
//!   readings one after another (忖度 as そん and たく);
//! - anything else, taking whatever reading is left for it. With its
//!   neighbours fixed by the three above, that is its own reading: a word
//!   nothing reads (今日 as きょう) is settled by elimination. This is the
//!   last resort, tried only where the rest cannot be covered without it.
//!
//! A segment starts at each word; hiragana and marks no word took belong
//! to the word before them. Of all the ways to cover the text, the ones
//! wanted leave the least outside the dictionary, and among those the
//! fewest segments come first — which is what "longest match" means. The
//! dictionary has no parts of speech, so it cannot say whether は is a
//! particle or これ a word: both are its words, and both are segments.
//! The cuts come ranked ([`ranked_cuts`]) because the one wanted is the
//! best that has two segments at least, and the best of all may be the
//! whole text as one word.
//!
//! Pure functions of the two strings and a word lookup, engine-free. A
//! split that comes out wrong costs the user a keystroke, and a resized
//! segment is converted again from its reading, so nothing here has to be
//! right to be safe.

use std::rc::Rc;

/// One segment: a stretch of the reading and the text it converted to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core) struct Piece {
    pub reading: String,
    pub text: String,
}

/// Text chars one guessed word may span, and reading chars it may take per
/// text char: bounds on the search, generous for real words (承る is 5 for
/// 1) and for a loanword spelled out (くろーど for Claude).
const MAX_RUN: usize = 16;
const MAX_READING_PER_CHAR: usize = 6;

/// Dictionary words starting at the head of a reading: for each, how many
/// reading chars it takes and its surface.
pub(in crate::core) type WordLookup<'a> = &'a dyn Fn(&str) -> Vec<(usize, String)>;

/// The readings a kanji has on its own, in hiragana; none for a character
/// that is not listed.
pub(in crate::core) type KanjiReadings<'a> = &'a dyn Fn(char) -> &'static [String];

/// What the text is lined up with the reading by.
#[derive(Clone, Copy)]
pub(in crate::core) struct Lexicon<'a> {
    pub words: WordLookup<'a>,
    pub readings: KanjiReadings<'a>,
}

/// What a step of the covering is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// One char that is the reading's own.
    Pin,
    /// A dictionary word.
    Word,
    /// Kanji the dictionary lacks, read by their single-kanji readings.
    Read,
    /// Text taking what reading is left: a word nothing reads.
    Unknown,
}

/// What kind of character a text char is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Hiragana,
    Katakana,
    /// `ー`: written in either kana.
    ProlongedMark,
    /// Latin letters and digits, either width.
    Alphanumeric,
    /// Kanji, marks, everything else.
    Other,
}

impl Script {
    fn of(c: char) -> Self {
        match c {
            'ー' => Self::ProlongedMark,
            '\u{3041}'..='\u{309F}' => Self::Hiragana,
            '\u{30A1}'..='\u{30FA}' | '\u{30FD}'..='\u{30FF}' => Self::Katakana,
            c if is_kanji(c) => Self::Other,
            c if c.is_alphanumeric() => Self::Alphanumeric,
            _ => Self::Other,
        }
    }

    /// Whether the char can only be itself in the reading. Kana must be:
    /// the model writes the reading's kana back as they are. Anything else
    /// may be itself (a digit typed as a digit) or what kana were
    /// converted to (いち → 1).
    fn is_kana(self) -> bool {
        matches!(self, Self::Hiragana | Self::Katakana | Self::ProlongedMark)
    }

    /// Whether a char the dictionary did not cover is part of a word all
    /// the same: katakana and alphanumerics are words wherever they
    /// appear, hiragana and marks belong to the word before them.
    fn is_word_char(self) -> bool {
        matches!(self, Self::Katakana | Self::Alphanumeric)
    }
}

/// `c` as the reading would spell it, for comparing: katakana as hiragana,
/// a full-width ASCII form as its half-width one.
fn fold(c: char) -> char {
    match c {
        '\u{30A1}'..='\u{30F6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
        '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
        '\u{3000}' => ' ',
        _ => c,
    }
}

/// Whether giving `reading[j..j2]` to `word` goes against the dictionary:
/// the dictionary reads `word` as a shorter stretch inside that one. That
/// is the mark of a conversion that dropped part of the reading —
/// わせだにだいがく answered with 早稲田大学 leaves に with nowhere to go
/// but into 大学, which the dictionary knows as だいがく alone. The text is
/// then not the reading's conversion, and no split of it can be trusted.
fn contradicts_dictionary(
    word: &[char],
    j: usize,
    j2: usize,
    words_at: &[Vec<(usize, Vec<char>)>],
) -> bool {
    (j..j2).any(|start| {
        words_at[start].iter().any(|(taken, surface)| {
            surface[..] == *word && start + taken <= j2 && (start, start + taken) != (j, j2)
        })
    })
}

/// Whether a segment starts at `text[i]`, where a step of kind `step`
/// starts.
fn starts_segment(text: &[char], i: usize, step: Step) -> bool {
    match step {
        // A word starts a segment.
        Step::Word | Step::Read | Step::Unknown => true,
        // So does a run of katakana or alphanumerics the dictionary did
        // not cover, at its first char. Hiragana and marks never do: they
        // belong to the word before them. `ー` inside the run does not end
        // it (コンピュータ).
        Step::Pin => {
            let is_word_char = |c: char| Script::of(c).is_word_char();
            let before = text[..i]
                .iter()
                .rev()
                .find(|&&c| Script::of(c) != Script::ProlongedMark);
            is_word_char(text[i]) && !before.is_some_and(|&c| is_word_char(c))
        }
    }
}

/// Longest reading/text this bothers to cut; beyond it the whole thing
/// stays one segment. The search is quadratic in both and keeps several
/// coverings per position.
const MAX_CHARS: usize = 120;

/// Coverings kept per position while ranking. More than are asked for,
/// since some are dropped at the end.
const KEEP: usize = 8;

/// What a covering is charged, compared left to right.
type Cost = [usize; 6];

/// One way of cutting, while it is being ranked.
#[derive(Debug, Clone)]
struct Cut {
    /// Cutting by the dictionary: the chars outside a dictionary word
    /// (marks aside), the guessed words, the unknown words (read or
    /// guessed), and what the [`Way`] is after — the words and then the
    /// segments, or the chars that start no segment. Cutting by script:
    /// the script's boundaries left inside a step first, then the same
    /// three. Last, the chars in guessed words: a guess takes no more of
    /// the text than it has to.
    cost: Cost,
    /// It gives some word a reading the dictionary contradicts.
    doubtful: bool,
    /// Where its segments start after the first: (text, reading)
    /// positions, the last one first.
    starts: Vec<(usize, usize)>,
}

impl Cut {
    /// Whether it cuts the text at `start` and then where `rest` does.
    fn cuts_like(&self, start: Option<(usize, usize)>, rest: &Cut) -> bool {
        let mine = self.starts.iter().map(|&(i, _)| i);
        let theirs = rest
            .starts
            .iter()
            .map(|&(i, _)| i)
            .chain(start.map(|(i, _)| i));
        mine.eq(theirs)
    }
}

/// Put the cut made of `step` followed by `rest` into `kept`, which holds
/// the cheapest [`KEEP`] distinct cuts in order.
fn offer(
    kept: &mut Vec<Cut>,
    rest: &Cut,
    step: Cost,
    doubtful: bool,
    start: Option<(usize, usize)>,
) {
    let cost = std::array::from_fn(|at| rest.cost[at] + step[at]);
    match kept.iter().position(|cut| cut.cuts_like(start, rest)) {
        Some(same) if kept[same].cost <= cost => return,
        Some(same) => {
            kept.remove(same);
        }
        None if kept.len() == KEEP && kept[KEEP - 1].cost <= cost => return,
        None => {}
    }
    let at = kept.partition_point(|cut| cut.cost <= cost);
    let mut starts = rest.starts.clone();
    starts.extend(start);
    kept.insert(
        at,
        Cut {
            cost,
            doubtful: doubtful || rest.doubtful,
            starts,
        },
    );
    kept.truncate(KEEP);
}

/// The best `k` ways the dictionary cuts `text`, the conversion of
/// `reading`, into two segments or more — the fewest segments first.
///
/// Every dictionary word is a unit here, hiragana ones too (東京|から): a
/// hiragana char no word took counts as left outside the dictionary, so
/// the dictionary's word for it always wins, and hiragana it lacks joins
/// the word before it. One rule sits on top of the dictionary: a word
/// that is a single hiragana joins the word before it as well (今日は,
/// 学校には), which is what makes the segments 文節 — the dictionary has
/// no parts of speech to say は is a particle, and one hiragana standing
/// alone as a word nearly always is one. Among the coverings that leave the least
/// outside the dictionary, the ones with the fewest segments come first:
/// the longest match, then what it gives way to — a word into the words
/// it is made of (競技|プログラミング). Coverings with the same number of
/// segments come in no particular order.
///
/// Empty when the text is not the reading's conversion, when it is too
/// long to rank, or when nothing cuts it.
pub(in crate::core) fn ranked_cuts(
    reading: &str,
    text: &str,
    lexicon: Lexicon,
    k: usize,
) -> Vec<Vec<Piece>> {
    cuts_where(reading, text, lexicon, k, Way::Longest)
}

/// `text` cut by script alone: a segment starts wherever the script
/// changes — kanji, hiragana, katakana, alphanumerics — and nowhere else
/// (競技|プログラミング|を|学|ぶ). Nothing is known of particles, prefixes
/// or inflection here; that is the point of it.
///
/// The dictionary says nothing about where to cut. The covering only
/// lines the text up with the reading, which is what gives each segment
/// its reading, and the one that exposes the most of the script's
/// boundaries is taken whatever the dictionary thinks of it. Empty when
/// the text is one run.
pub(in crate::core) fn script_cut(reading: &str, text: &str, lexicon: Lexicon) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    let starts = script_starts(&chars);
    cuts_where(reading, text, lexicon, 1, Way::Script(&starts))
        .pop()
        .unwrap_or_default()
}

/// `text` cut as finely as the dictionary cuts it: the opposite end from
/// [`ranked_cuts`], the most segments instead of the fewest, so a word
/// made of words comes apart (早稲田|大学, 東|京). The extreme on purpose,
/// and nothing holds it back: whatever the dictionary can tile comes
/// apart, down to single characters (プロ|グラ|ミング, あ|る). Which of
/// those is wanted is not this function's to say.
///
/// Empty when that leaves fewer than two segments.
pub(in crate::core) fn finest_cut(reading: &str, text: &str, lexicon: Lexicon) -> Vec<Piece> {
    cuts_where(reading, text, lexicon, 1, Way::Shortest)
        .pop()
        .unwrap_or_default()
}

/// What a cut is after.
#[derive(Debug, Clone, Copy)]
enum Way<'a> {
    /// The dictionary's words, the fewest segments: [`ranked_cuts`].
    Longest,
    /// The dictionary's words, the most segments: [`finest_cut`].
    Shortest,
    /// These places, where segments start by script alone:
    /// [`script_cut`]. The covering only lines the text up with the
    /// reading, and the one wanted hides the fewest of them in a step —
    /// before anything the dictionary says.
    Script(&'a [bool]),
}

/// For each char of `text`, whether a segment starts there by script
/// alone ([`script_cut`]): where the script is not the one before it. A
/// mark has no script of its own and goes with what it follows, as `ー`
/// goes with what it prolongs.
fn script_starts(text: &[char]) -> Vec<bool> {
    let mut runs: Vec<Option<Script>> = Vec::with_capacity(text.len());
    for &c in text {
        let run = match Script::of(c) {
            Script::ProlongedMark => runs.last().copied().flatten(),
            Script::Other if !is_kanji(c) => runs.last().copied().flatten(),
            script => Some(script),
        };
        runs.push(run);
    }
    (0..text.len())
        .map(|i| i > 0 && runs[i] != runs[i - 1])
        .collect()
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆')
}

/// The search for the cuts of one text: the coverings of `text[i..]` with
/// `reading[j..]`, worked out from the start of the text and only for the
/// positions a covering can pass through.
struct Search<'a> {
    text: &'a [char],
    /// The text and the reading as they compare ([`fold`]).
    folded: Vec<char>,
    reading: Vec<char>,
    /// The dictionary's words starting at each reading position.
    words_at: Vec<Vec<(usize, Vec<char>)>>,
    /// What each text char reads as on its own: a kanji's readings, an
    /// alphanumeric itself.
    readings_at: Vec<Vec<Vec<char>>>,
    way: Way<'a>,
    /// Per position, the cheapest distinct cuts by what they begin with:
    /// a dictionary word or a char of the reading, kanji read as listed,
    /// and a guessed word.
    firm: Vec<Option<Cuts>>,
    read: Vec<Option<Cuts>>,
    guessed: Vec<Option<Cuts>>,
}

type Cuts = Rc<Vec<Cut>>;

impl Search<'_> {
    fn at(&self, i: usize, j: usize) -> usize {
        i * (self.reading.len() + 1) + j
    }

    /// The ends `(i2, j2)` of every way to read `text[i..i2]` as
    /// `reading[j..j2]`: each char one of its own readings.
    fn read_ends(&self, i: usize, j: usize) -> Vec<(usize, usize)> {
        let mut ends = Vec::new();
        let mut reached = vec![j];
        for at in i..self.text.len() {
            let mut next = Vec::new();
            for &from in &reached {
                for own in &self.readings_at[at] {
                    let to = from + own.len();
                    if self.reading[from..].starts_with(own) && !next.contains(&to) {
                        next.push(to);
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            ends.extend(next.iter().map(|&to| (at + 1, to)));
            reached = next;
        }
        ends
    }

    /// What the step over `text[i..i2]` is charged, and whether a segment
    /// starts at it.
    fn charge(&self, i: usize, i2: usize, kind: Step, outside: usize) -> (bool, Cost) {
        let guess = kind == Step::Unknown;
        let (guessed, guessed_chars) = (usize::from(guess), if guess { i2 - i } else { 0 });
        let unknown = usize::from(matches!(kind, Step::Read | Step::Unknown));
        match self.way {
            Way::Longest => {
                // The one rule on top of the dictionary: a word that is a
                // single hiragana (は, を, に) joins the word before it.
                let particle = kind == Step::Word
                    && i2 - i == 1
                    && Script::of(self.text[i]) == Script::Hiragana;
                let starts = i > 0 && !particle && starts_segment(self.text, i, kind);
                // The fewest words before the fewest segments: that is
                // the longest match, and without it です would be で and
                // す, each joining as a particle.
                let word = usize::from(kind != Step::Pin);
                let segment = usize::from(starts);
                let cost = [outside, guessed, unknown, word, segment, guessed_chars];
                (starts, cost)
            }
            Way::Shortest => {
                let starts = i > 0 && starts_segment(self.text, i, kind);
                // Every char that starts no segment is charged, so the
                // fewest of them is the most segments.
                let joined = i2 - i - usize::from(starts);
                (
                    starts,
                    [outside, guessed, unknown, joined, 0, guessed_chars],
                )
            }
            Way::Script(script) => {
                let hidden = script[i + 1..i2].iter().filter(|&&s| s).count();
                (
                    script[i],
                    [hidden, outside, guessed, unknown, 0, guessed_chars],
                )
            }
        }
    }

    /// The guessed words in a cut.
    fn guesses(&self, cut: &Cut) -> usize {
        match self.way {
            Way::Longest | Way::Shortest => cut.cost[1],
            Way::Script(_) => cut.cost[2],
        }
    }

    /// Offer `here` the step from `(i, j)` to `(i2, j2)` followed by each
    /// of `rests`.
    fn take(
        &self,
        here: &mut Vec<Cut>,
        (i, j): (usize, usize),
        (i2, j2): (usize, usize),
        kind: Step,
        rests: &[Cuts],
    ) {
        if rests.iter().all(|rests| rests.is_empty()) {
            return;
        }
        let run = &self.folded[i..i2];
        // A kana or an alphanumeric no word took is outside the dictionary
        // — hiragana too, so a word the dictionary has is a word (今日|は)
        // and only what it lacks joins the word before it.
        let outside = match kind {
            Step::Word => 0,
            Step::Pin => usize::from(Script::of(self.text[i]) != Script::Other),
            Step::Read | Step::Unknown => run.len(),
        };
        // The kanji's own readings are not held against a guess the way
        // the dictionary is: they are too many and too short (大 is お, 人
        // と) not to turn up inside a reading that is right (大人, おとな).
        let doubtful = matches!(kind, Step::Read | Step::Unknown)
            && contradicts_dictionary(run, j, j2, &self.words_at);
        let (starts, step) = self.charge(i, i2, kind, outside);
        for rest in rests.iter().flat_map(|rests| rests.iter()) {
            offer(here, rest, step, doubtful, starts.then_some((i, j)));
        }
    }

    /// The cuts of `text[i..]` with `reading[j..]` that begin with a
    /// dictionary word or a char of the reading, and those that begin with
    /// kanji read as listed. Such a reading runs on until something else
    /// ends it: where one unknown word stops and the next starts, nothing
    /// says.
    fn solid(&mut self, i: usize, j: usize) -> (Cuts, Cuts) {
        let at = self.at(i, j);
        if let (Some(firm), Some(read)) = (&self.firm[at], &self.read[at]) {
            return (firm.clone(), read.clone());
        }
        let (n, m) = (self.text.len(), self.reading.len());
        let (mut firm, mut read) = (Vec::new(), Vec::new());
        if (i, j) == (n, m) {
            firm.push(Cut {
                cost: [0; 6],
                doubtful: false,
                starts: Vec::new(),
            });
        } else if i < n && j < m {
            let mut steps: Vec<(usize, usize, Step)> = self.words_at[j]
                .iter()
                .map(|(taken, surface)| (surface, i + surface.len(), j + taken))
                .filter(|&(surface, i2, j2)| {
                    i2 <= n && j2 <= m && self.folded[i..i2] == surface[..]
                })
                .map(|(_, i2, j2)| (i2, j2, Step::Word))
                .collect();
            if self.folded[i] == self.reading[j] {
                steps.push((i + 1, j + 1, Step::Pin));
            }
            for (i2, j2, kind) in steps {
                let (firm2, read2) = self.solid(i2, j2);
                let rests = [firm2, read2, self.guessed(i2, j2)];
                self.take(&mut firm, (i, j), (i2, j2), kind, &rests);
            }
            for (i2, j2) in self.read_ends(i, j) {
                let rests = [self.solid(i2, j2).0, self.guessed(i2, j2)];
                self.take(&mut read, (i, j), (i2, j2), Step::Read, &rests);
            }
        }
        let (firm, read) = (Rc::new(firm), Rc::new(read));
        self.firm[at] = Some(firm.clone());
        self.read[at] = Some(read.clone());
        (firm, read)
    }

    /// The cuts of `text[i..]` with `reading[j..]` that begin with a
    /// guessed word. The last resort: none when some cut does without a
    /// guess, and what follows the guess is not another one, which would
    /// be cut from it at a place nothing vouches for.
    fn guessed(&mut self, i: usize, j: usize) -> Cuts {
        let at = self.at(i, j);
        if let Some(done) = &self.guessed[at] {
            return done.clone();
        }
        let (firm, read) = self.solid(i, j);
        let (n, m) = (self.text.len(), self.reading.len());
        let mut here = Vec::new();
        let needed = firm
            .iter()
            .chain(read.iter())
            .all(|cut| self.guesses(cut) > 0);
        if i < n && j < m && needed {
            for len in 1..=MAX_RUN.min(n - i) {
                if Script::of(self.text[i + len - 1]).is_kana() {
                    break;
                }
                for taken in 1..=(len * MAX_READING_PER_CHAR).min(m - j) {
                    let (i2, j2) = (i + len, j + taken);
                    let (firm2, read2) = self.solid(i2, j2);
                    self.take(&mut here, (i, j), (i2, j2), Step::Unknown, &[firm2, read2]);
                }
            }
        }
        let here = Rc::new(here);
        self.guessed[at] = Some(here.clone());
        here
    }
}

/// The best `k` cuts of `text` into two segments or more, for what `way`
/// is after. One search for all three: they differ in where a segment
/// starts and in what a covering is charged for.
fn cuts_where(reading: &str, text: &str, lexicon: Lexicon, k: usize, way: Way) -> Vec<Vec<Piece>> {
    let reading: Vec<char> = reading.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (n, m) = (text.len(), reading.len());
    if n == 0 || m == 0 || n > MAX_CHARS || m > MAX_CHARS {
        return Vec::new();
    }
    let words_at = (0..m)
        .map(|j| {
            let rest: String = reading[j..].iter().collect();
            (lexicon.words)(&rest)
                .into_iter()
                .map(|(taken, surface)| (taken, surface.chars().map(fold).collect()))
                .collect()
        })
        .collect();
    let readings_at = text
        .iter()
        .map(|&c| match Script::of(c) {
            // Typed as itself (3 in 3人), it reads as itself.
            Script::Alphanumeric => vec![vec![fold(c)]],
            _ => (lexicon.readings)(c)
                .iter()
                .map(|r| r.chars().collect())
                .collect(),
        })
        .collect();
    let mut search = Search {
        text: &text,
        folded: text.iter().map(|&c| fold(c)).collect(),
        reading: reading.iter().map(|&c| fold(c)).collect(),
        words_at,
        readings_at,
        way,
        firm: vec![None; (n + 1) * (m + 1)],
        read: vec![None; (n + 1) * (m + 1)],
        guessed: vec![None; (n + 1) * (m + 1)],
    };

    // The cheapest covering says how much the dictionary knows of this
    // text; one it contradicts means the text is not the reading's
    // conversion, and then no cut of it can be trusted.
    let (firm, read) = search.solid(0, 0);
    let guessed = search.guessed(0, 0);
    let mut cuts = Vec::new();
    for cut in firm.iter().chain(read.iter()).chain(guessed.iter()) {
        offer(&mut cuts, cut, [0; 6], false, None);
    }
    let Some(best) = cuts.first().filter(|best| !best.doubtful) else {
        return Vec::new();
    };
    // Cutting by script the first component is the script's, and the
    // dictionary's knowledge is not what the cuts are held to.
    let known = |cut: &Cut| match way {
        Way::Longest | Way::Shortest => [cut.cost[0], cut.cost[1], cut.cost[2]],
        Way::Script(_) => [cut.cost[0], 0, 0],
    };
    cuts.iter()
        .filter(|cut| !cut.doubtful && known(cut) == known(best))
        .filter(|cut| !cut.starts.is_empty())
        .take(k)
        .map(|cut| {
            let ends = cut.starts.iter().rev().copied().chain([(n, m)]);
            let mut from = (0, 0);
            ends.map(|to| {
                let piece = Piece {
                    reading: reading[from.1..to.1].iter().collect(),
                    text: text[from.0..to.0].iter().collect(),
                };
                from = to;
                piece
            })
            .collect()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A lookup over `(reading, surface)` pairs.
    fn lookup<'a>(entries: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Vec<(usize, String)> + 'a {
        move |rest: &str| {
            entries
                .iter()
                .filter(|(reading, _)| rest.starts_with(reading))
                .map(|(reading, surface)| (reading.chars().count(), surface.to_string()))
                .collect()
        }
    }

    /// The dictionary `entries` and the single-kanji data.
    fn lexicon<'a>(words: WordLookup<'a>) -> Lexicon<'a> {
        Lexicon {
            words,
            readings: &|c| karukan_engine::single_kanji_readings(c),
        }
    }

    /// The first cut, written as `reading=text` pieces joined by `|`; the
    /// whole as one piece when nothing cuts it.
    fn split_with(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        match ranked_cuts(reading, text, lexicon(&lookup(entries)), 1).first() {
            Some(pieces) => pieces
                .iter()
                .map(|p| format!("{}={}", p.reading, p.text))
                .collect::<Vec<_>>()
                .join("|"),
            None => format!("{reading}={text}"),
        }
    }

    /// The first cut with an empty dictionary.
    fn split(reading: &str, text: &str) -> String {
        split_with(reading, text, &[])
    }

    /// A small dictionary for a sentence.
    const SENTENCE: [(&str, &str); 6] = [
        ("きょう", "今日"),
        ("は", "は"),
        ("いしゃ", "医者"),
        ("に", "に"),
        ("いく", "行く"),
        ("い", "行"),
    ];

    #[test]
    fn a_single_hiragana_word_joins_the_word_before_it() {
        // は and に are words the dictionary has, of one hiragana each.
        assert_eq!(
            split_with("きょうはいしゃにいく", "今日は医者に行く", &SENTENCE),
            "きょうは=今日は|いしゃに=医者に|いく=行く"
        );
        // Two in a row both join; a longer hiragana word is a segment,
        // even with each of its kana a word of its own in the dictionary
        // (as nearly every kana is in the real one).
        let entries = [
            ("がっこう", "学校"),
            ("に", "に"),
            ("は", "は"),
            ("から", "から"),
            ("か", "か"),
            ("ら", "ら"),
            ("いく", "行く"),
        ];
        assert_eq!(
            split_with("がっこうにはいく", "学校には行く", &entries),
            "がっこうには=学校には|いく=行く"
        );
        assert_eq!(
            split_with("がっこうからいく", "学校から行く", &entries),
            "がっこう=学校|から=から|いく=行く"
        );
    }

    #[test]
    fn hiragana_the_dictionary_lacks_joins_the_word_before_it() {
        // No は or に here: nothing to make them words.
        let entries = [("きょう", "今日"), ("いしゃ", "医者"), ("いく", "行く")];
        assert_eq!(
            split_with("きょうはいしゃにいく", "今日は医者に行く", &entries),
            "きょうは=今日は|いしゃに=医者に|いく=行く"
        );
    }

    #[test]
    fn words_the_dictionary_lacks_are_settled_by_elimination() {
        // No dictionary at all: the kana fix where each unknown word's
        // reading starts and ends.
        assert_eq!(
            split("きょうはいしゃにいく", "今日は医者に行く"),
            "きょうは=今日は|いしゃに=医者に|いく=行く"
        );
        // 忖度 is not in the dictionary; its neighbours are, and what is
        // left of the reading between them is its own.
        let entries = [("じょうし", "上司"), ("ぶか", "部下")];
        assert_eq!(
            split_with("じょうしそんたくぶか", "上司忖度部下", &entries),
            "じょうし=上司|そんたく=忖度|ぶか=部下"
        );
    }

    #[test]
    fn kanji_the_dictionary_lacks_are_read_by_their_own_readings() {
        // は five times over, and no dictionary: 母 reads はは and 歯 は,
        // not the other way round.
        assert_eq!(split("ははははは", "母は歯は"), "ははは=母は|はは=歯は");
        // A digit typed as a digit reads as itself, inside the word.
        assert_eq!(split("3にんでいく", "3人で行く"), "3にんで=3人で|いく=行く");
        // Kanji that read on are one word: nothing says where to cut.
        assert_eq!(
            split("とうきょうだいがくにいく", "東京大学に行く"),
            "とうきょうだいがくに=東京大学に|いく=行く"
        );
    }

    #[test]
    fn a_reading_no_kanji_has_is_still_settled_by_elimination() {
        // 大人 is not おとな by its kanji: the guess is what is left.
        assert_eq!(
            split("おとなしいひと", "大人しい人"),
            "おとなしい=大人しい|ひと=人"
        );
        // A guess does not stop where its kanji happen to read short (大
        // is お, 人 と), and is not doubted for it.
        assert_eq!(
            split("とうきょうできょうあう", "東京で今日会う"),
            "とうきょうで=東京で|きょうあう=今日会う"
        );
    }

    #[test]
    fn a_dropped_kana_does_not_cut_the_kanji_around_it() {
        // に went missing from 上司に忖度部下. With no dictionary the run
        // is one guess; 忖度 is not handed にそんたく between its
        // neighbours.
        assert_eq!(
            split("じょうしにそんたくぶかがいく", "上司忖度部下が行く"),
            "じょうしにそんたくぶかが=上司忖度部下が|いく=行く"
        );
    }

    #[test]
    fn dictionary_words_cut_a_run_of_kanji() {
        let entries = [
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("きょういく", "教育"),
            ("がくぶ", "学部"),
        ];
        assert_eq!(
            split_with(
                "わせだだいがくきょういくがくぶ",
                "早稲田大学教育学部",
                &entries
            ),
            "わせだ=早稲田|だいがく=大学|きょういく=教育|がくぶ=学部"
        );
    }

    #[test]
    fn the_longest_dictionary_word_wins() {
        let entries = [("とう", "東"), ("きょう", "京"), ("とうきょう", "東京")];
        assert_eq!(
            split_with("とうきょうにいく", "東京に行く", &entries),
            "とうきょうに=東京に|いく=行く"
        );
    }

    #[test]
    fn a_dictionary_word_is_one_unit_whatever_it_is_written_in() {
        // ご存知 begins in hiragana and ends in kanji.
        let entries = [("ごぞんじ", "ご存知"), ("ぞんじ", "存知"), ("それ", "それ")];
        assert_eq!(
            split_with("それはごぞんじなどは", "それはご存知などは", &entries),
            "それは=それは|ごぞんじなどは=ご存知などは"
        );
    }

    #[test]
    fn a_dictionary_word_fixes_what_the_kana_leave_open() {
        // は three times over: 母 could be は or はは as far as the kana
        // go. The dictionary knows which.
        assert_eq!(
            split_with("はははいしゃです", "母は医者です", &[("はは", "母")]),
            "ははは=母は|いしゃです=医者です"
        );
    }

    #[test]
    fn katakana_words_are_cut_by_the_dictionary() {
        let entries = [("てすと", "テスト"), ("けーす", "ケース"), ("かく", "書く")];
        assert_eq!(
            split_with("てすとけーすをかく", "テストケースを書く", &entries),
            "てすと=テスト|けーすを=ケースを|かく=書く"
        );
    }

    #[test]
    fn katakana_and_alphanumerics_the_dictionary_lacks_are_words_too() {
        assert_eq!(
            split("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            "きょうぎ=競技|ぷろぐらみんぐ=プログラミング"
        );
        assert_eq!(
            split("これはくろーどです", "これはClaudeです"),
            "これは=これは|くろーどです=Claudeです"
        );
    }

    #[test]
    fn the_long_vowel_does_not_end_a_katakana_word() {
        assert_eq!(
            split("こんぴゅーたをつかう", "コンピュータを使う"),
            "こんぴゅーたを=コンピュータを|つかう=使う"
        );
        assert_eq!(
            split("らーめんをたべる", "ラーメンを食べる"),
            "らーめんを=ラーメンを|たべる=食べる"
        );
        assert_eq!(
            split("さーばーにつなぐ", "サーバーに繋ぐ"),
            "さーばーに=サーバーに|つなぐ=繋ぐ"
        );
        // After hiragana it prolongs no word, and katakana starts one.
        assert_eq!(
            split("らーめんをたべる", "らーメンを食べる"),
            "らー=らー|めんを=メンを|たべる=食べる"
        );
    }

    #[test]
    fn hiragana_and_marks_belong_to_the_word_before_them() {
        assert_eq!(
            split("おい、おまえだよ。", "おい、お前だよ。"),
            "おい、お=おい、お|まえだよ。=前だよ。"
        );
        // With the dictionary knowing the word, its prefix is part of it.
        assert_eq!(
            split_with(
                "おい、おまえだよ。",
                "おい、お前だよ。",
                &[("おまえ", "お前")]
            ),
            "おい、=おい、|おまえだよ。=お前だよ。"
        );
        // Written in hiragana alone there is no word to start a segment.
        assert_eq!(
            split("これはここにある", "これはここにある"),
            "これはここにある=これはここにある"
        );
        assert_eq!(split("らーめん", "らーめん"), "らーめん=らーめん");
    }

    #[test]
    fn a_conversion_that_dropped_kana_stays_whole() {
        // The model answered わせだにだいがく with 早稲田大学: に has nowhere
        // to go but into a neighbour, and the dictionary says neither
        // neighbour reads that way.
        let entries = [("わせだ", "早稲田"), ("だいがく", "大学")];
        assert_eq!(
            split_with("わせだにだいがくへいく", "早稲田大学へ行く", &entries),
            "わせだにだいがくへいく=早稲田大学へ行く"
        );
        // The faithful conversion of the same reading splits as usual.
        assert_eq!(
            split_with("わせだにだいがくへいく", "早稲田に大学へ行く", &entries),
            "わせだに=早稲田に|だいがくへ=大学へ|いく=行く"
        );
    }

    #[test]
    fn width_differences_still_line_up() {
        // The model answers in half-width; settling made it full-width.
        assert_eq!(
            split("3にんでいく", "３人で行く"),
            "3にんで=３人で|いく=行く"
        );
    }

    #[test]
    fn one_word_is_one_segment() {
        assert_eq!(split("とうきょうと", "東京都"), "とうきょうと=東京都");
    }

    #[test]
    fn text_that_is_not_the_readings_conversion_stays_whole() {
        // The model rewrote a kana: nothing lines up.
        assert_eq!(
            split("こんにちわせかい", "こんにちは世界"),
            "こんにちわせかい=こんにちは世界"
        );
        assert_eq!(split("", ""), "=");
    }

    /// The ranked cuts, each written as its texts joined by `|`.
    fn ranked(reading: &str, text: &str, entries: &[(&str, &str)]) -> Vec<String> {
        ranked_cuts(reading, text, lexicon(&lookup(entries)), 3)
            .iter()
            .map(|pieces| {
                pieces
                    .iter()
                    .map(|p| p.text.as_str())
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    #[test]
    fn every_cut_concatenates_back() {
        let entries = [
            ("きょう", "今日"),
            ("は", "は"),
            ("いしゃ", "医者"),
            ("に", "に"),
            ("いく", "行く"),
        ];
        let (reading, text) = ("きょうはいしゃにいく", "今日は医者に行く");
        let lookup = lookup(&entries);
        let cuts = [
            ranked_cuts(reading, text, lexicon(&lookup), 1)
                .pop()
                .unwrap(),
            finest_cut(reading, text, lexicon(&lookup)),
            script_cut(reading, text, lexicon(&lookup)),
        ];
        for pieces in cuts {
            assert!(pieces.len() > 1);
            let joined = |f: fn(&Piece) -> &str| pieces.iter().map(f).collect::<String>();
            assert_eq!(joined(|p| &p.reading), reading);
            assert_eq!(joined(|p| &p.text), text);
        }
    }

    #[test]
    fn ranked_cuts_start_from_the_longest_match() {
        let entries = [
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("わせだだいがく", "早稲田大学"),
            ("に", "に"),
            ("いく", "行く"),
        ];
        let cuts = ranked("わせだだいがくにいく", "早稲田大学に行く", &entries);
        // Fewest segments first: the longest word, に with it.
        assert_eq!(cuts[0], "早稲田大学に|行く");
        // Then one segment more: the long word gives way.
        assert_eq!(cuts[1], "早稲田|大学に|行く");
    }

    #[test]
    fn ranked_cuts_of_one_dictionary_word_are_its_parts() {
        let entries = [
            ("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            ("きょうぎ", "競技"),
            ("ぷろぐらみんぐ", "プログラミング"),
            ("ぷろ", "プロ"),
            ("ぐら", "グラ"),
            ("みんぐ", "ミング"),
        ];
        let cuts = ranked("きょうぎぷろぐらみんぐ", "競技プログラミング", &entries);
        // The whole word is no cut; its two parts are the first.
        assert_eq!(cuts[0], "競技|プログラミング");
        // Then all the way down. Nothing in between: プロ with the rest
        // left over as loose katakana is not a cut.
        assert_eq!(cuts[1..], ["競技|プロ|グラ|ミング"]);
    }

    #[test]
    fn ranked_cuts_split_hiragana_at_its_words() {
        let entries = [
            ("これ", "これ"),
            ("は", "は"),
            ("ここ", "ここ"),
            ("に", "に"),
            ("ある", "ある"),
        ];
        let cuts = ranked("これはここにある", "これはここにある", &entries);
        // The words of two characters are segments; は and に join.
        assert_eq!(cuts[0], "これは|ここに|ある");
        // The same with no は or に in the dictionary at all.
        let fewer = [("これ", "これ"), ("ここ", "ここ"), ("ある", "ある")];
        let cuts = ranked("これはここにある", "これはここにある", &fewer);
        assert_eq!(cuts[0], "これは|ここに|ある");
    }

    #[test]
    fn ranked_cuts_trust_nothing_the_dictionary_contradicts() {
        let entries = [("わせだ", "早稲田"), ("だいがく", "大学")];
        assert!(ranked("わせだにだいがく", "早稲田大学", &entries).is_empty());
        // One word, nothing to cut.
        assert!(ranked("きょう", "今日", &[("きょう", "今日")]).is_empty());
        // A word the dictionary lacks is not cut at a guess.
        assert!(ranked("わせだだいがく", "早稲田大学", &[]).is_empty());
    }

    /// The cut by script, written like [`split_with`]'s; empty when it
    /// does not cut.
    fn by_script(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        script_cut(reading, text, lexicon(&lookup(entries)))
            .iter()
            .map(|p| format!("{}={}", p.reading, p.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn the_script_cuts_wherever_the_script_changes() {
        let entries = [
            ("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            ("を", "を"),
            ("まなぶ", "学ぶ"),
        ];
        // Whatever the dictionary has as one word: its long compound, and
        // a verb with its ending.
        assert_eq!(
            by_script(
                "きょうぎぷろぐらみんぐをまなぶ",
                "競技プログラミングを学ぶ",
                &entries
            ),
            "きょうぎ=競技|ぷろぐらみんぐ=プログラミング|を=を|まな=学|ぶ=ぶ"
        );
        // Kanji that run on are one run.
        assert_eq!(
            by_script("わせだだいがくにいく", "早稲田大学に行く", &[]),
            "わせだだいがく=早稲田大学|に=に|い=行|く=く"
        );
    }

    #[test]
    fn the_script_cut_knows_nothing_but_the_script() {
        // No prefix, no particle: お goes with the は before it.
        assert_eq!(
            by_script("きょうはおちゃ", "今日はお茶", &[("おちゃ", "お茶")]),
            "きょう=今日|はお=はお|ちゃ=茶"
        );
        // A word that changes script comes apart too.
        assert_eq!(
            by_script("ごぞんじ", "ご存知", &[("ごぞんじ", "ご存知")]),
            "ご=ご|ぞんじ=存知"
        );
    }

    #[test]
    fn marks_and_the_long_vowel_have_no_script_of_their_own() {
        // ー goes with what it prolongs, a mark with what it follows.
        assert_eq!(by_script("らーめん", "らーめん", &[]), "");
        assert_eq!(
            by_script("たわー、いく", "タワー、行く", &[]),
            "たわー、=タワー、|い=行|く=く"
        );
    }

    /// The finest cut, written like [`split_with`]'s; empty when it does
    /// not cut.
    fn finest(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        finest_cut(reading, text, lexicon(&lookup(entries)))
            .iter()
            .map(|p| format!("{}={}", p.reading, p.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn the_finest_cut_is_every_dictionary_word() {
        let entries = [
            ("わせだだいがく", "早稲田大学"),
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("に", "に"),
            ("は", "は"),
            ("いく", "行く"),
        ];
        assert_eq!(
            finest("わせだだいがくにはいく", "早稲田大学には行く", &entries),
            "わせだ=早稲田|だいがく=大学|に=に|は=は|いく=行く"
        );
    }

    #[test]
    fn the_finest_cut_goes_down_to_single_characters() {
        let entries = [
            ("とうきょう", "東京"),
            ("とう", "東"),
            ("きょう", "京"),
            ("あ", "あ"),
            ("る", "る"),
            ("ある", "ある"),
            ("に", "に"),
        ];
        // Nothing is held back: which cut is wanted is the user's to say.
        assert_eq!(finest("とうきょう", "東京", &entries), "とう=東|きょう=京");
        assert_eq!(
            finest("とうきょうにある", "東京にある", &entries),
            "とう=東|きょう=京|に=に|あ=あ|る=る"
        );
    }

    #[test]
    fn the_finest_cut_takes_apart_whatever_the_dictionary_tiles() {
        let entries = [
            ("ぷろぐらみんぐ", "プログラミング"),
            ("ぷろ", "プロ"),
            ("ぐら", "グラ"),
            ("みんぐ", "ミング"),
        ];
        // The extreme, as asked for.
        assert_eq!(
            finest("ぷろぐらみんぐ", "プログラミング", &entries),
            "ぷろ=プロ|ぐら=グラ|みんぐ=ミング"
        );
    }
}
