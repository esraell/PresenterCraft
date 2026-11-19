use parley::{Alignment, AlignmentOptions, FontWeight, Layout, LineHeight, PositionedLayoutItem, StyleProperty};
use pc_model::{
    Align, BoxStyle, Object, ObjectKind, Rgba, Role, SLIDE_HEIGHT, SLIDE_WIDTH, Slide, SlideRect, Theme, VAlign,
};
use vello::kurbo::{Affine, Join, Rect, Stroke};
use vello::peniko::{Color, Fill, Mix};
use vello::{Glyph, Scene};

use crate::{Fonts, Picture};

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

/// What is on screen, bottom layer first. A layer that is `None` is clear.
#[derive(Clone, Copy, Default)]
pub struct Layers<'a> {
    /// A picture behind the words. It fills the slide, cropped at the edges if its shape differs.
    pub media: Option<&'a Picture>,
    pub slide: Option<&'a Slide>,
}

/// Draws a slide for a target of the given pixel size: black, with the slide fitted inside.
/// `None` draws black only, which is what a cleared layer looks like.
pub fn build_scene(fonts: &mut Fonts, slide: Option<&Slide>, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    build_layers(fonts, &Layers { media: None, slide }, theme, target_w, target_h)
}

/// Draws every layer for a target of the given pixel size: black, then the media, then the
/// slide's words, all fitted inside the 16:9 slide area.
pub fn build_layers(fonts: &mut Fonts, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    let mut scene = Scene::new();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &whole);
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));

    if let Some(picture) = layers.media {
        let (slide_w, slide_h) = (f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
        let (pic_w, pic_h) = (f64::from(picture.width()), f64::from(picture.height()));
        // Scale the picture to cover the slide, centre it, and cut off what hangs over.
        let cover = (slide_w / pic_w).max(slide_h / pic_h);
        let offset = ((slide_w - pic_w * cover) / 2.0, (slide_h - pic_h * cover) / 2.0);
        scene.push_clip_layer(Fill::NonZero, to_target, &Rect::new(0.0, 0.0, slide_w, slide_h));
        scene.draw_image(&picture.image, to_target * Affine::translate(offset) * Affine::scale(cover));
        scene.pop_layer();
    }

    let mut overflow = false;
    if let Some(slide) = layers.slide {
        let alone = slide.objects.len() == 1;
        for object in &slide.objects {
            let (_, themed) = placement(theme, object.role.as_ref(), alone);
            let rect = object.frame.unwrap_or(themed);
            // Turn the object about its own centre.
            let centre = (f64::from(rect.x + rect.w / 2.0), f64::from(rect.y + rect.h / 2.0));
            let placed = to_target * Affine::rotate_about(f64::from(object.rotation).to_radians(), centre);
            let faded = object.opacity < 1.0;
            if faded {
                let whole_slide = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
                scene.push_layer(Fill::NonZero, Mix::Normal, object.opacity.clamp(0.0, 1.0), to_target, &whole_slide);
            }
            if let Some(fill) = object.fill {
                let (x, y) = (f64::from(rect.x), f64::from(rect.y));
                let filled = Rect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h));
                scene.fill(Fill::NonZero, placed, color(fill), None, &filled);
            }
            match &object.kind {
                ObjectKind::Text { text, style } => {
                    // The theme's style for the object's role, with the object's own settings over it.
                    let Some(style) = theme.text_style(object.role.as_ref(), style) else { continue };
                    overflow |= draw_text_box(&mut scene, fonts, placed, text, &style, rect);
                }
            }
            if faded {
                scene.pop_layer();
            }
        }
    }
    Built { scene, overflow }
}

/// The style and rectangle for a box. A lone main-language box, or a box whose role the theme
/// does not know, is centred on the slide in the main-language style.
fn placement<'a>(theme: &'a Theme, role: Option<&Role>, alone: bool) -> (Option<&'a BoxStyle>, SlideRect) {
    match role.and_then(|role| theme.style_for(role)) {
        Some(style) if alone && role == Some(&Role::primary()) => (Some(style), theme.solo_primary),
        Some(style) => (Some(style), style.rect),
        None => (theme.style_for(&Role::primary()), theme.solo_primary),
    }
}

/// Where an object sits on its slide, in slide units: its own place if it has one, otherwise
/// the theme's place for its role. The editor uses this to find and outline objects.
pub fn object_frame(slide: &Slide, object: &Object, theme: &Theme) -> SlideRect {
    object.frame.unwrap_or_else(|| placement(theme, object.role.as_ref(), slide.objects.len() == 1).1)
}

