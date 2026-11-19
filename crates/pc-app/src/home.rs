//! The home window: toolbar, library, slide grid, media bin and the live panel.
//!
//! This draws one frame and returns what the operator asked for. It changes nothing itself.
//!
//! Many controls are placeholders for parts that are not built yet. They are drawn in their
//! final positions so the layout is settled, they are dim, and they say "not built yet".

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind, TextureId, pos2, vec2,
};
use pc_engine::{Command, Layer, Show, SlideRef};
use pc_model::{Rgba, Slide};

use crate::icons::{self, Icon};

/// The one accent colour: the live slide's outline and selections.
pub const ACCENT: Color32 = Color32::from_rgb(64, 156, 255);
const BAR: Color32 = Color32::from_rgb(44, 44, 47);
const SIDE: Color32 = Color32::from_rgb(40, 40, 43);
const CANVAS: Color32 = Color32::from_rgb(27, 27, 29);
const ON: Color32 = Color32::from_rgb(70, 200, 110);
const OFF: Color32 = Color32::from_rgb(210, 70, 70);
const DIM: Color32 = Color32::from_gray(120);
/// The strip that names what the slide area is showing.
const TITLE_STRIP: Color32 = Color32::from_rgb(120, 120, 126);
/// Height of the media bin, shared by its list (left column) and its thumbnails (middle).
const MEDIA_BIN: f32 = 190.0;

pub struct HomeState {
    /// Displayed thumbnail width in points.
    pub thumb_width: f32,
    /// Text typed into the library's filter box.
    pub filter: String,
    /// Set by the app when the output has stopped drawing, so the operator is told.
    pub output_stopped: bool,
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState { thumb_width: 170.0, filter: String::new(), output_stopped: false }
    }
}

fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Black or white, whichever reads better on the colour.
fn text_on(c: Rgba) -> Color32 {
    let luminance = 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b);
    if luminance > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// True when a title matches what was typed in the filter box (case-insensitive, any part).
pub fn matches_filter(title: &str, filter: &str) -> bool {
    let filter = filter.trim();
    filter.is_empty() || title.to_lowercase().contains(&filter.to_lowercase())
}

fn section_label(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).size(11.0).strong().color(Color32::from_gray(140)));
}

fn panel_frame(fill: Color32, margin: i8) -> Frame {
    Frame::new().fill(fill).inner_margin(margin)
}

fn not_built(what: &str) -> String {
    format!("{what} (not built yet)")
}

/// What a button shows: one of the app's icons, or a short piece of text.
#[derive(Clone, Copy)]
enum Face {
    Icon(Icon),
    Text(&'static str),
}

impl From<Icon> for Face {
    fn from(icon: Icon) -> Face {
        Face::Icon(icon)
    }
}

impl From<&'static str> for Face {
    fn from(text: &'static str) -> Face {
        Face::Text(text)
    }
}

fn paint_face(ui: &egui::Ui, face: Face, rect: Rect, text_size: f32, color: Color32) {
    match face {
        Face::Icon(icon) => icons::paint(ui.painter(), icon, rect, color),
        Face::Text(text) => {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, FontId::proportional(text_size), color);
        }
    }
}

/// A small placeholder button: an icon, dim, that only explains itself.
fn ghost(ui: &mut egui::Ui, face: impl Into<Face>, what: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, Color32::from_gray(48));
    }
    paint_face(ui, face.into(), Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), 12.5, DIM);
    response.on_hover_text(not_built(what));
}

/// A section heading with placeholder buttons at its right edge.
fn heading_with(ui: &mut egui::Ui, text: &str, buttons: &[(Icon, &str)]) {
    ui.horizontal(|ui| {
        section_label(ui, text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (icon, what) in buttons {
                ghost(ui, *icon, what);
            }
        });
    });
}

