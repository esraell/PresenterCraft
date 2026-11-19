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
use pc_model::{Rgba, Role, Slide};

use crate::icons::{self, Icon};

/// The outline of the media item that is on screen.
const MEDIA_LIVE: Color32 = Color32::from_rgb(240, 138, 36);
/// The one accent colour: the live slide's outline and selections.
pub const ACCENT: Color32 = Color32::from_rgb(64, 156, 255);
/// Bars around the slide area and the media bin.
const BAR: Color32 = Color32::from_rgb(18, 18, 18);
/// The toolbar across the top.
const TOOLBAR: Color32 = Color32::from_rgb(34, 40, 44);
/// The pills that hold toolbar items, and text fields.
const FIELD: Color32 = Color32::from_rgb(36, 43, 45);
const PILL: Color32 = Color32::from_rgb(28, 33, 36);
/// The darkest strips and the lines between buttons.
const DEEP: Color32 = Color32::from_rgb(18, 18, 18);
/// The area behind the preview and the playback controls.
const WELL: Color32 = Color32::from_rgb(46, 46, 46);
/// A layer that is on screen, and the strip beside the preview while anything is.
const LIVE_RED: Color32 = Color32::from_rgb(87, 26, 27);
const TAB: Color32 = Color32::from_rgb(85, 85, 85);
const TAB_BLUE: Color32 = Color32::from_rgb(1, 121, 255);
const SCRUB_EMPTY: Color32 = Color32::from_rgb(63, 63, 63);
/// A selected row in a list.
const ROW_SELECTED: Color32 = Color32::from_rgb(88, 88, 88);
const SIDE: Color32 = Color32::from_rgb(70, 70, 70);
const CANVAS: Color32 = Color32::from_rgb(30, 30, 30);
const ON: Color32 = Color32::from_rgb(83, 194, 43);
const OFF: Color32 = Color32::from_rgb(210, 70, 70);
const DIM: Color32 = Color32::from_gray(150);
/// The strip that names what the slide area is showing.
const TITLE_STRIP: Color32 = Color32::from_rgb(127, 127, 127);
/// Corner radius of the cards in the right column.
const CARD: u8 = 10;
/// Space between the cards in the right column and around them.
const GAP: f32 = 8.0;
/// Height of the media bin, shared by its list (left column) and its thumbnails (middle).
const MEDIA_BIN: f32 = 190.0;

pub struct HomeState {
    /// Displayed thumbnail width in points.
    pub thumb_width: f32,
    /// Text typed into the library's filter box.
    pub filter: String,
    /// Set by the app when the output has stopped drawing, so the operator is told.
    pub output_stopped: bool,
    /// Displayed media thumbnail width in points.
    pub media_width: f32,
    /// True while a file is being dragged over the window.
    pub drop_hover: bool,
    /// A short message for the operator, shown in the media bin (a file that could not be
    /// added, a song that could not be saved).
    pub notice: Option<String>,
    /// The slide being edited, while the editor is open.
    pub editing: Option<EditDraft>,
    /// An edit the operator has just saved. The app takes it and applies it.
    pub finished_edit: Option<EditDraft>,
}

/// The words of one slide as they are being edited.
#[derive(Clone, Debug, PartialEq)]
pub struct EditDraft {
    pub at: SlideRef,
    pub primary: String,
    pub translation: String,
}

impl EditDraft {
    /// Starts an edit of a slide with its current words.
    pub fn of(show: &Show, at: SlideRef) -> Option<EditDraft> {
        let (_, slide) = show.presentations().get(at.presentation)?.slide(at.slide)?;
        Some(EditDraft {
            at,
            primary: slide.text_for(&Role::primary()).unwrap_or_default().to_owned(),
            translation: slide.text_for(&Role::translation()).unwrap_or_default().to_owned(),
        })
    }

