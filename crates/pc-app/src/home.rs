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
use pc_model::{Rgba, Slide, SlideRect, Template, Theme};
use pc_render::object_frame;

use crate::displays::{DisplayInfo, Placement};
use crate::editor::EditDraft;
use crate::output::NdiStatus;

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

/// How a presentation's slides are laid out in the Show view.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlideView {
    /// Thumbnails in rows.
    #[default]
    Grid,
    /// The same tiles showing only each slide's words, which is quicker to read.
    Easy,
    /// One slide to a row: thumbnail, then its words.
    Table,
}

pub struct HomeState {
    /// How slides are laid out, and how many fit across the grid (2 to 7).
    pub slide_view: SlideView,
    pub columns: usize,
    /// The slide picked out without being shown, by a right-click.
    pub selected_slide: Option<SlideRef>,
    /// Displayed thumbnail width in points.
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
    /// The slide being edited. While this is set the window shows the Edit view.
    pub editing: Option<EditDraft>,
    /// The slide as it was just changed in the Edit view. The app takes this and puts it into
    /// the song.
    pub pending_edit: Option<(SlideRef, Slide)>,
    /// When on, edits to a slide that is on screen go to the output as they are typed.
    pub live_edit: bool,
    /// The font families installed on this computer, for the editor's font list.
    pub fonts: std::sync::Arc<Vec<String>>,
    /// True while the Bible page is showing in place of the slide grid.
    pub bible_open: bool,
    /// True while the Settings window is open, and which of its panes is showing.
    pub settings_open: bool,
    pub settings_pane: usize,
    /// The displays connected now, kept up to date by the app while Settings is open.
    pub displays: Vec<DisplayInfo>,
    /// Where the audience output is now.
    pub output_placement: Placement,
    /// Where the operator has just asked for the audience output to go. The app takes this
    /// and moves the output.
    pub requested_placement: Option<Placement>,
    /// The name the audience screen is sent under over NDI, and what NDI is doing now.
    pub ndi_name: String,
    pub ndi_status: NdiStatus,
    /// True when this build includes NDI.
    pub ndi_available: bool,
    /// The operator has just switched NDI on (with a name) or off. The app takes this.
    pub requested_ndi: Option<Option<String>>,
    /// The Edit view's right-hand panel.
    pub inspector: crate::inspector::InspectorState,
    /// The Theme Editor, while it is open. The layout being worked on is the draft in `editing`.
    pub theme_edit: Option<ThemeEdit>,
    /// Every theme's name, and the places of the ones songs and Bible slides use.
    pub theme_names: Vec<String>,
    pub theme_for_songs: usize,
    pub theme_for_bible: usize,
    /// What the Theme menu or the Theme Editor has just asked for. The app takes this.
    pub theme_request: Option<ThemeRequest>,
    /// A menu open under a toolbar button, and where its top-left corner is.
    pub menu: Option<(ToolMenu, egui::Pos2)>,
    /// True on the frame a menu opened, so the click that opened it does not close it.
    pub menu_fresh: bool,
    /// What is typed in the Bible page's two fields, and its rows of translations.
    pub bible: BiblePage,
}

/// Where a translation's words, or its reference, go on a Bible slide: one of the Bible
/// theme's boxes, or nowhere.
pub type BibleTarget = Option<pc_model::Role>;

/// What a target is called in its menu.
fn target_label(target: &BibleTarget) -> String {
    target.as_ref().map_or_else(|| "None".to_owned(), pc_model::Role::label)
}

/// One translation on the Bible page and where its text goes.
#[derive(Clone, Debug, PartialEq)]
pub struct BibleRow {
    /// Which Bible, as its place on the shelf.
    pub bible: usize,
    pub verse: BibleTarget,
    pub reference: BibleTarget,
}

/// The Bible page: what is typed, the translations chosen, and the passage being shown.
#[derive(Debug)]
pub struct BiblePage {
    pub lookup: String,
    pub search: String,
    pub rows: Vec<BibleRow>,
    pub options: crate::bible::BibleOptions,
    /// The Bibles installed.
    pub shelf: crate::bible::Shelf,
    /// The boxes of the theme Bible slides use, which the Verse and Reference menus offer.
    pub boxes: Vec<pc_model::Role>,
    /// Bibles being downloaded.
    pub downloads: crate::bible_store::Downloads,
    /// The two places among the show's presentations kept for Bible passages. A new passage
    /// goes into the one that is not on screen, so looking up the next verses never changes
    /// what the audience sees.
    pub slots: Option<[usize; 2]>,
    /// Which of the two the page is showing.
    pub showing: usize,
    /// What was typed for the passage now shown, and the choices it was made with.
    pub shown: String,
    pub made_with: String,
    /// A passage just made. The app takes it and puts it in the slot named.
    pub pending: Option<(usize, pc_model::Presentation)>,
    /// What went wrong with the last lookup, for the operator.
    pub message: Option<String>,
    /// Verses found by the last search: the reference to type and the verse's words.
    pub found: Vec<(String, String)>,
}

impl Default for BiblePage {
    fn default() -> BiblePage {
        let shelf = crate::bible::Shelf::shipped();
        let place = |id: &str| shelf.position(id).unwrap_or(0);
        BiblePage {
            lookup: String::new(),
            search: String::new(),
            options: crate::bible::BibleOptions::default(),
            // Two translations side by side is the usual case here: one for each language.
            rows: vec![
                BibleRow {
                    bible: place("kjv"),
                    verse: Some(pc_model::Role::primary()),
                    reference: Some(pc_model::Role::reference()),
                },
                BibleRow { bible: place("syn"), verse: Some(pc_model::Role::translation()), reference: None },
            ],
            shelf,
            boxes: Theme::builtin().boxes.into_iter().map(|style| style.role).collect(),
            downloads: crate::bible_store::Downloads::default(),
            slots: None,
            showing: 0,
            shown: String::new(),
            made_with: String::new(),
            pending: None,
            message: None,
            found: Vec::new(),
        }
    }
}

impl BiblePage {
    /// The most translations that can be shown together.
    pub const MAX_ROWS: usize = 4;

    /// Adds a translation row, up to the limit. Returns true if one was added.
    pub fn add_row(&mut self) -> bool {
        if self.rows.len() >= Self::MAX_ROWS {
            return false;
        }
        self.rows.push(BibleRow { bible: 0, verse: None, reference: None });
        true
    }

    /// The choices a passage is made with, as text, so a change to any of them can be noticed.
    fn choices(&self) -> String {
        let o = &self.options;
        format!(
            "{:?} {} {} {} {:?}",
            self.rows, o.show_verse_numbers, o.break_on_new_verse, o.display_translation, o.reference
        )
    }

