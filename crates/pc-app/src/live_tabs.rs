//! The tabs under the Program panel: Timers, Messages and Stage.

use std::time::Instant;

use egui::{Align, Color32, FontId, Layout, Sense, vec2};

use crate::home::HomeState;
use crate::messages::Message;
use crate::screens::Kind;
use crate::studio::{ACCENT, CARD, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, caption, segmented};
use crate::timers::{self, Timer};

/// Draws the tabs and the one that is open.
pub fn live_tabs(ui: &mut egui::Ui, state: &mut HomeState) {
    ui.add_space(4.0);
    let width = ui.available_width();
    let picked = segmented(
        ui,
        "live tabs",
        vec2((width - 8.0) / 3.0, 26.0),
        &["Timers", "Messages", "Stage"],
        Some(state.live_tab),
    );
    if let Some(tab) = picked {
        state.live_tab = tab;
    }
    ui.add_space(2.0);
    egui::ScrollArea::vertical().id_salt("live tab").auto_shrink([false, false]).show(ui, |ui| {
        ui.set_width(width);
        match state.live_tab {
            0 => timers_tab(ui, state),
            1 => messages_tab(ui, state),
            _ => stage_tab(ui, state),
        }
    });
}

/// A heading with a + at its right. Returns true when the + was clicked.
fn heading_with_add(ui: &mut egui::Ui, title: &str, hint: &str) -> bool {
    let mut add = false;
    ui.horizontal(|ui| {
        caption(ui, title);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            add = ui.add(egui::Button::new("+").frame(false)).on_hover_text(hint).clicked();
        });
    });
    add
}

/// A field for a length of time or a time of day as `HH:MM:SS`. What is typed is kept while the
/// field has the keyboard and read when it is left. Returns `Some` once it is left with
/// something that reads (`Some(None)` when it was emptied).
fn clock_field(ui: &mut egui::Ui, id: egui::Id, current: Option<u32>, empty: &str) -> Option<Option<u32>> {
    let shown = current.map(|seconds| timers::clock_text(i64::from(seconds))).unwrap_or_default();
    let mut text = ui.data_mut(|data| data.get_temp::<String>(id)).unwrap_or(shown);
    let response = ui.add(egui::TextEdit::singleline(&mut text).id(id).desired_width(96.0).hint_text(empty));
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text.clone()));
    }
    if response.lost_focus() {
        ui.data_mut(|data| data.remove::<String>(id));
        return if text.trim().is_empty() { Some(None) } else { timers::parse_clock(&text).map(Some) };
    }
    None
}

