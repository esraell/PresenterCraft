//! The look of the app and the parts of the main window drawn in it: the top bar, the song
//! list, the Program panel and the media strip.
//!
//! The look is a studio one: near-black surfaces in three tones with almost no lines, one
//! colour for "selected" and one, tally red, kept only for what is on screen right now.

use std::sync::Arc;

use egui::{
    Align, Align2, Color32, CornerRadius, FontFamily, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind,
    TextureId, pos2, vec2,
};
use pc_engine::{Command, Layer, Show, SlideRef};

use crate::editor::EditDraft;
use crate::home::{HomeState, SlideView, ThumbOf, matches_filter};
use crate::icons::{self, Icon};

/// The window behind everything.
pub const BG: Color32 = Color32::from_rgb(14, 15, 17);
/// Side panels and bars.
pub const PANEL: Color32 = Color32::from_rgb(20, 22, 26);
/// Things raised off a panel: buttons, cards, fields.
pub const CARD: Color32 = Color32::from_rgb(30, 33, 37);
pub const HOVER: Color32 = Color32::from_rgb(40, 44, 50);
/// The rare line between two areas.
pub const LINE: Color32 = Color32::from_rgb(38, 42, 47);
pub const TEXT: Color32 = Color32::from_rgb(232, 234, 237);
pub const TEXT_DIM: Color32 = Color32::from_rgb(154, 160, 168);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(104, 110, 118);
/// Selected, in focus, chosen.
pub const ACCENT: Color32 = Color32::from_rgb(76, 141, 255);
/// On screen right now. Used for nothing else.
pub const TALLY: Color32 = Color32::from_rgb(255, 69, 58);
/// The output is on and ready.
pub const READY: Color32 = Color32::from_rgb(50, 208, 116);
/// The corner radius of buttons, fields and cards.
pub const RADIUS: f32 = 8.0;

/// Sets the whole window's colours, sizes and font.
pub fn apply_style(ctx: &egui::Context) {
    load_fonts(ctx);
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = BG;
    visuals.window_fill = PANEL;
    visuals.window_stroke = Stroke::new(1.0, LINE);
    visuals.window_corner_radius = CornerRadius::same(12);
    visuals.menu_corner_radius = CornerRadius::same(10);
    visuals.extreme_bg_color = BG;
    visuals.faint_bg_color = CARD;
    visuals.override_text_color = Some(TEXT);
    visuals.selection.bg_fill = ACCENT.gamma_multiply(0.55);
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.hyperlink_color = ACCENT;
    let radius = CornerRadius::same(RADIUS as u8);
    for (widget, fill) in [
        (&mut visuals.widgets.noninteractive, PANEL),
        (&mut visuals.widgets.inactive, CARD),
        (&mut visuals.widgets.hovered, HOVER),
        (&mut visuals.widgets.active, HOVER),
        (&mut visuals.widgets.open, HOVER),
    ] {
        widget.corner_radius = radius;
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::NONE;
        widget.fg_stroke = Stroke::new(1.0, TEXT);
    }
    visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, LINE);
    visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT_DIM);
    ctx.set_visuals(visuals);
    ctx.all_styles_mut(|style| {
        use egui::TextStyle::{Body, Button, Heading, Monospace, Small};
        style.text_styles.insert(Body, FontId::new(13.5, FontFamily::Proportional));
        style.text_styles.insert(Button, FontId::new(13.5, FontFamily::Proportional));
        style.text_styles.insert(Small, FontId::new(11.5, FontFamily::Proportional));
        style.text_styles.insert(Heading, FontId::new(18.0, FontFamily::Proportional));
        style.text_styles.insert(Monospace, FontId::new(12.5, FontFamily::Monospace));
        style.spacing.button_padding = vec2(10.0, 5.0);
        style.spacing.item_spacing = vec2(8.0, 6.0);
        style.spacing.interact_size.y = 26.0;
    });
}

/// The name of the heavier cut of the interface font, for headings and buttons.
const SEMIBOLD: &str = "semibold";

/// The heavier cut of the interface font at a size.
pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

