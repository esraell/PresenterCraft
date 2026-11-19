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

/// The one accent colour: the live slide's outline and selections.
pub const ACCENT: Color32 = Color32::from_rgb(64, 156, 255);
const BAR: Color32 = Color32::from_rgb(38, 38, 42);
const SIDE: Color32 = Color32::from_rgb(30, 30, 33);
const CANVAS: Color32 = Color32::from_rgb(20, 20, 22);
const ON: Color32 = Color32::from_rgb(70, 200, 110);
const OFF: Color32 = Color32::from_rgb(210, 70, 70);
const DIM: Color32 = Color32::from_gray(105);
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
        HomeState { thumb_width: 200.0, filter: String::new(), output_stopped: false }
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

/// A small placeholder button: a symbol, dim, that only explains itself.
fn ghost(ui: &mut egui::Ui, symbol: &str, what: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::hover());
    if response.hovered() {
        ui.painter().rect_filled(rect, 4.0, Color32::from_gray(48));
    }
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, symbol, FontId::proportional(13.0), DIM);
    response.on_hover_text(not_built(what));
}

/// A section heading with placeholder buttons at its right edge.
fn heading_with(ui: &mut egui::Ui, text: &str, buttons: &[(&str, &str)]) {
    ui.horizontal(|ui| {
        section_label(ui, text);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            for (symbol, what) in buttons {
                ghost(ui, symbol, what);
            }
        });
    });
}

/// One toolbar item: a symbol above a small label. Items that are not built are dim.
fn tool(ui: &mut egui::Ui, symbol: &str, label: &str, selected: bool, built: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(vec2(50.0, 40.0), if built { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, 6.0, Color32::from_gray(64));
    } else if response.hovered() {
        painter.rect_filled(rect, 6.0, Color32::from_gray(50));
    }
    let ink = if !built {
        DIM
    } else if selected {
        Color32::WHITE
    } else {
        Color32::from_gray(200)
    };
    painter.text(rect.center_top() + vec2(0.0, 13.0), Align2::CENTER_CENTER, symbol, FontId::proportional(16.0), ink);
    painter.text(rect.center_bottom() - vec2(0.0, 8.0), Align2::CENTER_CENTER, label, FontId::proportional(10.5), ink);
    if built { response } else { response.on_hover_text(not_built(label)) }
}

