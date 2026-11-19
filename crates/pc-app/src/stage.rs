//! What a stage screen shows: the words on screen now, large; the next slide's words under
//! them; and the time. One layout for now.

use pc_model::{Align, BoxStyle, Object, Rgba, Role, Slide, SlideRect, Theme, ThemeTemplate, Uuid, VAlign};

const NOW: &str = "now";
const NEXT_LABEL: &str = "next-label";
const NEXT: &str = "next";
const CLOCK: &str = "clock";

/// The layout stage screens are drawn with.
pub fn theme() -> Theme {
    let base = base_box();
    let plain = |role: &str, rect: SlideRect, size: f32, fill: Rgba, align: Align, valign: VAlign| BoxStyle {
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
        ..base.clone()
    };
    let boxes = vec![
        plain(
            NOW,
            SlideRect { x: 60.0, y: 130.0, w: 1800.0, h: 560.0 },
            110.0,
            Rgba::WHITE,
            Align::Center,
            VAlign::Middle,
        ),
        BoxStyle {
            weight: 600,
            ..plain(
                NEXT_LABEL,
                SlideRect { x: 60.0, y: 712.0, w: 600.0, h: 50.0 },
                34.0,
                Rgba::rgb(140, 140, 140),
                Align::Left,
                VAlign::Middle,
            )
        },
        plain(
            NEXT,
            SlideRect { x: 60.0, y: 770.0, w: 1800.0, h: 280.0 },
            64.0,
            Rgba::rgb(255, 200, 60),
            Align::Center,
            VAlign::Top,
        ),
        plain(
            CLOCK,
            SlideRect { x: 1360.0, y: 24.0, w: 520.0, h: 90.0 },
            64.0,
            Rgba::WHITE,
            Align::Right,
            VAlign::Middle,
        ),
    ];
    Theme {
        id: Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0001),
        name: "Stage".to_owned(),
        templates: vec![ThemeTemplate {
            id: Uuid::from_u128(0x7374_6167_6500_0000_0000_0000_0000_0002),
            name: "Stage".to_owned(),
            boxes,
        }],
        background: None,
    }
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
        shadow_look: pc_model::Shadow::default(),
        valign: VAlign::Middle,
        caps: pc_model::Caps::None,
        underline: false,
        strike: false,
        scaling: pc_model::Scaling::TextDown,
        line_transform: pc_model::LineTransform::None,
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

/// The stage screen's picture as a slide: the time, the words on screen now, and the next
/// slide's words.
pub fn slide(now: Option<&Slide>, next: Option<&Slide>) -> Slide {
    let time = chrono::Local::now().format("%-I:%M").to_string();
    let mut objects = vec![Object::text(Role(CLOCK.to_owned()), time)];
    if let Some(now) = now {
        objects.push(Object::text(Role(NOW.to_owned()), words(now)));
    }
    if let Some(next) = next {
        objects.push(Object::text(Role(NEXT_LABEL.to_owned()), "NEXT"));
        objects.push(Object::text(Role(NEXT.to_owned()), words(next)));
    }
    Slide::new(objects)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Draws a stage screen and saves it where `STAGE_PNG` says, to look at. Run by hand:
    /// `STAGE_PNG=/tmp/stage.png cargo test -p pc-app stage_picture -- --ignored`.
    #[test]
    #[ignore = "needs a GPU, and is for looking at"]
    fn stage_picture() {
        use pc_render::vello::util::RenderContext;
        use pc_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};
        let Ok(path) = std::env::var("STAGE_PNG") else { return };
        let song = pc_model::parse_song_text(
            "S",
            "We lift our voices, we lift our hands\nWe sing together across the lands\n--\nМы поднимаем голоса и руки\n\nOne song, one heart\n--\nОдна песня, одно сердце\n",
        );
        let built = slide(song.slide(0).map(|(_, s)| s), song.slide(1).map(|(_, s)| s));
        let (w, h) = (1920, 1080);
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let scene = build_scene(&mut Fonts::new(), Some(&built), &theme(), w, h).scene;
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
        let song = pc_model::parse_song_text("S", "One\n--\nОдин\n\nTwo\n");
        let (now, next) = (song.slide(0).unwrap().1, song.slide(1).unwrap().1);
        let built = slide(Some(now), Some(next));
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
        assert_eq!(slide(None, None).objects.len(), 1);
        // Every box the slide uses is in the layout.
        let theme = theme();
        for object in &built.objects {
            assert!(theme.templates[0].style_for(object.role.as_ref().unwrap()).is_some());
        }
    }
}
