//! The Bible page's working parts: the Bibles installed, turning a typed reference into
//! slides, and the Options pop-up that says how.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};

use std::sync::Arc;

use sp_bible::{Bible, VerseRef, convert, parse_reference_in};
use sp_model::{Group, Object, Presentation, Rgba, Role, Slide, Uuid};

use crate::home::BibleTarget;

/// Where a Bible's file is.
enum Kept {
    /// Inside the app.
    Shipped(&'static [u8]),
    /// In the Bibles folder.
    File(std::path::PathBuf),
}

/// A Bible the app has, and its text once it has been needed.
pub struct Shelved {
    pub id: String,
    pub name: String,
    pub short: String,
    /// Its language, as its file says it (`en`, `ru`, `uk`…).
    pub language: String,
    pub terms: String,
    pub credit: String,
    /// True when it only mostly follows that numbering.
    pub loose: bool,
    pub shipped: bool,
    kept: Kept,
    /// Read the first time it is asked for, so an unused Bible costs no memory.
    loaded: Option<Result<Arc<Bible>, String>>,
}

/// The Bibles installed: the ones that come with the app, then the ones added since.
pub struct Shelf {
    pub bibles: Vec<Shelved>,
}

impl std::fmt::Debug for Shelf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.bibles.iter().map(|b| b.id.as_str())).finish()
    }
}

impl Shelf {
    /// The Bibles that come with the app. All are in the public domain; see
    /// `docs/licences/bibles.md`.
    pub fn shipped() -> Shelf {
        let files: [&'static [u8]; 4] = [
            include_bytes!("../../../assets/bibles/kjv.spbible"),
            include_bytes!("../../../assets/bibles/web.spbible"),
            include_bytes!("../../../assets/bibles/bsb.spbible"),
            include_bytes!("../../../assets/bibles/syn.spbible"),
        ];
        let mut shelf = Shelf { bibles: Vec::new() };
        for bytes in files {
            if let Ok(info) = Bible::read_info(bytes) {
                shelf.put(info, Kept::Shipped(bytes), true);
            }
        }
        shelf
    }

    /// The Bibles that come with the app and the ones in the Bibles folder.
    pub fn with_installed() -> Shelf {
        let mut shelf = Shelf::shipped();
        for (path, info) in crate::bible_store::installed() {
            shelf.add_file(path, info);
        }
        shelf
    }

    fn put(&mut self, info: sp_bible::Info, kept: Kept, shipped: bool) {
        self.bibles.push(Shelved {
            id: info.id,
            name: info.name,
            short: info.short,
            language: info.language,
            terms: info.terms,
            credit: info.credit,
            loose: info.loose,
            shipped,
            kept,
            loaded: None,
        });
    }

    /// Puts a Bible from the Bibles folder on the shelf. One already there under the same
    /// short name stays, at its place, so the translations chosen do not shift.
    pub fn add_file(&mut self, path: std::path::PathBuf, info: sp_bible::Info) {
        if self.position(&info.id).is_none() {
            self.put(info, Kept::File(path), false);
        }
    }

    /// Takes a downloaded Bible off the shelf and moves its file to the Trash. The Bibles that
    /// come with the app cannot be removed.
    pub fn remove(&mut self, index: usize) -> Result<(), String> {
        let Some(shelved) = self.bibles.get(index) else { return Err("There is no such Bible.".to_owned()) };
        match &shelved.kept {
            Kept::Shipped(_) => return Err(format!("The {} comes with the app and stays.", shelved.name)),
            Kept::File(path) => {
                if path.exists() {
                    trash::delete(path)
                        .map_err(|e| format!("Could not move the {} to the Trash: {e}", shelved.name))?;
                }
            }
        }
        self.bibles.remove(index);
        Ok(())
    }

    pub fn position(&self, id: &str) -> Option<usize> {
        self.bibles.iter().position(|b| b.id == id)
    }

    /// The Bible at a place on the shelf, reading it the first time.
    pub fn get(&mut self, index: usize) -> Result<Arc<Bible>, String> {
        let shelved = self.bibles.get_mut(index).ok_or_else(|| "Choose a Bible first.".to_owned())?;
        shelved
            .loaded
            .get_or_insert_with(|| match &shelved.kept {
                Kept::Shipped(bytes) => Bible::from_bytes(bytes).map(Arc::new),
                Kept::File(path) => std::fs::read(path)
                    .map_err(|e| format!("Could not read the {}: {e}", shelved.name))
                    .and_then(|bytes| Bible::from_bytes(&bytes).map(Arc::new)),
            })
            .clone()
    }
}

/// One translation on the Bible page: its text and where its words and reference go.
pub struct RowChoice {
    pub bible: Arc<Bible>,
    pub verse: BibleTarget,
    pub reference: BibleTarget,
}

/// A digit as its small raised form.
fn raised(digit: char) -> char {
    match digit {
        '0' => '⁰',
        '1' => '¹',
        '2' => '²',
        '3' => '³',
        '4' => '⁴',
        '5' => '⁵',
        '6' => '⁶',
        '7' => '⁷',
        '8' => '⁸',
        '9' => '⁹',
        other => other,
    }
}

