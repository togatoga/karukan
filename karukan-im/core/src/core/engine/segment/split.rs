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
//! The text is covered, left to right, by three kinds of step, each taking
//! a stretch of the reading with it:
//!
//! - a dictionary word: its reading at this place in the reading, its
//!   surface at this place in the text;
//! - a character that is the reading's own, and can only be that one (a
//!   kana the dictionary did not cover, a digit typed as a digit);
//! - anything else, taking whatever reading is left for it. With its
//!   neighbours fixed by the two above, that is its own reading: a word
//!   the dictionary lacks is settled by elimination.
//!
//! Of all the ways to cover the text, the one wanted leaves the fewest
//! characters outside a dictionary word, and among those uses the fewest
//! units. That is all the cost there is, and it is what "longest match"
//! means: as much as possible by the dictionary, in as few words as
//! possible.
//!
//! A segment starts at each word, and how the hiragana around the words is
//! handled is the only thing the three ways of cutting ([`Granularity`])
//! differ in.
//!
//! Pure functions of the two strings and a word lookup, engine-free. A
//! split that comes out wrong costs the user a keystroke, and a resized
//! segment is converted again from its reading, so nothing here has to be
//! right to be safe.

/// One segment: a stretch of the reading and the text it converted to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core) struct Piece {
    pub reading: String,
    pub text: String,
}

/// Longest reading/text this bothers to align; beyond it the whole thing
/// stays one segment. The alignment is quadratic in both.
const MAX_CHARS: usize = 200;

/// Text chars one unknown word may span, and reading chars it may take per
/// text char: bounds on the search, generous for real words (承る is 5 for
/// 1) and for a loanword spelled out (くろーど for Claude).
const MAX_RUN: usize = 16;
const MAX_READING_PER_CHAR: usize = 6;

/// Dictionary words starting at the head of a reading: for each, how many
/// reading chars it takes and its surface.
pub(in crate::core) type WordLookup<'a> = &'a dyn Fn(&str) -> Vec<(usize, String)>;

/// How a sentence is cut into segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::core) enum Granularity {
    /// 文節: the dictionary's longest words, each with the hiragana that
    /// follows it — its particles and inflection (今日は|医者に|行く).
    #[default]
    Phrases,
    /// 単語: the dictionary's longest words and nothing joined to them, so
    /// a particle the dictionary has is a segment of its own
    /// (今日|は|医者|に|行く).
    Words,
    /// 細かく: the words of 単語, each given way to the words it is made
    /// of — the fewest that make it, so it comes apart one level and no
    /// further (競技プログラミング into 競技|プログラミング, not on into
    /// プロ|グラ|ミング), and never into single characters.
    Fine,
}

impl Granularity {
    /// Every way of cutting, the coarsest first.
    pub(in crate::core) const COARSEST_FIRST: [Self; 3] = [Self::Phrases, Self::Words, Self::Fine];

    /// Whether dictionary words written in hiragana alone are units. Not
    /// for 文節: the dictionary has no parts of speech, so は and に would
    /// be words like any other, and what makes a 文節 is that they are not.
    fn uses_hiragana_words(self) -> bool {
        self != Self::Phrases
    }
}

/// What a covering costs, compared left to right: the text chars left
/// outside a dictionary word; then the words of a single character, counted
/// only when a word is taken apart; then the number of units.
type Cost = (usize, usize, usize);

/// What is being covered: a sentence, or one dictionary word by the words
/// it is made of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Cover {
    Sentence,
    /// The word itself is not an answer, and neither is anything but
    /// dictionary words of two characters or more.
    Parts,
}

/// A covering no one can take.
const UNREACHABLE: Cost = (usize::MAX, 0, 0);

fn add(rest: Cost, step: Cost) -> Cost {
    (rest.0 + step.0, rest.1 + step.1, rest.2 + step.2)
}

impl Cover {
    /// A dictionary word of `len` text chars.
    fn word(self, len: usize) -> Cost {
        match self {
            Self::Sentence => (0, 0, 1),
            Self::Parts => (0, usize::from(len == 1), 1),
        }
    }
}

/// An unknown word of `len` text chars.
fn unknown(len: usize) -> Cost {
    (len, 0, 1)
}

/// A char that is the reading's own: outside the dictionary, and no unit.
const PIN: Cost = (1, 0, 0);