/// One row in a list: an icon, a name, and optional dim text at the right edge.
fn list_row(ui: &mut egui::Ui, icon: Icon, name: &str, right: &str, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 5.0, ACCENT.gamma_multiply(0.55));
    } else if response.hovered() {
        painter.rect_filled(rect, 5.0, Color32::from_gray(44));
    }
    let ink = if selected { Color32::WHITE } else { Color32::from_gray(215) };
    let icon_rect = Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), vec2(14.0, 14.0));
    icons::paint(painter, icon, icon_rect, if selected { Color32::WHITE } else { ACCENT });
    let right_width = if right.is_empty() { 0.0 } else { 52.0 };
    let text_clip = Rect::from_min_max(rect.min + vec2(30.0, 0.0), rect.max - vec2(right_width + 6.0, 0.0));
    painter.with_clip_rect(text_clip).text(
        rect.left_center() + vec2(30.0, 0.0),
        Align2::LEFT_CENTER,
        name,
        FontId::proportional(13.0),
        ink,
    );
    if !right.is_empty() {
        painter.text(
            rect.right_center() - vec2(8.0, 0.0),
            Align2::RIGHT_CENTER,
            right,
            FontId::proportional(11.0),
            if selected { Color32::from_gray(220) } else { DIM },
        );
    }
    response
}

/// One item in a toolbar group.
struct Tool {
    face: Face,
    label: &'static str,
    selected: bool,
    built: bool,
}

impl Tool {
    fn ghost(face: impl Into<Face>, label: &'static str) -> Tool {
        Tool { face: face.into(), label, selected: false, built: false }
    }
}

const TOOL_WIDTH: f32 = 46.0;
const PILL_HEIGHT: f32 = 30.0;
const TOOL_HEIGHT: f32 = 50.0;

/// A group of toolbar items sharing one rounded pill, each with its label beneath. The
/// selected item is lit; items that are not built are dim. Returns the index of a clicked,
/// working item.
fn tool_group(ui: &mut egui::Ui, tools: &[Tool]) -> Option<usize> {
    let width = TOOL_WIDTH * tools.len() as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(width, TOOL_HEIGHT), Sense::hover());
    let pill = Rect::from_min_size(rect.min + vec2(0.0, 2.0), vec2(width, PILL_HEIGHT));
    ui.painter().rect_filled(pill, 9.0, Color32::from_gray(70));
    let mut clicked = None;
    for (index, tool) in tools.iter().enumerate() {
        let cell = Rect::from_min_size(rect.min + vec2(TOOL_WIDTH * index as f32, 0.0), vec2(TOOL_WIDTH, TOOL_HEIGHT));
        let segment = Rect::from_min_size(cell.min + vec2(0.0, 2.0), vec2(TOOL_WIDTH, PILL_HEIGHT));
        let id = ui.scope_id().with(("tool", tool.label));
        let response = ui.interact(cell, id, if tool.built { Sense::click() } else { Sense::hover() });
        let painter = ui.painter();
        if tool.selected {
            painter.rect_filled(segment.shrink(2.0), 7.0, Color32::from_gray(118));
        } else if response.hovered() {
            painter.rect_filled(segment.shrink(2.0), 7.0, Color32::from_gray(86));
        }
        let ink = if tool.selected {
            Color32::WHITE
        } else if tool.built {
            Color32::from_gray(225)
        } else {
            Color32::from_gray(150)
        };
        paint_face(ui, tool.face, Rect::from_center_size(segment.center(), vec2(16.0, 16.0)), 15.0, ink);
        let label_ink = if tool.selected || tool.built { Color32::from_gray(225) } else { Color32::from_gray(150) };
        ui.painter().text(
            cell.center_bottom() - vec2(0.0, 8.0),
            Align2::CENTER_CENTER,
            tool.label,
            FontId::proportional(10.5),
            label_ink,
        );
        if tool.built {
            if response.clicked() {
                clicked = Some(index);
            }
        } else {
            response.on_hover_text(not_built(tool.label));
        }
    }
    clicked
}

/// A toolbar item for a screen: a status light on a chip above its name. `None` is a screen
/// that is not built yet.
fn screen_light(ui: &mut egui::Ui, label: &str, on: Option<bool>) -> egui::Response {
    let built = on.is_some();
    let (rect, response) =
        ui.allocate_exact_size(vec2(58.0, TOOL_HEIGHT), if built { Sense::click() } else { Sense::hover() });
    let chip = Rect::from_min_size(rect.min + vec2(6.0, 2.0), vec2(TOOL_WIDTH, PILL_HEIGHT));
    let painter = ui.painter();
    painter.rect_filled(chip, 9.0, if response.hovered() { Color32::from_gray(86) } else { Color32::from_gray(70) });
    let light = match on {
        Some(true) => ON,
        Some(false) => OFF,
        None => Color32::from_gray(90),
    };
    painter.circle_filled(chip.center(), 7.0, light);
    let ink = if built { Color32::from_gray(215) } else { Color32::from_gray(125) };
    painter.text(rect.center_bottom() - vec2(0.0, 8.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), ink);
    if built { response } else { response.on_hover_text(not_built(label)) }
}

