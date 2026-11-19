//! What a stage screen shows, in a layout of its own: the words on screen now, the next
//! slide's words, the time, a timer, the slide's notes, the song's title and group, and the
//! stage message. Layouts are kept in `stage-layouts.json`; each stage screen picks one.

use sp_model::{Align, BoxStyle, Object, Rgba, Role, Slide, SlideRect, Theme, ThemeTemplate, Uuid, VAlign};

const NOW: &str = "now";
const NEXT_LABEL: &str = "next-label";
const NEXT: &str = "next";
const CLOCK: &str = "clock";
const TIMER: &str = "timer";
const MESSAGE: &str = "stage-message";
const NOTES: &str = "notes";
const TITLE: &str = "song-title";
const GROUP: &str = "group";

/// What a stage layout can show: each item's role, its name, and words to stand in for it in
/// the editor.
pub const ITEMS: [(&str, &str, &str); 9] = [
    (NOW, "Current Slide", "The words on screen now"),
    (NEXT, "Next Slide", "The next slide's words"),
    (NEXT_LABEL, "Next Label", "NEXT"),
    (CLOCK, "Clock", "10:42"),
    (TIMER, "Timer", "Sermon  00:24:10"),
    (NOTES, "Slide Notes", "Notes for the slide on screen"),
    (TITLE, "Song Title", "Song Title"),
    (GROUP, "Group", "Chorus"),
    (MESSAGE, "Stage Message", "A note for the stage"),
];

/// An item's name, by its role.
pub fn item_name(role: &Role) -> &'static str {
    ITEMS.iter().find(|(have, _, _)| *have == role.0).map_or("Item", |(_, name, _)| name)
}

/// An item's stand-in words for the editor, by its role.
pub fn item_sample(role: &Role) -> &'static str {
    ITEMS.iter().find(|(have, _, _)| *have == role.0).map_or("", |(_, _, sample)| sample)
}

/// A box for a stage item, in its place, at its size and colour.
pub fn item_box(role: &str, rect: SlideRect, size: f32, fill: Rgba, align: Align, valign: VAlign) -> BoxStyle {
    BoxStyle {
        role: Role(role.to_owned()),
        name: None,
        rect,
        size,
        min_size: (size * 0.4).max(18.0),
        fill,
        align,
        valign,
        outline_width: 0.0,
        shadow: false,
        ..base_box()
    }
}

