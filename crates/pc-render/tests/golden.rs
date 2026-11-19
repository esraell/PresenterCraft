//! Image tests: draw through the real GPU path, read the pixels back and compare with stored
//! pictures in `tests/golden/`. Run with `PC_UPDATE_GOLDEN=1` to write the pictures; without
//! it a missing picture is a failure, so a deleted file cannot pass quietly.
//! They use the system font and need a GPU.

use std::path::PathBuf;

use pc_model::{Object, ObjectKind, Role, Slide, Theme};
use pc_render::vello::util::RenderContext;
use pc_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};

fn render(slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    render_with(slide, &Theme::builtin(), w, h)
}

fn render_with(slide: Option<&Slide>, theme: &Theme, w: u32, h: u32) -> Vec<u8> {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let built = build_scene(&mut Fonts::new(), slide, theme, w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

fn slide(primary: &str, translation: Option<&str>) -> Slide {
    let mut boxes = vec![Object::text(Role::primary(), primary)];
    if let Some(t) = translation {
        boxes.push(Object::text(Role::translation(), t));
    }
    Slide::new(boxes)
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/golden").join(format!("{name}.png"))
}

fn write_png(path: &PathBuf, pixels: &[u8], w: u32, h: u32) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header().expect("header").write_image_data(pixels).expect("data");
}

fn read_png(path: &PathBuf) -> (Vec<u8>, u32, u32) {
    let decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(path).expect("open png")));
    let mut reader = decoder.read_info().expect("png info");
    let mut data = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut data).expect("png frame");
    data.truncate(info.buffer_size());
    (data, info.width, info.height)
}

/// Passes when the pictures are nearly the same: small average difference and few pixels
/// that differ a lot.
fn assert_matches_golden(name: &str, pixels: &[u8], w: u32, h: u32) {
    let path = golden_path(name);
    if std::env::var_os("PC_UPDATE_GOLDEN").is_some() {
        write_png(&path, pixels, w, h);
        eprintln!("wrote {}", path.display());
        return;
    }
    assert!(
        path.exists(),
        "{name}: no stored picture at {}; run with PC_UPDATE_GOLDEN=1 and check it by eye",
        path.display()
    );
    let (expected, ew, eh) = read_png(&path);
    assert_eq!((ew, eh), (w, h), "{name}: size changed");
    assert_eq!(expected.len(), pixels.len(), "{name}: byte count changed");
    let mut total = 0u64;
    let mut far = 0usize;
    for (a, b) in expected.chunks(4).zip(pixels.chunks(4)) {
        let mut worst = 0u8;
        for (x, y) in a.iter().zip(b) {
            let d = x.abs_diff(*y);
            total += u64::from(d);
            worst = worst.max(d);
        }
        if worst > 32 {
            far += 1;
        }
    }
    let mean = total as f64 / pixels.len() as f64;
    let far_share = far as f64 / (pixels.len() / 4) as f64;
    // 0.05% of a 1080p frame is about a thousand pixels: a missing letter fails, GPU noise does not.
    if mean >= 1.0 || far_share >= 0.0005 {
        let actual = path.with_extension("actual.png");
        write_png(&actual, pixels, w, h);
        panic!(
            "{name}: differs from the stored picture (mean {mean:.2}, {:.2}% far off); see {}",
            far_share * 100.0,
            actual.display()
        );
    }
}

fn pixel(pixels: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2], pixels[i + 3]]
}

fn count_lit(pixels: &[u8]) -> usize {
    pixels.chunks(4).filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count()
}