/// Uses the computer's own interface font, so the app looks at home on it, with a
/// wide-coverage font behind it so song titles in any script show as letters. Without either
/// the app still works with its built-in font.
fn load_fonts(ctx: &egui::Context) {
    use egui::epaint::text::VariationCoords;
    // Each is the system's interface font where it exists, then a common one, with the file
    // that holds its heavier cut.
    const INTERFACE: [(&str, &str); 4] = [
        ("/System/Library/Fonts/SFNS.ttf", "/System/Library/Fonts/SFNS.ttf"),
        ("/System/Library/Fonts/HelveticaNeue.ttc", "/System/Library/Fonts/HelveticaNeue.ttc"),
        ("C:\\Windows\\Fonts\\segoeui.ttf", "C:\\Windows\\Fonts\\seguisb.ttf"),
        ("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf"),
    ];
    const WIDE: [&str; 2] =
        ["/System/Library/Fonts/Supplemental/Arial Unicode.ttf", "C:\\Windows\\Fonts\\arialuni.ttf"];
    // Apple's system font is one file that can be drawn at any weight, and in a cut made for
    // small sizes or one made for headlines. Left alone it uses the headline cut, which looks
    // cramped and thin at the size of a label; ask for the small-text cut, a touch heavier
    // because light text on dark looks thinner than it is. Fonts without these settings
    // ignore them.
    let cut = |weight: f32| egui::FontTweak {
        coords: VariationCoords::new([(b"opsz", 17.0), (b"wght", weight)]),
        ..egui::FontTweak::default()
    };
    let mut fonts = egui::FontDefinitions::default();
    let mut regular_first = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mut heavy_first = regular_first.clone();
    let found =
        INTERFACE.iter().find_map(|(regular, heavy)| Some((std::fs::read(regular).ok()?, std::fs::read(heavy).ok())));
    if let Some((regular, heavy)) = found {
        let heavy = heavy.unwrap_or_else(|| regular.clone());
        fonts.font_data.insert("interface".to_owned(), Arc::new(egui::FontData::from_owned(regular).tweak(cut(440.0))));
        fonts
            .font_data
            .insert("interface heavy".to_owned(), Arc::new(egui::FontData::from_owned(heavy).tweak(cut(620.0))));
        regular_first.insert(0, "interface".to_owned());
        heavy_first.insert(0, "interface heavy".to_owned());
    }
    fonts.families.insert(FontFamily::Proportional, regular_first);
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), heavy_first);
    if let Some(bytes) = WIDE.iter().find_map(|path| std::fs::read(path).ok()) {
        fonts.font_data.insert("wide".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        for family in fonts.families.values_mut() {
            family.push("wide".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

/// A field for renaming something in place. It keeps the look of the text it stands in for
/// (same font, same colour, no filled box) and adds only a thin outline, so the words stay
/// where they were and it is plain that they can now be typed over.
fn rename_field<'t>(text: &'t mut String, font: FontId, ink: Color32, outline: Color32) -> egui::TextEdit<'t> {
    egui::TextEdit::singleline(text)
        .font(font)
        .text_color(ink)
        .background_color(Color32::TRANSPARENT)
        .min_size(egui::Vec2::ZERO)
        .frame(
            Frame::new().stroke(Stroke::new(1.0, outline)).corner_radius(4).inner_margin(egui::Margin::symmetric(4, 0)),
        )
}

/// Puts a field for retyping a name exactly over the name, so nothing around it moves. `start`
/// is where the name's text begins, at the middle of its line; the field's text begins there
/// too, the field is one line of the font tall, and it reaches `right`. It takes no room in
/// the layout.
pub fn rename_field_at(
    ui: &mut egui::Ui,
    text: &mut String,
    font: FontId,
    ink: Color32,
    outline: Color32,
    start: egui::Pos2,
    right: f32,
) -> egui::Response {
    let height = ui.fonts_mut(|fonts| fonts.row_height(&font)) + 2.0;
    // The field's frame is a line of 1 and a margin of 4, so its text lines up with the name's.
    let rect = Rect::from_min_max(
        egui::pos2(start.x - 5.0, start.y - height / 2.0),
        egui::pos2(right.max(start.x + 40.0), start.y + height / 2.0),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(rect).layout(Layout::centered_and_justified(egui::Direction::TopDown)),
    );
    // The theme gives fields a least height taller than a line of text; not here.
    child.spacing_mut().interact_size = egui::Vec2::ZERO;
    child.add(rename_field(text, font, ink, outline))
}

/// A small heading in capitals, as each section starts with.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(10.5)).color(TEXT_FAINT).extra_letter_spacing(0.8));
}

/// A button drawn as a rounded block with its words in the middle. `emphasis` picks its
/// colours: plain, or live (tally red).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Plain,
    Live,
    /// Nothing to do: drawn faint, and not clickable.
    Idle,
}

pub fn block_button(ui: &mut egui::Ui, size: egui::Vec2, text: &str, key: &str, emphasis: Emphasis) -> egui::Response {
    let sense = if emphasis == Emphasis::Idle { Sense::hover() } else { Sense::click() };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = response.hovered() && emphasis != Emphasis::Idle;
    let (fill, ink) = match emphasis {
        Emphasis::Plain => (if hovered { HOVER } else { CARD }, TEXT),
        Emphasis::Live => (if hovered { TALLY } else { TALLY.gamma_multiply(0.88) }, Color32::WHITE),
        Emphasis::Idle => (CARD.gamma_multiply(0.7), TEXT_FAINT),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, RADIUS, fill);
    painter.text(rect.center(), Align2::CENTER_CENTER, text, semibold(13.5), ink);
    if !key.is_empty() {
        // The key that does the same thing, small at the right.
        painter.text(
            rect.right_center() - vec2(10.0, 0.0),
            Align2::RIGHT_CENTER,
            key,
            FontId::proportional(10.5),
            ink.gamma_multiply(0.7),
        );
    }
    response
}

/// Several choices in one rounded bar, the chosen one raised. Returns the one clicked.
pub fn segmented(
    ui: &mut egui::Ui,
    salt: &str,
    each: egui::Vec2,
    labels: &[&str],
    chosen: Option<usize>,
) -> Option<usize> {
    let (bar, _) = ui.allocate_exact_size(vec2(each.x * labels.len() as f32 + 6.0, each.y + 6.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 2.0, BG);
    let mut clicked = None;
    for (index, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(bar.min + vec2(3.0 + each.x * index as f32, 3.0), each);
        let response = ui.interact(cell, ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        if on {
            ui.painter().rect_filled(cell, RADIUS, CARD);
        } else if response.hovered() {
            ui.painter().rect_filled(cell, RADIUS, PANEL);
        }
        let ink = if on { TEXT } else { TEXT_DIM };
        // Many choices share the width, so their words are smaller.
        let size = match labels.len() {
            0..=4 => 13.5,
            5 => 12.5,
            _ => 11.5,
        };
        let font = if on { semibold(size) } else { FontId::proportional(size) };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *label, font, ink);
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// The bar along the top: the three views, themes and more on the left; the audience output
/// on the right.
pub fn toolbar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(14, 9));
    egui::Panel::top("toolbar").resizable(false).frame(frame).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            // The four views. Themes is one of them: it opens the Theme Editor.
            let editing_theme = state.theme_edit.is_some();
            let view = if editing_theme {
                3
            } else if state.editing.is_some() {
                1
            } else if state.bible_open {
                2
            } else {
                0
            };
            let picked = segmented(ui, "views", vec2(74.0, 28.0), &["Show", "Edit", "Bible", "Themes"], Some(view))
                .or(state.view_request.take().filter(|asked| *asked != view));
            match picked {
                Some(3) if !editing_theme => {
                    state.theme_request = Some(crate::home::ThemeRequest::Edit(state.theme_for_songs))
                }
                Some(3) => {}
                // Leaving the Theme Editor goes through the app, which saves the theme.
                Some(to) if editing_theme => {
                    if let Some(edit) = state.theme_edit.as_mut() {
                        edit.leave = true;
                    }
                    state.bible_open = to == 2;
                    if to == 1 {
                        state.open_editor_next = true;
                    }
                }
                Some(0) => {
                    state.editing = None;
                    state.bible_open = false;
                }
                Some(2) => {
                    state.editing = None;
                    state.bible_open = true;
                }
                // Edit opens the slide that was last on screen, or the song's first slide.
                Some(1) if state.editing.is_none() => {
                    let at = show.cursor().unwrap_or(SlideRef { presentation: show.selected(), slide: 0 });
                    state.editing = EditDraft::of(show, at);
                }
                _ => {}
            }
            // Asked for while the Theme Editor was open: it has closed now, so open the slide editor.
            if state.open_editor_next && state.theme_edit.is_none() {
                state.open_editor_next = false;
                let at = show.cursor().unwrap_or(SlideRef { presentation: show.selected(), slide: 0 });
                state.editing = EditDraft::of(show, at);
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // Settings, at the far right.
                let (gear, response) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                let on = state.preferences_open;
                ui.painter().rect_filled(gear, RADIUS, if on || response.hovered() { HOVER } else { CARD });
                icons::paint(
                    ui.painter(),
                    Icon::Gear,
                    Rect::from_center_size(gear.center(), vec2(15.0, 15.0)),
                    TEXT_DIM,
                );
                if response.on_hover_text("Settings (Command-comma)").clicked() {
                    state.preferences_open = !state.preferences_open;
                }
                // The audience output: a tally light, its name, and whether it is on.
                let on = show.output_enabled();
                let (rect, response) = ui.allocate_exact_size(vec2(148.0, 30.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect, RADIUS, if response.hovered() { HOVER } else { CARD });
                let light = if on { READY } else { TEXT_FAINT };
                painter.circle_filled(rect.left_center() + vec2(15.0, 0.0), 4.5, light);
                painter.text(
                    rect.left_center() + vec2(28.0, 0.0),
                    Align2::LEFT_CENTER,
                    "Audience",
                    FontId::proportional(13.5),
                    TEXT,
                );
                painter.text(
                    rect.right_center() - vec2(12.0, 0.0),
                    Align2::RIGHT_CENTER,
                    if on { "ON" } else { "OFF" },
                    FontId::proportional(11.0),
                    if on { READY } else { TEXT_DIM },
                );
                let hint = if on {
                    "The audience output is on. Click to switch it off."
                } else {
                    "The audience output is off. Click to switch it on."
                };
                if response.on_hover_text(hint).clicked() {
                    commands.push(Command::SetOutputEnabled(!on));
                }
            });
        });
    });
}