/// Turns what was typed in Scripture Lookup into slides. The reference is read in the
/// numbering of the first translation; the others are lined up with it. Each translation's
/// words, and its reference, go into the theme box chosen for them; a box given both holds
/// the words with the reference under them. Every slide is drawn with `template`, the Bible
/// theme's template chosen for Bible slides. The error is a plain sentence.
pub fn build_passage(
    typed: &str,
    rows: &[RowChoice],
    options: &BibleOptions,
    template: Option<Uuid>,
) -> Result<Presentation, String> {
    let lead = rows.first().ok_or_else(|| "Choose a Bible first.".to_owned())?;
    // A book's short form is read in the first translation's language first: "1 Цар" is
    // 1 Samuel to a Russian Bible and 1 Kings to a Ukrainian one.
    let mut range = parse_reference_in(typed, &lead.bible.info.language)?;
    // A chapter past the end of the book is read as its last chapter: "John 99" is John 21.
    let chapters = lead.bible.chapter_count(range.start.book);
    if chapters > 0 && range.start.chapter > chapters {
        range.start = VerseRef { chapter: chapters, verse: 1, ..range.start };
        range.end = VerseRef { chapter: chapters, verse: sp_bible::LAST, ..range.start };
    }
    let places: Vec<VerseRef> = lead.bible.verses(range).into_iter().map(|(place, _)| place).collect();
    let (Some(first), Some(last)) = (places.first().copied(), places.last().copied()) else {
        return Err(format!("The {} has no such verses.", lead.bible.info.name));
    };
    let in_row = |row: &RowChoice, place: VerseRef| convert(place, lead.bible.info.numbering, row.bible.info.numbering);
    let label = |row: &RowChoice, from: VerseRef, to: VerseRef| {
        let text = row.bible.label(in_row(row, from), in_row(row, to));
        if options.display_translation { format!("{text} ({})", row.bible.info.short) } else { text }
    };

    // One slide for each verse, or one for the whole passage.
    let chunks: Vec<&[VerseRef]> =
        if options.break_on_new_verse { places.chunks(1).collect() } else { vec![places.as_slice()] };
    let count = chunks.len();
    let mut groups = Vec::with_capacity(count);
    for (index, chunk) in chunks.into_iter().enumerate() {
        let (Some(from), Some(to)) = (chunk.first().copied(), chunk.last().copied()) else { continue };
        let words_of = |row: &RowChoice| -> String {
            let mut text = String::new();
            for place in chunk {
                let there = in_row(row, *place);
                let Some(words) = row.bible.verse(there) else { continue };
                if !text.is_empty() {
                    text.push(' ');
                }
                if options.show_verse_numbers {
                    // Small raised digits, as a printed Bible has them.
                    text.extend(there.verse.to_string().chars().map(raised));
                    text.push('\u{2009}');
                }
                text.push_str(words);
            }
            text
        };
        let reference_shown = match options.reference {
            ReferenceStyle::NoReference => false,
            ReferenceStyle::PassageLast => index + 1 == count,
            ReferenceStyle::VerseReferences | ReferenceStyle::PassageEach => true,
        };
        let reference_of = |row: &RowChoice| match options.reference {
            ReferenceStyle::VerseReferences => label(row, from, to),
            _ => label(row, first, last),
        };
        // What goes in each box, in the order the boxes are first named: every translation's
        // words first, then the references.
        let mut boxes: Vec<(Role, Vec<String>, Vec<String>)> = Vec::new();
        let mut put = |role: &Role, words: Option<String>, reference: Option<String>| {
            let at = boxes.iter().position(|(have, _, _)| have == role).unwrap_or_else(|| {
                boxes.push((role.clone(), Vec::new(), Vec::new()));
                boxes.len() - 1
            });
            boxes[at].1.extend(words);
            boxes[at].2.extend(reference);
        };
        for row in rows {
            if let Some(role) = &row.verse {
                let words = words_of(row);
                if !words.is_empty() {
                    put(role, Some(words), None);
                }
            }
        }
        if reference_shown {
            for row in rows {
                if let Some(role) = &row.reference {
                    put(role, None, Some(reference_of(row)));
                }
            }
        }
        let objects: Vec<Object> = boxes
            .into_iter()
            .map(|(role, words, references)| {
                // Translations sharing a box each take a line; references sharing one sit side by side.
                let mut lines = words;
                if !references.is_empty() {
                    lines.push(references.join("  ·  "));
                }
                Object::text(role, lines.join("\n"))
            })
            .collect();
        // Each slide is a group of its own, named by the verses on it, so the bar under
        // every slide says where it is from.
        groups.push(Group {
            id: Uuid::new_v4(),
            name: label(lead, from, to),
            // A Bible slide's bar is white on black.
            color: Rgba::rgb(0, 0, 0),
            slides: vec![Slide { template, ..Slide::new(objects) }],
        });
    }
    Ok(Presentation {
        id: Uuid::new_v4(),
        title: lead.bible.label(first, last),
        groups,
        copyright: sp_model::Copyright::default(),
        theme: None,
        arrangements: Vec::new(),
        arrangement: None,
        loops: false,
        transition: None,
    })
}

/// What is chosen: grey, as the rest of the app.
const BLUE: Color32 = crate::studio::SELECTED;
const SHELL: Color32 = Color32::from_rgb(30, 30, 30);
const PANE: Color32 = Color32::from_rgb(48, 48, 48);
const DIM: Color32 = Color32::from_gray(160);
const INK: Color32 = Color32::from_gray(235);

/// Where the reference (such as "John 3:16") is shown on the slides made from a passage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReferenceStyle {
    /// Each slide names its own verses.
    #[default]
    VerseReferences,
    /// Each slide names the whole passage.
    PassageEach,
    /// Only the last slide names the passage.
    PassageLast,
    NoReference,
}

/// A Bible that can be downloaded. Every entry must be free to pass on, including in an app
/// that is sold: `docs/licences/bibles.md` says where each one's terms were read and what they
/// ask. Do not add a Bible here without adding it there.
pub struct Offered {
    pub language: &'static str,
    pub name: &'static str,
    pub short: &'static str,
    /// Its short name for files: the download is `<id>.pcbible` (the ending it was published with).
    pub id: &'static str,
    /// The terms it is offered under, in a few words.
    pub terms: &'static str,
    /// The SHA-256 of its file. A download with any other checksum is not installed.
    pub checksum: &'static str,
}