    /// Looks up what is typed and readies its slides for the app to take. `live` is the
    /// presentation on screen now, if any.
    pub fn look_up(&mut self, live: Option<usize>) {
        let Some(slots) = self.slots else { return };
        let mut rows = Vec::new();
        for row in &self.rows {
            match self.shelf.get(row.bible) {
                Ok(bible) => rows.push(crate::bible::RowChoice {
                    bible,
                    verse: row.verse.clone(),
                    reference: row.reference.clone(),
                }),
                Err(problem) => {
                    self.message = Some(problem);
                    return;
                }
            }
        }
        match crate::bible::build_passage(&self.lookup, &rows, &self.options) {
            Ok(passage) => {
                // Leave a passage that is on screen alone: use the other slot.
                if live == Some(slots[self.showing]) {
                    self.showing = 1 - self.showing;
                }
                self.pending = Some((slots[self.showing], passage));
                self.shown = self.lookup.clone();
                self.made_with = self.choices();
                self.message = None;
                self.found.clear();
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// Finds verses holding the words typed in Search, in the first translation.
    pub fn find(&mut self) {
        self.found.clear();
        self.message = None;
        let Some(row) = self.rows.first() else { return };
        match self.shelf.get(row.bible) {
            Ok(bible) => {
                for (place, words) in bible.search(&self.search, 60) {
                    self.found.push((bible.label(place, place), words.to_owned()));
                }
                if self.found.is_empty() && !self.search.trim().is_empty() {
                    self.message = Some(format!("Nothing in the {} has those words.", bible.info.name));
                }
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// Removes a translation row, always leaving one. Returns true if one was removed.
    pub fn remove_row(&mut self, index: usize) -> bool {
        if self.rows.len() <= 1 || index >= self.rows.len() {
            return false;
        }
        self.rows.remove(index);
        true
    }
}

/// The Theme Editor's working copy of a theme.
#[derive(Clone, Debug)]
pub struct ThemeEdit {
    /// The theme's place in the list.
    pub index: usize,
    pub theme: Theme,
    /// Which of the theme's two layouts is on the canvas.
    pub template: Template,
    /// Set when the theme has changed this frame. The app takes the theme and clears this.
    pub changed: bool,
    /// Set when the operator asks to go back to the Show view.
    pub leave: bool,
}

impl ThemeEdit {
    pub fn of(index: usize, theme: Theme) -> ThemeEdit {
        ThemeEdit { index, theme, template: Template::Full, changed: false, leave: false }
    }
}

/// Something asked of the app about themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeRequest {
    /// Songs use the theme at this place in the list.
    UseForSongs(usize),
    /// Bible slides use the theme at this place in the list.
    UseForBible(usize),
    /// Make a new theme, a copy of the one songs use, and open it in the Theme Editor.
    New,
    /// Open the Theme Editor on the theme at this place in the list.
    Edit(usize),
}

/// A menu that opens under a toolbar button.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolMenu {
    Theme,
    More,
}

/// Which thumbnail the window is asking for.
#[derive(Clone, Copy)]
pub enum ThumbOf<'a> {
    /// A slide of a Bible passage, drawn with the theme Bible slides use.
    BibleSlide(&'a Slide),
    /// One of a theme's layouts, by its place; drawn fresh whenever the theme changes.
    Layout(usize, &'a Slide),
    Slide(&'a Slide),
    /// A media item, by its index in the show.
    Media(usize),
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState {
            slide_view: SlideView::Grid,
            columns: 4,
            selected_slide: None,
            filter: String::new(),
            output_stopped: false,
            media_width: 150.0,
            drop_hover: false,
            notice: None,
            editing: None,
            pending_edit: None,
            live_edit: false,
            fonts: std::sync::Arc::default(),
            bible_open: false,
            settings_open: false,
            settings_pane: 1,
            displays: Vec::new(),
            output_placement: Placement::Windowed,
            requested_placement: None,
            ndi_name: "PresenterCraft Audience".to_owned(),
            ndi_status: NdiStatus::Off,
            ndi_available: pc_ndi::NdiOutput::AVAILABLE,
            requested_ndi: None,
            inspector: crate::inspector::InspectorState::default(),
            theme_edit: None,
            theme_names: Vec::new(),
            theme_for_songs: 0,
            theme_for_bible: 0,
            theme_request: None,
            menu: None,
            menu_fresh: false,
            bible: BiblePage::default(),
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

pub fn not_built(what: &str) -> String {
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
/// `preview` is the picture of the audience screen and `canvas` the picture of the slide being
/// edited.
pub fn home_ui(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    preview: Option<TextureId>,
    canvas: Option<TextureId>,
) -> Vec<Command> {
    let mut commands = Vec::new();
    // Text boxes are dark wells in the lighter side columns.
    ui.visuals_mut().extreme_bg_color = FIELD;
    toolbar(ui, show, state, &mut commands);
    if ui.input(|input| input.modifiers.command && input.key_pressed(egui::Key::Comma)) {
        state.settings_open = !state.settings_open;
    }
    tool_menu(ui, state);
    settings_window(ui, state);
    if state.editing.is_some() {
        edit_view(ui, show, state, theme, thumb, canvas);
        return commands;
    }
    library(ui, show, state, &mut commands);
    live_panel(ui, show, state.output_stopped, preview, &mut commands);
    media_bin(ui, show, state, thumb, &mut commands);
    if state.bible_open {
        bible_area(ui, show, state, thumb, &mut commands);
    } else {
        slide_area(ui, show, state, theme, thumb, &mut commands);
    }
    commands
}

/// The menu open under a toolbar button, if any. A click anywhere else closes it.
fn tool_menu(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some((menu, corner)) = state.menu else { return };
    let mut close = ui.input(|input| input.key_pressed(egui::Key::Escape));
    let area = egui::Area::new(egui::Id::unique("tool menu"))
        .order(egui::Order::Foreground)
        .fixed_pos(corner)
        .fade_in(false)
        .constrain(true)
        .show(ui.ctx(), |ui| {
            Frame::new()
                .fill(Color32::from_rgb(44, 44, 46))
                .corner_radius(8)
                .stroke(Stroke::new(1.0, Color32::from_gray(80)))
                .inner_margin(6)
                .show(ui, |ui| {
                    ui.set_min_width(190.0);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let row = |ui: &mut egui::Ui, text: &str, ticked: bool, built: bool| -> bool {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(ui.available_width().max(190.0), 24.0), Sense::click());
                        if built && response.hovered() {
                            ui.painter().rect_filled(rect, 5.0, TAB_BLUE);
                        }
                        let ink = if built { Color32::from_gray(238) } else { Color32::from_gray(125) };
                        if ticked {
                            ui.painter().text(
                                rect.left_center() + vec2(6.0, 0.0),
                                Align2::LEFT_CENTER,
                                "✓",
                                FontId::proportional(13.0),
                                ink,
                            );
                        }
                        ui.painter().text(
                            rect.left_center() + vec2(24.0, 0.0),
                            Align2::LEFT_CENTER,
                            text,
                            FontId::proportional(13.0),
                            ink,
                        );
                        if !built {
                            response.on_hover_text(not_built(text));
                            return false;
                        }
                        response.clicked()
                    };
                    match menu {
                        ToolMenu::Theme => {
                            // One short list says which theme each kind of slide uses.
                            let names = state.theme_names.clone();
                            let heading = |ui: &mut egui::Ui, text: &str| {
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(text).size(10.5).strong().color(Color32::from_gray(150)));
                            };
                            heading(ui, "  SONGS USE");
                            for (index, name) in names.iter().enumerate() {
                                if row(ui, name, index == state.theme_for_songs, true) {
                                    state.theme_request = Some(ThemeRequest::UseForSongs(index));
                                    close = true;
                                }
                            }
                            ui.separator();
                            heading(ui, "  BIBLE SLIDES USE");
                            for (index, name) in names.iter().enumerate() {
                                if row(ui, name, index == state.theme_for_bible, true) {
                                    state.theme_request = Some(ThemeRequest::UseForBible(index));
                                    close = true;
                                }
                            }
                            ui.separator();
                            if row(ui, "New Theme…", false, true) {
                                state.theme_request = Some(ThemeRequest::New);
                                close = true;
                            }
                            if row(ui, "Edit Themes…", false, true) {
                                state.theme_request = Some(ThemeRequest::Edit(state.theme_for_songs));
                                close = true;
                            }
                        }
                        ToolMenu::More => {
                            if row(ui, "Theme Editor", state.theme_edit.is_some(), true) {
                                state.theme_request = Some(ThemeRequest::Edit(state.theme_for_songs));
                                close = true;
                            }
                            for name in ["Stage Editor", "CCLI Editor", "Props Editor", "Mask Editor"] {
                                row(ui, name, false, false);
                            }
                            ui.separator();
                            if row(ui, "Settings…", state.settings_open, true) {
                                state.settings_open = !state.settings_open;
                                close = true;
                            }
                        }
                    }
                });
        });
    // A click outside the menu closes it; the click that opened it does not count.
    let fresh = std::mem::take(&mut state.menu_fresh);
    let clicked_away = ui.input(|input| {
        input.pointer.any_click() && input.pointer.interact_pos().is_some_and(|at| !area.response.rect.contains(at))
    });
    if close || (clicked_away && !fresh) {
        state.menu = None;
    }
}

/// The strip along the top: tools and views on the left, media and screens on the right.
fn toolbar(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    egui::Panel::top("toolbar").resizable(false).frame(panel_frame(TOOLBAR, 6)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            tool_group(ui, &[Tool::ghost(Icon::Search, "Search")]);
            let look = tool_group(
                ui,
                &[
                    Tool::ghost("Aa", "Text"),
                    Tool {
                        face: Icon::Theme.into(),
                        label: "Theme",
                        selected: matches!(state.menu, Some((ToolMenu::Theme, _))),
                        built: true,
                    },
                ],
            );
            let under_pointer = |ui: &egui::Ui| {
                let at = ui.input(|input| input.pointer.interact_pos()).unwrap_or_default();
                pos2((at.x - TOOL_WIDTH / 2.0).max(4.0), TOOL_HEIGHT + 10.0)
            };
            if look == Some(1) {
                state.menu = Some((ToolMenu::Theme, under_pointer(ui)));
                state.menu_fresh = true;
            }
            let views = tool_group(
                ui,
                &[
                    Tool {
                        face: Icon::Play.into(),
                        label: "Show",
                        selected: state.editing.is_none() && !state.bible_open,
                        built: true,
                    },
                    Tool {
                        face: Icon::Pencil.into(),
                        label: "Edit",
                        selected: state.editing.is_some() && state.theme_edit.is_none(),
                        built: true,
                    },
                    Tool::ghost(Icon::Reflow, "Reflow"),
                    Tool {
                        face: Icon::Book.into(),
                        label: "Bible",
                        selected: state.editing.is_none() && state.bible_open,
                        built: true,
                    },
                    Tool {
                        face: Icon::More.into(),
                        label: "More",
                        selected: state.theme_edit.is_some() || matches!(state.menu, Some((ToolMenu::More, _))),
                        built: true,
                    },
                ],
            );
            match views {
                // Leaving the Theme Editor goes through the app, which saves the theme.
                Some(0 | 1 | 3) if state.theme_edit.is_some() => {
                    if let Some(edit) = state.theme_edit.as_mut() {
                        edit.leave = true;
                    }
                    state.bible_open = views == Some(3);
                }
                Some(0) => {
                    state.editing = None;
                    state.bible_open = false;
                }
                Some(3) => {
                    state.editing = None;
                    state.bible_open = true;
                }
                Some(4) => {
                    state.menu = Some((ToolMenu::More, under_pointer(ui)));
                    state.menu_fresh = true;
                }
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
        let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
        let total =
            show.presentations().len() - hidden.iter().filter(|slot| **slot < show.presentations().len()).count();

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
            let shown = show
                .presentations()
                .iter()
                .enumerate()
                .filter(|(index, p)| !hidden.contains(index) && matches_filter(&p.title, &state.filter))
                .count();
            let items = if shown == 1 { "1 ITEM".to_owned() } else { format!("{shown} ITEMS") };
            heading_with(ui, &items, &[(Icon::Plus, "Add an item")]);
            ui.add_space(2.0);
            egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
                for (index, presentation) in show.presentations().iter().enumerate() {
                    if hidden.contains(&index) || !matches_filter(&presentation.title, &state.filter) {
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
    (lit, any_live): (bool, bool),
    built: bool,
    hint: &str,
) -> bool {
    let id = ui.scope_id().with(("layer", hint));
    let response = ui.interact(rect, id, if built { Sense::click() } else { Sense::hover() });
    // The whole strip goes dark red while anything is on screen, and the layer that has
    // something on it is lit.
    let fill = if lit {
        Color32::from_rgb(117, 82, 78)
    } else if any_live {
        if response.hovered() { Color32::from_rgb(104, 40, 38) } else { Color32::from_rgb(80, 30, 29) }
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
        if clear_all.on_hover_text("Clear All").clicked() {
            commands.push(Command::ClearAll);
        }

        let layers: [(Icon, &str, bool, bool); 7] = [
            (Icon::Note, "Clear Audio", false, false),
            (Icon::Message, "Clear Messages", false, false),
            (Icon::Layers, "Clear Props", false, false),
            (Icon::Megaphone, "Clear Announcements", false, false),
            (Icon::Slide, "Clear Slide", live, true),
            (Icon::Picture, "Clear Media", media_live, true),
            (Icon::Camera, "Clear Video Input", false, false),
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
            if layer_button(ui, rect, corners, icon, (lit, any_live), built, hint) {
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
                for (index, (icon, what)) in
                    [(Icon::Slide, "Presentation"), (Icon::Megaphone, "Announcements"), (Icon::Note, "Audio")]
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
                    (Icon::Back, "Skip Backward 15 Seconds"),
                    (Icon::Pause, "Play"),
                    (Icon::Forward, "Skip Forward 15 Seconds"),
                    (Icon::ToEnd, "Go To 30 Seconds From The End"),
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
/// The right end of a slide area's bottom bar: the three views and the size slider.
fn view_controls(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.spacing_mut().item_spacing.x = 2.0;
    // Further right is bigger, so fewer slides fit across.
    let mut size = 7 - state.columns.clamp(2, 7);
    ui.spacing_mut().slider_width = 90.0;
    if ui.add(egui::Slider::new(&mut size, 0..=5).show_value(false)).on_hover_text("Slide size").changed() {
        state.columns = 7 - size;
    }
    ghost(ui, Icon::Options, "View options");
    for (view, face, hint) in [
        (SlideView::Table, Face::Icon(Icon::Rows), "Table View"),
        (SlideView::Easy, Face::Text("T"), "Easy View"),
        (SlideView::Grid, Face::Icon(Icon::Grid), "Grid View"),
    ] {
        let (rect, response) = ui.allocate_exact_size(vec2(24.0, 22.0), Sense::click());
        let ink = if state.slide_view == view {
            ACCENT
        } else if response.hovered() {
            Color32::from_gray(235)
        } else {
            Color32::from_gray(170)
        };
        paint_face(ui, face, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), 14.0, ink);
        if response.on_hover_text(hint).clicked() {
            state.slide_view = view;
        }
    }
}

fn transition_controls(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing.x = 2.0;
    ghost(ui, Icon::Plus, "Add Slide");
    ui.add_space(6.0);
    ghost(ui, Icon::Cut, "Cut");
    ghost(ui, Icon::Dissolve, "Dissolve");
    ghost(ui, Icon::Transition, "Advanced");
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

const EDIT_PANEL: Color32 = Color32::from_rgb(29, 29, 29);
const EDIT_HEADER: Color32 = Color32::from_rgb(36, 36, 36);
const EDIT_SURROUND: Color32 = Color32::from_rgb(113, 117, 122);
const EDIT_STATUS: Color32 = Color32::from_rgb(52, 54, 55);
const EDIT_SELECTED: Color32 = Color32::from_rgb(49, 115, 185);
const EDIT_OUTLINE: Color32 = Color32::from_rgb(220, 194, 87);
/// Half the side of a resize handle, in screen points.
const HANDLE: f32 = 4.5;

/// The Edit view. It replaces the whole window below the toolbar: the song's slides down the
/// left with the chosen slide's objects under them, a canvas in the middle where objects are
/// selected, dragged and resized, and an inspector on the right for the selected object.
fn edit_view(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    canvas: Option<TextureId>,
) {
    let Some(at) = state.editing.as_ref().map(|draft| draft.at) else { return };
    // The draft is a song's slide, or one of a theme's layouts when the Theme Editor is open.
    let theme_mode = state.theme_edit.is_some();
    let presentation = show.presentations().get(at.presentation);
    if presentation.is_none() && !theme_mode {
        state.editing = None;
        return;
    }
    let before = state.editing.as_ref().map(|draft| draft.slide.clone());
    let now = ui.input(|input| input.time);
    let typing = ui.ctx().text_edit_focused();

    // Keys: undo and redo always; delete and nudging when not typing in a box.
    let (undo, redo, delete, nudge) = ui.input(|input| {
        let command = input.modifiers.command;
        let undo = command && !input.modifiers.shift && input.key_pressed(egui::Key::Z);
        let redo = command && input.modifiers.shift && input.key_pressed(egui::Key::Z);
        let delete = !typing && (input.key_pressed(egui::Key::Delete) || input.key_pressed(egui::Key::Backspace));
        let step = if input.modifiers.shift { 10.0 } else { 1.0 };
        let mut nudge = (0.0, 0.0);
        if !typing {
            for (key, by) in [
                (egui::Key::ArrowLeft, (-step, 0.0)),
                (egui::Key::ArrowRight, (step, 0.0)),
                (egui::Key::ArrowUp, (0.0, -step)),
                (egui::Key::ArrowDown, (0.0, step)),
            ] {
                if input.key_pressed(key) {
                    nudge = (nudge.0 + by.0, nudge.1 + by.1);
                }
            }
        }
        (undo, redo, delete, nudge)
    });
    if let Some(draft) = state.editing.as_mut() {
        if undo && !typing {
            draft.undo();
        } else if redo && !typing {
            draft.redo();
        } else if delete {
            draft.delete_selected();
        } else if nudge != (0.0, 0.0)
            && let Some(object) = draft.selected_object()
        {
            let snapshot = draft.slide.clone();
            let frame = crate::editor::moved(object_frame(&draft.slide, object, theme), nudge);
            if let Some(object) = draft.selected_object_mut() {
                object.frame = Some(frame);
            }
            draft.record_run(snapshot, now);
        }
    }

    // Left: the slide navigator, with the slide's objects below it.
    let mut chosen = None;
    let mut chosen_template = None;
    egui::Panel::left("edit slides").resizable(false).exact_size(186.0).frame(panel_frame(EDIT_PANEL, 0)).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            Frame::new().fill(EDIT_HEADER).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(presentation) = presentation.filter(|_| !theme_mode) {
                        ui.label(
                            egui::RichText::new(presentation.title.as_str()).strong().size(12.5).color(Color32::WHITE),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| ghost(ui, Icon::Plus, "Add a slide"));
                    } else {
                        // The theme being edited. Choosing another edits that one; which theme
                        // songs and Bible slides use is set from the Theme button.
                        let editing = state.theme_edit.as_ref().map_or(0, |edit| edit.index);
                        let in_use = state.theme_names.get(editing).cloned().unwrap_or_default();
                        egui::ComboBox::from_id_salt("theme picker").selected_text(in_use).width(120.0).show_ui(
                            ui,
                            |ui| {
                                for (index, name) in state.theme_names.iter().enumerate() {
                                    if ui.selectable_label(index == editing, name.as_str()).clicked() {
                                        state.theme_request = Some(ThemeRequest::Edit(index));
                                    }
                                }
                                ui.separator();
                                if ui.button("New Theme…").clicked() {
                                    state.theme_request = Some(ThemeRequest::New);
                                }
                            },
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ghost(ui, Icon::Plus, "More layouts in a theme");
                        });
                    }
                });
            });

            egui::Panel::bottom("edit objects")
                .resizable(false)
                .exact_size(200.0)
                .frame(panel_frame(EDIT_PANEL, 0))
                .show(ui, |ui| {
                    Frame::new().fill(EDIT_HEADER).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new("Objects").strong().size(12.5).color(Color32::WHITE));
                            ui.add_space(ui.available_width());
                        });
                    });
                    egui::ScrollArea::vertical().id_salt("edit object list").auto_shrink([false, false]).show(
                        ui,
                        |ui| {
                            ui.add_space(4.0);
                            let Some(draft) = state.editing.as_mut() else { return };
                            // The top of the stack is listed first, as it is in front on the slide.
                            let mut pick = None;
                            // What a row's icons or its right-click menu asked for.
                            enum Ask {
                                Hide(bool),
                                Lock(bool),
                                Delete,
                                Rename(String),
                            }
                            let mut asked: Option<(pc_model::Uuid, Ask)> = None;
                            for object in draft.slide.objects.iter().rev() {
                                let selected = draft.selected == Some(object.id);
                                let (row, response) =
                                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
                                if selected {
                                    ui.painter().rect_filled(row, 0.0, EDIT_SELECTED);
                                } else if response.hovered() {
                                    ui.painter().rect_filled(row, 0.0, Color32::from_gray(46));
                                }
                                // A hidden object's row is greyed.
                                let ink = if object.hidden { Color32::from_gray(120) } else { Color32::from_gray(230) };
                                ui.painter().text(
                                    row.left_center() + vec2(12.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    "T",
                                    FontId::proportional(15.0),
                                    ink,
                                );
                                let name_area = Rect::from_min_max(row.min, pos2(row.max.x - 46.0, row.max.y));
                                ui.painter().with_clip_rect(name_area).text(
                                    row.left_center() + vec2(32.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    object.name.as_str(),
                                    FontId::proportional(13.0),
                                    ink,
                                );
                                // Hide and lock sit at the right of the row under the pointer, the
                                // selected row, and any row that is already hidden or locked.
                                let marks = [
                                    (object.hidden, "Hide", "Show", pos2(row.max.x - 34.0, row.center().y)),
                                    (object.locked, "Lock", "Unlock", pos2(row.max.x - 14.0, row.center().y)),
                                ];
                                for (index, (on, to_set, to_clear, centre)) in marks.into_iter().enumerate() {
                                    if !(on || selected || response.hovered()) {
                                        continue;
                                    }
                                    let spot = Rect::from_center_size(centre, vec2(18.0, 18.0));
                                    let mark = ui.interact(
                                        spot,
                                        ui.scope_id().with(("object mark", object.id, index)),
                                        Sense::click(),
                                    );
                                    let mark_ink =
                                        if on { Color32::from_rgb(240, 190, 40) } else { Color32::from_gray(170) };
                                    let p = ui.painter();
                                    if index == 0 {
                                        // An eye; struck through when the object is hidden.
                                        p.circle_stroke(centre, 5.0, Stroke::new(1.2, mark_ink));
                                        p.circle_filled(centre, 2.0, mark_ink);
                                        if on {
                                            p.line_segment(
                                                [centre + vec2(-6.0, 6.0), centre + vec2(6.0, -6.0)],
                                                Stroke::new(1.4, mark_ink),
                                            );
                                        }
                                    } else {
                                        // A padlock; its shackle is open when unlocked.
                                        p.rect_filled(
                                            Rect::from_center_size(centre + vec2(0.0, 2.5), vec2(9.0, 7.0)),
                                            1.5,
                                            mark_ink,
                                        );
                                        let lift = if on { 0.0 } else { -2.0 };
                                        p.line_segment(
                                            [centre + vec2(-2.5, -1.0), centre + vec2(-2.5, -4.5 + lift)],
                                            Stroke::new(1.3, mark_ink),
                                        );
                                        p.line_segment(
                                            [centre + vec2(-2.5, -4.5 + lift), centre + vec2(2.5, -4.5 + lift)],
                                            Stroke::new(1.3, mark_ink),
                                        );
                                        if on {
                                            p.line_segment(
                                                [centre + vec2(2.5, -1.0), centre + vec2(2.5, -4.5)],
                                                Stroke::new(1.3, mark_ink),
                                            );
                                        }
                                    }
                                    if mark.on_hover_text(if on { to_clear } else { to_set }).clicked() {
                                        asked =
                                            Some((object.id, if index == 0 { Ask::Hide(!on) } else { Ask::Lock(!on) }));
                                    }
                                }
                                if response.clicked() {
                                    pick = Some(object.id);
                                }
                                response.context_menu(|ui| {
                                    ui.label(egui::RichText::new("Rename").size(11.0).color(DIM));
                                    let mut name = object.name.clone();
                                    if ui.add(egui::TextEdit::singleline(&mut name).desired_width(150.0)).changed() {
                                        asked = Some((object.id, Ask::Rename(name)));
                                    }
                                    ui.separator();
                                    if ui.button(if object.hidden { "Show" } else { "Hide" }).clicked() {
                                        asked = Some((object.id, Ask::Hide(!object.hidden)));
                                        ui.close();
                                    }
                                    if ui.button(if object.locked { "Unlock" } else { "Lock" }).clicked() {
                                        asked = Some((object.id, Ask::Lock(!object.locked)));
                                        ui.close();
                                    }
                                    if ui.button("Delete").clicked() {
                                        asked = Some((object.id, Ask::Delete));
                                        ui.close();
                                    }
                                });
                            }
                            if let Some((id, ask)) = asked {
                                if matches!(ask, Ask::Delete) {
                                    draft.selected = Some(id);
                                    draft.delete_selected();
                                } else if let Some(object) = draft.slide.objects.iter_mut().find(|o| o.id == id) {
                                    match ask {
                                        Ask::Hide(hidden) => object.hidden = hidden,
                                        Ask::Lock(locked) => object.locked = locked,
                                        Ask::Rename(name) => object.name = name,
                                        Ask::Delete => {}
                                    }
                                }
                            }
                            if draft.slide.objects.is_empty() {
                                ui.label(egui::RichText::new("   This slide is empty").size(12.0).color(DIM));
                            }
                            if pick.is_some() {
                                draft.selected = pick;
                            }
                        },
                    );
                });

            if let Some(edit) = state.theme_edit.as_ref() {
                // A theme's two layouts, each drawn as it will look.
                let layouts = [(Template::Full, "Two languages"), (Template::Alone, "One language")];
                let slides: Vec<Slide> =
                    layouts.iter().map(|(template, _)| edit.theme.template_slide(*template)).collect();
                let current = edit.template;
                egui::ScrollArea::vertical().id_salt("theme layout list").auto_shrink([false, false]).show(ui, |ui| {
                    let width = ui.available_width();
                    let image_size = vec2(width - 16.0, (width - 16.0) * 9.0 / 16.0);
                    for (index, ((template, name), slide)) in layouts.into_iter().zip(&slides).enumerate() {
                        let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 30.0), Sense::click());
                        let selected = template == current;
                        ui.painter().rect_filled(
                            cell,
                            0.0,
                            if selected { EDIT_SELECTED } else { Color32::from_gray(52) },
                        );
                        let image = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                        ui.painter().rect_filled(image, 0.0, Color32::BLACK);
                        if let Some(texture) = canvas.filter(|_| selected) {
                            // The layout on the canvas is shown as it is this instant.
                            egui::Image::from_texture((texture, image_size)).paint_at(ui, image);
                        } else {
                            match thumb(ThumbOf::Layout(index, slide)) {
                                Some((texture, _)) => {
                                    egui::Image::from_texture((texture, image_size)).paint_at(ui, image)
                                }
                                None => ui.ctx().request_repaint(),
                            }
                        }
                        if selected {
                            ui.painter().rect_stroke(image, 0.0, Stroke::new(2.0, EDIT_OUTLINE), StrokeKind::Outside);
                        }
                        ui.painter().text(
                            pos2(cell.min.x + 9.0, image.max.y + 10.0),
                            Align2::LEFT_CENTER,
                            format!("{}.  {name}", index + 1),
                            FontId::proportional(11.5),
                            Color32::from_gray(230),
                        );
                        if response.clicked() && !selected {
                            chosen_template = Some(template);
                        }
                    }
                });
            } else {
                egui::ScrollArea::vertical().id_salt("edit slide list").auto_shrink([false, false]).show(ui, |ui| {
                    let width = ui.available_width();
                    let image_size = vec2(width - 16.0, (width - 16.0) * 9.0 / 16.0);
                    let mut index = 0;
                    for group in presentation.map(|p| p.groups.as_slice()).unwrap_or_default() {
                        let (bar, _) = ui.allocate_exact_size(vec2(width, 20.0), Sense::hover());
                        ui.painter().rect_filled(bar, 0.0, color32(group.color).gamma_multiply(0.8));
                        let ink = text_on(group.color);
                        icons::paint(
                            ui.painter(),
                            Icon::ChevronDown,
                            Rect::from_center_size(bar.left_center() + vec2(9.0, 0.0), vec2(8.0, 8.0)),
                            ink,
                        );
                        let name = if group.name.is_empty() { "No group" } else { group.name.as_str() };
                        ui.painter().text(
                            bar.left_center() + vec2(18.0, 0.0),
                            Align2::LEFT_CENTER,
                            name,
                            FontId::proportional(12.5),
                            ink,
                        );

                        for slide in &group.slides {
                            let here = SlideRef { presentation: at.presentation, slide: index };
                            let (cell, response) =
                                ui.allocate_exact_size(vec2(width, image_size.y + 30.0), Sense::click());
                            let selected = here == at;
                            if ui.is_rect_visible(cell) {
                                ui.painter().rect_filled(
                                    cell,
                                    0.0,
                                    if selected { EDIT_SELECTED } else { color32(group.color).gamma_multiply(0.22) },
                                );
                                let image = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                                ui.painter().rect_filled(image, 0.0, Color32::BLACK);
                                let of = if state.bible.slots.is_some_and(|slots| slots.contains(&at.presentation)) {
                                    ThumbOf::BibleSlide(slide)
                                } else {
                                    ThumbOf::Slide(slide)
                                };
                                match thumb(of) {
                                    Some((texture, _)) => {
                                        egui::Image::from_texture((texture, image_size)).paint_at(ui, image)
                                    }
                                    None => ui.ctx().request_repaint(),
                                }
                                if selected {
                                    ui.painter().rect_stroke(
                                        image,
                                        0.0,
                                        Stroke::new(2.0, EDIT_OUTLINE),
                                        StrokeKind::Outside,
                                    );
                                }
                                ui.painter().text(
                                    pos2(cell.min.x + 9.0, image.max.y + 10.0),
                                    Align2::LEFT_CENTER,
                                    format!("{}.", index + 1),
                                    FontId::proportional(11.5),
                                    Color32::from_gray(230),
                                );
                            }
                            if response.clicked() && !selected {
                                chosen = Some(here);
                            }
                            index += 1;
                        }
                    }
                });
            }
        },
    );

    // Right: the inspector.
    let leave = crate::inspector::inspector(ui, state, theme);

    // Middle: the canvas.
    egui::CentralPanel::default().frame(panel_frame(EDIT_SURROUND, 0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
        egui::Panel::bottom("edit status").resizable(false).frame(panel_frame(EDIT_STATUS, 4)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                let frame = state
                    .editing
                    .as_ref()
                    .and_then(|draft| draft.selected_object().map(|object| object_frame(&draft.slide, object, theme)));
                for (name, value) in [
                    ("X", frame.map(|f| f.x)),
                    ("Y", frame.map(|f| f.y)),
                    ("W", frame.map(|f| f.w)),
                    ("H", frame.map(|f| f.h)),
                ] {
                    let text = value.map_or_else(|| format!("{name}: ----"), |v| format!("{name}: {v:.0}"));
                    ui.label(egui::RichText::new(text).size(11.5).color(Color32::from_gray(200)));
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.label(egui::RichText::new("Fit").size(11.5).color(Color32::from_gray(200)));
                    ghost(ui, Icon::Search, "Zoom");
                    ghost(ui, Icon::Grid, "Guides");
                });
            });
        });
        let area = ui.available_rect_before_wrap();
        let ruler = 14.0;
        ui.painter().rect_filled(
            Rect::from_min_size(area.min, vec2(area.width(), ruler)),
            0.0,
            Color32::from_rgb(12, 12, 12),
        );
        ui.painter().rect_filled(
            Rect::from_min_size(area.min, vec2(ruler, area.height())),
            0.0,
            Color32::from_rgb(12, 12, 12),
        );
        let inner = Rect::from_min_max(area.min + vec2(ruler, ruler), area.max).shrink(28.0);
        let width = inner.width().min(inner.height() * 16.0 / 9.0).max(1.0);
        let slide = Rect::from_min_size(
            pos2(inner.center().x - width / 2.0, inner.min.y + 44.0),
            vec2(width, width * 9.0 / 16.0),
        );
        ui.painter().rect_filled(slide.expand(1.0), 0.0, Color32::from_black_alpha(90));
        ui.painter().rect_filled(slide, 0.0, Color32::BLACK);
        if let Some(texture) = canvas {
            egui::Image::from_texture((texture, slide.size())).paint_at(ui, slide);
        }
        for x in (0..=4).map(|i| slide.min.x + slide.width() * i as f32 / 4.0) {
            ui.painter().line_segment(
                [pos2(x, area.min.y + 6.0), pos2(x, area.min.y + ruler)],
                Stroke::new(1.0, Color32::from_gray(150)),
            );
        }

        // Pointer on the canvas: select, move, resize. Positions are turned into slide units.
        let scale = slide.width() / pc_model::SLIDE_WIDTH;
        let to_slide = |p: egui::Pos2| ((p.x - slide.min.x) / scale, (p.y - slide.min.y) / scale);
        let to_screen = |f: SlideRect| Rect::from_min_size(slide.min + vec2(f.x, f.y) * scale, vec2(f.w, f.h) * scale);
        let response = ui.interact(area, ui.scope_id().with("canvas"), Sense::click_and_drag());
        if let Some(draft) = state.editing.as_mut() {
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = to_slide(pointer);
                if response.drag_started() || response.clicked() {
                    draft.press(theme, point, (HANDLE + 3.0) / scale);
                }
                if response.dragged() {
                    let keep_shape = ui.input(|input| input.modifiers.shift);
                    draft.drag_to(point, keep_shape);
                }
            }
            if response.drag_stopped() || response.clicked() {
                draft.release();
            }

            // Outlines: every object faintly, the selected one with handles.
            for object in &draft.slide.objects {
                let frame = object_frame(&draft.slide, object, theme);
                let rect = to_screen(frame);
                let selected = draft.selected == Some(object.id);
                let centre = rect.center();
                let (sin, cos) = object.rotation.to_radians().sin_cos();
                let corners: Vec<egui::Pos2> =
                    [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
                        .into_iter()
                        .map(|c| {
                            let d = c - centre;
                            centre + vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
                        })
                        .collect();
                let stroke =
                    if selected { Stroke::new(1.5, TAB_BLUE) } else { Stroke::new(1.0, Color32::from_white_alpha(45)) };
                ui.painter().add(egui::Shape::closed_line(corners, stroke));
                if selected && object.rotation == 0.0 {
                    for handle in crate::editor::Handle::ALL {
                        let (x, y) = handle.position(frame);
                        let at = slide.min + vec2(x, y) * scale;
                        let knob = Rect::from_center_size(at, vec2(HANDLE * 2.0, HANDLE * 2.0));
                        ui.painter().rect_filled(knob, 1.5, Color32::WHITE);
                        ui.painter().rect_stroke(knob, 1.5, Stroke::new(1.0, TAB_BLUE), StrokeKind::Inside);
                    }
                }
            }
        }

        // The floating row of object tools. Text works; the rest are in place for later.
        let tools = [
            (Face::Text("T"), "Text"),
            (Face::Icon(Icon::Layers), "Shape"),
            (Face::Icon(Icon::Pencil), "Draw"),
            (Face::Icon(Icon::Picture), "Picture"),
            (Face::Icon(Icon::Camera), "Video"),
            (Face::Icon(Icon::Live), "Web page"),
            (Face::Icon(Icon::Look), "Mask"),
        ];
        let bar =
            Rect::from_center_size(pos2(slide.center().x, slide.min.y - 22.0), vec2(tools.len() as f32 * 40.0, 30.0));
        ui.painter().rect_filled(bar, 6.0, Color32::from_rgb(35, 35, 35));
        ui.painter().rect_stroke(bar, 6.0, Stroke::new(1.0, Color32::from_gray(70)), StrokeKind::Inside);
        for (index, (face, name)) in tools.into_iter().enumerate() {
            let cell = Rect::from_min_size(pos2(bar.min.x + 40.0 * index as f32, bar.min.y), vec2(40.0, 30.0));
            let built = index == 0;
            let cell_response = ui.interact(
                cell,
                ui.scope_id().with(("object tool", index)),
                if built { Sense::click() } else { Sense::hover() },
            );
            if built && cell_response.hovered() {
                ui.painter().rect_filled(cell.shrink(3.0), 4.0, Color32::from_gray(70));
            }
            let ink = if built { Color32::from_gray(235) } else { Color32::from_gray(130) };
            paint_face(ui, face, Rect::from_center_size(cell.center(), vec2(15.0, 15.0)), 15.0, ink);
            if built {
                if cell_response.on_hover_text("Add a text box").clicked()
                    && let Some(draft) = state.editing.as_mut()
                {
                    draft.add_text();
                }
            } else {
                cell_response.on_hover_text(not_built(name));
            }
        }
    });

    // Whatever changed this frame (a drag, typing, a button) is handed to the app, and typing
    // or dragging numbers is gathered into single undo steps.
    if let (Some(draft), Some(before)) = (state.editing.as_mut(), before)
        && draft.slide != before
    {
        if draft.drag.is_none() && !draft.stepped {
            draft.record_run(before, now);
        }
        match state.theme_edit.as_mut() {
            // A theme's layout: the theme is remade from it, and the app takes the theme.
            Some(edit) => {
                edit.theme = edit.theme.with_template(edit.template, &draft.slide);
                edit.changed = true;
            }
            None => state.pending_edit = Some((draft.at, draft.slide.clone())),
        }
    }
    if let Some(draft) = state.editing.as_mut() {
        draft.stepped = false;
    }
    if let Some(edit) = state.theme_edit.as_mut() {
        if leave {
            edit.leave = true;
        } else if let Some(template) = chosen_template {
            edit.template = template;
            let which = usize::from(template == Template::Alone);
            state.editing = Some(EditDraft::of_slide(edit.theme.template_slide(template), which));
        }
    } else if leave {
        state.editing = None;
    } else if let Some(here) = chosen {
        state.editing = EditDraft::of(show, here);
    }
}