/// One row of a list in the left column. Returns its response; `live` puts a tally light at
/// the right, and `trailing` is room kept clear there for a button.
fn list_row(ui: &mut egui::Ui, icon: Icon, title: &str, chosen: bool, live: bool, missing: bool) -> egui::Response {
    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click_and_drag());
    let painter = ui.painter();
    if chosen {
        painter.rect_filled(row, RADIUS, ACCENT.gamma_multiply(0.22));
    } else if response.hovered() {
        painter.rect_filled(row, RADIUS, CARD);
    }
    let ink = if missing {
        TEXT_FAINT
    } else if chosen {
        Color32::WHITE
    } else {
        TEXT
    };
    icons::paint(
        painter,
        icon,
        Rect::from_center_size(row.left_center() + vec2(15.0, 0.0), vec2(13.0, 13.0)),
        TEXT_FAINT,
    );
    if live {
        painter.circle_filled(row.right_center() - vec2(12.0, 0.0), 3.5, TALLY);
    }
    let clip = Rect::from_min_max(row.min, pos2(row.max.x - 26.0, row.max.y));
    painter.with_clip_rect(clip).text(
        row.left_center() + vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        title,
        FontId::proportional(13.5),
        ink,
    );
    response
}

/// The left column: the library of every song above, the service (today's running order) below.
pub fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 8 });
    egui::Panel::left("library").resizable(false).exact_size(256.0).frame(frame).show(ui, |ui| {
        let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
        let removed = state.removed_songs.clone();
        let listed = |index: usize| !hidden.contains(&index) && !removed.contains(&index);
        // A song in a service is found by the name its file gave it; one saved before it had a
        // file is found by its title.
        let song_at = |keys: &[String], key: &str| {
            let by_title = key.strip_prefix("title:");
            keys.iter()
                .position(|have| have == key)
                .or_else(|| by_title.and_then(|title| show.presentations().iter().position(|song| song.title == title)))
                .filter(|index| listed(*index))
        };

        // The library is on top, as operators are used to. The service has its own strip fixed
        // to the bottom of the column, so however many songs there are it is never pushed out
        // of sight; it takes the room its items need, up to two fifths of the column.
        let items = state.services.front().items.clone();
        let column_height = ui.available_height();
        // To begin with the service takes the room its items need; after that it is as tall
        // as the line between the two was last dragged to make it.
        let fitted = (items.len().max(1) as f32 * 32.0 + 4.0).min(column_height * 0.4) + 96.0;
        let mut add = None;
        let live_in = show.live().map(|at| at.presentation);
        // The line between the library and the service can be dragged up and down.
        let strip = egui::Panel::bottom("service strip")
            .resizable(true)
            .size_range(130.0..=(column_height - 150.0).max(130.0))
            .default_size(state.service_height.unwrap_or(fitted.max(220.0)))
            .frame(Frame::new());
        let strip_shown = strip.show(ui, |ui| {
            // The strip's own top edge is the line between the library and the service.
            ui.add_space(12.0);
            // ---- The service.
            ui.horizontal(|ui| {
                caption(ui, "SERVICE");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // + adds a header, a heading that groups the items under it.
                    let add = ui.add(egui::Button::new(egui::RichText::new("+").color(TEXT_FAINT)).frame(false));
                    if add.on_hover_text("New Header").clicked() {
                        state.services.add(Item::Header { name: "Header".to_owned() });
                        let at = state.services.front().items.len() - 1;
                        state.renaming_header = Some((at, "Header".to_owned()));
                        state.rename_started = true;
                    }
                    ui.label(egui::RichText::new(state.services.front().items.len().to_string()).size(11.0).color(TEXT_FAINT));
                });
            });
            ui.add_space(2.0);
            if let Some(name) = state.renaming_service.as_mut() {
                // The field sits where the service menu was, in a space of the menu's height.
                let (place, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ui.spacing().interact_size.y), Sense::hover());
                let field = rename_field_at(
                    ui,
                    name,
                    FontId::proportional(13.5),
                    TEXT,
                    ACCENT,
                    place.left_center() + vec2(8.0, 0.0),
                    place.max.x,
                );
                field.request_focus();
                let done = ui.input(|input| input.key_pressed(egui::Key::Enter) || input.key_pressed(egui::Key::Escape));
                if done || field.lost_focus() {
                    if !ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        state.services.rename(name);
                    }
                    state.renaming_service = None;
                }
            } else {
                let name = state.services.front().name.clone();
                egui::ComboBox::from_id_salt("service picker").selected_text(name.as_str()).width(ui.available_width()).show_ui(
                    ui,
                    |ui| {
                        let names: Vec<String> = state.services.list.iter().map(|service| service.name.clone()).collect();
                        for (index, other) in names.iter().enumerate() {
                            if ui.selectable_label(index == state.services.current, other.as_str()).clicked() {
                                state.services.show(index);
                            }
                        }
                        ui.separator();
                        if ui.button("New Service").clicked() {
                            state.services.add_service();
                            state.renaming_service = Some(state.services.front().name.clone());
                        }
                        if ui.button("Rename…").clicked() {
                            state.renaming_service = Some(name.clone());
                        }
                        if ui.add_enabled(names.len() > 1, egui::Button::new("Delete This Service")).clicked() {
                            state.services.remove_service();
                        }
                    },
                );
            }
            ui.add_space(6.0);

            // What a click on a service row asks for, carried out after the list is drawn.
            enum Ask {
                Open(usize),
                Earlier(usize),
                Later(usize),
                Remove(usize),
                Rename(usize),
            }
            let mut ask = None;
            // Rows can be dragged up and down to change the running order.
            let mut rows: Vec<Rect> = Vec::new();
            let mut dragging = None;
            let mut dropped = None;
            let mut renamed: Option<Option<(usize, String)>> = None;
            let mut rename_started = std::mem::take(&mut state.rename_started);
            egui::ScrollArea::vertical().id_salt("service list").auto_shrink([false, false]).show(
                ui,
                |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if items.is_empty() {
                        ui.label(
                            egui::RichText::new("Nothing yet. Add songs with + in the library above, and Bible passages with Save to Service.")
                                .size(12.0)
                                .color(TEXT_FAINT),
                        );
                    }
                    for (index, item) in items.iter().enumerate() {
                        // A header: bold, full width, and renamed in place.
                        if let Item::Header { name } = item {
                            let (row, response) =
                                ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
                            rows.push(row);
                            match state.renaming_header.as_mut().filter(|(at, _)| *at == index) {
                                Some((_, typed)) => {
                                    let field = rename_field_at(
                                        ui,
                                        typed,
                                        semibold(13.0),
                                        TEXT,
                                        ACCENT,
                                        row.left_center() + vec2(6.0, 0.0),
                                        row.max.x - 4.0,
                                    );
                                    if std::mem::take(&mut rename_started) {
                                        field.request_focus();
                                    }
                                    let (enter, escape) = ui.input(|input| {
                                        (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape))
                                    });
                                    if escape {
                                        renamed = Some(None);
                                    } else if enter || field.lost_focus() {
                                        renamed = Some(Some((index, typed.clone())));
                                    }
                                }
                                None => {
                                    ui.painter().text(
                                        row.left_center() + vec2(6.0, 0.0),
                                        Align2::LEFT_CENTER,
                                        name.as_str(),
                                        semibold(13.0),
                                        TEXT,
                                    );
                                    ui.painter().hline(
                                        row.x_range(),
                                        row.max.y - 1.0,
                                        Stroke::new(1.0, LINE),
                                    );
                                }
                            }
                            if response.dragged() {
                                dragging = Some(index);
                            }
                            if response.drag_stopped() {
                                dropped = Some(index);
                            }
                            if response.double_clicked() {
                                ask = Some(Ask::Rename(index));
                            }
                            response.context_menu(|ui| {
                                if ui.button("Rename").clicked() {
                                    ask = Some(Ask::Rename(index));
                                    ui.close();
                                }
                                if ui.button("Remove from Service").clicked() {
                                    ask = Some(Ask::Remove(index));
                                    ui.close();
                                }
                            });
                            continue;
                        }
                        let (icon, found) = match item {
                            Item::Song { key, .. } => (Icon::Document, song_at(&state.song_keys, key)),
                            _ => (Icon::Book, None),
                        };
                        let missing = matches!(item, Item::Song { .. }) && found.is_none();
                        let live = found.is_some() && found == live_in;
                        let chosen = state.services.chosen == Some(index);
                        let mut response = list_row(ui, icon, item.title(), chosen, live, missing);
                        rows.push(response.rect);
                        if response.dragged() {
                            dragging = Some(index);
                        }
                        if response.drag_stopped() {
                            dropped = Some(index);
                        }
                        if missing {
                            response = response.on_hover_text("This song is not open in the library");
                        }
                        if response.clicked() {
                            ask = Some(Ask::Open(index));
                        }
                        response.context_menu(|ui| {
                            if ui.add_enabled(index > 0, egui::Button::new("Move Up")).clicked() {
                                ask = Some(Ask::Earlier(index));
                                ui.close();
                            }
                            if ui.add_enabled(index + 1 < items.len(), egui::Button::new("Move Down")).clicked() {
                                ask = Some(Ask::Later(index));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Remove from Service").clicked() {
                                ask = Some(Ask::Remove(index));
                                ui.close();
                            }
                        });
                    }
                },
            );
            // Where a dragged row would land: a line between rows, and on release the move.
            let pointer_y = ui.input(|input| input.pointer.interact_pos()).map(|at| at.y);
            let slot = pointer_y.map(|y| rows.iter().filter(|row| row.center().y < y).count());
            if let (Some(_), Some(slot)) = (dragging, slot) {
                let y = rows.get(slot).map_or_else(|| rows.last().map_or(0.0, |row| row.max.y), |row| row.min.y);
                let (left, right) = rows.first().map_or((0.0, 0.0), |row| (row.min.x, row.max.x));
                ui.painter().line_segment([pos2(left + 4.0, y), pos2(right - 4.0, y)], Stroke::new(2.0, ACCENT));
            }
            if let (Some(index), Some(slot)) = (dropped, slot) {
                state.services.move_item(index, slot);
            }
            state.rename_started = rename_started;
            match renamed {
                Some(Some((index, name))) => {
                    state.services.rename_header(index, &name);
                    state.renaming_header = None;
                }
                Some(None) => state.renaming_header = None,
                None => {}
            }
            match ask {
                Some(Ask::Rename(index)) => {
                    state.renaming_header = Some((index, items.get(index).map(|item| item.title().to_owned()).unwrap_or_default()));
                    state.rename_started = true;
                }
                Some(Ask::Open(index)) => {
                    state.services.chosen = Some(index);
                    match items.get(index) {
                        Some(Item::Song { key, .. }) => {
                            if let Some(at) = song_at(&state.song_keys, key) {
                                commands.push(Command::Select(at));
                                state.bible_open = false;
                            }
                        }
                        Some(Item::Passage(saved)) => {
                            state.bible_open = true;
                            state.bible.open_saved(saved, live_in);
                        }
                        Some(Item::Header { .. }) | None => {}
                    }
                }
                Some(Ask::Earlier(index)) => {
                    state.services.nudge(index, true);
                }
                Some(Ask::Later(index)) => {
                    state.services.nudge(index, false);
                }
                Some(Ask::Remove(index)) => {
                    state.services.remove(index);
                }
                None => {}
            }
        });
        state.service_height = Some(strip_shown.response.rect.height());

        // ---- The library: All Songs, or one library's songs.
        let library = state.library_shown.and_then(|id| state.libraries.iter().find(|library| library.id == id));
        if library.is_none() {
            state.library_shown = None;
        }
        let in_library = |index: usize| {
            library.is_none_or(|library| state.song_keys.get(index).is_some_and(|key| library.has(key)))
        };
        let total = (0..show.presentations().len()).filter(|index| listed(*index) && in_library(*index)).count();
        let shown: Vec<usize> = show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(index, song)| listed(*index) && in_library(*index) && matches_filter(&song.title, &state.filter))
            .map(|(index, _)| index)
            .collect();
        let library_name = library.map(|library| library.name.clone());
        ui.horizontal(|ui| {
            caption(ui, "LIBRARY");
            library_picker(ui, state, library_name.as_deref());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // The arrow folds the library away, leaving the service more room.
                let arrow = if state.library_collapsed { "⌄" } else { "⌃" };
                let hint = if state.library_collapsed { "Show the library" } else { "Fold the library away" };
                if ui.add(egui::Button::new(egui::RichText::new(arrow).color(TEXT_FAINT)).frame(false)).on_hover_text(hint).clicked() {
                    state.library_collapsed = !state.library_collapsed;
                }
                let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
                let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                if response.on_hover_text("New song (Command-N)").clicked() {
                    state.new_song = true;
                }
                let count = if shown.len() == total { total.to_string() } else { format!("{} of {total}", shown.len()) };
                ui.label(egui::RichText::new(count).size(11.0).color(TEXT_FAINT));
            });
        });
        if state.library_collapsed {
            return;
        }
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut state.filter)
                .hint_text("Search songs")
                .desired_width(f32::INFINITY)
                .margin(egui::Margin::symmetric(10, 6))
                .background_color(CARD),
        );
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            for index in shown.iter().copied() {
                let Some(song) = show.presentations().get(index) else { continue };
                let chosen = index == show.selected() && !state.bible_open;
                let response = list_row(ui, Icon::Document, song.title.as_str(), chosen, live_in == Some(index), false);
                // A "+" at the right of the row under the pointer puts the song in the service.
                let plus = Rect::from_center_size(response.rect.right_center() - vec2(13.0, 0.0), vec2(20.0, 20.0));
                if response.hovered() || ui.rect_contains_pointer(plus) {
                    let button = ui.interact(plus, ui.scope_id().with(("add to service", index)), Sense::click());
                    let ink = if button.hovered() { Color32::WHITE } else { TEXT_DIM };
                    ui.painter().rect_filled(plus, 5.0, if button.hovered() { ACCENT } else { HOVER });
                    icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                    if button.on_hover_text("Add to the service").clicked() {
                        add = Some(index);
                        continue;
                    }
                }
                if response.clicked() {
                    commands.push(Command::Select(index));
                    state.bible_open = false;
                    state.services.chosen = None;
                }
                response.context_menu(|ui| {
                    if ui.button("Add to Service").clicked() {
                        add = Some(index);
                        ui.close();
                    }
                    // The libraries the song is in.
                    if let Some(key) = state.song_keys.get(index).cloned() {
                        ui.menu_button("Libraries", |ui| {
                            if state.libraries.is_empty() {
                                ui.label(egui::RichText::new("No libraries yet. Make one from the LIBRARY menu.").color(TEXT_DIM));
                            }
                            for library in &mut state.libraries {
                                if ui.add(egui::Button::selectable(library.has(&key), library.name.as_str())).clicked() {
                                    library.toggle(&key);
                                    state.libraries_changed = true;
                                }
                            }
                        });
                    }
                    ui.separator();
                    if ui.button("Rename").clicked() {
                        commands.push(Command::Select(index));
                        state.bible_open = false;
                        state.editing = None;
                        state.renaming_song = show.presentations().get(index).map(|song| song.title.clone());
                        state.rename_started = true;
                        ui.close();
                    }
                    if ui.button("Duplicate").clicked() {
                        state.song_request = Some((index, crate::home::SongRequest::Duplicate));
                        ui.close();
                    }
                    // Next on the last slide goes back to the first.
                    let loops = show.presentations().get(index).is_some_and(|song| song.loops);
                    if ui
                        .add(egui::Button::selectable(loops, "Loop to First Slide"))
                        .on_hover_text("Next on the last slide goes back to the first, for a loop of announcements")
                        .clicked()
                    {
                        state.song_loop_request = Some(index);
                        ui.close();
                    }
                    // The theme this song is drawn with.
                    let own = show.presentations().get(index).and_then(|song| song.theme);
                    ui.menu_button("Theme", |ui| {
                        if ui.add(egui::Button::selectable(own.is_none(), "Songs Theme")).on_hover_text("The theme songs use").clicked() {
                            state.song_theme_request = Some((index, None));
                            ui.close();
                        }
                        ui.separator();
                        for (at, name) in state.theme_names.iter().enumerate() {
                            let on = own.is_some() && state.theme_ids.get(at).copied() == own;
                            if ui.add(egui::Button::selectable(on, name.as_str())).clicked() {
                                state.song_theme_request = Some((index, Some(at)));
                                ui.close();
                            }
                        }
                    });
                    if ui.button("Reflow").clicked()
                        && let Some(song) = show.presentations().get(index)
                    {
                        commands.push(Command::Select(index));
                        state.bible_open = false;
                        state.editing = None;
                        state.reflow = Some((index, pc_model::to_song_text(song)));
                        ui.close();
                    }
                    if ui.button("Delete…").clicked() {
                        state.confirm_delete_song = Some(index);
                        ui.close();
                    }
                });
            }
            if total == 0 {
                ui.label(egui::RichText::new("No songs loaded.").color(TEXT_DIM));
            } else if shown.is_empty() {
                ui.label(egui::RichText::new("No song matches.").color(TEXT_DIM));
            }
        });
        if let Some(index) = add
            && let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index))
        {
            state.services.add(Item::Song { key: key.clone(), title: song.title.clone() });
        }
    });
}

