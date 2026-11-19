//! The words of a rich-text (RTF) document, without its formatting. Enough of RTF for song
//! words: paragraphs and line breaks, Unicode characters (`\uN`), characters in a code page
//! (`\'hh`: Windows-1251 when the font or the document says Cyrillic, otherwise Windows-1252),
//! the usual typographic marks, and skipping the parts that hold no words (font and colour
//! tables, pictures, document information).

use crate::import::{windows_1251, windows_1252};

/// Parts of a document that hold no words, skipped with everything inside them.
const NO_WORDS: [&str; 22] = [
    "fonttbl",
    "colortbl",
    "expandedcolortbl",
    "stylesheet",
    "info",
    "pict",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "generator",
    "header",
    "footer",
    "headerl",
    "headerr",
    "footerl",
    "footerr",
    "footnote",
    "xmlnstbl",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "fldinst",
];

/// What holds inside one `{…}` group of a document.
#[derive(Clone, Copy)]
struct Group {
    /// Its words are not shown (a font table, a picture…).
    skip: bool,
    /// How many characters stand in for a Unicode character after `\uN` (`\ucN`).
    stand_ins: usize,
    /// The font chosen with `\fN`, which says which code page `\'hh` is in.
    font: Option<i64>,
    /// Inside the font table, where `\fN` names a font rather than choosing one.
    font_table: bool,
}

/// The `\fcharset` that means Cyrillic (Windows-1251).
const CYRILLIC_CHARSET: i64 = 204;

