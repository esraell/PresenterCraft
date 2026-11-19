//! The Edit view's right-hand panel: three tabs (Shape, Text, Build) of settings for the
//! selected object. Controls that do nothing yet are still shown where they belong, and say so
//! when the pointer rests on them.

use egui::{Align, Align2, Color32, FontId, Frame, Layout, Rect, Sense, Stroke, StrokeKind, pos2, vec2};
use pc_model::{
    Align as TextAlign, BoxStyle, Caps, LineTransform, ObjectKind, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, Scaling, TextStyle,
    Theme, VAlign,
};
use pc_render::object_frame;

use crate::editor::MIN_SIDE;
use crate::home::{HomeState, not_built};

const PANEL: Color32 = Color32::from_rgb(29, 29, 29);
const TAB_IDLE: Color32 = Color32::from_rgb(85, 85, 85);
const BLUE: Color32 = Color32::from_rgb(47, 123, 246);
const CONTROL: Color32 = Color32::from_rgb(45, 45, 45);
const FIELD: Color32 = Color32::from_rgb(20, 20, 20);
const RULE: Color32 = Color32::from_rgb(50, 50, 50);
const INK: Color32 = Color32::from_gray(235);
const DIM: Color32 = Color32::from_gray(150);
/// Space between the panel's edge and its controls.
const SIDE: f32 = 12.0;
const ROW: f32 = 24.0;
/// The width of the panel and of the controls inside it. Rows are sized from this, not from
/// the space they find, so a row can never push the panel wider.
const PANEL_WIDTH: f32 = 300.0;
const INNER: f32 = PANEL_WIDTH - 2.0 * SIDE;
/// The width of the column a group's tick box and title sit in.
const TICK: f32 = 98.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InspectorTab {
    Shape,
    #[default]
    Text,
    Build,
}

/// What the panel remembers between frames.
#[derive(Debug, Default)]
pub struct InspectorState {
    pub tab: InspectorTab,
    /// The look taken by Copy Style, for Paste Style.
    pub copied_style: Option<TextStyle>,
    /// Keep the shape of the box when its width or height is typed.
    pub lock_shape: bool,
    /// Switches for the options that are shown but not built yet, so they can still be ticked.
    pub later: Later,
    /// What has been typed to narrow the list of fonts.
    pub font_search: String,
}

#[derive(Debug, Default)]
pub struct Later {
    shape_stroke: bool,
    shape_shadow: bool,
    feathering: bool,
    visibility: bool,
    lines_only: bool,
    scrolling: bool,
    scroll_direction: usize,
    list: bool,
    linked_text: bool,
    linked_type: usize,
}

const LINKED_TYPES: [&str; 18] = [
    "Slide Object",
    "Current Slide",
    "Next Slide",
    "Timers",
    "System Clock",
    "Stage Message",
    "Video Countdown",
    "Audio Countdown",
    "Playback Marker",
    "Timecode",
    "Group",
    "Slide Label",
    "Slide Count",
    "Presentation Name",
    "Playlist",
    "Capture",
    "Auto Advance Time",
    "Operator Notes",
];

const WEIGHTS: [(u16, &str); 9] = [
    (100, "Thin"),
    (200, "Extra Light"),
    (300, "Light"),
    (400, "Regular"),
    (500, "Medium"),
    (600, "Semibold"),
    (700, "Bold"),
    (800, "Heavy"),
    (900, "Black"),
];

fn weight_name(weight: u16) -> &'static str {
    WEIGHTS.iter().min_by_key(|(w, _)| w.abs_diff(weight)).map_or("Regular", |(_, name)| name)
}

fn to_color32(c: Rgba) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

fn to_rgba(c: Color32) -> Rgba {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Rgba { r, g, b, a }
}

/// A thin line across the whole panel, with room above and below.
fn rule(ui: &mut egui::Ui) {
    ui.add_space(9.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(rect.expand2(vec2(SIDE, 0.0)), 0.0, RULE);
    ui.add_space(9.0);
}

/// A number box with a small caption under it.
fn number(
    ui: &mut egui::Ui,
    value: &mut f32,
    width: f32,
    caption: &str,
    make: impl FnOnce(egui::DragValue) -> egui::DragValue,
) {
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        ui.add_sized([width, ROW - 2.0], make(egui::DragValue::new(value).speed(1.0).max_decimals(0)));
        if !caption.is_empty() {
            ui.allocate_ui_with_layout(vec2(width, 12.0), Layout::top_down(Align::Center), |ui| {
                ui.label(egui::RichText::new(caption).size(10.5).color(DIM));
            });
        }
    });
}