/// The Bibles offered under "Free", in language order. The four that come with the app are
/// not here.
pub const OFFERED: &[Offered] = &[
    Offered {
        language: "English",
        name: "American Standard Version",
        short: "ASV",
        id: "asv",
        terms: "Public domain",
        checksum: "2c22ff4c11a61c54290c6ed2707b0db42bfd47d45cafbb53dc99c86fb09d1145",
    },
    Offered {
        language: "English",
        name: "Young's Literal Translation",
        short: "YLT",
        id: "ylt",
        terms: "Public domain",
        checksum: "e4fa412454a02a4d9e8b869db7f930f2bfef77fab00c06bc155fc5935cd817d5",
    },
    Offered {
        language: "English",
        name: "Darby Translation",
        short: "DBY",
        id: "dby",
        terms: "Public domain",
        checksum: "ef27866208f7b101c3aea860ed0fea9d083e982bca8c54fd7f228a61026af1f3",
    },
    Offered {
        language: "English",
        name: "Geneva Bible 1599",
        short: "GNV",
        id: "gnv",
        terms: "Public domain",
        checksum: "f87c06d1934e1a075c09246fd604ef22d31be162567c215bdfc28043ac02eb1a",
    },
    Offered {
        language: "French",
        name: "Louis Segond 1910",
        short: "LSG",
        id: "lsg",
        terms: "Public domain",
        checksum: "7f044422526909a0611618fe55663db4079964f1db0dc0cdc304b244db4682ee",
    },
    Offered {
        language: "French",
        name: "Bible Darby",
        short: "FRDBY",
        id: "frdby",
        terms: "Public domain",
        checksum: "5d179898daa5e6ed02009eb7909d62fe7273eb55cb13d519fd1478c7a4d40eb2",
    },
    Offered {
        language: "German",
        name: "Luther 1912",
        short: "LUT1912",
        id: "lut1912",
        terms: "Public domain",
        checksum: "d9e65a85d606686f06bf2131ebd6f00a1113c783a16b3753edf59d81183ed3b5",
    },
    Offered {
        language: "German",
        name: "Elberfelder 1905",
        short: "ELB1905",
        id: "elb1905",
        terms: "Public domain",
        checksum: "b208bc964c3889f2dc7e08fb9395e8bfdd2d276a89fe49dfed0045909752f8b2",
    },
    Offered {
        language: "Greek",
        name: "SBL Greek New Testament",
        short: "SBLGNT",
        id: "sblgnt",
        terms: "CC BY 4.0",
        checksum: "5874411a89d59706a1bf7d8d34616ddd91d86aadc7dffbd911da1d8769da6c7f",
    },
    Offered {
        language: "Greek",
        name: "Byzantine Text, Robinson-Pierpont",
        short: "RP",
        id: "rp",
        terms: "Public domain",
        checksum: "57876e9b5662d7095cc6cdf60412c87047e39e6943c92bfaeeb2a0f72b2979df",
    },
    Offered {
        language: "Hebrew",
        name: "Westminster Leningrad Codex",
        short: "WLC",
        id: "wlc",
        terms: "Public domain",
        checksum: "376b60ec5e9db4c4afeb5fd5b542c53cea2973e38e879f368a6b09011cbb43d7",
    },
    Offered {
        language: "Portuguese",
        name: "Bíblia Livre",
        short: "BLIVRE",
        id: "blivre",
        terms: "CC BY 4.0",
        checksum: "25d1b645b9a82bd7c8cd21f3999987b35072744056a8bdb76d627889bee4f1f4",
    },
    Offered {
        language: "Spanish",
        name: "Reina-Valera 1909",
        short: "RV1909",
        id: "rv1909",
        terms: "Public domain",
        checksum: "2e48a2e412be0471cd1dacbd4bc9c4de213c7cb00ad9d5b9080c06ed2f389fd7",
    },
    Offered {
        language: "Spanish",
        name: "Versión Biblia Libre",
        short: "VBL",
        id: "vbl",
        terms: "CC BY-SA 4.0",
        checksum: "07c50f1e213747cac53b32a515a79fe9f3f774dfbebb0cd20b172811b032119d",
    },
    Offered {
        language: "Ukrainian",
        name: "Переклад Куліша та Пулюя",
        short: "UKR1905",
        id: "ukr1905",
        terms: "Public domain",
        checksum: "2f7f15cdc4bcdd1bde456487d202258846c6259aafa0feeb83f5038bef3d63a3",
    },
    Offered {
        language: "Ukrainian",
        name: "Новий переклад українською (Новий Завіт і Псалми)",
        short: "НПУ",
        id: "ukronpu",
        terms: "CC BY-SA 4.0",
        checksum: "5c34d4bfce41efa33e85bf003160d1c4b439a275083a0c5de5a8f14040dc3265",
    },
];

#[derive(Debug)]
pub struct BibleOptions {
    pub open: bool,
    /// The Bibles window, where Bibles are installed and removed, and what is typed in its
    /// search box.
    pub manage_open: bool,
    pub manage_search: String,
    /// 0 is Slide Options, 1 is Bibles.
    pub pane: usize,
    pub show_verse_numbers: bool,
    pub break_on_new_verse: bool,
    pub display_translation: bool,
    pub reference: ReferenceStyle,
    /// What went wrong with the last download, for the operator.
    pub problem: Option<String>,
    /// True when one of the pop-up's own menus was open on the last frame, so the click that
    /// closed that menu does not also close the pop-up.
    pub menu_was_open: bool,
    /// A downloaded Bible asked to be removed, by its place on the shelf.
    pub remove: Option<usize>,
}

impl Default for BibleOptions {
    fn default() -> BibleOptions {
        BibleOptions {
            open: false,
            manage_open: false,
            manage_search: String::new(),
            pane: 0,
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            reference: ReferenceStyle::VerseReferences,
            problem: None,
            menu_was_open: false,
            remove: None,
        }
    }
}

/// The themes to choose from for Bible slides, and the choice now made.
pub struct ThemeChoice<'a> {
    pub names: &'a [String],
    /// What each theme calls its two templates.
    pub template_names: &'a [Vec<String>],
    /// Each theme's two layouts, as boxes, for the pictures.
    pub layouts: &'a [Vec<Vec<(Role, sp_model::SlideRect)>>],
    pub theme: usize,
    pub layout: usize,
}

