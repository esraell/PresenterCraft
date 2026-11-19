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

// A grey studio: neutral greys with no tint, light grey for what is chosen, and red only for
// what is on screen.

/// The window behind everything.
pub const BG: Color32 = Color32::from_rgb(28, 28, 29);
/// Side panels and bars.
pub const PANEL: Color32 = Color32::from_rgb(38, 38, 39);
/// Things raised off a panel: buttons, cards, fields.
pub const CARD: Color32 = Color32::from_rgb(52, 52, 54);
pub const HOVER: Color32 = Color32::from_rgb(64, 64, 66);
/// The rare line between two areas.
pub const LINE: Color32 = Color32::from_rgb(60, 60, 62);
pub const TEXT: Color32 = Color32::from_rgb(236, 236, 236);
pub const TEXT_DIM: Color32 = Color32::from_rgb(168, 168, 170);
pub const TEXT_FAINT: Color32 = Color32::from_rgb(118, 118, 120);
/// Chosen, in focus: outlines, lines and marks in light grey (and, faded, a lighter row).
pub const ACCENT: Color32 = Color32::from_rgb(205, 205, 208);
/// The fill of a chosen tab or button, under white words.
pub const SELECTED: Color32 = Color32::from_rgb(88, 88, 92);
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
    let count = labels.len();
    let total = each.x * count as f32;
    let (bar, _) = ui.allocate_exact_size(vec2(total + 6.0, each.y + 6.0), Sense::hover());
    ui.painter().rect_filled(bar, RADIUS + 2.0, BG);
    // Many choices share the width, so their words are smaller.
    let mut size: f32 = match count {
        0..=4 => 13.5,
        5 => 12.5,
        _ => 11.5,
    };
    // Each word, measured at a size (in the bolder face, the wider of the two).
    const PAD: f32 = 10.0;
    let measure = |ui: &egui::Ui, size: f32| -> Vec<f32> {
        labels
            .iter()
            .map(|label| ui.painter().layout_no_wrap((*label).to_owned(), semibold(size), TEXT).size().x)
            .collect()
    };
    let mut widths = measure(ui, size);
    // In a narrow bar the words shrink a little until they fit side by side.
    while widths.iter().sum::<f32>() + PAD * count as f32 > total && size > 9.5 {
        size -= 0.5;
        widths = measure(ui, size);
    }
    // Equal cells while every word fits in one; otherwise each takes room for its word, and the
    // room left over is shared out evenly.
    let crowded = widths.iter().any(|width| width + PAD > each.x);
    let spare = ((total - widths.iter().sum::<f32>()) / count.max(1) as f32).max(0.0);
    let cells: Vec<f32> =
        if crowded { widths.iter().map(|width| width + spare).collect() } else { vec![each.x; count] };
    let mut clicked = None;
    let mut x = bar.min.x + 3.0;
    for (index, label) in labels.iter().enumerate() {
        let cell = Rect::from_min_size(pos2(x, bar.min.y + 3.0), vec2(cells[index], each.y));
        x += cells[index];
        let response = ui.interact(cell, ui.scope_id().with((salt, index)), Sense::click());
        let on = chosen == Some(index);
        if on {
            ui.painter().rect_filled(cell, RADIUS, CARD);
        } else if response.hovered() {
            ui.painter().rect_filled(cell, RADIUS, PANEL);
        }
        let ink = if on { TEXT } else { TEXT_DIM };
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
                // The stage screens: a light and whether they are on.
                let stage_on = state.stage_on;
                let (rect, response) = ui.allocate_exact_size(vec2(118.0, 30.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(rect, RADIUS, if response.hovered() { HOVER } else { CARD });
                painter.circle_filled(
                    rect.left_center() + vec2(15.0, 0.0),
                    4.5,
                    if stage_on { READY } else { TEXT_FAINT },
                );
                painter.text(
                    rect.left_center() + vec2(28.0, 0.0),
                    Align2::LEFT_CENTER,
                    "Stage",
                    FontId::proportional(13.5),
                    TEXT,
                );
                painter.text(
                    rect.right_center() - vec2(12.0, 0.0),
                    Align2::RIGHT_CENTER,
                    if stage_on { "ON" } else { "OFF" },
                    FontId::proportional(11.0),
                    if stage_on { READY } else { TEXT_DIM },
                );
                let hint = if stage_on {
                    "The stage screens are on. Click to switch them off (Command-2)."
                } else {
                    "The stage screens are off. Click to switch them on (Command-2)."
                };
                if response.on_hover_text(hint).clicked() {
                    state.stage_on = !stage_on;
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

/// A song being dragged from the library, by its place among the songs.
#[derive(Clone, Copy)]
struct SongDrag(usize);

/// A row in the list of libraries and playlists: an icon and a name, renamed in place.
/// Shows when a dragged song would be dropped on it.
fn source_row(ui: &mut egui::Ui, icon: Icon, name: &str, chosen: bool) -> egui::Response {
    let response = list_row(ui, icon, name, chosen, false, false);
    if response.dnd_hover_payload::<SongDrag>().is_some() {
        ui.painter().rect_stroke(response.rect, RADIUS, Stroke::new(2.0, ACCENT), StrokeKind::Inside);
    }
    response
}

/// A heading over a group of rows, with a + and an arrow that folds the rows away.
/// Returns whether + was clicked.
fn source_heading(ui: &mut egui::Ui, text: &str, collapsed: &mut bool, add_hint: &str) -> bool {
    let mut add = false;
    ui.horizontal(|ui| {
        caption(ui, text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let (arrow, hint) = if *collapsed { ("⌄", "Show") } else { ("⌃", "Fold away") };
            if ui
                .add(egui::Button::new(egui::RichText::new(arrow).color(TEXT_FAINT)).frame(false))
                .on_hover_text(hint)
                .clicked()
            {
                *collapsed = !*collapsed;
            }
            let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
            add = response.on_hover_text(add_hint).clicked();
        });
    });
    add
}

/// The left column, as in ProPresenter: the libraries and the playlists as rows on top, the
/// songs or items of the one chosen below, and a filter at the bottom.
pub fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 8 });
    egui::Panel::left("library").resizable(false).exact_size(256.0).frame(frame).show(ui, |ui| {
        let column_height = ui.available_height();
        let rows = state.libraries.len() + state.services.list.len() + 1;
        let fitted = (rows as f32 * 32.0 + 70.0).min(column_height * 0.45);
        // The line between the rows on top and the list under them can be dragged.
        let sources = egui::Panel::top("sources")
            .resizable(true)
            .size_range(90.0..=(column_height * 0.7).max(90.0))
            .default_size(state.service_height.unwrap_or(fitted))
            .frame(Frame::new());
        let shown = sources.show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("sources").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                source_rows(ui, show, state);
            });
        });
        state.service_height = Some(shown.response.rect.height());

        // The filter, fixed to the bottom.
        egui::Panel::bottom("filter")
            .frame(Frame::new().inner_margin(egui::Margin { top: 8, ..Default::default() }))
            .show(ui, |ui| {
                let hint = if state.playlist_open { "Filter" } else { "Search songs" };
                ui.add(
                    egui::TextEdit::singleline(&mut state.filter)
                        .hint_text(hint)
                        .desired_width(f32::INFINITY)
                        .margin(egui::Margin::symmetric(10, 6))
                        .background_color(CARD),
                );
            });
        ui.add_space(10.0);
        if state.playlist_open {
            playlist_items(ui, show, state, commands);
        } else {
            library_songs(ui, show, state, commands);
        }
    });
}