/// What a step of the covering is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// One char that is the reading's own.
    Pin,
    /// A dictionary word.
    Word,
    /// Text taking what reading is left: a word the dictionary lacks.
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
            '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' => Self::Other,
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

    fn is_hiragana(self) -> bool {
        matches!(self, Self::Hiragana | Self::ProlongedMark)
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

/// For every text position where a step starts, the reading position it
/// lines up with and the step that starts there — `None` inside a step,
/// where no single position exists. `None` overall when the text cannot be
/// the reading's conversion at all (the model rewrote a kana), or when
/// either is too long to align.
fn align(
    reading: &[char],
    text: &[char],
    words: WordLookup,
    granularity: Granularity,
    cover: Cover,
) -> Option<Vec<Option<(usize, Step)>>> {
    let (n, m) = (text.len(), reading.len());
    if n == 0 || m == 0 || n > MAX_CHARS || m > MAX_CHARS {
        return None;
    }
    let folded_text: Vec<char> = text.iter().map(|&c| fold(c)).collect();
    let folded_reading: Vec<char> = reading.iter().map(|&c| fold(c)).collect();

    // Dictionary words by where their reading starts, surfaces folded like
    // the text.
    let words_at: Vec<Vec<(usize, Vec<char>)>> = (0..m)
        .map(|j| {
            let rest: String = reading[j..].iter().collect();
            words(&rest)
                .into_iter()
                .filter(|(_, surface)| {
                    granularity.uses_hiragana_words()
                        || !surface.chars().all(|c| Script::of(c).is_hiragana())
                })
                .map(|(taken, surface)| (taken, surface.chars().map(fold).collect()))
                .collect()
        })
        .collect();

    // best[i][j]: the cheapest covering of text[i..] with reading[j..], and
    // the step it starts with: (next i, next j, what it is).
    let mut best = vec![vec![UNREACHABLE; m + 1]; n + 1];
    let mut step = vec![vec![(0usize, 0usize, Step::Pin); m + 1]; n + 1];
    best[n][m] = (0, 0, 0);
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let mut take = |i2: usize, j2: usize, kind: Step, cost: Cost| {
                let rest = best[i2][j2];
                if rest != UNREACHABLE && add(rest, cost) < best[i][j] {
                    best[i][j] = add(rest, cost);
                    step[i][j] = (i2, j2, kind);
                }
            };
            // A dictionary word: its surface here, its reading there.
            for (taken, surface) in &words_at[j] {
                let (i2, j2) = (i + surface.len(), j + taken);
                let whole = (i, j, i2, j2) == (0, 0, n, m);
                if i2 <= n
                    && j2 <= m
                    && folded_text[i..i2] == surface[..]
                    && !(whole && cover == Cover::Parts)
                {
                    take(i2, j2, Step::Word, cover.word(surface.len()));
                }
            }
            // This char is that reading char.
            if folded_text[i] == folded_reading[j] {
                take(i + 1, j + 1, Step::Pin, PIN);
            }
            // An unknown word taking a stretch of the reading.
            for len in 1..=MAX_RUN.min(n - i) {
                if Script::of(text[i + len - 1]).is_kana() {
                    break;
                }
                for taken in 1..=(len * MAX_READING_PER_CHAR).min(m - j) {
                    take(i + len, j + taken, Step::Unknown, unknown(len));
                }
            }
        }
    }
    if best[0][0] == UNREACHABLE {
        return None;
    }
    // A word comes apart only into whole words, none of a single char.
    if cover == Cover::Parts && (best[0][0].0, best[0][0].1) != (0, 0) {
        return None;
    }

    let mut positions = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n {
        let (i2, j2, kind) = step[i][j];
        positions[i] = Some((j, kind));
        if kind == Step::Unknown && contradicts_dictionary(&folded_text[i..i2], j, j2, &words_at) {
            return None;
        }
        (i, j) = (i2, j2);
    }
    Some(positions)
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
        Step::Word | Step::Unknown => true,
        // So does a run of katakana or alphanumerics the dictionary did
        // not cover, at its first char. Hiragana and marks never do: they
        // belong to the word before them.
        Step::Pin => {
            let is_word_char = |c: char| Script::of(c).is_word_char();
            is_word_char(text[i]) && !is_word_char(text[i - 1])
        }
    }
}

/// The fine cut: each dictionary word in `positions` gives way to the
/// fewest dictionary words that make it, where there are such. One level
/// only — the parts are not taken apart in turn, which is what would turn
/// プログラミング into プロ|グラ|ミング.
fn take_words_apart(
    positions: &mut [Option<(usize, Step)>],
    reading: &[char],
    text: &[char],
    words: WordLookup,
) {
    let starts: Vec<usize> = (0..text.len())
        .filter(|&i| positions[i].is_some())
        .collect();
    for (k, &i) in starts.iter().enumerate() {
        let Some((j, Step::Word)) = positions[i] else {
            continue;
        };
        let i2 = starts.get(k + 1).copied().unwrap_or(text.len());
        let j2 = positions
            .get(i2)
            .copied()
            .flatten()
            .map_or(reading.len(), |(j2, _)| j2);
        let Some(parts) = align(
            &reading[j..j2],
            &text[i..i2],
            words,
            Granularity::Fine,
            Cover::Parts,
        ) else {
            continue;
        };
        for (offset, part) in parts.into_iter().enumerate() {
            positions[i + offset] = part.map(|(at, step)| (j + at, step));
        }
    }
}

