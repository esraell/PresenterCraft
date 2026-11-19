//! Bibles do not all number their verses the same way. The Russian Synodal text counts a
//! psalm's title as a verse, joins and splits a few psalms, and starts some chapters a verse
//! or two earlier or later than English Bibles do; the Hebrew text differs from both. This
//! turns a verse's number in one tradition into its number in another, so two Bibles can be
//! shown side by side.
//!
//! The numbers come from the standard tables in `data/` (see its README): each says how its
//! tradition's verses map to the original-language numbering, so any two can be matched by
//! going through the original.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::VerseRef;
use crate::books::book_by_code;

/// A way of numbering verses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Numbering {
    /// As in the King James Version and most English Bibles.
    English,
    /// As in the Russian Synodal translation (its Protestant edition).
    Synodal,
    /// As in the Hebrew and Greek texts.
    Original,
}

impl Numbering {
    pub fn name(self) -> &'static str {
        match self {
            Numbering::English => "english",
            Numbering::Synodal => "synodal",
            Numbering::Original => "original",
        }
    }

    pub fn from_name(name: &str) -> Option<Numbering> {
        match name {
            "english" => Some(Numbering::English),
            "synodal" => Some(Numbering::Synodal),
            "original" => Some(Numbering::Original),
            _ => None,
        }
    }
}

/// One tradition's verses and the original-numbering verse each one is.
struct Table {
    to_original: HashMap<VerseRef, VerseRef>,
    from_original: HashMap<VerseRef, VerseRef>,
}

/// The tables name books by their standard codes; a few differ from the codes in Bible files.
fn book_of(code: &str) -> Option<usize> {
    let ours = match code {
        "SNG" => "SOL",
        "EZK" => "EZE",
        "JOL" => "JOE",
        "NAM" => "NAH",
        "MRK" => "MAR",
        "JHN" => "JOH",
        "PHP" => "PHI",
        "JAS" => "JAM",
        "1JN" => "1JO",
        "2JN" => "2JO",
        "3JN" => "3JO",
        other => other,
    };
    book_by_code(ours)
}

/// Reads `3:1-8` into chapter 3, verses 1 to 8.
fn span(text: &str) -> Option<(u16, u16, u16)> {
    let (chapter, verses) = text.split_once(':')?;
    let (from, to) = verses.split_once('-').unwrap_or((verses, verses));
    Some((chapter.parse().ok()?, from.parse().ok()?, to.parse().ok()?))
}

/// Reads a table's mapping lines, such as `PSA 51:1-19 = PSA 51:3-21`. Other lines (the lists
/// of chapter lengths, notes, books beyond the sixty-six) are passed over.
fn read(table: &str) -> Table {
    let mut to_original = HashMap::new();
    let mut from_original = HashMap::new();
    for line in table.lines() {
        let line = line.split('#').next().unwrap_or_default();
        let parts: Vec<&str> = line.split_whitespace().collect();
        let [book, here, "=", original_book, there] = parts.as_slice() else { continue };
        let (Some(book), Some(original_book)) = (book_of(book), book_of(original_book)) else { continue };
        let (Some((chapter, from, to)), Some((original_chapter, original_from, _))) = (span(here), span(there)) else {
            continue;
        };
        for step in 0..=to.saturating_sub(from) {
            let ours = VerseRef { book, chapter, verse: from + step };
            let original = VerseRef { book: original_book, chapter: original_chapter, verse: original_from + step };
            // Where a table gives a verse two partners, the first stands.
            to_original.entry(ours).or_insert(original);
            from_original.entry(original).or_insert(ours);
        }
    }
    Table { to_original, from_original }
}

fn table(numbering: Numbering) -> Option<&'static Table> {
    static ENGLISH: OnceLock<Table> = OnceLock::new();
    static SYNODAL: OnceLock<Table> = OnceLock::new();
    match numbering {
        Numbering::English => Some(ENGLISH.get_or_init(|| read(include_str!("../data/eng.vrs")))),
        Numbering::Synodal => Some(SYNODAL.get_or_init(|| read(include_str!("../data/rsc.vrs")))),
        Numbering::Original => None,
    }
}

/// The same verse under another way of numbering. Where the traditions agree the number is
/// unchanged.
pub fn convert(verse: VerseRef, from: Numbering, to: Numbering) -> VerseRef {
    if from == to {
        return verse;
    }
    let original = table(from).and_then(|t| t.to_original.get(&verse)).copied().unwrap_or(verse);
    let there = table(to).and_then(|t| t.from_original.get(&original)).copied().unwrap_or(original);
    // The tables count a psalm's title as verse 0 where a tradition leaves it unnumbered.
    // Bible files keep such a title in verse 1.
    VerseRef { verse: there.verse.max(1), ..there }
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
    fn the_tables_are_read() {
        assert!(table(Numbering::English).unwrap().to_original.len() > 2000);
        assert!(table(Numbering::Synodal).unwrap().to_original.len() > 2000);
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
        assert_eq!(convert(at("PSA", 50, 2), Numbering::Synodal, Numbering::English), at("PSA", 51, 1));
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
    fn hebrew_numbering_is_reached_from_both() {
        // "Have mercy upon me, O God": English 51:1, Synodal 50:3, Hebrew 51:3.
        assert_eq!(convert(at("PSA", 51, 1), Numbering::English, Numbering::Original), at("PSA", 51, 3));
        assert_eq!(convert(at("PSA", 50, 3), Numbering::Synodal, Numbering::Original), at("PSA", 51, 3));
        assert_eq!(convert(at("PSA", 51, 3), Numbering::Original, Numbering::Synodal), at("PSA", 50, 3));
        // Malachi 4 in English is the end of chapter 3 in Hebrew.
        assert_eq!(convert(at("MAL", 4, 1), Numbering::English, Numbering::Original), at("MAL", 3, 19));
    }

    #[test]
    fn the_same_numbering_changes_nothing() {
        let verse = at("PSA", 23, 1);
        assert_eq!(convert(verse, Numbering::English, Numbering::English), verse);
    }
}