/// The words of an RTF document, one line for each paragraph or line break, with trailing
/// spaces and empty lines at the end left off. Not RTF at all: the bytes read as text.
pub fn rtf_to_text(bytes: &[u8]) -> String {
    if !bytes.starts_with(b"{\\rtf") {
        return crate::decode_text(bytes);
    }
    let mut out = String::new();
    let mut groups = vec![Group { skip: false, stand_ins: 1, font: None, font_table: false }];
    // The document's own code page is Cyrillic (`\ansicpg1251`).
    let mut cyrillic = false;
    // Each font's character set, from the font table, and the font used when none is chosen.
    // Text in a Cyrillic font is in Windows-1251 even when the document says Windows-1252,
    // as rich text saved on a Mac often is.
    let mut charsets: std::collections::HashMap<i64, i64> = std::collections::HashMap::new();
    let mut naming_font: Option<i64> = None;
    let mut default_font: Option<i64> = None;
    // Characters still to skip after a `\uN`, and the first half of a character written as
    // two UTF-16 halves.
    let mut stand_ins = 0usize;
    let mut high_half: Option<u16> = None;
    let mut at = 0;
    let push = |out: &mut String, c: char, groups: &[Group], stand_ins: &mut usize| {
        if *stand_ins > 0 {
            *stand_ins -= 1;
        } else if !groups.last().is_some_and(|g| g.skip) {
            out.push(c);
        }
    };
    // A byte of text in the code page of the font in use.
    let in_code_page = |byte: u8,
                        groups: &[Group],
                        charsets: &std::collections::HashMap<i64, i64>,
                        default_font: Option<i64>,
                        cyrillic: bool| {
        let font = groups.last().and_then(|g| g.font).or(default_font);
        let cyrillic = match font.and_then(|font| charsets.get(&font)) {
            Some(&charset) => charset == CYRILLIC_CHARSET || (charset == 0 && cyrillic),
            None => cyrillic,
        };
        if cyrillic { windows_1251(byte) } else { windows_1252(byte) }
    };
    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        match byte {
            b'{' => {
                let parent =
                    *groups.last().unwrap_or(&Group { skip: false, stand_ins: 1, font: None, font_table: false });
                // `{\*\…}` is a part a reader may skip when it doesn't know it.
                let starred = bytes[at..].starts_with(b"\\*");
                groups.push(Group { skip: parent.skip || starred, ..parent });
                stand_ins = 0;
            }
            b'}' => {
                if groups.len() > 1 {
                    groups.pop();
                }
                stand_ins = 0;
            }
            b'\\' => {
                let Some(&next) = bytes.get(at) else { break };
                if next.is_ascii_alphabetic() {
                    let start = at;
                    while at < bytes.len() && bytes[at].is_ascii_alphabetic() {
                        at += 1;
                    }
                    let word = std::str::from_utf8(&bytes[start..at]).unwrap_or("");
                    let number_start = at;
                    if at < bytes.len() && (bytes[at] == b'-' || bytes[at].is_ascii_digit()) {
                        at += 1;
                        while at < bytes.len() && bytes[at].is_ascii_digit() {
                            at += 1;
                        }
                    }
                    let number: Option<i64> =
                        std::str::from_utf8(&bytes[number_start..at]).ok().and_then(|n| n.parse().ok());
                    // One space after a control word belongs to it.
                    if bytes.get(at) == Some(&b' ') {
                        at += 1;
                    }
                    if NO_WORDS.contains(&word)
                        && let Some(group) = groups.last_mut()
                    {
                        group.skip = true;
                        group.font_table |= word == "fonttbl";
                    }
                    let in_font_table = groups.last().is_some_and(|g| g.font_table);
                    match word {
                        "ansicpg" => cyrillic = number == Some(1251),
                        "deff" => default_font = number,
                        "f" if in_font_table => naming_font = number,
                        "f" => {
                            if let Some(group) = groups.last_mut() {
                                group.font = number;
                            }
                        }
                        "fcharset" if in_font_table => {
                            if let (Some(font), Some(charset)) = (naming_font, number) {
                                charsets.insert(font, charset);
                            }
                        }
                        "uc" => {
                            if let Some(group) = groups.last_mut() {
                                group.stand_ins = number.unwrap_or(1).clamp(0, 8) as usize;
                            }
                        }
                        "u" => {
                            let unit = number.unwrap_or(0).rem_euclid(65536) as u16;
                            let c = match (high_half.take(), unit) {
                                (Some(high), 0xDC00..=0xDFFF) => {
                                    char::decode_utf16([high, unit]).next().and_then(Result::ok)
                                }
                                (_, 0xD800..=0xDBFF) => {
                                    high_half = Some(unit);
                                    None
                                }
                                (_, unit) => char::from_u32(u32::from(unit)),
                            };
                            stand_ins = 0;
                            if let Some(c) = c {
                                push(&mut out, c, &groups, &mut stand_ins);
                            }
                            stand_ins = groups.last().map_or(1, |g| g.stand_ins);
                        }
                        "par" | "line" | "sect" | "page" | "row" => push(&mut out, '\n', &groups, &mut 0),
                        "tab" | "cell" => push(&mut out, ' ', &groups, &mut 0),
                        "emdash" => push(&mut out, '—', &groups, &mut stand_ins),
                        "endash" => push(&mut out, '–', &groups, &mut stand_ins),
                        "lquote" => push(&mut out, '‘', &groups, &mut stand_ins),
                        "rquote" => push(&mut out, '’', &groups, &mut stand_ins),
                        "ldblquote" => push(&mut out, '“', &groups, &mut stand_ins),
                        "rdblquote" => push(&mut out, '”', &groups, &mut stand_ins),
                        "bullet" => push(&mut out, '•', &groups, &mut stand_ins),
                        _ => {}
                    }
                } else {
                    at += 1;
                    match next {
                        b'\'' => {
                            let hex = bytes.get(at..at + 2).and_then(|h| std::str::from_utf8(h).ok());
                            if let Some(value) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                                at += 2;
                                let c = in_code_page(value, &groups, &charsets, default_font, cyrillic);
                                push(&mut out, c, &groups, &mut stand_ins);
                            }
                        }
                        // A backslash before a line break is a paragraph mark.
                        b'\n' | b'\r' => push(&mut out, '\n', &groups, &mut 0),
                        b'~' => push(&mut out, '\u{a0}', &groups, &mut stand_ins),
                        b'_' => push(&mut out, '‑', &groups, &mut stand_ins),
                        b'\\' | b'{' | b'}' => push(&mut out, char::from(next), &groups, &mut stand_ins),
                        b'*' => {
                            if let Some(group) = groups.last_mut() {
                                group.skip = true;
                            }
                        }
                        // `\-`, an optional hyphen, and anything else: nothing to show.
                        _ => {}
                    }
                }
            }
            // Line breaks in the RTF itself mean nothing.
            b'\n' | b'\r' => {}
            _ if byte.is_ascii() => push(&mut out, char::from(byte), &groups, &mut stand_ins),
            // A byte outside ASCII shouldn't be in RTF, but some writers put one in: read it
            // in the document's code page.
            _ => {
                let c = in_code_page(byte, &groups, &charsets, default_font, cyrillic);
                push(&mut out, c, &groups, &mut stand_ins);
            }
        }
    }
    let lines: Vec<&str> = out.lines().map(str::trim_end).collect();
    let end = lines.iter().rposition(|line| !line.is_empty()).map_or(0, |at| at + 1);
    lines[..end].join("\n").replace('\u{a0}', " ")
}