/// The LIBRARY rows (All Songs and each library) and the PLAYLISTS rows. A song dragged from
/// the list below and dropped on one goes into it.
fn source_rows(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    use crate::service::Item;
    let song_item = |state: &HomeState, index: usize| {
        let key = state.song_keys.get(index)?;
        let song = show.presentations().get(index)?;
        Some(Item::Song { key: key.clone(), title: song.title.clone() })
    };
    if source_heading(ui, "LIBRARY", &mut state.library_collapsed, "New Library") {
        let made = crate::libraries::Library::new(crate::libraries::new_name(&state.libraries));
        state.renaming_library = Some((made.id, made.name.clone()));
        state.rename_started = true;
        state.library_shown = Some(made.id);
        state.playlist_open = false;
        state.libraries.push(made);
        state.libraries_changed = true;
    }
    if !state.library_collapsed {
        let all = source_row(ui, Icon::Library, "All Songs", !state.playlist_open && state.library_shown.is_none());
        if all.clicked() {
            state.library_shown = None;
            state.playlist_open = false;
        }
        let mut delete = None;
        let ids: Vec<(pc_model::Uuid, String)> =
            state.libraries.iter().map(|library| (library.id, library.name.clone())).collect();
        for (id, name) in ids {
            let renaming = state.renaming_library.as_ref().is_some_and(|(have, _)| *have == id);
            let chosen = !state.playlist_open && state.library_shown == Some(id);
            let shown = if name.trim().is_empty() { "Untitled" } else { name.as_str() };
            let response = source_row(ui, Icon::Library, if renaming { "" } else { shown }, chosen);
            if renaming && let Some((_, typed)) = state.renaming_library.as_mut() {
                let field = rename_field_at(
                    ui,
                    typed,
                    FontId::proportional(13.5),
                    TEXT,
                    ACCENT,
                    response.rect.left_center() + vec2(26.0, 0.0),
                    response.rect.max.x - 4.0,
                );
                if std::mem::take(&mut state.rename_started) {
                    field.request_focus();
                }
                let (enter, escape) =
                    ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                if escape {
                    state.renaming_library = None;
                } else if enter || field.lost_focus() {
                    let typed = typed.trim().to_owned();
                    if let Some(library) = state.libraries.iter_mut().find(|library| library.id == id)
                        && !typed.is_empty()
                    {
                        library.name = typed;
                        state.libraries_changed = true;
                    }
                    state.renaming_library = None;
                }
                continue;
            }
            if response.clicked() {
                state.library_shown = Some(id);
                state.playlist_open = false;
            }
            if response.double_clicked() {
                state.renaming_library = Some((id, name.clone()));
                state.rename_started = true;
            }
            if let Some(dropped) = response.dnd_release_payload::<SongDrag>()
                && let Some(key) = state.song_keys.get(dropped.0).cloned()
                && let Some(library) = state.libraries.iter_mut().find(|library| library.id == id)
                && !library.has(&key)
            {
                library.toggle(&key);
                state.libraries_changed = true;
            }
            response.on_hover_text("Drag songs here to add them").context_menu(|ui| {
                if ui.button("Rename").clicked() {
                    state.renaming_library = Some((id, name.clone()));
                    state.rename_started = true;
                    ui.close();
                }
                if ui.button("Delete Library").on_hover_text("The songs stay in All Songs").clicked() {
                    delete = Some(id);
                    ui.close();
                }
            });
        }
        if let Some(id) = delete {
            state.libraries.retain(|library| library.id != id);
            if state.library_shown == Some(id) {
                state.library_shown = None;
            }
            state.libraries_changed = true;
        }
    }
    ui.add_space(8.0);
    if source_heading(ui, "PLAYLISTS", &mut state.playlists_collapsed, "New Playlist") {
        state.services.add_service();
        state.playlist_open = true;
        state.renaming_service = Some(state.services.front().name.clone());
        state.rename_started = true;
    }
    if state.playlists_collapsed {
        return;
    }
    let names: Vec<String> = state.services.list.iter().map(|playlist| playlist.name.clone()).collect();
    let mut delete = false;
    for (index, name) in names.iter().enumerate() {
        let front = index == state.services.current;
        let renaming = front && state.renaming_service.is_some();
        let response = source_row(ui, Icon::Rows, if renaming { "" } else { name }, state.playlist_open && front);
        if renaming && let Some(typed) = state.renaming_service.as_mut() {
            let field = rename_field_at(
                ui,
                typed,
                FontId::proportional(13.5),
                TEXT,
                ACCENT,
                response.rect.left_center() + vec2(26.0, 0.0),
                response.rect.max.x - 4.0,
            );
            if std::mem::take(&mut state.rename_started) {
                field.request_focus();
            }
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if escape {
                state.renaming_service = None;
            } else if enter || field.lost_focus() {
                let typed = typed.clone();
                state.services.rename(&typed);
                state.renaming_service = None;
            }
            continue;
        }
        if response.clicked() {
            state.services.show(index);
            state.playlist_open = true;
        }
        if response.double_clicked() {
            state.services.show(index);
            state.renaming_service = Some(name.clone());
            state.rename_started = true;
        }
        if let Some(dropped) = response.dnd_release_payload::<SongDrag>()
            && let Some(item) = song_item(state, dropped.0)
        {
            state.services.add_to(index, item);
        }
        response.on_hover_text("Drag songs here to add them").context_menu(|ui| {
            if ui.button("Rename").clicked() {
                state.services.show(index);
                state.renaming_service = Some(name.clone());
                state.rename_started = true;
                ui.close();
            }
            if ui.add_enabled(names.len() > 1, egui::Button::new("Delete Playlist")).clicked() {
                state.services.show(index);
                delete = true;
                ui.close();
            }
        });
    }
    if delete {
        state.services.remove_service();
    }
}