#[test]
fn bilingual_1920x1080() {
    let s = slide(
        "Light rises over the quiet hills\nAnd the morning sings again",
        Some("Свет встаёт над тихими холмами\nІ ранок знову співає: їжак, ґанок, є"),
    );
    let px = render(Some(&s), 1920, 1080);
    // White main-language text and orange translation text are both really there.
    assert!(count_lit(&px) > 20_000, "expected white text");
    let orange = px.chunks(4).filter(|p| p[0] > 230 && (120..180).contains(&p[1]) && p[2] < 80).count();
    assert!(orange > 10_000, "expected orange text, found {orange} pixels");
    assert_matches_golden("bilingual_1920x1080", &px, 1920, 1080);
}

#[test]
fn wrapping_long_line() {
    let line = "The river runs and the fields are wide and the road goes on beyond the last low wall ".repeat(2);
    let px = render(Some(&slide(line.trim(), None)), 1920, 1080);
    // Wrapped text never reaches the left or right edge of the slide.
    for y in (0..1080).step_by(7) {
        assert!(
            pixel(&px, 1920, 4, y)[0] < 40 && pixel(&px, 1920, 1915, y)[0] < 40,
            "text reached the edge at row {y}"
        );
    }
    assert_matches_golden("wrapping_long_line", &px, 1920, 1080);
}

#[test]
fn shrink_to_fit() {
    let text = ["A line of plain words"; 9].join("\n");
    let px = render(Some(&slide(&text, None)), 1920, 1080);
    // Shrunk to fit: nothing is drawn in the top or bottom 40 rows.
    for y in (0..40).chain(1040..1080) {
        for x in (0..1920).step_by(5) {
            assert!(pixel(&px, 1920, x, y)[0] < 40, "text spilled to row {y}");
        }
    }
    assert_matches_golden("shrink_to_fit", &px, 1920, 1080);
}

#[test]
fn solo_primary_centred() {
    let px = render(Some(&slide("One line alone", None)), 1920, 1080);
    let rows: Vec<u32> = (0..1080).filter(|y| (0..1920).any(|x| pixel(&px, 1920, x, *y)[0] > 200)).collect();
    let middle = (rows.first().expect("text") + rows.last().expect("text")) / 2;
    assert!((500..580).contains(&middle), "a lone box should sit mid-slide, found row {middle}");
    assert_matches_golden("solo_primary_centred", &px, 1920, 1080);
}

#[test]
fn letterbox_4_3() {
    let s = slide("Light rises", Some("Свет встаёт"));
    let px = render(Some(&s), 1600, 1200);
    for y in (0..150).chain(1050..1200) {
        for x in (0..1600).step_by(3) {
            assert_eq!(pixel(&px, 1600, x, y), [0, 0, 0, 255], "bar not black at {x},{y}");
        }
    }
    assert!(count_lit(&px) > 1_000);
    assert_matches_golden("letterbox_4_3", &px, 1600, 1200);
}

/// The built-in theme's outline and shadow are black on black, so they cannot be seen in the
/// other pictures. Here the outline is green and the shadow is switched off and on.
#[test]
fn outline_and_shadow_are_drawn() {
    use pc_model::Rgba;
    let s = slide("OUTLINE", None);
    let mut theme = Theme::builtin();
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_color = Rgba::rgb(0, 255, 0);
        style.outline_width = 8.0;
        style.fill = Rgba::rgb(255, 0, 0);
        style.shadow = false;
    }
    let px = render_with(Some(&s), &theme, 1920, 1080);
    let green = px.chunks(4).filter(|p| p[0] < 60 && p[1] > 200 && p[2] < 60).count();
    let red = px.chunks(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    assert!(green > 5_000, "expected a green outline, found {green} pixels");
    assert!(red > 5_000, "expected red letters, found {red} pixels");

    // Without an outline the green goes away.
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_width = 0.0;
    }
    let px = render_with(Some(&s), &theme, 1920, 1080);
    assert_eq!(px.chunks(4).filter(|p| p[0] < 60 && p[1] > 200 && p[2] < 60).count(), 0);

    // The shadow is drawn underneath the letters. With white letters on black, switching it on
    // must leave exactly the same white pixels.
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.outline_color = Rgba::rgb(255, 255, 255);
        style.outline_width = 2.0;
        style.fill = Rgba::rgb(255, 255, 255);
    }
    let without = render_with(Some(&s), &theme, 1920, 1080);
    for style in theme.templates.iter_mut().flat_map(|template| &mut template.boxes) {
        style.shadow = true;
    }
    let with = render_with(Some(&s), &theme, 1920, 1080);
    assert_eq!(count_lit(&without), count_lit(&with), "a shadow must not eat into the letters");
}

