//! The Bible page's working parts: the Bibles installed, turning a typed reference into
//! slides, and the Options pop-up that says how.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};

use std::sync::Arc;

use pc_bible::{Bible, Numbering, VerseRef, convert, parse_reference};
use pc_model::{
    Align as TextAlign, Group, Object, ObjectKind, Presentation, Rgba, Role, Slide, SlideRect, TextStyle, Uuid,
};

use crate::home::{BibleTarget, not_built};

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
    pub terms: String,
    pub credit: String,
    pub numbering: Numbering,
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
            include_bytes!("../../../assets/bibles/kjv.pcbible"),
            include_bytes!("../../../assets/bibles/web.pcbible"),
            include_bytes!("../../../assets/bibles/bsb.pcbible"),
            include_bytes!("../../../assets/bibles/syn.pcbible"),
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

    fn put(&mut self, info: pc_bible::Info, kept: Kept, shipped: bool) {
        self.bibles.push(Shelved {
            id: info.id,
            name: info.name,
            short: info.short,
            terms: info.terms,
            credit: info.credit,
            numbering: info.numbering,
            shipped,
            kept,
            loaded: None,
        });
    }

    /// Puts a Bible from the Bibles folder on the shelf. One already there under the same
    /// short name stays, at its place, so the translations chosen do not shift.
    pub fn add_file(&mut self, path: std::path::PathBuf, info: pc_bible::Info) {
        if self.position(&info.id).is_none() {
            self.put(info, Kept::File(path), false);
        }
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

/// Where the reference line sits on a Bible slide, in slide units.
const REFERENCE_PLACE: SlideRect = SlideRect { x: 120.0, y: 950.0, w: 1680.0, h: 80.0 };
/// Where the verse sits when there is no second language under it.
const ALONE_PLACE: SlideRect = SlideRect { x: 120.0, y: 160.0, w: 1680.0, h: 740.0 };

/// Turns what was typed in Scripture Lookup into slides. The reference is read in the
/// numbering of the first translation; the others are lined up with it. The error is a plain
/// sentence for the operator.
pub fn build_passage(typed: &str, rows: &[RowChoice], options: &BibleOptions) -> Result<Presentation, String> {
    let lead = rows.first().ok_or_else(|| "Choose a Bible first.".to_owned())?;
    let range = parse_reference(typed)?;
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
    let mut slides = Vec::with_capacity(count);
    for (index, chunk) in chunks.into_iter().enumerate() {
        let words_for = |target: BibleTarget| -> String {
            let mut parts: Vec<String> = Vec::new();
            for row in rows.iter().filter(|row| row.verse == target) {
                let mut text = String::new();
                for place in chunk {
                    let there = in_row(row, *place);
                    let Some(words) = row.bible.verse(there) else { continue };
                    if !text.is_empty() {
                        text.push(' ');
                    }
                    if options.show_verse_numbers {
                        text.push_str(&format!("{} ", there.verse));
                    }
                    text.push_str(words);
                }
                if !text.is_empty() {
                    parts.push(text);
                }
            }
            parts.join("\n")
        };
        let (main, translation) = (words_for(BibleTarget::Main), words_for(BibleTarget::Translation));
        let mut objects = Vec::new();
        if !main.is_empty() {
            let mut object = Object::text(Role::primary(), main);
            if translation.is_empty() {
                object.frame = Some(ALONE_PLACE);
            }
            objects.push(object);
        }
        if !translation.is_empty() {
            objects.push(Object::text(Role::translation(), translation));
        }
        let (Some(from), Some(to)) = (chunk.first().copied(), chunk.last().copied()) else { continue };
        let named: Vec<String> = match options.reference {
            ReferenceStyle::NoReference => Vec::new(),
            ReferenceStyle::PassageLast if index + 1 != count => Vec::new(),
            style => rows
                .iter()
                .filter(|row| row.reference != BibleTarget::None)
                .map(|row| match style {
                    ReferenceStyle::VerseReferences => label(row, from, to),
                    _ => label(row, first, last),
                })
                .collect(),
        };
        if !named.is_empty() {
            objects.push(Object {
                id: Uuid::new_v4(),
                name: "Reference".to_owned(),
                role: None,
                frame: Some(REFERENCE_PLACE),
                rotation: 0.0,
                opacity: 1.0,
                fill: None,
                kind: ObjectKind::Text {
                    text: named.join("  ·  "),
                    style: TextStyle {
                        size: Some(44.0),
                        weight: Some(500),
                        outline_width: Some(2.0),
                        align: Some(TextAlign::Center),
                        ..TextStyle::default()
                    },
                },
            });
        }
        slides.push(Slide::new(objects));
    }
    let name = lead.bible.label(first, last);
    Ok(Presentation {
        id: Uuid::new_v4(),
        title: name.clone(),
        groups: vec![Group { id: Uuid::new_v4(), name, color: Rgba::rgb(40, 110, 200), slides }],
    })
}

const BLUE: Color32 = Color32::from_rgb(47, 123, 246);
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
    /// Its short name for files: the download is `<id>.pcbible`.
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
        checksum: "5dd7feac9014063c062ad314913e66f3f07e8a93763fe8ac701d3cf6b5cd10e9",
    },
    Offered {
        language: "French",
        name: "Louis Segond 1910",
        short: "LSG",
        id: "lsg",
        terms: "Public domain",
        checksum: "abbc1f94215e66f68c5d4a8a5c1a8a285a048aad8d26e56e444c34d6df5b9010",
    },
    Offered {
        language: "French",
        name: "Bible Darby",
        short: "FRDBY",
        id: "frdby",
        terms: "Public domain",
        checksum: "75fceec270c23298182f7502e87fe00eb5a69051c093afa5b2f7a9873c2f80d8",
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
        checksum: "4fa6c89017ea41a429d674759325372619811c8917ad30ed936cfa6be07a9944",
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
        checksum: "34349ded0cf4584b5735eb41c7a10cbca5045bd8a3691dc05794021cdb406257",
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
        checksum: "b8039d9d88784dd4da7c5be6755529609c698d03e436a5b8e0aa0e800f925c32",
    },
];