/// A row of equal segments, like one wide button cut into parts. `draw` paints segment `i`.
/// Returns the segment clicked, if any.
fn segments(
    ui: &mut egui::Ui,
    salt: &str,
    width: f32,
    count: usize,
    is_on: impl Fn(usize) -> bool,
    draw: impl Fn(&egui::Painter, usize, Rect, Color32),
) -> Option<usize> {
    let (bar, _) = ui.allocate_exact_size(vec2(width, ROW), Sense::hover());
    ui.painter().rect_filled(bar, 6.0, CONTROL);
    let each = bar.width() / count as f32;
    let mut clicked = None;
    for index in 0..count {
        let cell = Rect::from_min_size(pos2(bar.min.x + each * index as f32, bar.min.y), vec2(each, bar.height()));
        let response = ui.interact(cell, ui.scope_id().with((salt, index)), Sense::click());
        if is_on(index) {
            ui.painter().rect_filled(cell, 6.0, BLUE);
        } else if response.hovered() {
            ui.painter().rect_filled(cell.shrink(2.0), 5.0, Color32::from_gray(64));
        }
        draw(ui.painter(), index, cell, INK);
        if response.clicked() {
            clicked = Some(index);
        }
    }
    clicked
}

/// Four lines of text, drawn left, centred, right or full width.
fn text_lines(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let (w, x0) = (15.0, cell.center().x - 7.5);
    for (row, part) in [1.0_f32, 0.6, 0.85, 0.5].into_iter().enumerate() {
        let len = if which == 3 { w } else { w * part };
        let x = match which {
            0 | 3 => x0,
            1 => x0 + (w - len) / 2.0,
            _ => x0 + w - len,
        };
        let y = cell.center().y - 5.25 + 3.5 * row as f32;
        painter.line_segment([pos2(x, y), pos2(x + len, y)], Stroke::new(1.3, ink));
    }
}

/// An arrow against a line: text to the top, the middle or the bottom of its box.
fn vertical_mark(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let c = cell.center();
    let pen = Stroke::new(1.3, ink);
    let arrow = |tip: egui::Pos2, up: bool| {
        let d = if up { 1.0 } else { -1.0 };
        painter.line_segment([tip, pos2(tip.x, tip.y + 6.0 * d)], pen);
        painter.line_segment([tip, pos2(tip.x - 2.5, tip.y + 2.5 * d)], pen);
        painter.line_segment([tip, pos2(tip.x + 2.5, tip.y + 2.5 * d)], pen);
    };
    match which {
        0 => {
            painter.line_segment([pos2(c.x - 6.0, c.y - 6.0), pos2(c.x + 6.0, c.y - 6.0)], pen);
            arrow(pos2(c.x, c.y - 3.5), true);
        }
        1 => {
            painter.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], pen);
            arrow(pos2(c.x, c.y - 1.5), false);
            arrow(pos2(c.x, c.y + 1.5), true);
        }
        _ => {
            painter.line_segment([pos2(c.x - 6.0, c.y + 6.0), pos2(c.x + 6.0, c.y + 6.0)], pen);
            arrow(pos2(c.x, c.y + 3.5), false);
        }
    }
}

/// Two bars against a line: objects lined up on one side, or through the middle.
/// `which` 0..3 line up left, centre, right; 3..6 top, middle, bottom.
fn align_mark(painter: &egui::Painter, cell: Rect, which: usize, ink: Color32) {
    let c = cell.center();
    let across = which < 3;
    let side = which % 3;
    let pen = Stroke::new(1.2, ink);
    // Drawn as if lining up left/centre/right, then turned for top/middle/bottom.
    let at =
        |along: f32, other: f32| if across { pos2(c.x + along, c.y + other) } else { pos2(c.x + other, c.y + along) };
    let edge = [-7.0, 0.0, 7.0][side];
    painter.line_segment([at(edge, -7.0), at(edge, 7.0)], pen);
    for (other, len) in [(-3.0_f32, 9.0_f32), (3.0, 6.0)] {
        let (from, to) = match side {
            0 => (edge + 1.5, edge + 1.5 + len),
            1 => (-len / 2.0, len / 2.0),
            _ => (edge - 1.5 - len, edge - 1.5),
        };
        painter.rect_filled(Rect::from_two_pos(at(from, other - 1.5), at(to, other + 1.5)), 0.5, ink);
    }
}

/// A tick box with a bold title, as each group of options starts with.
fn tick(ui: &mut egui::Ui, on: &mut bool, title: &str) -> egui::Response {
    ui.checkbox(on, egui::RichText::new(title).size(13.0).strong().color(INK))
}

/// A tick box for something not built yet. It can be ticked, and says it does nothing.
fn tick_later(ui: &mut egui::Ui, on: &mut bool, title: &str) {
    tick(ui, on, title).on_hover_text(not_built(title));
}

