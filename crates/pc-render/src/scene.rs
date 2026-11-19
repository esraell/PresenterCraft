use parley::{Alignment, AlignmentOptions, FontWeight, Layout, LineHeight, PositionedLayoutItem, StyleProperty};
use pc_model::{Align, BoxStyle, Rgba, Role, SLIDE_HEIGHT, SLIDE_WIDTH, Slide, SlideRect, Theme};
use vello::kurbo::{Affine, Join, Rect, Stroke};
use vello::peniko::{Color, Fill};
use vello::{Glyph, Scene};

use crate::Fonts;

/// A scene ready to paint, and whether any text did not fit its box.
pub struct Built {
    pub scene: Scene,
    /// True when a box's text was still too tall at its smallest allowed size. The text is
    /// drawn anyway, spilling past its box, because cut-off words are worse than untidy ones.
    pub overflow: bool,
}

/// The largest 16:9 rectangle centred inside a target, as (x, y, width, height) in pixels.
pub fn fit_rect(target_w: u32, target_h: u32) -> (f64, f64, f64, f64) {
    let (tw, th) = (f64::from(target_w), f64::from(target_h));
    let (sw, sh) = (f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
    // Compare by cross-multiplying so exact fits stay exact.
    let (w, h) = if tw * sh <= th * sw { (tw, tw * sh / sw) } else { (th * sw / sh, th) };
    ((tw - w) / 2.0, (th - h) / 2.0, w, h)
}

/// Draws a slide for a target of the given pixel size: black, with the slide fitted inside.
/// `None` draws black only, which is what a cleared layer looks like.
pub fn build_scene(fonts: &mut Fonts, slide: Option<&Slide>, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    let mut scene = Scene::new();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &whole);

    let mut overflow = false;
    if let Some(slide) = slide {
        let (x, y, w, _) = fit_rect(target_w, target_h);
        let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));
        let alone = slide.boxes.len() == 1;
        for text_box in &slide.boxes {
            let (style, rect) = placement(theme, &text_box.role, alone);
            let Some(style) = style else { continue };
            overflow |= draw_text_box(&mut scene, fonts, to_target, &text_box.text, style, rect);
        }
    }
    Built { scene, overflow }
}

/// The style and rectangle for a box. A lone main-language box, or a box whose role the theme
/// does not know, is centred on the slide in the main-language style.
fn placement<'a>(theme: &'a Theme, role: &Role, alone: bool) -> (Option<&'a BoxStyle>, SlideRect) {
    match theme.style_for(role) {
        Some(style) if alone && *role == Role::primary() => (Some(style), theme.solo_primary),
        Some(style) => (Some(style), style.rect),
        None => (theme.style_for(&Role::primary()), theme.solo_primary),
    }
}

fn lay_out(fonts: &mut Fonts, text: &str, style: &BoxStyle, size: f32, width: f32) -> Layout<()> {
    let mut builder = fonts.layout_cx.ranged_builder(&mut fonts.font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(style.weight))));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.25)));
    builder.push_default(StyleProperty::FontFamily(style.font_family.as_str().into()));
    let mut layout: Layout<()> = builder.build(text);
    let alignment = match style.align {
        Align::Left => Alignment::Left,
        Align::Center => Alignment::Center,
        Align::Right => Alignment::Right,
    };
    // Lines wider than the box (one very long word) stay centred instead of jumping left.
    layout.set_alignment(alignment, AlignmentOptions { align_when_overflowing: true, ..AlignmentOptions::default() });
    layout.break_all_lines(Some(width));
    layout
}

/// Lays the text out at the style's size, shrinking in small steps until it fits the box's
/// height or reaches the smallest allowed size. Returns the layout and whether it still
/// overflows.
fn fit(fonts: &mut Fonts, text: &str, style: &BoxStyle, rect: SlideRect) -> (Layout<()>, bool) {
    let floor = style.min_size.min(style.size).max(1.0);
    let mut size = style.size.max(floor);
    loop {
        let layout = lay_out(fonts, text, style, size, rect.w);
        let too_big = layout.height() > rect.h || layout.width() > rect.w + 0.5;
        if !too_big || size <= floor {
            return (layout, too_big);
        }
        size = (size * 0.96).max(floor);
    }
}

fn color(c: Rgba) -> Color {
    Color::from_rgba8(c.r, c.g, c.b, c.a)
}