/// The name of the first library a song is in, shown at the right of its row in a playlist.
fn library_of<'a>(state: &'a HomeState, key: &str) -> Option<&'a str> {
    state.libraries.iter().find(|library| library.has(key)).map(|library| library.name.as_str())
}

/// The songs of the library chosen (or every song), each with a + to put it in the playlist in
/// front; a song can also be dragged onto a playlist or a library above.
fn library_songs(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    let removed = state.removed_songs.clone();
    let listed = |index: usize| !hidden.contains(&index) && !removed.contains(&index);
    let library = state.library_shown.and_then(|id| state.libraries.iter().find(|library| library.id == id)).cloned();
    if library.is_none() {
        state.library_shown = None;
    }
    let keys = state.song_keys.clone();
    let in_library =
        |index: usize| library.as_ref().is_none_or(|library| keys.get(index).is_some_and(|key| library.has(key)));
    let total = (0..show.presentations().len()).filter(|index| listed(*index) && in_library(*index)).count();
    let shown: Vec<usize> = show
        .presentations()
        .iter()
        .enumerate()
        .filter(|(index, song)| listed(*index) && in_library(*index) && matches_filter(&song.title, &state.filter))
        .map(|(index, _)| index)
        .collect();
    let heading = library.as_ref().map_or_else(|| "ALL SONGS".to_owned(), |library| library.name.to_uppercase());
    let live_in = show.live().map(|at| at.presentation);
    let mut add = None;
    ui.horizontal(|ui| {
        caption(ui, &heading);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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
    ui.add_space(4.0);
    let front = state.services.front().name.clone();
    egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for index in shown.iter().copied() {
            let Some(song) = show.presentations().get(index) else { continue };
            let chosen = index == show.selected() && !state.bible_open;
            let response = list_row(ui, Icon::Document, song.title.as_str(), chosen, live_in == Some(index), false);
            response.dnd_set_drag_payload(SongDrag(index));
            // A "+" at the right of the row under the pointer puts the song in the playlist in front.
            let plus = Rect::from_center_size(response.rect.right_center() - vec2(13.0, 0.0), vec2(20.0, 20.0));
            if response.hovered() || ui.rect_contains_pointer(plus) {
                let button = ui.interact(plus, ui.scope_id().with(("add to playlist", index)), Sense::click());
                let ink = if button.hovered() { Color32::WHITE } else { TEXT_DIM };
                ui.painter().rect_filled(plus, 5.0, if button.hovered() { SELECTED } else { HOVER });
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                if button.on_hover_text(format!("Add to {front}")).clicked() {
                    add = Some(index);
                    continue;
                }
            }
            if response.clicked() {
                commands.push(Command::Select(index));
                state.bible_open = false;
                state.services.chosen = None;
            }
            response.context_menu(|ui| song_menu(ui, show, state, commands, index, &mut add));
        }
        if total == 0 {
            let empty = if library.is_some() {
                "No songs in this library yet. Drag songs onto it, or use a song's right-click menu."
            } else {
                "No songs loaded."
            };
            ui.label(egui::RichText::new(empty).size(12.0).color(TEXT_DIM));
        } else if shown.is_empty() {
            ui.label(egui::RichText::new("No song matches.").color(TEXT_DIM));
        }
    });
    if let Some(index) = add
        && let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index))
    {
        state.services.add(Item::Song { key: key.clone(), title: song.title.clone() });
    }
}