/// The panes of the Settings window, in order.
const SETTINGS_PANES: [&str; 10] =
    ["General", "Screens", "Audio", "Network", "Inputs", "Devices", "Media", "Import", "Groups", "Slide Labels"];

/// The Settings window. The Screens pane works: it chooses the display the audience output
/// is shown on. The other panes are in place and say they are not built yet.
fn settings_window(ui: &mut egui::Ui, state: &mut HomeState) {
    if !state.settings_open {
        return;
    }
    let mut open = true;
    egui::Window::new("Settings")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .fade_in(false)
        .fade_out(false)
        .default_size([640.0, 420.0])
        .anchor(Align2::CENTER_CENTER, [0.0, -20.0])
        .show(ui.ctx(), |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    for (index, name) in SETTINGS_PANES.into_iter().enumerate() {
                        if ui.selectable_label(state.settings_pane == index, name).clicked() {
                            state.settings_pane = index;
                        }
                    }
                });
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(450.0);
                    ui.set_min_height(360.0);
                    let name = SETTINGS_PANES.get(state.settings_pane).copied().unwrap_or("General");
                    ui.heading(name);
                    ui.add_space(6.0);
                    if name == "Screens" {
                        screens_pane(ui, state);
                    } else {
                        ui.label(egui::RichText::new(format!("{name} settings are not built yet.")).color(DIM));
                    }
                });
            });
        });
    if !open {
        state.settings_open = false;
    }
}