/// The largest font size an RTF document sets, in half points, if it sets one: which of two
/// boxes holds a slide's main words is told by their size.
pub fn rtf_largest_font(bytes: &[u8]) -> Option<u32> {
    let mut largest = None;
    let mut at = 0;
    while let Some(found) = bytes[at..].windows(3).position(|w| w == b"\\fs") {
        at += found + 3;
        let digits: String = bytes[at..].iter().take_while(|b| b.is_ascii_digit()).map(|&b| char::from(b)).collect();
        if let Ok(size) = digits.parse::<u32>() {
            largest = largest.max(Some(size));
        }
    }
    largest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_line_breaks_without_the_formatting() {
        let rtf = br"{\rtf1\ansi\ansicpg1252\cocoartf2639 {\fonttbl\f0\fswiss\fcharset0 Helvetica;}{\colortbl;\red255\green255\blue255;}{\*\expandedcolortbl;;}\pard\qc\f0\fs120 \cf1 Light along the shore\
Home again\par
Caf\'e9 \ldblquote here\rdblquote }";
        assert_eq!(rtf_to_text(rtf), "Light along the shore\nHome again\nCafé “here”");
    }

    #[test]
    fn unicode_cyrillic_and_the_code_page() {
        // "Свет" as \uN with a stand-in "?" after each, as writers put them.
        assert_eq!(rtf_to_text(br"{\rtf1\ansi\uc1 \u1057?\u1074?\u1077?\u1090?}"), "Свет");
        // "Дом" in the Windows-1251 code page.
        assert_eq!(rtf_to_text(br"{\rtf1\ansi\ansicpg1251 \'c4\'ee\'ec}"), "Дом");
        // A character outside the first plane, in two halves.
        assert_eq!(rtf_to_text(br"{\rtf1 \u-10179?\u-8704?}"), "😀");
    }

    #[test]
    fn text_in_a_cyrillic_font_is_read_in_its_code_page_though_the_document_says_western() {
        // As a Mac saves it: the document in Windows-1252, the Cyrillic font naming charset 204.
        let rtf = br"{\rtf1\ansi\ansicpg1252 {\fonttbl\f0\fswiss\fcharset0 Helvetica;\f1\fnil\fcharset204 PTSans;}\f0 Caf\'e9 \f1 \'c4\'ee\'ec\par {\f0 \'e9}\'c4}";
        assert_eq!(rtf_to_text(rtf), "Café Дом\néД");
    }

    #[test]
    fn the_largest_font_size_is_found() {
        assert_eq!(rtf_largest_font(br"{\rtf1 \fs80 small \fs120 big}"), Some(120));
        assert_eq!(rtf_largest_font(br"{\rtf1 none}"), None);
    }

    #[test]
    fn escaped_braces_and_backslashes_stay_and_text_that_isnt_rtf_reads_as_text() {
        assert_eq!(rtf_to_text(br"{\rtf1 a \{b\} c\\d}"), "a {b} c\\d");
        assert_eq!(rtf_to_text("plain words".as_bytes()), "plain words");
        assert_eq!(rtf_to_text(b"{\\rtf1 unfinished \\"), "unfinished");
    }
}
