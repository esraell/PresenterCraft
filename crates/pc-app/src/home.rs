//! The home window: toolbar, library, slide grid, media bin and the live panel.
//!
//! This draws one frame and returns what the operator asked for. It changes nothing itself.
//!
//! Many controls are placeholders for parts that are not built yet. They are drawn in their
//! final positions so the layout is settled, they are dim, and they say "not built yet".

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind, TextureId, pos2, vec2};
use pc_engine::{Command, Show, SlideRef};
use pc_model::{Rgba, Slide, SlideRect, Theme};
use pc_render::object_frame;

use crate::displays::DisplayInfo;
use crate::editor::EditDraft;
use crate::screens::{Kind, Output, Screen};

use crate::icons::{self, Icon};

/// Bars around the slide area and the media bin.
const BAR: Color32 = Color32::from_rgb(20, 22, 26);
/// The pills that hold toolbar items, and text fields.
const FIELD: Color32 = Color32::from_rgb(30, 33, 37);
const TAB_BLUE: Color32 = Color32::from_rgb(76, 141, 255);
const SIDE: Color32 = Color32::from_rgb(20, 22, 26);
const CANVAS: Color32 = Color32::from_rgb(14, 15, 17);
const ON: Color32 = Color32::from_rgb(50, 208, 116);
const DIM: Color32 = Color32::from_rgb(154, 160, 168);

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
    /// Whether one slide fades into the next, and over how many seconds.
    /// How one slide gives way to the next on the screens.
    pub transition: pc_model::Transition,
    /// True on the frame a rename began in the Objects list, so its field takes the keyboard.
    pub rename_started: bool,
    /// Set when a new song is asked for (Command-N). The app makes it and clears this.
    pub new_song: bool,
    /// The song's title while it is being retyped in the heading, and the title just typed,
    /// with the song's place, for the app to take.
    pub renaming_song: Option<String>,
    pub pending_title: Option<(usize, String)>,
    /// The group being renamed in the editor's slide list (its place in the song) and the name
    /// as typed; and a name just typed, with the song's and group's places, for the app to take.
    pub renaming_group: Option<(usize, String)>,
    pub pending_group_name: Option<(usize, usize, String)>,
    /// The theme the operator has asked to delete, while they are being asked to confirm.
    pub confirm_delete_theme: Option<usize>,
    /// The open theme's name while it is being retyped in the list of themes.
    pub renaming_theme: Option<String>,
    /// How tall the service's part of the left column is, as last drawn; kept between runs.
    pub service_height: Option<f32>,
    /// The song a new slide has just been asked for, at its end. The app adds it.
    pub add_slide: Option<usize>,
    /// Objects copied in the editor, and the words that went to the system clipboard with them.
    pub object_clip: Vec<pc_model::Object>,
    pub object_clip_text: String,
    /// The running orders, and the one in front.
    pub services: crate::service::Services,
    /// For each presentation, the name its song is found by again in a service.
    pub song_keys: Vec<String>,
    /// The service's name while it is being retyped.
    pub renaming_service: Option<String>,
    /// Displayed thumbnail width in points.
    /// Text typed into the library's filter box.
    pub filter: String,
    /// Set by the app when the output has stopped drawing, so the operator is told.
    pub output_stopped: bool,
    /// Displayed media thumbnail width in points.
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
    /// A slide cut or copied from its menu, for Paste.
    pub slide_clip: Option<Slide>,
    /// A change to a slide asked for from its menu.
    pub slide_request: Option<(SlideRef, SlideRequest)>,
    /// A change to a song asked for from its menu, and a song waiting to be confirmed deleted.
    pub song_request: Option<(usize, SongRequest)>,
    pub confirm_delete_song: Option<usize>,
    /// The Reflow editor, while it is open: the song being reflowed and its words as text.
    pub reflow: Option<(usize, String)>,
    /// Words finished in the Reflow editor, for the app to rebuild the song from.
    pub reflow_done: Option<(usize, String)>,
    /// The library search pop-up (Command-F), while it is open: what is typed, and the row
    /// picked out with the arrow keys.
    pub library_search: Option<(String, usize)>,
    /// A change to the media bin asked for.
    pub media_request: Option<MediaRequest>,
    /// Songs taken out of the library this run, by their place in the show.
    pub removed_songs: Vec<usize>,
    /// A change to the selected objects asked for from the menu bar.
    pub edit_request: Option<EditRequest>,
    /// A view asked for from the menu bar: 0 Show, 1 Edit, 2 Bible, 3 Themes. The toolbar
    /// takes it as if its tab had been clicked.
    pub view_request: Option<usize>,
    /// True while the Settings window is open, and which of its panes is showing.
    pub preferences_open: bool,
    pub preferences_pane: usize,
    /// When a song's copyright line is shown, and the church's CCLI licence number.
    pub copyright_show: crate::service::CopyrightShow,
    pub ccli_licence: String,
    /// True to start with the audience screens off.
    pub start_audience_off: bool,
    /// New copyright details for a song, from its Info pop-over.
    pub copyright_request: Option<(usize, pc_model::Copyright)>,
    /// True while the Screens window is open.
    pub settings_open: bool,
    /// The displays connected now, kept up to date by the app.
    pub displays: Vec<DisplayInfo>,
    /// The audience screens, in order. The Screens window changes this list and sets
    /// `screens_changed`; the app then saves it and moves the pictures.
    pub screens: Vec<Screen>,
    pub screens_changed: bool,
    /// What each screen is doing now, in words, and whether it is being shown somewhere.
    pub screen_status: Vec<(String, bool)>,
    /// The screen chosen in the Screens window.
    pub screen_selected: usize,
    /// A service header being renamed, by its place, and its name as typed so far.
    pub renaming_header: Option<(usize, String)>,
    /// True while the library is folded away.
    pub library_collapsed: bool,
    /// Next at the end of a song goes on to the next song in the service.
    pub continuous: bool,
    /// A test pattern shown on the audience screens in place of everything else.
    pub test_pattern: Option<crate::props::TestPattern>,
    /// True while the notes of the slide on screen are shown above the slides.
    pub show_notes: bool,
    /// The props, and true when they have changed and are to be saved.
    pub props: Vec<crate::props::Prop>,
    pub props_changed: bool,
    /// The props on screen, in the order they were put on, and since when.
    pub props_live: Vec<(pc_model::Uuid, std::time::Instant)>,
    /// A prop being renamed, and its name as typed so far.
    pub prop_rename: Option<(pc_model::Uuid, String)>,
    /// The tab open under the Program panel: 0 Timers, 1 Messages, 2 Props, 3 Stage.
    pub live_tab: usize,
    /// The timers, and true when their settings have changed and are to be saved.
    pub timers: Vec<crate::timers::Timer>,
    pub timers_changed: bool,
    /// The timer whose settings are open.
    pub timer_open: Option<pc_model::Uuid>,
    /// The messages, and true when they have changed and are to be saved.
    pub messages: Vec<crate::messages::Message>,
    pub messages_changed: bool,
    pub message_selected: Option<pc_model::Uuid>,
    /// The message showing over the slides, and since when.
    pub message_live: Option<(pc_model::Uuid, std::time::Instant)>,
    /// The note for the people on stage, and whether it is showing on the stage screens.
    pub stage_message: String,
    pub stage_message_live: bool,
    /// True while the stage screens are on.
    pub stage_on: bool,
    /// True while every screen shows its own name instead of the show.
    pub identify: bool,
    /// An NDI name being typed in the Screens window, and the screen it is for. It is used
    /// when the field is left, so NDI is not restarted at every letter.
    pub ndi_draft: Option<(pc_model::Uuid, String)>,
    /// True when this build includes NDI.
    pub ndi_available: bool,
    /// The Edit view's right-hand panel.
    pub inspector: crate::inspector::InspectorState,
    /// The Theme Editor, while it is open. The layout being worked on is the draft in `editing`.
    pub theme_edit: Option<ThemeEdit>,
    /// Every theme's name, and the places of the ones songs and Bible slides use.
    pub theme_names: Vec<String>,
    /// What each theme calls its two templates, the one for two languages first.
    pub template_names: Vec<Vec<String>>,
    /// The templates of the theme songs use, by id and name, for a slide's Template menu.
    pub song_templates: Vec<(pc_model::Uuid, String)>,
    /// A slide whose template was just chosen from its menu (`None` lets the theme choose).
    /// The app changes the slide and the screen.
    pub pending_template: Option<(SlideRef, Option<pc_model::Uuid>)>,
    /// The template being renamed in the Theme Editor, and its name as typed so far.
    pub renaming_template: Option<(usize, String)>,
    /// The template of the theme being edited that is about to be deleted, once confirmed.
    pub confirm_delete_template: Option<usize>,
    /// Each theme's two layouts as boxes, for the pictures in the theme picker.
    pub theme_layouts: Vec<Vec<Vec<(pc_model::Role, SlideRect)>>>,
    /// Which layout of its theme Bible slides use.
    pub bible_layout: usize,
    pub theme_for_songs: usize,
    pub theme_for_bible: usize,
    /// What the Theme menu or the Theme Editor has just asked for. The app takes this.
    pub theme_request: Option<ThemeRequest>,
    /// Set when Edit was pressed in the Theme Editor: the slide editor opens once it has closed.
    pub open_editor_next: bool,
    /// What is typed in the Bible page's two fields, and its rows of translations.
    pub bible: BiblePage,
}

/// Where a translation's words, or its reference, go on a Bible slide: one of the Bible
/// theme's boxes, or nowhere.
pub type BibleTarget = Option<pc_model::Role>;

/// The boxes one of a theme's templates has, and where each sits.
pub fn layout_boxes(theme: &Theme, template: usize) -> Vec<(pc_model::Role, SlideRect)> {
    let boxes = theme.templates.get(template).map_or(&[][..], |template| template.boxes.as_slice());
    boxes.iter().map(|style| (style.role.clone(), style.rect)).collect()
}

/// What each box of a template is called, in the order `layout_boxes` gives them.
pub fn layout_labels(theme: &Theme, template: usize) -> Vec<String> {
    let boxes = theme.templates.get(template).map_or(&[][..], |template| template.boxes.as_slice());
    boxes.iter().map(pc_model::BoxStyle::label).collect()
}