impl ThemeChoice<'_> {
    /// What a theme calls one of its templates.
    fn template_name(&self, theme: usize, template: usize) -> &str {
        self.template_names.get(theme).and_then(|names| names.get(template)).map_or("", String::as_str)
    }
}

/// Draws the pop-up under the Options button, its right edge in line with the button's.
pub fn options_popup(
    ctx: &egui::Context,
    options: &mut BibleOptions,
    shelf: &mut Shelf,
    downloads: &mut crate::bible_store::Downloads,
    button: Rect,
    themes: &ThemeChoice,
) -> Option<(usize, usize)> {
    let mut picked_theme = None;
    let size = vec2(508.0, 262.0);
    let corner = pos2(button.max.x - size.x + 8.0, button.max.y + 8.0);
    let area = egui::Area::new(egui::Id::unique("bible options"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .constrain(false)
        .fixed_pos(corner)
        .show(ctx, |ui| {
            Frame::new()
                .fill(SHELL)
                .corner_radius(12)
                .stroke(Stroke::new(1.0, Color32::from_gray(78)))
                .shadow(egui::epaint::Shadow {
                    offset: [0, 6],
                    blur: 18,
                    spread: 0,
                    color: Color32::from_black_alpha(120),
                })
                .show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
                    ui.horizontal_top(|ui| {
                        // Left: the two panes to choose from.
                        ui.allocate_ui_with_layout(vec2(144.0, size.y), Layout::top_down(Align::Min), |ui| {
                            ui.set_min_size(vec2(144.0, size.y));
                            for (index, name) in ["Slide Options", "Bibles"].into_iter().enumerate() {
                                let (row, response) = ui.allocate_exact_size(vec2(144.0, 46.0), Sense::click());
                                let selected = options.pane == index;
                                if selected {
                                    let radius = if index == 0 {
                                        egui::CornerRadius { nw: 12, ne: 0, sw: 0, se: 0 }
                                    } else {
                                        egui::CornerRadius::ZERO
                                    };
                                    ui.painter().rect_filled(row, radius, BLUE);
                                } else if response.hovered() {
                                    ui.painter().rect_filled(row, 0.0, Color32::from_gray(44));
                                }
                                let mark =
                                    Rect::from_center_size(pos2(row.min.x + 26.0, row.center().y), vec2(18.0, 18.0));
                                if index == 0 {
                                    ui.painter().circle_filled(mark.center(), 10.0, Color32::from_rgb(70, 88, 220));
                                    ui.painter().text(
                                        mark.center(),
                                        Align2::CENTER_CENTER,
                                        "i",
                                        FontId::proportional(13.0),
                                        INK,
                                    );
                                } else {
                                    crate::icons::paint(ui.painter(), crate::icons::Icon::Book, mark, INK);
                                }
                                ui.painter().text(
                                    pos2(row.min.x + 48.0, row.center().y),
                                    Align2::LEFT_CENTER,
                                    name,
                                    FontId::proportional(13.0),
                                    INK,
                                );
                                if response.clicked() {
                                    // Bibles are installed and removed in a window of their own.
                                    if index == 1 {
                                        options.manage_open = true;
                                        options.open = false;
                                    } else {
                                        options.pane = 0;
                                    }
                                }
                            }
                        });
                        // Right: the chosen pane.
                        let pane =
                            Frame::new().fill(PANE).corner_radius(egui::CornerRadius { nw: 0, ne: 12, sw: 0, se: 12 });
                        pane.inner_margin(egui::Margin::symmetric(16, 12)).show(ui, |ui| {
                            ui.set_min_size(vec2(size.x - 144.0 - 32.0, size.y - 24.0));
                            ui.set_max_width(size.x - 144.0 - 32.0);
                            ui.spacing_mut().item_spacing = vec2(8.0, 6.0);
                            // The pane sits in a row, so its own contents need to be told to run downwards.
                            ui.vertical(|ui| {
                                ui.set_width(size.x - 144.0 - 32.0);
                                let _ = (&shelf, &downloads);
                                picked_theme = slide_options(ui, options, themes);
                            });
                        });
                    });
                });
        });
    // A click anywhere outside the pop-up and its button closes it, as any pop-over does. A
    // click in one of its own menus, which open outside it, does not.
    let outside = ctx.input(|input| {
        input.pointer.any_click()
            && input.pointer.interact_pos().is_some_and(|at| !area.response.rect.contains(at) && !button.contains(at))
    });
    if outside && !egui::Popup::is_any_open(ctx) && !std::mem::take(&mut options.menu_was_open) {
        options.open = false;
    }
    options.menu_was_open = egui::Popup::is_any_open(ctx);
    picked_theme
}

