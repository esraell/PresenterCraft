//! Image tests: draw through the real GPU path, read the pixels back and compare with stored
//! pictures in `tests/golden/`. Run with `PC_UPDATE_GOLDEN=1` to write the pictures; without
//! it a missing picture is a failure, so a deleted file cannot pass quietly.
//! They use the system font and need a GPU.

use std::path::PathBuf;

use pc_model::{Object, Role, Slide, Theme};
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
    for style in &mut theme.boxes {
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
    for style in &mut theme.boxes {
        style.outline_width = 0.0;
    }
    let px = render_with(Some(&s), &theme, 1920, 1080);
    assert_eq!(px.chunks(4).filter(|p| p[0] < 60 && p[1] > 200 && p[2] < 60).count(), 0);

    // The shadow is drawn underneath the letters. With white letters on black, switching it on
    // must leave exactly the same white pixels.
    for style in &mut theme.boxes {
        style.outline_color = Rgba::rgb(255, 255, 255);
        style.outline_width = 2.0;
        style.fill = Rgba::rgb(255, 255, 255);
    }
    let without = render_with(Some(&s), &theme, 1920, 1080);
    for style in &mut theme.boxes {
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
    let layers = pc_render::Layers { media, slide };
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