/// A small picture of a slide with its boxes outlined and one of them filled, to show which
/// box a menu entry means.
pub fn box_diagram(
    painter: &egui::Painter,
    area: Rect,
    boxes: &[(pc_model::Role, SlideRect)],
    lit: Option<&pc_model::Role>,
) {
    painter.rect_filled(area, 2.0, Color32::BLACK);
    let scale = area.width() / pc_model::SLIDE_WIDTH;
    for (role, frame) in boxes {
        let rect = Rect::from_min_size(area.min + vec2(frame.x, frame.y) * scale, vec2(frame.w, frame.h) * scale);
        if lit == Some(role) {
            painter.rect_filled(rect, 1.0, Color32::from_rgb(250, 204, 21));
        } else {
            painter.rect_stroke(rect, 1.0, Stroke::new(1.0, Color32::from_gray(90)), StrokeKind::Inside);
        }
    }
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
    /// The boxes of the theme layout Bible slides use, and where each sits on the slide. The
    /// Verse and Reference menus offer these.
    pub boxes: Vec<(pc_model::Role, SlideRect)>,
    /// What each of those boxes is called, in the same order.
    pub box_labels: Vec<String>,
    /// The template of the Bible theme that Bible slides are drawn with.
    pub template: Option<pc_model::Uuid>,
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
    /// Verses found by the last search: the verse's reference, its chapter's, and its words.
    pub found: Vec<(String, String, String)>,
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
            boxes: layout_boxes(&Theme::builtin(), 0),
            box_labels: layout_labels(&Theme::builtin(), 0),
            template: None,
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

    /// Adds a translation row, up to the limit. Returns true if one was added. When every
    /// installed Bible is already in a row, it says so, as ProPresenter does.
    pub fn add_row(&mut self) -> bool {
        // The next Bible no row is using yet.
        let unused = (0..self.shelf.bibles.len()).find(|at| self.rows.iter().all(|row| row.bible != *at));
        let Some(bible) = unused.filter(|_| self.rows.len() < Self::MAX_ROWS) else {
            if unused.is_none() {
                self.message = Some(
                    "All available bibles are in use. If you would like to add another translation, please install more bibles."
                        .to_owned(),
                );
            }
            return false;
        };
        self.rows.push(BibleRow { bible, verse: None, reference: None });
        true
    }

    /// Removes a downloaded Bible, and points any row that used it at the first Bible.
    pub fn remove_bible(&mut self, index: usize) {
        match self.shelf.remove(index) {
            Ok(()) => {
                for row in &mut self.rows {
                    if row.bible == index {
                        row.bible = 0;
                    } else if row.bible > index {
                        row.bible -= 1;
                    }
                }
                self.made_with.clear();
            }
            Err(problem) => self.options.problem = Some(problem),
        }
    }

    /// Points each translation at the boxes of the layout now in use, as choosing a theme
    /// does: the first translation's words in the main box and its reference in the reference
    /// box, the second's words in the translation box, the rest nowhere.
    pub fn assign_boxes(&mut self) {
        let has = |role: pc_model::Role| self.boxes.iter().any(|(have, _)| *have == role).then_some(role);
        let main = has(pc_model::Role::primary()).or_else(|| self.boxes.first().map(|(role, _)| role.clone()));
        let reference = has(pc_model::Role::reference());
        let translation = has(pc_model::Role::translation());
        for (index, row) in self.rows.iter_mut().enumerate() {
            (row.verse, row.reference) = match index {
                0 => (main.clone(), reference.clone()),
                1 => (translation.clone(), None),
                _ => (None, None),
            };
        }
        // Whatever is showing is made again with the new boxes.
        self.made_with.clear();
    }

    /// The passage now shown, as something a service can keep.
    pub fn saved(&self) -> Option<crate::service::SavedPassage> {
        (!self.shown.is_empty()).then(|| self.settings())
    }

    /// How the page is set up now: its translations, the boxes their words and references
    /// go in, and the slide options. Kept between runs, and with each saved passage.
    pub fn settings(&self) -> crate::service::SavedPassage {
        use crate::bible::ReferenceStyle;
        let rows = self
            .rows
            .iter()
            .filter_map(|row| {
                Some(crate::service::SavedRow {
                    bible: self.shelf.bibles.get(row.bible)?.id.clone(),
                    verse: row.verse.as_ref().map(|role| role.0.clone()),
                    reference: row.reference.as_ref().map(|role| role.0.clone()),
                })
            })
            .collect();
        let o = &self.options;
        crate::service::SavedPassage {
            lookup: self.shown.clone(),
            rows,
            show_verse_numbers: o.show_verse_numbers,
            break_on_new_verse: o.break_on_new_verse,
            display_translation: o.display_translation,
            reference_style: match o.reference {
                ReferenceStyle::VerseReferences => "verse",
                ReferenceStyle::PassageEach => "passage_each",
                ReferenceStyle::PassageLast => "passage_last",
                ReferenceStyle::NoReference => "none",
            }
            .to_owned(),
        }
    }

    /// Brings a saved passage back: its translations and options are set as they were, and
    /// it is looked up. A translation that is no longer installed is left out, and said so.
    pub fn open_saved(&mut self, saved: &crate::service::SavedPassage, live: Option<usize>) {
        let missing = self.apply_settings(saved);
        self.lookup = saved.lookup.clone();
        self.look_up(live);
        if !missing.is_empty() && self.message.is_none() {
            self.message = Some(format!("Not installed, so left out: {}.", missing.join(", ")));
        }
    }

    /// Sets the page up as it was saved: translations, boxes and slide options. Returns the
    /// translations that are no longer installed, which are left out.
    pub fn apply_settings(&mut self, saved: &crate::service::SavedPassage) -> Vec<String> {
        use crate::bible::ReferenceStyle;
        let mut rows = Vec::new();
        let mut missing = Vec::new();
        // A box the Bible template no longer has is not a place to send anything.
        let known = |role: &Option<String>| {
            role.clone().map(pc_model::Role).filter(|role| self.boxes.iter().any(|(have, _)| have == role))
        };
        for row in &saved.rows {
            match self.shelf.position(&row.bible) {
                Some(bible) => {
                    rows.push(BibleRow { bible, verse: known(&row.verse), reference: known(&row.reference) })
                }
                None => missing.push(row.bible.to_uppercase()),
            }
        }
        if !rows.is_empty() {
            self.rows = rows;
        }
        let o = &mut self.options;
        o.show_verse_numbers = saved.show_verse_numbers;
        o.break_on_new_verse = saved.break_on_new_verse;
        o.display_translation = saved.display_translation;
        o.reference = match saved.reference_style.as_str() {
            "passage_each" => ReferenceStyle::PassageEach,
            "passage_last" => ReferenceStyle::PassageLast,
            "none" => ReferenceStyle::NoReference,
            _ => ReferenceStyle::VerseReferences,
        };
        missing
    }

    /// The choices a passage is made with, as text, so a change to any of them can be noticed.
    pub fn choices(&self) -> String {
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
        match crate::bible::build_passage(&self.lookup, &rows, &self.options, self.template) {
            Ok(passage) => {
                let passage_name = passage.title.clone();
                // Leave a passage that is on screen alone: use the other slot.
                if live == Some(slots[self.showing]) {
                    self.showing = 1 - self.showing;
                }
                self.pending = Some((slots[self.showing], passage));
                // The field then says the passage in full, in the first Bible's language:
                // "john 3" becomes "John 3:1-36".
                self.lookup = passage_name;
                self.shown = self.lookup.clone();
                self.made_with = self.choices();
                self.message = None;
                self.found.clear();
            }
            Err(problem) => self.message = Some(problem),
        }
    }

    /// Makes the passage shown one verse longer, at its end or at its start.
    pub fn step(&mut self, forward: bool, live: Option<usize>) {
        let Some(row) = self.rows.first() else { return };
        let (Ok(bible), Ok(range)) = (self.shelf.get(row.bible), pc_bible::parse_reference(&self.shown)) else {
            return;
        };
        match bible.extend(range, forward) {
            Some(longer) => {
                self.lookup = bible.label(longer.start, longer.end);
                self.look_up(live);
            }
            None => {
                let end = if forward { "end" } else { "start" };
                self.message = Some(format!("That is the {end} of {}.", bible.book_name(range.start.book)));
            }
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
                    let chapter = format!("{} {}", bible.book_name(place.book), place.chapter);
                    self.found.push((bible.label(place, place), chapter, words.to_owned()));
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
    pub template: usize,
    /// Set when the theme has changed this frame. The app takes the theme and clears this.
    pub changed: bool,
    /// Set when the operator asks to go back to the Show view.
    pub leave: bool,
}

impl ThemeEdit {
    pub fn of(index: usize, theme: Theme) -> ThemeEdit {
        ThemeEdit { index, theme, template: 0, changed: false, leave: false }
    }
}

/// Something asked of the app about themes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThemeRequest {
    /// Songs use the theme at this place in the list.
    UseForSongs(usize),
    /// Bible slides use this layout of the theme at this place in the list.
    UseForBible(usize, usize),
    /// Make a new theme, a copy of the one songs use, and open it in the Theme Editor.
    New,
    /// Open the Theme Editor on the theme at this place in the list.
    Edit(usize),
    /// Delete the theme at this place in the list, for good.
    Delete(usize),
}

/// The picture of the audience screen for the Program panel: what is there now, what was
/// there before, and how far the new one has faded in (1 when there is no fade).
#[derive(Clone, Copy, Debug, Default)]
pub struct Preview {
    pub now: Option<TextureId>,
    pub before: Option<TextureId>,
    pub amount: f32,
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
            transition: pc_model::Transition::default(),
            rename_started: false,
            new_song: false,
            renaming_song: None,
            pending_title: None,
            renaming_group: None,
            pending_group_name: None,
            confirm_delete_theme: None,
            renaming_theme: None,
            service_height: None,
            add_slide: None,
            object_clip: Vec::new(),
            object_clip_text: String::new(),
            services: crate::service::Services::default(),
            song_keys: Vec::new(),
            renaming_service: None,
            filter: String::new(),
            output_stopped: false,
            drop_hover: false,
            notice: None,
            editing: None,
            pending_edit: None,
            live_edit: false,
            fonts: std::sync::Arc::default(),
            bible_open: false,
            settings_open: false,
            displays: Vec::new(),
            view_request: None,
            preferences_open: false,
            preferences_pane: 0,
            copyright_show: crate::service::CopyrightShow::Never,
            ccli_licence: String::new(),
            start_audience_off: false,
            copyright_request: None,
            edit_request: None,
            slide_clip: None,
            slide_request: None,
            song_request: None,
            confirm_delete_song: None,
            removed_songs: Vec::new(),
            media_request: None,
            library_search: None,
            reflow: None,
            reflow_done: None,
            screens: crate::screens::starting(None),
            screens_changed: false,
            screen_status: Vec::new(),
            screen_selected: 0,
            show_notes: false,
            library_collapsed: false,
            renaming_header: None,
            continuous: false,
            test_pattern: None,
            props: Vec::new(),
            props_changed: false,
            props_live: Vec::new(),
            prop_rename: None,
            live_tab: 0,
            timers: Vec::new(),
            timers_changed: false,
            timer_open: None,
            messages: Vec::new(),
            messages_changed: false,
            message_selected: None,
            message_live: None,
            stage_message: String::new(),
            stage_message_live: false,
            stage_on: true,
            identify: false,
            ndi_draft: None,
            ndi_available: pc_ndi::NdiOutput::AVAILABLE,
            inspector: crate::inspector::InspectorState::default(),
            theme_edit: None,
            theme_names: Vec::new(),
            template_names: Vec::new(),
            song_templates: Vec::new(),
            pending_template: None,
            renaming_template: None,
            confirm_delete_template: None,
            theme_layouts: Vec::new(),
            bible_layout: 0,
            theme_for_songs: 0,
            theme_for_bible: 0,
            theme_request: None,
            open_editor_next: false,
            bible: BiblePage::default(),
        }
    }
}

impl HomeState {
    /// The small choices kept between runs, as they are now.
    pub fn prefs(&self) -> crate::service::Prefs {
        crate::service::Prefs {
            dissolve: self.transition.kind != pc_model::TransitionKind::Cut,
            dissolve_time: self.transition.seconds,
            transition: Some(self.transition),
            columns: self.columns,
            slide_view: match self.slide_view {
                SlideView::Grid => "grid",
                SlideView::Easy => "easy",
                SlideView::Table => "table",
            }
            .to_owned(),
            service_height: self.service_height.map(f32::round),
            continuous: self.continuous,
            copyright_show: self.copyright_show,
            ccli_licence: self.ccli_licence.clone(),
            start_audience_off: self.start_audience_off,
            stage_off: !self.stage_on,
            ..crate::service::Prefs::default()
        }
    }

    /// Sets the small choices as they were saved.
    pub fn apply_prefs(&mut self, prefs: &crate::service::Prefs) {
        self.transition = prefs.transition.unwrap_or_else(|| {
            if prefs.dissolve {
                pc_model::Transition { seconds: prefs.dissolve_time.clamp(0.1, 5.0), ..pc_model::Transition::default() }
            } else {
                pc_model::Transition::of(pc_model::TransitionKind::Cut)
            }
        });
        self.columns = prefs.columns.clamp(2, 7);
        self.slide_view = match prefs.slide_view.as_str() {
            "easy" => SlideView::Easy,
            "table" => SlideView::Table,
            _ => SlideView::Grid,
        };
        self.service_height = prefs.service_height;
        self.continuous = prefs.continuous;
        self.copyright_show = prefs.copyright_show;
        self.ccli_licence = prefs.ccli_licence.clone();
        self.start_audience_off = prefs.start_audience_off;
        self.stage_on = !prefs.stage_off;
    }