    /// A slide needs words in at least one box.
    pub fn can_save(&self) -> bool {
        !self.primary.trim().is_empty() || !self.translation.trim().is_empty()
    }
}

/// Which thumbnail the window is asking for.
#[derive(Clone, Copy)]
pub enum ThumbOf<'a> {
    Slide(&'a Slide),
    /// A media item, by its index in the show.
    Media(usize),
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState {
            thumb_width: 170.0,
            filter: String::new(),
            output_stopped: false,
            media_width: 150.0,
            drop_hover: false,
            notice: None,
            editing: None,
            finished_edit: None,
        }
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
        painter.rect_filled(rect, 4.0, ROW_SELECTED);
    } else if response.hovered() {
        painter.rect_filled(rect, 4.0, Color32::from_gray(80));
    }
    let ink = if selected { Color32::WHITE } else { Color32::from_gray(215) };
    let icon_rect = Rect::from_center_size(rect.left_center() + vec2(15.0, 0.0), vec2(14.0, 14.0));
    icons::paint(painter, icon, icon_rect, Color32::from_rgb(236, 122, 60));
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
    ui.painter().rect_filled(pill, 9.0, PILL);
    let mut clicked = None;
    for (index, tool) in tools.iter().enumerate() {
        let cell = Rect::from_min_size(rect.min + vec2(TOOL_WIDTH * index as f32, 0.0), vec2(TOOL_WIDTH, TOOL_HEIGHT));
        let segment = Rect::from_min_size(cell.min + vec2(0.0, 2.0), vec2(TOOL_WIDTH, PILL_HEIGHT));
        let id = ui.scope_id().with(("tool", tool.label));
        let response = ui.interact(cell, id, if tool.built { Sense::click() } else { Sense::hover() });
        let painter = ui.painter();
        if tool.selected {
            painter.rect_filled(segment.shrink(2.0), 7.0, Color32::from_rgb(86, 94, 99));
        } else if response.hovered() {
            painter.rect_filled(segment.shrink(2.0), 7.0, Color32::from_rgb(48, 56, 60));
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
    painter.rect_filled(chip, 9.0, if response.hovered() { Color32::from_rgb(48, 56, 60) } else { PILL });
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
/// `thumb` returns the thumbnail of a slide or a media item and whether its text overflows, or
/// `None` while the thumbnail is still being made. It is only called for what is on screen.
pub fn home_ui(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    preview: Option<TextureId>,
) -> Vec<Command> {
    let mut commands = Vec::new();
    // Text boxes are dark wells in the lighter side columns.
    ui.visuals_mut().extreme_bg_color = FIELD;
    toolbar(ui, show, state, &mut commands);
    library(ui, show, state, &mut commands);
    live_panel(ui, show, state.output_stopped, preview, &mut commands);
    media_bin(ui, show, state, thumb, &mut commands);
    slide_area(ui, show, state, thumb, &mut commands);
    editor(ui, show, state);
    commands
}

/// The strip along the top: tools and views on the left, media and screens on the right.
fn toolbar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    egui::Panel::top("toolbar").resizable(false).frame(panel_frame(TOOLBAR, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            tool_group(ui, &[Tool::ghost(Icon::Search, "Search")]);
            tool_group(ui, &[Tool::ghost("Aa", "Text"), Tool::ghost(Icon::Theme, "Theme")]);
            let views = tool_group(
                ui,
                &[
                    Tool { face: Icon::Play.into(), label: "Show", selected: state.editing.is_none(), built: true },
                    Tool { face: Icon::Pencil.into(), label: "Edit", selected: state.editing.is_some(), built: true },
                    Tool::ghost(Icon::Reflow, "Reflow"),
                    Tool::ghost(Icon::Book, "Bible"),
                    Tool::ghost(Icon::More, "More"),
                ],
            );
            match views {
                Some(0) => state.editing = None,
                // Edit opens the slide that was last on screen, or the song's first slide.
                Some(1) if state.editing.is_none() => {
                    let at = show.cursor().unwrap_or(SlideRef { presentation: show.selected(), slide: 0 });
                    state.editing = EditDraft::of(show, at);
                }
                _ => {}
            }
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
                let count = show.media().len();
                list_row(ui, Icon::Picture, "All media", &count.to_string(), true);
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
/// lit while that layer is showing. The buttons sit flush against each other with a thin dark
/// line between them. Returns true when a working one is clicked.
fn layer_button(
    ui: &mut egui::Ui,
    rect: Rect,
    corners: CornerRadius,
    icon: Icon,
    lit: bool,
    built: bool,
    hint: &str,
) -> bool {
    let id = ui.scope_id().with(("layer", hint));
    let response = ui.interact(rect, id, if built { Sense::click() } else { Sense::hover() });
    let fill = if lit {
        LIVE_RED
    } else if response.hovered() {
        Color32::from_rgb(52, 60, 63)
    } else {
        FIELD
    };
    let painter = ui.painter();
    painter.rect_filled(rect, corners, fill);
    if corners.se == 0 {
        painter.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, DEEP));
    }
    painter.line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, DEEP));
    let ink = if lit || built { Color32::from_gray(225) } else { Color32::from_gray(170) };
    icons::paint(painter, icon, Rect::from_center_size(rect.center(), vec2(16.0, 16.0)), ink);
    let hint = if built { hint.to_owned() } else { not_built(hint) };
    response.on_hover_text(hint).clicked() && built
}