/// A toolbar item for a screen: a status light above its name. `None` is a screen that is not
/// built yet.
fn screen_light(ui: &mut egui::Ui, label: &str, on: Option<bool>) -> egui::Response {
    let built = on.is_some();
    let (rect, response) =
        ui.allocate_exact_size(vec2(58.0, 40.0), if built { Sense::click() } else { Sense::hover() });
    let painter = ui.painter();
    if response.hovered() {
        painter.rect_filled(rect, 6.0, Color32::from_gray(50));
    }
    let light = match on {
        Some(true) => ON,
        Some(false) => OFF,
        None => Color32::from_gray(70),
    };
    painter.circle_filled(rect.center_top() + vec2(0.0, 13.0), 6.5, light);
    let ink = if built { Color32::from_gray(200) } else { DIM };
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
            ui.spacing_mut().item_spacing.x = 2.0;
            tool(ui, "🔍", "Search", false, false);
            ui.add_space(8.0);
            tool(ui, "Aa", "Text", false, false);
            tool(ui, "◧", "Theme", false, false);
            ui.add_space(8.0);
            tool(ui, "▶", "Show", true, true);
            tool(ui, "✎", "Edit", false, false);
            tool(ui, "≡", "Reflow", false, false);
            tool(ui, "§", "Bible", false, false);
            tool(ui, "⋯", "More", false, false);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                tool(ui, "◎", "Status", false, false);
                ui.add_space(8.0);
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
                ui.add_space(8.0);
                tool(ui, "◉", "Live", false, false);
                tool(ui, "◐", "Look", false, false);
                tool(ui, "▣", "Media", false, false);
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
                heading_with(ui, "Playlists", &[("+", "Add a media playlist")]);
                ui.label(egui::RichText::new("No media playlists yet").size(11.5).color(DIM));
            },
        );
        egui::Panel::bottom("library filter").resizable(false).frame(panel_frame(SIDE, 8)).show(ui, |ui| {
            ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Filter").desired_width(f32::INFINITY));
        });

        Frame::new().inner_margin(8).show(ui, |ui| {
            heading_with(ui, "LIBRARY", &[("⌃", "Collapse"), ("+", "Add a library")]);
            // One library for now: everything that was opened.
            ui.add_sized([ui.available_width(), 22.0], egui::Button::selectable(false, "▤  Songs"));
            ui.add_space(6.0);
            section_label(ui, "PLAYLIST");
            ui.label(egui::RichText::new("No playlists yet").size(11.5).color(DIM));
        });
        ui.separator();

        Frame::new().inner_margin(8).show(ui, |ui| {
            let shown = show.presentations().iter().filter(|p| matches_filter(&p.title, &state.filter)).count();
            heading_with(ui, &format!("{shown} ITEMS"), &[("+", "Add an item")]);
            ui.add_space(2.0);
            egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
                for (index, presentation) in show.presentations().iter().enumerate() {
                    if !matches_filter(&presentation.title, &state.filter) {
                        continue;
                    }
                    let row = ui.add_sized(
                        [ui.available_width(), 22.0],
                        egui::Button::selectable(index == show.selected(), format!("▤  {}", presentation.title))
                            .right_text(egui::RichText::new("Songs").weak().size(11.0)),
                    );
                    if row.clicked() {
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
fn layer_button(ui: &mut egui::Ui, symbol: &str, lit: bool, built: bool, hint: &str) -> bool {
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
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, symbol, FontId::proportional(12.0), ink);
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
                if layer_button(ui, "✖", live, true, "Clear everything from the screen (F1)") {
                    commands.push(Command::ClearAll);
                }
                layer_button(ui, "♪", false, false, "Clear audio");
                layer_button(ui, "✉", false, false, "Clear messages");
                layer_button(ui, "◆", false, false, "Clear props");
                layer_button(ui, "❢", false, false, "Clear announcements");
                if layer_button(ui, "▤", live, true, "Clear the slide: take the words off the screen (C)") {
                    commands.push(Command::Clear(Layer::Slides));
                }
                layer_button(ui, "▣", false, false, "Clear media");
                layer_button(ui, "◙", false, false, "Clear video input");
            });
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Audience screen  ⌄").size(11.5).color(Color32::from_gray(150)))
                .on_hover_text(not_built("Choose which screen the preview shows"));
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
            ghost(ui, "▤", "Presentation playback");
            ghost(ui, "❢", "Announcement playback");
            ghost(ui, "♪", "Audio playback");
        });
        ui.vertical_centered(|ui| ui.label(egui::RichText::new("No media playing").size(11.5).color(DIM)));
        let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 10.0), Sense::hover());
        ui.painter().rect_filled(bar.shrink2(vec2(0.0, 3.5)), 2.0, Color32::from_gray(60));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.label(egui::RichText::new("--:--").size(11.0).color(DIM));
            ui.add_space((ui.available_width() - 150.0 - 30.0).max(0.0) / 2.0);
            ghost(ui, "↺", "Back 15 seconds");
            ghost(ui, "▶", "Play or pause");
            ghost(ui, "↻", "Forward 15 seconds");
            ghost(ui, "⇥", "Jump to the last 30 seconds");
            ghost(ui, "⚑", "Markers");
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(egui::RichText::new("--:--").size(11.0).color(DIM));
            });
        });
        ui.add_space(4.0);

        // The tabs: audio, stage, timers, messages, props, macros. Stage is the one shown.
        let tabs =
            [("♪", "Audio"), ("▦", "Stage"), ("◷", "Timers"), ("➤", "Messages"), ("◆", "Props"), ("M", "Macros")];
        let tab_width = ui.available_width() / tabs.len() as f32;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (index, (symbol, name)) in tabs.iter().enumerate() {
                let (rect, response) = ui.allocate_exact_size(vec2(tab_width, 24.0), Sense::hover());
                let selected = index == 1;
                ui.painter().rect_filled(
                    rect,
                    0.0,
                    if selected { ACCENT.gamma_multiply(0.75) } else { Color32::from_gray(46) },
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    *symbol,
                    FontId::proportional(13.0),
                    if selected { Color32::WHITE } else { DIM },
                );
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

        heading_with(ui, "STAGE", &[("+", "Add a stage screen"), ("✎", "Edit stage layouts")]);
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
    ghost(ui, "+", "Add");
    ui.add_space(6.0);
    ghost(ui, "▮▮", "Cut");
    ghost(ui, "◪", "Dissolve");
    ghost(ui, "▶▏", "More transitions");
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
                    ghost(ui, "◫", "Show or hide the media list");
                    ghost(ui, "‹", "Back");
                    ghost(ui, "›", "Forward");
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
                        ghost(ui, "≡", "List view");
                        ghost(ui, "▦", "Grid view");
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
        egui::Panel::top("library title").resizable(false).frame(panel_frame(Color32::from_gray(58), 6)).show(
            ui,
            |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Songs").strong().size(15.0).color(Color32::WHITE));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| ghost(ui, "⚙", "View settings"));
                });
            },
        );
        egui::Panel::bottom("slide bar").resizable(false).frame(panel_frame(BAR, 5)).show(ui, |ui| {
            ui.horizontal(|ui| {
                transition_controls(ui);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.add(egui::Slider::new(&mut state.thumb_width, 120.0..=480.0).show_value(false))
                        .on_hover_text("Slide size");
                    ghost(ui, "◌", "View options");
                    ghost(ui, "≡", "Table view");
                    ghost(ui, "T", "Text view");
                    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::hover());
                    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "▦", FontId::proportional(13.0), ACCENT);
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
                ui.label(egui::RichText::new(format!("▤  {}", presentation.title)).size(13.5));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ghost(ui, "▶▏", "Transition for this presentation");
                    ghost(ui, "◎", "Destination");
                    ui.add_space(6.0);
                    ghost(ui, "◫", "Timeline");
                    ghost(ui, "≣", "Arrangements");
                    ghost(ui, "✍", "Operator notes");
                    ui.add_space(6.0);
                    ghost(ui, "▷", "Slide show");
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