    /// How long a new picture takes to fade in on the output; zero when slides cut.
    pub fn dissolve_seconds(&self) -> f32 {
        self.transition.duration()
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

/// True when this frame has a press of `key` with Command (and Shift, exactly when `shift`).
/// It is judged by the key press itself, not by the keys held now, so a shortcut chosen from
/// the menu bar, which arrives with no key held, counts as well.
fn command_key(input: &egui::InputState, key: egui::Key, shift: bool) -> bool {
    input.events.iter().any(|event| {
        matches!(event, egui::Event::Key { key: pressed_key, pressed: true, modifiers, .. }
            if *pressed_key == key && modifiers.command && modifiers.shift == shift)
    })
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
    preview: Preview,
    canvas: Option<TextureId>,
) -> Vec<Command> {
    let mut commands = Vec::new();
    // Text boxes are dark wells in the lighter side columns.
    ui.visuals_mut().extreme_bg_color = FIELD;
    crate::studio::toolbar(ui, show, state, &mut commands);
    if ui.input(|input| command_key(input, egui::Key::Comma, false)) {
        state.preferences_open = true;
    }
    // Command-F searches the library (on the Mac the menu bar's Find does this).
    if ui.input(|input| command_key(input, egui::Key::F, false)) {
        state.library_search = Some((String::new(), 0));
    }
    // Command-N makes a new song, wherever the keyboard is not in a field.
    if !ui.ctx().text_edit_focused() && ui.input(|input| command_key(input, egui::Key::N, false)) {
        state.new_song = true;
    }
    confirm_delete_theme(ui, state);
    confirm_delete_song(ui, show, state);
    library_search(ui, show, state, &mut commands);
    if state.editing.is_some() {
        edit_view(ui, show, state, theme, thumb, canvas);
        return commands;
    }
    crate::studio::library(ui, show, state, &mut commands);
    crate::studio::program_panel(ui, show, state, preview, &mut commands);
    crate::studio::media_strip(ui, show, state, thumb, &mut commands);
    if state.bible_open {
        bible_area(ui, show, state, thumb, &mut commands);
    } else {
        slide_area(ui, show, state, theme, thumb, &mut commands);
    }
    commands
}

const EDIT_PANEL: Color32 = Color32::from_rgb(20, 22, 26);
const EDIT_HEADER: Color32 = Color32::from_rgb(26, 29, 33);
const EDIT_SURROUND: Color32 = Color32::from_rgb(38, 41, 46);
const EDIT_STATUS: Color32 = Color32::from_rgb(20, 22, 26);
const EDIT_SELECTED: Color32 = Color32::from_rgb(36, 67, 122);
const EDIT_OUTLINE: Color32 = Color32::from_rgb(76, 141, 255);
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
    let prop_mode = at.presentation == crate::props::PROP_SLOT;
    if presentation.is_none() && !theme_mode && !prop_mode {
        state.editing = None;
        return;
    }
    let before = state.editing.as_ref().map(|draft| draft.slide.clone());
    let now = ui.input(|input| input.time);
    let typing = ui.ctx().text_edit_focused();

    // Keys: undo and redo always; delete and nudging when not typing in a box.
    let (undo, redo, delete, nudge) = ui.input(|input| {
        let undo = command_key(input, egui::Key::Z, false);
        let redo = command_key(input, egui::Key::Z, true);
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
    // Copy, paste, duplicate and select all act on objects unless a field is being typed in.
    if !typing {
        let (copy, cut, pasted, duplicate, all) = ui.input(|input| {
            let has = |wanted: fn(&egui::Event) -> bool| input.events.iter().any(wanted);
            let pasted = input.events.iter().find_map(|event| match event {
                egui::Event::Paste(text) => Some(text.clone()),
                _ => None,
            });
            (
                has(|event| matches!(event, egui::Event::Copy)),
                has(|event| matches!(event, egui::Event::Cut)),
                pasted,
                command_key(input, egui::Key::D, false),
                command_key(input, egui::Key::A, false),
            )
        });
        if let Some(draft) = state.editing.as_mut() {
            if (copy || cut || duplicate) && draft.selected.is_some() {
                let copies = draft.copied(theme);
                if duplicate {
                    draft.paste(&copies);
                } else {
                    // The words also go to the system clipboard, so they can be pasted
                    // elsewhere, and so a paste here can tell these are the objects meant.
                    let words: Vec<&str> = copies.iter().filter_map(pc_model::Object::as_text).collect();
                    state.object_clip_text = words.join("\n");
                    ui.ctx().copy_text(state.object_clip_text.clone());
                    state.object_clip = copies;
                    if cut {
                        draft.delete_selected();
                    }
                }
            } else if let Some(text) = pasted
                && !state.object_clip.is_empty()
                && text == state.object_clip_text
            {
                draft.paste(&state.object_clip);
                // Pasting again lands further on, not on top of the last copy.
                state.object_clip = draft.copied(theme);
            } else if all {
                draft.select_all();
            }
        }
    }
    // In a song's editor, a letter adds an object: T a text box, and each shape its own key.
    if !typing && !theme_mode && !ui.ctx().text_edit_focused() && state.editing.is_some() {
        let pressed = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Text(text) if !input.modifiers.command && !input.modifiers.ctrl => {
                    text.chars().next().map(|c| c.to_ascii_uppercase())
                }
                _ => None,
            })
        });
        if let (Some(key), Some(draft)) = (pressed, state.editing.as_mut()) {
            if key == 'T' {
                draft.add_text();
            } else if let Some(at) = SHAPE_KEYS.iter().position(|have| *have == key) {
                draft.add_shape(pc_model::Shape::ALL[at]);
            }
        }
    }
    // A change asked for from the menu bar.
    if let (Some(request), Some(draft)) = (state.edit_request.take(), state.editing.as_mut()) {
        match request {
            EditRequest::Bold => {
                let bold = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.weight >= 600);
                draft.restyle(theme, |look| look.weight = if bold { 400 } else { 700 });
            }
            EditRequest::Italic => {
                let italic = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.italic);
                draft.restyle(theme, |look| look.italic = !italic);
            }
            EditRequest::Underline => {
                let underline = draft
                    .selected_object()
                    .and_then(|object| theme.text_style_on(&draft.slide, object.role.as_ref(), object.text_style()?))
                    .is_some_and(|look| look.underline);
                draft.restyle(theme, |look| look.underline = !underline);
            }
            EditRequest::Bigger => {
                draft.restyle(theme, |look| look.size = (look.size * 1.1).round().min(1000.0));
            }
            EditRequest::Smaller => {
                draft.restyle(theme, |look| look.size = (look.size / 1.1).round().max(8.0));
            }
            EditRequest::Align(align) => {
                draft.restyle(theme, |look| look.align = align);
            }
            EditRequest::UpperCase => {
                draft.retext(str::to_uppercase);
            }
            EditRequest::LowerCase => {
                draft.retext(str::to_lowercase);
            }
            EditRequest::Capitalize => {
                draft.retext(capitalize);
            }
            EditRequest::CopyStyle => {
                state.inspector.copied_style = draft.selected_object().and_then(|object| object.text_style().cloned());
            }
            EditRequest::PasteStyle => {
                if let Some(copied) = state.inspector.copied_style.clone() {
                    let ids = draft.selection();
                    for object in draft.slide.objects.iter_mut().filter(|object| ids.contains(&object.id)) {
                        if let Some(style) = object.text_style_mut() {
                            *style = copied.clone();
                        }
                    }
                }
            }
            EditRequest::BringForward => {
                draft.restack(true);
            }
            EditRequest::SendBackward => {
                draft.restack(false);
            }
            EditRequest::BringToFront => {
                draft.move_to_end_of_stack(true);
            }
            EditRequest::SendToBack => {
                draft.move_to_end_of_stack(false);
            }
            EditRequest::Lock => {
                draft.lock(true);
            }
            EditRequest::UnlockAll => {
                draft.lock(false);
            }
            EditRequest::DeselectAll => draft.select_only(None),
        }
    }
    if let Some(draft) = state.editing.as_mut() {
        if undo && !typing {
            draft.undo();
        } else if redo && !typing {
            draft.redo();
        } else if delete {
            draft.delete_selected();
        } else if nudge != (0.0, 0.0) && draft.selected.is_some() {
            let snapshot = draft.slide.clone();
            draft.nudge(theme, nudge);
            draft.record_run(snapshot, now);
        }
    }

    // Delete with no box selected, in the Theme Editor, is for the template itself.
    let delete_template_key = delete
        && theme_mode
        && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none())
        && state.renaming_template.is_none()
        && state.confirm_delete_template.is_none()
        && state.confirm_delete_theme.is_none();
    // Left: the slide navigator, with the slide's objects below it.
    let mut chosen = None;
    let mut chosen_template = None;
    // The Theme Editor's column is a little wider, for the list of themes.
    let column = if theme_mode { 224.0 } else { 186.0 };
    egui::Panel::left("edit slides").resizable(false).exact_size(column).frame(panel_frame(EDIT_PANEL, 0)).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if theme_mode {
                theme_list(ui, state);
            } else if let Some(presentation) = presentation {
                Frame::new().fill(EDIT_HEADER).inner_margin(egui::Margin::symmetric(8, 5)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(presentation.title.as_str()).strong().size(12.5).color(Color32::WHITE),
                        );
                        ui.add_space(ui.available_width());
                    });
                });
            }

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
                            }
                            let mut asked: Option<(pc_model::Uuid, Ask)> = None;
                            // A rename that has just begun takes the keyboard once; one that has
                            // ended is carried out after the list is drawn.
                            let mut rename_started = std::mem::take(&mut state.rename_started);
                            let mut rename_done: Option<Option<(pc_model::Uuid, String)>> = None;
                            let mut start_rename = None;
                            let extend = ui.input(|input| input.modifiers.shift || input.modifiers.command);
                            // Rows can be dragged up and down: the order is front to back.
                            let mut rows: Vec<(pc_model::Uuid, Rect)> = Vec::new();
                            let mut dragging = None;
                            let mut dropped = None;
                            for object in draft.slide.objects.iter().rev() {
                                let selected = draft.is_selected(object.id);
                                let (row, response) =
                                    ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click_and_drag());
                                rows.push((object.id, row));
                                if response.dragged() {
                                    dragging = Some(object.id);
                                }
                                if response.drag_stopped() {
                                    dropped = Some(object.id);
                                }
                                if selected {
                                    ui.painter().rect_filled(row, 0.0, EDIT_SELECTED);
                                } else if response.hovered() {
                                    ui.painter().rect_filled(row, 0.0, Color32::from_gray(46));
                                }
                                // A hidden object's row is greyed.
                                let ink = if object.hidden { Color32::from_gray(120) } else { Color32::from_gray(230) };
                                // T for text, a diamond for a shape.
                                let mark = if object.as_text().is_some() { "T" } else { "◆" };
                                ui.painter().text(
                                    row.left_center() + vec2(12.0, 0.0),
                                    Align2::LEFT_CENTER,
                                    mark,
                                    FontId::proportional(if mark == "T" { 15.0 } else { 12.0 }),
                                    ink,
                                );
                                let name_area = Rect::from_min_max(row.min, pos2(row.max.x - 46.0, row.max.y));
                                // The name, or a field in its place while it is being retyped.
                                let being_renamed = draft.renaming.as_ref().is_some_and(|(id, _)| *id == object.id);
                                if being_renamed && let Some((_, name)) = draft.renaming.as_mut() {
                                    let field = crate::studio::rename_field_at(
                                        ui,
                                        name,
                                        FontId::proportional(13.0),
                                        crate::studio::TEXT,
                                        crate::studio::ACCENT,
                                        row.left_center() + vec2(32.0, 0.0),
                                        row.max.x - 46.0,
                                    );
                                    if std::mem::take(&mut rename_started) {
                                        field.request_focus();
                                    }
                                    let (enter, escape) = ui.input(|input| {
                                        (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape))
                                    });
                                    if escape {
                                        rename_done = Some(None);
                                    } else if enter || field.lost_focus() {
                                        rename_done = Some(Some((object.id, name.clone())));
                                    }
                                } else {
                                    let shown = if theme_mode { object.name.clone() } else { listed_name(object) };
                                    ui.painter().with_clip_rect(name_area).text(
                                        row.left_center() + vec2(32.0, 0.0),
                                        Align2::LEFT_CENTER,
                                        shown,
                                        FontId::proportional(13.0),
                                        ink,
                                    );
                                }
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
                                // A click selects; a click on the name of the one already selected,
                                // or a double click, starts renaming it, as in a file list.
                                let on_name = response
                                    .interact_pointer_pos()
                                    .is_some_and(|at| name_area.contains(at) && at.x > row.min.x + 26.0);
                                if response.double_clicked()
                                    || (response.clicked() && !extend && selected && draft.also.is_empty() && on_name)
                                {
                                    start_rename = Some((object.id, object.name.clone()));
                                } else if response.clicked() {
                                    pick = Some(object.id);
                                }
                                response.context_menu(|ui| {
                                    if ui.button("Rename").clicked() {
                                        start_rename = Some((object.id, object.name.clone()));
                                        ui.close();
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
                            // Where a dragged row would land: a line between the rows, and on
                            // release the object moves there in the stack.
                            let pointer_y = ui.input(|input| input.pointer.interact_pos()).map(|at| at.y);
                            let slot = pointer_y.map(|y| rows.iter().filter(|(_, row)| row.center().y < y).count());
                            if let (Some(_), Some(slot)) = (dragging, slot) {
                                let y = rows.get(slot).map_or_else(
                                    || rows.last().map_or(0.0, |(_, row)| row.max.y),
                                    |(_, row)| row.min.y,
                                );
                                let (left, right) = rows.first().map_or((0.0, 0.0), |(_, row)| (row.min.x, row.max.x));
                                ui.painter().line_segment(
                                    [pos2(left + 6.0, y), pos2(right - 6.0, y)],
                                    Stroke::new(2.0, crate::studio::ACCENT),
                                );
                            }
                            if let (Some(id), Some(slot)) = (dropped, slot) {
                                draft.move_in_stack(id, slot);
                            }
                            if let Some((id, ask)) = asked {
                                if matches!(ask, Ask::Delete) {
                                    draft.selected = Some(id);
                                    draft.delete_selected();
                                } else if let Some(object) = draft.slide.objects.iter_mut().find(|o| o.id == id) {
                                    match ask {
                                        Ask::Hide(hidden) => object.hidden = hidden,
                                        Ask::Lock(locked) => object.locked = locked,
                                        Ask::Delete => {}
                                    }
                                }
                            }
                            if draft.slide.objects.is_empty() {
                                ui.label(egui::RichText::new("   This slide is empty").size(12.0).color(DIM));
                            }
                            if let Some(id) = pick {
                                if extend {
                                    draft.toggle_selected(id);
                                } else {
                                    draft.select_only(Some(id));
                                }
                            }
                            match rename_done {
                                Some(Some((id, name))) => {
                                    if !name.trim().is_empty()
                                        && let Some(object) = draft.slide.objects.iter_mut().find(|o| o.id == id)
                                    {
                                        object.name = name.trim().to_owned();
                                    }
                                    draft.renaming = None;
                                }
                                Some(None) => draft.renaming = None,
                                None => {}
                            }
                            if let Some((id, name)) = start_rename {
                                draft.select_only(Some(id));
                                draft.renaming = Some((id, name));
                                rename_started = true;
                            }
                            state.rename_started = rename_started;
                        },
                    );
                });

            if let Some(edit) = state.theme_edit.as_ref() {
                // The theme's templates, each drawn as it will look. A tag says what is
                // linked to each: Bible slides use one template; a song's slide uses the
                // one that suits what is on it.
                let theme_index = edit.index;
                let (songs_here, bible_here) =
                    (state.theme_for_songs == theme_index, state.theme_for_bible == theme_index);
                let bible_template = state.bible_layout;
                let mut link_bible = None;
                let layouts: Vec<(usize, String)> =
                    edit.theme.templates.iter().map(|template| template.name.clone()).enumerate().collect();
                let only_one = layouts.len() <= 1;
                let mut renamed_template: Option<Option<(usize, String)>> = None;
                // Asked for from a template's menu, or the tile after the last: a copy of
                // a template, or a template to delete.
                let mut add_after = None;
                let mut ask_delete = None;
                let mut rename_started = std::mem::take(&mut state.rename_started);
                let mut start_rename = None;
                let slides: Vec<Slide> = edit.theme.templates.iter().map(pc_model::ThemeTemplate::as_slide).collect();
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
                        // The template's number and name. A click on the name of the template
                        // on the canvas, or a double click, turns the name into a field.
                        let ink = Color32::from_gray(230);
                        let number = ui.painter().text(
                            pos2(cell.min.x + 9.0, image.max.y + 10.0),
                            Align2::LEFT_CENTER,
                            format!("{}.", index + 1),
                            FontId::proportional(11.5),
                            ink,
                        );
                        let name_area = Rect::from_min_max(
                            pos2(number.max.x + 2.0, image.max.y + 1.0),
                            pos2(cell.max.x - 96.0, cell.max.y - 1.0),
                        );
                        let being_renamed =
                            state.renaming_template.as_ref().is_some_and(|(which, _)| *which == template);
                        if being_renamed && let Some((_, typed)) = state.renaming_template.as_mut() {
                            let field = crate::studio::rename_field_at(
                                ui,
                                typed,
                                FontId::proportional(11.5),
                                ink,
                                ink,
                                pos2(number.max.x + 6.0, image.max.y + 10.0),
                                name_area.max.x,
                            );
                            if std::mem::take(&mut rename_started) {
                                field.request_focus();
                            }
                            let (enter, escape) = ui.input(|input| {
                                (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape))
                            });
                            if escape {
                                renamed_template = Some(None);
                            } else if enter || field.lost_focus() {
                                renamed_template = Some(Some((template, typed.clone())));
                            }
                        } else {
                            ui.painter().with_clip_rect(name_area).text(
                                pos2(number.max.x + 6.0, image.max.y + 10.0),
                                Align2::LEFT_CENTER,
                                name.as_str(),
                                FontId::proportional(11.5),
                                ink,
                            );
                            let on_name = response.interact_pointer_pos().is_some_and(|at| name_area.contains(at));
                            if response.double_clicked() || (response.clicked() && selected && on_name) {
                                start_rename = Some((template, name.clone()));
                            }
                        }
                        // What is linked to this template, at the right of its name.
                        let mut right = cell.max.x - 8.0;
                        for (linked, tag) in
                            [(bible_here && bible_template == template, "Bible"), (songs_here, "Songs")]
                        {
                            if !linked {
                                continue;
                            }
                            let ink = Color32::from_gray(225);
                            let text = ui.painter().layout_no_wrap(tag.to_owned(), FontId::proportional(10.0), ink);
                            let pill = Rect::from_min_size(
                                pos2(right - text.size().x - 10.0, image.max.y + 2.0),
                                vec2(text.size().x + 10.0, 16.0),
                            );
                            ui.painter().rect_filled(pill, 8.0, Color32::from_white_alpha(32));
                            ui.painter().galley(
                                pos2(pill.min.x + 5.0, pill.center().y - text.size().y / 2.0),
                                text,
                                ink,
                            );
                            right = pill.min.x - 4.0;
                        }
                        if response.clicked() && !selected {
                            chosen_template = Some(template);
                        }
                        response.context_menu(|ui| {
                            let linked = bible_here && bible_template == template;
                            if ui.add_enabled(!linked, egui::Button::new("Use for Bible Slides")).clicked() {
                                link_bible = Some(template);
                                ui.close();
                            }
                            ui.separator();
                            if ui.button("Rename").clicked() {
                                start_rename = Some((template, name.clone()));
                                ui.close();
                            }
                            if ui.button("Duplicate").clicked() {
                                add_after = Some(template);
                                ui.close();
                            }
                            if ui.add_enabled(!only_one, egui::Button::new("Delete…")).clicked() {
                                ask_delete = Some(template);
                                ui.close();
                            }
                        });
                    }
                    // After the last template: a blank tile with a plus. It adds a template
                    // that starts as a copy of the one being edited.
                    let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 16.0), Sense::click());
                    let tile = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                    let line = if response.hovered() { crate::studio::ACCENT } else { Color32::from_gray(90) };
                    ui.painter().rect_filled(tile, 4.0, Color32::from_gray(if response.hovered() { 34 } else { 24 }));
                    ui.painter().rect_stroke(tile, 4.0, Stroke::new(1.0, line), StrokeKind::Inside);
                    icons::paint(
                        ui.painter(),
                        Icon::Plus,
                        Rect::from_center_size(tile.center(), vec2(18.0, 18.0)),
                        line,
                    );
                    if response.on_hover_text("Add a template. It starts as a copy of the one being edited.").clicked()
                    {
                        add_after = Some(current);
                    }
                });
                // Delete, with no box selected, deletes the template being edited (once confirmed).
                if delete_template_key && !only_one {
                    ask_delete = Some(current);
                }
                if let Some(template) = ask_delete {
                    state.confirm_delete_template = Some(template);
                }
                if let (Some(after), Some(edit)) = (add_after, state.theme_edit.as_mut()) {
                    let at = edit.theme.add_template(after);
                    edit.changed = true;
                    chosen_template = Some(at);
                    // Its name is ready to be typed over.
                    start_rename = Some((at, edit.theme.templates[at].name.clone()));
                }
                if let Some(template) = link_bible {
                    state.theme_request = Some(ThemeRequest::UseForBible(theme_index, template));
                }
                if let Some(start) = start_rename {
                    state.renaming_template = Some(start);
                    rename_started = true;
                }
                state.rename_started = rename_started;
                if let Some(outcome) = renamed_template {
                    state.renaming_template = None;
                    if let (Some((template, name)), Some(edit)) = (outcome, state.theme_edit.as_mut()) {
                        let name = name.trim();
                        if let Some(renamed) = edit.theme.templates.get_mut(template)
                            && !name.is_empty()
                            && renamed.name != name
                        {
                            renamed.name = name.to_owned();
                            edit.changed = true;
                        }
                    }
                }
            } else {
                egui::ScrollArea::vertical().id_salt("edit slide list").auto_shrink([false, false]).show(ui, |ui| {
                    let width = ui.available_width();
                    let image_size = vec2(width - 16.0, (width - 16.0) * 9.0 / 16.0);
                    let mut index = 0;
                    let groups = presentation.map(|p| p.groups.as_slice()).unwrap_or_default();
                    for (group_index, group) in groups.iter().enumerate() {
                        let (bar, bar_response) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click());
                        ui.painter().rect_filled(bar, 0.0, color32(group.color).gamma_multiply(0.8));
                        let ink = text_on(group.color);
                        icons::paint(
                            ui.painter(),
                            Icon::ChevronDown,
                            Rect::from_center_size(bar.left_center() + vec2(9.0, 0.0), vec2(8.0, 8.0)),
                            ink,
                        );
                        // The group's name; a click turns it into a field to retype it.
                        let renaming = state.renaming_group.as_ref().is_some_and(|(at, _)| *at == group_index);
                        if renaming && let Some((_, typed)) = state.renaming_group.as_mut() {
                            let field = crate::studio::rename_field_at(
                                ui,
                                typed,
                                FontId::proportional(12.5),
                                ink,
                                ink,
                                bar.left_center() + vec2(18.0, 0.0),
                                bar.max.x - 4.0,
                            );
                            if std::mem::take(&mut state.rename_started) {
                                field.request_focus();
                            }
                            let (enter, escape) = ui.input(|input| {
                                (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape))
                            });
                            if escape {
                                state.renaming_group = None;
                            } else if enter || field.lost_focus() {
                                if typed.trim() != group.name {
                                    state.pending_group_name =
                                        Some((at.presentation, group_index, typed.trim().to_owned()));
                                }
                                state.renaming_group = None;
                            }
                        } else {
                            let name = if group.name.is_empty() { "No group" } else { group.name.as_str() };
                            ui.painter().text(
                                bar.left_center() + vec2(18.0, 0.0),
                                Align2::LEFT_CENTER,
                                name,
                                FontId::proportional(12.5),
                                ink,
                            );
                            if bar_response.on_hover_text("Click to rename this group").clicked() {
                                state.renaming_group = Some((group_index, group.name.clone()));
                                state.rename_started = true;
                            }
                        }

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
                    // After the last slide: a blank tile with a plus. It adds a slide laid out
                    // like the one before it.
                    if !groups.is_empty() || presentation.is_some() {
                        let (cell, response) = ui.allocate_exact_size(vec2(width, image_size.y + 22.0), Sense::click());
                        let tile = Rect::from_min_size(cell.min + vec2(8.0, 8.0), image_size);
                        let hovered = response.hovered();
                        let line = if hovered { crate::studio::ACCENT } else { Color32::from_gray(90) };
                        ui.painter().rect_filled(tile, 4.0, Color32::from_gray(if hovered { 34 } else { 24 }));
                        ui.painter().rect_stroke(tile, 4.0, Stroke::new(1.0, line), StrokeKind::Inside);
                        icons::paint(
                            ui.painter(),
                            Icon::Plus,
                            Rect::from_center_size(tile.center(), vec2(18.0, 18.0)),
                            line,
                        );
                        if response.on_hover_text("Add a slide, laid out like the one before it").clicked() {
                            state.add_slide = Some(at.presentation);
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
        // The safe edge: a faint frame a little inside the slide. Boxes snap to it, so text can
        // be kept an even distance from the edge of the screen.
        let (safe_x, safe_y) = crate::editor::SAFE_EDGE;
        let safe = slide.shrink2(vec2(safe_x * scale, safe_y * scale));
        ui.painter().rect_stroke(safe, 0.0, Stroke::new(1.0, Color32::from_white_alpha(38)), StrokeKind::Inside);
        let to_slide = |p: egui::Pos2| ((p.x - slide.min.x) / scale, (p.y - slide.min.y) / scale);
        let to_screen = |f: SlideRect| Rect::from_min_size(slide.min + vec2(f.x, f.y) * scale, vec2(f.w, f.h) * scale);
        let response = ui.interact(area, ui.scope_id().with("canvas"), Sense::click_and_drag());
        let for_theme = state.theme_edit.is_some();
        if let Some(draft) = state.editing.as_mut() {
            // A press anywhere on the canvas ends typing; the field itself is in front of the
            // canvas, so presses inside it never reach here.
            if response.drag_started() || response.clicked() {
                draft.typing = None;
            }
            if let Some(pointer) = response.interact_pointer_pos() {
                let point = to_slide(pointer);
                // Shift or Command adds to the selection; while dragging, Command lets a box
                // move freely, without being pulled onto the guide lines.
                let (extend, free) =
                    ui.input(|input| (input.modifiers.shift || input.modifiers.command, input.modifiers.command));
                if response.drag_started() || response.clicked() {
                    draft.press(theme, point, (HANDLE + 3.0) / scale, extend);
                }
                // A double click on a text box starts typing its words right there. A theme's
                // boxes only ever say "Text", so there is nothing to type in them.
                if response.double_clicked() && !for_theme && draft.selected.is_some() && draft.also.is_empty() {
                    draft.typing = draft.selected;
                    draft.typing_started = true;
                }
                if response.dragged() {
                    let keep_shape = ui.input(|input| input.modifiers.shift);
                    draft.drag_to(point, keep_shape, !free);
                }
            }
            if response.drag_stopped() || response.clicked() {
                draft.release();
            }

            // Outlines: every object faintly, the selected one with handles.
            for object in &draft.slide.objects {
                let frame = object_frame(&draft.slide, object, theme);
                let rect = to_screen(frame);
                let selected = draft.is_selected(object.id);
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
                if selected && draft.also.is_empty() && object.rotation == 0.0 && draft.typing != Some(object.id) {
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

        // The rectangle being dragged out to select with.
        if let Some(((ax, ay), (bx, by))) = state.editing.as_ref().and_then(|draft| draft.marquee) {
            let rect = Rect::from_two_pos(slide.min + vec2(ax, ay) * scale, slide.min + vec2(bx, by) * scale);
            ui.painter().rect_filled(rect, 0.0, crate::studio::ACCENT.gamma_multiply(0.15));
            ui.painter().rect_stroke(rect, 0.0, Stroke::new(1.0, crate::studio::ACCENT), StrokeKind::Inside);
        }

        // The lines a dragged box is sitting on: the slide's middle or edges, or another box's.
        if let Some(draft) = state.editing.as_ref() {
            let pen = Stroke::new(1.0, Color32::from_rgb(255, 64, 196));
            for guide in &draft.guides {
                match guide {
                    crate::editor::Guide::Down(x) => {
                        let x = slide.min.x + x * scale;
                        ui.painter().line_segment([pos2(x, slide.min.y), pos2(x, slide.max.y)], pen);
                    }
                    crate::editor::Guide::Across(y) => {
                        let y = slide.min.y + y * scale;
                        ui.painter().line_segment([pos2(slide.min.x, y), pos2(slide.max.x, y)], pen);
                    }
                }
            }
        }

        // Typing on the slide: a field over the box being edited, in the box's own place.
        if let Some(draft) = state.editing.as_mut()
            && let Some(id) = draft.typing
        {
            let started = std::mem::take(&mut draft.typing_started);
            // Only a text object can be typed in.
            let placed = draft.slide.objects.iter().find(|o| o.id == id).and_then(|object| {
                let look = theme.text_style(object.role.as_ref(), object.text_style()?);
                Some((to_screen(object_frame(&draft.slide, object, theme)), look))
            });
            let object = draft.slide.objects.iter_mut().find(|o| o.id == id);
            match (placed, object) {
                (Some((rect, look)), Some(pc_model::Object { kind: pc_model::ObjectKind::Text { text, .. }, .. })) => {
                    // The box is covered while it is typed in, so only the words being typed
                    // are read; the slide shows them in its own look as soon as typing ends.
                    ui.painter().rect_filled(rect, 4.0, Color32::from_rgb(8, 9, 11));
                    ui.painter().rect_stroke(rect, 4.0, Stroke::new(1.5, crate::studio::ACCENT), StrokeKind::Outside);
                    let size = look.map_or(20.0, |look| (look.size * scale).clamp(13.0, 34.0));
                    let field = egui::TextEdit::multiline(text)
                        .frame(Frame::NONE)
                        .font(FontId::proportional(size))
                        .align(Align2::CENTER_TOP)
                        .desired_width(rect.width() - 16.0);
                    let field = ui.put(rect.shrink(8.0), field);
                    if started {
                        field.request_focus();
                    }
                    if ui.input(|input| input.key_pressed(egui::Key::Escape)) {
                        draft.typing = None;
                    }
                }
                // The object went away while it was being typed in.
                _ => draft.typing = None,
            }
        }

        // The object tools: a text box, and (on a song's slide) the shapes.
        let shapes_too = !theme_mode;
        let offset = if shapes_too { -52.0 } else { 0.0 };
        if shapes_too {
            let shapes_bar =
                Rect::from_center_size(pos2(slide.center().x + 52.0, slide.min.y - 22.0), vec2(96.0, 28.0));
            let shapes = ui.interact(shapes_bar, ui.scope_id().with("add shape"), Sense::click());
            ui.painter().rect_filled(
                shapes_bar,
                8.0,
                if shapes.hovered() { crate::studio::HOVER } else { crate::studio::CARD },
            );
            ui.painter().text(
                shapes_bar.center(),
                Align2::CENTER_CENTER,
                "Shapes ▾",
                crate::studio::semibold(12.5),
                crate::studio::TEXT,
            );
            let shapes = shapes.on_hover_text("Add a shape. Each has a key: O R E G L H P S D A");
            egui::Popup::menu(&shapes).show(|ui| {
                for (shape, key) in pc_model::Shape::ALL.into_iter().zip(SHAPE_KEYS) {
                    if ui.add(egui::Button::new(shape.name()).shortcut_text(key.to_string())).clicked()
                        && let Some(draft) = state.editing.as_mut()
                    {
                        draft.add_shape(shape);
                    }
                }
            });
        }
        let bar = Rect::from_center_size(pos2(slide.center().x + offset, slide.min.y - 22.0), vec2(96.0, 28.0));
        let add = ui.interact(bar, ui.scope_id().with("add text"), Sense::click());
        ui.painter().rect_filled(bar, 8.0, if add.hovered() { crate::studio::HOVER } else { crate::studio::CARD });
        ui.painter().text(
            bar.center(),
            Align2::CENTER_CENTER,
            "Add Text",
            crate::studio::semibold(12.5),
            crate::studio::TEXT,
        );
        if add.on_hover_text("Add a text box to the slide").clicked()
            && let Some(draft) = state.editing.as_mut()
        {
            draft.add_text();
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
                // A box added in the editor is given a role of its own once, so renaming it
                // later does not turn it into a different box.
                let mut taken: Vec<pc_model::Role> =
                    draft.slide.objects.iter().filter_map(|o| o.role.clone()).collect();
                if let Some(template) = edit.theme.templates.get_mut(edit.template) {
                    for object in draft.slide.objects.iter_mut().filter(|o| o.role.is_none()) {
                        let role = template.unused_role(&object.name, &taken);
                        taken.push(role.clone());
                        object.role = Some(role);
                    }
                    *template = template.with_slide(&draft.slide);
                    edit.changed = true;
                }
            }
            // A prop: the prop takes the slide as it now is.
            None if draft.at.presentation == crate::props::PROP_SLOT => {
                if let Some(prop) = state.props.get_mut(draft.at.slide) {
                    prop.slide = draft.slide.clone();
                    state.props_changed = true;
                }
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
        } else if let Some(template) = chosen_template
            && let Some(opened) = edit.theme.templates.get(template)
        {
            edit.template = template;
            state.editing = Some(EditDraft::of_slide(opened.as_slide(), template));
        }
    } else if leave {
        state.editing = None;
    } else if let Some(here) = chosen {
        state.editing = EditDraft::of(show, here);
    }
}

/// The panes of the Settings window, in order.
/// What an output is called in the Screens window's menus.
fn output_label(output: &Output, displays: &[DisplayInfo]) -> String {
    match output {
        Output::Automatic => "Automatic".to_owned(),
        Output::Display(name) => match displays.iter().find(|display| display.name == *name) {
            Some(display) => format!("{}   {} x {}", display.name, display.w, display.h),
            None => format!("{name}   (disconnected)"),
        },
        Output::Window => "A window".to_owned(),
        Output::Ndi(_) => "NDI".to_owned(),
        Output::Off => "Off".to_owned(),
    }
}

/// The outputs a screen can be given: automatic, each connected display, a window, NDI, off.
fn output_choices(current: &Output, screen_name: &str, displays: &[DisplayInfo]) -> Vec<Output> {
    let mut choices = vec![Output::Automatic];
    choices.extend(displays.iter().map(|display| Output::Display(display.name.clone())));
    choices.push(Output::Window);
    // Choosing NDI again keeps the name already given.
    choices.push(match current {
        Output::Ndi(name) => Output::Ndi(name.clone()),
        _ => Output::Ndi(format!("PresenterCraft {screen_name}")),
    });
    choices.push(Output::Off);
    choices
}

/// A change the menu bar asks the editor to make to the selected objects. The editor makes it
/// during its own frame, so it can be undone and is saved like any other edit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditRequest {
    Bold,
    Italic,
    Underline,
    Bigger,
    Smaller,
    Align(pc_model::Align),
    UpperCase,
    LowerCase,
    Capitalize,
    CopyStyle,
    PasteStyle,
    BringForward,
    BringToFront,
    SendBackward,
    SendToBack,
    Lock,
    UnlockAll,
    DeselectAll,
}

/// Each word with a capital first letter, the rest as they are.
fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if start && c.is_alphabetic() {
            out.extend(c.to_uppercase());
            start = false;
        } else {
            out.push(c);
            if c.is_whitespace() {
                start = true;
            } else if c.is_alphabetic() {
                start = false;
            }
        }
    }
    out
}

/// A change to one slide asked for from its right-click menu. The app makes it.
#[derive(Clone, Debug, PartialEq)]
pub enum SlideRequest {
    Delete,
    PasteAfter(Slide),
    Group(String),
    Label(Option<String>),
}

/// A change to the media bin asked for: pictures to choose and add, or one to remove.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MediaRequest {
    Choose,
    Remove(usize),
}

/// A change to a song asked for from its right-click menu in the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SongRequest {
    Duplicate,
    Delete,
}