/// A song's right-click menu in the library.
fn song_menu(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    commands: &mut Vec<Command>,
    index: usize,
    add: &mut Option<usize>,
) {
    let front = state.services.front().name.clone();
    if ui.button(format!("Add to {front}")).clicked() {
        *add = Some(index);
        ui.close();
    }
    // The libraries the song is in.
    if let Some(key) = state.song_keys.get(index).cloned() {
        ui.menu_button("Libraries", |ui| {
            if state.libraries.is_empty() {
                ui.label(egui::RichText::new("No libraries yet. Make one with + beside LIBRARY.").color(TEXT_DIM));
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
    // The Announcements layer: the song's slides go round by themselves over everything else.
    let announcing = state.announcement_live == Some(index);
    let (label, hint) = if announcing {
        ("Stop Announcements", "Take this song off the Announcements layer")
    } else {
        (
            "Play on Announcements Layer",
            "Its slides go round by themselves over the slides (each for its Go to Next time, or 8 seconds)",
        )
    };
    if ui.button(label).on_hover_text(hint).clicked() {
        state.announcement_request = Some((!announcing).then_some(index));
        ui.close();
    }
    // How its slides give way to each other, unless a slide has its own.
    let own_transition = show.presentations().get(index).and_then(|song| song.transition);
    ui.menu_button("Transition", |ui| {
        if ui
            .add(egui::Button::selectable(own_transition.is_none(), "As All Slides"))
            .on_hover_text("The transition chosen under the slides")
            .clicked()
        {
            state.song_transition_request = Some((index, None));
            ui.close();
        }
        ui.separator();
        let mut look = own_transition.unwrap_or(state.transition);
        let before = look;
        transition_picker(ui, &mut look, "song");
        if look != before || (own_transition.is_none() && ui.button("Use This for the Song").clicked()) {
            state.song_transition_request = Some((index, Some(look)));
        }
    });
    // The theme this song is drawn with.
    let own = show.presentations().get(index).and_then(|song| song.theme);
    ui.menu_button("Theme", |ui| {
        if ui.add(egui::Button::selectable(own.is_none(), "Songs Theme")).on_hover_text("The theme songs use").clicked()
        {
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
}

/// The items of the playlist in front: headers, songs (with the library each is in) and Bible
/// passages, dragged up and down to change the order.
fn playlist_items(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    use crate::service::Item;
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    let removed = state.removed_songs.clone();
    let listed = |index: usize| !hidden.contains(&index) && !removed.contains(&index);
    // A song in a playlist is found by the name its file gave it; one saved before it had a
    // file is found by its title.
    let song_at = |keys: &[String], key: &str| {
        let by_title = key.strip_prefix("title:");
        keys.iter()
            .position(|have| have == key)
            .or_else(|| by_title.and_then(|title| show.presentations().iter().position(|song| song.title == title)))
            .filter(|index| listed(*index))
    };
    let items = state.services.front().items.clone();
    let live_in = show.live().map(|at| at.presentation);
    let filtering = !state.filter.trim().is_empty();
    ui.horizontal(|ui| {
        let count = items.iter().filter(|item| !matches!(item, Item::Header { .. })).count();
        let noun = if count == 1 { "ITEM" } else { "ITEMS" };
        caption(ui, &format!("{count} {noun}"));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // + adds a header, a heading that groups the items under it.
            let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
            let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
            icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
            if response.on_hover_text("New Header").clicked() {
                state.services.add(Item::Header { name: "Header".to_owned() });
                let at = state.services.front().items.len() - 1;
                state.renaming_header = Some((at, "Header".to_owned()));
                state.rename_started = true;
            }
        });
    });
    ui.add_space(4.0);

    // What a click on a row asks for, carried out after the list is drawn.
    enum Ask {
        Open(usize),
        Earlier(usize),
        Later(usize),
        Remove(usize),
        Rename(usize),
    }
    let mut ask = None;
    // Rows can be dragged up and down to change the order (not while the list is filtered).
    let mut rows: Vec<Rect> = Vec::new();
    let mut dragging = None;
    let mut dropped = None;
    let mut renamed: Option<Option<(usize, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    egui::ScrollArea::vertical().id_salt("playlist items").auto_shrink([false, false]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        if items.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Nothing yet. Drag songs onto this playlist, use + on a song in a library, or Save to \
                     Playlist on the Bible page.",
                )
                .size(12.0)
                .color(TEXT_FAINT),
            );
        }
        for (index, item) in items.iter().enumerate() {
            if filtering && !matches_filter(item.title(), &state.filter) {
                continue;
            }
            // A header: bold, full width, and renamed in place.
            if let Item::Header { name } = item {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
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
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
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
                        ui.painter().hline(row.x_range(), row.max.y - 1.0, Stroke::new(1.0, LINE));
                    }
                }
                if response.dragged() && !filtering {
                    dragging = Some(index);
                }
                if response.drag_stopped() && !filtering {
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
                    if ui.button("Remove from Playlist").clicked() {
                        ask = Some(Ask::Remove(index));
                        ui.close();
                    }
                });
                continue;
            }
            let (icon, found, library) = match item {
                Item::Song { key, .. } => (Icon::Document, song_at(&state.song_keys, key), library_of(state, key)),
                _ => (Icon::Book, None, None),
            };
            let missing = matches!(item, Item::Song { .. }) && found.is_none();
            let live = found.is_some() && found == live_in;
            let chosen = state.services.chosen == Some(index);
            let mut response = list_row(ui, icon, item.title(), chosen, live, missing);
            // The library the song is in, at the right, as ProPresenter shows it.
            if let Some(library) = library.filter(|_| !live) {
                ui.painter().text(
                    response.rect.right_center() - vec2(8.0, 0.0),
                    Align2::RIGHT_CENTER,
                    library,
                    FontId::proportional(11.0),
                    TEXT_FAINT,
                );
            }
            rows.push(response.rect);
            if response.dragged() && !filtering {
                dragging = Some(index);
            }
            if response.drag_stopped() && !filtering {
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
                if ui.button("Remove from Playlist").clicked() {
                    ask = Some(Ask::Remove(index));
                    ui.close();
                }
            });
        }
    });
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
            state.renaming_header =
                Some((index, items.get(index).map(|item| item.title().to_owned()).unwrap_or_default()));
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
}