/// A dim label on the left and a control filling the rest of the row.
/// The spacing pop-up under the gear: room between letters, between lines, and inside the
/// box's edges.
fn spacing_popup(ui: &mut egui::Ui, wanted: &mut pc_model::BoxStyle) {
    ui.set_width(250.0);
    egui::Grid::new("text spacing").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
        ui.label(egui::RichText::new("Character Spacing").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.letter_spacing).speed(0.2).range(-20.0..=100.0).max_decimals(1))
            .on_hover_text("Extra room between letters, in points at the box's font size");
        ui.end_row();
        ui.label(egui::RichText::new("Line Height").size(12.5).color(DIM));
        ui.add(egui::DragValue::new(&mut wanted.line_height).speed(0.01).range(0.5..=3.0).fixed_decimals(2))
            .on_hover_text("The distance from one line to the next, as a multiple of the font size");
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Margins").size(12.5).color(DIM));
    ui.horizontal(|ui| {
        for (margin, name) in wanted.margins.iter_mut().zip(["Left", "Top", "Right", "Bottom"]) {
            ui.vertical(|ui| {
                ui.add(egui::DragValue::new(margin).speed(1.0).range(0.0..=400.0).max_decimals(0));
                ui.label(egui::RichText::new(name).size(10.5).color(DIM));
            });
        }
    });
    ui.add_space(8.0);
    if ui.button("Reset Spacing").clicked() {
        wanted.letter_spacing = 0.0;
        wanted.line_height = pc_model::LINE_HEIGHT;
        wanted.margins = [0.0; 4];
    }
}

fn labelled(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui, f32)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(104.0, ROW), Layout::right_to_left(Align::Center), |ui| {
            ui.label(egui::RichText::new(label).size(12.5).color(DIM));
        });
        let width = ui.available_width();
        add(ui, width);
    });
}

/// A pop-up list showing its current choice.
fn choice<T: PartialEq + Copy>(ui: &mut egui::Ui, salt: &str, width: f32, value: &mut T, options: &[(T, &str)]) {
    let shown = options.iter().find(|(v, _)| v == value).map_or("", |(_, name)| name);
    egui::ComboBox::from_id_salt(salt).selected_text(shown).width(width).show_ui(ui, |ui| {
        for (option, name) in options {
            ui.selectable_value(value, *option, *name);
        }
    });
}

/// A pop-up list for something not built yet: it shows its choices and remembers the pick.
fn choice_later(ui: &mut egui::Ui, salt: &str, width: f32, picked: &mut usize, options: &[&str], what: &str) {
    egui::ComboBox::from_id_salt(salt)
        .selected_text(options.get(*picked).copied().unwrap_or_default())
        .width(width)
        .height(420.0)
        .show_ui(ui, |ui| {
            for (index, name) in options.iter().enumerate() {
                ui.selectable_value(picked, index, *name);
            }
        })
        .response
        .on_hover_text(not_built(what));
}

fn colour(ui: &mut egui::Ui, value: &mut Rgba, hint: &str) {
    let mut c = to_color32(*value);
    ui.scope(|ui| {
        ui.spacing_mut().interact_size = vec2(54.0, ROW - 2.0);
        if ui.color_edit_button_srgba(&mut c).on_hover_text(hint).changed() {
            *value = to_rgba(c);
        }
    });
}