/// Split `text`, the conversion of `reading`, into segments. Always at
/// least one piece, and the pieces always concatenate back to both strings:
/// when the two cannot be lined up, the whole is a single segment.
pub(in crate::core) fn split_segments(
    reading: &str,
    text: &str,
    words: WordLookup,
    granularity: Granularity,
) -> Vec<Piece> {
    let whole = || {
        vec![Piece {
            reading: reading.to_string(),
            text: text.to_string(),
        }]
    };
    let reading_chars: Vec<char> = reading.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let Some(mut positions) = align(
        &reading_chars,
        &text_chars,
        words,
        granularity,
        Cover::Sentence,
    ) else {
        return whole();
    };
    if granularity == Granularity::Fine {
        take_words_apart(&mut positions, &reading_chars, &text_chars, words);
    }

    let mut pieces = Vec::new();
    let (mut text_start, mut reading_start) = (0, 0);
    for i in 1..text_chars.len() {
        let Some((reading_at, step)) = positions[i] else {
            continue;
        };
        if !starts_segment(&text_chars, i, step) {
            continue;
        }
        pieces.push(Piece {
            reading: reading_chars[reading_start..reading_at].iter().collect(),
            text: text_chars[text_start..i].iter().collect(),
        });
        (text_start, reading_start) = (i, reading_at);
    }
    pieces.push(Piece {
        reading: reading_chars[reading_start..].iter().collect(),
        text: text_chars[text_start..].iter().collect(),
    });
    pieces
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

    /// The split written as `reading=text` pieces joined by `|`.
    fn split_at(
        reading: &str,
        text: &str,
        entries: &[(&str, &str)],
        granularity: Granularity,
    ) -> String {
        split_segments(reading, text, &lookup(entries), granularity)
            .iter()
            .map(|p| format!("{}={}", p.reading, p.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    /// Cut into 文節.
    fn split_with(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        split_at(reading, text, entries, Granularity::Phrases)
    }

    /// Cut into 文節 with an empty dictionary.
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
    fn a_segment_is_a_word_and_the_hiragana_after_it() {
        assert_eq!(
            split_with("きょうはいしゃにいく", "今日は医者に行く", &SENTENCE),
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
    fn the_three_ways_of_cutting() {
        let entries = [
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("わせだだいがく", "早稲田大学"),
            ("は", "は"),
            ("とう", "東"),
            ("きょう", "京"),
            ("とうきょう", "東京"),
            ("に", "に"),
            ("ある", "ある"),
        ];
        let (reading, text) = ("わせだだいがくはとうきょうにある", "早稲田大学は東京にある");
        // 文節: the longest words, each with the hiragana after it.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Phrases),
            "わせだだいがくは=早稲田大学は|とうきょうにある=東京にある"
        );
        // 単語: the longest words, and the particles the dictionary has.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Words),
            "わせだだいがく=早稲田大学|は=は|とうきょう=東京|に=に|ある=ある"
        );
        // 細かく: the words those are made of — never down to single
        // kanji.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Fine),
            "わせだ=早稲田|だいがく=大学|は=は|とうきょう=東京|に=に|ある=ある"
        );
    }

    #[test]
    fn the_fine_cut_takes_a_word_apart_one_level() {
        let entries = [
            ("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            ("きょうぎ", "競技"),
            ("ぷろぐらみんぐ", "プログラミング"),
            ("ぷろ", "プロ"),
            ("ぐら", "グラ"),
            ("みんぐ", "ミング"),
            ("とうきょうと", "東京都"),
            ("とうきょう", "東京"),
            ("と", "都"),
        ];
        let fine = |reading, text| split_at(reading, text, &entries, Granularity::Fine);
        // The fewest words that make it, not the most.
        assert_eq!(
            fine("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            "きょうぎ=競技|ぷろぐらみんぐ=プログラミング"
        );
        // Asked for on its own, the part comes apart in turn.
        assert_eq!(
            fine("ぷろぐらみんぐ", "プログラミング"),
            "ぷろ=プロ|ぐら=グラ|みんぐ=ミング"
        );
        // Not when a part would be a single character.
        assert_eq!(fine("とうきょうと", "東京都"), "とうきょうと=東京都");
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
            "3=３|にんで=人で|いく=行く"
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

    #[test]
    fn pieces_always_concatenate_back() {
        for granularity in [Granularity::Phrases, Granularity::Words, Granularity::Fine] {
            for (reading, text) in [
                ("きょうはいしゃにいく", "今日は医者に行く"),
                ("こんにちわせかい", "こんにちは世界"),
                ("あ", "亜"),
            ] {
                let pieces = split_segments(reading, text, &lookup(&SENTENCE), granularity);
                let r: String = pieces.iter().map(|p| p.reading.as_str()).collect();
                let t: String = pieces.iter().map(|p| p.text.as_str()).collect();
                assert_eq!((r.as_str(), t.as_str()), (reading, text));
            }
        }
    }
}