/// The right column, top to bottom: the preview with the clear strip and the layer column
/// beside it, the bar naming the screen, the playback controls, the tab strip, the stage
/// section, and the stage message at the bottom.
fn live_panel(
    ui: &mut egui::Ui,
    show: &Show,
    output_stopped: bool,
    preview: Option<TextureId>,
    commands: &mut Vec<Command>,
) {
    egui::Panel::right("live").resizable(false).exact_size(300.0).frame(panel_frame(SIDE, 0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        let live = show.live().is_some();
        let media_live = show.live_media().is_some();
        let any_live = show.anything_live();
        let full_width = ui.available_width();

        // The preview block: preview, then the clear strip, then seven layer buttons.
        let layer_height = 29.0;
        let block_height = layer_height * 7.0;
        let (layer_width, strip_width) = (44.0, 28.0);
        ui.add_space(GAP);
        let (outer, _) = ui.allocate_exact_size(vec2(full_width, block_height), Sense::hover());
        let block = outer.shrink2(vec2(GAP, 0.0));
        let preview_area = Rect::from_min_max(block.min, pos2(block.max.x - layer_width - strip_width, block.max.y));
        let strip =
            Rect::from_min_max(pos2(preview_area.max.x, block.min.y), pos2(block.max.x - layer_width, block.max.y));
        ui.painter().rect_filled(preview_area, CornerRadius { nw: CARD, sw: CARD, ne: 0, se: 0 }, WELL);
        let picture_height = preview_area.width() * 9.0 / 16.0;
        let picture_width = preview_area.width() - 12.0;
        let picture_height = picture_height.min(picture_width * 9.0 / 16.0);
        let picture = Rect::from_center_size(preview_area.center(), vec2(picture_width, picture_height));
        ui.painter().rect_filled(picture, 6.0, Color32::BLACK);
        if let Some(texture) = preview {
            egui::Image::from_texture((texture, picture.size())).corner_radius(6.0).paint_at(ui, picture);
        }
        if !show.output_enabled() {
            ui.painter().rect_filled(picture, 6.0, Color32::from_black_alpha(190));
            ui.painter().text(
                picture.center(),
                Align2::CENTER_CENTER,
                "Audience output is off",
                FontId::proportional(13.0),
                OFF,
            );
        }

        // The strip is red while anything is on screen; its round button clears everything.
        ui.painter().rect_filled(strip, 0.0, if any_live { LIVE_RED } else { WELL });
        let clear_all = ui.interact(strip, ui.scope_id().with("clear all"), Sense::click());
        let knob = strip.center();
        let knob_fill = if clear_all.hovered() || any_live { Color32::WHITE } else { Color32::from_gray(150) };
        ui.painter().circle_filled(knob, 9.0, knob_fill);
        icons::paint(
            ui.painter(),
            Icon::Close,
            Rect::from_center_size(knob, vec2(11.0, 11.0)),
            if any_live { LIVE_RED } else { WELL },
        );
        if clear_all.on_hover_text("Clear everything from the screen (F1)").clicked() {
            commands.push(Command::ClearAll);
        }

        let layers: [(Icon, &str, bool, bool); 7] = [
            (Icon::Note, "Clear audio", false, false),
            (Icon::Message, "Clear messages", false, false),
            (Icon::Layers, "Clear props", false, false),
            (Icon::Megaphone, "Clear announcements", false, false),
            (Icon::Slide, "Clear the slide: take the words off the screen (C)", live, true),
            (Icon::Picture, "Clear the media: take the picture off the screen", media_live, true),
            (Icon::Camera, "Clear video input", false, false),
        ];
        for (index, (icon, hint, lit, built)) in layers.into_iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(block.max.x - layer_width, block.min.y + layer_height * index as f32),
                vec2(layer_width, layer_height),
            );
            let corners = match index {
                0 => CornerRadius { ne: CARD, nw: 0, sw: 0, se: 0 },
                6 => CornerRadius { se: CARD, nw: 0, sw: 0, ne: 0 },
                _ => CornerRadius::ZERO,
            };
            if layer_button(ui, rect, corners, icon, lit, built, hint) {
                match icon {
                    Icon::Slide => commands.push(Command::Clear(Layer::Slides)),
                    Icon::Picture => commands.push(Command::Clear(Layer::Media)),
                    _ => {}
                }
            }
        }

        // The bar that names the screen the preview shows.
        ui.add_space(GAP);
        let (outer, bar_response) = ui.allocate_exact_size(vec2(full_width, 26.0), Sense::hover());
        let bar = outer.shrink2(vec2(GAP, 0.0));
        ui.painter().rect_filled(bar, 13.0, DEEP);
        let name = ui.painter().text(
            bar.left_center() + vec2(14.0, 0.0),
            Align2::LEFT_CENTER,
            "Audience screen",
            FontId::proportional(12.0),
            Color32::from_gray(170),
        );
        icons::paint(
            ui.painter(),
            Icon::ChevronDown,
            Rect::from_center_size(pos2(name.max.x + 10.0, bar.center().y), vec2(9.0, 9.0)),
            Color32::from_gray(200),
        );
        let (status, status_ink) = if output_stopped {
            ("Output stopped: quit and start again".to_owned(), OFF)
        } else {
            match show.live_slide() {
                Some((presentation, _)) => (
                    format!("{} / {}", show.live().map_or(0, |l| l.slide + 1), presentation.slide_count()),
                    Color32::from_gray(170),
                ),
                None => (String::new(), DIM),
            }
        };
        ui.painter().text(
            bar.right_center() - vec2(14.0, 0.0),
            Align2::RIGHT_CENTER,
            status,
            FontId::proportional(11.5),
            status_ink,
        );
        bar_response.on_hover_text(not_built("Choose which screen the preview shows"));

        // Playback: which layer the controls drive, what is playing, the scrub bar, the buttons.
        Frame::new()
            .fill(WELL)
            .corner_radius(CARD)
            .outer_margin(egui::Margin { left: 8, right: 8, top: 8, bottom: 8 })
            .inner_margin(egui::Margin { left: 14, right: 14, top: 8, bottom: 10 })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 4.0);
                let width = ui.available_width();
                let (row, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
                for (index, (icon, what)) in [
                    (Icon::Slide, "Presentation playback"),
                    (Icon::Megaphone, "Announcement playback"),
                    (Icon::Note, "Audio playback"),
                ]
                .into_iter()
                .enumerate()
                {
                    let centre = row.center() + vec2((index as f32 - 1.0) * 26.0, 0.0);
                    let ink = if index == 0 { TAB_BLUE } else { Color32::from_gray(170) };
                    icons::paint(ui.painter(), icon, Rect::from_center_size(centre, vec2(15.0, 15.0)), ink);
                    ui.interact(
                        Rect::from_center_size(centre, vec2(22.0, 20.0)),
                        ui.scope_id().with(("playback", index)),
                        Sense::hover(),
                    )
                    .on_hover_text(not_built(what));
                }
                ui.add_space(4.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("No media playing").size(12.0).color(Color32::from_gray(150)))
                });
                ui.add_space(4.0);
                let (scrub, _) = ui.allocate_exact_size(vec2(width, 10.0), Sense::hover());
                ui.painter().rect_filled(scrub.shrink2(vec2(0.0, 1.0)), 4.0, SCRUB_EMPTY);
                ui.add_space(2.0);
                let (row, _) = ui.allocate_exact_size(vec2(width, 26.0), Sense::hover());
                let time = FontId::proportional(11.5);
                ui.painter().text(row.left_top(), Align2::LEFT_TOP, "--:--", time.clone(), Color32::from_gray(130));
                ui.painter().text(row.right_top(), Align2::RIGHT_TOP, "--:--", time, Color32::from_gray(130));
                for (index, (icon, what)) in [
                    (Icon::Back, "Back 15 seconds"),
                    (Icon::Pause, "Play or pause"),
                    (Icon::Forward, "Forward 15 seconds"),
                    (Icon::ToEnd, "Jump to the last 30 seconds"),
                    (Icon::Flag, "Markers"),
                ]
                .into_iter()
                .enumerate()
                {
                    let centre = pos2(row.center().x + (index as f32 - 2.0) * 29.0, row.max.y - 9.0);
                    icons::paint(
                        ui.painter(),
                        icon,
                        Rect::from_center_size(centre, vec2(17.0, 17.0)),
                        Color32::from_gray(190),
                    );
                    ui.interact(
                        Rect::from_center_size(centre, vec2(26.0, 22.0)),
                        ui.scope_id().with(("transport", index)),
                        Sense::hover(),
                    )
                    .on_hover_text(not_built(what));
                }
            });

        // The tabs: audio, stage, timers, messages, props, macros. Stage is the one shown.
        let tabs: [(Face, &str); 6] = [
            (Face::Icon(Icon::Note), "Audio"),
            (Face::Icon(Icon::Stage), "Stage"),
            (Face::Icon(Icon::Clock), "Timers"),
            (Face::Icon(Icon::Message), "Messages"),
            (Face::Icon(Icon::Layers), "Props"),
            (Face::Text("[M]"), "Macros"),
        ];
        let (outer, _) = ui.allocate_exact_size(vec2(full_width, 28.0), Sense::hover());
        let strip = outer.shrink2(vec2(GAP, 0.0));
        ui.painter().rect_filled(strip, 9.0, TAB);
        let tab_width = strip.width() / tabs.len() as f32;
        for (index, (face, name)) in tabs.iter().enumerate() {
            let rect = Rect::from_min_size(
                pos2(strip.min.x + tab_width * index as f32, strip.min.y),
                vec2(tab_width, strip.height()),
            );
            let selected = index == 1;
            if selected {
                ui.painter().rect_filled(rect.shrink(2.0), 7.0, TAB_BLUE);
            } else if index > 0 && index != 2 {
                let (top, bottom) = (rect.left_top() + vec2(0.0, 7.0), rect.left_bottom() - vec2(0.0, 7.0));
                ui.painter().line_segment([top, bottom], Stroke::new(1.0, Color32::from_gray(110)));
            }
            paint_face(
                ui,
                *face,
                Rect::from_center_size(rect.center(), vec2(15.0, 15.0)),
                12.0,
                Color32::from_gray(235),
            );
            ui.interact(rect, ui.scope_id().with(("tab", index)), Sense::hover()).on_hover_text(not_built(name));
        }

        ui.add_space(4.0);

        // The stage message sits at the bottom of the column.
        egui::Panel::bottom("stage message").resizable(false).frame(panel_frame(SIDE, 0)).show(ui, |ui| {
            let (line, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
            ui.painter().rect_filled(line, 0.0, Color32::from_gray(48));
            Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 6, bottom: 8 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 5.0);
                ui.label(egui::RichText::new("STAGE MESSAGE").size(11.0).strong().color(Color32::from_gray(160)));
                let (boxed, _) = ui.allocate_exact_size(vec2(ui.available_width(), 84.0), Sense::hover());
                ui.painter().rect_filled(boxed, 9.0, FIELD);
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("Shown on the stage screen").size(10.5).color(Color32::from_gray(150)),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let (button, response) = ui.allocate_exact_size(vec2(52.0, 20.0), Sense::hover());
                        ui.painter().rect_filled(button, 10.0, Color32::from_gray(110));
                        ui.painter().text(
                            button.center(),
                            Align2::CENTER_CENTER,
                            "Show",
                            FontId::proportional(12.0),
                            Color32::from_gray(200),
                        );
                        response.on_hover_text(not_built("Show the stage message"));
                    });
                });
            });
        });

        // The stage section.
        Frame::new().inner_margin(egui::Margin { left: 10, right: 8, top: 5, bottom: 6 }).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 6.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("STAGE").size(11.0).strong().color(Color32::from_gray(160)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ghost(ui, Icon::Plus, "Add a stage screen");
                    ghost(ui, Icon::Pencil, "Edit stage layouts");
                });
            });
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(egui::RichText::new("Stage screen").size(13.5).color(Color32::from_gray(235)));
                    ui.label(egui::RichText::new("No layout").size(12.5).color(Color32::from_gray(160)));
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (chevrons, _) = ui.allocate_exact_size(vec2(12.0, 26.0), Sense::hover());
                    icons::paint(
                        ui.painter(),
                        Icon::ChevronUp,
                        Rect::from_center_size(chevrons.center() - vec2(0.0, 5.0), vec2(8.0, 8.0)),
                        Color32::from_gray(190),
                    );
                    icons::paint(
                        ui.painter(),
                        Icon::ChevronDown,
                        Rect::from_center_size(chevrons.center() + vec2(0.0, 5.0), vec2(8.0, 8.0)),
                        Color32::from_gray(190),
                    );
                    let (thumb, _) = ui.allocate_exact_size(vec2(88.0, 50.0), Sense::hover());
                    ui.painter().rect_filled(thumb, 6.0, Color32::BLACK);
                    ui.painter().rect_stroke(thumb, 6.0, Stroke::new(1.0, Color32::from_gray(40)), StrokeKind::Inside);
                });
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