/// The whole panel. Returns true when the operator asks to go back to the Show view.
pub fn inspector(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) -> bool {
    let mut leave = false;
    egui::Panel::right("edit inspector").resizable(false).exact_size(PANEL_WIDTH).frame(Frame::new().fill(PANEL)).show(
        ui,
        |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let (tabs, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
        let theme_slide =
            state.theme_edit.is_some() && state.editing.as_ref().is_some_and(|draft| draft.selected.is_none());
        if theme_slide {
            ui.painter().rect_filled(tabs, 0.0, BLUE);
            ui.painter().text(tabs.center(), Align2::CENTER_CENTER, "Theme Slide", FontId::proportional(12.5), INK);
        }
            let tab_width = tabs.width() / 3.0;
            let tab_names = [(InspectorTab::Shape, "Shape"), (InspectorTab::Text, "Text"), (InspectorTab::Build, "Build")];
        for (index, (tab, name)) in tab_names.into_iter().enumerate().filter(|_| !theme_slide) {
                let rect = Rect::from_min_size(
                    pos2(tabs.min.x + tab_width * index as f32, tabs.min.y),
                    vec2(tab_width, tabs.height()),
                );
                let selected = state.inspector.tab == tab;
                ui.painter().rect_filled(rect.shrink2(vec2(0.5, 0.0)), 0.0, if selected { BLUE } else { TAB_IDLE });
                ui.painter().text(rect.center(), Align2::CENTER_CENTER, name, FontId::proportional(12.5), INK);
                if ui.interact(rect, ui.scope_id().with(("inspector tab", index)), Sense::click()).clicked() {
                    state.inspector.tab = tab;
                }
            }

            egui::Panel::bottom("edit footer").resizable(false).frame(Frame::new().fill(PANEL).inner_margin(8)).show(
                ui,
                |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let for_theme = state.theme_edit.is_some();
                        if !for_theme {
                            ui.add_enabled(false, egui::Button::new("Slide Notes"))
                                .on_disabled_hover_text(not_built("Slide Notes"));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button("Show").on_hover_text("Go back to the Show view").clicked() {
                                leave = true;
                            }
                            if !for_theme {
                                ui.checkbox(&mut state.live_edit, "Live Edit").on_hover_text(
                                    "When on, changes to a slide that is on screen appear on the output as they are made",
                                );
                            }
                        });
                    });
                },
            );

            egui::ScrollArea::vertical().id_salt("inspector body").auto_shrink([false, false]).show(ui, |ui| {
                Frame::new()
                    .inner_margin(egui::Margin { left: SIDE as i8, right: SIDE as i8, top: 8, bottom: 10 })
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                        let visuals = ui.visuals_mut();
                        visuals.extreme_bg_color = FIELD;
                        visuals.widgets.inactive.weak_bg_fill = CONTROL;
                        visuals.widgets.inactive.bg_fill = CONTROL;
                        visuals.selection.bg_fill = BLUE;
                        if theme_slide {
                    theme_slide_panel(ui, state);
                } else {
                    // With several objects selected, a change made here is made to them all.
                    let before = state.editing.as_ref().and_then(|draft| draft.selected_object().cloned());
                    match state.inspector.tab {
                        InspectorTab::Shape => shape_tab(ui, state, theme),
                        InspectorTab::Text => text_tab(ui, state, theme),
                        InspectorTab::Build => build_tab(ui),
                    }
                    if let (Some(before), Some(draft)) = (before, state.editing.as_mut()) {
                        draft.share_change(theme, &before);
                    }
                }
                        if let Some(notice) = &state.notice {
                            rule(ui);
                            ui.label(
                                egui::RichText::new(notice.as_str()).size(12.0).color(Color32::from_rgb(255, 150, 120)),
                            );
                        }
                    });
            });
        },
    );
    leave
}

/// The panel for a theme as a whole, shown in the Theme Editor when no box is selected.
fn theme_slide_panel(ui: &mut egui::Ui, state: &mut HomeState) {
    let Some(edit) = state.theme_edit.as_mut() else { return };
    let before = (edit.theme.name.clone(), edit.theme.background);
    ui.add_space(4.0);
    labelled_left(ui, "Name", |ui| {
        ui.add(egui::TextEdit::singleline(&mut edit.theme.name).desired_width(f32::INFINITY));
    });
    labelled_left(ui, "Size", |ui| {
        let mut picked = 0;
        choice_later(ui, "theme size", ui.available_width(), &mut picked, &["1920 x 1080"], "Other slide sizes");
    });
    rule(ui);
    ui.horizontal(|ui| {
        let mut filled = edit.theme.background.is_some();
        if tick(ui, &mut filled, "Background").on_hover_text("A colour behind everything on the slide").changed() {
            edit.theme.background = filled.then_some(Rgba { r: 20, g: 30, b: 60, a: 255 });
        }
        if let Some(background) = edit.theme.background.as_mut() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, background, "Background colour"));
        }
    });
    rule(ui);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Media Action").size(13.0).color(INK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_enabled(false, egui::Button::new("+")).on_disabled_hover_text(not_built("Media on a theme"));
        });
    });
    rule(ui);
    let about = "A slide uses the template that suits what is on it: with the main language and a translation, the first template that has a box for both; with one language, the first that has no translation box. Bible slides use the template marked Bible.";
    ui.label(egui::RichText::new(about).size(11.5).color(DIM));
    if before != (edit.theme.name.clone(), edit.theme.background) {
        // A theme always has a name.
        if edit.theme.name.trim().is_empty() {
            edit.theme.name = before.0;
        }
        edit.changed = true;
    }
}

/// A bold label on the left and a control filling the rest of the row.
fn labelled_left(ui: &mut egui::Ui, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(44.0, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(44.0);
            ui.label(egui::RichText::new(label).size(12.5).strong().color(INK));
        });
        add(ui);
    });
}

fn nothing_selected(ui: &mut egui::Ui) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Click something on the slide to select it.").size(12.5).color(Color32::from_gray(190)),
    );
    ui.label(
        egui::RichText::new(
            "Drag to move it, drag a handle to resize it, press Delete to remove it. Arrow keys nudge.",
        )
        .size(11.5)
        .color(DIM),
    );
}