/// The right column: what the audience sees now, the buttons that take it away, stepping
/// through the slides, and what comes next.
/// The menu beside LIBRARY: All Songs or one library, a new library, and renaming or deleting
/// the one shown.
fn library_picker(ui: &mut egui::Ui, state: &mut HomeState, shown: Option<&str>) {
    let label = shown.map_or_else(|| "All Songs ▾".to_owned(), |name| format!("{name} ▾"));
    let picker = ui
        .add(egui::Label::new(egui::RichText::new(label).font(semibold(11.5)).color(TEXT_DIM)).sense(Sense::click()))
        .on_hover_text("Choose a library");
    egui::Popup::from_toggle_button_response(&picker)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(200.0);
            if ui.add(egui::Button::selectable(state.library_shown.is_none(), "All Songs")).clicked() {
                state.library_shown = None;
                ui.close();
            }
            for library in &state.libraries {
                let on = state.library_shown == Some(library.id);
                let name = if library.name.trim().is_empty() { "Untitled" } else { library.name.as_str() };
                if ui.add(egui::Button::selectable(on, name)).clicked() {
                    state.library_shown = Some(library.id);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("New Library").on_hover_text("Add songs to it from a song's right-click menu").clicked() {
                let made = crate::libraries::Library::new(crate::libraries::new_name(&state.libraries));
                state.library_shown = Some(made.id);
                state.libraries.push(made);
                state.libraries_changed = true;
            }
            if let Some(at) = state.library_shown.and_then(|id| state.libraries.iter().position(|l| l.id == id)) {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                    let field = ui.add(egui::TextEdit::singleline(&mut state.libraries[at].name).desired_width(140.0));
                    state.libraries_changed |= field.changed();
                });
                if ui.button("Delete Library").on_hover_text("The songs stay in All Songs").clicked() {
                    state.libraries.remove(at);
                    state.library_shown = None;
                    state.libraries_changed = true;
                    ui.close();
                }
            }
        });
}

