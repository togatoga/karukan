//! Where the segment (文節) boundaries are, found in the model's own answer.
//!
//! There is no morphological analysis in this IME: the model turns a
//! reading into text and says nothing about which part came from where. But
//! the text carries the answer. Its kana are the reading's own characters,
//! so they pin the text to the reading (今日は医者に行く against
//! きょうはいしゃにいく: は, に and く can only be those), and what lies
//! between two pins is the reading of the kanji between them. And where
//! hiragana gives way to kanji or katakana is where a particle or an
//! inflection ended and the next word began, which is what a segment is.
//!
//! The dictionary sharpens the first half. A dictionary word found at the
//! same place in both — its reading in the reading, its surface in the text
//! — is a pin as good as a kana, and the longest one wins. A word the
//! dictionary lacks is then settled by elimination: with its neighbours
//! pinned, the reading left between them is its own. The dictionary has no
//! parts of speech, so it does not decide where a segment starts.
//!
//! Pure functions of the two strings and a word lookup, engine-free. A
//! split that comes out wrong costs the user a Shift+arrow, and a resized
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

/// What a dictionary word costs: next to nothing, but not nothing, so that
/// one long word beats the same stretch covered by two short ones.
const WORD_COST: usize = 2;

/// What a run of unpinned text costs: a flat price, so that fewer runs
/// win; a price per character well above a dictionary word's, so that a
/// known word next to an unknown one is taken as the word it is instead of
/// melting into the run; and how far the run is from two kana per
/// character, the usual length of a kanji's reading, which only ever
/// chooses between alignments the pins allow.
fn run_cost(text_len: usize, reading_len: usize) -> usize {
    10 + 4 * text_len + reading_len.abs_diff(text_len * 2)
}

fn is_hiragana(c: char) -> bool {
    matches!(c, '\u{3041}'..='\u{309F}')
}

fn is_katakana(c: char) -> bool {
    matches!(c, '\u{30A1}'..='\u{30FA}' | '\u{30FC}'..='\u{30FF}')
}

fn is_kanji(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}' | '々' | '〆' | 'ヶ')
}

/// Marks that close a clause: the text after one starts a new segment.
fn is_clause_mark(c: char) -> bool {
    matches!(
        c,
        '、' | '。' | '，' | '．' | '！' | '？' | ',' | '.' | '!' | '?' | '､' | '｡'
    )
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

/// Whether a text char can only be itself in the reading. Kana must be:
/// the model writes the reading's kana back as they are. Anything else that
/// is not kanji (a digit, a mark) may be itself or may be what kana were
/// converted to (いち → 1), so it pins when it matches and floats otherwise.
fn must_pin(c: char) -> bool {
    is_hiragana(c) || is_katakana(c)
}

/// For every text position, the reading position it lines up with — `None`
/// inside an unpinned run, where no single position exists. `None` overall
/// when the text cannot be the reading's conversion at all (the model
/// rewrote a kana), or when either is too long to align.
fn align(reading: &[char], text: &[char], words: WordLookup) -> Option<Vec<Option<usize>>> {
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
    // the step taken from there: (next i, next j).
    const UNREACHABLE: usize = usize::MAX;
    let mut best = vec![vec![UNREACHABLE; m + 1]; n + 1];
    let mut step = vec![vec![(0usize, 0usize); m + 1]; n + 1];
    best[n][m] = 0;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            // A pin: this char is that reading char.
            if folded_text[i] == folded_reading[j] && best[i + 1][j + 1] != UNREACHABLE {
                best[i][j] = best[i + 1][j + 1];
                step[i][j] = (i + 1, j + 1);
            }
            // A dictionary word: its surface here, its reading there.
            for (taken, surface) in &words_at[j] {
                let (i2, j2) = (i + surface.len(), j + taken);
                if i2 > n || j2 > m || folded_text[i..i2] != surface[..] {
                    continue;
                }
                let rest = best[i2][j2];
                if rest != UNREACHABLE && rest + WORD_COST < best[i][j] {
                    best[i][j] = rest + WORD_COST;
                    step[i][j] = (i2, j2);
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
                        step[i][j] = (i + run, j + taken);
                    }
                }
            }
        }
    }
    if best[0][0] == UNREACHABLE {
        return None;
    }

    let mut positions = vec![None; n + 1];
    let (mut i, mut j) = (0, 0);
    while i < n {
        positions[i] = Some(j);
        let (i2, j2) = step[i][j];
        if contradicts_dictionary(&folded_text[i..i2], j, j2, &words_at) {
            return None;
        }
        (i, j) = (i2, j2);
    }
    positions[n] = Some(m);
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

/// Whether a segment starts at `text[i]`, given what the segment before it
/// holds so far.
fn starts_segment(text: &[char], i: usize, current: &[char]) -> bool {
    let (prev, c) = (text[i - 1], text[i]);
    if is_clause_mark(c) {
        return false;
    }
    if is_clause_mark(prev) {
        return true;
    }
    // A word begins where hiragana gives way to something that is not: the
    // particle or inflection before it has ended. 「お茶」「ご飯」 keep their
    // prefix.
    let starts_word = is_kanji(c) || is_katakana(c) || c.is_alphanumeric() && !is_hiragana(c);
    starts_word && is_hiragana(prev) && !matches!(current, ['お'] | ['ご'])
}

/// Split `text`, the conversion of `reading`, into segments. Always at
/// least one piece, and the pieces always concatenate back to both strings:
/// when the two cannot be lined up, the whole is a single segment.
pub(in crate::core) fn split_segments(reading: &str, text: &str, words: WordLookup) -> Vec<Piece> {
    let whole = || {
        vec![Piece {
            reading: reading.to_string(),
            text: text.to_string(),
        }]
    };
    let reading_chars: Vec<char> = reading.chars().collect();
    let text_chars: Vec<char> = text.chars().collect();
    let Some(positions) = align(&reading_chars, &text_chars, words) else {
        return whole();
    };

    let mut pieces = Vec::new();
    let (mut text_start, mut reading_start) = (0, 0);
    for i in 1..text_chars.len() {
        let Some(reading_at) = positions[i] else {
            continue;
        };
        if !starts_segment(&text_chars, i, &text_chars[text_start..i]) {
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
        split_segments(reading, text, &lookup(entries))
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
    fn an_unknown_word_between_known_ones_is_settled_by_elimination() {
        // 忖度 is not in the dictionary; its neighbours are, so what is
        // left of the reading between them is its own.
        let entries = [("じょうし", "上司"), ("ぶか", "部下")];
        assert_eq!(
            split_with("じょうしそんたくぶか", "上司忖度部下", &entries),
            "じょうしそんたくぶか=上司忖度部下"
        );
        let positions = align(
            &"じょうしそんたくぶか".chars().collect::<Vec<_>>(),
            &"上司忖度部下".chars().collect::<Vec<_>>(),
            &lookup(&entries),
        )
        .unwrap();
        // 上司 | 忖度 | 部下 at reading 0, 4, 8.
        assert_eq!(positions[0], Some(0));
        assert_eq!(positions[2], Some(4));
        assert_eq!(positions[4], Some(8));
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
            let pieces = split_segments(reading, text, &lookup(&[]));
            let r: String = pieces.iter().map(|p| p.reading.as_str()).collect();
            let t: String = pieces.iter().map(|p| p.text.as_str()).collect();
            assert_eq!((r.as_str(), t.as_str()), (reading, text));
        }
    }
}