/// Draws the home window for one frame and returns the commands the operator issued.
///
/// `thumb` returns a slide's thumbnail and whether its text overflows, or `None` while the
/// thumbnail is still being made. It is only called for slides that are on screen.
pub fn home_ui(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(&Slide) -> Option<(TextureId, bool)>,
    preview: Option<TextureId>,
) -> Vec<Command> {
    let mut commands = Vec::new();
    toolbar(ui, show, &mut commands);
    library(ui, show, state, &mut commands);
    live_panel(ui, show, state.output_stopped, preview, &mut commands);
    media_bin(ui);
    slide_area(ui, show, state, thumb, &mut commands);
    commands
}

/// The strip along the top: tools and views on the left, media and screens on the right.
fn toolbar(ui: &mut egui::Ui, show: &Show, commands: &mut Vec<Command>) {
    egui::Panel::top("toolbar").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            tool_group(ui, &[Tool::ghost(Icon::Search, "Search")]);
            tool_group(ui, &[Tool::ghost("Aa", "Text"), Tool::ghost(Icon::Theme, "Theme")]);
            tool_group(
                ui,
                &[
                    Tool { face: Icon::Play.into(), label: "Show", selected: true, built: true },
                    Tool::ghost(Icon::Pencil, "Edit"),
                    Tool::ghost(Icon::Reflow, "Reflow"),
                    Tool::ghost(Icon::Book, "Bible"),
                    Tool::ghost(Icon::More, "More"),
                ],
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                tool_group(ui, &[Tool::ghost(Icon::Status, "Status")]);
                ui.separator();
                screen_light(ui, "Stage", None);
                let on = show.output_enabled();
                let audience = screen_light(ui, "Audience", Some(on)).on_hover_text(if on {
                    "The audience output is on. Click to switch it off."
                } else {
                    "The audience output is off. Click to switch it on."
                });
                if audience.clicked() {
                    commands.push(Command::SetOutputEnabled(!on));
                }
                ui.separator();
                tool_group(ui, &[Tool::ghost(Icon::Live, "Live")]);
                tool_group(ui, &[Tool::ghost(Icon::Look, "Look")]);
                tool_group(ui, &[Tool::ghost(Icon::Picture, "Media")]);
            });
        });
    });
}

/// The left column: libraries and playlists on top, the selected library's items in the
/// middle with a filter box under them, and the media bin's list at the bottom.
fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    egui::Panel::left("library").resizable(false).exact_size(240.0).frame(panel_frame(SIDE, 0)).show(ui, |ui| {
        let total = show.presentations().len();

        egui::Panel::bottom("media list").resizable(false).exact_size(MEDIA_BIN).frame(panel_frame(SIDE, 8)).show(
            ui,
            |ui| {
                section_label(ui, "MEDIA");
                ui.add_space(2.0);
                heading_with(ui, "Playlists", &[(Icon::Plus, "Add a media playlist")]);
                ui.label(egui::RichText::new("No media playlists yet").size(11.5).color(DIM));
            },
        );
        egui::Panel::bottom("library filter").resizable(false).frame(panel_frame(SIDE, 8)).show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Filter").desired_width(f32::INFINITY));
        });

        Frame::new().inner_margin(8).show(ui, |ui| {
            heading_with(ui, "LIBRARY", &[(Icon::ChevronUp, "Collapse"), (Icon::Plus, "Add a library")]);
            // One library for now: everything that was opened.
            list_row(ui, Icon::Library, "Songs", "", false);
            ui.add_space(6.0);
            section_label(ui, "PLAYLIST");
            ui.label(egui::RichText::new("No playlists yet").size(11.5).color(DIM));
        });
        ui.separator();

        Frame::new().inner_margin(8).show(ui, |ui| {
            let shown = show.presentations().iter().filter(|p| matches_filter(&p.title, &state.filter)).count();
            let items = if shown == 1 { "1 ITEM".to_owned() } else { format!("{shown} ITEMS") };
            heading_with(ui, &items, &[(Icon::Plus, "Add an item")]);
            ui.add_space(2.0);
            egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
                for (index, presentation) in show.presentations().iter().enumerate() {
                    if !matches_filter(&presentation.title, &state.filter) {
                        continue;
                    }
                    let selected = index == show.selected();
                    if list_row(ui, Icon::Document, &presentation.title, "Songs", selected).clicked() {
                        commands.push(Command::Select(index));
                    }
                }
                if total == 0 {
                    ui.weak("No songs loaded.");
                } else if shown == 0 {
                    ui.weak("No song matches.");
                }
            });
        });
    });
}