/// The media bin, along the bottom of the middle column: pictures dropped onto the window,
/// as thumbnails. Clicking one puts it on screen behind the words.
fn media_bin(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
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
                    ui.label(egui::RichText::new("All media").size(12.0).color(Color32::from_gray(200)));
                    if let Some(notice) = &state.notice {
                        ui.add_space(12.0);
                        ui.label(
                            egui::RichText::new(notice.as_str()).size(12.0).color(Color32::from_rgb(255, 150, 120)),
                        );
                    }
                });
            });
            egui::Panel::bottom("media bar").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    transition_controls(ui);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        ui.add(egui::Slider::new(&mut state.media_width, 90.0..=260.0).show_value(false))
                            .on_hover_text("Thumbnail size");
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

            let area = ui.available_rect_before_wrap();
            if show.media().is_empty() {
                ui.centered_and_justified(|ui| {
                    ui.label(egui::RichText::new("Drop pictures here to add them").color(DIM));
                });
            } else {
                let width = state.media_width;
                let image_size = vec2(width, width * 9.0 / 16.0);
                let cell = vec2(width, image_size.y + 18.0);
                egui::ScrollArea::vertical().id_salt("media grid").auto_shrink([false, false]).show(ui, |ui| {
                    ui.add_space(10.0);
                    Frame::new().inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = vec2(12.0, 10.0);
                            for (index, item) in show.media().iter().enumerate() {
                                let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                                if response.clicked() {
                                    commands.push(Command::ShowMedia(index));
                                }
                                if !ui.is_rect_visible(rect) {
                                    continue;
                                }
                                let image = Rect::from_min_size(rect.min, image_size);
                                ui.painter().rect_filled(image, 3.0, Color32::BLACK);
                                match thumb(ThumbOf::Media(index)) {
                                    Some((texture, _)) => {
                                        egui::Image::from_texture((texture, image_size))
                                            .corner_radius(3.0)
                                            .paint_at(ui, image);
                                    }
                                    None => ui.ctx().request_repaint(),
                                }
                                if show.live_media() == Some(index) {
                                    ui.painter().rect_stroke(
                                        image,
                                        3.0,
                                        Stroke::new(2.5, MEDIA_LIVE),
                                        StrokeKind::Outside,
                                    );
                                } else if response.hovered() {
                                    ui.painter().rect_stroke(
                                        image,
                                        3.0,
                                        Stroke::new(1.0, Color32::from_gray(150)),
                                        StrokeKind::Outside,
                                    );
                                }
                                let label = Rect::from_min_max(pos2(rect.min.x, image.max.y + 2.0), rect.max);
                                ui.painter().with_clip_rect(label).text(
                                    label.left_center(),
                                    Align2::LEFT_CENTER,
                                    format!("{}  {}", index + 1, item.name),
                                    FontId::proportional(11.5),
                                    Color32::from_gray(190),
                                );
                                response.on_hover_text(item.name.as_str());
                            }
                        });
                    });
                    ui.add_space(8.0);
                });
            }
            if state.drop_hover {
                let painter = ui.painter();
                painter.rect_filled(area, 0.0, Color32::from_rgba_unmultiplied(1, 121, 255, 40));
                painter.rect_stroke(area.shrink(3.0), 8.0, Stroke::new(2.0, TAB_BLUE), StrokeKind::Inside);
                painter.text(
                    area.center(),
                    Align2::CENTER_CENTER,
                    "Drop to add to the media bin",
                    FontId::proportional(15.0),
                    Color32::WHITE,
                );
            }
        },
    );
}