#[test]
fn cleared_is_black() {
    let px = render(None, 640, 360);
    assert!(px.chunks(4).all(|p| p == [0, 0, 0, 255]));
}

#[test]
fn odd_width_reads_back_without_padding() {
    // 333 pixels wide is not a multiple of the GPU's row alignment.
    let px = render(None, 333, 77);
    assert_eq!(px.len(), 333 * 77 * 4);
}

fn render_layers(media: Option<&pc_render::Picture>, slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let layers = pc_render::Layers {
        media_fit: pc_model::ImageFit::Fill,
        media,
        slide,
        overlay: None,
        time: 0.0,
        backdrop: None,
        mask: [0.0; 4],
    };
    let built = pc_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

/// A picture of one flat colour, of any shape.
fn flat(width: u32, height: u32, rgb: [u8; 3]) -> pc_render::Picture {
    let rgba = [rgb[0], rgb[1], rgb[2], 255].repeat((width * height) as usize);
    pc_render::Picture::from_rgba(width, height, rgba).expect("picture")
}

fn is_about(p: [u8; 4], rgb: [u8; 3]) -> bool {
    p[0].abs_diff(rgb[0]) < 12 && p[1].abs_diff(rgb[1]) < 12 && p[2].abs_diff(rgb[2]) < 12
}

#[test]
fn media_fills_the_slide_whatever_its_shape() {
    let green = [20, 200, 60];
    for (pw, ph) in [(64, 36), (40, 40), (120, 20), (20, 90)] {
        let px = render_layers(Some(&flat(pw, ph, green)), None, 640, 360);
        for (x, y) in [(2, 2), (637, 2), (2, 357), (637, 357), (320, 180)] {
            assert!(is_about(pixel(&px, 640, x, y), green), "{pw}x{ph} picture left a gap at {x},{y}");
        }
    }
}

#[test]
fn media_stays_inside_the_slide_when_the_screen_is_another_shape() {
    let px = render_layers(Some(&flat(100, 10, [255, 0, 0])), None, 400, 300);
    // 400x300 holds a 400x225 slide with bars above and below.
    for y in [2, 30, 270, 297] {
        assert_eq!(pixel(&px, 400, 200, y), [0, 0, 0, 255], "the picture spilled into the bar at row {y}");
    }
    assert!(is_about(pixel(&px, 400, 200, 150), [255, 0, 0]));
}

#[test]
fn words_are_drawn_over_the_media() {
    let blue = [0, 0, 200];
    let s = slide("WORDS", None);
    let px = render_layers(Some(&flat(16, 9, blue)), Some(&s), 1920, 1080);
    assert!(is_about(pixel(&px, 1920, 20, 20), blue), "the corner should show the picture");
    assert!(count_lit(&px) > 3_000, "the white words should be on top of the picture");
}

#[test]
fn a_moved_object_is_drawn_where_it_was_put() {
    use pc_model::SlideRect;
    let mut s = slide("X", None);
    // Bottom-right quarter of the slide.
    s.objects[0].frame = Some(SlideRect { x: 960.0, y: 540.0, w: 960.0, h: 540.0 });
    let px = render(Some(&s), 640, 360);
    let lit = |x0: u32, y0: u32| {
        (y0..y0 + 180)
            .flat_map(|y| (x0..x0 + 320).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(&px, 640, *x, *y)[0] > 200)
            .count()
    };
    assert_eq!(lit(0, 0) + lit(320, 0) + lit(0, 180), 0, "nothing should be drawn outside the object's quarter");
    assert!(lit(320, 180) > 100, "the letter should be in the bottom-right quarter");
}

#[test]
fn a_faded_object_is_dimmer_and_a_turned_one_moves_its_ink() {
    let upright = slide("IIIIIIII", None);
    let px_upright = render(Some(&upright), 640, 360);
    let mut faded = slide("IIIIIIII", None);
    faded.objects[0].opacity = 0.3;
    let px_faded = render(Some(&faded), 640, 360);
    assert!(count_lit(&px_upright) > 200);
    assert_eq!(count_lit(&px_faded), 0, "at 30% no pixel should stay bright white");
    assert!(px_faded.chunks(4).any(|p| p[0] > 40), "but the letters are still there");

    let mut turned = slide("IIIIIIII", None);
    turned.objects[0].rotation = 90.0;
    let px_turned = render(Some(&turned), 640, 360);
    // Upright, the row of letters is wide and short; turned, it is tall and narrow.
    let extent = |px: &[u8]| {
        let lit: Vec<(u32, u32)> = (0..360)
            .flat_map(|y| (0..640).map(move |x| (x, y)))
            .filter(|(x, y)| pixel(px, 640, *x, *y)[0] > 200)
            .collect();
        let xs = lit.iter().map(|p| p.0);
        let ys = lit.iter().map(|p| p.1);
        (xs.clone().max().unwrap() - xs.min().unwrap(), ys.clone().max().unwrap() - ys.min().unwrap())
    };
    let (wide, short) = extent(&px_upright);
    let (narrow, tall) = extent(&px_turned);
    assert!(wide > short && tall > narrow, "upright {wide}x{short}, turned {narrow}x{tall}");
}

#[test]
fn an_objects_own_colour_alignment_and_size_are_drawn() {
    use pc_model::{Align, ObjectKind, Rgba};
    let mut s = slide("IIII", None);
    let plain = render(Some(&s), 640, 360);
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.fill = Some(Rgba::rgb(255, 0, 0));
    style.align = Some(Align::Left);
    style.outline_width = Some(0.0);
    style.shadow = Some(false);
    let px = render(Some(&s), 640, 360);
    let red: Vec<u32> = (0..360)
        .flat_map(|y| (0..640).map(move |x| (x, y)))
        .filter(|(x, y)| is_about(pixel(&px, 640, *x, *y), [255, 0, 0]))
        .map(|p| p.0)
        .collect();
    assert!(red.len() > 100, "the letters should be red");
    assert_eq!(count_lit(&px), 0, "and no longer white");
    assert!(*red.iter().max().unwrap() < 320, "left-aligned letters stay in the left half");
    assert!(count_lit(&plain) > 100, "the same slide in the theme's style is white");

    // A much smaller size draws much less ink.
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.size = Some(20.0);
    let small = render(Some(&s), 640, 360);
    let small_red = small.chunks(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    assert!(small_red * 4 < red.len(), "{small_red} pixels at size 20 against {}", red.len());
}

/// A fade half-way from a black screen to a white one is mid grey; at its ends it is one
/// picture or the other.
#[test]
fn a_cross_fade_blends_two_pictures() {
    use pc_model::Rgba;
    use pc_render::cross_fade;
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), None, &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    let mut middle_of = |amount: f32| {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &cross_fade(&black, &white, amount, w, h), &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        let at = ((h / 2 * w + w / 2) * 4) as usize;
        pixels[at]
    };
    assert_eq!(middle_of(0.0), 0);
    assert_eq!(middle_of(1.0), 255);
    let half = middle_of(0.5);
    assert!((110..=145).contains(&half), "half-way was {half}");
}

/// Half-way through a wipe or a push from the left, the left half shows the new picture and the
/// right half the old one; half-way through a fade through black, the screen is black.
#[test]
fn transitions_put_each_picture_where_it_belongs() {
    use pc_model::{Direction, Rgba, TransitionKind};
    let (w, h) = (64, 36);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    let mut shade_at = |kind: TransitionKind, amount: f32, x: u32| {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scene = pc_render::transition(kind, Direction::FromLeft, &black, &white, amount, w, h);
        painter.paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        pixels[((h / 2 * w + x) * 4) as usize]
    };
    for kind in [TransitionKind::Wipe, TransitionKind::Push, TransitionKind::MoveIn] {
        assert_eq!(shade_at(kind, 0.5, w / 4), 255, "{kind:?}: the left is new");
        assert_eq!(shade_at(kind, 0.5, w * 3 / 4), 0, "{kind:?}: the right is old");
    }
    assert_eq!(shade_at(TransitionKind::FadeBlack, 0.5, w / 2), 0);
    assert_eq!(shade_at(TransitionKind::FadeWhite, 0.5, w / 2), 255);
    assert_eq!(shade_at(TransitionKind::Iris, 0.0, w / 2), 0);
    assert_eq!(shade_at(TransitionKind::Iris, 1.0, 1), 255);
}

#[test]
fn every_transition_starts_on_the_old_picture_and_ends_on_the_new() {
    use pc_model::{Direction, Rgba, TransitionKind};
    let (w, h) = (160, 90);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let black = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &Theme::builtin(), w, h).scene;
    let mut white_theme = Theme::builtin();
    white_theme.background = Some(Rgba::WHITE);
    let white = build_scene(&mut Fonts::new(), Some(&Slide::new(Vec::new())), &white_theme, w, h).scene;
    // Points in from the edges: the middle and near each corner.
    let points = [(w / 2, h / 2), (w / 8, h / 8), (w * 7 / 8, h / 8), (w / 8, h * 7 / 8), (w * 7 / 8, h * 7 / 8)];
    for direction in [Direction::FromLeft, Direction::FromTop] {
        for kind in TransitionKind::ALL.into_iter().filter(|kind| *kind != TransitionKind::Cut) {
            for (amount, want) in [(0.0, 0), (1.0, 255)] {
                let texture = target_texture(device, w, h);
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                let scene = pc_render::transition(kind, direction, &black, &white, amount, w, h);
                painter.paint(device, queue, &scene, &view, w, h).expect("paint");
                let pixels = read_pixels(device, queue, &texture).expect("read back");
                for (x, y) in points {
                    let red = pixels[((y * w + x) * 4) as usize];
                    assert_eq!(red, want, "{kind:?} {direction:?} at {amount}: ({x}, {y})");
                }
            }
        }
    }
}