pub fn program_panel(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    preview: crate::home::Preview,
    commands: &mut Vec<Command>,
) {
    let output_stopped = state.output_stopped;
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 16, right: 16, top: 14, bottom: 12 });
    egui::Panel::right("live").resizable(false).exact_size(340.0).frame(frame).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        let slide_live = show.live().is_some();
        let media_live = show.live_media().is_some();
        let any_live = show.anything_live();
        let on = show.output_enabled();
        let width = ui.available_width();

        // The heading carries the tally light: red while anything is on screen.
        ui.horizontal(|ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(10.0, 12.0), Sense::hover());
            ui.painter().circle_filled(dot.center(), 4.5, if any_live && on { TALLY } else { TEXT_FAINT });
            caption(ui, "PROGRAM");
            // Which screens the preview shows: the audience's, or the stage's.
            let shown = if state.preview_stage { "STAGE ▾" } else { "AUDIENCE ▾" };
            let chooser = ui
                .add(
                    egui::Label::new(
                        egui::RichText::new(shown).font(semibold(10.5)).color(TEXT_DIM).extra_letter_spacing(0.8),
                    )
                    .sense(Sense::click()),
                )
                .on_hover_text("Choose which screens the preview shows");
            egui::Popup::menu(&chooser).show(|ui| {
                for (stage, name) in [(false, "Audience"), (true, "Stage")] {
                    if ui.add(egui::Button::selectable(state.preview_stage == stage, name)).clicked() {
                        state.preview_stage = stage;
                        ui.close();
                    }
                }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (text, ink) = match (on, any_live) {
                    (false, _) => ("OUTPUT OFF", TALLY),
                    (true, true) => ("ON SCREEN", TALLY),
                    (true, false) => ("CLEAR", TEXT_FAINT),
                };
                ui.label(egui::RichText::new(text).font(semibold(10.5)).color(ink).extra_letter_spacing(0.8));
            });
        });

        let (picture, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).round()), Sense::hover());
        ui.painter().rect_filled(picture, RADIUS, Color32::BLACK);
        // While the output fades from one picture to the next, so does this.
        let fading = preview.amount < 1.0;
        if let Some(before) = preview.before.filter(|_| fading) {
            egui::Image::from_texture((before, picture.size())).corner_radius(RADIUS).paint_at(ui, picture);
        }
        if let Some(texture) = preview.now {
            let veil =
                if fading { Color32::WHITE.gamma_multiply(preview.amount.clamp(0.0, 1.0)) } else { Color32::WHITE };
            egui::Image::from_texture((texture, picture.size())).corner_radius(RADIUS).tint(veil).paint_at(ui, picture);
        }
        if fading {
            ui.ctx().request_repaint();
        }
        if !on && !state.preview_stage {
            ui.painter().rect_filled(picture, RADIUS, Color32::from_black_alpha(190));
            ui.painter().text(
                picture.center(),
                Align2::CENTER_CENTER,
                "Audience output is off",
                FontId::proportional(13.5),
                TEXT_DIM,
            );
        }
        let border = if any_live && on { Stroke::new(2.0, TALLY) } else { Stroke::new(1.0, LINE) };
        ui.painter().rect_stroke(picture, RADIUS, border, StrokeKind::Outside);
        if output_stopped {
            ui.label(
                egui::RichText::new("The output has stopped. Restart PresenterCraft to bring it back.").color(TALLY),
            );
        }
        ui.add_space(2.0);

        // Clear: the big one takes everything off; the two under it take one layer each.
        let clear_all = if any_live { Emphasis::Live } else { Emphasis::Idle };
        if block_button(ui, vec2(width, 40.0), "Clear All", "esc", clear_all)
            .on_hover_text("Clear All (Escape)")
            .clicked()
        {
            commands.push(Command::ClearAll);
        }
        ui.horizontal(|ui| {
            let half = (width - 8.0) / 2.0;
            let emphasis = |live: bool| if live { Emphasis::Plain } else { Emphasis::Idle };
            if block_button(ui, vec2(half, 30.0), "Clear Slide", "", emphasis(slide_live))
                .on_hover_text("Take the words off the screen")
                .clicked()
            {
                commands.push(Command::Clear(Layer::Slides));
            }
            if block_button(ui, vec2(half, 30.0), "Clear Media", "", emphasis(media_live))
                .on_hover_text("Take the picture off the screen")
                .clicked()
            {
                commands.push(Command::Clear(Layer::Media));
            }
        });
        ui.add_space(6.0);

        // Stepping through the song, with where we are between the buttons.
        let at = show.live().or(show.cursor());
        let song = at.and_then(|at| show.presentations().get(at.presentation));
        ui.horizontal(|ui| {
            let side = ((width - 16.0 - 64.0) / 2.0).floor();
            if block_button(ui, vec2(side, 36.0), "Previous", "", Emphasis::Plain)
                .on_hover_text("Show the slide before (Left arrow)")
                .clicked()
            {
                commands.push(Command::Previous);
            }
            let (middle, _) = ui.allocate_exact_size(vec2(64.0, 36.0), Sense::hover());
            // The place in the song's play order, which an arrangement can make longer.
            let place = match (show.position(), at, song) {
                (Some((step, length)), _, _) => format!("{} / {length}", step + 1),
                (None, Some(at), Some(song)) => format!("{} / {}", at.slide + 1, song.slide_count()),
                _ => "–".to_owned(),
            };
            ui.painter().text(middle.center(), Align2::CENTER_CENTER, place, FontId::monospace(12.5), TEXT_DIM);
            if block_button(ui, vec2(side, 36.0), "Next", "", Emphasis::Plain)
                .on_hover_text("Show the next slide (Right arrow or Space)")
                .clicked()
            {
                commands.push(Command::Next);
            }
        });
        ui.add_space(6.0);

        // What the next slide says, so the operator can read ahead.
        caption(ui, "NEXT");
        let next = show.upcoming().and_then(|up| Some((song?, song?.slide(up.slide)?)));
        Frame::new().fill(CARD).corner_radius(RADIUS).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
            ui.set_width(width - 24.0);
            match next {
                Some((_, (group, slide))) => {
                    if !group.name.is_empty() {
                        ui.label(egui::RichText::new(group.name.as_str()).size(11.0).color(TEXT_FAINT));
                    }
                    let words: Vec<String> = slide.texts().into_iter().map(|(_, text)| text).collect();
                    let words = words.first().cloned().unwrap_or_default();
                    ui.label(egui::RichText::new(words).size(13.0).color(TEXT_DIM));
                }
                None if at.is_some() => {
                    ui.label(egui::RichText::new("End of the song").size(13.0).color(TEXT_FAINT));
                }
                None => {
                    ui.label(egui::RichText::new("Click a slide to show it").size(13.0).color(TEXT_FAINT));
                }
            }
        });
        // Timers, messages and the stage, in tabs under the rest.
        crate::live_tabs::live_tabs(ui, show, state);
    });
}

