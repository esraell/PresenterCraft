//! The tabs under the Program panel: Timers, Messages and Stage.

use std::time::Instant;

use egui::{Align, Color32, FontId, Layout, Sense, vec2};

use crate::home::HomeState;
use crate::messages::Message;
use crate::screens::Kind;
use crate::studio::{ACCENT, CARD, TALLY, TEXT, TEXT_DIM, TEXT_FAINT, caption, segmented};
use crate::timers::{self, Timer};

/// Draws the tabs and the one that is open.
pub fn live_tabs(ui: &mut egui::Ui, show: &pc_engine::Show, state: &mut HomeState) {
    ui.add_space(4.0);
    let width = ui.available_width();
    let picked = segmented(
        ui,
        "live tabs",
        vec2((width - 8.0) / 5.0, 26.0),
        &["Timers", "Messages", "Props", "Macros", "Stage"],
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
            2 => props_tab(ui, state),
            3 => macros_tab(ui, show, state),
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

fn props_tab(ui: &mut egui::Ui, state: &mut HomeState) {
    use crate::props::Prop;
    if heading_with_add(ui, "PROPS", "Add a prop") {
        let taken = |name: &str| state.props.iter().any(|prop| prop.name == name);
        let name = (1..).map(|n| format!("Prop {n}")).find(|name| !taken(name)).unwrap_or_default();
        let prop = Prop::new(name.clone());
        state.prop_rename = Some((prop.id, name));
        state.rename_started = true;
        state.props.push(prop);
        state.props_changed = true;
    }
    if state.props.is_empty() {
        ui.label(
            egui::RichText::new(
                "No props. A prop stays over the slides until it is taken off, such as a name in a corner.",
            )
            .size(12.0)
            .color(TEXT_FAINT),
        );
        return;
    }
    enum Ask {
        Toggle,
        Edit,
        Rename,
        Duplicate,
        AutoClear(Option<u32>),
        Delete,
    }
    let mut asked: Option<(usize, Ask)> = None;
    let mut renamed: Option<Option<(pc_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let tile = vec2(96.0, 54.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (index, prop) in state.props.iter().enumerate() {
            let live = state.props_live.iter().any(|(id, _)| *id == prop.id);
            let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
            ui.painter().rect_filled(rect, 6.0, CARD);
            if live {
                ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(2.0, TALLY), egui::StrokeKind::Outside);
            } else if response.hovered() {
                ui.painter().rect_stroke(rect, 6.0, egui::Stroke::new(1.0, TEXT_FAINT), egui::StrokeKind::Outside);
            }
            match state.prop_rename.as_mut().filter(|(id, _)| *id == prop.id) {
                Some((_, typed)) => {
                    let field = crate::studio::rename_field_at(
                        ui,
                        typed,
                        FontId::proportional(12.0),
                        TEXT,
                        ACCENT,
                        rect.left_center() + vec2(8.0, 0.0),
                        rect.max.x - 4.0,
                    );
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((prop.id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().with_clip_rect(rect.shrink(4.0)).text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        prop.name.as_str(),
                        FontId::proportional(12.0),
                        if live { TEXT } else { TEXT_DIM },
                    );
                }
            }
            if response.double_clicked() {
                asked = Some((index, Ask::Edit));
            } else if response.clicked() {
                asked = Some((index, Ask::Toggle));
            }
            response.on_hover_text("Click to show or take off; double-click to edit").context_menu(|ui| {
                if ui.button("Edit…").clicked() {
                    asked = Some((index, Ask::Edit));
                    ui.close();
                }
                if ui.button("Rename").clicked() {
                    asked = Some((index, Ask::Rename));
                    ui.close();
                }
                if ui.button("Duplicate").clicked() {
                    asked = Some((index, Ask::Duplicate));
                    ui.close();
                }
                ui.menu_button("Auto-Clear", |ui| {
                    for choice in [None, Some(5), Some(10), Some(15), Some(20), Some(30), Some(45), Some(60)] {
                        let label = choice.map_or("Never".to_owned(), |seconds| format!("After {seconds} seconds"));
                        if ui.add(egui::Button::selectable(prop.auto_clear == choice, label)).clicked() {
                            asked = Some((index, Ask::AutoClear(choice)));
                            ui.close();
                        }
                    }
                });
                ui.separator();
                if ui.button("Delete").clicked() {
                    asked = Some((index, Ask::Delete));
                    ui.close();
                }
            });
        }
    });
    state.rename_started = rename_started;
    match renamed {
        Some(Some((id, name))) => {
            if let Some(prop) = state.props.iter_mut().find(|prop| prop.id == id)
                && !name.trim().is_empty()
            {
                prop.name = name.trim().to_owned();
                state.props_changed = true;
            }
            state.prop_rename = None;
        }
        Some(None) => state.prop_rename = None,
        None => {}
    }
    if let Some((index, ask)) = asked {
        let id = state.props[index].id;
        match ask {
            Ask::Toggle => {
                if state.props_live.iter().any(|(live, _)| *live == id) {
                    state.props_live.retain(|(live, _)| *live != id);
                } else {
                    state.props_live.push((id, Instant::now()));
                }
            }
            Ask::Edit => {
                // The prop opens in the editor, as a slide of its own.
                let mut draft = crate::editor::EditDraft::of_slide(state.props[index].slide.clone(), index);
                draft.at.presentation = crate::props::PROP_SLOT;
                state.editing = Some(draft);
                state.bible_open = false;
                state.reflow = None;
            }
            Ask::Rename => {
                state.prop_rename = Some((id, state.props[index].name.clone()));
                state.rename_started = true;
            }
            Ask::Duplicate => {
                let copy = state.props[index].duplicate();
                state.props.insert(index + 1, copy);
                state.props_changed = true;
            }
            Ask::AutoClear(choice) => {
                state.props[index].auto_clear = choice;
                state.props_changed = true;
            }
            Ask::Delete => {
                state.props.remove(index);
                state.props_live.retain(|(live, _)| *live != id);
                state.props_changed = true;
            }
        }
    }
    ui.add_space(8.0);
    let clear = ui.add_enabled(!state.props_live.is_empty(), egui::Button::new("Clear Props"));
    if clear.on_hover_text("Take every prop off (F4)").clicked() {
        state.props_live.clear();
    }
}

/// The songs in the library, by key and title, for actions that go to a song.
pub fn song_list(show: &pc_engine::Show, state: &HomeState) -> Vec<(String, String)> {
    let hidden = state.bible.slots.unwrap_or([usize::MAX; 2]);
    state
        .song_keys
        .iter()
        .enumerate()
        .filter(|(index, _)| !hidden.contains(index) && !state.removed_songs.contains(index))
        .filter_map(|(index, key)| Some((key.clone(), show.presentations().get(index)?.title.clone())))
        .collect()
}

/// An action in words.
pub fn describe_action(state: &HomeState, songs: &[(String, String)], action: &pc_model::ShowAction) -> String {
    crate::macros::describe(
        action,
        |id| state.messages.iter().find(|m| m.id == id).map(|m| m.name.clone()),
        |id| state.props.iter().find(|p| p.id == id).map(|p| p.name.clone()),
        |id| state.timers.iter().find(|t| t.id == id).map(|t| t.name.clone()),
        |key| songs.iter().find(|(have, _)| have == key).map(|(_, title)| title.clone()),
    )
}

/// The menu of actions to add: clearing, messages, props, timers, the audience screens, songs.
/// Returns the one chosen.
pub fn add_action_menu(
    ui: &mut egui::Ui,
    state: &HomeState,
    songs: &[(String, String)],
) -> Option<pc_model::ShowAction> {
    use pc_model::{ClearWhat, ShowAction, TimerDo};
    let mut chosen = None;
    ui.menu_button("Clear", |ui| {
        for what in ClearWhat::ALL {
            if ui.button(what.name()).clicked() {
                chosen = Some(ShowAction::Clear { what });
            }
        }
    });
    ui.menu_button("Message", |ui| {
        if state.messages.is_empty() {
            ui.label("No messages yet");
        }
        for message in &state.messages {
            for show in [true, false] {
                let label = format!("{} {}", if show { "Show" } else { "Hide" }, message.name);
                if ui.button(label).clicked() {
                    chosen = Some(ShowAction::Message { id: message.id, show });
                }
            }
        }
    });
    ui.menu_button("Prop", |ui| {
        if state.props.is_empty() {
            ui.label("No props yet");
        }
        for prop in &state.props {
            for show in [true, false] {
                let label = format!("{} {}", if show { "Show" } else { "Hide" }, prop.name);
                if ui.button(label).clicked() {
                    chosen = Some(ShowAction::Prop { id: prop.id, show });
                }
            }
        }
    });
    ui.menu_button("Timer", |ui| {
        if state.timers.is_empty() {
            ui.label("No timers yet");
        }
        for timer in &state.timers {
            for act in TimerDo::ALL {
                if ui.button(format!("{} {}", act.name(), timer.name)).clicked() {
                    chosen = Some(ShowAction::Timer { id: timer.id, act });
                }
            }
        }
    });
    ui.menu_button("Audience Screens", |ui| {
        for on in [true, false] {
            if ui.button(if on { "On" } else { "Off" }).clicked() {
                chosen = Some(ShowAction::Audience { on });
            }
        }
    });
    ui.menu_button("Go to Song", |ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for (key, title) in songs {
                if ui.button(title.as_str()).clicked() {
                    chosen = Some(ShowAction::Song { key: key.clone() });
                }
            }
        });
    });
    chosen
}