/// The group names a slide's Group menu offers, as ProPresenter's does.
const GROUP_NAMES: [&str; 19] = [
    "Verse 1",
    "Verse 2",
    "Verse 3",
    "Verse 4",
    "Verse 5",
    "Verse 6",
    "Chorus",
    "Chorus 2",
    "Chorus 3",
    "Chorus 4",
    "Bridge",
    "Bridge 2",
    "Bridge 3",
    "Pre-chorus",
    "Tag",
    "Intro",
    "Ending",
    "Outro",
    "Interlude",
];

/// The labels a slide's Label menu offers.
const SLIDE_LABELS: [&str; 4] = ["Blank Slide", "Instrumental", "Repeat", "Spoken Word"];

/// What an object is called in the Objects list: the name it was given, or, until it is given
/// one, the first line of its words, as ProPresenter shows them.
fn listed_name(object: &pc_model::Object) -> String {
    let named = !(object.name.is_empty()
        || object.role.as_ref().is_some_and(|role| object.name == role.label())
        || object.name.strip_prefix("Text ").is_some_and(|number| number.parse::<u32>().is_ok()));
    let first_line = object.as_text().and_then(|text| text.lines().map(str::trim).find(|line| !line.is_empty()));
    match first_line {
        Some(line) if !named => line.to_owned(),
        _ => object.name.clone(),
    }
}