/// The layouts SoftPresenter comes with.
pub fn builtin_layouts() -> Vec<ThemeTemplate> {
    let id = |n: u128| Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0000 + n);
    let rect = |x: f32, y: f32, w: f32, h: f32| SlideRect { x, y, w, h };
    let (white, grey, yellow, green) =
        (Rgba::WHITE, Rgba::rgb(140, 140, 140), Rgba::rgb(255, 200, 60), Rgba::rgb(120, 220, 140));
    let clock = || item_box(CLOCK, rect(1360.0, 24.0, 520.0, 90.0), 64.0, white, Align::Right, VAlign::Middle);
    let timer = || item_box(TIMER, rect(60.0, 24.0, 900.0, 90.0), 64.0, green, Align::Left, VAlign::Middle);
    let message = || item_box(MESSAGE, rect(0.0, 120.0, 1920.0, 110.0), 64.0, white, Align::Center, VAlign::Middle);
    vec![
        ThemeTemplate {
            objects: Vec::new(),
            id: id(2),
            name: "Words and Next".to_owned(),
            boxes: vec![
                item_box(NOW, rect(60.0, 130.0, 1800.0, 560.0), 110.0, white, Align::Center, VAlign::Middle),
                BoxStyle {
                    weight: 600,
                    ..item_box(NEXT_LABEL, rect(60.0, 712.0, 600.0, 50.0), 34.0, grey, Align::Left, VAlign::Middle)
                },
                item_box(NEXT, rect(60.0, 770.0, 1800.0, 280.0), 64.0, yellow, Align::Center, VAlign::Top),
                clock(),
                timer(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            id: id(3),
            name: "Words Only".to_owned(),
            boxes: vec![
                item_box(NOW, rect(60.0, 130.0, 1800.0, 900.0), 140.0, white, Align::Center, VAlign::Middle),
                clock(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            id: id(4),
            name: "Words and Notes".to_owned(),
            boxes: vec![
                item_box(GROUP, rect(60.0, 24.0, 900.0, 90.0), 56.0, grey, Align::Left, VAlign::Middle),
                item_box(NOW, rect(60.0, 130.0, 1800.0, 520.0), 100.0, white, Align::Center, VAlign::Middle),
                item_box(NOTES, rect(60.0, 680.0, 1800.0, 370.0), 56.0, yellow, Align::Left, VAlign::Top),
                clock(),
                message(),
            ],
        },
        ThemeTemplate {
            objects: Vec::new(),
            id: id(5),
            name: "Clock and Timer".to_owned(),
            boxes: vec![
                item_box(TITLE, rect(60.0, 24.0, 1800.0, 90.0), 56.0, grey, Align::Center, VAlign::Middle),
                item_box(CLOCK, rect(60.0, 220.0, 1800.0, 380.0), 300.0, white, Align::Center, VAlign::Middle),
                item_box(TIMER, rect(60.0, 640.0, 1800.0, 300.0), 160.0, green, Align::Center, VAlign::Middle),
                message(),
            ],
        },
    ]
}

/// A layout as a theme, to draw a stage screen with.
pub fn theme_for(layout: &ThemeTemplate) -> Theme {
    Theme {
        id: Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0001),
        name: "Stage".to_owned(),
        templates: vec![layout.clone()],
        background: None,
        background_picture: None,
    }
}

/// The layout a stage screen asks for, or the first one if it asks for none (or one deleted).
pub fn layout_for(layouts: &[ThemeTemplate], wanted: Option<Uuid>) -> ThemeTemplate {
    wanted
        .and_then(|id| layouts.iter().find(|layout| layout.id == id))
        .or(layouts.first())
        .cloned()
        .unwrap_or_else(|| builtin_layouts().remove(0))
}

/// A new layout, laid out like the first built-in one, with ids of its own.
pub fn new_layout(name: String) -> ThemeTemplate {
    ThemeTemplate { id: Uuid::new_v4(), name, ..builtin_layouts().remove(0) }
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("stage-layouts.json"))
}

/// The stage layouts: the ones saved, or the built-in ones if none are.
pub fn load_layouts() -> Vec<ThemeTemplate> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Vec<ThemeTemplate>>(&text).ok())
        .filter(|layouts| !layouts.is_empty())
        .unwrap_or_else(builtin_layouts)
}

pub fn save_layouts(layouts: &[ThemeTemplate]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(layouts).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the stage layouts: {e}"))
}

/// What the stage screens are told about the song on screen besides its slides.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct About {
    pub title: Option<String>,
    pub group: Option<String>,
}

/// The look every stage box starts from: bold, white, no outline or shadow.
fn base_box() -> BoxStyle {
    BoxStyle {
        role: Role::primary(),
        name: None,
        rect: SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 },
        font_family: "Helvetica Neue".to_owned(),
        weight: 700,
        italic: false,
        size: 96.0,
        min_size: 30.0,
        fill: Rgba::WHITE,
        align: Align::Center,
        outline_width: 0.0,
        outline_color: Rgba::BLACK,
        shadow: false,
        shadow_look: sp_model::Shadow::default(),
        valign: VAlign::Middle,
        caps: sp_model::Caps::None,
        underline: false,
        strike: false,
        scaling: sp_model::Scaling::TextDown,
        line_transform: sp_model::LineTransform::None,
        line_height: sp_model::LINE_HEIGHT,
        letter_spacing: 0.0,
        margins: [0.0; 4],
        line_fill: sp_model::LineFill::default(),
        list: Default::default(),
        scroll: Default::default(),
        paragraph_spacing: 0.0,
        baseline: 0.0,
        gradient: sp_model::Gradient::default(),
    }
}

