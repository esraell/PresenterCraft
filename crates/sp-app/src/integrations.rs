//! Settings › Integrations: the services SoftPresenter works with, each a row with its state,
//! opening a page of its own to connect it. VoxLive is the first.

use egui::{Align, Align2, Color32, FontId, Layout, Rect, Sense, Stroke, vec2};

use crate::studio::{CARD, HOVER, LINE, TEXT, TEXT_DIM, TEXT_FAINT};
use crate::voxlive::{Status, View};

/// VoxLive's own colour, for its mark; and its lighter shade, for words on a dark ground.
pub const VOXLIVE: Color32 = Color32::from_rgb(44, 93, 124);
pub const VOXLIVE_LIGHT: Color32 = Color32::from_rgb(127, 169, 198);

/// The green of something connected.
const GOOD: Color32 = Color32::from_rgb(52, 199, 89);

/// What the Integrations pane asks of the app.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    /// Take this key, and check it.
    Connect(String),
    /// Forget the key here.
    Disconnect,
    /// Find the screens again now.
    Refresh,
}

/// The pane: the list of services, or the page of the one open.
pub fn pane(ui: &mut egui::Ui, view: &mut View) {
    if view.open {
        voxlive_page(ui, view);
    } else {
        list(ui, view);
    }
}

/// A service's mark: a rounded square of its colour with its initial.
fn mark(ui: &mut egui::Ui, size: f32, colour: Color32, letter: &str) {
    let (place, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(place, size * 0.24, colour);
    ui.painter().text(
        place.center(),
        Align2::CENTER_CENTER,
        letter,
        crate::studio::semibold(size * 0.52),
        Color32::WHITE,
    );
}

/// A dot and a word or two for how a service stands.
fn state_words(status: &Status) -> (Color32, String) {
    match status {
        Status::Connected => (GOOD, "Connected".to_owned()),
        Status::Checking => (TEXT_FAINT, "Connecting…".to_owned()),
        Status::NoKey => (TEXT_FAINT, "Not connected".to_owned()),
        Status::Refused(_) => (crate::studio::TALLY, "Key refused".to_owned()),
        Status::Trouble(_) => (Color32::from_rgb(230, 170, 60), "Can't reach it".to_owned()),
    }
}

fn list(ui: &mut egui::Ui, view: &mut View) {
    ui.label(egui::RichText::new("Services SoftPresenter works with. Click one to connect it.").color(TEXT_DIM));
    ui.add_space(12.0);
    let width = ui.available_width().min(560.0);
    let (row, response) = ui.allocate_exact_size(vec2(width, 60.0), Sense::click());
    ui.painter().rect_filled(row, 10.0, if response.hovered() { HOVER } else { CARD });
    let mut inside = ui.new_child(
        egui::UiBuilder::new().max_rect(row.shrink2(vec2(14.0, 10.0))).layout(Layout::left_to_right(Align::Center)),
    );
    mark(&mut inside, 36.0, VOXLIVE, "V");
    inside.add_space(10.0);
    inside.vertical(|ui| {
        ui.add_space(2.0);
        ui.label(egui::RichText::new("VoxLive").font(crate::studio::semibold(14.0)).color(TEXT));
        ui.label(
            egui::RichText::new("Live translation of the sermon, as captions on your screens")
                .size(11.5)
                .color(TEXT_DIM),
        );
    });
    let (dot, words) = state_words(&view.status);
    let right = Rect::from_min_max(row.right_top() - vec2(170.0, 0.0), row.max - vec2(14.0, 0.0));
    ui.painter().text(
        right.right_center() - vec2(0.0, 0.0),
        Align2::RIGHT_CENTER,
        "›",
        FontId::proportional(18.0),
        TEXT_FAINT,
    );
    let text = ui.painter().text(
        right.right_center() - vec2(18.0, 0.0),
        Align2::RIGHT_CENTER,
        &words,
        FontId::proportional(12.0),
        TEXT_DIM,
    );
    ui.painter().circle_filled(text.left_center() - vec2(9.0, 0.0), 3.5, dot);
    if response.on_hover_text("Connect VoxLive, or see its screens").clicked() {
        view.open = true;
    }
}

fn voxlive_page(ui: &mut egui::Ui, view: &mut View) {
    if ui.add(egui::Button::new(egui::RichText::new("‹ Integrations").color(TEXT_DIM)).frame(false)).clicked() {
        view.open = false;
        view.typed.clear();
        view.problem = None;
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        mark(ui, 44.0, VOXLIVE, "V");
        ui.add_space(6.0);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new("VoxLive").font(crate::studio::semibold(18.0)).color(TEXT));
            let (dot, words) = state_words(&view.status);
            ui.horizontal(|ui| {
                let (place, _) = ui.allocate_exact_size(vec2(8.0, 14.0), Sense::hover());
                ui.painter().circle_filled(place.center(), 3.5, dot);
                ui.label(egui::RichText::new(words).size(12.0).color(TEXT_DIM));
            });
        });
    });
    ui.add_space(10.0);
    let width = ui.available_width().min(560.0);
    ui.set_max_width(width);
    ui.label(
        egui::RichText::new(
            "VoxLive translates the sermon as it is preached, and shows it as captions on its screens. Connected, \
             SoftPresenter shows your church's VoxLive screens as the slides of a VoxLive item (under LIBRARY, and \
             in playlists) and in the VoxLive tab beside the screens: one click puts the captions on screen in place \
             of a verse, and a click on the verse brings it back.",
        )
        .color(TEXT_DIM),
    );
    ui.add_space(12.0);
    match (&view.status, &view.key_hint) {
        (Status::NoKey, _) | (_, None) => connect_form(ui, view),
        (status, Some(hint)) => {
            if let Status::Refused(why) | Status::Trouble(why) = status {
                ui.label(egui::RichText::new(why.as_str()).color(crate::studio::TALLY));
                ui.add_space(6.0);
            }
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Key").color(TEXT_DIM));
                ui.label(egui::RichText::new(hint.as_str()).monospace().color(TEXT));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .button("Disconnect")
                        .on_hover_text(
                            "Forget the key on this computer (it stays in VoxLive until you revoke it there)",
                        )
                        .clicked()
                    {
                        view.request = Some(Request::Disconnect);
                    }
                    if ui.button("Find Screens Again").clicked() {
                        view.request = Some(Request::Refresh);
                    }
                });
            });
            if matches!(status, Status::Refused(_)) {
                ui.add_space(8.0);
                connect_form(ui, view);
            }
            ui.add_space(14.0);
            screens(ui, view);
        }
    }
}