/// A wide red picture for any file but "missing.png", which can't be read.
fn wide_red(file: &str) -> Option<pc_render::Picture> {
    (file != "missing.png").then(|| flat(200, 100, [230, 30, 30]))
}

#[test]
fn a_picture_object_sits_in_its_place_as_its_fit_says() {
    use pc_model::{ImageFit, Object, ObjectKind, SlideRect};
    pc_render::set_picture_loader(wide_red);
    let (w, h) = (192, 108);
    // A square place in the middle of the slide; the picture is twice as wide as it is tall.
    let place = SlideRect { x: 660.0, y: 240.0, w: 600.0, h: 600.0 };
    let draw = |file: &str, fit: ImageFit| {
        let mut object = Object::image(file.to_owned(), "Picture".to_owned(), 200, 100);
        object.frame = Some(place);
        object.kind = ObjectKind::Image { file: file.to_owned(), fit };
        render_layers(None, Some(&Slide::new(vec![object])), w, h)
    };
    // The square's middle, a point near its top (above a fitted picture), and outside it.
    let (middle, near_top, outside) = ((96, 54), (96, 28), (40, 54));
    let red = [230, 30, 30];
    let fit = draw("logo.png", ImageFit::Fit);
    assert!(is_about(pixel(&fit, w, middle.0, middle.1), red));
    assert!(is_about(pixel(&fit, w, near_top.0, near_top.1), [0, 0, 0]), "a fitted picture leaves room above");
    assert!(is_about(pixel(&fit, w, outside.0, outside.1), [0, 0, 0]));
    for filled in [ImageFit::Fill, ImageFit::Stretch] {
        let pixels = draw("logo.png", filled);
        assert!(is_about(pixel(&pixels, w, near_top.0, near_top.1), red), "{filled:?} fills the place");
        assert!(is_about(pixel(&pixels, w, outside.0, outside.1), [0, 0, 0]), "{filled:?} stays inside it");
    }
    // A picture that can't be read is a grey box, not nothing.
    let missing = draw("missing.png", ImageFit::Fit);
    let grey = pixel(&missing, w, near_top.0, near_top.1);
    assert!(grey[0] > 60 && grey[0].abs_diff(grey[2]) < 20, "{grey:?}");
}

