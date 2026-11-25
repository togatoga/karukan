//! Where the segment (文節) boundaries are, found in the model's own answer.
//!
//! There is no morphological analysis in this IME — no connection costs,
//! no word costs, no lattice — and the model turns a reading into text
//! without saying which part came from where. What there is, is a
//! dictionary, and two strings that are the same sentence: the kana that
//! were typed and the text they became. Finding the segments is a matching
//! problem between the two.
//!
//! The text is covered, left to right, by three kinds of step, each taking
//! a stretch of the reading with it:
//!
//! - a kana, which is the reading's own character and can only be that one
//!   (は, に and く of 今日は医者に行く);
//! - a dictionary word: its reading at this place in the reading, its
//!   surface at this place in the text. The longest wins;
//! - anything else, taking whatever reading is left for it. With its
//!   neighbours pinned by the two above, that is its own reading: a word
//!   the dictionary lacks is settled by elimination.
//!
//! A segment then starts at a word — a dictionary word, or one settled by
//! elimination — and takes the kana after it, the particles and inflection
//! that belong to it. Katakana words are not looked up (a word written in
//! kana pins itself), so where katakana begins is taken from the script.
//!
//! How finely to cut is a choice, not a fact, so there are three answers
//! ([`Granularity`]) and a key that steps through them.
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

/// Text chars one unpinned run may span, and reading chars it may take per
/// text char: bounds on the search, generous for real words (承る is 5 for
/// 1) and for a loanword spelled out (くろーど for Claude).
const MAX_RUN: usize = 16;
const MAX_READING_PER_CHAR: usize = 6;

/// Dictionary words starting at the head of a reading: for each, how many
/// reading chars it takes and its surface.
pub(in crate::core) type WordLookup<'a> = &'a dyn Fn(&str) -> Vec<(usize, String)>;

/// How finely a sentence is cut into segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(in crate::core) enum Granularity {
    /// The longest dictionary words: 早稲田大学 stays one segment when the
    /// dictionary has it as one word.
    #[default]
    Words,
    /// Shorter dictionary words where a long one is made of them:
    /// 早稲田|大学, 教育|学部.
    Fine,
    /// Where the script changes and nowhere else: a run of kanji is one
    /// segment whatever words it is made of.
    Coarse,
}

impl Granularity {
    /// The next one, round and round.
    pub(in crate::core) fn next(self) -> Self {
        match self {
            Self::Words => Self::Fine,
            Self::Fine => Self::Coarse,
            Self::Coarse => Self::Words,
        }
    }

    /// Its place in the cycle, from 1, for the aux line.
    pub(in crate::core) fn number(self) -> usize {
        match self {
            Self::Words => 1,
            Self::Fine => 2,
            Self::Coarse => 3,
        }
    }

    /// What a dictionary word of `len` text chars costs. Always far less
    /// than an unpinned run, so a known word is taken as one.
    ///
    /// Flat, for the longest match: one long word beats the same stretch
    /// covered by two short ones. For the fine cut the price grows with
    /// the length faster than the sum of its parts, so a word made of two
    /// shorter words gives way to them — but a single character costs more
    /// than its share, or 東京 would come apart into 東 and 京.
    fn word_cost(self, len: usize) -> usize {
        match self {
            Self::Words | Self::Coarse => 2,
            Self::Fine => match len {
                1 => 6,
                2 => 4,
                3 => 7,
                n => n * n - 4,
            },
        }
    }
}

/// What a step of the alignment is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Step {
    /// One char that is the reading's own.
    Pin,
    /// A dictionary word.
    Word,
    /// Unpinned text taking what reading is left: an unknown word.
    Run,
}

/// What a run of unpinned text costs: a flat price, so that fewer runs
/// win; a price per character well above a dictionary word's, so that a
/// known word next to an unknown one is taken as the word it is instead of
/// melting into the run; and how far the run is from two kana per
/// character, the usual length of a kanji's reading, which only ever
/// chooses between alignments the pins allow.
fn run_cost(text_len: usize, reading_len: usize) -> usize {
    10 + 4 * text_len + reading_len.abs_diff(text_len * 2)
}

/// What kind of character a text char is, as far as segments care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Script {
    Hiragana,
    Katakana,
    /// `ー`: written in either kana, and never the start of a word.
    ProlongedMark,
    Kanji,
    /// Latin letters and digits, either width.
    Alphanumeric,
    /// Marks that close a clause: the text after one starts a new segment.
    ClauseMark,
    Other,
}