/// The Screens pane: the audience screen and where it is shown.
fn screens_pane(ui: &mut egui::Ui, state: &mut HomeState) {
    section_label(ui, "AUDIENCE SCREEN");
    ui.label("Show the audience output on:");
    ui.add_space(2.0);
    let mut wanted = state.output_placement;
    for display in &state.displays {
        let label = format!("Display {}:  {}   {} x {}", display.index + 1, display.name, display.w, display.h);
        ui.radio_value(&mut wanted, Placement::Fullscreen(display.index), label)
            .on_hover_text("Fill this display with the audience output");
    }
    ui.radio_value(&mut wanted, Placement::Windowed, "A window")
        .on_hover_text("Show the audience output in an ordinary window, for trying things out");
    if state.displays.is_empty() {
        ui.label(egui::RichText::new("No displays were found.").color(DIM));
    }
    if wanted != state.output_placement {
        state.requested_placement = Some(wanted);
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(
            "Choosing the display this window is on covers this window. Press Cmd+, to open Settings again.",
        )
        .size(11.5)
        .color(DIM),
    );
    ui.add_space(12.0);
    section_label(ui, "STAGE SCREEN");
    ui.label(egui::RichText::new("The stage screen is not built yet.").color(DIM));
    ui.add_space(12.0);
    section_label(ui, "NDI OUTPUT");
    if !state.ndi_available {
        ui.label(egui::RichText::new("This build of PresenterCraft does not include NDI.").color(DIM));
    } else {
        let on = !matches!(state.ndi_status, NdiStatus::Off);
        let mut wanted = on;
        ui.checkbox(&mut wanted, "Send the audience screen over NDI")
            .on_hover_text("Other computers on the network can then show the audience screen");
        ui.horizontal(|ui| {
            ui.label("Name");
            let name = ui.add_enabled(!on, egui::TextEdit::singleline(&mut state.ndi_name).desired_width(240.0));
            name.on_disabled_hover_text("Switch NDI off to change the name");
        });
        if wanted != on {
            state.requested_ndi = Some(wanted.then(|| state.ndi_name.clone()));
        }
        match &state.ndi_status {
            NdiStatus::Off => {
                ui.label(egui::RichText::new("Off").color(DIM));
            }
            NdiStatus::On(name) => {
                ui.label(egui::RichText::new(format!("Sending as \"{name}\" at 1920 x 1080")).color(ON));
            }
            NdiStatus::Failed(reason) => {
                ui.label(egui::RichText::new(reason.as_str()).color(Color32::from_rgb(255, 150, 120)));
            }
        }
    }
    ui.add_space(8.0);
    ui.label(egui::RichText::new("NDI® is a registered trademark of Vizrt NDI AB.").size(10.5).color(DIM));
}