/// The strip under the slides that holds pictures to show behind the words. It is slim until
/// it has something in it.
pub fn media_strip(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let empty = show.media().is_empty();
    let height = if empty && !state.drop_hover { 40.0 } else { 148.0 };
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(16, 10));
    egui::Panel::bottom("media bin").resizable(false).exact_size(height).frame(frame).show(ui, |ui| {
        let area = ui.max_rect().expand2(vec2(16.0, 10.0));
        ui.horizontal(|ui| {
            caption(ui, "MEDIA");
            let note = match &state.notice {
                Some(notice) => egui::RichText::new(notice.as_str()).size(12.0).color(Color32::from_rgb(255, 170, 120)),
                None if empty => {
                    egui::RichText::new("Drop pictures on the window to add backgrounds").size(12.0).color(TEXT_FAINT)
                }
                None => egui::RichText::new("Click a picture to put it behind the words").size(12.0).color(TEXT_FAINT),
            };
            ui.add_space(6.0);
            ui.label(note);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("+").on_hover_text("Add pictures from a folder").clicked() {
                    state.media_request = Some(crate::home::MediaRequest::Choose);
                }
            });
        });
        if !empty {
            ui.add_space(4.0);
            let image_size = vec2(144.0, 81.0);
            egui::ScrollArea::horizontal().id_salt("media row").auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    for (index, item) in show.media().iter().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(image_size + vec2(0.0, 18.0), Sense::click());
                        if response.clicked() {
                            commands.push(Command::ShowMedia(index));
                        }
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let image = Rect::from_min_size(rect.min, image_size);
                        ui.painter().rect_filled(image, 6.0, Color32::BLACK);
                        match thumb(ThumbOf::Media(index)) {
                            Some((texture, _)) => {
                                egui::Image::from_texture((texture, image_size)).corner_radius(6.0).paint_at(ui, image);
                            }
                            None => ui.ctx().request_repaint(),
                        }
                        let live = show.live_media() == Some(index);
                        if live {
                            ui.painter().rect_stroke(image, 6.0, Stroke::new(2.0, TALLY), StrokeKind::Outside);
                        } else if response.hovered() {
                            ui.painter().rect_stroke(image, 6.0, Stroke::new(1.0, TEXT_FAINT), StrokeKind::Outside);
                        }
                        let label = Rect::from_min_max(pos2(rect.min.x, image.max.y + 3.0), rect.max);
                        ui.painter().with_clip_rect(label).text(
                            label.left_center(),
                            Align2::LEFT_CENTER,
                            item.name.as_str(),
                            FontId::proportional(11.5),
                            TEXT_DIM,
                        );
                        response.on_hover_text(item.name.as_str()).context_menu(|ui| {
                            if ui.button("Remove").clicked() {
                                state.media_request = Some(crate::home::MediaRequest::Remove(index));
                                ui.close();
                            }
                        });
                    }
                });
            });
        }
        if state.drop_hover {
            let painter = ui.painter();
            painter.rect_filled(area, 0.0, ACCENT.gamma_multiply(0.18));
            painter.rect_stroke(area.shrink(4.0), RADIUS, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
            painter.text(
                area.center(),
                Align2::CENTER_CENTER,
                "Drop to add",
                FontId::proportional(15.0),
                Color32::WHITE,
            );
        }
    });
}

