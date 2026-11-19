//! Bibles do not all number their verses the same way. The Russian Synodal text counts a
//! psalm's title as a verse, joins and splits a few psalms, and starts some chapters a verse
//! or two earlier or later than English Bibles do. This turns a verse's number in one scheme
//! into its number in the other, so two Bibles can be shown side by side.
//!
//! The table covers the Psalms and the places where a chapter starts differently. A few
//! places where one scheme joins two verses into one are not covered and may be one verse off
//! at the very end of a chapter: Leviticus 14, Joshua 24, 1 Samuel 20, Proverbs 4 and 13,
//! Isaiah 3, Acts 19, 2 Corinthians 11 and 13, 3 John.

use std::sync::OnceLock;

use crate::VerseRef;
use crate::books::book_by_code;

/// A way of numbering verses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numbering {
    /// As in the King James Version and most English Bibles.
    English,
    /// As in the Russian Synodal translation.
    Synodal,
    /// Its own way, which the table does not know. Such a Bible is fine on its own; beside
    /// another, some verses (the Psalms above all) may not be the matching ones.
    Own,
}

impl Numbering {
    pub fn name(self) -> &'static str {
        match self {
            Numbering::English => "english",
            Numbering::Synodal => "synodal",
            Numbering::Own => "own",
        }
    }

    pub fn from_name(name: &str) -> Option<Numbering> {
        match name {
            "english" => Some(Numbering::English),
            "synodal" => Some(Numbering::Synodal),
            "own" => Some(Numbering::Own),
            _ => None,
        }
    }
}

/// A run of verses in one English chapter and where the run starts in the Synodal text.
struct Rule {
    book: usize,
    english_chapter: u16,
    english_from: u16,
    english_to: u16,
    synodal_chapter: u16,
    synodal_from: u16,
}

/// The end of a chapter, whatever its length.
const END: u16 = u16::MAX;

/// Synodal psalms whose title takes up verse 1.
const TITLE_ONE: [u16; 56] = [
    3, 4, 5, 6, 7, 8, 11, 17, 18, 19, 20, 21, 29, 30, 33, 35, 37, 38, 39, 40, 41, 43, 44, 45, 46, 47, 48, 52, 54, 55,
    56, 57, 58, 60, 61, 62, 63, 64, 66, 67, 68, 69, 74, 75, 76, 79, 80, 82, 83, 84, 87, 88, 91, 101, 107, 139,
];
/// Synodal psalms whose title takes up verses 1 and 2.
const TITLE_TWO: [u16; 4] = [50, 51, 53, 59];

fn rules() -> &'static [Rule] {
    static RULES: OnceLock<Vec<Rule>> = OnceLock::new();
    RULES.get_or_init(|| {
        let mut rules = Vec::new();
        let mut add = |code: &str, ec: u16, ef: u16, et: u16, sc: u16, sf: u16| {
            if let Some(book) = book_by_code(code) {
                rules.push(Rule {
                    book,
                    english_chapter: ec,
                    english_from: ef,
                    english_to: et,
                    synodal_chapter: sc,
                    synodal_from: sf,
                });
            }
        };
        // Psalms. English 9 and 10 are Synodal 9; 114 and 115 are 113; 116 is 114 and 115;
        // 147 is 146 and 147. Between those the Synodal number is one less.
        for english in 1..=150_u16 {
            let synodal = match english {
                1..=8 | 148..=150 => english,
                9 | 10 | 114..=116 | 147 => continue,
                _ => english - 1,
            };
            let title = if TITLE_TWO.contains(&synodal) {
                2
            } else if TITLE_ONE.contains(&synodal) {
                1
            } else {
                0
            };
            if synodal != english || title != 0 {
                add("PSA", english, 1, END, synodal, 1 + title);
            }
        }
        add("PSA", 9, 1, END, 9, 2);
        add("PSA", 10, 1, END, 9, 22);
        add("PSA", 114, 1, END, 113, 1);
        add("PSA", 115, 1, END, 113, 9);
        add("PSA", 116, 1, 9, 114, 1);
        add("PSA", 116, 10, END, 115, 1);
        add("PSA", 147, 1, 11, 146, 1);
        add("PSA", 147, 12, END, 147, 1);
        // Chapters that start at a different verse.
        add("NUM", 12, 16, 16, 13, 1);
        add("NUM", 13, 1, END, 13, 2);
        add("NUM", 29, 40, 40, 30, 1);
        add("NUM", 30, 1, END, 30, 2);
        add("JOS", 6, 1, 1, 5, 16);
        add("JOS", 6, 2, END, 6, 1);
        add("1SA", 23, 29, 29, 24, 1);
        add("1SA", 24, 1, END, 24, 2);
        add("JOB", 40, 1, 5, 39, 31);
        add("JOB", 40, 6, END, 40, 1);
        add("JOB", 41, 1, 8, 40, 20);
        add("JOB", 41, 9, END, 41, 1);
        add("ECC", 5, 1, 1, 4, 17);
        add("ECC", 5, 2, END, 5, 1);
        add("SOL", 1, 2, END, 1, 1);
        add("SOL", 6, 13, 13, 7, 1);
        add("SOL", 7, 1, END, 7, 2);
        add("DAN", 4, 1, 3, 3, 31);
        add("DAN", 4, 4, END, 4, 1);
        add("HOS", 13, 16, 16, 14, 1);
        add("HOS", 14, 1, END, 14, 2);
        add("JON", 1, 17, 17, 2, 1);
        add("JON", 2, 1, END, 2, 2);
        // The closing praise of Romans stands after chapter 14 in the Synodal text.
        add("ROM", 16, 25, 27, 14, 24);
        rules
    })
}