fn timers_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    if heading_with_add(ui, "TIMERS", "Add a timer") {
        let taken = |name: &str| state.timers.iter().any(|timer| timer.name == name);
        let name = std::iter::once("Timer".to_owned())
            .chain((2..).map(|n| format!("Timer {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        let timer = Timer::new(name);
        state.timer_open = Some(timer.id);
        state.timers.push(timer);
        state.timers_changed = true;
    }
    if state.timers.is_empty() {
        ui.label(egui::RichText::new("No timers. Press + to add one.").size(12.0).color(TEXT_FAINT));
        return;
    }
    let now = Instant::now();
    let day = timers::seconds_into_day();
    let mut remove = None;
    let width = ui.available_width();
    for index in 0..state.timers.len() {
        let open = state.timer_open == Some(state.timers[index].id);
        egui::Frame::new().fill(CARD).corner_radius(8).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(width - 20.0);
            let timer = &mut state.timers[index];
            let row = ui.horizontal(|ui| {
                let arrow = if open { "⌄" } else { "›" };
                if ui.add(egui::Button::new(arrow).frame(false)).on_hover_text("Settings").clicked() {
                    state.timer_open = if open { None } else { Some(timer.id) };
                }
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new(timer.name.as_str()).size(13.0).color(TEXT));
                    let (value, ink, size) = if timer.running() {
                        (timer.text(now, day), TEXT, 18.0)
                    } else if matches!(timer.kind, timers::Kind::CountdownTo { .. }) {
                        (timer.describe(), TEXT_DIM, 12.0)
                    } else {
                        (timer.text(now, day), TEXT_DIM, 15.0)
                    };
                    ui.label(egui::RichText::new(value).size(size).color(ink).monospace());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (label, hint) = if timer.running() { ("❚❚", "Pause") } else { ("▶", "Start") };
                    if ui.add(egui::Button::new(label).min_size(vec2(30.0, 26.0))).on_hover_text(hint).clicked() {
                        if timer.running() {
                            timer.stop(now);
                        } else {
                            timer.start(now);
                        }
                    }
                    if ui.add(egui::Button::new("↺").min_size(vec2(30.0, 26.0))).on_hover_text("Reset").clicked() {
                        timer.reset();
                    }
                });
            });
            row.response.context_menu(|ui| {
                if ui.button("Delete").clicked() {
                    remove = Some(index);
                    ui.close();
                }
            });
            if !open {
                return;
            }
            ui.add_space(6.0);
            let mut changed = false;
            egui::Grid::new(("timer settings", timer.id)).num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                ui.label(egui::RichText::new("Name").color(TEXT_DIM));
                changed |= ui.add(egui::TextEdit::singleline(&mut timer.name).desired_width(160.0)).changed();
                ui.end_row();
                ui.label(egui::RichText::new("Type").color(TEXT_DIM));
                let kind_name = match timer.kind {
                    timers::Kind::Countdown { .. } => "Countdown Timer",
                    timers::Kind::CountdownTo { .. } => "Countdown to Time",
                    timers::Kind::Elapsed { .. } => "Elapsed Time",
                };
                egui::ComboBox::from_id_salt(("timer type", timer.id)).selected_text(kind_name).width(160.0).show_ui(
                    ui,
                    |ui| {
                        let choices = [
                            ("Countdown Timer", timers::Kind::Countdown { seconds: 300 }),
                            ("Countdown to Time", timers::Kind::CountdownTo { hour: 11, minute: 0, second: 0 }),
                            ("Elapsed Time", timers::Kind::Elapsed { start: 0, end: None }),
                        ];
                        for (name, kind) in choices {
                            let same = std::mem::discriminant(&kind) == std::mem::discriminant(&timer.kind);
                            if ui.add(egui::Button::selectable(same, name)).clicked() && !same {
                                timer.kind = kind;
                                timer.reset();
                                changed = true;
                            }
                        }
                    },
                );
                ui.end_row();
                match timer.kind {
                    timers::Kind::Countdown { seconds } => {
                        ui.label(egui::RichText::new("Duration").color(TEXT_DIM));
                        if let Some(Some(seconds)) =
                            clock_field(ui, ui.scope_id().with(("duration", timer.id)), Some(seconds), "00:05:00")
                        {
                            timer.kind = timers::Kind::Countdown { seconds };
                            changed = true;
                        }
                        ui.end_row();
                    }
                    timers::Kind::CountdownTo { hour, minute, second } => {
                        ui.label(egui::RichText::new("Time").color(TEXT_DIM));
                        let current = u32::from(hour) * 3600 + u32::from(minute) * 60 + u32::from(second);
                        ui.horizontal(|ui| {
                            if let Some(Some(at)) =
                                clock_field(ui, ui.scope_id().with(("time", timer.id)), Some(current), "11:00:00")
                            {
                                let at = at % (24 * 3600);
                                timer.kind = timers::Kind::CountdownTo {
                                    hour: u8::try_from(at / 3600).unwrap_or(0),
                                    minute: u8::try_from(at / 60 % 60).unwrap_or(0),
                                    second: u8::try_from(at % 60).unwrap_or(0),
                                };
                                changed = true;
                            }
                            ui.label(egui::RichText::new("24-hour").size(11.0).color(TEXT_FAINT));
                        });
                        ui.end_row();
                    }
                    timers::Kind::Elapsed { start, end } => {
                        ui.label(egui::RichText::new("Start").color(TEXT_DIM));
                        if let Some(Some(start)) =
                            clock_field(ui, ui.scope_id().with(("start", timer.id)), Some(start), "00:00:00")
                        {
                            timer.kind = timers::Kind::Elapsed { start, end };
                            changed = true;
                        }
                        ui.end_row();
                        ui.label(egui::RichText::new("End").color(TEXT_DIM));
                        if let Some(end) = clock_field(ui, ui.scope_id().with(("end", timer.id)), end, "No Limit") {
                            timer.kind = timers::Kind::Elapsed { start, end };
                            changed = true;
                        }
                        ui.end_row();
                    }
                }
            });
            changed |= ui.checkbox(&mut timer.allow_overrun, "Allows Overrun").changed();
            state.timers_changed |= changed;
        });
    }
    if let Some(index) = remove {
        state.timers.remove(index);
        state.timers_changed = true;
    }
}