/// The Reflow editor: a song's words as text, to type into. `[Group]` starts a group, a blank
/// line starts a slide, and `--` comes before a translation. Done rebuilds the slides.
fn reflow_editor(ui: &mut egui::Ui, title: &str, state: &mut HomeState) {
    let Some((index, text)) = state.reflow.as_mut() else { return };
    let index = *index;
    let mut done = None;
    let field_id = ui.scope_id().with("reflow text");
    let heading = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 12, bottom: 8 });
    egui::Panel::top("reflow header").resizable(false).frame(heading).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(format!("Reflow  ·  {title}"))
                    .font(crate::studio::semibold(18.0))
                    .color(crate::studio::TEXT),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Done").on_hover_text("Rebuild the slides from these words").clicked() {
                    done = Some(true);
                }
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
                ui.add_space(12.0);
                // Insert at the cursor: a slide break, or a group's heading.
                let cursor = egui::text_edit::TextEditState::load(ui.ctx(), field_id)
                    .and_then(|edit| edit.cursor.char_range())
                    .map_or(text.chars().count(), |range| range.primary.index.into());
                let mut insert = |what: &str| {
                    let at = text.char_indices().nth(cursor).map_or(text.len(), |(byte, _)| byte);
                    text.insert_str(at, what);
                };
                ui.menu_button("Insert Group", |ui| {
                    for name in GROUP_NAMES {
                        if ui.button(name).clicked() {
                            insert(&format!("\n[{name}]\n"));
                            ui.close();
                        }
                    }
                });
                if ui.button("Insert Slide Break").on_hover_text("Start a new slide here").clicked() {
                    insert("\n\n");
                }
            });
        });
        ui.label(
            egui::RichText::new(
                "[Group] starts a group  ·  a blank line starts a new slide  ·  -- comes before the translation",
            )
            .size(11.5)
            .color(crate::studio::TEXT_FAINT),
        );
    });
    egui::ScrollArea::vertical().id_salt("reflow").auto_shrink([false, false]).show(ui, |ui| {
        Frame::new().inner_margin(egui::Margin::symmetric(20, 8)).show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(text)
                    .id(field_id)
                    .font(FontId::proportional(16.0))
                    .desired_width(f32::INFINITY)
                    .desired_rows(24)
                    .lock_focus(true),
            );
        });
    });
    match done {
        Some(true) => {
            state.reflow_done = state.reflow.take().map(|(_, text)| (index, text));
        }
        Some(false) => state.reflow = None,
        None => {}
    }
}

/// The songs a library search finds, best first: titles that hold the words, then songs whose
/// lyrics do, with the line that matched. Songs not listed in the library are left out.
fn search_songs(show: &Show, listed: impl Fn(usize) -> bool, query: &str) -> Vec<(usize, Option<String>)> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut by_title = Vec::new();
    let mut by_words = Vec::new();
    for (index, song) in show.presentations().iter().enumerate().filter(|(index, _)| listed(*index)) {
        if song.title.to_lowercase().contains(&query) {
            by_title.push((index, None));
            continue;
        }
        let line = song
            .slides()
            .flat_map(|(_, slide)| slide.texts().into_iter().map(|(_, text)| text))
            .flat_map(|text| text.lines().map(str::to_owned).collect::<Vec<_>>())
            .find(|line| line.to_lowercase().contains(&query));
        if let Some(line) = line {
            by_words.push((index, Some(line)));
        }
    }
    by_title.extend(by_words);
    by_title
}

/// The library search pop-up: a field, the songs it finds, and what Return does.
fn library_search(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    let Some((query, picked)) = state.library_search.as_mut() else { return };
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    let removed = state.removed_songs.clone();
    let found = search_songs(show, |index| !hidden.contains(&index) && !removed.contains(&index), query);
    *picked = (*picked).min(found.len().saturating_sub(1));
    let (up, down, enter, command, escape) = ui.input(|input| {
        (
            input.key_pressed(egui::Key::ArrowUp),
            input.key_pressed(egui::Key::ArrowDown),
            input.key_pressed(egui::Key::Enter),
            input.modifiers.command,
            input.key_pressed(egui::Key::Escape),
        )
    });
    if up {
        *picked = picked.saturating_sub(1);
    }
    if down && *picked + 1 < found.len() {
        *picked += 1;
    }
    let mut chosen: Option<(usize, bool)> = None;
    let screen = ui.ctx().content_rect();
    egui::Area::new(egui::Id::unique("library search"))
        .order(egui::Order::Foreground)
        .fade_in(false)
        .fixed_pos(pos2(screen.center().x - 260.0, screen.min.y + 90.0))
        .show(ui.ctx(), |ui| {
            Frame::popup(ui.style()).inner_margin(12).show(ui, |ui| {
                ui.set_width(520.0);
                let field = ui.add(
                    egui::TextEdit::singleline(query)
                        .hint_text("Search the library: titles and words")
                        .desired_width(f32::INFINITY)
                        .font(FontId::proportional(16.0)),
                );
                field.request_focus();
                ui.add_space(6.0);
                if found.is_empty() && !query.trim().is_empty() {
                    ui.label(egui::RichText::new("No song matches.").color(DIM));
                }
                for (row, (index, line)) in found.iter().take(12).enumerate() {
                    let Some(song) = show.presentations().get(*index) else { continue };
                    let height = if line.is_some() { 40.0 } else { 26.0 };
                    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
                    if row == *picked || response.hovered() {
                        ui.painter().rect_filled(
                            rect,
                            6.0,
                            crate::studio::ACCENT.gamma_multiply(if row == *picked { 0.4 } else { 0.2 }),
                        );
                    }
                    ui.painter().text(
                        pos2(rect.min.x + 10.0, rect.min.y + 13.0),
                        Align2::LEFT_CENTER,
                        song.title.as_str(),
                        FontId::proportional(14.0),
                        crate::studio::TEXT,
                    );
                    ui.painter().text(
                        pos2(rect.max.x - 10.0, rect.min.y + 13.0),
                        Align2::RIGHT_CENTER,
                        "Library",
                        FontId::proportional(11.5),
                        crate::studio::TEXT_FAINT,
                    );
                    if let Some(line) = line {
                        ui.painter().with_clip_rect(rect).text(
                            pos2(rect.min.x + 10.0, rect.min.y + 30.0),
                            Align2::LEFT_CENTER,
                            line.as_str(),
                            FontId::proportional(12.0),
                            DIM,
                        );
                    }
                    if response.clicked() {
                        chosen = Some((*index, false));
                    }
                }
                ui.add_space(6.0);
                ui.label(egui::RichText::new("↵ Open    ⌘↵ Add to Service    esc Close").size(11.0).color(DIM));
            });
        });
    if enter && let Some((index, _)) = found.get(*picked) {
        chosen = Some((*index, command));
    }
    if let Some((index, add)) = chosen {
        if add {
            if let (Some(key), Some(song)) = (state.song_keys.get(index), show.presentations().get(index)) {
                state.services.add(crate::service::Item::Song { key: key.clone(), title: song.title.clone() });
            }
        } else {
            commands.push(Command::Select(index));
            state.editing = None;
            state.bible_open = false;
            state.reflow = None;
        }
        state.library_search = None;
    } else if escape {
        state.library_search = None;
    }
}

/// The key that adds each shape in the editor, in the order of `Shape::ALL`.
const SHAPE_KEYS: [char; 10] = ['O', 'R', 'E', 'G', 'L', 'H', 'P', 'S', 'D', 'A'];

/// Room after a song's title, for the field it turns into when renamed to fit in.
const TITLE_ROOM: f32 = 12.0;

/// The Settings window's contents: its panes down the left, the chosen one on the right.
pub fn settings_ui(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{ACCENT, HOVER, PANEL, TEXT, TEXT_DIM};
    const PANES: [&str; 4] = ["General", "Screens", "Groups", "Key Mappings"];
    egui::Panel::left("settings panes")
        .resizable(false)
        .exact_size(180.0)
        .frame(Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 10, right: 10, top: 14, bottom: 10 }))
        .show(ui, |ui| {
            for (index, name) in PANES.into_iter().enumerate() {
                let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                if state.preferences_pane == index {
                    ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
                } else if response.hovered() {
                    ui.painter().rect_filled(row, 6.0, HOVER);
                }
                ui.painter().text(
                    row.left_center() + vec2(10.0, 0.0),
                    Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(13.5),
                    TEXT,
                );
                if response.clicked() {
                    state.preferences_pane = index;
                }
            }
        });
    egui::CentralPanel::default().frame(Frame::new().fill(crate::studio::BG).inner_margin(20)).show(ui, |ui| {
        let pane = PANES.get(state.preferences_pane).copied().unwrap_or("General");
        ui.label(egui::RichText::new(pane).font(crate::studio::semibold(18.0)).color(TEXT));
        ui.add_space(12.0);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match pane {
            "General" => {
                crate::studio::caption(ui, "COPYRIGHT");
                ui.label("Show a song's copyright line on the audience screens:");
                for choice in crate::service::CopyrightShow::ALL {
                    ui.radio_value(&mut state.copyright_show, choice, choice.name());
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("CCLI License Number");
                    ui.add(egui::TextEdit::singleline(&mut state.ccli_licence).desired_width(160.0));
                });
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "A song's writer, publisher and CCLI song number are set with Info beside its title.",
                    )
                    .size(11.5)
                    .color(TEXT_DIM),
                );
            }
            "Screens" => {
                if ui.button("Configure Screens…").on_hover_text("Option-Command-1").clicked() {
                    state.settings_open = true;
                }
                ui.add_space(10.0);
                ui.checkbox(&mut state.start_audience_off, "Start with the audience screens off")
                    .on_hover_text("When PresenterCraft opens, nothing is shown until Audience is switched on");
                ui.checkbox(&mut state.stage_on, "Stage screens on");
            }
            "Groups" => {
                ui.label(egui::RichText::new("Each group's colour, and the key that jumps to it.").color(TEXT_DIM));
                ui.add_space(8.0);
                egui::Grid::new("groups").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
                    for (key, names) in pc_model::GROUP_KEYS {
                        let name =
                            names.iter().find(|name| name.contains(' ')).or(names.first()).copied().unwrap_or("");
                        let mut shown = name.to_owned();
                        if let Some(first) = shown.get_mut(0..1) {
                            first.make_ascii_uppercase();
                        }
                        let (swatch, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
                        ui.painter().rect_filled(swatch, 4.0, color32(pc_model::group_color(name)));
                        ui.label(shown);
                        ui.label(egui::RichText::new(key.to_ascii_uppercase().to_string()).monospace().color(TEXT_DIM));
                        ui.end_row();
                    }
                });
            }
            _ => {
                ui.label(egui::RichText::new("The keys that run the show, and the menu shortcuts.").color(TEXT_DIM));
                ui.add_space(8.0);
                egui::Grid::new("keys").num_columns(3).spacing([16.0, 6.0]).striped(true).show(ui, |ui| {
                    let show_keys = [
                        ("Show", "Next slide", "→ or Space"),
                        ("Show", "Previous slide", "←"),
                        ("Show", "First / last slide", "Home / End"),
                        ("Show", "Clear All", "Esc or F1"),
                        ("Show", "Clear Slide", "F2"),
                        ("Show", "Clear Media", "F3"),
                        ("Show", "Clear Props", "F4"),
                        ("Show", "Clear Messages", "F6"),
                        ("Show", "Jump to a group", "its letter (see Groups)"),
                    ];
                    for (area, command, key) in show_keys {
                        ui.label(egui::RichText::new(area).color(TEXT_DIM));
                        ui.label(command);
                        ui.label(egui::RichText::new(key).monospace());
                        ui.end_row();
                    }
                    for (menu, command, key) in crate::menubar::shortcuts() {
                        ui.label(egui::RichText::new(menu).color(TEXT_DIM));
                        ui.label(command.trim_end_matches('…'));
                        ui.label(egui::RichText::new(key).monospace());
                        ui.end_row();
                    }
                });
            }
        });
    });
}

/// A switch drawn the way the Mac draws one: a capsule, green with the knob at the right when
/// on. Returns true when it was clicked.
fn switch(ui: &mut egui::Ui, on: bool, hint: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(34.0, 20.0), Sense::click());
    let how_far = ui.ctx().animate_bool(response.id, on);
    let track = Color32::from_gray(70).lerp_to_gamma(ON, how_far);
    ui.painter().rect_filled(rect, 10.0, track);
    let knob = pos2(egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, how_far), rect.center().y);
    ui.painter().circle_filled(knob, 8.0, Color32::WHITE);
    response.on_hover_text(hint).clicked()
}