/// Says which of a song's fonts this computer does not have, and offers a place to look for
/// each. Until the font is installed the song is drawn with a substitute, so it will not look
/// the way it does on the computer it came from.
fn missing_font_notice(ui: &mut egui::Ui, missing: &[String]) {
    if missing.is_empty() {
        return;
    }
    ui.add_space(10.0);
    let label = if missing.len() == 1 {
        format!("Missing font: {}", missing[0])
    } else {
        format!("{} missing fonts", missing.len())
    };
    let warning = Color32::from_rgb(240, 190, 40);
    ui.menu_button(egui::RichText::new(label).size(12.0).color(warning), |ui| {
        ui.set_max_width(320.0);
        ui.label("This song uses fonts that are not installed on this computer. It is shown with a substitute until they are installed.");
        ui.separator();
        for family in missing {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(family.as_str()).strong());
                let query: String = family.chars().map(|c| if c == ' ' { '+' } else { c }).collect();
                ui.hyperlink_to("Look for it", format!("https://fonts.google.com/?query={query}"))
                    .on_hover_text("Opens a font library in your browser. Not every font is free to download.");
            });
        }
        ui.separator();
        ui.label(egui::RichText::new("After installing a font, restart PresenterCraft.").size(11.5).color(DIM));
    });
}