fn lay_out(fonts: &mut Fonts, text: &str, style: &BoxStyle, size: f32, width: f32) -> Layout<()> {
    let mut builder = fonts.layout_cx.ranged_builder(&mut fonts.font_cx, text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(f32::from(style.weight))));
    if style.italic {
        builder.push_default(StyleProperty::FontStyle(parley::FontStyle::Italic));
    }
    builder.push_default(StyleProperty::Underline(style.underline));
    builder.push_default(StyleProperty::Strikethrough(style.strike));
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
    let text = &*style.caps.apply(text);
    let (layout, overflow) = fit(fonts, text, style, rect);
    // Place the text block in its box (in the middle it may spill both ways when it overflows).
    let spare = f64::from(rect.h) - f64::from(layout.height());
    let top = f64::from(rect.y)
        + match style.valign {
            VAlign::Top => 0.0,
            VAlign::Middle => spare / 2.0,
            VAlign::Bottom => spare,
        };
    let origin = to_target * Affine::translate((f64::from(rect.x), top));
    let outline_width = f64::from(style.outline_width);
    let outline = Stroke::new(outline_width * 2.0).with_join(Join::Round);
    let fill = color(style.fill);
    let outline_color = color(style.outline_color);
    let look = style.shadow_look;
    let fall = f64::from(look.angle).to_radians();
    // The angle is counted anticlockwise, and the slide's y runs downwards.
    let shadow_offset = (f64::from(look.offset) * fall.cos(), -f64::from(look.offset) * fall.sin());
    // Six faint layers add up to the wanted opacity where they all overlap.
    let layer_alpha = 1.0 - (1.0 - look.opacity.clamp(0.0, 1.0)).powf(1.0 / 6.0);
    let spread = f64::from(look.blur.max(0.0)) / 3.0;

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
                        let shadow = origin * Affine::translate(shadow_offset);
                        for step in 1..=6 {
                            let stroke =
                                Stroke::new(outline_width * 2.0 + f64::from(step) * spread).with_join(Join::Round);
                            scene
                                .draw_glyphs(font)
                                .font_size(size)
                                .transform(shadow)
                                .normalized_coords(&coords)
                                .brush(color(look.color))
                                .brush_alpha(layer_alpha)
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
                // Underline and strike-through are bars across the run, in the text's colour.
                if pass == Pass::Fill {
                    let metrics = run.run().font_metrics();
                    let (left, baseline) = (f64::from(run.offset()), f64::from(run.baseline()));
                    let right = left + f64::from(run.advance());
                    let bars = [
                        (style.underline, metrics.underline_offset, metrics.underline_size),
                        (style.strike, metrics.strikethrough_offset, metrics.strikethrough_size),
                    ];
                    for (wanted, offset, thickness) in bars {
                        if wanted {
                            let y = baseline - f64::from(offset);
                            let bar = Rect::new(left, y, right, y + f64::from(thickness.max(1.0)));
                            scene.fill(Fill::NonZero, origin, fill, None, &bar);
                        }
                    }
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
    use pc_model::parse_song_text;

    use super::*;

    fn slide(primary: &str, translation: Option<&str>) -> Slide {
        let mut boxes = vec![Object::text(Role::primary(), primary)];
        if let Some(t) = translation {
            boxes.push(Object::text(Role::translation(), t));
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
    fn every_text_setting_draws() {
        use pc_model::{Caps, TextStyle};
        let mut object = Object::text(Role::primary(), "Light rises\nover the hills");
        object.fill = Some(Rgba { r: 0, g: 0, b: 0, a: 160 });
        let ObjectKind::Text { style, .. } = &mut object.kind;
        *style = TextStyle {
            valign: Some(VAlign::Bottom),
            caps: Some(Caps::AllCaps),
            underline: Some(true),
            strike: Some(true),
            ..TextStyle::default()
        };
        assert!(!overflows(&Slide::new(vec![object])));
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
        let s = Slide::new(vec![Object::text(Role("notes".to_owned()), "Hello")]);
        let theme = Theme::builtin();
        let (style, rect) = placement(&theme, s.objects[0].role.as_ref(), true);
        assert_eq!(style.map(|s| &s.role), Some(&Role::primary()));
        assert_eq!(rect, theme.solo_primary);
    }

    #[test]
    fn lone_primary_is_centred_but_paired_primary_is_not() {
        let theme = Theme::builtin();
        assert_eq!(placement(&theme, Some(&Role::primary()), true).1, theme.solo_primary);
        assert_eq!(placement(&theme, Some(&Role::primary()), false).1, theme.boxes[0].rect);
        // A lone translation keeps its own place.
        assert_eq!(placement(&theme, Some(&Role::translation()), true).1, theme.boxes[1].rect);
    }

    /// A missing letter is drawn as an empty box, which the picture tests could mistake for
    /// text. Glyph 0 is the font's "no such letter" box.
    #[test]
    fn latin_russian_and_ukrainian_letters_all_have_glyphs() {
        let text =
            "ABCXYZ abcxyz 0123456789 АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ абвгдеёжзийклмнопрстуфхцчшщъыьэюя ҐґЄєІіЇї";
        let theme = Theme::builtin();
        let layout = lay_out(&mut Fonts::new(), text, &theme.boxes[0], 40.0, 100_000.0);
        let mut glyphs = 0;
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    for glyph in run.glyphs() {
                        glyphs += 1;
                        assert_ne!(glyph.id, 0, "a letter has no glyph in the font that was chosen");
                    }
                }
            }
        }
        assert!(glyphs >= text.chars().filter(|c| !c.is_whitespace()).count());
    }

    #[test]
    fn an_objects_own_place_wins_over_the_themes() {
        let theme = Theme::builtin();
        let mut s = slide("A", Some("Б"));
        assert_eq!(object_frame(&s, &s.objects[0], &theme), theme.boxes[0].rect);
        let moved = SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 200.0 };
        s.objects[0].frame = Some(moved);
        assert_eq!(object_frame(&s, &s.objects[0], &theme), moved);
        assert_eq!(object_frame(&s, &s.objects[1], &theme), theme.boxes[1].rect);
        // A lone main-language object sits in the theme's centred place.
        let alone = slide("A", None);
        assert_eq!(object_frame(&alone, &alone.objects[0], &theme), theme.solo_primary);
    }

    #[test]
    fn turned_and_faded_objects_build() {
        let mut s = slide("Turned", Some("Faded"));
        s.objects[0].rotation = 30.0;
        s.objects[1].opacity = 0.4;
        assert!(!build_scene(&mut Fonts::new(), Some(&s), &Theme::builtin(), 640, 360).overflow);
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