/// Where the key is pasted, and how to get one.
fn connect_form(ui: &mut egui::Ui, view: &mut View) {
    crate::studio::caption(ui, "CONNECT");
    ui.label(
        egui::RichText::new(
            "In VoxLive, open Settings › API keys and make a key with the screens permissions (read and write). \
             Paste it here. It is kept in this Mac's Keychain and sent only to VoxLive.",
        )
        .size(12.0)
        .color(TEXT_DIM),
    );
    ui.add_space(6.0);
    let mut connect = false;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut view.typed)
                .password(true)
                .hint_text("vxk_live_…")
                .desired_width(ui.available_width() - 100.0),
        );
        connect |= field.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter));
        connect |= ui.add_enabled(!view.typed.trim().is_empty(), egui::Button::new("Connect")).clicked();
    });
    if connect && !view.typed.trim().is_empty() {
        view.request = Some(Request::Connect(std::mem::take(&mut view.typed)));
    }
    if let Some(problem) = &view.problem {
        ui.label(egui::RichText::new(problem.as_str()).size(12.0).color(crate::studio::TALLY));
    }
}

/// The church's VoxLive screens, as last found.
fn screens(ui: &mut egui::Ui, view: &View) {
    crate::studio::caption(ui, "YOUR VOXLIVE SCREENS");
    if view.screens.is_empty() {
        let words = match view.status {
            Status::Connected => "No screens yet. Make one in VoxLive (Screens), then Find Screens Again.",
            _ => "Not found yet.",
        };
        ui.label(egui::RichText::new(words).size(12.0).color(TEXT_FAINT));
        return;
    }
    for screen in &view.screens {
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
        ui.painter().text(
            row.left_center() + vec2(4.0, 0.0),
            Align2::LEFT_CENTER,
            screen.label(),
            FontId::proportional(13.0),
            TEXT,
        );
        let kind = match screen.kind() {
            "captions" => "Captions",
            other => other,
        };
        ui.painter().text(
            row.right_center() - vec2(4.0, 0.0),
            Align2::RIGHT_CENTER,
            kind,
            FontId::proportional(11.5),
            VOXLIVE_LIGHT,
        );
        ui.painter().hline(row.x_range(), row.max.y, Stroke::new(1.0, LINE));
    }
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(
            "A screen's live display is opened only on your real screens, where it shows the captions (and can \
             start that language's translation); previews use its free test pattern.",
        )
        .size(11.5)
        .color(TEXT_FAINT),
    );
}