/// One button in the column beside the preview. Each clears one layer from the screen and is
/// lit while that layer is showing. Returns true when a working one is clicked.
fn layer_button(ui: &mut egui::Ui, icon: Icon, lit: bool, built: bool, hint: &str) -> bool {
    let (rect, response) =
        ui.allocate_exact_size(vec2(26.0, 19.0), if built { Sense::click() } else { Sense::hover() });
    let fill = if lit {
        Color32::from_rgb(120, 34, 34)
    } else if response.hovered() {
        Color32::from_gray(58)
    } else {
        Color32::from_gray(42)
    };
    ui.painter().rect_filled(rect, 4.0, fill);
    let ink = if lit {
        Color32::WHITE
    } else if built {
        Color32::from_gray(180)
    } else {
        DIM
    };
    icons::paint(ui.painter(), icon, Rect::from_center_size(rect.center(), vec2(13.0, 13.0)), ink);
    let hint = if built { hint.to_owned() } else { not_built(hint) };
    response.on_hover_text(hint).clicked() && built
}

/// The right column: the preview with the clear column beside it, the screen it shows, the
/// playback controls, and the tabs with the stage section.
fn live_panel(
    ui: &mut egui::Ui,
    show: &Show,
    output_stopped: bool,
    preview: Option<TextureId>,
    commands: &mut Vec<Command>,
) {
    egui::Panel::right("live").resizable(false).exact_size(320.0).frame(panel_frame(SIDE, 8)).show(ui, |ui| {
        let live = show.live().is_some();
        let (strip, gap) = (26.0, 6.0);
        let width = ui.available_width() - strip - gap;
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(gap, 1.5);
            let (rect, _) = ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), Sense::hover());
            ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
            if let Some(texture) = preview {
                egui::Image::from_texture((texture, rect.size())).paint_at(ui, rect);
            }
            if !show.output_enabled() {
                ui.painter().rect_filled(rect, 4.0, Color32::from_black_alpha(190));
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "Audience output is off",
                    FontId::proportional(14.0),
                    OFF,
                );
            }
            let edge = if !show.output_enabled() {
                OFF
            } else if live {
                ON
            } else {
                Color32::from_gray(70)
            };
            ui.painter().rect_stroke(rect, 4.0, Stroke::new(2.0, edge), StrokeKind::Outside);

            ui.vertical(|ui| {
                if layer_button(ui, Icon::Close, live, true, "Clear everything from the screen (F1)") {
                    commands.push(Command::ClearAll);
                }
                layer_button(ui, Icon::Note, false, false, "Clear audio");
                layer_button(ui, Icon::Message, false, false, "Clear messages");
                layer_button(ui, Icon::Layers, false, false, "Clear props");
                layer_button(ui, Icon::Megaphone, false, false, "Clear announcements");
                if layer_button(ui, Icon::Slide, live, true, "Clear the slide: take the words off the screen (C)") {
                    commands.push(Command::Clear(Layer::Slides));
                }
                layer_button(ui, Icon::Picture, false, false, "Clear media");
                layer_button(ui, Icon::Camera, false, false, "Clear video input");
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Audience screen").size(11.5).color(Color32::from_gray(150)))
                .on_hover_text(not_built("Choose which screen the preview shows"));
            let (rect, _) = ui.allocate_exact_size(vec2(10.0, 10.0), Sense::hover());
            icons::paint(ui.painter(), Icon::ChevronDown, rect, Color32::from_gray(150));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| match show.live_slide() {
                Some((presentation, _)) => {
                    let at = show.live().map_or(0, |l| l.slide + 1);
                    ui.label(egui::RichText::new(format!("{at} / {}", presentation.slide_count())).size(11.5));
                }
                None => {
                    ui.label(egui::RichText::new("Nothing on screen").size(11.5).color(DIM));
                }
            });
        });
        if output_stopped {
            ui.label(egui::RichText::new("The output has stopped. Quit and start again.").strong().color(OFF));
        }
        ui.separator();

        // Playback: which layer's media the controls drive, then the transport.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.add_space(ui.available_width() / 2.0 - 36.0);
            ghost(ui, Icon::Slide, "Presentation playback");
            ghost(ui, Icon::Megaphone, "Announcement playback");
            ghost(ui, Icon::Note, "Audio playback");
        });
        ui.vertical_centered(|ui| ui.label(egui::RichText::new("No media playing").size(11.5).color(DIM)));
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
        ui.painter().rect_filled(bar.shrink2(vec2(0.0, 3.5)), 2.0, Color32::from_gray(60));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.label(egui::RichText::new("--:--").size(11.0).color(DIM));
            ui.add_space((ui.available_width() - 150.0 - 30.0).max(0.0) / 2.0);
            ghost(ui, Icon::Back, "Back 15 seconds");
            ghost(ui, Icon::Pause, "Play or pause");
            ghost(ui, Icon::Forward, "Forward 15 seconds");
            ghost(ui, Icon::ToEnd, "Jump to the last 30 seconds");
            ghost(ui, Icon::Flag, "Markers");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new("--:--").size(11.0).color(DIM));
            });
        });
        ui.add_space(4.0);

        // The tabs: audio, stage, timers, messages, props, macros. Stage is the one shown.
        let tabs: [(Face, &str); 6] = [
            (Face::Icon(Icon::Note), "Audio"),
            (Face::Icon(Icon::Stage), "Stage"),
            (Face::Icon(Icon::Clock), "Timers"),
            (Face::Icon(Icon::Message), "Messages"),
            (Face::Icon(Icon::Layers), "Props"),
            (Face::Text("[M]"), "Macros"),
        ];
        let tab_width = ui.available_width() / tabs.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, (face, name)) in tabs.iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(vec2(tab_width, 28.0), Sense::hover());
                let selected = index == 1;
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    if selected { ACCENT.gamma_multiply(0.75) } else { Color32::from_gray(46) },
                );
                let ink = if selected { Color32::WHITE } else { Color32::from_gray(150) };
                paint_face(ui, *face, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), 12.0, ink);
                response.on_hover_text(not_built(name));
            }
        });
        ui.add_space(6.0);

        egui::Panel::bottom("stage message").resizable(false).frame(Frame::new()).show(ui, |ui| {
            section_label(ui, "STAGE MESSAGE");
            ui.add_space(2.0);
            let mut nothing = String::new();
            ui.add_enabled(false, egui::TextEdit::multiline(&mut nothing).desired_rows(3).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Shown on the stage screen").size(10.5).color(DIM));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled(false, egui::Button::new("Show"))
                        .on_disabled_hover_text(not_built("Show the stage message"));
                });
            });
        });

        heading_with(ui, "STAGE", &[(Icon::Plus, "Add a stage screen"), (Icon::Pencil, "Edit stage layouts")]);
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Stage screen").size(12.5).color(DIM));
                ui.label(egui::RichText::new("No layout").size(11.5).color(DIM));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(72.0, 40.0), Sense::hover());
                ui.painter().rect_filled(rect, 3.0, Color32::BLACK);
                ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.0, Color32::from_gray(60)), StrokeKind::Inside);
            });
        });
    });
}