#[test]
fn a_text_background_is_drawn_behind_the_lines_or_across_the_box() {
    use pc_model::{LineFill, ObjectKind, Rgba};
    let blue = [20, 40, 220];
    let band = |full_width: bool| {
        let mut s = slide("II", None);
        let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
        style.line_fill = Some(LineFill { on: true, color: Rgba::rgb(blue[0], blue[1], blue[2]), full_width });
        style.shadow = Some(false);
        render(Some(&s), 640, 360)
    };
    let plain = render(Some(&slide("II", None)), 640, 360);
    let blue_count = |px: &[u8]| px.chunks(4).filter(|p| is_about([p[0], p[1], p[2], p[3]], blue)).count();
    assert_eq!(blue_count(&plain), 0);
    let lines_only = band(false);
    let full = band(true);
    assert!(blue_count(&lines_only) > 200, "a band sits behind the short line");
    assert!(blue_count(&full) > blue_count(&lines_only) * 3, "across the box is much wider");
}

#[test]
fn scrolling_text_starts_in_its_box_and_moves_with_time() {
    use pc_model::{ObjectKind, Scroll};
    let mut s = slide("IIIIIIII", None);
    let ObjectKind::Text { style, .. } = &mut s.objects[0].kind else { panic!("a text object") };
    style.scroll = Some(Scroll::Left);
    style.shadow = Some(false);
    style.outline_width = Some(0.0);
    let draw = |time: f32| {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let layers = pc_render::Layers {
            media_fit: pc_model::ImageFit::Fill,
            media: None,
            slide: Some(&s),
            overlay: None,
            time,
            backdrop: None,
            mask: [0.0; 4],
        };
        let built = pc_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 640, 360);
        let texture = target_texture(device, 640, 360);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 640, 360).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    // Where the white letters are, across the screen.
    let lit_columns =
        |px: &[u8]| -> Vec<u32> { (0..640).filter(|x| (0..360).any(|y| pixel(px, 640, *x, y)[0] > 200)).collect() };
    let start = lit_columns(&draw(0.0));
    let later = lit_columns(&draw(1.0));
    assert!(!start.is_empty() && !later.is_empty());
    assert!(later[0] < start[0], "moving left: {} then {}", start[0], later[0]);
    assert!(pc_render::scrolls(&s, &Theme::builtin()));
    assert!(!pc_render::scrolls(&slide("Still", None), &Theme::builtin()));
}