fn shape_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let for_theme = state.theme_edit.is_some();
    let later = &mut state.inspector.later;
    let lock_shape = &mut state.inspector.lock_shape;
    let Some(draft) = state.editing.as_mut() else { return };
    let themed = draft.selected_object().map(|object| object_frame(&draft.slide, object, theme));
    let (Some(frame), Some(object)) = (themed, draft.selected_object_mut()) else {
        return nothing_selected(ui);
    };
    let mut edited = frame;

    // Line the object up with the slide's sides or middle.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let each = ((INNER - 24.0) / 3.0).floor();
        let across = segments(ui, "align across", each, 3, |_| false, |p, i, cell, ink| align_mark(p, cell, i, ink));
        ui.add_space(12.0);
        let down = segments(ui, "align down", each, 3, |_| false, |p, i, cell, ink| align_mark(p, cell, i + 3, ink));
        ui.add_space(12.0);
        ui.add_enabled_ui(false, |ui| {
            segments(
                ui,
                "distribute",
                each,
                3,
                |_| false,
                |p, i, cell, _| {
                    align_mark(p, cell, [1, 4, 1][i], Color32::from_gray(95));
                },
            );
        })
        .response
        .on_hover_text(not_built("Distribute"));
        match across {
            Some(0) => edited.x = 0.0,
            Some(1) => edited.x = (SLIDE_WIDTH - edited.w) / 2.0,
            Some(_) => edited.x = SLIDE_WIDTH - edited.w,
            None => {}
        }
        match down {
            Some(0) => edited.y = 0.0,
            Some(1) => edited.y = (SLIDE_HEIGHT - edited.h) / 2.0,
            Some(_) => edited.y = SLIDE_HEIGHT - edited.h,
            None => {}
        }
    });
    rule(ui);

    let title = |ui: &mut egui::Ui, text: &str| {
        ui.allocate_ui_with_layout(vec2(72.0, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(72.0);
            ui.label(egui::RichText::new(text).size(12.5).strong().color(INK));
        });
    };
    ui.horizontal_top(|ui| {
        title(ui, "Position");
        number(ui, &mut edited.x, 84.0, "X", |d| d);
        ui.add_space(16.0);
        number(ui, &mut edited.y, 84.0, "Y", |d| d);
    });
    ui.horizontal_top(|ui| {
        title(ui, "Size");
        let shape = frame.w / frame.h.max(1.0);
        number(ui, &mut edited.w, 84.0, "Width", |d| d.range(MIN_SIDE..=10_000.0));
        let lock = ui
            .add_sized(
                [10.0, ROW - 2.0],
                egui::Button::selectable(*lock_shape, egui::RichText::new("=").size(11.0)).frame(false),
            )
            .on_hover_text("Keep the box's shape when its width or height changes");
        if lock.clicked() {
            *lock_shape = !*lock_shape;
        }
        number(ui, &mut edited.h, 84.0, "Height", |d| d.range(MIN_SIDE..=10_000.0));
        if *lock_shape {
            if edited.w != frame.w {
                edited.h = (edited.w / shape).max(MIN_SIDE);
            } else if edited.h != frame.h {
                edited.w = (edited.h * shape).max(MIN_SIDE);
            }
        }
    });
    ui.horizontal_top(|ui| {
        title(ui, "Transform");
        ui.add_enabled_ui(false, |ui| {
            segments(
                ui,
                "flip",
                84.0,
                2,
                |_| false,
                |p, i, cell, _| {
                    let c = cell.center();
                    let ink = Color32::from_gray(120);
                    let (a, b) =
                        if i == 0 { (vec2(-6.0, 4.0), vec2(6.0, 4.0)) } else { (vec2(-4.0, -6.0), vec2(-4.0, 6.0)) };
                    let tip = if i == 0 { vec2(0.0, -5.0) } else { vec2(5.0, 0.0) };
                    p.add(egui::Shape::convex_polygon(vec![c + a, c + b, c + tip], ink, Stroke::NONE));
                },
            );
        })
        .response
        .on_hover_text(not_built("Flip"));
        ui.add_space(16.0);
        number(ui, &mut object.rotation, 84.0, "Rotation", |d| {
            d.speed(0.5).range(-360.0..=360.0).suffix("°").max_decimals(1)
        });
    });
    if edited != frame {
        object.frame = Some(edited);
    }
    rule(ui);

    ui.horizontal(|ui| {
        title(ui, "Opacity");
        ui.spacing_mut().slider_width = INNER - 72.0 - 14.0;
        ui.add(egui::Slider::new(&mut object.opacity, 0.0..=1.0).show_value(false));
    });
    rule(ui);

    ui.horizontal(|ui| {
        let mut filled = object.fill.is_some();
        if tick(ui, &mut filled, "Fill").on_hover_text("Fill the box with a colour behind its text").changed() {
            object.fill = filled.then_some(Rgba { r: 0, g: 0, b: 0, a: 160 });
        }
        if let Some(fill) = object.fill.as_mut() {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, fill, "Fill colour"));
        }
    });
    rule(ui);
    tick_later(ui, &mut later.shape_stroke, "Stroke");
    rule(ui);
    tick_later(ui, &mut later.shape_shadow, "Shadow");
    rule(ui);
    tick_later(ui, &mut later.feathering, "Feathering");
    rule(ui);
    tick_later(ui, &mut later.visibility, "Visibility");
    rule(ui);

    // Not in the reference layout, but needed until objects can be managed from the slide itself.
    ui.label(egui::RichText::new("OBJECT").size(10.5).strong().color(DIM));
    ui.add(egui::TextEdit::singleline(&mut object.name).desired_width(f32::INFINITY).hint_text("Name"));
    let follows_theme =
        object.frame.is_none() && object.rotation == 0.0 && object.opacity == 1.0 && object.fill.is_none();
    ui.horizontal_wrapped(|ui| {
        let small = |label: &str| egui::Button::new(egui::RichText::new(label).size(12.0));
        if ui.add_enabled(draft.can_undo(), small("Undo")).on_hover_text("Undo the last change (Cmd+Z)").clicked() {
            draft.undo();
        }
        if ui.add_enabled(draft.can_redo(), small("Redo")).on_hover_text("Redo (Shift+Cmd+Z)").clicked() {
            draft.redo();
        }
        if ui.add(small("Forward")).on_hover_text("Move in front of the next object").clicked() {
            draft.restack(true);
        }
        if ui.add(small("Back")).on_hover_text("Move behind the previous object").clicked() {
            draft.restack(false);
        }
        if ui.add(small("Delete")).on_hover_text("Remove this object from the slide (Delete)").clicked() {
            draft.delete_selected();
        }
        if !for_theme
            && ui
                .add_enabled(!follows_theme, small("Reset to theme"))
                .on_hover_text("Put this object back where the theme has it")
                .clicked()
        {
            draft.reset_to_theme();
        }
    });
}