/// The transition buttons and duration that sit at the left of a bottom bar.
fn transition_controls(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 2.0;
    ghost(ui, Icon::Plus, "Add");
    ui.add_space(6.0);
    ghost(ui, Icon::Cut, "Cut");
    ghost(ui, Icon::Dissolve, "Dissolve");
    ghost(ui, Icon::Transition, "More transitions");
    ui.add_space(6.0);
    ui.label(egui::RichText::new("0.4s").size(11.5).color(DIM)).on_hover_text(not_built("Transition time"));
}

/// The media bin's thumbnails, along the bottom of the middle column.
fn media_bin(ui: &mut egui::Ui) {
    egui::Panel::bottom("media bin").resizable(false).exact_size(MEDIA_BIN).frame(panel_frame(CANVAS, 0)).show(
        ui,
        |ui| {
            egui::Panel::top("media path").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ghost(ui, Icon::Sidebar, "Show or hide the media list");
                    ghost(ui, Icon::ChevronLeft, "Back");
                    ghost(ui, Icon::ChevronRight, "Forward");
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Playlists").size(12.0).color(DIM));
                });
            });
            egui::Panel::bottom("media bar").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    transition_controls(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        let mut size = 0.5_f32;
                        ui.add_enabled(false, egui::Slider::new(&mut size, 0.0..=1.0).show_value(false));
                        ghost(ui, Icon::Rows, "List view");
                        ghost(ui, Icon::Grid, "Grid view");
                        ui.add_space(8.0);
                        let mut nothing = String::new();
                        ui.add_enabled(
                            false,
                            egui::TextEdit::singleline(&mut nothing).hint_text("Filter").desired_width(160.0),
                        );
                    });
                });
            });
            ui.centered_and_justified(|ui| {
                ui.label(egui::RichText::new("Backgrounds and videos will appear here (not built yet)").color(DIM));
            });
        },
    );
}