/// The Screens window's contents: the audience screens on the left, and where the chosen one's
/// picture goes on the right. Every screen shows the same picture. Changes take effect, and are
/// saved, as they are made. `output_on` is whether the audience output is on; switching it is
/// returned as a command.
pub fn screens_ui(ui: &mut egui::Ui, state: &mut HomeState, output_on: bool) -> Vec<Command> {
    use crate::studio::{ACCENT, CARD, HOVER, LINE, PANEL, TEXT, TEXT_DIM, TEXT_FAINT};
    let mut commands = Vec::new();
    let mut changed = false;
    state.screen_selected = state.screen_selected.min(state.screens.len().saturating_sub(1));

    // The list of screens, with + and − under it, as Mac lists have.
    egui::Panel::left("screen list")
        .resizable(false)
        .exact_size(214.0)
        .frame(Frame::new().fill(PANEL).inner_margin(egui::Margin { left: 12, right: 12, top: 14, bottom: 12 }))
        .show(ui, |ui| {
            egui::Panel::bottom("screen list buttons").resizable(false).frame(Frame::new()).show(ui, |ui| {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let small = vec2(26.0, 22.0);
                    let only = state.screens.iter().filter(|screen| screen.kind == Kind::Audience).count() <= 1
                        && state.screens.get(state.screen_selected).is_some_and(|screen| screen.kind == Kind::Audience);
                    let remove = ui
                        .add_enabled_ui(!only, |ui| ui.add_sized(small, egui::Button::new("−")))
                        .inner
                        .on_hover_text("Remove the selected screen")
                        .on_disabled_hover_text(
                            "There is always one audience screen. Set its output to Off to show nothing.",
                        );
                    if remove.clicked() && !only && state.screen_selected < state.screens.len() {
                        state.screens.remove(state.screen_selected);
                        state.screen_selected = state.screen_selected.saturating_sub(1);
                        state.ndi_draft = None;
                        changed = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let identify = ui
                            .add(egui::Button::selectable(state.identify, "Identify"))
                            .on_hover_text("Show each screen's name on it, in place of the show, to tell them apart");
                        if identify.clicked() {
                            state.identify = !state.identify;
                        }
                    });
                });
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for kind in [Kind::Audience, Kind::Stage] {
                    // The section's heading, with + to add a screen of its kind and a switch for all of them.
                    ui.horizontal(|ui| {
                        crate::studio::caption(ui, if kind == Kind::Audience { "AUDIENCE" } else { "STAGE" });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (on, what) = match kind {
                                Kind::Audience => (output_on, "audience"),
                                Kind::Stage => (state.stage_on, "stage"),
                            };
                            let hint = format!("The {what} screens are {}", if on { "on" } else { "off" });
                            if switch(ui, on, &hint) {
                                match kind {
                                    Kind::Audience => commands.push(Command::SetOutputEnabled(!output_on)),
                                    Kind::Stage => state.stage_on = !state.stage_on,
                                }
                            }
                            let add = ui
                                .add_sized(vec2(22.0, 20.0), egui::Button::new("+").frame(false))
                                .on_hover_text(format!("Add a {what} screen"));
                            if add.clicked() {
                                let name = crate::screens::next_name(&state.screens, kind);
                                // A new screen goes nowhere until it is told where, so nothing
                                // changes on any display by surprise.
                                let screen = match kind {
                                    Kind::Audience => Screen::new(name, Output::Off),
                                    Kind::Stage => Screen::stage(name, Output::Off),
                                };
                                state.screens.push(screen);
                                state.screen_selected = state.screens.len() - 1;
                                state.ndi_draft = None;
                                changed = true;
                            }
                        });
                    });
                    ui.add_space(4.0);
                    let mut any = false;
                    for index in 0..state.screens.len() {
                        if state.screens[index].kind != kind {
                            continue;
                        }
                        any = true;
                        let shown = state.screen_status.get(index).is_some_and(|(_, shown)| *shown);
                        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                        let selected = state.screen_selected == index;
                        if selected {
                            ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
                        } else if response.hovered() {
                            ui.painter().rect_filled(row, 6.0, HOVER);
                        }
                        // A green light when the screen is being shown somewhere.
                        let light = if shown { ON } else { TEXT_FAINT };
                        ui.painter().circle_filled(row.left_center() + vec2(12.0, 0.0), 4.0, light);
                        ui.painter().with_clip_rect(row.shrink(2.0)).text(
                            row.left_center() + vec2(24.0, 0.0),
                            Align2::LEFT_CENTER,
                            state.screens[index].name.as_str(),
                            FontId::proportional(13.5),
                            TEXT,
                        );
                        if response.clicked() {
                            state.screen_selected = index;
                            state.ndi_draft = None;
                        }
                    }
                    if !any {
                        ui.label(egui::RichText::new("None. Press + to add one.").size(12.0).color(TEXT_FAINT));
                    }
                    ui.add_space(14.0);
                }
            });
        });

    // The chosen screen.
    egui::CentralPanel::default().frame(Frame::new().fill(crate::studio::BG).inner_margin(20)).show(ui, |ui| {
        let index = state.screen_selected;
        let Some(screen) = state.screens.get(index).cloned() else { return };
        let (status, shown) = state.screen_status.get(index).cloned().unwrap_or_default();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(screen.name.as_str()).font(crate::studio::semibold(18.0)).color(TEXT));
            let what = if screen.kind == Kind::Stage { "Stage  ·  1920 × 1080" } else { "1920 × 1080" };
            ui.label(egui::RichText::new(what).size(13.0).color(TEXT_DIM));
        });
        ui.add_space(12.0);

        // Where the picture goes, as a little screen.
        let width = ui.available_width();
        let (picture, _) = ui.allocate_exact_size(vec2(width, (width * 9.0 / 16.0).min(190.0)), Sense::hover());
        let frame_rect = Rect::from_center_size(picture.center(), vec2(picture.height() * 16.0 / 9.0, picture.height()));
        ui.painter().rect_filled(frame_rect, 8.0, Color32::BLACK);
        ui.painter().rect_stroke(frame_rect, 8.0, Stroke::new(1.0, LINE), StrokeKind::Inside);
        let target = match &screen.output {
            Output::Automatic => "Automatic".to_owned(),
            Output::Display(name) => name.clone(),
            Output::Window => "Window".to_owned(),
            Output::Ndi(name) => format!("NDI  ·  {name}"),
            Output::Off => "Off".to_owned(),
        };
        ui.painter().text(
            frame_rect.center() - vec2(0.0, 10.0),
            Align2::CENTER_CENTER,
            target,
            crate::studio::semibold(16.0),
            if shown { TEXT } else { TEXT_DIM },
        );
        ui.painter().text(
            frame_rect.center() + vec2(0.0, 14.0),
            Align2::CENTER_CENTER,
            status.as_str(),
            FontId::proportional(12.0),
            if shown { ON } else { TEXT_DIM },
        );
        ui.add_space(16.0);

        // The settings, as rows in a rounded box, labels on the left.
        Frame::new().fill(CARD).corner_radius(10).inner_margin(egui::Margin::symmetric(14, 10)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            egui::Grid::new("screen settings").num_columns(2).spacing([16.0, 12.0]).min_col_width(84.0).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                let mut name = screen.name.clone();
                if ui.add(egui::TextEdit::singleline(&mut name).desired_width(f32::INFINITY)).changed() {
                    state.screens[index].name = name;
                    changed = true;
                }
                ui.end_row();

                ui.label(egui::RichText::new("Output").color(TEXT_DIM));
                egui::ComboBox::from_id_salt("screen output")
                    .selected_text(output_label(&screen.output, &state.displays))
                    .width(ui.available_width())
                    .show_ui(ui, |ui| {
                        for choice in output_choices(&screen.output, &screen.name, &state.displays) {
                            let is_ndi = matches!(choice, Output::Ndi(_));
                            let label = output_label(&choice, &state.displays);
                            let on = std::mem::discriminant(&choice) == std::mem::discriminant(&screen.output)
                                && (is_ndi || choice == screen.output);
                            let entry = ui
                                .add_enabled(!is_ndi || state.ndi_available, egui::Button::selectable(on, label))
                                .on_disabled_hover_text("This build of PresenterCraft does not include NDI.");
                            if entry.clicked() && choice != screen.output {
                                state.screens[index].output = choice;
                                state.ndi_draft = None;
                                changed = true;
                            }
                        }
                    });
                ui.end_row();

                if let Output::Ndi(current) = &screen.output {
                    ui.label(egui::RichText::new("NDI name").color(TEXT_DIM));
                    let mut typed = match &state.ndi_draft {
                        Some((id, typed)) if *id == screen.id => typed.clone(),
                        _ => current.clone(),
                    };
                    let field = ui.add(egui::TextEdit::singleline(&mut typed).desired_width(f32::INFINITY));
                    if field.changed() {
                        state.ndi_draft = Some((screen.id, typed.clone()));
                    }
                    // The name is used once the field is left, or Return is pressed, so NDI is
                    // not restarted at every letter.
                    if field.lost_focus() && state.ndi_draft.take().is_some() && typed != *current {
                        state.screens[index].output = Output::Ndi(typed);
                        changed = true;
                    }
                    ui.end_row();
                }
            });
        });
        ui.add_space(10.0);
        let about = match &screen.output {
            Output::Automatic => "Automatic fills the first display the main window is not on. With one display it uses a window.",
            Output::Display(_) => {
                "If this display is unplugged the screen shows nowhere until it is back; it never moves to another display."
            }
            Output::Window => "An ordinary window, for trying things out or for capturing in other software.",
            Output::Ndi(_) => {
                "Other computers on the network can show this screen, at 1920 × 1080. NDI® is a registered trademark of Vizrt NDI AB."
            }
            Output::Off => "This screen is kept in the list and shows nowhere.",
        };
        ui.label(egui::RichText::new(about).size(11.5).color(TEXT_DIM));
        if screen.kind == Kind::Stage {
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new("A stage screen shows the words on screen now, the next slide's words under them, and the time.")
                    .size(11.5)
                    .color(TEXT_DIM),
            );
        }
    });
    state.screens_changed |= changed;
    commands
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

/// Asks before a theme is deleted, since it cannot be brought back.
/// Asks before taking a song out of the library.
fn confirm_delete_song(ui: &mut egui::Ui, show: &Show, state: &mut HomeState) {
    let Some(index) = state.confirm_delete_song else { return };
    let Some(song) = show.presentations().get(index) else {
        state.confirm_delete_song = None;
        return;
    };
    let question = format!("Remove \u{201c}{}\u{201d} from the library?", song.title);
    let note = "A song made in PresenterCraft is moved to the Trash; a song opened from elsewhere stays where it is.";
    if let Some(delete) = confirm_delete(ui, &question, note) {
        state.confirm_delete_song = None;
        if delete {
            state.song_request = Some((index, SongRequest::Delete));
        }
    }
}

fn confirm_delete_theme(ui: &mut egui::Ui, state: &mut HomeState) {
    confirm_delete_template(ui, state);
    let Some(index) = state.confirm_delete_theme else { return };
    let Some(name) = state.theme_names.get(index).cloned() else {
        state.confirm_delete_theme = None;
        return;
    };
    let in_use = match (state.theme_for_songs == index, state.theme_for_bible == index) {
        (true, true) => "Songs and Bible slides use it; they will use the built-in theme instead.",
        (true, false) => "Songs use it; they will use the built-in theme instead.",
        (false, true) => "Bible slides use it; they will use the built-in theme instead.",
        (false, false) => "",
    };
    if let Some(delete) = confirm_delete(ui, &format!("Delete the theme \"{name}\"?"), in_use) {
        state.confirm_delete_theme = None;
        if delete {
            state.theme_request = Some(ThemeRequest::Delete(index));
        }
    }
}

/// Asks before deleting one of the templates of the theme being edited, and deletes it.
fn confirm_delete_template(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(template) = state.confirm_delete_template else { return };
    let bible_uses_it = state.theme_edit.as_ref().is_some_and(|edit| edit.index == state.theme_for_bible)
        && state.bible_layout == template;
    let name = state.theme_edit.as_ref().and_then(|edit| edit.theme.templates.get(template)).map(|t| t.name.clone());
    let Some(name) = name else {
        state.confirm_delete_template = None;
        return;
    };
    let note =
        if bible_uses_it { "Bible slides use it; they will use the theme's first template instead." } else { "" };
    let Some(delete) = confirm_delete(ui, &format!("Delete the template \"{name}\"?"), note) else { return };
    state.confirm_delete_template = None;
    if delete
        && let Some(edit) = state.theme_edit.as_mut()
        && edit.theme.remove_template(template)
    {
        edit.changed = true;
        // The template being edited stays open. If it was the one deleted, the one before
        // it (or the new first one) is opened in its place.
        let was = edit.template;
        edit.template = if was >= template { was.saturating_sub(1) } else { was };
        if was == template {
            state.editing = Some(EditDraft::of_slide(edit.theme.templates[edit.template].as_slide(), edit.template));
        } else if let Some(draft) = state.editing.as_mut() {
            draft.at.slide = edit.template;
        }
    }
}

/// A small window in the middle of the app asking whether to delete something. Return
/// deletes, as the Delete button does; Escape cancels. `None` until it is answered.
fn confirm_delete(ui: &mut egui::Ui, question: &str, note: &str) -> Option<bool> {
    let mut decided = None;
    egui::Window::new(question)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fade_in(false)
        .anchor(Align2::CENTER_CENTER, [0.0, -40.0])
        .show(ui.ctx(), |ui| {
            ui.set_width(340.0);
            ui.add_space(4.0);
            ui.label(egui::RichText::new(question).font(crate::studio::semibold(15.0)));
            ui.add_space(4.0);
            ui.label(egui::RichText::new("It cannot be brought back.").color(DIM));
            if !note.is_empty() {
                ui.label(egui::RichText::new(note).color(DIM));
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let size = vec2(96.0, 30.0);
                    if crate::studio::block_button(ui, size, "Delete", "", crate::studio::Emphasis::Live).clicked() {
                        decided = Some(true);
                    }
                    if crate::studio::block_button(ui, size, "Cancel", "", crate::studio::Emphasis::Plain).clicked() {
                        decided = Some(false);
                    }
                });
            });
            let (enter, escape) =
                ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
            if escape {
                decided = Some(false);
            } else if enter {
                decided = Some(true);
            }
        });
    decided
}

/// The top of the Theme Editor's left column: every theme in a plain list. A click opens a
/// theme; a click on the open theme's name renames it; a tag shows which kinds of slide use
/// each one, and a right-click changes that. The plus makes a new theme.
fn theme_list(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::studio::{ACCENT, CARD, TEXT, TEXT_DIM, TEXT_FAINT};
    let editing = state.theme_edit.as_ref().map_or(0, |edit| edit.index);
    Frame::new().inner_margin(egui::Margin { left: 10, right: 10, top: 10, bottom: 6 }).show(ui, |ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 2.0);
        ui.horizontal(|ui| {
            crate::studio::caption(ui, "THEMES");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (plus, response) = ui.allocate_exact_size(vec2(20.0, 18.0), Sense::click());
                let ink = if response.hovered() { TEXT } else { TEXT_FAINT };
                icons::paint(ui.painter(), Icon::Plus, Rect::from_center_size(plus.center(), vec2(10.0, 10.0)), ink);
                if response.on_hover_text("New theme: a copy of the one songs use").clicked() {
                    state.theme_request = Some(ThemeRequest::New);
                }
            });
        });
        ui.add_space(4.0);
        let names = state.theme_names.clone();
        let mut start_rename = false;
        let mut renamed: Option<Option<String>> = None;
        egui::ScrollArea::vertical().id_salt("theme list").max_height(150.0).auto_shrink([false, true]).show(
            ui,
            |ui| {
                for (index, name) in names.iter().enumerate() {
                    let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
                    let open = index == editing;
                    if open {
                        ui.painter().rect_filled(row, 7.0, ACCENT.gamma_multiply(0.22));
                    } else if response.hovered() {
                        ui.painter().rect_filled(row, 7.0, CARD);
                    }
                    // Tags at the right say what uses this theme.
                    let mut right = row.max.x - 6.0;
                    for (used, tag) in
                        [(state.theme_for_songs == index, "Songs"), (state.theme_for_bible == index, "Bible")]
                    {
                        if !used {
                            continue;
                        }
                        let text = ui.painter().layout_no_wrap(tag.to_owned(), FontId::proportional(10.0), TEXT_DIM);
                        let pill = Rect::from_min_size(
                            pos2(right - text.size().x - 10.0, row.center().y - 8.0),
                            vec2(text.size().x + 10.0, 16.0),
                        );
                        ui.painter().rect_filled(pill, 8.0, Color32::from_white_alpha(18));
                        ui.painter().galley(
                            pos2(pill.min.x + 5.0, pill.center().y - text.size().y / 2.0),
                            text,
                            TEXT_DIM,
                        );
                        right = pill.min.x - 4.0;
                    }
                    let name_area =
                        Rect::from_min_max(pos2(row.min.x + 6.0, row.min.y + 3.0), pos2(right - 2.0, row.max.y - 3.0));
                    if open && let Some(typed) = state.renaming_theme.as_mut() {
                        let field = crate::studio::rename_field_at(
                            ui,
                            typed,
                            FontId::proportional(13.5),
                            TEXT,
                            ACCENT,
                            pos2(row.min.x + 10.0, row.center().y),
                            name_area.max.x,
                        );
                        if std::mem::take(&mut state.rename_started) {
                            field.request_focus();
                        }
                        let (enter, escape) = ui
                            .input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                        if escape {
                            renamed = Some(None);
                        } else if enter || field.lost_focus() {
                            renamed = Some(Some(typed.clone()));
                        }
                    } else {
                        ui.painter().with_clip_rect(name_area).text(
                            pos2(row.min.x + 10.0, row.center().y),
                            Align2::LEFT_CENTER,
                            name.as_str(),
                            FontId::proportional(13.5),
                            if open { Color32::WHITE } else { TEXT },
                        );
                        // A click opens the theme; a click on the open theme's name, or a double
                        // click, renames it, as in a file list.
                        if response.double_clicked() || (response.clicked() && open) {
                            if !open {
                                state.theme_request = Some(ThemeRequest::Edit(index));
                            } else {
                                start_rename = true;
                            }
                        } else if response.clicked() {
                            state.theme_request = Some(ThemeRequest::Edit(index));
                        }
                    }
                    response.context_menu(|ui| {
                        if ui.add_enabled(state.theme_for_songs != index, egui::Button::new("Use for Songs")).clicked()
                        {
                            state.theme_request = Some(ThemeRequest::UseForSongs(index));
                            ui.close();
                        }
                        if open {
                            ui.separator();
                            if ui.button("Rename").clicked() {
                                start_rename = true;
                                ui.close();
                            }
                        }
                        // Deleting asks first. The built-in theme is always kept.
                        ui.separator();
                        let delete = ui
                            .add_enabled(index != 0, egui::Button::new("Delete Theme…"))
                            .on_disabled_hover_text("The built-in theme is always kept");
                        if delete.clicked() {
                            state.confirm_delete_theme = Some(index);
                            ui.close();
                        }
                    });
                }
            },
        );
        if start_rename {
            state.renaming_theme = names.get(editing).cloned();
            state.rename_started = true;
        }
        if let Some(outcome) = renamed {
            state.renaming_theme = None;
            if let (Some(name), Some(edit)) = (outcome, state.theme_edit.as_mut())
                && !name.trim().is_empty()
                && name.trim() != edit.theme.name
            {
                edit.theme.name = name.trim().to_owned();
                edit.changed = true;
            }
        }
        ui.add_space(8.0);
        crate::studio::caption(ui, "TEMPLATES");
    });
}