/// Draws one box: soft shadow, then outline, then fill. Returns whether the text overflowed.
fn draw_text_box(
    scene: &mut Scene,
    fonts: &mut Fonts,
    to_target: Affine,
    text: &str,
    style: &BoxStyle,
    rect: SlideRect,
) -> bool {
    let (layout, overflow) = fit(fonts, text, style, rect);
    // Centre the text block vertically in its box (it may spill both ways when it overflows).
    let top = f64::from(rect.y) + (f64::from(rect.h) - f64::from(layout.height())) / 2.0;
    let origin = to_target * Affine::translate((f64::from(rect.x), top));
    let outline_width = f64::from(style.outline_width);
    let outline = Stroke::new(outline_width * 2.0).with_join(Join::Round);
    let fill = color(style.fill);
    let outline_color = color(style.outline_color);

    for pass in [Pass::Shadow, Pass::Outline, Pass::Fill] {
        if (pass == Pass::Shadow && !style.shadow) || (pass == Pass::Outline && outline_width <= 0.0) {
            continue;
        }
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else {
                    continue;
                };
                let font = run.run().font();
                let size = run.run().font_size();
                let coords: Vec<i16> = run.run().normalized_coords().iter().map(|c| c.to_bits()).collect();
                let glyphs: Vec<Glyph> = run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }).collect();
                match pass {
                    Pass::Shadow => {
                        // The renderer cannot blur glyphs, so a soft shadow is several faint,
                        // widening strokes, offset down and to the right.
                        let shadow = origin * Affine::translate((6.0, 8.0));
                        for step in 1..=6 {
                            let stroke =
                                Stroke::new(outline_width * 2.0 + f64::from(step) * 5.0).with_join(Join::Round);
                            scene
                                .draw_glyphs(font)
                                .font_size(size)
                                .transform(shadow)
                                .normalized_coords(&coords)
                                .brush(Color::BLACK)
                                .brush_alpha(0.10)
                                .draw(&stroke, glyphs.iter().copied());
                        }
                    }
                    Pass::Outline => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .normalized_coords(&coords)
                        .brush(outline_color)
                        .draw(&outline, glyphs.iter().copied()),
                    Pass::Fill => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .normalized_coords(&coords)
                        .brush(fill)
                        .draw(Fill::NonZero, glyphs.iter().copied()),
                }
            }
        }
    }
    overflow
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pass {
    Shadow,
    Outline,
    Fill,
}

#[cfg(test)]
mod tests {
    use pc_model::{TextBox, parse_song_text};

    use super::*;

    fn slide(primary: &str, translation: Option<&str>) -> Slide {
        let mut boxes = vec![TextBox { role: Role::primary(), text: primary.to_owned() }];
        if let Some(t) = translation {
            boxes.push(TextBox { role: Role::translation(), text: t.to_owned() });
        }
        Slide::new(boxes)
    }

    fn overflows(slide: &Slide) -> bool {
        build_scene(&mut Fonts::new(), Some(slide), &Theme::builtin(), 1920, 1080).overflow
    }

    #[test]
    fn fit_rect_same_shape() {
        assert_eq!(fit_rect(1920, 1080), (0.0, 0.0, 1920.0, 1080.0));
    }

    #[test]
    fn fit_rect_4_3_letterboxes() {
        let (x, y, w, h) = fit_rect(1600, 1200);
        assert_eq!((x, w), (0.0, 1600.0));
        assert_eq!(h, 900.0);
        assert_eq!(y, 150.0);
    }

    #[test]
    fn fit_rect_wide_pillarboxes() {
        let (x, _, w, h) = fit_rect(3840, 1080);
        assert_eq!((w, h), (1920.0, 1080.0));
        assert_eq!(x, 960.0);
    }

    #[test]
    fn fit_rect_zero_target_does_not_panic() {
        assert_eq!(fit_rect(0, 0), (0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn short_text_does_not_overflow() {
        assert!(!overflows(&slide("Light rises\nover the hills", Some("Свет встаёт\nнад холмами"))));
    }

    #[test]
    fn many_lines_shrink_and_fit() {
        assert!(!overflows(&slide(&["A line of words"; 7].join("\n"), None)));
    }

    #[test]
    fn huge_text_overflows_not_panics() {
        assert!(overflows(&slide(&["A line of words"; 40].join("\n"), Some("перевод"))));
        assert!(overflows(&slide(&"W".repeat(400), None)));
    }

    #[test]
    fn empty_slide_builds() {
        let mut fonts = Fonts::new();
        let theme = Theme::builtin();
        assert!(!build_scene(&mut fonts, None, &theme, 1920, 1080).overflow);
        assert!(!build_scene(&mut fonts, Some(&Slide::new(vec![])), &theme, 1920, 1080).overflow);
        assert!(!build_scene(&mut fonts, None, &theme, 0, 0).overflow);
    }

    #[test]
    fn unknown_role_is_drawn_like_a_lone_primary() {
        let s = Slide::new(vec![TextBox { role: Role("notes".to_owned()), text: "Hello".to_owned() }]);
        let theme = Theme::builtin();
        let (style, rect) = placement(&theme, &s.boxes[0].role, true);
        assert_eq!(style.map(|s| &s.role), Some(&Role::primary()));
        assert_eq!(rect, theme.solo_primary);
    }

    #[test]
    fn lone_primary_is_centred_but_paired_primary_is_not() {
        let theme = Theme::builtin();
        assert_eq!(placement(&theme, &Role::primary(), true).1, theme.solo_primary);
        assert_eq!(placement(&theme, &Role::primary(), false).1, theme.boxes[0].rect);
        // A lone translation keeps its own place.
        assert_eq!(placement(&theme, &Role::translation(), true).1, theme.boxes[1].rect);
    }

    #[test]
    fn parsed_song_slides_build() {
        let p = parse_song_text("T", "[V]\nA\n--\nБ\n\nC\n");
        let mut fonts = Fonts::new();
        for (_, s) in p.slides() {
            assert!(!build_scene(&mut fonts, Some(s), &Theme::builtin(), 480, 270).overflow);
        }
    }
}