/// The middle: the library's title bar, the selected song's bar, and its slides as a grid of
/// thumbnails with a group bar under each.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(&Slide) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        egui::Panel::top("library title").resizable(false).frame(panel_frame(TITLE_STRIP, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Songs").strong().size(15.0).color(Color32::WHITE));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| ghost(ui, Icon::Gear, "View settings"));
            });
        });
        egui::Panel::bottom("slide bar").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                transition_controls(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.add(egui::Slider::new(&mut state.thumb_width, 120.0..=480.0).show_value(false))
                        .on_hover_text("Slide size");
                    ghost(ui, Icon::Options, "View options");
                    ghost(ui, Icon::Rows, "Table view");
                    ghost(ui, "T", "Text view");
                    let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::hover());
                    icons::paint(
                        ui.painter(),
                        Icon::Grid,
                        Rect::from_center_size(rect.center(), vec2(14.0, 14.0)),
                        ACCENT,
                    );
                    response.on_hover_text("Grid view");
                });
            });
        });

        let Some(presentation) = show.presentations().get(show.selected()) else {
            ui.centered_and_justified(|ui| ui.weak("Open a song to see its slides."));
            return;
        };
        egui::Panel::top("song header").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                icons::paint(ui.painter(), Icon::Document, rect, ACCENT);
                ui.label(egui::RichText::new(presentation.title.as_str()).size(13.5));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ghost(ui, Icon::Transition, "Transition for this presentation");
                    ghost(ui, Icon::Target, "Destination");
                    ui.add_space(6.0);
                    ghost(ui, Icon::Timeline, "Timeline");
                    ghost(ui, Icon::Arrangement, "Arrangements");
                    ghost(ui, Icon::Notes, "Operator notes");
                    ui.add_space(6.0);
                    ghost(ui, Icon::SlideShow, "Slide show");
                });
            });
        });

        let width = state.thumb_width;
        let image_size = vec2(width, width * 9.0 / 16.0);
        let bar_height = 20.0;
        let cell = vec2(width, image_size.y + bar_height);
        egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]).show(ui, |ui| {
            ui.add_space(12.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                ui.add_space(0.0);
                for (index, (group, slide)) in presentation.slides().enumerate() {
                    let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                    let here = SlideRef { presentation: show.selected(), slide: index };
                    if response.clicked() {
                        commands.push(Command::GoTo(here));
                    }
                    // Slides scrolled out of view cost nothing: no thumbnail is asked for.
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let image = Rect::from_min_size(rect.min, image_size);
                    let painter = ui.painter();
                    painter.rect_filled(image, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::BLACK);
                    match thumb(slide) {
                        Some((texture, overflow)) => {
                            egui::Image::from_texture((texture, image_size)).paint_at(ui, image);
                            if overflow {
                                let mark = image.right_top() + vec2(-14.0, 14.0);
                                painter.circle_filled(mark, 9.0, Color32::from_rgb(240, 190, 40));
                                painter.text(
                                    mark,
                                    Align2::CENTER_CENTER,
                                    "!",
                                    FontId::proportional(14.0),
                                    Color32::BLACK,
                                );
                            }
                        }
                        None => ui.ctx().request_repaint(),
                    }
                    // The bar under the slide: its number and its group, in the group's colour.
                    let bar = Rect::from_min_max(pos2(rect.min.x, image.max.y), rect.max);
                    painter.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 3, se: 3 }, color32(group.color));
                    let ink = text_on(group.color);
                    painter.text(
                        bar.left_center() + vec2(7.0, 0.0),
                        Align2::LEFT_CENTER,
                        (index + 1).to_string(),
                        FontId::proportional(13.0),
                        ink,
                    );
                    painter.text(
                        bar.right_center() - vec2(7.0, 0.0),
                        Align2::RIGHT_CENTER,
                        group.name.as_str(),
                        FontId::proportional(12.0),
                        ink,
                    );

                    if show.live() == Some(here) {
                        painter.rect_stroke(rect, 3.0, Stroke::new(3.0, ACCENT), StrokeKind::Outside);
                    } else if response.hovered() {
                        painter.rect_stroke(rect, 3.0, Stroke::new(1.0, Color32::from_gray(150)), StrokeKind::Outside);
                    }
                }
            });
            ui.add_space(12.0);
        });
    });
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Event, Key, Modifiers, RawInput};
    use pc_model::parse_song_text;

    use super::*;

    fn input(key: Option<Key>) -> RawInput {
        RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1280.0, 800.0))),
            events: key
                .map(|key| Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                })
                .into_iter()
                .collect(),
            ..RawInput::default()
        }
    }

    /// Runs one frame and returns the commands and how many thumbnails were asked for.
    fn frame(ctx: &egui::Context, show: &Show, key: Option<Key>) -> (Vec<Command>, usize) {
        let mut commands = Vec::new();
        let mut asked = 0;
        let mut state = HomeState::default();
        let output = ctx.run_ui(input(key), |ui| {
            asked = 0;
            // As in the app: a frame may run twice and only the first run sees the input.
            commands.extend(home_ui(
                ui,
                show,
                &mut state,
                &mut |_| {
                    asked += 1;
                    None
                },
                None,
            ));
        });
        output.drop_without_applying_deltas();
        (commands, asked)
    }

    fn show(slides: usize) -> Show {
        let text = (0..slides).map(|i| format!("Slide {i}\n")).collect::<Vec<_>>().join("\n");
        Show::new(vec![Arc::new(parse_song_text("Song", &text))])
    }

    #[test]
    fn filter_matches_any_part_ignoring_case() {
        assert!(matches_filter("Morning Song", ""));
        assert!(matches_filter("Morning Song", "  "));
        assert!(matches_filter("Morning Song", "song"));
        assert!(matches_filter("Песня Утра", "утра"));
        assert!(!matches_filter("Morning Song", "evening"));
    }

    #[test]
    fn group_bar_text_contrasts_with_its_colour() {
        assert_eq!(text_on(Rgba::rgb(250, 240, 200)), Color32::BLACK);
        assert_eq!(text_on(Rgba::rgb(20, 40, 120)), Color32::WHITE);
    }

    #[test]
    fn empty_show_draws_without_panicking() {
        let ctx = egui::Context::default();
        assert!(frame(&ctx, &Show::new(vec![]), None).0.is_empty());
    }

    #[test]
    fn offscreen_thumbnails_are_not_requested() {
        let ctx = egui::Context::default();
        let show = show(400);
        frame(&ctx, &show, None);
        let (_, asked) = frame(&ctx, &show, None);
        assert!(asked > 0, "visible thumbnails should be requested");
        assert!(asked < 80, "asked for {asked} thumbnails of 400");
    }
}
