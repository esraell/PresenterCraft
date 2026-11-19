//! Image tests: draw through the real GPU path, read the pixels back and compare with stored
//! pictures in `tests/golden/`. A missing picture, or `PC_UPDATE_GOLDEN=1`, writes it instead.
//! They use the system font and need a GPU.

use std::path::PathBuf;

use pc_model::{Role, Slide, TextBox, Theme};
use pc_render::vello::util::RenderContext;
use pc_render::{Fonts, Painter, build_scene, read_pixels, target_texture, wgpu};

fn render(slide: Option<&Slide>, w: u32, h: u32) -> Vec<u8> {
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("these tests need a GPU");
    let (device, queue) = (&cx.devices[id].device, &cx.devices[id].queue);
    let mut painter = Painter::new(device).expect("painter");
    let built = build_scene(&mut Fonts::new(), slide, &Theme::builtin(), w, h);
    let texture = target_texture(device, w, h);
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    painter.paint(device, queue, &built.scene, &view, w, h).expect("paint");
    read_pixels(device, queue, &texture).expect("read back")
}

fn slide(primary: &str, translation: Option<&str>) -> Slide {
    let mut boxes = vec![TextBox { role: Role::primary(), text: primary.to_owned() }];
    if let Some(t) = translation {
        boxes.push(TextBox { role: Role::translation(), text: t.to_owned() });
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
    if std::env::var_os("PC_UPDATE_GOLDEN").is_some() || !path.exists() {
        write_png(&path, pixels, w, h);
        eprintln!("wrote {}", path.display());
        return;
    }
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
    if mean >= 2.0 || far_share >= 0.005 {
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
