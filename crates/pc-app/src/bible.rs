//! The Bible page's Options pop-up: how verses become slides, and which Bibles are installed.
//! Looking verses up is not built yet, so the choices here are remembered but not yet used.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, pos2, vec2};

use crate::home::not_built;

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

/// A Bible that can be offered for download. Every entry must be free to pass on, including
/// in an app that is sold: `docs/licences/bibles.md` says where each one's terms were read and
/// what they ask. Do not add a Bible here without adding it there.
pub struct Offered {
    pub language: &'static str,
    pub name: &'static str,
    pub short: &'static str,
    /// The terms it is offered under, in a few words.
    pub terms: &'static str,
}

/// The Bibles offered under "Free", in language order.
pub const OFFERED: &[Offered] = &[
    Offered { language: "English", name: "King James Version", short: "KJV", terms: "Public domain" },
    Offered { language: "English", name: "American Standard Version", short: "ASV", terms: "Public domain" },
    Offered { language: "English", name: "World English Bible", short: "WEB", terms: "Public domain" },
    Offered { language: "English", name: "Berean Standard Bible", short: "BSB", terms: "Public domain" },
    Offered { language: "English", name: "Young's Literal Translation", short: "YLT", terms: "Public domain" },
    Offered { language: "English", name: "Darby Translation", short: "DBY", terms: "Public domain" },
    Offered { language: "English", name: "Douay-Rheims, American edition", short: "DRA", terms: "Public domain" },
    Offered { language: "English", name: "Geneva Bible 1599", short: "GNV", terms: "Public domain" },
    Offered { language: "French", name: "Louis Segond 1910", short: "LSG", terms: "Public domain" },
    Offered { language: "French", name: "Bible Darby", short: "FRDBY", terms: "Public domain" },
    Offered { language: "German", name: "Luther 1912", short: "LUT1912", terms: "Public domain" },
    Offered { language: "German", name: "Elberfelder 1905", short: "ELB1905", terms: "Public domain" },
    Offered { language: "Greek", name: "SBL Greek New Testament", short: "SBLGNT", terms: "CC BY 4.0" },
    Offered { language: "Greek", name: "Byzantine Text, Robinson-Pierpont", short: "RP", terms: "Public domain" },
    Offered { language: "Greek", name: "Septuagint, Brenton", short: "LXX", terms: "Public domain" },
    Offered { language: "Hebrew", name: "Westminster Leningrad Codex", short: "WLC", terms: "Public domain" },
    Offered { language: "Portuguese", name: "Bíblia Livre", short: "BLIVRE", terms: "CC BY 4.0" },
    Offered {
        language: "Russian", name: "Синодальный перевод", short: "SYN", terms: "Public domain"
    },
    Offered { language: "Russian", name: "Russian Open Bible", short: "ROB", terms: "CC BY-SA 4.0" },
    Offered { language: "Spanish", name: "Reina-Valera 1909", short: "RV1909", terms: "Public domain" },
    Offered { language: "Spanish", name: "Versión Biblia Libre", short: "VBL", terms: "CC BY-SA 4.0" },
    Offered {
        language: "Ukrainian", name: "Переклад Куліша та Пулюя", short: "UKR1905", terms: "Public domain"
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
        }
    }
}

/// Draws the pop-up under the Options button, its right edge in line with the button's.
pub fn options_popup(ctx: &egui::Context, options: &mut BibleOptions, button: Rect, library: &str) {
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
                                    bibles(ui, options);
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

fn bibles(ui: &mut egui::Ui, options: &mut BibleOptions) {
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
    egui::ScrollArea::vertical().id_salt("bibles list").auto_shrink([false, false]).max_height(196.0).show(ui, |ui| {
        if options.bibles_tab == 0 {
            ui.add_space(8.0);
            ui.label(egui::RichText::new("No Bibles are installed yet.").color(DIM));
            ui.label(egui::RichText::new("The ones under Free can be passed on freely.").size(11.5).color(DIM));
            return;
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
                    ui.add_enabled(false, egui::Button::new("Install"))
                        .on_disabled_hover_text(not_built("Downloading Bibles"));
                    ui.label(egui::RichText::new(bible.terms).size(10.5).color(DIM));
                });
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offered_bibles_are_grouped_by_language_and_named_once() {
        let mut seen: Vec<&str> = Vec::new();
        let mut languages: Vec<&str> = Vec::new();
        for bible in OFFERED {
            assert!(!seen.contains(&bible.short), "{} is listed twice", bible.short);
            seen.push(bible.short);
            if languages.last() != Some(&bible.language) {
                assert!(!languages.contains(&bible.language), "{} is split into two groups", bible.language);
                languages.push(bible.language);
            }
        }
    }
}