impl Script {
    fn of(c: char) -> Self {
        match c {
            'ー' => Self::ProlongedMark,
            '\u{3041}'..='\u{309F}' => Self::Hiragana,
            '\u{30A1}'..='\u{30FA}' | '\u{30FD}'..='\u{30FF}' => Self::Katakana,
            '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' => Self::Kanji,
            '、' | '。' | '，' | '．' | '！' | '？' | ',' | '.' | '!' | '?' | '､' | '｡' => {
                Self::ClauseMark
            }
            c if c.is_alphanumeric() => Self::Alphanumeric,
            _ => Self::Other,
        }
    }

    /// Whether the char can only be itself in the reading. Kana must be:
    /// the model writes the reading's kana back as they are. Anything
    /// else that is not kanji (a digit, a mark) may be itself or may be
    /// what kana were converted to (いち → 1), so it pins when it matches
    /// and floats otherwise.
    fn must_pin(self) -> bool {
        matches!(self, Self::Hiragana | Self::Katakana | Self::ProlongedMark)
    }

    /// Whether a word can begin with the char: what follows hiragana as
    /// the next word rather than as more of the same one.
    fn starts_word(self) -> bool {
        matches!(self, Self::Kanji | Self::Katakana | Self::Alphanumeric)
    }
}

fn must_pin(c: char) -> bool {
    Script::of(c).must_pin()
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
) -> Option<Vec<Option<(usize, Step)>>> {
    let (n, m) = (text.len(), reading.len());
    if n == 0 || m == 0 || n > MAX_CHARS || m > MAX_CHARS {
        return None;
    }
    let folded_text: Vec<char> = text.iter().map(|&c| fold(c)).collect();
    let folded_reading: Vec<char> = reading.iter().map(|&c| fold(c)).collect();

    // Dictionary words by where their reading starts, surfaces folded like
    // the text. A word written in kana alone adds nothing: its kana pin
    // themselves.
    let words_at: Vec<Vec<(usize, Vec<char>)>> = (0..m)
        .map(|j| {
            let rest: String = reading[j..].iter().collect();
            words(&rest)
                .into_iter()
                .filter(|(_, surface)| !surface.chars().all(must_pin))
                .map(|(taken, surface)| (taken, surface.chars().map(fold).collect()))
                .collect()
        })
        .collect();

    // best[i][j]: cheapest way to line text[i..] up with reading[j..], and
    // the step taken from there: (next i, next j, what it is).
    const UNREACHABLE: usize = usize::MAX;
    let mut best = vec![vec![UNREACHABLE; m + 1]; n + 1];
    let mut step = vec![vec![(0usize, 0usize, Step::Pin); m + 1]; n + 1];
    best[n][m] = 0;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            // A pin: this char is that reading char.
            if folded_text[i] == folded_reading[j] && best[i + 1][j + 1] != UNREACHABLE {
                best[i][j] = best[i + 1][j + 1];
                step[i][j] = (i + 1, j + 1, Step::Pin);
            }
            // A dictionary word: its surface here, its reading there.
            for (taken, surface) in &words_at[j] {
                let (i2, j2) = (i + surface.len(), j + taken);
                if i2 > n || j2 > m || folded_text[i..i2] != surface[..] {
                    continue;
                }
                let rest = best[i2][j2];
                let cost = granularity.word_cost(surface.len());
                // On a tie the word wins over a kana pinned on its own:
                // ご存知 is the word, not ご followed by 存知.
                if rest != UNREACHABLE && rest + cost <= best[i][j] {
                    best[i][j] = rest + cost;
                    step[i][j] = (i2, j2, Step::Word);
                }
            }
            // A run of unpinned chars taking a stretch of the reading.
            for run in 1..=MAX_RUN.min(n - i) {
                if must_pin(text[i + run - 1]) {
                    break;
                }
                for taken in 1..=(run * MAX_READING_PER_CHAR).min(m - j) {
                    let rest = best[i + run][j + taken];
                    if rest == UNREACHABLE {
                        continue;
                    }
                    let cost = rest + run_cost(run, taken);
                    if cost < best[i][j] {
                        best[i][j] = cost;
                        step[i][j] = (i + run, j + taken, Step::Run);
                    }
                }
            }
        }
    }
    if best[0][0] == UNREACHABLE {
        return None;
    }

    let mut positions = vec![None; n];
    let (mut i, mut j) = (0, 0);
    while i < n {
        let (i2, j2, kind) = step[i][j];
        positions[i] = Some((j, kind));
        if kind == Step::Run && contradicts_dictionary(&folded_text[i..i2], j, j2, &words_at) {
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
/// starts, given what the segment before it holds so far.
fn starts_segment(
    text: &[char],
    i: usize,
    step: Step,
    current: &[char],
    granularity: Granularity,
) -> bool {
    let (prev, next) = (Script::of(text[i - 1]), Script::of(text[i]));
    // 「お茶」「ご飯」 keep their prefix when the dictionary does not have
    // them as words of their own.
    let after_prefix = matches!(current, ['お'] | ['ご']);
    match (prev, next) {
        (_, Script::ClauseMark) => return false,
        (Script::ClauseMark, _) => return true,
        // A word begins where hiragana gives way to something that is
        // not: the particle or inflection before it has ended.
        (Script::Hiragana, next) if next.starts_word() => return !after_prefix,
        // Kanji and katakana side by side are two words of a compound
        // (競技プログラミング, ソフトウェア開発).
        (Script::Kanji, Script::Katakana) | (Script::Katakana, Script::Kanji) => return true,
        _ => {}
    }
    if granularity == Granularity::Coarse {
        return false;
    }
    // The dictionary's answer, where the script has none: a word starts a
    // segment whatever it is written in (大学 after 早稲田; ご存知, which
    // begins in hiragana), and so does the unknown word the words around
    // it leave over. Not after a digit or a letter: 3人 is one thing.
    let after_text = matches!(prev, Script::Hiragana | Script::Katakana | Script::Kanji);
    match step {
        Step::Word => after_text && !after_prefix,
        Step::Run => prev == Script::Kanji,
        Step::Pin => false,
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
    let Some(positions) = align(&reading_chars, &text_chars, words, granularity) else {
        return whole();
    };

    let mut pieces = Vec::new();
    let (mut text_start, mut reading_start) = (0, 0);
    for i in 1..text_chars.len() {
        let Some((reading_at, step)) = positions[i] else {
            continue;
        };
        if !starts_segment(
            &text_chars,
            i,
            step,
            &text_chars[text_start..i],
            granularity,
        ) {
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

    /// The split written as `reading=text` pieces joined by `|`, with no
    /// dictionary.
    fn split(reading: &str, text: &str) -> String {
        split_with(reading, text, &[])
    }

    fn split_with(reading: &str, text: &str, entries: &[(&str, &str)]) -> String {
        split_at(reading, text, entries, Granularity::Words)
    }

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

    #[test]
    fn splits_where_hiragana_gives_way_to_a_word() {
        assert_eq!(
            split("きょうはいしゃにいく", "今日は医者に行く"),
            "きょうは=今日は|いしゃに=医者に|いく=行く"
        );
        assert_eq!(
            split("わたしのなまえはなかのです", "私の名前は中野です"),
            "わたしの=私の|なまえは=名前は|なかのです=中野です"
        );
    }

    #[test]
    fn katakana_and_latin_words_start_segments_too() {
        assert_eq!(
            split("きょうはらーめんをたべる", "今日はラーメンを食べる"),
            "きょうは=今日は|らーめんを=ラーメンを|たべる=食べる"
        );
        assert_eq!(
            split("これはくろーどです", "これはClaudeです"),
            "これは=これは|くろーどです=Claudeです"
        );
    }

    #[test]
    fn a_dictionary_word_pins_what_the_kana_leave_open() {
        // は three times over: 母 could be は or はは as far as the kana
        // go. The dictionary knows which.
        assert_eq!(
            split_with("はははいしゃです", "母は医者です", &[("はは", "母")]),
            "ははは=母は|いしゃです=医者です"
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
    fn an_unknown_word_between_known_ones_is_settled_by_elimination() {
        // 忖度 is not in the dictionary; its neighbours are, so what is
        // left of the reading between them is its own.
        let entries = [("じょうし", "上司"), ("ぶか", "部下")];
        assert_eq!(
            split_with("じょうしそんたくぶか", "上司忖度部下", &entries),
            "じょうし=上司|そんたく=忖度|ぶか=部下"
        );
    }

    #[test]
    fn a_dictionary_word_is_one_unit_whatever_it_is_written_in() {
        // ご存知 begins in hiragana and ends in kanji: the script would
        // cut it in two, the dictionary knows it is one word.
        let entries = [("ごぞんじ", "ご存知"), ("ぞんじ", "存知")];
        assert_eq!(
            split_with("それはごぞんじなどは", "それはご存知などは", &entries),
            "それは=それは|ごぞんじなどは=ご存知などは"
        );
    }

    #[test]
    fn the_three_granularities() {
        let entries = [
            ("わせだ", "早稲田"),
            ("だいがく", "大学"),
            ("わせだだいがく", "早稲田大学"),
            ("とう", "東"),
            ("きょう", "京"),
            ("とうきょう", "東京"),
        ];
        let (reading, text) = ("わせだだいがくはとうきょうにある", "早稲田大学は東京にある");
        // The longest words the dictionary has.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Words),
            "わせだだいがくは=早稲田大学は|とうきょうにある=東京にある"
        );
        // The words those are made of — but never down to single kanji.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Fine),
            "わせだ=早稲田|だいがくは=大学は|とうきょうにある=東京にある"
        );
        // The script alone.
        assert_eq!(
            split_at(reading, text, &entries, Granularity::Coarse),
            "わせだだいがくは=早稲田大学は|とうきょうにある=東京にある"
        );
        let entries = [("わせだ", "早稲田"), ("だいがく", "大学")];
        assert_eq!(
            split_at(
                "わせだだいがく",
                "早稲田大学",
                &entries,
                Granularity::Coarse
            ),
            "わせだだいがく=早稲田大学"
        );
    }

    #[test]
    fn a_digit_keeps_its_counter() {
        let entries = [("にん", "人"), ("いく", "行く")];
        assert_eq!(
            split_with("3にんでいく", "3人で行く", &entries),
            "3にんで=3人で|いく=行く"
        );
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
    fn kanji_and_katakana_side_by_side_are_separate_segments() {
        // Whichever way the same reading is written, it splits at the same
        // place wherever the script says where that is.
        assert_eq!(
            split("きょうぎぷろぐらみんぐ", "競技プログラミング"),
            "きょうぎ=競技|ぷろぐらみんぐ=プログラミング"
        );
        assert_eq!(
            split("きょうぎぷろぐらみんぐ", "きょうぎプログラミング"),
            "きょうぎ=きょうぎ|ぷろぐらみんぐ=プログラミング"
        );
        assert_eq!(
            split("そふとうぇあかいはつ", "ソフトウェア開発"),
            "そふとうぇあ=ソフトウェア|かいはつ=開発"
        );
        // Written in one script there is nothing to go by.
        assert_eq!(
            split("きょうぎぷろぐらみんぐ", "きょうぎぷろぐらみんぐ"),
            "きょうぎぷろぐらみんぐ=きょうぎぷろぐらみんぐ"
        );
    }

    #[test]
    fn a_prolonged_mark_never_starts_a_segment() {
        // Kana typed and left as kana: ー belongs to what it lengthens.
        assert_eq!(
            split("でばっぐもーど", "でばっぐもーど"),
            "でばっぐもーど=でばっぐもーど"
        );
        assert_eq!(split("らーめん", "らーめん"), "らーめん=らーめん");
    }

    #[test]
    fn clause_marks_close_a_segment() {
        assert_eq!(
            split("おい、おまえだよ", "おい、お前だよ"),
            "おい、=おい、|おまえだよ=お前だよ"
        );
        assert_eq!(split("そうです。", "そうです。"), "そうです。=そうです。");
    }

    #[test]
    fn honorific_prefix_stays_with_its_word() {
        assert_eq!(
            split("おちゃをのむ", "お茶を飲む"),
            "おちゃを=お茶を|のむ=飲む"
        );
    }

    #[test]
    fn digits_converted_from_kana_float() {
        assert_eq!(
            split("いちにちにさんかい", "1日に3回"),
            "いちにちに=1日に|さんかい=3回"
        );
    }

    #[test]
    fn width_and_script_differences_still_pin() {
        // The model answers in half-width; settling made it full-width.
        assert_eq!(
            split("3にんでいく", "３人で行く"),
            "3にんで=３人で|いく=行く"
        );
    }

    #[test]
    fn one_segment_when_nothing_separates() {
        assert_eq!(split("とうきょうと", "東京都"), "とうきょうと=東京都");
        assert_eq!(
            split("これはここにある", "これはここにある"),
            "これはここにある=これはここにある"
        );
    }

    #[test]
    fn unalignable_text_stays_whole() {
        // The model rewrote a kana: no alignment, no split.
        assert_eq!(
            split("こんにちわせかい", "こんにちは世界"),
            "こんにちわせかい=こんにちは世界"
        );
        assert_eq!(split("", ""), "=");
    }

    #[test]
    fn pieces_always_concatenate_back() {
        for (reading, text) in [
            ("きょうはいしゃにいく", "今日は医者に行く"),
            ("こんにちわせかい", "こんにちは世界"),
            ("あ", "亜"),
        ] {
            let pieces = split_segments(reading, text, &lookup(&[]), Granularity::Words);
            let r: String = pieces.iter().map(|p| p.reading.as_str()).collect();
            let t: String = pieces.iter().map(|p| p.text.as_str()).collect();
            assert_eq!((r.as_str(), t.as_str()), (reading, text));
        }
    }
}
