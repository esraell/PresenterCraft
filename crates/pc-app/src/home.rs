//! The home window: a toolbar, the library, the slide grid and the live panel.
//!
//! This draws one frame and returns what the operator asked for. It changes nothing itself.

use egui::{
    Align, Align2, Color32, CornerRadius, FontId, Frame, Key, Layout, Rect, Sense, Stroke, StrokeKind, TextureId, pos2,
    vec2,
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

pub struct HomeState {
    /// Displayed thumbnail width in points.
    pub thumb_width: f32,
    /// Text typed into the library's search box.
    pub filter: String,
}

impl Default for HomeState {
    fn default() -> HomeState {
        HomeState { thumb_width: 240.0, filter: String::new() }
    }
}

const KEYS: [(Key, Command); 6] = [
    (Key::ArrowRight, Command::Next),
    (Key::ArrowLeft, Command::Previous),
    (Key::Space, Command::Next),
    (Key::Home, Command::First),
    (Key::End, Command::Last),
    (Key::C, Command::Clear(Layer::Slides)),
];

fn color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

/// Black or white, whichever reads better on the colour.
fn text_on(c: Rgba) -> Color32 {
    let luminance = 0.299 * f32::from(c.r) + 0.587 * f32::from(c.g) + 0.114 * f32::from(c.b);
    if luminance > 150.0 { Color32::BLACK } else { Color32::WHITE }
}

/// True when a title matches what was typed in the search box (case-insensitive, any part).
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

/// The slide after the one that was last live, if there is one.
fn next_slide(show: &Show) -> Option<(SlideRef, &Slide)> {
    let cursor = show.cursor()?;
    let next = SlideRef { slide: cursor.slide + 1, ..cursor };
    let (_, slide) = show.presentations().get(next.presentation)?.slide(next.slide)?;
    Some((next, slide))
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

    // Keys drive the show unless the operator is typing in a text box.
    if !ui.ctx().text_edit_focused() {
        ui.input(|input| {
            for (key, command) in KEYS {
                if input.key_pressed(key) {
                    commands.push(command);
                }
            }
        });
    }

    toolbar(ui, show, &mut commands);
    library(ui, show, state, &mut commands);
    live_panel(ui, show, thumb, preview, &mut commands);
    slide_area(ui, show, state, thumb, &mut commands);
    commands
}

/// The strip along the top: the app's name, the clear buttons and the output switch.
fn toolbar(ui: &mut egui::Ui, show: &Show, commands: &mut Vec<Command>) {
    egui::Panel::top("toolbar").resizable(false).frame(panel_frame(BAR, 8)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("PresenterCraft").strong().size(15.0));
            ui.add_space(16.0);
            if ui.button("Clear slide").on_hover_text("Take the words off the screen (C)").clicked() {
                commands.push(Command::Clear(Layer::Slides));
            }
            if ui.button("Clear all").on_hover_text("Take everything off the screen").clicked() {
                commands.push(Command::ClearAll);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                // One switch for the audience output: a light, a name, and what it is doing.
                let on = show.output_enabled();
                let label = if on { "Audience  ON" } else { "Audience  OFF" };
                let button = egui::Button::new(egui::RichText::new(label).strong())
                    .fill(if on { Color32::from_rgb(28, 60, 38) } else { Color32::from_rgb(70, 30, 30) })
                    .stroke(Stroke::new(1.0, if on { ON } else { OFF }));
                if ui.add(button).on_hover_text("Switch the audience output on or off").clicked() {
                    commands.push(Command::SetOutputEnabled(!on));
                }
            });
        });
    });
}