fn macros_tab(ui: &mut egui::Ui, show: &pc_engine::Show, state: &mut HomeState) {
    use crate::macros::Macro;
    if heading_with_add(ui, "MACROS", "Add a macro") {
        let taken = |name: &str| state.macros.iter().any(|m| m.name == name);
        let name = (1..).map(|n| format!("Macro {n}")).find(|name| !taken(name)).unwrap_or_default();
        let made = Macro::new(name.clone());
        state.macro_rename = Some((made.id, name));
        state.rename_started = true;
        state.macros.push(made);
        state.macros_changed = true;
    }
    if state.macros.is_empty() {
        ui.label(
            egui::RichText::new(
                "No macros. A macro does several things at once, such as clearing the screen and starting a timer.",
            )
            .size(12.0)
            .color(TEXT_FAINT),
        );
        return;
    }
    let songs = song_list(show, state);
    enum Ask {
        Run,
        Rename,
        Color(pc_model::Rgba),
        Add(pc_model::ShowAction),
        Remove(usize),
        Startup,
        Duplicate,
        Delete,
    }
    let mut asked: Option<(usize, Ask)> = None;
    let mut renamed: Option<Option<(pc_model::Uuid, String)>> = None;
    let mut rename_started = std::mem::take(&mut state.rename_started);
    let tile = vec2(96.0, 60.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
        for (index, made) in state.macros.iter().enumerate() {
            let (rect, response) = ui.allocate_exact_size(tile, Sense::click());
            let color = crate::home::color32(made.color);
            ui.painter().rect_filled(rect, 8.0, if response.hovered() { color } else { color.gamma_multiply(0.85) });
            match state.macro_rename.as_mut().filter(|(id, _)| *id == made.id) {
                Some((_, typed)) => {
                    let field = crate::studio::rename_field_at(
                        ui,
                        typed,
                        FontId::proportional(12.0),
                        Color32::WHITE,
                        Color32::WHITE,
                        rect.left_center() + vec2(8.0, 0.0),
                        rect.max.x - 4.0,
                    );
                    if std::mem::take(&mut rename_started) {
                        field.request_focus();
                    }
                    let (enter, escape) =
                        ui.input(|input| (input.key_pressed(egui::Key::Enter), input.key_pressed(egui::Key::Escape)));
                    if escape {
                        renamed = Some(None);
                    } else if enter || field.lost_focus() {
                        renamed = Some(Some((made.id, typed.clone())));
                    }
                }
                None => {
                    ui.painter().with_clip_rect(rect.shrink(4.0)).text(
                        rect.center() - vec2(0.0, 6.0),
                        egui::Align2::CENTER_CENTER,
                        made.name.as_str(),
                        FontId::proportional(12.5),
                        Color32::WHITE,
                    );
                    let count = made.actions.len();
                    let note = if count == 1 { "1 action".to_owned() } else { format!("{count} actions") };
                    ui.painter().text(
                        rect.center() + vec2(0.0, 12.0),
                        egui::Align2::CENTER_CENTER,
                        note,
                        FontId::proportional(10.5),
                        Color32::from_white_alpha(200),
                    );
                }
            }
            let summary: Vec<String> =
                made.actions.iter().map(|action| describe_action(state, &songs, action)).collect();
            let hint = if summary.is_empty() {
                "No actions yet: right-click to add one".to_owned()
            } else {
                summary.join("\n")
            };
            if response.clicked() {
                asked = Some((index, Ask::Run));
            }
            response.on_hover_text(hint).context_menu(|ui| {
                ui.menu_button("Add Action", |ui| {
                    if let Some(action) = add_action_menu(ui, state, &songs) {
                        asked = Some((index, Ask::Add(action)));
                        ui.close();
                    }
                });
                if !made.actions.is_empty() {
                    ui.menu_button("Remove Action", |ui| {
                        for (at, words) in summary.iter().enumerate() {
                            if ui.button(words.as_str()).clicked() {
                                asked = Some((index, Ask::Remove(at)));
                                ui.close();
                            }
                        }
                    });
                }
                ui.separator();
                if ui.button("Rename").clicked() {
                    asked = Some((index, Ask::Rename));
                    ui.close();
                }
                ui.menu_button("Change Color", |ui| {
                    for (name, color) in crate::macros::COLORS {
                        if ui.add(egui::Button::selectable(made.color == color, name)).clicked() {
                            asked = Some((index, Ask::Color(color)));
                            ui.close();
                        }
                    }
                });
                if ui.add(egui::Button::selectable(made.on_startup, "Trigger on Startup")).clicked() {
                    asked = Some((index, Ask::Startup));
                    ui.close();
                }
                ui.separator();
                if ui.button("Duplicate").clicked() {
                    asked = Some((index, Ask::Duplicate));
                    ui.close();
                }
                if ui.button("Delete").clicked() {
                    asked = Some((index, Ask::Delete));
                    ui.close();
                }
            });
        }
    });
    state.rename_started = rename_started;
    match renamed {
        Some(Some((id, name))) => {
            if let Some(made) = state.macros.iter_mut().find(|m| m.id == id)
                && !name.trim().is_empty()
            {
                made.name = name.trim().to_owned();
                state.macros_changed = true;
            }
            state.macro_rename = None;
        }
        Some(None) => state.macro_rename = None,
        None => {}
    }
    if let Some((index, ask)) = asked {
        match ask {
            Ask::Run => state.actions_to_run.extend(state.macros[index].actions.iter().cloned()),
            Ask::Rename => {
                state.macro_rename = Some((state.macros[index].id, state.macros[index].name.clone()));
                state.rename_started = true;
            }
            Ask::Color(color) => state.macros[index].color = color,
            Ask::Add(action) => state.macros[index].actions.push(action),
            Ask::Remove(at) => {
                state.macros[index].actions.remove(at);
            }
            Ask::Startup => state.macros[index].on_startup = !state.macros[index].on_startup,
            Ask::Duplicate => {
                let copy = state.macros[index].duplicate();
                state.macros.insert(index + 1, copy);
            }
            Ask::Delete => {
                state.macros.remove(index);
            }
        }
        state.macros_changed = true;
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
