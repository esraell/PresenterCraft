//! The words of a rich-text (RTF) document, without its formatting. Enough of RTF for song
//! words: paragraphs and line breaks, Unicode characters (`\uN`), characters in the document's
//! code page (`\'hh`, Windows-1251 or Windows-1252), the usual typographic marks, and skipping
//! the parts that hold no words (font and colour tables, pictures, document information).

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

/// The words of an RTF document, one line for each paragraph or line break, with trailing
/// spaces and empty lines at the end left off. Not RTF at all: the bytes read as text.
pub fn rtf_to_text(bytes: &[u8]) -> String {
    if !bytes.starts_with(b"{\\rtf") {
        return crate::decode_text(bytes);
    }
    let mut out = String::new();
    // For each open group: whether it is skipped, and how many characters stand in for a
    // Unicode character after it (`\ucN`).
    let mut groups: Vec<(bool, usize)> = vec![(false, 1)];
    let mut cyrillic = false;
    // Characters still to skip after a `\uN`, and the first half of a character written as
    // two UTF-16 halves.
    let mut stand_ins = 0usize;
    let mut high_half: Option<u16> = None;
    let mut at = 0;
    let push = |out: &mut String, c: char, groups: &[(bool, usize)], stand_ins: &mut usize| {
        if *stand_ins > 0 {
            *stand_ins -= 1;
        } else if !groups.last().is_some_and(|g| g.0) {
            out.push(c);
        }
    };
    while at < bytes.len() {
        let byte = bytes[at];
        at += 1;
        match byte {
            b'{' => {
                let parent = *groups.last().unwrap_or(&(false, 1));
                // `{\*\…}` is a part a reader may skip when it doesn't know it.
                let starred = bytes[at..].starts_with(b"\\*");
                groups.push((parent.0 || starred, parent.1));
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
                        group.0 = true;
                    }
                    match word {
                        "ansicpg" => cyrillic = number == Some(1251),
                        "uc" => {
                            if let Some(group) = groups.last_mut() {
                                group.1 = number.unwrap_or(1).clamp(0, 8) as usize;
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
                            stand_ins = groups.last().map_or(1, |g| g.1);
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
                                let c = if cyrillic { windows_1251(value) } else { windows_1252(value) };
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
                                group.0 = true;
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
                let c = if cyrillic { windows_1251(byte) } else { windows_1252(byte) };
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