fn messages_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    if heading_with_add(ui, "MESSAGES", "Add a message") {
        let taken = |name: &str| state.messages.iter().any(|message| message.name == name);
        let name = (1..).map(|n| format!("New Message {n}")).find(|name| !taken(name)).unwrap_or_default();
        let message = Message::new(name);
        state.message_selected = Some(message.id);
        state.messages.push(message);
        state.messages_changed = true;
    }
    if state.messages.is_empty() {
        ui.label(
            egui::RichText::new("No messages. A message shows over the slides, such as a notice for parents.")
                .size(12.0)
                .color(TEXT_FAINT),
        );
        return;
    }
    let live = state.message_live.map(|(id, _)| id);
    let mut action: Option<(usize, &str)> = None;
    for (index, message) in state.messages.iter().enumerate() {
        let selected = state.message_selected == Some(message.id);
        let (row, response) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
        if selected {
            ui.painter().rect_filled(row, 6.0, ACCENT.gamma_multiply(0.35));
        } else if response.hovered() {
            ui.painter().rect_filled(row, 6.0, crate::studio::HOVER);
        }
        if live == Some(message.id) {
            ui.painter().circle_filled(row.left_center() + vec2(10.0, 0.0), 4.0, TALLY);
        }
        ui.painter().text(
            row.left_center() + vec2(22.0, 0.0),
            egui::Align2::LEFT_CENTER,
            message.name.as_str(),
            FontId::proportional(13.0),
            TEXT,
        );
        if response.clicked() {
            state.message_selected = Some(message.id);
        }
        response.context_menu(|ui| {
            for name in ["Duplicate", "Delete"] {
                if ui.button(name).clicked() {
                    action = Some((index, name));
                    ui.close();
                }
            }
        });
    }
    match action {
        Some((index, "Duplicate")) => {
            let copy = Message {
                id: pc_model::Uuid::new_v4(),
                name: format!("{} copy", state.messages[index].name),
                ..state.messages[index].clone()
            };
            state.message_selected = Some(copy.id);
            state.messages.insert(index + 1, copy);
            state.messages_changed = true;
        }
        Some((index, _)) => {
            let gone = state.messages.remove(index);
            if live == Some(gone.id) {
                state.message_live = None;
            }
            state.messages_changed = true;
        }
        None => {}
    }

    // The chosen message: its name, words, tokens, how it leaves, and Show.
    let Some(index) = state.messages.iter().position(|message| Some(message.id) == state.message_selected) else {
        return;
    };
    ui.add_space(8.0);
    caption(ui, "MESSAGE DETAIL");
    let timer_names: Vec<String> = state.timers.iter().map(|timer| timer.name.clone()).collect();
    let message = &mut state.messages[index];
    let mut changed = false;
    changed |= ui.add(egui::TextEdit::singleline(&mut message.name).desired_width(f32::INFINITY)).changed();
    changed |= ui
        .add(
            egui::TextEdit::multiline(&mut message.text)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text("Words to show"),
        )
        .changed();
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("add token").selected_text("Add Token…").width(130.0).show_ui(ui, |ui| {
            if ui.button("System Clock").clicked() {
                message.text.push_str(crate::messages::CLOCK_TOKEN);
                changed = true;
            }
            for name in &timer_names {
                if ui.button(format!("Timer: {name}")).clicked() {
                    message.text.push_str(&crate::messages::timer_token(name));
                    changed = true;
                }
            }
        });
        let dismiss = match message.dismiss_after {
            None => "Dismiss: Manually".to_owned(),
            Some(seconds) => format!("Dismiss: After {seconds} s"),
        };
        egui::ComboBox::from_id_salt("dismiss").selected_text(dismiss).width(140.0).show_ui(ui, |ui| {
            for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
                let label = choice.map_or("Manually".to_owned(), |seconds| format!("After {seconds} seconds"));
                if ui.add(egui::Button::selectable(message.dismiss_after == choice, label)).clicked() {
                    message.dismiss_after = choice;
                    changed = true;
                }
            }
        });
    });
    let id = message.id;
    let showing = live == Some(id);
    state.messages_changed |= changed;
    ui.add_space(4.0);
    let label = if showing { "Hide" } else { "Show" };
    let button = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
        .fill(if showing { TALLY } else { ACCENT })
        .min_size(vec2(ui.available_width(), 30.0));
    if ui.add(button).on_hover_text("Show this message over the slides on the audience screens").clicked() {
        state.message_live = if showing { None } else { Some((id, Instant::now())) };
    }
}

fn stage_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    caption(ui, "STAGE");
    let mut any = false;
    for (index, screen) in state.screens.iter().enumerate().filter(|(_, screen)| screen.kind == Kind::Stage) {
        any = true;
        let status = state.screen_status.get(index).map_or("", |(status, _)| status.as_str());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(screen.name.as_str()).size(13.0).color(TEXT));
            ui.label(egui::RichText::new(status).size(11.5).color(TEXT_DIM));
        });
    }
    if !any {
        ui.label(egui::RichText::new("No stage screens yet.").size(12.0).color(TEXT_FAINT));
    }
    if ui.button("Configure Screens…").clicked() {
        state.settings_open = true;
    }
    ui.add_space(10.0);
    caption(ui, "STAGE MESSAGE");
    let field = ui.add(
        egui::TextEdit::multiline(&mut state.stage_message)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .hint_text("A note for the people on stage"),
    );
    if field.changed() && state.stage_message.trim().is_empty() {
        state.stage_message_live = false;
    }
    ui.label(
        egui::RichText::new("Appears in a red band at the top of the stage screens.").size(11.0).color(TEXT_FAINT),
    );
    let showing = state.stage_message_live;
    let label = if showing { "Hide" } else { "Show" };
    let has_words = !state.stage_message.trim().is_empty();
    let button = egui::Button::new(egui::RichText::new(label).color(Color32::WHITE))
        .fill(if showing { TALLY } else { ACCENT })
        .min_size(vec2(ui.available_width(), 30.0));
    if ui.add_enabled(has_words || showing, button).clicked() {
        state.stage_message_live = !showing;
    }
}