/// The slide editor: a small window with one box for each language.
fn editor(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let Some(draft) = state.editing.as_mut() else { return };
    let title = show.presentations().get(draft.at.presentation).map_or("", |p| p.title.as_str());
    let mut save = false;
    let mut cancel = false;
    egui::Window::new(format!("Edit slide {}", draft.at.slide + 1))
        .collapsible(false)
        .resizable(false)
        .fade_in(false)
        .fade_out(false)
        .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
        .show(ui.ctx(), |ui| {
            ui.spacing_mut().item_spacing.y = 6.0;
            ui.label(egui::RichText::new(title).color(DIM));
            section_label(ui, "MAIN LANGUAGE");
            ui.add(egui::TextEdit::multiline(&mut draft.primary).desired_rows(4).desired_width(460.0));
            section_label(ui, "TRANSLATION");
            ui.add(egui::TextEdit::multiline(&mut draft.translation).desired_rows(4).desired_width(460.0));
            ui.label(
                egui::RichText::new("Leave a box empty to remove it. Blank lines are taken out.").size(11.0).color(DIM),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add_enabled(draft.can_save(), egui::Button::new("Save")).clicked() {
                        save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if save {
        state.finished_edit = state.editing.take();
    } else if cancel {
        state.editing = None;
    }
}

/// The middle: the library's title bar, the selected song's bar, and its slides as a grid of
/// thumbnails with a group bar under each.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
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
            Frame::new().inner_margin(egui::Margin::symmetric(20, 0)).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                    ui.add_space(0.0);
                    for (index, (group, slide)) in presentation.slides().enumerate() {
                        let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                        let here = SlideRef { presentation: show.selected(), slide: index };
                        if response.clicked() {
                            commands.push(Command::GoTo(here));
                        }
                        // Editing is on the right-click menu, so reaching for it never puts a
                        // slide on screen by accident.
                        response.context_menu(|ui| {
                            if ui.button("Edit words…").clicked() {
                                state.editing = EditDraft::of(show, here);
                                ui.close();
                            }
                        });
                        // Slides scrolled out of view cost nothing: no thumbnail is asked for.
                        if !ui.is_rect_visible(rect) {
                            continue;
                        }
                        let image = Rect::from_min_size(rect.min, image_size);
                        let painter = ui.painter();
                        painter.rect_filled(image, CornerRadius { nw: 3, ne: 3, sw: 0, se: 0 }, Color32::BLACK);
                        match thumb(ThumbOf::Slide(slide)) {
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
                            painter.rect_stroke(
                                rect,
                                3.0,
                                Stroke::new(1.0, Color32::from_gray(150)),
                                StrokeKind::Outside,
                            );
                        }
                    }
                });
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
    fn an_edit_draft_starts_with_the_slides_words() {
        let show = Show::new(vec![Arc::new(parse_song_text("S", "One\n--\nОдин\n\nTwo\n"))]);
        let both = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }).unwrap();
        assert_eq!((both.primary.as_str(), both.translation.as_str()), ("One", "Один"));
        let one = EditDraft::of(&show, SlideRef { presentation: 0, slide: 1 }).unwrap();
        assert_eq!((one.primary.as_str(), one.translation.as_str()), ("Two", ""));
        assert!(EditDraft::of(&show, SlideRef { presentation: 0, slide: 9 }).is_none());
        assert!(EditDraft::of(&show, SlideRef { presentation: 3, slide: 0 }).is_none());
    }

    #[test]
    fn a_draft_with_no_words_cannot_be_saved() {
        let at = SlideRef { presentation: 0, slide: 0 };
        assert!(!EditDraft { at, primary: " \n".into(), translation: String::new() }.can_save());
        assert!(EditDraft { at, primary: String::new(), translation: "Слова".into() }.can_save());
    }

    #[test]
    fn the_window_draws_with_media_a_notice_a_drag_and_the_editor_open() {
        let ctx = egui::Context::default();
        let mut show = show(3);
        for name in ["a.png", "b.png"] {
            show.add_media(pc_model::MediaItem { id: pc_model::Uuid::new_v4(), name: name.into(), path: name.into() });
        }
        show.apply(Command::ShowMedia(1));
        let mut state = HomeState {
            drop_hover: true,
            notice: Some("x.mp4: videos are not supported yet, only pictures.".into()),
            ..HomeState::default()
        };
        state.editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
        let mut media_asked = 0;
        for _ in 0..2 {
            let output = ctx.run_ui(input(None), |ui| {
                home_ui(
                    ui,
                    &show,
                    &mut state,
                    &mut |which| {
                        if matches!(which, ThumbOf::Media(_)) {
                            media_asked += 1;
                        }
                        None
                    },
                    None,
                );
            });
            output.drop_without_applying_deltas();
        }
        assert!(media_asked >= 2, "the two media thumbnails should be asked for");
        assert!(state.editing.is_some(), "drawing must not close the editor");
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