/// The right column: what the audience sees now, the buttons that take it away, stepping
/// through the slides, and what comes next.
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
        if let Some(video) = state.video_live.clone() {
            video_controls(ui, state, &video, width);
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
                .on_hover_text("Take the picture or video off the screen")
                .clicked()
            {
                commands.push(Command::Clear(Layer::Media));
            }
        });
        // The other layers, each on its own.
        ui.horizontal(|ui| {
            let third = (width - 16.0) / 3.0;
            let emphasis = |live: bool| if live { Emphasis::Plain } else { Emphasis::Idle };
            let layers = [
                ("Props", "Clear Props: take the props off (F4)", !state.props_live.is_empty(), Layer::Props),
                (
                    "Messages",
                    "Clear Messages: take the message off (F6)",
                    state.message_live.is_some(),
                    Layer::Messages,
                ),
                ("Audio", "Clear Audio: stop the audio (F5)", state.audio_playing.track.is_some(), Layer::Audio),
            ];
            for (name, hint, live, layer) in layers {
                if block_button(ui, vec2(third, 26.0), name, "", emphasis(live)).on_hover_text(hint).clicked() {
                    commands.push(Command::Clear(layer));
                }
            }
        });
        // While a song plays on the Announcements layer, a button to take it off.
        if state.announcement_live.is_some()
            && block_button(ui, vec2(width, 26.0), "Clear Announcements", "", Emphasis::Plain)
                .on_hover_text("Take the announcements off the screens")
                .clicked()
        {
            commands.push(Command::Clear(Layer::Announcements));
        }
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
    // Folded away from the View menu, unless there is something to say or a file is being dropped.
    if state.media_hidden && state.notice.is_none() && !state.drop_hover {
        return;
    }
    let empty = show.media().is_empty();
    let height = if empty && !state.drop_hover { 40.0 } else { 148.0 };
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin::symmetric(16, 10));
    egui::Panel::bottom("media bin").resizable(false).exact_size(height).frame(frame).show(ui, |ui| {
        let area = ui.max_rect().expand2(vec2(16.0, 10.0));
        ui.horizontal(|ui| {
            caption(ui, "MEDIA");
            media_playlist_picker(ui, state);
            let note = match &state.notice {
                Some(notice) => egui::RichText::new(notice.as_str()).size(12.0).color(Color32::from_rgb(255, 170, 120)),
                None if empty => egui::RichText::new("Drop pictures or videos on the window to add backgrounds")
                    .size(12.0)
                    .color(TEXT_FAINT),
                None => egui::RichText::new("Click a picture or video to put it behind the words")
                    .size(12.0)
                    .color(TEXT_FAINT),
            };
            ui.add_space(6.0);
            ui.label(note);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.small_button("+").on_hover_text("Add pictures and videos").clicked() {
                    state.media_request = Some(crate::home::MediaRequest::Choose);
                }
                // How one picture or video gives way to the next, as ProPresenter's media bin has.
                let label = ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(format!("{} ▾", state.media_transition.label()))
                                .size(12.0)
                                .color(TEXT_DIM),
                        )
                        .frame(false),
                    )
                    .on_hover_text("How one picture or video gives way to the next");
                egui::Popup::from_toggle_button_response(&label)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| transition_picker(ui, &mut state.media_transition, "media"));
            });
        });
        if !empty {
            ui.add_space(4.0);
            let image_size = vec2(144.0, 81.0);
            egui::ScrollArea::horizontal().id_salt("media row").auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    // A media playlist shows only its own files.
                    let shown = state
                        .media_playlist_shown
                        .and_then(|id| state.media_playlists.iter().find(|playlist| playlist.id == id))
                        .cloned();
                    for (index, item) in show.media().iter().enumerate() {
                        if shown.as_ref().is_some_and(|playlist| !playlist.has(&item.path)) {
                            continue;
                        }
                        let (rect, response) =
                            ui.allocate_exact_size(image_size + vec2(0.0, 18.0), Sense::click_and_drag());
                        // Dragged onto a slide, it becomes what the slide shows; onto the space
                        // under the slides, a slide of its own.
                        response.dnd_set_drag_payload(crate::home::MediaDrag(index));
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
                        // A video says so in its corner.
                        if crate::media::is_video(&item.path) {
                            let badge = Rect::from_min_size(image.left_bottom() + vec2(5.0, -19.0), vec2(22.0, 14.0));
                            ui.painter().rect_filled(badge, 3.0, Color32::from_black_alpha(170));
                            ui.painter().text(
                                badge.center(),
                                Align2::CENTER_CENTER,
                                "▶",
                                FontId::proportional(9.0),
                                Color32::WHITE,
                            );
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
                            media_item_menu(ui, state, index, item);
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
            .show(|ui| transition_settings(ui, &mut state.transition, "slides"));
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
            .show(|ui| transition_picker(ui, &mut state.transition, "slides"));
    }
}

/// The transition picker: every transition, grouped as ProPresenter groups them, with the
/// chosen one's settings under them.
pub fn transition_picker(ui: &mut egui::Ui, look: &mut pc_model::Transition, id: &str) {
    use pc_model::TransitionKind as K;
    ui.set_width(330.0);
    for category in ["Dissolves", "Movements", "Wipes", "None"] {
        caption(ui, &category.to_uppercase());
        ui.horizontal_wrapped(|ui| {
            for kind in K::ALL.into_iter().filter(|kind| kind.category() == category) {
                let on = look.kind == kind;
                if ui.add(egui::Button::selectable(on, kind.name()).min_size(vec2(100.0, 28.0))).clicked() && !on {
                    // A newly chosen kind keeps the direction, and takes its usual time.
                    *look = pc_model::Transition { direction: look.direction, ..pc_model::Transition::of(kind) };
                }
            }
        });
        ui.add_space(4.0);
    }
    ui.separator();
    transition_settings(ui, look, id);
}

/// The chosen transition's settings: its direction, if it moves, and how long it takes.
fn transition_settings(ui: &mut egui::Ui, look: &mut pc_model::Transition, id: &str) {
    if look.kind == pc_model::TransitionKind::Cut {
        ui.label(egui::RichText::new("Cut switches at once.").color(TEXT_DIM));
        return;
    }
    egui::Grid::new(("transition settings", id)).num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        if look.kind.has_direction() {
            ui.label(egui::RichText::new("Direction").color(TEXT_DIM));
            egui::ComboBox::from_id_salt(("transition direction", id)).selected_text(look.direction.name()).show_ui(
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

/// The video on screen: play and pause, start again, how far in it is (a click on the bar goes
/// there), and whether it loops and has sound.
fn video_controls(ui: &mut egui::Ui, state: &mut HomeState, video: &crate::home::VideoLive, width: f32) {
    use crate::home::VideoRequest;
    let clock = |seconds: f64| crate::audio::clock(std::time::Duration::from_secs_f64(seconds.max(0.0)));
    // A small button that is only an icon.
    let tool = |ui: &mut egui::Ui, icon: Icon, hint: &str| {
        let (cell, response) = ui.allocate_exact_size(vec2(26.0, 22.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(cell, 5.0, HOVER);
        }
        icons::paint(ui.painter(), icon, Rect::from_center_size(cell.center(), vec2(12.0, 12.0)), TEXT);
        response.on_hover_text(hint)
    };
    egui::Frame::new().fill(CARD).corner_radius(8).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.set_width(width - 16.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let (icon, hint) =
                if video.playing { (Icon::Pause, "Pause the video") } else { (Icon::Play, "Play the video") };
            if tool(ui, icon, hint).clicked() {
                state.video_request = Some(VideoRequest::TogglePause);
            }
            if tool(ui, Icon::Back, "Play the video from the start").clicked() {
                state.video_request = Some(VideoRequest::Restart);
            }
            ui.add_space(6.0);
            ui.add(egui::Label::new(egui::RichText::new(video.name.as_str()).size(12.0).color(TEXT)).truncate());
        });
        let fraction = video.duration.map_or(0.0, |length| (video.position / length.max(0.001)).clamp(0.0, 1.0));
        let (bar, response) = ui.allocate_exact_size(vec2(ui.available_width(), 12.0), Sense::click());
        let track = Rect::from_center_size(bar.center(), vec2(bar.width(), 4.0));
        ui.painter().rect_filled(track, 2.0, LINE);
        let done = Rect::from_min_max(track.min, pos2(track.min.x + track.width() * fraction as f32, track.max.y));
        ui.painter().rect_filled(done, 2.0, ACCENT);
        if let (Some(length), Some(at)) =
            (video.duration, response.interact_pointer_pos().filter(|_| response.clicked()))
        {
            let to = f64::from(((at.x - track.min.x) / track.width()).clamp(0.0, 1.0)) * length;
            state.video_request = Some(VideoRequest::Seek(to));
        }
        response.on_hover_text("Click to go to that point in the video");
        ui.horizontal(|ui| {
            let total = video.duration.map_or_else(|| "–".to_owned(), clock);
            ui.label(
                egui::RichText::new(format!("{} / {total}", clock(video.position)))
                    .size(11.0)
                    .color(TEXT_DIM)
                    .monospace(),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.selectable_label(!video.muted, "Sound").on_hover_text("Play the video's sound").clicked() {
                    state.video_request = Some(VideoRequest::Muted(!video.muted));
                }
                if ui.selectable_label(video.looping, "Loop").on_hover_text("Start again at the end").clicked() {
                    state.video_request = Some(VideoRequest::Looping(!video.looping));
                }
            });
        });
    });
}

/// The menu beside MEDIA: all media or one media playlist, a new playlist, and renaming or
/// deleting the one shown.
fn media_playlist_picker(ui: &mut egui::Ui, state: &mut HomeState) {
    let shown = state.media_playlist_shown.and_then(|id| state.media_playlists.iter().find(|p| p.id == id));
    if shown.is_none() {
        state.media_playlist_shown = None;
    }
    let label = shown.map_or_else(|| "All Media ▾".to_owned(), |playlist| format!("{} ▾", playlist.name));
    let picker = ui
        .add(egui::Label::new(egui::RichText::new(label).font(semibold(11.5)).color(TEXT_DIM)).sense(Sense::click()))
        .on_hover_text("Choose a media playlist");
    egui::Popup::from_toggle_button_response(&picker)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_min_width(200.0);
            if ui.add(egui::Button::selectable(state.media_playlist_shown.is_none(), "All Media")).clicked() {
                state.media_playlist_shown = None;
                ui.close();
            }
            for playlist in &state.media_playlists {
                let on = state.media_playlist_shown == Some(playlist.id);
                let name = if playlist.name.trim().is_empty() { "Untitled" } else { playlist.name.as_str() };
                if ui.add(egui::Button::selectable(on, name)).clicked() {
                    state.media_playlist_shown = Some(playlist.id);
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("New Playlist").on_hover_text("Add media to it from an item's right-click menu").clicked() {
                let made = crate::media::Playlist::new(crate::media::new_playlist_name(&state.media_playlists));
                state.media_playlist_shown = Some(made.id);
                state.media_playlists.push(made);
                state.media_playlists_changed = true;
            }
            if let Some(at) =
                state.media_playlist_shown.and_then(|id| state.media_playlists.iter().position(|p| p.id == id))
            {
                let id = state.media_playlists[at].id;
                ui.separator();
                // Played item after item on the media layer, round and round.
                if state.media_playing == Some(id) {
                    if ui.button("Stop Playing").clicked() {
                        state.media_request = Some(crate::home::MediaRequest::PlayPlaylist(None));
                        ui.close();
                    }
                } else if ui
                    .button("Play This Playlist")
                    .on_hover_text("Its pictures (8 seconds each) and videos (to their end) in turn, round and round")
                    .clicked()
                {
                    state.media_request = Some(crate::home::MediaRequest::PlayPlaylist(Some(id)));
                    ui.close();
                }
                // A folder it follows: what is in the folder is in the playlist.
                match state.media_playlists[at].folder.clone() {
                    Some(folder) => {
                        ui.label(
                            egui::RichText::new(format!("Follows {}", folder.display())).size(11.5).color(TEXT_DIM),
                        );
                        if ui.button("Stop Following the Folder").clicked() {
                            state.media_request = Some(crate::home::MediaRequest::WatchFolder(id, false));
                            ui.close();
                        }
                    }
                    None => {
                        if ui
                            .button("Follow a Folder…")
                            .on_hover_text(
                                "The pictures and videos in a folder join this playlist as they come and change",
                            )
                            .clicked()
                        {
                            state.media_request = Some(crate::home::MediaRequest::WatchFolder(id, true));
                            ui.close();
                        }
                    }
                }
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                    let field =
                        ui.add(egui::TextEdit::singleline(&mut state.media_playlists[at].name).desired_width(140.0));
                    state.media_playlists_changed |= field.changed();
                });
                if ui.button("Delete Playlist").on_hover_text("The media stays in All Media").clicked() {
                    state.media_playlists.remove(at);
                    state.media_playlist_shown = None;
                    state.media_playlists_changed = true;
                    ui.close();
                }
            }
        });
}