#[test]
fn a_themes_picture_is_behind_its_slides_until_media_shows() {
    pc_render::set_picture_loader(wide_red);
    let mut theme = Theme::builtin();
    theme.background_picture = Some("church.png".to_owned());
    let draw = |media: Option<&pc_render::Picture>| {
        let mut cx = RenderContext::new();
        let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
        let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
        let mut painter = Painter::new(device).expect("painter");
        let empty = Slide::new(Vec::new());
        let layers = pc_render::Layers {
            media_fit: pc_model::ImageFit::Fill,
            media,
            slide: Some(&empty),
            overlay: None,
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
        };
        let built = pc_render::build_layers(&mut Fonts::new(), &layers, &theme, 160, 90);
        let texture = target_texture(device, 160, 90);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 160, 90).expect("paint");
        read_pixels(device, queue, &texture).expect("read back")
    };
    assert!(is_about(pixel(&draw(None), 160, 80, 45), [230, 30, 30]), "the theme's picture fills the slide");
    let green = flat(16, 9, [30, 200, 60]);
    assert!(is_about(pixel(&draw(Some(&green)), 160, 80, 45), [30, 200, 60]), "media covers it");
}

#[test]
fn a_screens_colour_fills_the_empty_room_and_its_covered_edges() {
    use pc_model::Rgba;
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let white = flat(16, 9, [255, 255, 255]);
    let teal = Rgba::rgb(0, 128, 128);
    // A 4:3 target: the slide is letterboxed, with room above and below it.
    let layers = pc_render::Layers {
        media_fit: pc_model::ImageFit::Fill,
        media: Some(&white),
        slide: None,
        overlay: None,
        time: 0.0,
        backdrop: Some(teal),
        mask: [480.0, 0.0, 0.0, 0.0],
    };
    let built = pc_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 160, 120);
    let texture = target_texture(device, 160, 120);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, 160, 120).expect("paint");
    let px = read_pixels(device, queue, &texture).expect("read back");
    assert!(is_about(pixel(&px, 160, 80, 4), [0, 128, 128]), "the room above the slide is the screen's colour");
    assert!(is_about(pixel(&px, 160, 10, 60), [0, 128, 128]), "the covered left edge is too");
    assert!(is_about(pixel(&px, 160, 120, 60), [255, 255, 255]), "the rest of the slide shows");
}

