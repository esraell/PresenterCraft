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
use crate::home::{HomeState, SlideView, ThumbOf, ToolMenu, matches_filter};
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

/// A small heading in capitals, as each section starts with.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(semibold(10.5)).color(TEXT_FAINT).extra_letter_spacing(0.8));
}

/// A button drawn as a rounded block with its words in the middle. `emphasis` picks its
/// colours: plain, chosen (accent), or live (tally red).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Emphasis {
    Plain,
    Chosen,
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
        Emphasis::Chosen => (if hovered { ACCENT } else { ACCENT.gamma_multiply(0.85) }, Color32::WHITE),
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
fn segmented(ui: &mut egui::Ui, salt: &str, each: egui::Vec2, labels: &[&str], chosen: Option<usize>) -> Option<usize> {
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
        let font = if on { semibold(13.5) } else { FontId::proportional(13.5) };
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
            let editing_theme = state.theme_edit.is_some();
            let view = if editing_theme {
                None
            } else if state.editing.is_some() {
                Some(1)
            } else if state.bible_open {
                Some(2)
            } else {
                Some(0)
            };
            let picked = segmented(ui, "views", vec2(74.0, 28.0), &["Show", "Edit", "Bible"], view);
            match picked {
                // Leaving the Theme Editor goes through the app, which saves the theme.
                Some(to) if editing_theme => {
                    if let Some(edit) = state.theme_edit.as_mut() {
                        edit.leave = true;
                    }
                    state.bible_open = to == 2;
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
            ui.add_space(6.0);
            for (menu, label, hint) in [
                (ToolMenu::Theme, "Theme", "Which theme songs and Bible slides use"),
                (ToolMenu::More, "More", "The Theme Editor and Settings"),
            ] {
                let open = matches!(state.menu, Some((now, _)) if now == menu);
                let emphasis =
                    if open || (menu == ToolMenu::More && editing_theme) { Emphasis::Chosen } else { Emphasis::Plain };
                let response = block_button(ui, vec2(70.0, 28.0), label, "", emphasis).on_hover_text(hint);
                if response.clicked() {
                    state.menu = Some((menu, pos2(response.rect.min.x, response.rect.max.y + 8.0)));
                    state.menu_fresh = true;
                }
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
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

/// The left column: the songs, with a field to search them.
pub fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let frame = Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 8 });
    egui::Panel::left("library").resizable(false).exact_size(248.0).frame(frame).show(ui, |ui| {
        let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
        let listed = |index: usize| !hidden.contains(&index);
        let total = (0..show.presentations().len()).filter(|index| listed(*index)).count();
        let shown: Vec<usize> = show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(index, song)| listed(*index) && matches_filter(&song.title, &state.filter))
            .map(|(index, _)| index)
            .collect();

        ui.horizontal(|ui| {
            caption(ui, "SONGS");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let count =
                    if shown.len() == total { total.to_string() } else { format!("{} of {total}", shown.len()) };
                ui.label(egui::RichText::new(count).size(11.0).color(TEXT_FAINT));
            });
        });
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
            let live_in = show.live().map(|at| at.presentation);
            for index in shown.iter().copied() {
                let Some(song) = show.presentations().get(index) else { continue };
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                let selected = index == show.selected();
                let painter = ui.painter();
                if selected {
                    painter.rect_filled(row, RADIUS, ACCENT.gamma_multiply(0.22));
                } else if response.hovered() {
                    painter.rect_filled(row, RADIUS, CARD);
                }
                // A tally light marks the song that has a slide on screen.
                let live = live_in == Some(index);
                if live {
                    painter.circle_filled(row.right_center() - vec2(12.0, 0.0), 3.5, TALLY);
                }
                let clip = Rect::from_min_max(row.min, pos2(row.max.x - if live { 24.0 } else { 8.0 }, row.max.y));
                painter.with_clip_rect(clip).text(
                    row.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    song.title.as_str(),
                    FontId::proportional(13.5),
                    if selected { Color32::WHITE } else { TEXT },
                );
                if response.clicked() {
                    commands.push(Command::Select(index));
                }
            }
            if total == 0 {
                ui.label(egui::RichText::new("No songs loaded.").color(TEXT_DIM));
            } else if shown.is_empty() {
                ui.label(egui::RichText::new("No song matches.").color(TEXT_DIM));
            }
        });
    });
}

/// The right column: what the audience sees now, the buttons that take it away, stepping
/// through the slides, and what comes next.
pub fn program_panel(
    ui: &mut egui::Ui,
    show: &Show,
    output_stopped: bool,
    preview: Option<TextureId>,
    commands: &mut Vec<Command>,
) {
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
        if let Some(texture) = preview {
            egui::Image::from_texture((texture, picture.size())).corner_radius(RADIUS).paint_at(ui, picture);
        }
        if !on {
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
        if block_button(ui, vec2(width, 40.0), "Clear All", "F1", clear_all).on_hover_text("Clear All").clicked() {
            commands.push(Command::ClearAll);
        }
        ui.horizontal(|ui| {
            let half = (width - 8.0) / 2.0;
            let emphasis = |live: bool| if live { Emphasis::Plain } else { Emphasis::Idle };
            if block_button(ui, vec2(half, 30.0), "Clear Slide", "C", emphasis(slide_live))
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
            let place = match (at, song) {
                (Some(at), Some(song)) => format!("{} / {}", at.slide + 1, song.slide_count()),
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
        let next = at.and_then(|at| Some((song?, song?.slide(at.slide + 1)?)));
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
                            live_tag(ui.painter(), pos2(rect.max.x, image.max.y + 10.0));
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
                        response.on_hover_text(item.name.as_str());
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

/// The small red "LIVE" tag for whatever is on screen. It sits beside the picture, never
/// over it, so nothing on the slide is hidden. `right_centre` is where its right edge goes.
pub fn live_tag(painter: &egui::Painter, right_centre: egui::Pos2) {
    let tag = Rect::from_min_size(right_centre - vec2(36.0, 8.5), vec2(36.0, 17.0));
    painter.rect_filled(tag, 8.5, TALLY);
    painter.text(tag.center(), Align2::CENTER_CENTER, "LIVE", semibold(10.0), Color32::WHITE);
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
}