#[derive(Debug)]
pub struct BibleOptions {
    pub open: bool,
    /// 0 is Slide Options, 1 is Bibles.
    pub pane: usize,
    /// 0 is Installed, 1 is Free.
    pub bibles_tab: usize,
    pub show_verse_numbers: bool,
    pub break_on_new_verse: bool,
    pub display_translation: bool,
    pub preserve_font_color: bool,
    pub reference: ReferenceStyle,
    /// What went wrong with the last download, for the operator.
    pub problem: Option<String>,
}

impl Default for BibleOptions {
    fn default() -> BibleOptions {
        BibleOptions {
            open: false,
            pane: 0,
            bibles_tab: 1,
            show_verse_numbers: false,
            break_on_new_verse: true,
            display_translation: false,
            preserve_font_color: false,
            reference: ReferenceStyle::VerseReferences,
            problem: None,
        }
    }
}

/// Draws the pop-up under the Options button, its right edge in line with the button's.
pub fn options_popup(
    ctx: &egui::Context,
    options: &mut BibleOptions,
    shelf: &mut Shelf,
    downloads: &mut crate::bible_store::Downloads,
    button: Rect,
    library: &str,
) {
    let size = vec2(508.0, 262.0);
    let corner = pos2(button.max.x - size.x + 8.0, button.max.y + 8.0);
    egui::Area::new(egui::Id::unique("bible options"))
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
                                    options.pane = index;
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
                                if options.pane == 0 {
                                    slide_options(ui, options, library);
                                } else {
                                    bibles(ui, options, shelf, downloads);
                                }
                            });
                        });
                    });
                });
        });
}