/// All the words on a slide, each box's on its own lines.
fn words(slide: &Slide) -> String {
    slide
        .texts()
        .into_iter()
        .map(|(_, text)| text)
        .filter(|text| !text.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// The stage screen's picture as a slide: the time, the words on screen now, the next slide's
/// words, the first running timer, the slide's notes, the song's title and group, and the stage
/// message if one is showing: those of them the layout has a box for.
pub fn slide(
    now: Option<&Slide>,
    next: Option<&Slide>,
    overlay: &crate::output::Overlay,
    about: &About,
    layout: &ThemeTemplate,
) -> Slide {
    let time = chrono::Local::now().format("%-I:%M").to_string();
    let mut objects = vec![Object::text(Role(CLOCK.to_owned()), time)];
    let instant = std::time::Instant::now();
    if let Some(timer) = overlay.timers.iter().find(|timer| timer.running()) {
        let value = timer.text(instant, crate::timers::seconds_into_day());
        objects.push(Object::text(Role(TIMER.to_owned()), format!("{}  {value}", timer.name)));
    }
    if let Some(now) = now {
        objects.push(Object::text(Role(NOW.to_owned()), words(now)));
        if let Some(notes) = now.notes.as_deref().filter(|notes| !notes.trim().is_empty()) {
            objects.push(Object::text(Role(NOTES.to_owned()), notes));
        }
    }
    if let Some(title) = about.title.as_deref() {
        objects.push(Object::text(Role(TITLE.to_owned()), title));
    }
    if let Some(group) = about.group.as_deref().filter(|group| !group.trim().is_empty()) {
        objects.push(Object::text(Role(GROUP.to_owned()), group));
    }
    if let Some(next) = next {
        objects.push(Object::text(Role(NEXT_LABEL.to_owned()), "NEXT"));
        objects.push(Object::text(Role(NEXT.to_owned()), words(next)));
    }
    if let Some(message) = overlay.stage_message.as_deref().filter(|message| !message.trim().is_empty()) {
        let mut banner = Object::text(Role(MESSAGE.to_owned()), message);
        banner.fill = Some(Rgba::rgb(190, 30, 30));
        objects.push(banner);
    }
    // An item the layout has no box for would be drawn in its first box, over what is there.
    objects.retain(|object| object.role.as_ref().is_some_and(|role| layout.style_for(role).is_some()));
    Slide { template: Some(layout.id), ..Slide::new(objects) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws a stage screen and saves it where `STAGE_PNG` says, to look at. Run by hand:
    /// `STAGE_PNG=/tmp/stage.png cargo test -p sp-app stage_picture -- --ignored`.
    #[test]
    #[ignore = "needs a GPU, and is for looking at"]
    fn stage_picture() {
        use sp_render::vello::util::RenderContext;
        use sp_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};
        let Ok(path) = std::env::var("STAGE_PNG") else { return };
        let song = sp_model::parse_song_text(
            "S",
            "We lift our voices, we lift our hands\nWe sing together across the lands\n--\nМы поднимаем голоса и руки\n\nOne song, one heart\n--\nОдна песня, одно сердце\n",
        );
        let built = slide(
            song.slide(0).map(|(_, s)| s),
            song.slide(1).map(|(_, s)| s),
            &crate::output::Overlay::default(),
            &About::default(),
            &builtin_layouts()[0],
        );
        let (w, h) = (1920, 1080);
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let scene = build_scene(&mut Fonts::new(), Some(&built), &theme_for(&builtin_layouts()[0]), w, h).scene;
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Painter::new(device).expect("painter").paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        let mut png = png::Encoder::new(std::fs::File::create(path).expect("file"), w, h);
        png.set_color(png::ColorType::Rgba);
        png.set_depth(png::BitDepth::Eight);
        png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).expect("png");
    }

    #[test]
    fn a_stage_screen_shows_the_time_the_words_now_and_the_next_words() {
        let song = sp_model::parse_song_text("S", "One\n--\nОдин\n\nTwo\n");
        let (now, next) = (song.slide(0).unwrap().1, song.slide(1).unwrap().1);
        let none = About::default();
        let layout = &builtin_layouts()[0];
        let built = slide(Some(now), Some(next), &crate::output::Overlay::default(), &none, layout);
        let text = |role: &str| {
            built
                .objects
                .iter()
                .find(|o| o.role == Some(Role(role.to_owned())))
                .and_then(|o| o.as_text())
                .map(str::to_owned)
        };
        assert_eq!(text(NOW).as_deref(), Some("One\nОдин"));
        assert_eq!(text(NEXT).as_deref(), Some("Two"));
        assert!(text(CLOCK).is_some_and(|time| time.contains(':')));
        // Nothing on screen and nothing next: only the time.
        assert_eq!(slide(None, None, &crate::output::Overlay::default(), &none, layout).objects.len(), 1);
        // A running timer and a stage message show too.
        let mut timer = crate::timers::Timer::new("Sermon".to_owned());
        timer.start(std::time::Instant::now());
        let overlay = crate::output::Overlay {
            timers: vec![timer],
            stage_message: Some("Wrap up".to_owned()),
            ..crate::output::Overlay::default()
        };
        let with = slide(None, None, &overlay, &none, layout);
        assert!(with.objects.iter().any(|o| o.as_text().is_some_and(|t| t.starts_with("Sermon  00:0"))));
        assert!(with.objects.iter().any(|o| o.as_text() == Some("Wrap up") && o.fill.is_some()));
        // Every box the slide uses is in the first layout.
        let theme = theme_for(&builtin_layouts()[0]);
        for object in &built.objects {
            assert!(theme.templates[0].style_for(object.role.as_ref().unwrap()).is_some());
        }
    }

    #[test]
    fn layouts_show_notes_title_and_group_and_a_screen_falls_back_to_the_first() {
        let mut song = sp_model::parse_song_text("S", "[Chorus]\nOne\n");
        song.groups[0].slides[0].notes = Some("Key change".to_owned());
        let now = song.slide(0).unwrap().1;
        let about = About { title: Some("Song".to_owned()), group: Some("Chorus".to_owned()) };
        let rect = SlideRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let everything = ThemeTemplate {
            id: Uuid::new_v4(),
            name: "All".to_owned(),
            objects: Vec::new(),
            boxes: ITEMS
                .iter()
                .map(|(role, _, _)| item_box(role, rect, 40.0, Rgba::WHITE, Align::Center, VAlign::Middle))
                .collect(),
        };
        let built = slide(Some(now), None, &crate::output::Overlay::default(), &about, &everything);
        for (role, words) in [(NOTES, "Key change"), (TITLE, "Song"), (GROUP, "Chorus")] {
            assert!(built.objects.iter().any(|o| o.role == Some(Role(role.to_owned())) && o.as_text() == Some(words)));
        }
        // A layout without a box for an item leaves it out, rather than drawing it in its first box.
        let words_only = &builtin_layouts()[1];
        let mut timer = crate::timers::Timer::new("Sermon".to_owned());
        timer.start(std::time::Instant::now());
        let overlay = crate::output::Overlay { timers: vec![timer], ..crate::output::Overlay::default() };
        let trimmed = slide(Some(now), None, &overlay, &about, words_only);
        assert!(trimmed.objects.iter().all(|o| words_only.style_for(o.role.as_ref().unwrap()).is_some()));
        assert!(!trimmed.objects.iter().any(|o| o.role == Some(Role(TIMER.to_owned()))));
        // Each built-in layout has a box only for items it knows, each item at most once.
        let layouts = builtin_layouts();
        for layout in &layouts {
            for (index, item) in layout.boxes.iter().enumerate() {
                assert!(ITEMS.iter().any(|(role, _, _)| *role == item.role.0), "{}", item.role.0);
                assert!(layout.boxes[index + 1..].iter().all(|other| other.role != item.role));
            }
        }
        assert_eq!(layout_for(&layouts, Some(layouts[2].id)).name, "Words and Notes");
        assert_eq!(layout_for(&layouts, Some(Uuid::new_v4())).id, layouts[0].id);
        assert_eq!(layout_for(&[], None).id, layouts[0].id);
        let made = new_layout("Mine".to_owned());
        assert_ne!(made.id, layouts[0].id);
        assert_eq!(
            serde_json::from_str::<Vec<ThemeTemplate>>(&serde_json::to_string(&layouts).unwrap()).unwrap(),
            layouts
        );
    }
}