/// The Bible page, in place of the slide grid: a field for a reference, a search field, and a
/// row for each translation saying where its verse and its reference go on the slide. Looking
/// verses up is not built yet, so this is the layout with its controls in place.
fn bible_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let live = show.live().map(|at| at.presentation);
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        egui::Panel::bottom("bible bar").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_enabled(false, egui::Button::new(egui::RichText::new("Save As…").size(12.0)))
                    .on_disabled_hover_text(not_built("Save these verses as a presentation"));
                ui.add_space(8.0);
                transition_controls(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    view_controls(ui, state);
                });
            });
        });

        Frame::new().fill(SIDE).inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            ui.horizontal(|ui| {
                let entered = |response: &egui::Response, ui: &egui::Ui| {
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                };
                let lookup = ui
                    .add(
                        egui::TextEdit::singleline(&mut state.bible.lookup)
                            .hint_text("Scripture Lookup")
                            .desired_width(240.0),
                    )
                    .on_hover_text("Type a reference, such as John 3:16, and press Return");
                if entered(&lookup, ui) {
                    state.bible.look_up(live);
                }
                let search =
                    ui.add(egui::TextEdit::singleline(&mut state.bible.search).hint_text("Search").desired_width(230.0))
                        .on_hover_text("Type words to find in the first Bible, and press Return");
                if entered(&search, ui) {
                    state.bible.find();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let button = ui.add(egui::Button::selectable(state.bible.options.open, "Options"))
                        .on_hover_text("How verses become slides, and which Bibles are installed");
                    if button.clicked() {
                        state.bible.options.open = !state.bible.options.open;
                    }
                    if state.bible.options.open {
                        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                            state.bible.options.open = false;
                        }
                        let bible = &mut state.bible;
                        let picked = crate::bible::options_popup(
                            ui.ctx(),
                            &mut bible.options,
                            &mut bible.shelf,
                            &mut bible.downloads,
                            button.rect,
                            "Songs",
                            (&state.theme_names, state.theme_for_bible),
                        );
                        if let Some(index) = picked {
                            state.theme_request = Some(ThemeRequest::UseForBible(index));
                        }
                    }
                });
            });

            let mut add = false;
            let mut remove = None;
            let count = state.bible.rows.len();
            for (index, row) in state.bible.rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let chosen = state.bible.shelf.bibles.get(row.bible).map_or("Choose a Bible", |b| b.name.as_str());
                    egui::ComboBox::from_id_salt(("bible translation", index)).selected_text(chosen).width(200.0).show_ui(
                        ui,
                        |ui| {
                            for (place, bible) in state.bible.shelf.bibles.iter().enumerate() {
                                ui.selectable_value(&mut row.bible, place, format!("{} ({})", bible.name, bible.short));
                            }
                        },
                    );
                    ui.add_space(8.0);
                    ui.label("Verse");
                    egui::ComboBox::from_id_salt(("bible verse", index))
                        .selected_text(target_label(&row.verse))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for target in std::iter::once(None).chain(state.bible.boxes.iter().cloned().map(Some)) {
                                let name = target_label(&target);
                                ui.selectable_value(&mut row.verse, target, name);
                            }
                        });
                    ui.add_space(8.0);
                    ui.label("Reference");
                    egui::ComboBox::from_id_salt(("bible reference", index))
                        .selected_text(target_label(&row.reference))
                        .width(120.0)
                        .show_ui(ui, |ui| {
                            for target in std::iter::once(None).chain(state.bible.boxes.iter().cloned().map(Some)) {
                                let name = target_label(&target);
                                ui.selectable_value(&mut row.reference, target, name);
                            }
                        });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // The first row adds a translation; the others remove themselves.
                        if index == 0 {
                            if ui.add_enabled(count < BiblePage::MAX_ROWS, egui::Button::new("+")).on_hover_text("Add a translation").clicked() {
                                add = true;
                            }
                        } else if ui.button("−").on_hover_text("Remove this translation").clicked() {
                            remove = Some(index);
                        }
                        let terms = state.bible.shelf.bibles.get(row.bible).map_or_else(String::new, |b| {
                            if b.credit.is_empty() { b.terms.clone() } else { format!("{}\n{}", b.terms, b.credit) }
                        });
                        ui.label(egui::RichText::new("License").size(11.0).color(DIM)).on_hover_text(terms);
                    });
                });
            }
            if add {
                state.bible.add_row();
            }
            if let Some(index) = remove {
                state.bible.remove_row(index);
            }
        });

        // Say so when two Bibles are shown together and one only mostly follows a known numbering.
        let own: Vec<&str> = state
            .bible
            .rows
            .iter()
            .filter_map(|row| state.bible.shelf.bibles.get(row.bible))
            .filter(|b| b.loose)
            .map(|b| b.name.as_str())
            .collect();
        if !own.is_empty() && state.bible.rows.len() > 1 {
            let note = format!(
                "{} numbers a few verses its own way, so beside another Bible a verse here and there may be one off.",
                own.join(" and ")
            );
            Frame::new().inner_margin(egui::Margin::symmetric(12, 4)).show(ui, |ui| {
                ui.label(egui::RichText::new(note).size(11.5).color(Color32::from_rgb(240, 190, 40)));
            });
        }
        // A change to the Bibles or the options remakes the passage that is showing.
        if !state.bible.shown.is_empty() && state.bible.made_with != state.bible.choices() {
            state.bible.lookup = state.bible.shown.clone();
            state.bible.look_up(live);
        }
        if let Some(message) = state.bible.message.clone() {
            Frame::new().inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                ui.label(egui::RichText::new(message).color(Color32::from_rgb(255, 150, 120)));
            });
        }
        if !state.bible.found.is_empty() {
            // Search results: click one to turn it into slides.
            let mut picked = None;
            egui::ScrollArea::vertical().id_salt("bible found").auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                    for (reference, words) in &state.bible.found {
                        let line = format!("{reference}   {words}");
                        if ui.add(egui::Button::new(egui::RichText::new(line).size(13.0)).frame(false).truncate()).clicked() {
                            picked = Some(reference.clone());
                        }
                    }
                });
            });
            if let Some(reference) = picked {
                state.bible.lookup = reference;
                state.bible.look_up(live);
            }
            return;
        }
        let slot = state.bible.slots.map(|slots| slots[state.bible.showing]);
        match slot.filter(|slot| show.presentations().get(*slot).is_some_and(|p| p.slide_count() > 0)) {
            Some(slot) => slide_grid(ui, show, slot, state, thumb, commands),
            None => {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        egui::RichText::new(
                            "Type a reference such as John 3:16-18 in Scripture Lookup and press Return.\nIts verses appear here as slides; click one to show it.",
                        )
                        .color(DIM),
                    );
                });
            }
        }
    });
}