/// A menu that points a translation's words, or its reference, at one of the theme's boxes.
/// Each entry has a small picture of the slide with its box filled in. Boxes in `taken` are
/// in use elsewhere and cannot be chosen. `with_verse`, for a reference menu, is the box the
/// same translation's words are in: choosing "With Verse" puts the reference there too.
fn target_menu(
    ui: &mut egui::Ui,
    salt: (&str, usize),
    target: &mut BibleTarget,
    boxes: &[(pc_model::Role, SlideRect)],
    labels: &[String],
    taken: &[pc_model::Role],
    with_verse: Option<&pc_model::Role>,
) {
    let is_with_verse = with_verse.is_some() && target.as_ref() == with_verse;
    // A box is called what the theme calls it.
    let label_of = |role: &pc_model::Role| {
        boxes
            .iter()
            .position(|(have, _)| have == role)
            .and_then(|at| labels.get(at).cloned())
            .unwrap_or_else(|| role.label())
    };
    let shown = match target {
        _ if is_with_verse => "With Verse".to_owned(),
        Some(role) => label_of(role),
        None => "None".to_owned(),
    };
    egui::ComboBox::from_id_salt(salt).selected_text(shown).width(130.0).height(360.0).show_ui(ui, |ui| {
        let entry = |ui: &mut egui::Ui, name: &str, lit: Option<&pc_model::Role>, chosen: bool, free: bool| -> bool {
            let (row, response) =
                ui.allocate_exact_size(vec2(170.0, 34.0), if free { Sense::click() } else { Sense::hover() });
            if chosen {
                ui.painter().rect_filled(row, 5.0, crate::studio::ACCENT.gamma_multiply(0.3));
            } else if free && response.hovered() {
                ui.painter().rect_filled(row, 5.0, crate::studio::HOVER);
            }
            let picture = Rect::from_min_size(row.min + vec2(4.0, 4.0), vec2(46.0, 26.0));
            box_diagram(ui.painter(), picture, boxes, lit);
            let ink = if free { crate::studio::TEXT } else { crate::studio::TEXT_FAINT };
            ui.painter().text(
                pos2(picture.max.x + 10.0, row.center().y),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(13.0),
                ink,
            );
            if !free {
                response.on_hover_text("Another translation is using this box");
                return false;
            }
            response.clicked()
        };
        if entry(ui, "None", None, target.is_none(), true) {
            *target = None;
            ui.close();
        }
        if let Some(with) = with_verse
            && entry(ui, "With Verse", Some(with), is_with_verse, true)
        {
            *target = Some(with.clone());
            ui.close();
        }
        for (role, _) in boxes {
            // In a reference menu the words' own box is reached by "With Verse".
            if Some(role) == with_verse {
                continue;
            }
            let chosen = target.as_ref() == Some(role);
            let free = chosen || !taken.contains(role);
            if entry(ui, &label_of(role), Some(role), chosen, free) {
                *target = Some(role.clone());
                ui.close();
            }
        }
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
        egui::Panel::bottom("bible bar").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                // Make the passage one verse longer, at its start or its end.
                let shown = !state.bible.shown.is_empty();
                let back = ui.add_enabled(shown, egui::Button::new("‹ Verse")).on_hover_text("Add the verse before");
                let on = ui.add_enabled(shown, egui::Button::new("Verse ›")).on_hover_text("Add the verse after");
                if back.clicked() {
                    state.bible.step(false, live);
                } else if on.clicked() {
                    state.bible.step(true, live);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| crate::studio::view_controls(ui, state));
            });
        });

        Frame::new().fill(SIDE).inner_margin(egui::Margin::symmetric(8, 8)).show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            ui.horizontal(|ui| {
                let entered = |response: &egui::Response, ui: &egui::Ui| {
                    response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter))
                };
                let lookup_id = ui.scope_id().with("scripture lookup");
                let lookup = ui
                    .add(
                        egui::TextEdit::singleline(&mut state.bible.lookup)
                            .id(lookup_id)
                            .hint_text("Scripture Lookup")
                            .desired_width(240.0),
                    )
                    .on_hover_text("Type a reference, such as John 3:16, and press Return");
                if entered(&lookup, ui) {
                    state.bible.look_up(live);
                    // The field keeps the keyboard with its words selected, ready for the next one.
                    lookup.request_focus();
                    if let Some(mut edit) = egui::text_edit::TextEditState::load(ui.ctx(), lookup_id) {
                        let all = egui::text::CCursorRange::two(
                            egui::text::CCursor::new(0),
                            egui::text::CCursor::new(state.bible.lookup.chars().count()),
                        );
                        edit.cursor.set_char_range(Some(all));
                        edit.store(ui.ctx(), lookup_id);
                    }
                }
                // Book, then chapter, then verse, for when the reference is easier to pick than type.
                let lead = state.bible.rows.first().map(|row| row.bible).and_then(|at| state.bible.shelf.get(at).ok());
                let mut chosen = None;
                ui.menu_button("Books", |ui| {
                    let Some(bible) = &lead else { return };
                    egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                        for book in 0..pc_bible::BOOKS.len() {
                            let chapters = bible.chapter_count(book);
                            if chapters == 0 {
                                continue;
                            }
                            let name = bible.book_name(book);
                            ui.menu_button(name, |ui| {
                                egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                                    for chapter in 1..=chapters {
                                        ui.menu_button(chapter.to_string(), |ui| {
                                            egui::ScrollArea::vertical().max_height(460.0).show(ui, |ui| {
                                                if ui.button("Whole chapter").clicked() {
                                                    chosen = Some(format!("{name} {chapter}"));
                                                }
                                                ui.separator();
                                                for verse in 1..=bible.last_verse(book, chapter) {
                                                    if ui.button(verse.to_string()).clicked() {
                                                        chosen = Some(format!("{name} {chapter}:{verse}"));
                                                    }
                                                }
                                            });
                                        });
                                    }
                                });
                            });
                        }
                    });
                })
                .response
                .on_hover_text("Pick a book, chapter and verse");
                if let Some(reference) = chosen {
                    state.bible.lookup = reference;
                    state.bible.look_up(live);
                    ui.close();
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
                            &crate::bible::ThemeChoice {
                                names: &state.theme_names,
                                template_names: &state.template_names,
                                layouts: &state.theme_layouts,
                                theme: state.theme_for_bible,
                                layout: state.bible_layout,
                            },
                        );
                        if let Some((index, layout)) = picked {
                            state.theme_request = Some(ThemeRequest::UseForBible(index, layout));
                        }
                        if let Some(index) = state.bible.options.remove.take() {
                            state.bible.remove_bible(index);
                        }
                    }
                    // Keeps the passage in the service, to be brought back with one click.
                    let saved = state.bible.saved();
                    let save = ui
                        .add_enabled(saved.is_some(), egui::Button::new("Save to Service"))
                        .on_hover_text("Keep this passage in the service, ready to open again")
                        .on_disabled_hover_text("Look a passage up first");
                    if save.clicked()
                        && let Some(saved) = saved
                    {
                        state.services.add(crate::service::Item::Passage(saved));
                    }
                });
            });

            let mut add = false;
            let mut remove = None;
            let count = state.bible.rows.len();
            let boxes = state.bible.boxes.clone();
            let labels = state.bible.box_labels.clone();
            let used: Vec<(BibleTarget, BibleTarget)> =
                state.bible.rows.iter().map(|row| (row.verse.clone(), row.reference.clone())).collect();
            let bibles_used: Vec<usize> = state.bible.rows.iter().map(|row| row.bible).collect();
            for (index, row) in state.bible.rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let chosen = state.bible.shelf.bibles.get(row.bible).map_or("Choose a Bible", |b| b.name.as_str());
                    egui::ComboBox::from_id_salt(("bible translation", index)).selected_text(chosen).width(200.0).show_ui(
                        ui,
                        |ui| {
                            // A Bible another row already shows is greyed out.
                            for (place, bible) in state.bible.shelf.bibles.iter().enumerate() {
                                let elsewhere = place != row.bible && bibles_used.contains(&place);
                                let label = format!("{} ({})", bible.name, bible.short);
                                let item = ui.add_enabled(!elsewhere, egui::Button::selectable(row.bible == place, label));
                                if item.on_disabled_hover_text("Another row shows this Bible").clicked() {
                                    row.bible = place;
                                }
                            }
                        },
                    );
                    ui.add_space(8.0);
                    ui.label("Verse");
                    // Boxes another translation already fills are not offered again.
                    let taken = |by_verse: bool| -> Vec<pc_model::Role> {
                        used.iter()
                            .enumerate()
                            .flat_map(|(other, (verse, reference))| {
                                let mine = other == index;
                                let verse = verse.clone().filter(|_| !(mine && by_verse));
                                let reference = reference.clone().filter(|_| !mine);
                                [verse, reference]
                            })
                            .flatten()
                            .collect()
                    };
                    target_menu(ui, ("bible verse", index), &mut row.verse, &boxes, &labels, &taken(true), None);
                    ui.add_space(8.0);
                    ui.label("Reference");
                    let with = row.verse.clone();
                    target_menu(ui, ("bible reference", index), &mut row.reference, &boxes, &labels, &taken(false), with.as_ref());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        // The first row adds a translation; the others remove themselves.
                        if index == 0 {
                            if ui.add_enabled(count < BiblePage::MAX_ROWS, egui::Button::new("+")).on_hover_text("Add a translation").clicked() {
                                add = true;
                            }
                        } else if ui.button("−").on_hover_text("Remove this translation").clicked() {
                            remove = Some(index);
                        }
                        // License: a click shows the Bible's name, abbreviation and terms.
                        let license = ui.add(egui::Button::new(egui::RichText::new("License").size(11.0).color(DIM)).frame(false));
                        let shelved = state.bible.shelf.bibles.get(row.bible);
                        egui::Popup::from_toggle_button_response(&license)
                            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                            .show(|ui| {
                                ui.set_width(300.0);
                                if let Some(bible) = shelved {
                                    egui::Grid::new(("license", index)).num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                                        for (label, value) in [
                                            ("Bible", bible.name.as_str()),
                                            ("Abbreviation", bible.short.as_str()),
                                            ("Copyright", bible.terms.as_str()),
                                            ("Credit", bible.credit.as_str()),
                                        ] {
                                            if value.is_empty() {
                                                continue;
                                            }
                                            ui.label(egui::RichText::new(label).color(DIM));
                                            ui.add(egui::Label::new(value).wrap());
                                            ui.end_row();
                                        }
                                    });
                                }
                            });
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
            // Search results: each can be opened as its verse or as its whole chapter.
            let mut picked = None;
            egui::ScrollArea::vertical().id_salt("bible found").auto_shrink([false, false]).show(ui, |ui| {
                Frame::new().inner_margin(egui::Margin::symmetric(16, 10)).show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.horizontal(|ui| {
                        let count = state.bible.found.len();
                        let found = if count == 1 { "1 verse found".to_owned() } else { format!("{count} verses found") };
                        ui.label(egui::RichText::new(found).size(12.0).color(DIM));
                        if ui.button("Close").clicked() {
                            picked = Some(String::new());
                        }
                    });
                    for (reference, chapter, words) in &state.bible.found {
                        Frame::new().fill(crate::studio::CARD).corner_radius(8).inner_margin(egui::Margin::symmetric(12, 8)).show(
                            ui,
                            |ui| {
                                ui.horizontal(|ui| {
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        if ui.button("Chapter").on_hover_text("Open the whole chapter as slides").clicked() {
                                            picked = Some(chapter.clone());
                                        }
                                        if ui.button("Verse").on_hover_text("Open this verse as a slide").clicked() {
                                            picked = Some(reference.clone());
                                        }
                                        ui.with_layout(Layout::left_to_right(Align::Center).with_main_wrap(false), |ui| {
                                            ui.label(egui::RichText::new(reference.as_str()).font(crate::studio::semibold(13.0)));
                                            ui.add(egui::Label::new(egui::RichText::new(words.as_str()).size(13.0).color(DIM)).truncate());
                                        });
                                    });
                                });
                            },
                        );
                    }
                });
            });
            match picked {
                Some(reference) if reference.is_empty() => state.bible.found.clear(),
                Some(reference) => {
                    state.bible.lookup = reference;
                    state.bible.look_up(live);
                }
                None => {}
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

/// The middle: the selected song's name and how its slides are laid out, then the slides.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    theme: &Theme,
    thumb: &mut dyn FnMut(ThumbOf) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        let Some(presentation) = show.presentations().get(show.selected()) else {
            ui.centered_and_justified(|ui| ui.label(egui::RichText::new("Open a song to see its slides.").color(DIM)));
            return;
        };
        // The Reflow editor takes the slides' place while it is open.
        if state.reflow.as_ref().is_some_and(|(index, _)| *index == show.selected()) {
            reflow_editor(ui, &presentation.title, state);
            return;
        }
        let heading = Frame::new().inner_margin(egui::Margin { left: 20, right: 16, top: 12, bottom: 6 });
        egui::Panel::top("song header").resizable(false).frame(heading).show(ui, |ui| {
            ui.horizontal(|ui| {
                // The title; a double click, or a new song, turns it into a field to retype it.
                if let Some(title) = state.renaming_song.as_mut() {
                    // The field takes the title's own room, growing as it is typed, as a file
                    // name does in the Finder.
                    let font = crate::studio::semibold(18.0);
                    let width = ui.painter().layout_no_wrap(title.clone(), font.clone(), Color32::WHITE).size().x;
                    let line = ui.fonts_mut(|fonts| fonts.row_height(&font));
                    let (place, _) = ui.allocate_exact_size(vec2(width + TITLE_ROOM, line), Sense::hover());
                    let field = crate::studio::rename_field_at(
                        ui,
                        title,
                        font,
                        crate::studio::TEXT,
                        crate::studio::ACCENT,
                        place.left_center(),
                        place.max.x,
                    );
                    if std::mem::take(&mut state.rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        state.renaming_song = None;
                    } else if enter || field.lost_focus() {
                        if !title.trim().is_empty() && title.trim() != presentation.title {
                            state.pending_title = Some((show.selected(), title.trim().to_owned()));
                        }
                        state.renaming_song = None;
                    }
                } else {
                    let title = ui
                        .add(
                            egui::Label::new(
                                egui::RichText::new(presentation.title.as_str()).font(crate::studio::semibold(18.0)),
                            )
                            .sense(Sense::click()),
                        )
                        .on_hover_text("Double-click to rename");
                    // The same room the field takes, so renaming moves nothing.
                    ui.add_space(TITLE_ROOM);
                    if title.double_clicked() {
                        state.renaming_song = Some(presentation.title.clone());
                        state.rename_started = true;
                    }
                }
                let slides = presentation.slide_count();
                let count = if slides == 1 { "1 slide".to_owned() } else { format!("{slides} slides") };
                ui.label(egui::RichText::new(count).size(12.0).color(crate::studio::TEXT_FAINT));
                let from_bible = state.bible.slots.is_some_and(|slots| slots.contains(&show.selected()));
                if !from_bible {
                    let info = ui.small_button("Info").on_hover_text("The song's writer, publisher and CCLI number");
                    egui::Popup::from_toggle_button_response(&info)
                        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                        .show(|ui| {
                            ui.set_width(320.0);
                            let mut details = presentation.copyright.clone();
                            egui::Grid::new("song info").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                                for (label, value) in [
                                    ("Artist", &mut details.artist),
                                    ("Author", &mut details.author),
                                    ("Publisher", &mut details.publisher),
                                    ("Year", &mut details.year),
                                    ("CCLI Song #", &mut details.ccli_song),
                                ] {
                                    ui.label(egui::RichText::new(label).color(DIM));
                                    ui.add(egui::TextEdit::singleline(value).desired_width(200.0));
                                    ui.end_row();
                                }
                            });
                            ui.add_space(4.0);
                            ui.label(
                                egui::RichText::new("Settings, General says when the copyright line is shown.")
                                    .size(11.0)
                                    .color(DIM),
                            );
                            if details != presentation.copyright {
                                state.copyright_request = Some((show.selected(), details));
                            }
                        });
                }
                if !from_bible
                    && ui.small_button("Reflow").on_hover_text("Edit the song's words as text (Control-R)").clicked()
                {
                    state.reflow = Some((show.selected(), pc_model::to_song_text(presentation)));
                }
                if ui
                    .add(egui::Button::selectable(state.show_notes, "Notes"))
                    .on_hover_text("Show the notes of the slide on screen, for whoever runs the slides")
                    .clicked()
                {
                    state.show_notes = !state.show_notes;
                }
                missing_font_notice(
                    ui,
                    &pc_model::missing_fonts(&pc_model::fonts_used(presentation, theme), &state.fonts),
                );
            });
        });
        // The bar under the slides, as ProPresenter has it: Add Slide on the left; the
        // transition, the slide view and the slide size on the right.
        egui::Panel::bottom("slide bar").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                let from_bible = state.bible.slots.is_some_and(|slots| slots.contains(&show.selected()));
                if !from_bible
                    && ui.button("+  Add Slide").on_hover_text("Add a slide laid out like the last one").clicked()
                {
                    state.add_slide = Some(show.selected());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| crate::studio::view_controls(ui, state));
            });
        });
        // Operator notes: the notes of the slide on screen, or of the one picked out.
        if state.show_notes {
            let at = show.live().or(state.selected_slide).or(show.cursor());
            let notes = at
                .and_then(|at| show.presentations().get(at.presentation)?.slide(at.slide))
                .and_then(|(_, slide)| slide.notes.clone());
            let strip = Frame::new().fill(crate::studio::CARD).inner_margin(egui::Margin::symmetric(20, 8));
            egui::Panel::top("operator notes").resizable(false).frame(strip).show(ui, |ui| {
                let (text, ink) = match notes {
                    Some(notes) => (notes, crate::studio::TEXT),
                    None => (
                        "No notes for this slide. Add them in the editor with Slide Notes.".to_owned(),
                        crate::studio::TEXT_FAINT,
                    ),
                };
                ui.label(egui::RichText::new(text).size(13.0).color(ink));
            });
        }
        slide_grid(ui, show, show.selected(), state, thumb, commands);
    });
}