/// The right end of a slide area's heading: the three ways to lay slides out, and their size.
pub fn view_controls(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.spacing_mut().item_spacing.x = 8.0;
    // Further right is bigger, so fewer slides fit across.
    let mut size = 7 - state.columns.clamp(2, 7);
    ui.spacing_mut().slider_width = 96.0;
    if ui.add(egui::Slider::new(&mut size, 0..=5).show_value(false)).on_hover_text("Slide size").changed() {
        state.columns = 7 - size;
    }
    ui.add_space(4.0);
    let views = [
        (SlideView::Table, Icon::Rows, "Table View"),
        (SlideView::Easy, Icon::Document, "Easy View: words only"),
        (SlideView::Grid, Icon::Grid, "Grid View"),
    ];
    let (bar, _) = ui.allocate_exact_size(vec2(30.0 * 3.0 + 6.0, 30.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 1.0, PANEL);
    // Laid out right to left, so the first here is the rightmost.
    for (index, (view, icon, hint)) in views.into_iter().enumerate() {
        let cell =
            Rect::from_min_size(pos2(bar.max.x - 3.0 - 30.0 * (index as f32 + 1.0), bar.min.y + 3.0), vec2(30.0, 24.0));
        let response = ui.interact(cell, ui.scope_id().with(("slide view", index)), Sense::click());
        let on = state.slide_view == view;
        if on {
            ui.painter().rect_filled(cell, RADIUS - 2.0, CARD);
        }
        let ink = if on || response.hovered() { TEXT } else { TEXT_FAINT };
        icons::paint(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(14.0, 14.0)), ink);
        if response.on_hover_text(hint).clicked() {
            state.slide_view = view;
        }
    }
    ui.add_space(10.0);
    // How one slide gives way to the next on the screens: Cut, Dissolve, or another chosen
    // from the picker, which the label beside the buttons names ("Push: 0.6s").
    use pc_model::TransitionKind as K;
    let kind = state.transition.kind;
    if kind != K::Cut {
        let label = ui
            .add(
                egui::Button::new(egui::RichText::new(state.transition.label()).size(12.0).color(TEXT_DIM))
                    .frame(false),
            )
            .on_hover_text("Choose how long it takes, and its direction");
        egui::Popup::from_toggle_button_response(&label)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| transition_settings(ui, state));
    }
    // Laid out right to left: the last cell here is the leftmost.
    let others =
        if matches!(kind, K::Cut | K::Dissolve) { "More ▾".to_owned() } else { format!("{} ▾", kind.name()) };
    let (bar, _) = ui.allocate_exact_size(vec2(44.0 + 64.0 + 84.0 + 6.0, 30.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 1.0, PANEL);
    let mut more = None;
    for (index, label, width, x, hint) in [
        (0, "Cut".to_owned(), 44.0, 3.0, "Slides switch at once"),
        (1, "Dissolve".to_owned(), 64.0, 47.0, "Each slide fades into the next"),
        (2, others, 84.0, 111.0, "Choose another transition"),
    ] {
        let cell = Rect::from_min_size(bar.min + vec2(x, 3.0), vec2(width, 24.0));
        let response = ui.interact(cell, ui.scope_id().with(("transition", index)), Sense::click());
        let on = match index {
            0 => kind == K::Cut,
            1 => kind == K::Dissolve,
            _ => !matches!(kind, K::Cut | K::Dissolve),
        };
        if on {
            ui.painter().rect_filled(cell, RADIUS - 2.0, CARD);
        }
        let ink = if on || response.hovered() { TEXT } else { TEXT_FAINT };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, label.as_str(), FontId::proportional(12.0), ink);
        let response = response.on_hover_text(hint);
        match index {
            0 if response.clicked() => state.transition = pc_model::Transition::of(K::Cut),
            1 if response.clicked() && kind != K::Dissolve => state.transition = pc_model::Transition::of(K::Dissolve),
            2 => more = Some(response),
            _ => {}
        }
    }
    if let Some(more) = more {
        egui::Popup::from_toggle_button_response(&more)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| transition_picker(ui, state));
    }
}