fn text_tab(ui: &mut egui::Ui, state: &mut HomeState, theme: &Theme) {
    let for_theme = state.theme_edit.is_some();
    let fonts = state.fonts.clone();
    let inspector = &mut state.inspector;
    let Some(draft) = state.editing.as_mut() else { return };
    let Some(object) = draft.selected_object_mut() else {
        return nothing_selected(ui);
    };
    let role = object.role.clone();
    let ObjectKind::Text { text, style } = &mut object.kind;
    let Some(base) = theme.text_style(role.as_ref(), &TextStyle::default()) else { return };

    ui.horizontal(|ui| {
        let half = (ui.available_width() - 6.0) / 2.0;
        if ui
            .add_sized([half, ROW], egui::Button::new("Copy Style"))
            .on_hover_text("Remember how this text looks")
            .clicked()
        {
            inspector.copied_style = Some(style.clone());
        }
        let paste = ui
            .add_enabled_ui(inspector.copied_style.is_some(), |ui| {
                ui.add_sized([half, ROW], egui::Button::new("Paste Style"))
            })
            .inner
            .on_hover_text("Give this text the look that was copied");
        if paste.clicked()
            && let Some(copied) = &inspector.copied_style
        {
            *style = copied.clone();
        }
    });
    ui.add_space(2.0);
    // A theme's box only ever says "Text": there are no words to type.
    if !for_theme {
        ui.add(egui::TextEdit::multiline(text).desired_rows(3).desired_width(f32::INFINITY).hint_text("Text"));
        ui.add_space(2.0);
    }

    let shown = style.over(&base);
    let mut wanted = shown.clone();
    text_look(ui, &mut wanted, &base, &fonts, inspector);
    if wanted != shown {
        *style = TextStyle::between(&base, &wanted);
    }

    if for_theme {
        return;
    }
    rule(ui);
    ui.horizontal(|ui| {
        let note = if style.is_default() { "Looks as the theme has it" } else { "Styled by hand on this slide" };
        ui.label(egui::RichText::new(note).size(11.5).color(DIM));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let reset = egui::Button::new(egui::RichText::new("Reset style").size(12.0));
            if ui
                .add_enabled(!style.is_default(), reset)
                .on_hover_text("Give this text the theme's look again")
                .clicked()
            {
                *style = TextStyle::default();
            }
        });
    });
}