/// Writes each transition 40% of the way through, from a red slide to a blue one, to the folder
/// SHEET_DIR names, to look at by hand:
/// `SHEET_DIR=/tmp/sheet cargo test -p pc-render --test golden transition_sheet -- --ignored`.
#[test]
#[ignore = "writes files, for looking at by hand"]
fn transition_sheet() {
    use pc_model::{Direction, Rgba, TransitionKind};
    let Ok(dir) = std::env::var("SHEET_DIR") else { return };
    let (w, h) = (320, 180);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let coloured = |rgba: Rgba, words: &str| {
        let mut theme = Theme::builtin();
        theme.background = Some(rgba);
        build_scene(&mut Fonts::new(), Some(&slide(words, None)), &theme, w, h).scene
    };
    let from = coloured(Rgba { r: 200, g: 40, b: 40, a: 255 }, "Old");
    let to = coloured(Rgba { r: 40, g: 80, b: 220, a: 255 }, "New");
    for kind in TransitionKind::ALL {
        let texture = target_texture(device, w, h);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let scene = pc_render::transition(kind, Direction::FromLeft, &from, &to, 0.4, w, h);
        painter.paint(device, queue, &scene, &view, w, h).expect("paint");
        let pixels = read_pixels(device, queue, &texture).expect("read back");
        write_png(&PathBuf::from(&dir).join(format!("{kind:?}.png")), &pixels, w, h);
    }
}

/// The lowest and highest rows with white (text) pixels, or `None` when there are none.
fn lit_rows(pixels: &[u8], w: u32, h: u32) -> Option<(u32, u32)> {
    let lit = |y: u32| (0..w).any(|x| pixel(pixels, w, x, y)[0] > 200 && pixel(pixels, w, x, y)[2] > 200);
    let top = (0..h).find(|y| lit(*y))?;
    let bottom = (0..h).rev().find(|y| lit(*y))?;
    Some((top, bottom))
}

