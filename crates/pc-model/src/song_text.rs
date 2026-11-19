//! The plain-text song format: `[Group]` headers, blank lines between slides, and a `--` line
//! between the main language and its translation.

use uuid::Uuid;

use crate::types::{Group, Presentation, Rgba, Role, Slide, TextBox};

/// Parses a song written in the plain-text format.
///
/// - A line in square brackets starts a group with that name.
/// - Blank lines separate slides.
/// - Inside a slide, the first line containing only `--` separates the main language from
///   its translation.
/// - Lines before the first group header belong to an unnamed group.
pub fn parse_song_text(title: &str, text: &str) -> Presentation {
    let mut groups: Vec<Group> = Vec::new();
    let mut current = new_group("");
    let mut draft = SlideDraft::default();

    for raw in text.lines() {
        let line = raw.trim_end();
        let trimmed = line.trim();
        if let Some(name) = group_header(trimmed) {
            draft.finish_into(&mut current);
            push_group(&mut groups, current);
            current = new_group(name);
        } else if trimmed.is_empty() {
            draft.finish_into(&mut current);
        } else if trimmed == "--" && !draft.in_translation {
            draft.in_translation = true;
        } else if draft.in_translation {
            draft.translation.push(line.to_owned());
        } else {
            draft.primary.push(line.to_owned());
        }
    }
    draft.finish_into(&mut current);
    push_group(&mut groups, current);

    Presentation { id: Uuid::new_v4(), title: title.to_owned(), groups }
}

/// The colour bar shown for a group, chosen from its name so that every "Chorus" matches.
pub fn group_color(name: &str) -> Rgba {
    const PALETTE: [Rgba; 8] = [
        Rgba::rgb(66, 133, 244),
        Rgba::rgb(219, 68, 85),
        Rgba::rgb(244, 160, 0),
        Rgba::rgb(52, 168, 110),
        Rgba::rgb(155, 89, 182),
        Rgba::rgb(0, 172, 193),
        Rgba::rgb(240, 98, 146),
        Rgba::rgb(141, 110, 99),
    ];
    const KNOWN: [(&str, usize); 8] =
        [("verse", 0), ("chorus", 1), ("bridge", 2), ("pre", 3), ("intro", 4), ("ending", 5), ("outro", 5), ("tag", 6)];
    let lower = name.trim().to_lowercase();
    if lower.is_empty() {
        return Rgba::rgb(128, 128, 128);
    }
    if let Some((_, index)) = KNOWN.iter().find(|(prefix, _)| lower.starts_with(prefix)) {
        return PALETTE[*index];
    }
    // FNV-1a: stable across runs and platforms, unlike the standard hasher.
    let hash = lower.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3));
    PALETTE[(hash % PALETTE.len() as u64) as usize]
}

#[derive(Default)]
struct SlideDraft {
    primary: Vec<String>,
    translation: Vec<String>,
    in_translation: bool,
}

impl SlideDraft {
    /// Ends the slide being collected and adds it to the group if it has any text.
    fn finish_into(&mut self, group: &mut Group) {
        let draft = std::mem::take(self);
        let mut boxes = Vec::new();
        if !draft.primary.is_empty() {
            boxes.push(TextBox { role: Role::primary(), text: draft.primary.join("\n") });
        }
        if !draft.translation.is_empty() {
            boxes.push(TextBox { role: Role::translation(), text: draft.translation.join("\n") });
        }
        if !boxes.is_empty() {
            group.slides.push(Slide::new(boxes));
        }
    }
}

fn group_header(trimmed: &str) -> Option<&str> {
    let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?;
    Some(inner.trim())
}

fn new_group(name: &str) -> Group {
    Group { id: Uuid::new_v4(), name: name.to_owned(), color: group_color(name), slides: Vec::new() }
}

fn push_group(groups: &mut Vec<Group>, group: Group) {
    if !group.slides.is_empty() {
        groups.push(group);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SONG: &str =
        "[Verse 1]\nLine a\nLine b\n--\nСтрока а\nСтрока б\n\nSecond slide\n\n\n[Chorus]\nSing\n--\nПой\n";

    #[test]
    fn groups_and_slides() {
        let p = parse_song_text("T", SONG);
        assert_eq!(p.title, "T");
        assert_eq!(
            p.groups.iter().map(|g| (g.name.as_str(), g.slides.len())).collect::<Vec<_>>(),
            vec![("Verse 1", 2), ("Chorus", 1)]
        );
        assert_eq!(p.slide_count(), 3);
    }

    #[test]
    fn box_split() {
        let p = parse_song_text("T", SONG);
        let (_, s) = p.slide(0).unwrap();
        assert_eq!(s.text_for(&Role::primary()), Some("Line a\nLine b"));
        assert_eq!(s.text_for(&Role::translation()), Some("Строка а\nСтрока б"));
        let (_, s) = p.slide(1).unwrap();
        assert_eq!(s.boxes.len(), 1);
    }

    #[test]
    fn text_before_any_header_goes_to_unnamed_group() {
        let p = parse_song_text("T", "Alone\n\n[Verse]\nX\n");
        assert_eq!(p.groups[0].name, "");
        assert_eq!(p.groups[0].slides.len(), 1);
    }

    #[test]
    fn windows_line_endings_and_trailing_blank_lines() {
        let p = parse_song_text("T", "[V]\r\nA\r\n--\r\nБ\r\n\r\n\r\n");
        assert_eq!(p.slide_count(), 1);
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Б"));
    }

    #[test]
    fn empty_file_has_no_slides() {
        assert_eq!(parse_song_text("T", "").slide_count(), 0);
    }

    #[test]
    fn empty_side_of_split_makes_no_empty_box() {
        let p = parse_song_text("T", "A\n--\n\nB\n--\n");
        assert_eq!(p.slide(0).unwrap().1.boxes.len(), 1);
        assert_eq!(p.slide(1).unwrap().1.boxes.len(), 1);
        let p = parse_song_text("T", "--\nТолько перевод\n");
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Только перевод"));
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::primary()), None);
    }

    #[test]
    fn empty_groups_are_dropped() {
        assert_eq!(parse_song_text("T", "[A]\n[B]\nX\n").groups.len(), 1);
    }

    #[test]
    fn same_group_name_gets_same_color() {
        assert_eq!(group_color("Chorus"), group_color("chorus 2"));
    }

    #[test]
    fn a_second_separator_is_text() {
        let p = parse_song_text("T", "A\n--\nB\n--\nC\n");
        assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("B\n--\nC"));
    }

    #[test]
    fn flat_slide_index_crosses_groups() {
        let p = parse_song_text("T", SONG);
        assert_eq!(p.slide(2).unwrap().0.name, "Chorus");
        assert!(p.slide(3).is_none());
    }
}