fn slide_options(ui: &mut egui::Ui, options: &mut BibleOptions, themes: &ThemeChoice) -> Option<(usize, usize)> {
    let mut picked_theme = None;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.checkbox(&mut options.show_verse_numbers, "Show Verse Numbers")
                .on_hover_text("Put each verse's number before its words");
            ui.checkbox(&mut options.break_on_new_verse, "Break on New Verse")
                .on_hover_text("Start a new slide at each verse");
            ui.checkbox(&mut options.display_translation, "Display Translation")
                .on_hover_text("Show the Bible's short name, such as KJV, with the reference");
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for (style, name) in [
                (ReferenceStyle::VerseReferences, "Verse References"),
                (ReferenceStyle::PassageEach, "Passage Each"),
                (ReferenceStyle::PassageLast, "Passage Last"),
                (ReferenceStyle::NoReference, "No Reference"),
            ] {
                ui.radio_value(&mut options.reference, style, name);
            }
        });
    });
    ui.add_space(6.0);
    egui::Grid::new("bible slide options").num_columns(2).spacing([10.0, 12.0]).show(ui, |ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(egui::RichText::new("Theme").strong()));
        // The theme Bible slides use, and which of its layouts. Each layout is shown as a
        // small picture of where its boxes sit.
        let chosen = format!(
            "{} › {}",
            themes.names.get(themes.theme).map_or("", String::as_str),
            themes.template_name(themes.theme, themes.layout)
        );
        egui::ComboBox::from_id_salt("bible theme")
            .selected_text(chosen)
            .width(190.0)
            .height(320.0)
            .show_ui(ui, |ui| {
                for (index, name) in themes.names.iter().enumerate() {
                    ui.label(egui::RichText::new(name.as_str()).font(crate::studio::semibold(12.0)).color(DIM));
                    for layout in 0..themes.layouts.get(index).map_or(0, Vec::len) {
                        let (row, response) = ui.allocate_exact_size(vec2(210.0, 40.0), Sense::click());
                        let on = index == themes.theme && layout == themes.layout;
                        if on {
                            ui.painter().rect_filled(row, 6.0, crate::studio::ACCENT.gamma_multiply(0.3));
                        } else if response.hovered() {
                            ui.painter().rect_filled(row, 6.0, crate::studio::HOVER);
                        }
                        let picture = Rect::from_min_size(row.min + vec2(6.0, 4.0), vec2(57.0, 32.0));
                        let boxes = themes.layouts.get(index).map_or(&[][..], |layouts| layouts[layout].as_slice());
                        crate::home::box_diagram(ui.painter(), picture, boxes, None);
                        ui.painter().text(
                            pos2(picture.max.x + 10.0, row.center().y),
                            Align2::LEFT_CENTER,
                            themes.template_name(index, layout),
                            FontId::proportional(13.0),
                            INK,
                        );
                        if response.clicked() {
                            picked_theme = Some((index, layout));
                            ui.close();
                        }
                    }
                    ui.add_space(4.0);
                }
            })
            .response
            .on_hover_text(
                "The theme and layout Bible slides use. Its boxes are what the Verse and Reference menus offer.",
            );
        ui.end_row();
    });
    picked_theme
}

/// How wide the book, chapter and verse lists are, together.
pub const LISTS_WIDTH: f32 = 300.0;

/// The book, chapter and verse lists beside the Bible page's slides, in the first
/// translation's language. A book opens its chapters, a chapter its verses; a verse is looked
/// up at once (its slides appear beside the lists), Shift-click makes a passage from the verse
/// before, and All takes the whole chapter. What is showing is lit, whichever way it was
/// looked up. Nothing goes on screen until a slide is clicked.
pub fn lists(ui: &mut egui::Ui, page: &mut crate::home::BiblePage, live: Option<usize>) {
    let bible = page.lead().and_then(|lead| page.shelf.get(lead).ok());
    let Some(bible) = bible else {
        ui.add_space(12.0);
        nothing(ui, "Choose a Bible for the first row.");
        return;
    };
    let shown = parse_reference_in(&page.shown, &bible.info.language).ok();
    let lit = |book: usize, chapter: u16, verse: u16| {
        shown.is_some_and(|range| {
            let here = (book, chapter, verse);
            (range.start.book, range.start.chapter, range.start.verse) <= here
                && here <= (range.end.book, range.end.chapter, range.end.verse)
        })
    };
    let scroll = std::mem::take(&mut page.list_scroll);
    let height = ui.available_height();
    let new_testament = sp_bible::BOOKS.iter().position(|book| book.code == "MAT").unwrap_or(39);
    let mut chosen: Option<String> = None;
    let shift = ui.input(|input| input.modifiers.shift);
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    Frame::new().fill(Color32::from_rgb(34, 34, 36)).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            // Books.
            list_column(ui, "bible books", 176.0, height, |ui| {
                for book in 0..sp_bible::BOOKS.len() {
                    let chapters = bible.chapter_count(book);
                    if chapters == 0 {
                        continue;
                    }
                    if book == 0 || book == new_testament {
                        let heading = if book == 0 { "OLD TESTAMENT" } else { "NEW TESTAMENT" };
                        ui.add_space(if book == 0 { 6.0 } else { 10.0 });
                        ui.label(egui::RichText::new(heading).size(10.0).strong().color(Color32::from_gray(120)));
                        ui.add_space(3.0);
                    }
                    let open = page.list_book == book;
                    let showing = shown.is_some_and(|range| range.start.book == book);
                    let row = list_row(ui, ("book", book), bible.book_name(book), open, showing, Align::Min);
                    if open && scroll {
                        row.scroll_to_me(Some(Align::Center));
                    }
                    if row.clicked() {
                        page.list_book = book;
                        page.list_chapter = if chapters == 1 { 1 } else { 0 };
                        page.list_from = None;
                    }
                }
            });
            // Chapters of the book open.
            let chapters = bible.chapter_count(page.list_book);
            list_column(ui, "bible chapters", 58.0, height, |ui| {
                ui.add_space(6.0);
                for chapter in 1..=chapters {
                    let open = page.list_chapter == chapter;
                    let showing = shown.is_some_and(|r| {
                        r.start.book == page.list_book && r.start.chapter <= chapter && chapter <= r.end.chapter
                    });
                    let row = list_row(ui, ("chapter", chapter), &chapter.to_string(), open, showing, Align::Center);
                    if open && scroll {
                        row.scroll_to_me(Some(Align::Center));
                    }
                    if row.clicked() {
                        page.list_chapter = chapter;
                        page.list_from = None;
                    }
                }
            });
            // Verses of the chapter open.
            list_column(ui, "bible verses", LISTS_WIDTH - 176.0 - 58.0, height, |ui| {
                ui.add_space(6.0);
                let (book, chapter) = (page.list_book, page.list_chapter);
                if chapter == 0 || chapter > chapters {
                    return;
                }
                let name = bible.book_name(book);
                if list_row(ui, ("all", chapter), "All", false, false, Align::Center)
                    .on_hover_text("The whole chapter")
                    .clicked()
                {
                    chosen = Some(format!("{name} {chapter}"));
                }
                for verse in 1..=bible.last_verse(book, chapter) {
                    let row = list_row(
                        ui,
                        ("verse", verse),
                        &verse.to_string(),
                        false,
                        lit(book, chapter, verse),
                        Align::Center,
                    );
                    if lit(book, chapter, verse) && scroll {
                        row.scroll_to_me(Some(Align::Center));
                    }
                    if row.clicked() {
                        let place = |verse| VerseRef { book, chapter, verse };
                        chosen = Some(match page.list_from.filter(|_| shift) {
                            Some(from) => bible.label(place(from.min(verse)), place(from.max(verse))),
                            None => bible.label(place(verse), place(verse)),
                        });
                        if !shift {
                            page.list_from = Some(verse);
                        }
                    }
                }
            });
        });
    });
    if let Some(reference) = chosen {
        page.lookup = reference;
        page.look_up(live);
    }
}