fn slide_options(ui: &mut egui::Ui, options: &mut BibleOptions, library: &str) {
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.checkbox(&mut options.show_verse_numbers, "Show Verse Numbers")
                .on_hover_text("Put each verse's number before its words");
            ui.checkbox(&mut options.break_on_new_verse, "Break on New Verse")
                .on_hover_text("Start a new slide at each verse");
            ui.checkbox(&mut options.display_translation, "Display Translation")
                .on_hover_text("Show the Bible's short name, such as KJV, with the reference");
            ui.checkbox(&mut options.preserve_font_color, "Preserve Font Color")
                .on_hover_text("Keep the colours the Bible itself gives, such as red letters");
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
        let (swatch, response) = ui.allocate_exact_size(vec2(66.0, 37.0), Sense::hover());
        // No theme chosen: the chequered pattern that stands for "nothing".
        ui.painter().rect_filled(swatch, 2.0, Color32::from_gray(18));
        for row in 0..4 {
            for col in 0..7 {
                if (row + col) % 2 == 0 {
                    let cell =
                        Rect::from_min_size(swatch.min + vec2(col as f32 * 9.5, row as f32 * 9.25), vec2(9.5, 9.25));
                    ui.painter().rect_filled(cell.intersect(swatch), 0.0, Color32::from_gray(34));
                }
            }
        }
        response.on_hover_text(not_built("Choosing a theme for Bible slides"));
        ui.end_row();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(egui::RichText::new("Import Library").strong())
        });
        egui::ComboBox::from_id_salt("bible import library").selected_text(library).width(120.0).show_ui(ui, |ui| {
            ui.label(library);
        });
        ui.end_row();
    });
}