/// The same verse under another way of numbering. Where the schemes agree, or the table does
/// not cover the place, the number is unchanged.
pub fn convert(verse: VerseRef, from: Numbering, to: Numbering) -> VerseRef {
    match (from, to) {
        (Numbering::English, Numbering::Synodal) => rules()
            .iter()
            .find(|r| {
                r.book == verse.book
                    && r.english_chapter == verse.chapter
                    && (r.english_from..=r.english_to).contains(&verse.verse)
            })
            .map_or(verse, |r| VerseRef {
                book: verse.book,
                chapter: r.synodal_chapter,
                verse: r.synodal_from + (verse.verse - r.english_from),
            }),
        (Numbering::Synodal, Numbering::English) => {
            // The rule that starts latest at or before the verse is the one it belongs to.
            let rule = rules()
                .iter()
                .filter(|r| {
                    r.book == verse.book
                        && r.synodal_chapter == verse.chapter
                        && r.synodal_from <= verse.verse
                        && (r.english_to == END || verse.verse - r.synodal_from <= r.english_to - r.english_from)
                })
                .max_by_key(|r| r.synodal_from);
            match rule {
                Some(r) => VerseRef {
                    book: verse.book,
                    chapter: r.english_chapter,
                    verse: r.english_from + (verse.verse - r.synodal_from),
                },
                // A title verse has no number of its own in English: it is part of verse 1.
                None => match rules().iter().find(|r| r.book == verse.book && r.synodal_chapter == verse.chapter) {
                    Some(r) if verse.verse < r.synodal_from => {
                        VerseRef { book: verse.book, chapter: r.english_chapter, verse: r.english_from }
                    }
                    _ => verse,
                },
            }
        }
        _ => verse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(code: &str, chapter: u16, verse: u16) -> VerseRef {
        VerseRef { book: book_by_code(code).unwrap(), chapter, verse }
    }

    fn both_ways(code: &str, english: (u16, u16), synodal: (u16, u16)) {
        let (e, s) = (at(code, english.0, english.1), at(code, synodal.0, synodal.1));
        assert_eq!(convert(e, Numbering::English, Numbering::Synodal), s, "{code} {english:?} to Synodal");
        assert_eq!(convert(s, Numbering::Synodal, Numbering::English), e, "{code} {synodal:?} to English");
    }

    #[test]
    fn psalms_line_up() {
        both_ways("PSA", (1, 1), (1, 1));
        both_ways("PSA", (3, 1), (3, 2));
        both_ways("PSA", (9, 1), (9, 2));
        both_ways("PSA", (10, 1), (9, 22));
        both_ways("PSA", (23, 1), (22, 1));
        both_ways("PSA", (51, 1), (50, 3));
        both_ways("PSA", (115, 1), (113, 9));
        both_ways("PSA", (116, 10), (115, 1));
        both_ways("PSA", (119, 105), (118, 105));
        both_ways("PSA", (147, 12), (147, 1));
        both_ways("PSA", (150, 6), (150, 6));
    }

    #[test]
    fn a_title_verse_belongs_to_the_first_english_verse() {
        assert_eq!(convert(at("PSA", 50, 1), Numbering::Synodal, Numbering::English), at("PSA", 51, 1));
        assert_eq!(convert(at("PSA", 3, 1), Numbering::Synodal, Numbering::English), at("PSA", 3, 1));
    }

    #[test]
    fn chapters_that_start_differently_line_up() {
        both_ways("JON", (1, 17), (2, 1));
        both_ways("JON", (2, 1), (2, 2));
        both_ways("DAN", (4, 1), (3, 31));
        both_ways("DAN", (4, 37), (4, 34));
        both_ways("JOB", (41, 1), (40, 20));
        both_ways("ROM", (16, 25), (14, 24));
        both_ways("JOH", (3, 16), (3, 16));
    }

    #[test]
    fn the_same_numbering_changes_nothing() {
        let verse = at("PSA", 23, 1);
        assert_eq!(convert(verse, Numbering::English, Numbering::English), verse);
    }
}