/// The outline of the slide on screen, of a slide picked out by a right-click, and of the
/// slide that was last on screen once the screen has been cleared.
const LIVE_OUTLINE: Color32 = crate::studio::TALLY;
const SELECTED_OUTLINE: Color32 = crate::studio::ACCENT;
const WAS_LIVE_OUTLINE: Color32 = Color32::from_rgb(110, 60, 58);

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
    let gap = 16.0;
    let bar_height = 22.0;
    egui::ScrollArea::vertical().id_salt("slide grid").auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(10.0);
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
                        if !from_bible {
                            ui.separator();
                            if ui.button("Cut").clicked() {
                                state.slide_clip = Some(slide.clone());
                                state.slide_request = Some((here, SlideRequest::Delete));
                                ui.close();
                            }
                            if ui.button("Copy").clicked() {
                                state.slide_clip = Some(slide.clone());
                                ui.close();
                            }
                            let clip = state.slide_clip.clone();
                            if ui.add_enabled(clip.is_some(), egui::Button::new("Paste")).clicked()
                                && let Some(clip) = clip
                            {
                                state.slide_request = Some((here, SlideRequest::PasteAfter(clip)));
                                ui.close();
                            }
                            if ui.button("Delete").clicked() {
                                state.slide_request = Some((here, SlideRequest::Delete));
                                ui.close();
                            }
                            ui.separator();
                            // Where a group starts: this slide and the rest of its group take the name.
                            ui.menu_button("Group", |ui| {
                                for name in GROUP_NAMES {
                                    if ui.button(name).clicked() {
                                        state.slide_request = Some((here, SlideRequest::Group(name.to_owned())));
                                        ui.close();
                                    }
                                }
                            });
                            ui.menu_button("Label", |ui| {
                                if ui.add(egui::Button::selectable(slide.label.is_none(), "None")).clicked() {
                                    state.slide_request = Some((here, SlideRequest::Label(None)));
                                    ui.close();
                                }
                                ui.separator();
                                for name in SLIDE_LABELS {
                                    let on = slide.label.as_deref() == Some(name);
                                    if ui.add(egui::Button::selectable(on, name)).clicked() {
                                        state.slide_request = Some((here, SlideRequest::Label(Some(name.to_owned()))));
                                        ui.close();
                                    }
                                }
                            });
                        }
                        // A disabled slide stays in the song but is stepped over and never shown.
                        if !from_bible {
                            let label = if slide.enabled { "Disable Slide" } else { "Enable Slide" };
                            if ui.button(label).clicked() {
                                state.pending_edit = Some((here, Slide { enabled: !slide.enabled, ..slide.clone() }));
                                ui.close();
                            }
                        }
                        // Which of the theme's templates the slide is drawn with. Automatic
                        // lets the theme pick the one that suits what is on the slide.
                        if !from_bible && !state.song_templates.is_empty() {
                            ui.menu_button("Template", |ui| {
                                let current =
                                    slide.template.filter(|id| state.song_templates.iter().any(|(t, _)| t == id));
                                if ui.add(egui::Button::selectable(current.is_none(), "Automatic")).clicked() {
                                    state.pending_template = Some((here, None));
                                    ui.close();
                                }
                                ui.separator();
                                for (id, name) in &state.song_templates {
                                    if ui.add(egui::Button::selectable(current == Some(*id), name.as_str())).clicked() {
                                        state.pending_template = Some((here, Some(*id)));
                                        ui.close();
                                    }
                                }
                            });
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
                        painter.rect_filled(image, egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 }, Color32::BLACK);
                        let of = if from_bible { ThumbOf::BibleSlide(slide) } else { ThumbOf::Slide(slide) };
                        match thumb(of) {
                            Some((texture, overflow)) => {
                                egui::Image::from_texture((texture, image.size()))
                                    .corner_radius(egui::CornerRadius { nw: 6, ne: 6, sw: 0, se: 0 })
                                    .paint_at(ui, image);
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
                            // Under the slide, a bar in its group's colour across the whole width:
                            // the slide's number, the group's name on the group's first slide, and
                            // the key that jumps to the group at the right.
                            let foot = Rect::from_min_max(pos2(rect.min.x, image.max.y), rect.max);
                            let corners = egui::CornerRadius { nw: 0, ne: 0, sw: 6, se: 6 };
                            painter.rect_filled(foot, corners, color32(group.color));
                            let number = painter.text(
                                foot.left_center() + vec2(8.0, 0.0),
                                Align2::LEFT_CENTER,
                                (index + 1).to_string(),
                                crate::studio::semibold(12.0),
                                ink,
                            );
                            if starts_group && !group.name.is_empty() {
                                let key = if from_bible { None } else { pc_model::key_for_group(&group.name) };
                                let name_end = foot.max.x - if key.is_some() { 22.0 } else { 8.0 };
                                let clip = Rect::from_min_max(
                                    pos2(number.max.x + 6.0, foot.min.y),
                                    pos2(name_end, foot.max.y),
                                );
                                painter.with_clip_rect(clip).text(
                                    pos2(number.max.x + 8.0, foot.center().y),
                                    Align2::LEFT_CENTER,
                                    group.name.as_str(),
                                    FontId::proportional(12.0),
                                    ink,
                                );
                                if let Some(key) = key {
                                    painter.text(
                                        foot.right_center() - vec2(8.0, 0.0),
                                        Align2::RIGHT_CENTER,
                                        key,
                                        FontId::monospace(10.5),
                                        ink.gamma_multiply(0.7),
                                    );
                                }
                            }
                        }
                    }
                    // A slide's label, in a small dark tab at the picture's bottom right.
                    if let Some(label) = slide.label.as_deref().filter(|label| !label.is_empty()) {
                        let ink = Color32::from_gray(225);
                        let text = painter.layout_no_wrap(label.to_owned(), FontId::proportional(10.5), ink);
                        let bottom =
                            if view == SlideView::Table { rect.max.y - 6.0 } else { rect.max.y - bar_height - 6.0 };
                        let tab = Rect::from_min_max(
                            pos2(rect.max.x - text.size().x - 18.0, bottom - text.size().y - 4.0),
                            pos2(rect.max.x - 6.0, bottom),
                        );
                        painter.rect_filled(tab, 4.0, Color32::from_black_alpha(190));
                        painter.galley(pos2(tab.min.x + 6.0, tab.min.y + 2.0), text, ink);
                    }
                    // A disabled slide is dimmed and says so: it is never shown.
                    if !slide.enabled {
                        painter.rect_filled(rect, 6.0, Color32::from_black_alpha(170));
                        painter.text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Disabled",
                            crate::studio::semibold(13.0),
                            crate::studio::TEXT_DIM,
                        );
                    }
                    // A hairline round the slide and its bar, so black slides do not melt into the background.
                    if view != SlideView::Table {
                        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, crate::studio::LINE), StrokeKind::Outside);
                    }
                    // Tally red for the slide on screen; the accent for one picked
                    // out; a dim red for the slide that was on screen before it was cleared.
                    let framed = rect;
                    let is_live = show.live() == Some(here);
                    let outline = if is_live {
                        Some(Stroke::new(2.5, LIVE_OUTLINE))
                    } else if state.selected_slide == Some(here) {
                        Some(Stroke::new(2.0, SELECTED_OUTLINE))
                    } else if show.live().is_none() && show.cursor() == Some(here) {
                        Some(Stroke::new(1.5, WAS_LIVE_OUTLINE))
                    } else if response.hovered() {
                        Some(Stroke::new(1.0, crate::studio::TEXT_FAINT))
                    } else {
                        None
                    };
                    if let Some(stroke) = outline {
                        painter.rect_stroke(framed, 6.0, stroke, StrokeKind::Outside);
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

    #[test]
    fn the_bible_pages_set_up_survives_being_saved_and_read_back() {
        let role = |name: &str| Some(pc_model::Role(name.to_owned()));
        let mut page = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        let (kjv, syn) = (page.shelf.position("kjv").unwrap(), page.shelf.position("syn").unwrap());
        // Not the usual set-up: Synodal first, in the translation box; KJV in the main box
        // with no reference; and the options changed.
        page.rows = vec![
            BibleRow { bible: syn, verse: role("translation"), reference: role("reference") },
            BibleRow { bible: kjv, verse: role("primary"), reference: None },
        ];
        page.options.show_verse_numbers = !page.options.show_verse_numbers;
        page.options.reference = crate::bible::ReferenceStyle::PassageLast;
        let text = serde_json::to_string(&page.settings()).unwrap();

        let mut fresh = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        assert_ne!(fresh.choices(), page.choices());
        let missing = fresh.apply_settings(&serde_json::from_str(&text).unwrap());
        assert!(missing.is_empty());
        assert_eq!(fresh.rows.len(), 2);
        for (got, want) in fresh.rows.iter().zip(&page.rows) {
            assert_eq!((got.bible, &got.verse, &got.reference), (want.bible, &want.verse, &want.reference));
        }
        assert_eq!(fresh.options.show_verse_numbers, page.options.show_verse_numbers);
        assert_eq!(fresh.options.reference, crate::bible::ReferenceStyle::PassageLast);
        assert_eq!(fresh.choices(), page.choices());

        // A box the Bible template does not have is sent nowhere, not drawn over the main text.
        let mut narrow = BiblePage { shelf: crate::bible::Shelf::shipped(), ..BiblePage::default() };
        narrow.boxes.retain(|(have, _)| *have != pc_model::Role::translation());
        narrow.apply_settings(&serde_json::from_str(&text).unwrap());
        assert_eq!((&narrow.rows[0].verse, &narrow.rows[0].reference), (&None, &role("reference")));
    }

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
                Preview::default(),
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
                    Preview::default(),
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
    fn an_object_is_listed_by_its_words_until_it_is_named() {
        let mut object = pc_model::Object::text(pc_model::Role::primary(), "\nLight rises\nover the hills");
        assert_eq!(listed_name(&object), "Light rises");
        object.name = "Lyrics Box".to_owned();
        assert_eq!(listed_name(&object), "Lyrics Box");
        let shape = pc_model::Object::shape(pc_model::Shape::Star);
        assert_eq!(listed_name(&shape), "Star");
    }

    #[test]
    fn a_library_search_finds_titles_first_then_words() {
        let show = Show::new(vec![
            std::sync::Arc::new(pc_model::parse_song_text("Harbour Lights", "When the night is long\n")),
            std::sync::Arc::new(pc_model::parse_song_text("Steady Ground", "You are the light on the farther side\n")),
        ]);
        let found = search_songs(&show, |_| true, "light");
        assert_eq!(found.iter().map(|(index, _)| *index).collect::<Vec<_>>(), [0, 1]);
        assert_eq!(found[1].1.as_deref(), Some("You are the light on the farther side"));
        assert!(search_songs(&show, |_| true, "  ").is_empty());
        assert!(search_songs(&show, |index| index != 0, "harbour").is_empty(), "a song not listed is left out");
    }

    #[test]
    fn capitalize_makes_each_word_start_with_a_capital() {
        assert_eq!(capitalize("light rises over the hills"), "Light Rises Over The Hills");
        assert_eq!(capitalize("свет встаёт\nи утро"), "Свет Встаёт\nИ Утро");
        assert_eq!(capitalize("don't stop"), "Don't Stop");
    }

    #[test]
    fn the_screens_window_draws_every_kind_of_screen_and_changes_nothing_by_itself() {
        let ctx = egui::Context::default();
        let screens = vec![
            Screen::new("A".to_owned(), Output::Automatic),
            Screen::new("B".to_owned(), Output::Display("Projector".to_owned())),
            Screen::new("C".to_owned(), Output::Display("Gone".to_owned())),
            Screen::new("D".to_owned(), Output::Window),
            Screen::new("E".to_owned(), Output::Ndi("Main".to_owned())),
            Screen::new("F".to_owned(), Output::Off),
        ];
        for selected in 0..screens.len() + 1 {
            let mut state = HomeState {
                settings_open: true,
                screen_selected: selected,
                screens: screens.clone(),
                displays: vec![
                    DisplayInfo { index: 0, name: "Built-in".into(), x: 0, y: 0, w: 3024, h: 1964 },
                    DisplayInfo { index: 1, name: "Projector".into(), x: 3024, y: 0, w: 1920, h: 1080 },
                ],
                ..HomeState::default()
            };
            for _ in 0..2 {
                let mut drawn = ctx.run_ui(input(None), |ui| {
                    assert!(screens_ui(ui, &mut state, true).is_empty(), "nothing was clicked");
                });
                drawn.textures_delta.clear();
            }
            assert!(!state.screens_changed, "nothing was clicked, so nothing is changed");
            assert_eq!(state.screens, screens);
        }
        assert_eq!(output_label(&screens[2].output, &[]), "Gone   (disconnected)");
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