fn bibles(
    ui: &mut egui::Ui,
    options: &mut BibleOptions,
    shelf: &mut Shelf,
    downloads: &mut crate::bible_store::Downloads,
) {
    // Installed | Free
    let width = ui.available_width();
    let (bar, _) = ui.allocate_exact_size(vec2(width, 22.0), Sense::hover());
    ui.painter().rect_filled(bar, 6.0, Color32::from_gray(70));
    for (index, name) in ["Installed", "Free"].into_iter().enumerate() {
        let cell = Rect::from_min_size(
            pos2(bar.min.x + width / 2.0 * index as f32, bar.min.y),
            vec2(width / 2.0, bar.height()),
        );
        if options.bibles_tab == index {
            ui.painter().rect_filled(cell, 6.0, BLUE);
        }
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.5), INK);
        if ui.interact(cell, ui.scope_id().with(("bibles tab", index)), Sense::click()).clicked() {
            options.bibles_tab = index;
        }
    }
    // Downloads that have just ended: put the Bible on the shelf, or say what went wrong.
    for (_, result) in downloads.finished() {
        match result {
            Ok((path, info)) => {
                options.problem = None;
                shelf.add_file(path, info);
            }
            Err(problem) => options.problem = Some(problem),
        }
    }
    let can_download = crate::bible_store::download_base().is_some();
    egui::ScrollArea::vertical().id_salt("bibles list").auto_shrink([false, false]).max_height(196.0).show(ui, |ui| {
        if let Some(problem) = &options.problem {
            ui.add_space(4.0);
            ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(Color32::from_rgb(255, 150, 120)));
        }
        if options.bibles_tab == 0 {
            ui.add_space(6.0);
            for bible in &shelf.bibles {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} ({})", bible.name, bible.short)).color(INK));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let note = if bible.shipped { "Comes with the app" } else { bible.terms.as_str() };
                        ui.label(egui::RichText::new(note).size(10.5).color(DIM));
                    });
                });
            }
            return;
        }
        if !can_download {
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Downloads start once PresenterCraft is published. Until then, drop a Bible file (.pcbible) on the window to install it.")
                    .size(11.5)
                    .color(DIM),
            );
        }
        let mut language = "";
        for bible in OFFERED {
            if bible.language != language {
                language = bible.language;
                ui.add_space(4.0);
                ui.label(egui::RichText::new(language).size(11.5).strong().color(DIM));
                ui.separator();
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} ({})", bible.name, bible.short)).color(INK));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if shelf.position(bible.id).is_some() {
                        ui.label(egui::RichText::new("Installed").size(12.0).color(Color32::from_rgb(83, 194, 43)));
                    } else if downloads.is_busy(bible.id) {
                        ui.label(egui::RichText::new("Downloading…").size(12.0).color(DIM));
                    } else if ui
                        .add_enabled(can_download, egui::Button::new("Install"))
                        .on_disabled_hover_text("Downloads start once PresenterCraft is published")
                        .clicked()
                    {
                        let ctx = ui.ctx().clone();
                        downloads.start(bible.id, bible.checksum, move || ctx.request_repaint());
                    }
                    ui.label(egui::RichText::new(bible.terms).size(10.5).color(DIM));
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(shelf: &mut Shelf, ids: &[(&str, BibleTarget, BibleTarget)]) -> Vec<RowChoice> {
        ids.iter()
            .map(|(id, verse, reference)| RowChoice {
                bible: shelf.get(shelf.position(id).unwrap()).unwrap(),
                verse: *verse,
                reference: *reference,
            })
            .collect()
    }

    fn texts(slide: &Slide) -> Vec<&str> {
        slide.objects.iter().filter_map(Object::as_text).collect()
    }

    #[test]
    fn a_reference_becomes_one_slide_for_each_verse_in_both_languages() {
        let mut shelf = Shelf::shipped();
        let rows = rows(
            &mut shelf,
            &[("kjv", BibleTarget::Main, BibleTarget::Main), ("syn", BibleTarget::Translation, BibleTarget::None)],
        );
        let passage = build_passage("John 3:16-17", &rows, &BibleOptions::default()).unwrap();
        assert_eq!(passage.title, "John 3:16-17");
        assert_eq!(passage.slide_count(), 2);
        let (_, first) = passage.slide(0).unwrap();
        let shown = texts(first);
        assert!(shown[0].starts_with("For God so loved the world"));
        assert!(shown[1].starts_with("Ибо так возлюбил Бог мир"));
        assert_eq!(shown[2], "John 3:16");
    }

    #[test]
    fn the_russian_psalm_is_the_one_that_matches_the_english() {
        let mut shelf = Shelf::shipped();
        let english_first = rows(
            &mut shelf,
            &[
                ("kjv", BibleTarget::Main, BibleTarget::Main),
                ("syn", BibleTarget::Translation, BibleTarget::Translation),
            ],
        );
        let passage = build_passage("Psalm 23:1", &english_first, &BibleOptions::default()).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("The LORD is my shepherd") && shown.contains("Пастырь мой"), "{shown}");
        assert!(shown.contains("Psalms 23:1  ·  Псалтирь 22:1"), "{shown}");
        // Typed the Russian way with the Russian Bible first, it is the same psalm.
        let russian_first = rows(
            &mut shelf,
            &[("syn", BibleTarget::Main, BibleTarget::None), ("kjv", BibleTarget::Translation, BibleTarget::None)],
        );
        let passage = build_passage("Пс 22:1", &russian_first, &BibleOptions::default()).unwrap();
        let shown = texts(passage.slide(0).unwrap().1).join(" | ");
        assert!(shown.contains("Пастырь мой") && shown.contains("The LORD is my shepherd"), "{shown}");
    }

    #[test]
    fn the_options_change_the_slides() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("web", BibleTarget::Main, BibleTarget::Main)]);
        let options = BibleOptions {
            break_on_new_verse: false,
            show_verse_numbers: true,
            display_translation: true,
            ..BibleOptions::default()
        };
        let passage = build_passage("jn 3:16-17", &rows, &options).unwrap();
        assert_eq!(passage.slide_count(), 1);
        let shown = texts(passage.slide(0).unwrap().1);
        assert!(shown[0].starts_with("16 For God so loved") && shown[0].contains(" 17 For God"), "{}", shown[0]);
        assert_eq!(shown[1], "John 3:16-17 (WEB)");

        let last_only = BibleOptions { reference: ReferenceStyle::PassageLast, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16-17", &rows, &last_only).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
        assert_eq!(texts(passage.slide(1).unwrap().1)[1], "John 3:16-17");

        let none = BibleOptions { reference: ReferenceStyle::NoReference, ..BibleOptions::default() };
        let passage = build_passage("jn 3:16", &rows, &none).unwrap();
        assert_eq!(texts(passage.slide(0).unwrap().1).len(), 1);
    }

    #[test]
    fn a_reference_that_names_nothing_says_so() {
        let mut shelf = Shelf::shipped();
        let rows = rows(&mut shelf, &[("kjv", BibleTarget::Main, BibleTarget::Main)]);
        assert!(build_passage("John 99", &rows, &BibleOptions::default()).unwrap_err().contains("no such verses"));
        assert!(build_passage("Nobody 1:1", &rows, &BibleOptions::default()).unwrap_err().contains("No book"));
        assert!(build_passage("John 3:16", &[], &BibleOptions::default()).is_err());
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