/// A media item's right-click menu: how it fills the screen, whether a video loops, the media
/// playlists it is in, and Remove.
fn media_item_menu(ui: &mut egui::Ui, state: &mut HomeState, index: usize, item: &pc_model::MediaItem) {
    use crate::home::MediaRequest;
    ui.menu_button("Scaling", |ui| {
        for (fit, name, hint) in [
            (pc_model::ImageFit::Fill, "Scale to Fill", "Fill the screen, cutting off what hangs over"),
            (pc_model::ImageFit::Fit, "Scale to Fit", "Show all of it, with bars where its shape differs"),
            (pc_model::ImageFit::Stretch, "Stretch to Fill", "Stretch it to the screen's shape"),
        ] {
            if ui.add(egui::Button::selectable(item.fit == fit, name)).on_hover_text(hint).clicked() {
                state.media_request = Some(MediaRequest::Look(index, fit, item.looping));
                ui.close();
            }
        }
    });
    if crate::media::is_video(&item.path)
        && ui
            .add(egui::Button::selectable(item.looping, "Loop"))
            .on_hover_text("Start again at the end; otherwise stay on the last picture")
            .clicked()
    {
        state.media_request = Some(MediaRequest::Look(index, item.fit, !item.looping));
        ui.close();
    }
    if ui
        .button("Add to Song as Slide")
        .on_hover_text("A slide that shows this when it goes on screen, after the song's chosen slide")
        .clicked()
    {
        state.media_request = Some(MediaRequest::AddAsSlide(index));
        ui.close();
    }
    ui.menu_button("Playlists", |ui| {
        if state.media_playlists.is_empty() {
            ui.label(
                egui::RichText::new("No media playlists yet. Make one from the menu beside MEDIA.").color(TEXT_DIM),
            );
        }
        for playlist in &mut state.media_playlists {
            if ui.add(egui::Button::selectable(playlist.has(&item.path), playlist.name.as_str())).clicked() {
                playlist.toggle(&item.path);
                state.media_playlists_changed = true;
            }
        }
    });
    ui.separator();
    if ui.button("Remove").clicked() {
        state.media_request = Some(MediaRequest::Remove(index));
        ui.close();
    }
}