/// The Text tab's controls for one look. `wanted` starts as the look the text has now.
fn text_look(
    ui: &mut egui::Ui,
    wanted: &mut BoxStyle,
    base: &BoxStyle,
    fonts: &[String],
    inspector: &mut InspectorState,
) {
    let full = ui.available_width();
    egui::ComboBox::from_id_salt("font family")
        .selected_text(wanted.font_family.as_str())
        .width(full)
        .height(360.0)
        .show_ui(ui, |ui| {
            // Type to narrow the list: the field has the keyboard as soon as the menu opens,
            // and Return takes the first font left.
            let search = ui.add(
                egui::TextEdit::singleline(&mut inspector.font_search)
                    .hint_text("Search fonts")
                    .desired_width(f32::INFINITY),
            );
            search.request_focus();
            let typed = inspector.font_search.trim().to_lowercase();
            // Fonts that start with what was typed come before those that only contain it.
            let mut found: Vec<&String> = fonts.iter().filter(|name| name.to_lowercase().starts_with(&typed)).collect();
            found.extend(fonts.iter().filter(|name| {
                let name = name.to_lowercase();
                !name.starts_with(&typed) && name.contains(&typed)
            }));
            let enter = ui.input(|input| input.key_pressed(egui::Key::Enter));
            if enter
                && !typed.is_empty()
                && let Some(first) = found.first()
            {
                wanted.font_family = (*first).clone();
                inspector.font_search.clear();
                ui.close();
            }
            ui.separator();
            if found.is_empty() {
                ui.label(egui::RichText::new("No font has that in its name.").color(DIM));
            }
            for family in found {
                if ui.selectable_label(wanted.font_family == *family, family.as_str()).clicked() {
                    wanted.font_family = family.clone();
                    inspector.font_search.clear();
                    ui.close();
                }
            }
        });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("font weight")
            .selected_text(weight_name(wanted.weight))
            .width(full - 86.0)
            .show_ui(ui, |ui| {
                for (weight, name) in WEIGHTS {
                    ui.selectable_value(&mut wanted.weight, weight, name);
                }
            });
        ui.add_sized(
            [80.0, ROW - 2.0],
            egui::DragValue::new(&mut wanted.size).speed(1.0).range(8.0..=600.0).max_decimals(0),
        )
        .on_hover_text("Size, in slide units");
    });
    ui.horizontal(|ui| {
        let on = [wanted.weight >= 600, wanted.italic, wanted.underline, wanted.strike];
        let pressed = segments(
            ui,
            "bius",
            112.0,
            4,
            |i| on[i],
            |p, i, cell, ink| {
                let font = FontId::proportional(13.5);
                let c = cell.center();
                p.text(c, Align2::CENTER_CENTER, ["B", "I", "U", "S"][i], font, ink);
                match i {
                    2 => {
                        p.line_segment([pos2(c.x - 5.0, c.y + 8.0), pos2(c.x + 5.0, c.y + 8.0)], Stroke::new(1.0, ink))
                    }
                    3 => p.line_segment([pos2(c.x - 6.0, c.y), pos2(c.x + 6.0, c.y)], Stroke::new(1.0, ink)),
                    _ => p.line_segment([c, c], Stroke::NONE),
                };
            },
        );
        match pressed {
            Some(0) => wanted.weight = if on[0] { 400 } else { 700 },
            Some(1) => wanted.italic = !wanted.italic,
            Some(2) => wanted.underline = !wanted.underline,
            Some(_) => wanted.strike = !wanted.strike,
            None => {}
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Spacing: between letters, between lines, and inside the box's edges.
            let gear = ui.add(egui::Button::new("").min_size(vec2(80.0, ROW))).on_hover_text("Spacing");
            let mark = Rect::from_center_size(gear.rect.center(), vec2(14.0, 14.0));
            crate::icons::paint(ui.painter(), crate::icons::Icon::Gear, mark, INK);
            egui::Popup::from_toggle_button_response(&gear)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| spacing_popup(ui, wanted));
        });
    });
    labelled(ui, "Capitalization", |ui, width| {
        let options = [
            (Caps::None, "None"),
            (Caps::AllCaps, "All Caps"),
            (Caps::Lowercase, "Lowercase"),
            (Caps::TitleCase, "Title Case"),
        ];
        choice(ui, "caps", width, &mut wanted.caps, &options);
    });
    rule(ui);

    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Text Color").size(13.0).color(INK));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| colour(ui, &mut wanted.fill, "Text colour"));
    });
    rule(ui);

    labelled(ui, "Scaling", |ui, width| {
        let options = [
            (Scaling::None, "None"),
            (Scaling::TextUp, "Text up"),
            (Scaling::TextDown, "Text down"),
            (Scaling::TextUpOrDown, "Text up or down"),
        ];
        choice(ui, "scaling", width, &mut wanted.scaling, &options);
    });
    labelled(ui, "Line Transform", |ui, width| {
        let options = [
            (LineTransform::None, "None"),
            (LineTransform::RemoveLineReturns, "Remove line returns"),
            (LineTransform::OneWordPerLine, "One word per line"),
            (LineTransform::OneCharacterPerLine, "One character per line"),
        ];
        choice(ui, "line transform", width, &mut wanted.line_transform, &options);
    });
    rule(ui);

    let across = [TextAlign::Left, TextAlign::Center, TextAlign::Right];
    let now = wanted.align;
    if let Some(index) = segments(
        ui,
        "align",
        full,
        4,
        |i| across.get(i) == Some(&now),
        |p, i, cell, ink| {
            text_lines(p, cell, i, if i == 3 { Color32::from_gray(110) } else { ink });
        },
    ) && let Some(align) = across.get(index)
    {
        wanted.align = *align;
    }
    let down = [VAlign::Top, VAlign::Middle, VAlign::Bottom];
    let now = wanted.valign;
    if let Some(index) =
        segments(ui, "valign", full, 3, |i| down[i] == now, |p, i, cell, ink| vertical_mark(p, cell, i, ink))
    {
        wanted.valign = down[index];
    }
    rule(ui);

    ui.horizontal_top(|ui| {
        let mut stroked = wanted.outline_width > 0.0;
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            if tick(ui, &mut stroked, "Stroke").changed() {
                wanted.outline_width = if stroked { base.outline_width.max(1.0) } else { 0.0 };
            }
        });
        if stroked {
            number(ui, &mut wanted.outline_width, 78.0, "Width", |d| d.speed(0.25).range(0.25..=60.0).max_decimals(1));
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                colour(ui, &mut wanted.outline_color, "Stroke colour")
            });
        }
    });
    rule(ui);

    ui.horizontal_top(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick(ui, &mut wanted.shadow, "Shadow");
        });
        if wanted.shadow {
            let mut percent = (wanted.shadow_look.opacity * 100.0).round();
            number(ui, &mut percent, 78.0, "Opacity", |d| d.range(0.0..=100.0).suffix("%"));
            wanted.shadow_look.opacity = percent / 100.0;
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                colour(ui, &mut wanted.shadow_look.color, "Shadow colour")
            });
        }
    });
    if wanted.shadow {
        ui.horizontal_top(|ui| {
            let each = (full - 2.0 * 12.0) / 3.0;
            let look = &mut wanted.shadow_look;
            number(ui, &mut look.angle, each, "Angle", |d| d.range(0.0..=360.0).suffix("°"));
            ui.add_space(6.0);
            number(ui, &mut look.offset, each, "Offset", |d| d.speed(0.5).range(0.0..=200.0));
            ui.add_space(6.0);
            number(ui, &mut look.blur, each, "Blur", |d| d.speed(0.5).range(0.0..=200.0));
        });
    }
    rule(ui);

    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick_later(ui, &mut inspector.later.lines_only, "Lines Only");
        });
        if inspector.later.lines_only {
            let mut picked = 0;
            choice_later(ui, "lines only", ui.available_width(), &mut picked, &["Full Width"], "Lines Only");
        }
    });
    rule(ui);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick_later(ui, &mut inspector.later.scrolling, "Scrolling");
        });
        if inspector.later.scrolling {
            let picked = inspector.later.scroll_direction;
            let width = ui.available_width();
            if let Some(index) = segments(
                ui,
                "scroll direction",
                width,
                4,
                |i| i == picked,
                |p, i, cell, ink| {
                    p.text(
                        cell.center(),
                        Align2::CENTER_CENTER,
                        ["←", "→", "↑", "↓"][i],
                        FontId::proportional(14.0),
                        ink,
                    );
                },
            ) {
                inspector.later.scroll_direction = index;
            }
        }
    });
    rule(ui);
    tick_later(ui, &mut inspector.later.list, "List");
    rule(ui);
    ui.horizontal(|ui| {
        ui.allocate_ui_with_layout(vec2(TICK, ROW), Layout::left_to_right(Align::Center), |ui| {
            ui.set_min_width(TICK);
            tick_later(ui, &mut inspector.later.linked_text, "Linked Text");
        });
        if inspector.later.linked_text {
            choice_later(
                ui,
                "linked text",
                ui.available_width(),
                &mut inspector.later.linked_type,
                &LINKED_TYPES,
                "Linked Text",
            );
        }
    });
}

fn build_tab(ui: &mut egui::Ui) {
    for name in ["Build In", "Build Out"] {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(name).size(13.0).color(INK));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled(false, egui::Button::new("Add Build...").min_size(vec2(150.0, ROW)))
                    .on_disabled_hover_text(not_built(name));
            });
        });
    }
    ui.add_space(ui.available_height().max(260.0) - 190.0);
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    ui.painter().rect_filled(bar.expand2(vec2(SIDE, 0.0)), 0.0, Color32::from_rgb(36, 36, 36));
    ui.painter().rect_stroke(bar.expand2(vec2(SIDE, 0.0)), 0.0, Stroke::new(1.0, RULE), StrokeKind::Inside);
    ui.painter().text(bar.left_center(), Align2::LEFT_CENTER, "Build Order", FontId::proportional(12.5), INK);
    ui.add_space(96.0);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Start").size(12.5).strong().color(INK));
            let mut picked = 0;
            choice_later(ui, "build start", 190.0, &mut picked, &[""], "Build order");
        });
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("Delay").size(12.5).strong().color(INK));
            let mut delay = 0.0;
            ui.add_enabled(false, egui::DragValue::new(&mut delay)).on_disabled_hover_text(not_built("Build delay"));
        });
    });
}