/// One of the lists: a column of its width, scrolling on its own.
fn list_column(ui: &mut egui::Ui, id: &str, width: f32, height: f32, add: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(vec2(width, height), Layout::top_down(Align::Min), |ui| {
        ui.set_min_size(vec2(width, height));
        egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).max_height(height).show(ui, |ui| {
            ui.set_width(width);
            Frame::new().inner_margin(egui::Margin::symmetric(6, 0)).show(ui, add);
            ui.add_space(8.0);
        });
    });
}

/// One row of a list: open (blue), part of the passage showing (lit), or neither.
fn list_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    text: &str,
    open: bool,
    showing: bool,
    align: Align,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let response = ui.interact(rect, ui.scope_id().with(id), Sense::click());
    let fill = if open {
        BLUE
    } else if showing {
        Color32::from_white_alpha(30)
    } else if response.hovered() {
        Color32::from_white_alpha(12)
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5.0, fill);
    let (at, anchor) = match align {
        Align::Min => (pos2(rect.min.x + 8.0, rect.center().y), Align2::LEFT_CENTER),
        _ => (rect.center(), Align2::CENTER_CENTER),
    };
    let painter = ui.painter().with_clip_rect(rect.shrink(2.0));
    painter.text(at, anchor, text, FontId::proportional(13.0), if open || showing { Color32::WHITE } else { INK });
    response
}

/// A language as people who speak it write it, for the Bibles window: from a language code
/// (`uk`) or an English name ("Ukrainian").
pub fn language_name(language: &str) -> String {
    let native = match language.to_lowercase().as_str() {
        "en" | "english" => "English",
        "ru" | "russian" => "Русский",
        "uk" | "ukrainian" => "Українська",
        "de" | "german" => "Deutsch",
        "fr" | "french" => "Français",
        "es" | "spanish" => "Español",
        "pt" | "portuguese" => "Português",
        "el" | "grc" | "greek" => "Ελληνικά · Greek",
        "he" | "hbo" | "hebrew" => "עברית · Hebrew",
        _ => return language.to_owned(),
    };
    native.to_owned()
}

/// Puts downloads that have just ended on the shelf, or says what went wrong. Done every
/// frame, so a Bible finished while its window is closed is ready all the same.
pub fn collect_downloads(options: &mut BibleOptions, shelf: &mut Shelf, downloads: &mut crate::bible_store::Downloads) {
    for (_, result) in downloads.finished() {
        match result {
            Ok((path, info)) => {
                options.problem = None;
                shelf.add_file(path, info);
            }
            Err(problem) => options.problem = Some(problem),
        }
    }
}

/// The Bibles window: a search box, the Bibles installed (each with a Remove button, but for
/// the ones built in), and every Bible that can be downloaded, grouped by language, each with
/// its terms and an Install button that shows the download going on. Removing asks nothing
/// more: the file goes to the Trash and can be put back. Escape or Done closes it.
pub fn manage_bibles(
    ctx: &egui::Context,
    options: &mut BibleOptions,
    shelf: &mut Shelf,
    downloads: &mut crate::bible_store::Downloads,
) {
    let can_download = crate::bible_store::download_base().is_some();
    let frame = Frame::new()
        .fill(Color32::from_rgb(38, 38, 40))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(22)))
        .corner_radius(14.0)
        .inner_margin(egui::Margin { left: 22, right: 22, top: 18, bottom: 16 })
        .shadow(egui::Shadow { offset: [0, 16], blur: 44, spread: 0, color: Color32::from_black_alpha(150) });
    let area =
        egui::Modal::default_area(egui::Id::unique("manage bibles")).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0));
    let mut close = false;
    egui::Modal::new(egui::Id::unique("manage bibles")).area(area).backdrop_color(Color32::from_black_alpha(90)).frame(frame).show(
        ctx,
        |ui| {
            let width = 600.0_f32.min(ctx.content_rect().width() - 80.0);
            ui.set_width(width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Bibles").font(crate::studio::semibold(18.0)).color(Color32::WHITE));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        close = true;
                    }
                    ui.add(
                        egui::TextEdit::singleline(&mut options.manage_search)
                            .hint_text("Search by name or language")
                            .desired_width(220.0),
                    );
                });
            });
            ui.add_space(8.0);
            if let Some(problem) = &options.problem {
                ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(Color32::from_rgb(255, 150, 120)));
                ui.add_space(6.0);
            }
            let typed = options.manage_search.trim().to_lowercase();
            let matches = |name: &str, short: &str, language: &str| {
                typed.is_empty()
                    || [name, short, language, &language_name(language)].iter().any(|t| t.to_lowercase().contains(&typed))
            };
            let height = (ctx.content_rect().height() - 220.0).clamp(240.0, 560.0);
            egui::ScrollArea::vertical().id_salt("bibles window").max_height(height).auto_shrink([false, true]).show(ui, |ui| {
                ui.set_width(width);
                heading(ui, "INSTALLED");
                let mut any = false;
                for (index, bible) in shelf.bibles.iter().enumerate() {
                    if !matches(&bible.name, &bible.short, &bible.language) {
                        continue;
                    }
                    any = true;
                    let detail = format!("{} · {}", bible.short, language_name(&bible.language));
                    let right = if bible.shipped { Right::Note("Built in") } else { Right::Button("Remove") };
                    if bible_row(ui, ("installed", index), &bible.name, &detail, None, right) {
                        options.remove = Some(index);
                    }
                }
                if !any {
                    nothing(ui, "No installed Bible matches.");
                }
                ui.add_space(12.0);
                heading(ui, "AVAILABLE");
                if !can_download {
                    nothing(ui, "Downloads aren't available in this copy. Drop a Bible file (.spbible) on the window to install it.");
                }
                let mut language = "";
                let mut shown = 0;
                for bible in OFFERED {
                    if shelf.position(bible.id).is_some() || !matches(bible.name, bible.short, bible.language) {
                        continue;
                    }
                    if bible.language != language {
                        language = bible.language;
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new(language_name(language)).font(crate::studio::semibold(12.5)).color(DIM));
                        ui.add_space(2.0);
                    }
                    shown += 1;
                    let right = if downloads.is_busy(bible.id) {
                        Right::Note("Downloading…")
                    } else if can_download {
                        Right::Button("Install")
                    } else {
                        Right::Note("")
                    };
                    if bible_row(ui, ("offered", bible.id), bible.name, bible.short, Some(bible.terms), right) {
                        let ctx = ui.ctx().clone();
                        downloads.start(bible.id, bible.checksum, move || ctx.request_repaint());
                    }
                }
                if shown == 0 && can_download {
                    nothing(ui, if typed.is_empty() { "Every Bible offered is installed." } else { "No Bible to install matches." });
                }
            });
            ui.add_space(10.0);
            ui.label(
                egui::RichText::new(
                    "Every Bible here may be shared freely: public domain, or an open licence. A Bible file you have (.spbible) installs when dropped on the window.",
                )
                .size(11.0)
                .color(DIM),
            );
            if downloads_busy(downloads) {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(250));
            }
        },
    );
    if close || ctx.input(|input| input.key_pressed(egui::Key::Escape)) {
        options.manage_open = false;
    }
}