/// The left column: a search box and the songs that match it.
fn library(ui: &mut egui::Ui, show: &Show, state: &mut HomeState, commands: &mut Vec<Command>) {
    egui::Panel::left("library").resizable(false).exact_size(220.0).frame(panel_frame(SIDE, 8)).show(ui, |ui| {
        section_label(ui, "LIBRARY");
        ui.add_space(4.0);
        ui.add(egui::TextEdit::singleline(&mut state.filter).hint_text("Search songs").desired_width(f32::INFINITY));
        ui.add_space(6.0);
        let total = show.presentations().len();
        let mut shown = 0;
        egui::ScrollArea::vertical().id_salt("song list").auto_shrink([false, false]).show(ui, |ui| {
            for (index, presentation) in show.presentations().iter().enumerate() {
                if !matches_filter(&presentation.title, &state.filter) {
                    continue;
                }
                shown += 1;
                let selected = index == show.selected();
                let row = ui.add_sized(
                    [ui.available_width(), 22.0],
                    egui::Button::selectable(selected, presentation.title.as_str())
                        .right_text(egui::RichText::new(presentation.slide_count().to_string()).weak().size(11.0)),
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
}

/// The right column: what the audience sees now, what comes next, and the keys.
fn live_panel(
    ui: &mut egui::Ui,
    show: &Show,
    thumb: &mut dyn FnMut(&Slide) -> Option<(TextureId, bool)>,
    preview: Option<TextureId>,
    commands: &mut Vec<Command>,
) {
    egui::Panel::right("live").resizable(false).exact_size(300.0).frame(panel_frame(SIDE, 8)).show(ui, |ui| {
        section_label(ui, "LIVE");
        ui.add_space(4.0);
        let width = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(vec2(width, width * 9.0 / 16.0), Sense::hover());
        ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
        if let Some(texture) = preview {
            egui::Image::from_texture((texture, rect.size())).paint_at(ui, rect);
        }
        let live = show.live().is_some();
        if !show.output_enabled() {
            ui.painter().rect_filled(rect, 4.0, Color32::from_black_alpha(190));
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "Audience output is off",
                FontId::proportional(15.0),
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
        ui.add_space(6.0);
        match show.live_slide() {
            Some((presentation, _)) => {
                let at = show.live().map_or(0, |l| l.slide + 1);
                ui.label(format!("{}  ·  slide {at} of {}", presentation.title, presentation.slide_count()));
            }
            None => {
                ui.weak("Nothing on screen");
            }
        }

        ui.add_space(14.0);
        section_label(ui, "NEXT");
        ui.add_space(4.0);
        let small = vec2(width * 0.6, width * 0.6 * 9.0 / 16.0);
        let (rect, response) = ui.allocate_exact_size(small, Sense::click());
        ui.painter().rect_filled(rect, 4.0, Color32::BLACK);
        match next_slide(show) {
            Some((at, slide)) => {
                match thumb(slide) {
                    Some((texture, _)) => egui::Image::from_texture((texture, small)).paint_at(ui, rect),
                    None => ui.ctx().request_repaint(),
                }
                if response.on_hover_text("Click to show this slide").clicked() {
                    commands.push(Command::GoTo(at));
                }
            }
            None => {
                let text = if show.cursor().is_some() { "End of song" } else { "—" };
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(13.0),
                    Color32::from_gray(120),
                );
            }
        }

        ui.add_space(14.0);
        section_label(ui, "KEYS");
        ui.add_space(2.0);
        egui::Grid::new("keys").num_columns(2).spacing([12.0, 3.0]).show(ui, |ui| {
            for (key, what) in [
                ("→  Space", "Next slide"),
                ("←", "Previous slide"),
                ("Home  End", "First, last slide"),
                ("C", "Clear slide"),
            ] {
                ui.label(egui::RichText::new(key).monospace().size(12.0));
                ui.weak(what);
                ui.end_row();
            }
        });
    });
}

/// The middle: the selected song's slides as a grid of thumbnails with a group bar under each.
fn slide_area(
    ui: &mut egui::Ui,
    show: &Show,
    state: &mut HomeState,
    thumb: &mut dyn FnMut(&Slide) -> Option<(TextureId, bool)>,
    commands: &mut Vec<Command>,
) {
    egui::CentralPanel::default().frame(panel_frame(CANVAS, 0)).show(ui, |ui| {
        let Some(presentation) = show.presentations().get(show.selected()) else {
            ui.centered_and_justified(|ui| ui.weak("Open a song to see its slides."));
            return;
        };

        egui::Panel::top("song header").resizable(false).frame(panel_frame(BAR, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(presentation.title.as_str()).strong().size(15.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.weak(format!("{} slides", presentation.slide_count()));
                });
            });
        });
        egui::Panel::bottom("thumb size").resizable(false).frame(panel_frame(BAR, 6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add(egui::Slider::new(&mut state.thumb_width, 120.0..=480.0).show_value(false));
                    ui.weak("Slide size");
                });
            });
        });

        let width = state.thumb_width;
        let image_size = vec2(width, width * 9.0 / 16.0);
        let bar_height = 22.0;
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
                    painter.rect_filled(image, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, Color32::BLACK);
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
                    painter.rect_filled(bar, CornerRadius { nw: 0, ne: 0, sw: 4, se: 4 }, color32(group.color));
                    let ink = text_on(group.color);
                    painter.text(
                        bar.left_center() + vec2(8.0, 0.0),
                        Align2::LEFT_CENTER,
                        (index + 1).to_string(),
                        FontId::proportional(13.0),
                        ink,
                    );
                    painter.text(
                        bar.right_center() - vec2(8.0, 0.0),
                        Align2::RIGHT_CENTER,
                        group.name.as_str(),
                        FontId::proportional(12.0),
                        ink,
                    );

                    if show.live() == Some(here) {
                        painter.rect_stroke(rect, 4.0, Stroke::new(3.0, ACCENT), StrokeKind::Outside);
                    } else if response.hovered() {
                        painter.rect_stroke(rect, 4.0, Stroke::new(1.0, Color32::from_gray(150)), StrokeKind::Outside);
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

    use egui::{Event, Modifiers, RawInput};
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
    fn next_slide_follows_the_cursor_and_stops_at_the_end() {
        let mut show = show(3);
        assert!(next_slide(&show).is_none());
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
        assert_eq!(next_slide(&show).map(|(at, _)| at.slide), Some(1));
        show.apply(Command::Clear(Layer::Slides));
        assert_eq!(next_slide(&show).map(|(at, _)| at.slide), Some(1));
        show.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
        assert!(next_slide(&show).is_none());
    }

    #[test]
    fn group_bar_text_contrasts_with_its_colour() {
        assert_eq!(text_on(Rgba::rgb(250, 240, 200)), Color32::BLACK);
        assert_eq!(text_on(Rgba::rgb(20, 40, 120)), Color32::WHITE);
    }

    #[test]
    fn keys_map_to_commands() {
        let ctx = egui::Context::default();
        let show = show(3);
        for (key, expected) in [
            (Key::ArrowRight, Command::Next),
            (Key::ArrowLeft, Command::Previous),
            (Key::Space, Command::Next),
            (Key::Home, Command::First),
            (Key::End, Command::Last),
            (Key::C, Command::Clear(Layer::Slides)),
        ] {
            assert_eq!(frame(&ctx, &show, Some(key)).0, vec![expected], "{key:?}");
        }
        assert!(frame(&ctx, &show, Some(Key::Q)).0.is_empty());
        assert!(frame(&ctx, &show, None).0.is_empty());
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