/// The transition picker: every transition, grouped as ProPresenter groups them, with the
/// chosen one's settings under them.
fn transition_picker(ui: &mut egui::Ui, state: &mut HomeState) {
    use pc_model::TransitionKind as K;
    ui.set_width(330.0);
    for category in ["Dissolves", "Movements", "Wipes", "None"] {
        caption(ui, &category.to_uppercase());
        ui.horizontal_wrapped(|ui| {
            for kind in K::ALL.into_iter().filter(|kind| kind.category() == category) {
                let on = state.transition.kind == kind;
                if ui.add(egui::Button::selectable(on, kind.name()).min_size(vec2(100.0, 28.0))).clicked() && !on {
                    // A newly chosen kind keeps the direction, and takes its usual time.
                    state.transition = pc_model::Transition {
                        direction: state.transition.direction,
                        ..pc_model::Transition::of(kind)
                    };
                }
            }
        });
        ui.add_space(4.0);
    }
    ui.separator();
    transition_settings(ui, state);
}

/// The chosen transition's settings: its direction, if it moves, and how long it takes.
fn transition_settings(ui: &mut egui::Ui, state: &mut HomeState) {
    let look = &mut state.transition;
    if look.kind == pc_model::TransitionKind::Cut {
        ui.label(egui::RichText::new("Cut switches at once.").color(TEXT_DIM));
        return;
    }
    egui::Grid::new("transition settings").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        if look.kind.has_direction() {
            ui.label(egui::RichText::new("Direction").color(TEXT_DIM));
            egui::ComboBox::from_id_salt("transition direction").selected_text(look.direction.name()).show_ui(
                ui,
                |ui| {
                    for direction in pc_model::Direction::ALL {
                        ui.selectable_value(&mut look.direction, direction, direction.name());
                    }
                },
            );
            ui.end_row();
        }
        ui.label(egui::RichText::new("Duration").color(TEXT_DIM));
        ui.add(egui::Slider::new(&mut look.seconds, 0.1..=5.0).suffix(" s").max_decimals(1));
        ui.end_row();
    });
    if ui.button("Reset to Default Parameters").clicked() {
        *look = pc_model::Transition::of(look.kind);
    }
}