/// True while any Bible is downloading.
fn downloads_busy(downloads: &crate::bible_store::Downloads) -> bool {
    OFFERED.iter().any(|bible| downloads.is_busy(bible.id))
}

fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.0).strong().color(Color32::from_gray(130)));
    ui.add_space(4.0);
}

fn nothing(ui: &mut egui::Ui, text: &str) {
    ui.add(egui::Label::new(egui::RichText::new(text).size(12.0).color(DIM)).wrap());
}

/// What a Bible's row shows at its right.
enum Right {
    Note(&'static str),
    Button(&'static str),
}

/// One Bible in the window: its name, a line under it, its terms, and a note or a button at
/// the right, which keeps its room however long the name is. True when the button is clicked.
fn bible_row(
    ui: &mut egui::Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    name: &str,
    detail: &str,
    terms: Option<&str>,
    right: Right,
) -> bool {
    let width = ui.available_width();
    let (row, hover) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
    if hover.hovered() {
        ui.painter().rect_filled(row, 8.0, Color32::from_white_alpha(8));
    }
    // The right side: the button (or note), and the terms before it.
    let button = Rect::from_min_size(pos2(row.max.x - 92.0, row.center().y - 13.0), vec2(84.0, 26.0));
    let mut clicked = false;
    match right {
        Right::Button(label) => {
            let response = ui.interact(button, ui.scope_id().with(id), Sense::click());
            let fill = if response.hovered() { Color32::from_white_alpha(40) } else { Color32::from_white_alpha(24) };
            ui.painter().rect_filled(button, 7.0, fill);
            ui.painter().text(button.center(), Align2::CENTER_CENTER, label, FontId::proportional(12.5), INK);
            clicked = response.clicked();
        }
        Right::Note(note) => {
            ui.painter().text(button.center(), Align2::CENTER_CENTER, note, FontId::proportional(11.5), DIM);
        }
    }
    let mut text_right = button.min.x - 10.0;
    if let Some(terms) = terms.filter(|t| !t.is_empty()) {
        let galley = ui.painter().layout_no_wrap(terms.to_owned(), FontId::proportional(10.5), DIM);
        let pill = Rect::from_min_size(
            pos2(text_right - galley.size().x - 12.0, row.center().y - 9.0),
            vec2(galley.size().x + 12.0, 18.0),
        );
        ui.painter().rect_filled(pill, 9.0, Color32::from_white_alpha(14));
        ui.painter().galley(pos2(pill.min.x + 6.0, pill.center().y - galley.size().y / 2.0), galley, DIM);
        text_right = pill.min.x - 10.0;
    }
    // The name and the line under it, cut off before what is at the right.
    let clip = Rect::from_min_max(pos2(row.min.x + 10.0, row.min.y), pos2(text_right, row.max.y));
    let painter = ui.painter().with_clip_rect(clip);
    painter.text(pos2(row.min.x + 10.0, row.min.y + 14.0), Align2::LEFT_CENTER, name, FontId::proportional(13.5), INK);
    painter.text(
        pos2(row.min.x + 10.0, row.min.y + 31.0),
        Align2::LEFT_CENTER,
        detail,
        FontId::proportional(11.0),
        DIM,
    );
    clicked
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_ukrainian_bible_reads_ukrainian_book_names_and_shows_them() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../dist/bibles/ukr1905.pcbible");
        let Ok(bytes) = std::fs::read(path) else { return };
        let mut shelf = Shelf::shipped();
        let info = sp_bible::Bible::read_info(&bytes).unwrap();
        shelf.add_file(path.into(), info);
        let rows = rows(&mut shelf, &[("ukr1905", main(), reference())]);
        let passage = build_passage("Ів 3:16", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.title, "Від Івана 3:16");
    }

    use super::*;

    fn main() -> BibleTarget {
        Some(Role::primary())
    }

    fn translation() -> BibleTarget {
        Some(Role::translation())
    }

    fn reference() -> BibleTarget {
        Some(Role::reference())
    }

    fn rows(shelf: &mut Shelf, ids: &[(&str, BibleTarget, BibleTarget)]) -> Vec<RowChoice> {
        ids.iter()
            .map(|(id, verse, reference)| RowChoice {
                bible: shelf.get(shelf.position(id).unwrap()).unwrap(),
                verse: verse.clone(),
                reference: reference.clone(),
            })
            .collect()
    }

    fn texts(slide: &Slide) -> Vec<&str> {
        slide.objects.iter().filter_map(Object::as_text).collect()
    }

    #[test]
    fn a_reference_becomes_one_slide_for_each_verse_in_both_languages() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), reference()), ("syn", translation(), None)]);
        let passage = build_passage("John 3:16-17", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.title, "John 3:16-17");
        assert_eq!(passage.slide_count(), 2);
        // Each slide's bar names its own verse.
        assert_eq!(passage.groups.iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), ["John 3:16", "John 3:17"]);
        let (_, first) = passage.slide(0).unwrap();
        let shown = texts(first);
        assert!(shown[0].starts_with("For God so loved the world"));
        assert!(shown[1].starts_with("Ибо так возлюбил Бог мир"));
        assert_eq!(shown[2], "John 3:16");
    }

    #[test]
    fn the_russian_psalm_is_the_one_that_matches_the_english() {
        let mut shelf = Shelf::shipped();
        let english_first = rows(&mut shelf, &[("kjv", main(), reference()), ("syn", translation(), reference())]);
        let passage = build_passage("Psalm 23:1", &english_first, &BibleOptions::default(), None).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("The LORD is my shepherd") && shown.contains("Пастырь мой"), "{shown}");
        assert!(shown.contains("Psalms 23:1  ·  Псалтирь 22:1"), "{shown}");
        // Typed the Russian way with the Russian Bible first, it is the same psalm.
        let russian_first = rows(&mut shelf, &[("syn", main(), None), ("kjv", translation(), None)]);
        let passage = build_passage("Пс 22:1", &russian_first, &BibleOptions::default(), None).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("Пастырь мой") && shown.contains("The LORD is my shepherd"), "{shown}");
    }

    #[test]
    fn the_options_change_the_slides() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("web", main(), reference())]);
        let options = BibleOptions {
            break_on_new_verse: false,
            show_verse_numbers: true,
            display_translation: true,
            ..BibleOptions::default()
        };
        let passage = build_passage("jn 3:16-17", &rows, &options, None).unwrap();
        assert_eq!(passage.slide_count(), 1);
        let shown = texts(passage.slide(0).unwrap().1);
        assert!(
            shown[0].starts_with("¹⁶\u{2009}For God so loved") && shown[0].contains(" ¹⁷\u{2009}For God"),
            "{}",
            shown[0]
        );
        assert_eq!(shown[1], "John 3:16-17 (WEB)");

        let last_only = BibleOptions { reference: ReferenceStyle::PassageLast, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16-17", &rows, &last_only, None).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
        assert_eq!(texts(passage.slide(1).unwrap().1)[1], "John 3:16-17");

        let none = BibleOptions { reference: ReferenceStyle::NoReference, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16", &rows, &none, None).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
    }

    #[test]
    fn words_and_reference_sharing_a_box_are_one_object_with_the_reference_last() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), main()), ("syn", translation(), None)]);
        let passage = build_passage("John 3:16", &rows, &BibleOptions::default(), None).unwrap();
        let (_, slide) = passage.slide(0).unwrap();
        assert_eq!(slide.objects.len(), 2);
        let main_box = slide.text_for(&Role::primary()).unwrap();
        assert!(main_box.starts_with("For God so loved") && main_box.ends_with("\nJohn 3:16"), "{main_box}");
        // A translation sent nowhere is left off the slide.
        let rows = self::rows(&mut shelf, &[("kjv", main(), None), ("syn", None, None)]);
        let passage = build_passage("John 3:16", &rows, &BibleOptions::default(), None).unwrap();
        assert_eq!(passage.slide(0).unwrap().1.objects.len(), 1);
    }

    #[test]
    fn bible_slides_name_the_template_chosen_for_them() {
        let mut shelf = Shelf::shipped();
        let theme = sp_model::Theme::builtin();
        let chosen = theme.templates[0].id;
        // One language, on the template for two: the slide still uses the one it names.
        let one = rows(&mut shelf, &[("kjv", main(), reference())]);
        let passage = build_passage("John 3:16-17", &one, &BibleOptions::default(), Some(chosen)).unwrap();
        for (_, slide) in passage.slides() {
            assert_eq!(slide.template, Some(chosen));
            assert_eq!(theme.template_for(slide).map(|template| template.id), Some(chosen));
            assert_eq!(slide.objects[0].frame, None);
        }
    }

    #[test]
    fn a_reference_that_names_nothing_says_so() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", main(), reference())]);
        // A chapter past the end of the book is read as its last chapter.
        let last = build_passage("John 99", &rows, &BibleOptions::default(), None).unwrap();
        assert!(last.title.starts_with("John 21"), "{}", last.title);
        assert!(
            build_passage("John 3:99", &rows, &BibleOptions::default(), None).unwrap_err().contains("no such verses")
        );
        assert!(build_passage("Nobody 1:1", &rows, &BibleOptions::default(), None).unwrap_err().contains("No book"));
        assert!(build_passage("John 3:16", &[], &BibleOptions::default(), None).is_err());
    }

    #[test]
    fn offered_bibles_are_grouped_by_language_and_named_once() {
        let mut seen: Vec<&str> = Vec::new();
        let mut languages: Vec<&str> = Vec::new();
        let shipped = Shelf::shipped();
        for bible in OFFERED {
            assert!(!seen.contains(&bible.id), "{} is listed twice", bible.id);
            assert!(shipped.position(bible.id).is_none(), "{} already comes with the app", bible.id);
            assert_eq!(bible.checksum.len(), 64);
            seen.push(bible.id);
            if languages.last() != Some(&bible.language) {
                assert!(!languages.contains(&bible.language), "{} is split into two groups", bible.language);
                languages.push(bible.language);
            }
        }
    }
}