/// The middle: the library's title bar, the selected song's bar, and its slides as a grid of
/// thumbnails with a group bar under each.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
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
                    view_controls(ui, state);
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
                missing_font_notice(
                    ui,
                    &pc_model::missing_fonts(&pc_model::fonts_used(presentation, theme), &state.fonts),
                );
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

        slide_grid(ui, show, show.selected(), state, thumb, commands);
    });
}

/// The outline of the slide on screen, of a slide picked out by a right-click, and of the
/// slide that was last on screen once the screen has been cleared.
const LIVE_OUTLINE: Color32 = Color32::from_rgb(240, 138, 36);
const SELECTED_OUTLINE: Color32 = Color32::from_rgb(51, 119, 255);
const WAS_LIVE_OUTLINE: Color32 = Color32::from_rgb(59, 84, 124);

/// One presentation's slides, in the view chosen: a grid of thumbnails, the same grid showing
/// only words, or a table. Clicking a slide puts it on screen; a right-click picks it out
/// without showing it.
fn slide_grid(
    ui: &mut egui::Ui,
    show: &Show,
    which: usize,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    let Some(presentation) = show.presentations().get(which) else { return };
    let from_bible = state.bible.slots.is_some_and(|slots| slots.contains(&which));
    let view = state.slide_view;
    let gap = 12.0;
    let bar_height = 18.0;
    egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(15.0);
        Frame::new().inner_margin(egui::Margin::symmetric(20, 0)).show(ui, |ui| {
            // The slider sets how many slides fit across; they stretch to fill the width.
            let columns = if view == SlideView::Table { 1 } else { state.columns.clamp(2, 7) };
            let across = ui.available_width();
            let width = ((across - gap * (columns as f32 - 1.0)) / columns as f32).floor().max(40.0);
            let image_size = if view == SlideView::Table {
                vec2(204.0, 204.0 * 9.0 / 16.0)
            } else {
                vec2(width, (width * 9.0 / 16.0).round())
            };
            let cell = if view == SlideView::Table {
                vec2(across, image_size.y)
            } else {
                vec2(width, image_size.y + bar_height)
            };
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = if view == SlideView::Table { vec2(0.0, 2.0) } else { vec2(gap, gap) };
                let mut last_group = None;
                for (index, (group, slide)) in presentation.slides().enumerate() {
                    // A group's name is shown on its first slide only.
                    let starts_group = last_group != Some(group.id);
                    last_group = Some(group.id);
                    let (rect, response) = ui.allocate_exact_size(cell, Sense::click());
                    let here = SlideRef { presentation: which, slide: index };
                    if response.clicked() {
                        state.selected_slide = None;
                        commands.push(Command::GoTo(here));
                    }
                    if response.secondary_clicked() {
                        state.selected_slide = Some(here);
                    }
                    // Editing is on the right-click menu, so reaching for it never puts a
                    // slide on screen by accident.
                    response.context_menu(|ui| {
                        if ui.button("Edit Slide").clicked() {
                            state.editing = EditDraft::of(show, here);
                            ui.close();
                        }
                    });
                    // Slides scrolled out of view cost nothing: no thumbnail is asked for.
                    if !ui.is_rect_visible(rect) {
                        continue;
                    }
                    let painter = ui.painter().clone();
                    let ink = text_on(group.color);
                    let words = || slide.texts().into_iter().map(|(_, text)| text).collect::<Vec<_>>().join("\n");
                    let mut picture = |ui: &mut egui::Ui, image: Rect| {
                        painter.rect_filled(image, 0.0, Color32::BLACK);
                        let of = if from_bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::Slide(slide) };
                        match thumb(of) {
                            Some((texture, overflow)) => {
                                egui::Image::from_texture((texture, image.size())).paint_at(ui, image);
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
                    };
                    match view {
                        SlideView::Table => {
                            // A strip in the group's colour with the number, the thumbnail, the words.
                            let strip = Rect::from_min_size(rect.min, vec2(34.0, rect.height()));
                            painter.rect_filled(rect, 0.0, Color32::from_gray(38));
                            painter.rect_filled(strip, 0.0, color32(group.color));
                            painter.text(
                                strip.center(),
                                Align2::CENTER_CENTER,
                                format!("{}.", index + 1),
                                FontId::proportional(13.0),
                                ink,
                            );
                            let image = Rect::from_min_size(rect.min + vec2(34.0, 0.0), image_size);
                            picture(ui, image);
                            let text_at = pos2(image.max.x + 14.0, rect.min.y + 8.0);
                            let mut top = text_at.y;
                            if starts_group && !group.name.is_empty() {
                                painter.text(
                                    text_at,
                                    Align2::LEFT_TOP,
                                    group.name.as_str(),
                                    FontId::proportional(12.0),
                                    Color32::from_gray(160),
                                );
                                top += 18.0;
                            }
                            let clip = Rect::from_min_max(pos2(text_at.x, rect.min.y), rect.max);
                            painter.with_clip_rect(clip).text(
                                pos2(text_at.x, top),
                                Align2::LEFT_TOP,
                                words(),
                                FontId::proportional(13.0),
                                Color32::from_gray(230),
                            );
                        }
                        SlideView::Grid | SlideView::Easy => {
                            let image = Rect::from_min_size(rect.min, image_size);
                            if view == SlideView::Easy {
                                // Only the words, plain, on black: quick to read at any size.
                                painter.rect_filled(image, 0.0, Color32::BLACK);
                                painter.with_clip_rect(image).text(
                                    image.min + vec2(8.0, 6.0),
                                    Align2::LEFT_TOP,
                                    words(),
                                    FontId::proportional((width / 15.0).clamp(10.0, 18.0)),
                                    Color32::from_gray(235),
                                );
                            } else {
                                picture(ui, image);
                            }
                            // The bar under the slide: its number, and its group on the group's first slide.
                            let bar = Rect::from_min_max(pos2(rect.min.x, image.max.y), rect.max);
                            painter.rect_filled(bar, 0.0, color32(group.color));
                            let number = painter.text(
                                bar.left_center() + vec2(5.0, 0.0),
                                Align2::LEFT_CENTER,
                                (index + 1).to_string(),
                                FontId::proportional(12.5),
                                ink,
                            );
                            if starts_group {
                                painter.with_clip_rect(bar).text(
                                    pos2(number.max.x + 6.0, bar.center().y),
                                    Align2::LEFT_CENTER,
                                    group.name.as_str(),
                                    FontId::proportional(12.0),
                                    ink,
                                );
                            }
                        }
                    }
                    // Orange for the slide on screen; blue for one picked out; a dim blue for
                    // the slide that was on screen before it was cleared.
                    let outline = if show.live() == Some(here) {
                        Some(Stroke::new(2.0, LIVE_OUTLINE))
                    } else if state.selected_slide == Some(here) {
                        Some(Stroke::new(3.0, SELECTED_OUTLINE))
                    } else if show.live().is_none() && show.cursor() == Some(here) {
                        Some(Stroke::new(1.5, WAS_LIVE_OUTLINE))
                    } else if response.hovered() {
                        Some(Stroke::new(1.0, Color32::from_gray(150)))
                    } else {
                        None
                    };
                    if let Some(stroke) = outline {
                        painter.rect_stroke(rect, 0.0, stroke, StrokeKind::Outside);
                    }
                }
            });
        });
        ui.add_space(12.0);
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
                &Theme::builtin(),
                &mut |_| {
                    asked += 1;
                    None
                },
                None,
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

    /// Runs two frames and returns how many slide and media thumbnails the last one asked for.
    fn count_thumbs(ctx: &egui::Context, show: &Show, state: &mut HomeState) -> (usize, usize) {
        let (mut slides, mut media) = (0, 0);
        for _ in 0..2 {
            let output = ctx.run_ui(input(None), |ui| {
                (slides, media) = (0, 0);
                home_ui(
                    ui,
                    show,
                    state,
                    &Theme::builtin(),
                    &mut |which| {
                        match which {
                            ThumbOf::Slide(_) | ThumbOf::BibleSlide(_) | ThumbOf::Layout(..) => slides += 1,
                            ThumbOf::Media(_) => media += 1,
                        }
                        None
                    },
                    None,
                    None,
                );
            });
            output.drop_without_applying_deltas();
        }
        (slides, media)
    }

    fn show_with_media() -> Show {
        let mut show = show(3);
        for name in ["a.png", "b.png"] {
            show.add_media(pc_model::MediaItem { id: pc_model::Uuid::new_v4(), name: name.into(), path: name.into() });
        }
        show.apply(Command::ShowMedia(1));
        show
    }

    #[test]
    fn the_show_view_draws_the_media_bin_with_a_notice_and_a_drag() {
        let show = show_with_media();
        let mut state = HomeState {
            drop_hover: true,
            notice: Some("x.mp4: videos are not supported yet, only pictures.".into()),
            ..HomeState::default()
        };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(media, 2, "both media thumbnails should be asked for");
        assert_eq!(slides, 3);
    }

    #[test]
    fn the_edit_view_replaces_the_show_view_and_changes_nothing_by_itself() {
        let show = show_with_media();
        let editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 1 });
        let mut state = HomeState { editing, ..HomeState::default() };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(media, 0, "the media bin is not part of the Edit view");
        assert_eq!(slides, 3, "the slide list down the left shows the song's slides");
        assert_eq!(state.editing.as_ref().map(|d| d.at.slide), Some(1), "drawing must not leave the Edit view");
        assert!(state.pending_edit.is_none(), "nothing was typed, so nothing changed");
    }

    #[test]
    fn the_edit_view_closes_if_its_song_is_gone() {
        let show = show(2);
        let mut editing = EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 });
        if let Some(draft) = editing.as_mut() {
            draft.at = SlideRef { presentation: 5, slide: 0 };
        }
        let mut state = HomeState { editing, ..HomeState::default() };
        count_thumbs(&egui::Context::default(), &show, &mut state);
        assert!(state.editing.is_none());
    }

    #[test]
    fn the_bible_page_takes_the_place_of_the_slide_grid() {
        let show = show_with_media();
        let mut state = HomeState { bible_open: true, ..HomeState::default() };
        let (slides, media) = count_thumbs(&egui::Context::default(), &show, &mut state);
        assert_eq!(slides, 0, "the slide grid is not drawn while the Bible page is open");
        assert_eq!(media, 2, "the media bin stays");
    }

    #[test]
    fn bible_rows_can_be_added_up_to_a_limit_and_removed_down_to_one() {
        let mut page = BiblePage::default();
        assert_eq!(page.rows.len(), 2);
        assert!(page.add_row());
        assert!(page.add_row());
        assert!(!page.add_row(), "four translations is the limit");
        assert!(page.remove_row(3));
        assert!(!page.remove_row(9));
        assert!(page.remove_row(0) && page.remove_row(0));
        assert_eq!(page.rows.len(), 1);
        assert!(!page.remove_row(0), "one row always stays");
    }

    #[test]
    fn the_settings_window_draws_every_pane() {
        let show = show(2);
        let ctx = egui::Context::default();
        for pane in 0..SETTINGS_PANES.len() + 1 {
            let mut state = HomeState {
                settings_open: true,
                settings_pane: pane,
                displays: vec![
                    DisplayInfo { index: 0, name: "Built-in".into(), x: 0, y: 0, w: 3024, h: 1964 },
                    DisplayInfo { index: 1, name: "Projector".into(), x: 3024, y: 0, w: 1920, h: 1080 },
                ],
                output_placement: Placement::Fullscreen(1),
                ..HomeState::default()
            };
            count_thumbs(&ctx, &show, &mut state);
            assert!(state.settings_open, "drawing must not close Settings");
            assert_eq!(state.requested_placement, None, "nothing was clicked, so nothing is asked for");
        }
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