#[test]
fn paragraph_spacing_baseline_and_paragraph_builds_move_and_hide_lines() {
    use pc_model::{SlideRect, TextStyle, VAlign};
    let (w, h) = (480, 270);
    let with = |style: TextStyle, shown: Option<usize>| {
        let mut s = slide("One\nTwo", None);
        s.objects[0].frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        if let ObjectKind::Text { style: own, .. } = &mut s.objects[0].kind {
            *own = TextStyle { valign: Some(VAlign::Top), size: Some(120.0), ..style };
        }
        s.objects[0].shown_paragraphs = shown;
        lit_rows(&render(Some(&s), w, h), w, h)
    };
    let (top, bottom) = with(TextStyle::default(), None).expect("text is drawn");
    // 200 points of paragraph spacing push the second line about 50 pixels lower at a quarter size.
    let (spaced_top, spaced_bottom) =
        with(TextStyle { paragraph_spacing: Some(200.0), ..TextStyle::default() }, None).expect("drawn");
    assert_eq!(spaced_top, top, "the first line stays put");
    assert!((spaced_bottom as i32 - bottom as i32 - 50).abs() <= 3, "{bottom} -> {spaced_bottom}");
    // A baseline of 40 points raises the whole block by about 10 pixels.
    let (raised_top, raised_bottom) =
        with(TextStyle { baseline: Some(40.0), ..TextStyle::default() }, None).expect("drawn");
    assert!((top as i32 - raised_top as i32 - 10).abs() <= 2 && (bottom as i32 - raised_bottom as i32 - 10).abs() <= 2);
    // Only the first paragraph of two shows while a build has reached one.
    let (_, first_only_bottom) = with(TextStyle::default(), Some(1)).expect("drawn");
    assert!(first_only_bottom < (top + bottom) / 2 + 10, "{first_only_bottom} vs {top}..{bottom}");
}

#[test]
fn a_gradient_fill_runs_from_one_colour_to_the_other() {
    use pc_model::{Gradient, GradientKind, Rgba, Shape, SlideRect};
    let (w, h) = (192, 108);
    let mut object = Object::shape(Shape::Rectangle);
    object.frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
    object.fill = Some(Rgba { r: 255, g: 0, b: 0, a: 255 });
    object.gradient =
        Some(Gradient { kind: GradientKind::Linear, to: Rgba { r: 0, g: 0, b: 255, a: 255 }, angle: 0.0 });
    let px = render(Some(&Slide::new(vec![object])), w, h);
    let (left, right) = (pixel(&px, w, 4, h / 2), pixel(&px, w, w - 5, h / 2));
    assert!(left[0] > 200 && left[2] < 60, "the left is red: {left:?}");
    assert!(right[2] > 200 && right[0] < 60, "the right is blue: {right:?}");
}

/// A blank drawing between two drawings of the same picture must not lose the picture (vello
/// 0.12 threw its picture atlas away on a scene of plain fills only).
#[test]
fn a_picture_still_shows_after_a_blank_drawing() {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let photo = flat(1600, 1200, [230, 30, 30]);
    let mut draw = |media: Option<&pc_render::Picture>| {
        let layers = pc_render::Layers { media_fit: pc_model::ImageFit::Fill, media, ..Default::default() };
        let built = pc_render::build_layers(&mut Fonts::new(), &layers, &Theme::builtin(), 160, 90);
        let texture = target_texture(device, 160, 90);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        painter.paint(device, queue, &built.scene, &view, 160, 90).expect("paint");
        pixel(&read_pixels(device, queue, &texture).expect("read back"), 160, 80, 45)
    };
    assert!(is_about(draw(Some(&photo)), [230, 30, 30]));
    assert!(is_about(draw(None), [0, 0, 0]), "the blank drawing is black");
    assert!(is_about(draw(Some(&photo)), [230, 30, 30]), "the picture is still there");
}
