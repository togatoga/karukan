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

/// One segment: a stretch of the reading and the text it converted to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::core) struct Piece {
    pub reading: String,
    pub text: String,
}

/// Text chars one unknown word may span, and reading chars it may take per
/// text char: bounds on the search, generous for real words (承る is 5 for
/// 1) and for a loanword spelled out (くろーど for Claude).
const MAX_RUN: usize = 16;
const MAX_READING_PER_CHAR: usize = 6;

/// Dictionary words starting at the head of a reading: for each, how many
/// reading chars it takes and its surface.
pub(in crate::core) type WordLookup<'a> = &'a dyn Fn(&str) -> Vec<(usize, String)>;

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

/// Longest reading/text this bothers to cut; beyond it the whole thing
/// stays one segment. The search is quadratic in both and keeps several
/// coverings per position.
const MAX_CHARS: usize = 120;

/// Coverings kept per position while ranking. More than are asked for,
/// since some are dropped at the end.
const KEEP: usize = 8;

/// One way of cutting, while it is being ranked.
#[derive(Debug, Clone)]
struct Cut {
    /// Compared left to right: the chars outside a dictionary word
    /// (hiragana and marks aside: those belong to the word before them);
    /// the unknown words; the segments; the segments that start with a
    /// word of a single character. The last two are what the [`Way`]
    /// makes them: cutting finely, the single-character kanji words and
    /// the chars that start no segment; by script, the script's
    /// boundaries left inside a step, and nothing.
    cost: (usize, usize, usize, usize),
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
    step: (usize, usize, usize, usize),
    doubtful: bool,
    start: Option<(usize, usize)>,
) {
    let cost = (
        rest.cost.0 + step.0,
        rest.cost.1 + step.1,
        rest.cost.2 + step.2,
        rest.cost.3 + step.3,
    );
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
/// Every dictionary word is a unit here, hiragana ones too (今日|は): a
/// hiragana char no word took counts as left outside the dictionary, so
/// the dictionary's word for it always wins, and only hiragana it lacks
/// joins the word before it. Among the coverings that leave the least
/// outside the dictionary, the ones with the fewest segments come first:
/// the longest match, then what it gives way to — a word into the words
/// it is made of (競技|プログラミング). Coverings with the same number of
/// segments come in no particular order, except that one starting a
/// segment with a single-character word comes after one that does not.
///
/// Empty when the text is not the reading's conversion, when it is too
/// long to rank, or when nothing cuts it.
pub(in crate::core) fn ranked_cuts(
    reading: &str,
    text: &str,
    words: WordLookup,
    k: usize,
) -> Vec<Vec<Piece>> {
    cuts_where(reading, text, words, k, Way::Longest)
}

/// `text` cut by script alone: a segment starts where a run of kanji, of
/// katakana or of alphanumerics does, and the hiragana and marks after a
/// run are its own (競技|プログラミングを|学ぶ). Two things a run's first
/// char does not say are allowed for: お or ご right before kanji is that
/// word's prefix and starts the segment in its place (お茶を|飲む), and a
/// single hiragana char at the head of the text — a particle carried over
/// from what was typed before — makes no segment of its own.
///
/// The dictionary says nothing about where to cut here. It still lines
/// the text up with the reading, which is what gives each segment its
/// reading, so a boundary that falls inside the only word covering it
/// (ご存知) is not cut. Empty when that leaves fewer than two segments.
pub(in crate::core) fn script_cut(reading: &str, text: &str, words: WordLookup) -> Vec<Piece> {
    let chars: Vec<char> = text.chars().collect();
    let starts = script_starts(&chars);
    cuts_where(reading, text, words, 1, Way::Script(&starts))
        .pop()
        .unwrap_or_default()
}

/// `text` cut as finely as the dictionary cuts it: the opposite end from
/// [`ranked_cuts`], the most segments instead of the fewest, so a word
/// made of words comes apart (早稲田|大学) and a particle the dictionary
/// has stands alone (今日|は). The extreme on purpose, for when the
/// longest match keeps together what is to be fixed apart — and it does
/// take apart whatever the dictionary can tile (プロ|グラ|ミング).
///
/// Two things keep it from going to dust. A word of a single kanji or
/// katakana counts against a covering before the segments are counted, so
/// 東京 does not become 東|京. And a single hiragana is a word only right
/// after something that is not hiragana: は after 今日 is, the あ and る
/// of ある are not.
///
/// Empty when that leaves fewer than two segments.
pub(in crate::core) fn finest_cut(reading: &str, text: &str, words: WordLookup) -> Vec<Piece> {
    cuts_where(reading, text, words, 1, Way::Shortest)
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
    /// reading, and the one wanted hides the fewest of them in a step.
    Script(&'a [bool]),
}

/// For each char of `text`, whether a segment starts there by script
/// alone ([`script_cut`]).
fn script_starts(text: &[char]) -> Vec<bool> {
    // The run each char is in: a script words are written in, or `None`
    // for hiragana and marks. `ー` goes with whatever it prolongs.
    let mut runs: Vec<Option<Script>> = Vec::with_capacity(text.len());
    for &c in text {
        let run = match Script::of(c) {
            Script::ProlongedMark => runs.last().copied().flatten(),
            Script::Hiragana => None,
            Script::Other if !is_kanji(c) => None,
            script => Some(script),
        };
        runs.push(run);
    }
    let mut starts = vec![false; text.len()];
    for i in 1..text.len() {
        if runs[i].is_none() || runs[i] == runs[i - 1] {
            continue;
        }
        let prefixed = is_kanji(text[i]) && matches!(text[i - 1], 'お' | 'ご');
        starts[if prefixed { i - 1 } else { i }] = true;
    }
    // A lone hiragana char at the head joins what follows it.
    if text.len() > 1 && runs[0].is_none() && starts[1] {
        starts[1] = false;
    }
    starts[0] = false;
    starts
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆')
}

/// The best `k` cuts of `text` into two segments or more, for what `way`
/// is after. One search for all three: they differ in where a segment
/// starts and in what a covering is charged for.
fn cuts_where(reading: &str, text: &str, words: WordLookup, k: usize, way: Way) -> Vec<Vec<Piece>> {
    let reading: Vec<char> = reading.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (n, m) = (text.len(), reading.len());
    if n == 0 || m == 0 || n > MAX_CHARS || m > MAX_CHARS {
        return Vec::new();
    }
    let folded_text: Vec<char> = text.iter().map(|&c| fold(c)).collect();
    let folded_reading: Vec<char> = reading.iter().map(|&c| fold(c)).collect();
    let words_at: Vec<Vec<(usize, Vec<char>)>> = (0..m)
        .map(|j| {
            let rest: String = reading[j..].iter().collect();
            words(&rest)
                .into_iter()
                .map(|(taken, surface)| (taken, surface.chars().map(fold).collect()))
                .collect()
        })
        .collect();

    // kept[i][j]: the cheapest distinct cuts of text[i..] with reading[j..].
    let mut kept: Vec<Vec<Vec<Cut>>> = vec![vec![Vec::new(); m + 1]; n + 1];
    kept[n][m].push(Cut {
        cost: (0, 0, 0, 0),
        doubtful: false,
        starts: Vec::new(),
    });
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let mut here = Vec::new();
            let mut take = |i2: usize, j2: usize, kind: Step, outside: usize, doubtful: bool| {
                let (starts, third, fourth) = match way {
                    Way::Longest => {
                        let starts = i > 0 && starts_segment(&text, i, kind);
                        let single = starts && kind == Step::Word && i2 - i == 1;
                        (starts, usize::from(starts), usize::from(single))
                    }
                    Way::Shortest => {
                        let starts = i > 0 && starts_segment(&text, i, kind);
                        let lone = kind == Step::Word
                            && i2 - i == 1
                            && Script::of(text[i]) != Script::Hiragana;
                        // Every char that starts no segment is charged, so
                        // the fewest of them is the most segments.
                        (starts, usize::from(lone), i2 - i - usize::from(starts))
                    }
                    Way::Script(script) => {
                        let hidden = script[i + 1..i2].iter().filter(|&&s| s).count();
                        (script[i], hidden, 0)
                    }
                };
                let step = (outside, usize::from(kind == Step::Unknown), third, fourth);
                for rest in &kept[i2][j2] {
                    offer(&mut here, rest, step, doubtful, starts.then_some((i, j)));
                }
            };
            for (taken, surface) in &words_at[j] {
                let (i2, j2) = (i + surface.len(), j + taken);
                // Cutting finely, a hiragana among hiragana is no word.
                let dust = matches!(way, Way::Shortest)
                    && surface.len() == 1
                    && i > 0
                    && Script::of(text[i]) == Script::Hiragana
                    && Script::of(text[i - 1]) == Script::Hiragana;
                if i2 <= n && j2 <= m && folded_text[i..i2] == surface[..] && !dust {
                    take(i2, j2, Step::Word, 0, false);
                }
            }
            if folded_text[i] == folded_reading[j] {
                // A kana or an alphanumeric no word took is outside the
                // dictionary. Cutting by its longest words that goes for
                // hiragana too, so a word the dictionary has is a word
                // (今日|は) and only what it lacks joins the word before;
                // the other ways leave hiragana free to join.
                let script = Script::of(text[i]);
                let outside = match way {
                    Way::Longest => script != Script::Other,
                    Way::Shortest | Way::Script(_) => script.is_word_char(),
                };
                take(i + 1, j + 1, Step::Pin, usize::from(outside), false);
            }
            for len in 1..=MAX_RUN.min(n - i) {
                if Script::of(text[i + len - 1]).is_kana() {
                    break;
                }
                for taken in 1..=(len * MAX_READING_PER_CHAR).min(m - j) {
                    if kept[i + len][j + taken].is_empty() {
                        continue;
                    }
                    let doubtful =
                        contradicts_dictionary(&folded_text[i..i + len], j, j + taken, &words_at);
                    take(i + len, j + taken, Step::Unknown, len, doubtful);
                }
            }
            kept[i][j] = here;
        }
    }

    // The cheapest covering says how much the dictionary knows of this
    // text; one it contradicts means the text is not the reading's
    // conversion, and then no cut of it can be trusted.
    let cuts = std::mem::take(&mut kept[0][0]);
    let Some(best) = cuts.first().filter(|best| !best.doubtful) else {
        return Vec::new();
    };
    // Cutting finely, single-kanji words are held to the best covering's
    // too: a cut that needs more of them (東|京) is no cut.
    let known = |cut: &Cut| match way {
        Way::Shortest => (cut.cost.0, cut.cost.1, cut.cost.2),
        Way::Longest | Way::Script(_) => (cut.cost.0, cut.cost.1, 0),
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

    /// The first cut, written as `reading=text` pieces joined by `|`; the
    /// whole as one piece when nothing cuts it.
    fn split_with(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        match ranked_cuts(reading, text, &lookup(entries), 1).first() {
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
    fn every_dictionary_word_is_a_segment() {
        // は and に are words the dictionary has, so they are segments.
        assert_eq!(
            split_with("きょうはいしゃにいく", "今日は医者に行く", &SENTENCE),
            "きょう=今日|は=は|いしゃ=医者|に=に|いく=行く"
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
        ranked_cuts(reading, text, &lookup(entries), 3)
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
            ranked_cuts(reading, text, &lookup, 1).pop().unwrap(),
            finest_cut(reading, text, &lookup),
            script_cut(reading, text, &lookup),
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
        // Fewest segments first: the longest word, and に, which the
        // dictionary has, a word of its own.
        assert_eq!(cuts[0], "早稲田大学|に|行く");
        // Then one segment more: the long word gives way.
        assert_eq!(cuts[1], "早稲田|大学|に|行く");
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
        assert_eq!(cuts[0], "これ|は|ここ|に|ある");
        // With no は or に in the dictionary they join the word before.
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
        script_cut(reading, text, &lookup(entries))
            .iter()
            .map(|p| format!("{}={}", p.reading, p.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn the_script_cuts_where_a_run_starts() {
        let entries = [
            ("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            ("きょうぎ", "競技"),
            ("ぷろぐらみんぐ", "プログラミング"),
            ("まなぶ", "学ぶ"),
        ];
        // Where the dictionary has one long word, the script still cuts.
        assert_eq!(
            by_script(
                "きょうぎぷろぐらみんぐをまなぶ",
                "競技プログラミングを学ぶ",
                &entries
            ),
            "きょうぎ=競技|ぷろぐらみんぐを=プログラミングを|まなぶ=学ぶ"
        );
        // Kanji that run on are one run.
        assert_eq!(
            by_script(
                "わせだだいがくにいく",
                "早稲田大学に行く",
                &[("いく", "行く")]
            ),
            "わせだだいがくに=早稲田大学に|いく=行く"
        );
        // ー goes with what it prolongs, and one run is no cut.
        assert_eq!(by_script("らーめん", "らーめん", &[]), "");
    }

    #[test]
    fn the_script_cut_knows_a_prefix_and_a_leading_particle() {
        let entries = [
            ("おちゃ", "お茶"),
            ("のむ", "飲む"),
            ("ごはん", "ご飯"),
            ("たべる", "食べる"),
        ];
        // お and ご before kanji start the segment.
        assert_eq!(
            by_script("きょうはおちゃをのむ", "今日はお茶を飲む", &entries),
            "きょうは=今日は|おちゃを=お茶を|のむ=飲む"
        );
        assert_eq!(
            by_script("おちゃをのむ", "お茶を飲む", &entries),
            "おちゃを=お茶を|のむ=飲む"
        );
        // A lone hiragana char at the head is no segment of its own.
        assert_eq!(
            by_script("をのんでごはんをたべる", "を飲んでご飯を食べる", &entries),
            "をのんで=を飲んで|ごはんを=ご飯を|たべる=食べる"
        );
        // A hiragana word at the head is.
        assert_eq!(
            by_script("これはごはん", "これはご飯", &entries),
            "これは=これは|ごはん=ご飯"
        );
    }

    #[test]
    fn the_script_cut_never_cuts_inside_the_only_word_there() {
        // ご存知 changes script, and nothing but the whole word reads it.
        assert_eq!(
            by_script("ごぞんじ", "ご存知", &[("ごぞんじ", "ご存知")]),
            ""
        );
    }

    /// The finest cut, written like [`split_with`]'s; empty when it does
    /// not cut.
    fn finest(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        finest_cut(reading, text, &lookup(entries))
            .iter()
            .map(|p| format!("{}={}", p.reading, p.text))
            .collect::<Vec<_>>()
            .join("|")
    }

    #[test]
    fn the_finest_cut_takes_words_and_particles_apart() {
        let entries = [
            ("わせだだいがく", "早稲田大学"),
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("に", "に"),
            ("は", "は"),
            ("いく", "行く"),
            ("い", "行"),
            ("く", "く"),
        ];
        assert_eq!(
            finest("わせだだいがくにはいく", "早稲田大学には行く", &entries),
            // に comes off its word; は, a hiragana after a hiragana, does
            // not come off に; 行く is not 行|く.
            "わせだ=早稲田|だいがく=大学|には=には|いく=行く"
        );
    }

    #[test]
    fn the_finest_cut_stops_short_of_dust() {
        let entries = [
            ("とうきょう", "東京"),
            ("とう", "東"),
            ("きょう", "京"),
            ("と", "都"),
            ("とうきょうと", "東京都"),
            ("あ", "あ"),
            ("る", "る"),
            ("ある", "ある"),
            ("に", "に"),
        ];
        // Never down to single kanji while a longer word will do.
        assert_eq!(finest("とうきょう", "東京", &entries), "");
        // Nor hiragana into its letters.
        assert_eq!(
            finest("とうきょうにある", "東京にある", &entries),
            "とうきょう=東京|に=に|ある=ある"
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
