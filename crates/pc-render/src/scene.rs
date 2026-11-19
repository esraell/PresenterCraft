use parley::{Alignment, AlignmentOptions, FontWeight, Layout, LineHeight, PositionedLayoutItem, StyleProperty};
use pc_model::{
    Align, BoxStyle, Object, ObjectKind, Rgba, SLIDE_HEIGHT, SLIDE_WIDTH, Scaling, Shape, Slide, SlideRect, Theme,
    VAlign,
};
use vello::kurbo::{Affine, BezPath, Ellipse, Join, Rect, RoundedRect, Shape as _, Stroke};
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
    /// Words over everything else, such as a message. Its objects carry their own places and
    /// looks.
    pub overlay: Option<&'a Slide>,
}

/// Draws a slide for a target of the given pixel size: black, with the slide fitted inside.
/// `None` draws black only, which is what a cleared layer looks like.
pub fn build_scene(fonts: &mut Fonts, slide: Option<&Slide>, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    build_layers(fonts, &Layers { media: None, slide, overlay: None }, theme, target_w, target_h)
}

/// Draws every layer for a target of the given pixel size: black, then the media, then the
/// slide's words, all fitted inside the 16:9 slide area.
pub fn build_layers(fonts: &mut Fonts, layers: &Layers, theme: &Theme, target_w: u32, target_h: u32) -> Built {
    let mut scene = Scene::new();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &whole);
    let (x, y, w, _) = fit_rect(target_w, target_h);
    let to_target = Affine::translate((x, y)) * Affine::scale(w / f64::from(SLIDE_WIDTH));

    // The theme's background colour fills the slide, under the media. A cleared screen stays black.
    if let (Some(background), Some(_)) = (theme.background, layers.slide) {
        let slide_area = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
        scene.fill(Fill::NonZero, to_target, color(background), None, &slide_area);
    }

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
        overflow |= draw_objects(&mut scene, fonts, to_target, slide, theme);
    }
    if let Some(overlay) = layers.overlay {
        draw_objects(&mut scene, fonts, to_target, overlay, theme);
    }
    Built { scene, overflow }
}

/// Draws a slide's objects. Returns whether any text overflowed its box.
fn draw_objects(scene: &mut Scene, fonts: &mut Fonts, to_target: Affine, slide: &Slide, theme: &Theme) -> bool {
    let mut overflow = false;
    // The slide is drawn with one of the theme's templates: the one it names, or the one
    // that suits what is on it.
    let template = theme.template_for(slide);
    for object in slide.objects.iter().filter(|object| !object.hidden) {
        let Some(template) = template else { continue };
        let rect = object.frame.unwrap_or_else(|| template.place_for(object.role.as_ref()));
        // Turn the object about its own centre.
        let centre = (f64::from(rect.x + rect.w / 2.0), f64::from(rect.y + rect.h / 2.0));
        let mirror = Affine::translate(centre)
            * Affine::scale_non_uniform(if object.flip_h { -1.0 } else { 1.0 }, if object.flip_v { -1.0 } else { 1.0 })
            * Affine::translate((-centre.0, -centre.1));
        let placed = to_target * Affine::rotate_about(f64::from(object.rotation).to_radians(), centre) * mirror;
        let faded = object.opacity < 1.0;
        if faded {
            let whole_slide = Rect::new(0.0, 0.0, f64::from(SLIDE_WIDTH), f64::from(SLIDE_HEIGHT));
            scene.push_layer(Fill::NonZero, Mix::Normal, object.opacity.clamp(0.0, 1.0), to_target, &whole_slide);
        }
        // The object's outline: its shape, or the box a text object sits in.
        let outline = match &object.kind {
            ObjectKind::Shape { shape } => shape_path(*shape, rect),
            ObjectKind::Text { .. } => {
                let (x, y) = (f64::from(rect.x), f64::from(rect.y));
                Rect::new(x, y, x + f64::from(rect.w), y + f64::from(rect.h)).to_path(0.1)
            }
        };
        // A soft shadow under the box or shape: several faint, widening layers, offset.
        if let Some(look) = object.shadow.filter(|look| look.opacity > 0.0) {
            let fall = f64::from(look.angle).to_radians();
            let offset = (f64::from(look.offset) * fall.cos(), -f64::from(look.offset) * fall.sin());
            let layer_alpha = 1.0 - (1.0 - look.opacity.clamp(0.0, 1.0)).powf(1.0 / 6.0);
            let mut shade = look.color;
            shade.a = (f32::from(shade.a) * layer_alpha).round() as u8;
            let shadowed = placed * Affine::translate(offset);
            let spread = f64::from(look.blur.max(0.0)) / 3.0;
            for step in 1..=6 {
                scene.fill(Fill::NonZero, shadowed, color(shade), None, &outline);
                if spread > 0.0 {
                    let edge = Stroke::new(f64::from(step) * spread).with_join(Join::Round);
                    scene.stroke(&edge, shadowed, color(shade), None, &outline);
                }
            }
        }
        if let Some(fill) = object.fill {
            scene.fill(Fill::NonZero, placed, color(fill), None, &outline);
        }
        if let Some(line) = object.stroke.filter(|line| line.width > 0.0) {
            scene.stroke(&Stroke::new(f64::from(line.width)), placed, color(line.color), None, &outline);
        }
        if let ObjectKind::Text { text, style } = &object.kind {
            // The theme's style for the object's role, with the object's own settings over it.
            let Some(style) = template.text_style(object.role.as_ref(), style) else { continue };
            overflow |= draw_text_box(scene, fonts, placed, text, &style, rect);
        }
        if faded {
            scene.pop_layer();
        }
    }
    overflow
}

/// A shape filling a place on the slide, as a path in slide units.
fn shape_path(shape: Shape, rect: SlideRect) -> BezPath {
    let (x, y, w, h) = (f64::from(rect.x), f64::from(rect.y), f64::from(rect.w), f64::from(rect.h));
    let bounds = Rect::new(x, y, x + w, y + h);
    match (shape, shape.corners()) {
        (_, Some(points)) => {
            let mut path = BezPath::new();
            for (index, (px, py)) in points.into_iter().enumerate() {
                let point = (x + px * w, y + py * h);
                if index == 0 {
                    path.move_to(point);
                } else {
                    path.line_to(point);
                }
            }
            path.close_path();
            path
        }
        (Shape::Ellipse, None) => Ellipse::from_rect(bounds).to_path(0.1),
        (_, None) => RoundedRect::from_rect(bounds, w.min(h) * 0.12).to_path(0.1),
    }
}

/// One picture fading into another: `from` with `to` over it, `amount` of the way in (0 is
/// all `from`, 1 is all `to`). Both must have been built for a target of this size.
pub fn cross_fade(from: &Scene, to: &Scene, amount: f32, target_w: u32, target_h: u32) -> Scene {
    let mut scene = from.clone();
    let whole = Rect::new(0.0, 0.0, f64::from(target_w), f64::from(target_h));
    scene.push_layer(Fill::NonZero, Mix::Normal, amount.clamp(0.0, 1.0), Affine::IDENTITY, &whole);
    scene.append(to, None);
    scene.pop_layer();
    scene
}

/// One picture giving way to another by a transition, `amount` of the way through (0 is all
/// `from`, 1 is all `to`). Both must have been built for a target of this size.
pub fn transition(
    kind: pc_model::TransitionKind,
    direction: pc_model::Direction,
    from: &Scene,
    to: &Scene,
    amount: f32,
    target_w: u32,
    target_h: u32,
) -> Scene {
    use pc_model::TransitionKind as K;
    let a = f64::from(amount.clamp(0.0, 1.0));
    let (w, h) = (f64::from(target_w), f64::from(target_h));
    let whole = Rect::new(0.0, 0.0, w, h);
    let (dx, dy) = direction.travel();
    // Where the new picture starts, off the screen on the side it comes from.
    let away = (-dx * w, -dy * h);
    let mut scene = Scene::new();
    scene.fill(Fill::NonZero, Affine::IDENTITY, Color::BLACK, None, &whole);
    match kind {
        K::Cut => scene.append(to, None),
        K::Dissolve => return cross_fade(from, to, amount, target_w, target_h),
        K::FadeBlack | K::FadeWhite => {
            let through = if kind == K::FadeBlack { Color::BLACK } else { Color::WHITE };
            if a < 0.5 {
                scene.append(from, None);
                scene.push_layer(Fill::NonZero, Mix::Normal, (a * 2.0) as f32, Affine::IDENTITY, &whole);
                scene.fill(Fill::NonZero, Affine::IDENTITY, through, None, &whole);
            } else {
                scene.fill(Fill::NonZero, Affine::IDENTITY, through, None, &whole);
                scene.push_layer(Fill::NonZero, Mix::Normal, ((a - 0.5) * 2.0) as f32, Affine::IDENTITY, &whole);
                scene.append(to, None);
            }
            scene.pop_layer();
        }
        K::Push => {
            scene.append(from, Some(Affine::translate((dx * w * a, dy * h * a))));
            scene.append(to, Some(Affine::translate((away.0 * (1.0 - a), away.1 * (1.0 - a)))));
        }
        K::MoveIn => {
            scene.append(from, None);
            scene.append(to, Some(Affine::translate((away.0 * (1.0 - a), away.1 * (1.0 - a)))));
        }
        K::Reveal => {
            scene.append(to, None);
            scene.append(from, Some(Affine::translate((dx * w * a, dy * h * a))));
        }
        K::Wipe => {
            scene.append(from, None);
            // The edge sweeps the way the new picture travels.
            let uncovered = match direction {
                pc_model::Direction::FromLeft => Rect::new(0.0, 0.0, w * a, h),
                pc_model::Direction::FromRight => Rect::new(w * (1.0 - a), 0.0, w, h),
                pc_model::Direction::FromTop => Rect::new(0.0, 0.0, w, h * a),
                pc_model::Direction::FromBottom => Rect::new(0.0, h * (1.0 - a), w, h),
            };
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &uncovered);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::Iris => {
            scene.append(from, None);
            let reach = (w * w + h * h).sqrt() / 2.0;
            let circle = Ellipse::new((w / 2.0, h / 2.0), (reach * a, reach * a), 0.0);
            scene.push_clip_layer(Fill::NonZero, Affine::IDENTITY, &circle);
            scene.append(to, None);
            scene.pop_layer();
        }
        K::ZoomIn => {
            scene.append(from, None);
            let grow = Affine::translate((w / 2.0, h / 2.0))
                * Affine::scale(0.6 + 0.4 * a)
                * Affine::translate((-w / 2.0, -h / 2.0));
            scene.push_layer(Fill::NonZero, Mix::Normal, a as f32, Affine::IDENTITY, &whole);
            scene.append(to, Some(grow));
            scene.pop_layer();
        }
    }
    scene
}

/// Where an object sits on its slide, in slide units: its own place if it has one, otherwise
/// the theme's place for its role. The editor uses this to find and outline objects.
pub fn object_frame(slide: &Slide, object: &Object, theme: &Theme) -> SlideRect {
    object.frame.unwrap_or_else(|| theme.place_on(slide, object.role.as_ref()))
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
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(style.line_height.clamp(0.5, 4.0))));
    // Letter spacing is set at the box's font size; text shrunk to fit keeps it in proportion.
    if style.letter_spacing != 0.0 {
        builder.push_default(StyleProperty::LetterSpacing(style.letter_spacing * size / style.size.max(1.0)));
    }
    builder.push_default(StyleProperty::FontFamily(style.font_family.as_str().into()));
    let mut layout: Layout<()> = builder.build(text);
    let alignment = match style.align {
        Align::Left => Alignment::Left,
        Align::Center => Alignment::Center,
        Align::Right => Alignment::Right,
        Align::Justify => Alignment::Justify,
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
    let too_big = |layout: &Layout<()>| layout.height() > rect.h || layout.width() > rect.w + 0.5;
    let (shrinks, grows) = match style.scaling {
        Scaling::None => (false, false),
        Scaling::TextDown => (true, false),
        Scaling::TextUp => (false, true),
        Scaling::TextUpOrDown => (true, true),
    };
    let floor = style.min_size.min(style.size).max(1.0);
    let mut size = style.size.max(floor);
    let mut layout = lay_out(fonts, text, style, size, rect.w);
    if too_big(&layout) {
        while shrinks && too_big(&layout) && size > floor {
            size = (size * 0.96).max(floor);
            layout = lay_out(fonts, text, style, size, rect.w);
        }
    } else if grows && !text.trim().is_empty() {
        // Grow in small steps and keep the last size that still fitted.
        while size < LARGEST_GROWN {
            let bigger = lay_out(fonts, text, style, size * 1.04, rect.w);
            if too_big(&bigger) {
                break;
            }
            size *= 1.04;
            layout = bigger;
        }
    }
    let overflow = too_big(&layout);
    (layout, overflow)
}

/// Text made bigger to fill its box stops growing here, in slide units.
const LARGEST_GROWN: f32 = 1200.0;

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
    let text = &*style.line_transform.apply(text);
    let text = &*style.caps.apply(text);
    // The text is laid out inside the box's margins.
    let [left, top, right, bottom] = style.margins.map(|margin| margin.max(0.0));
    let rect = SlideRect {
        x: rect.x + left,
        y: rect.y + top,
        w: (rect.w - left - right).max(1.0),
        h: (rect.h - top - bottom).max(1.0),
    };
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
    use pc_model::Role;

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
        let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
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
    fn scaling_decides_whether_text_shrinks_grows_or_spills() {
        use pc_model::TextStyle;
        let with = |text: &str, scaling: Scaling| {
            let mut object = Object::text(Role::primary(), text);
            let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
            *style = TextStyle { scaling: Some(scaling), ..TextStyle::default() };
            Slide::new(vec![object])
        };
        let long = ["A line of words"; 9].join("\n");
        assert!(!overflows(&with(&long, Scaling::TextDown)));
        assert!(overflows(&with(&long, Scaling::None)));
        assert!(overflows(&with(&long, Scaling::TextUp)));
        assert!(!overflows(&with(&long, Scaling::TextUpOrDown)));
        assert!(!overflows(&with("Short", Scaling::TextUp)));
    }

    #[test]
    fn a_hidden_object_is_not_drawn() {
        let mut slide = slide(&["A line of words"; 40].join("\n"), None);
        assert!(overflows(&slide));
        slide.objects[0].hidden = true;
        assert!(!overflows(&slide));
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
    fn a_role_the_theme_does_not_know_is_placed_like_the_main_language() {
        let s = Slide::new(vec![Object::text(Role("notes".to_owned()), "Hello")]);
        let theme = Theme::builtin();
        assert_eq!(object_frame(&s, &s.objects[0], &theme), theme.place_on(&s, Some(&Role::primary())));
    }

    /// A missing letter is drawn as an empty box, which the picture tests could mistake for
    /// text. Glyph 0 is the font's "no such letter" box.
    #[test]
    fn latin_russian_and_ukrainian_letters_all_have_glyphs() {
        let text =
            "ABCXYZ abcxyz 0123456789 АБВГДЕЁЖЗИЙКЛМНОПРСТУФХЦЧШЩЪЫЬЭЮЯ абвгдеёжзийклмнопрстуфхцчшщъыьэюя ҐґЄєІіЇї";
        let theme = Theme::builtin();
        let layout = lay_out(&mut Fonts::new(), text, &theme.templates[0].boxes[0], 40.0, 100_000.0);
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
        assert_eq!(object_frame(&s, &s.objects[0], &theme), theme.templates[0].boxes[0].rect);
        let moved = SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 200.0 };
        s.objects[0].frame = Some(moved);
        assert_eq!(object_frame(&s, &s.objects[0], &theme), moved);
        assert_eq!(object_frame(&s, &s.objects[1], &theme), theme.templates[0].boxes[1].rect);
        // A lone main-language object sits in the theme's centred place.
        let alone = slide("A", None);
        assert_eq!(object_frame(&alone, &alone.objects[0], &theme), theme.templates[1].boxes[0].rect);
    }

    #[test]
    fn spacing_widens_letters_spreads_lines_and_margins_squeeze_the_text() {
        let mut fonts = Fonts::new();
        let theme = Theme::builtin();
        let base = theme.templates[0].boxes[0].clone();
        let text = "Light rises\nover the hills";
        let plain = lay_out(&mut fonts, text, &base, 60.0, 10_000.0);
        let spaced = BoxStyle { letter_spacing: 10.0, ..base.clone() };
        assert!(lay_out(&mut fonts, text, &spaced, 60.0, 10_000.0).width() > plain.width() + 50.0);
        let tall = BoxStyle { line_height: 2.0, ..base.clone() };
        assert!(lay_out(&mut fonts, text, &tall, 60.0, 10_000.0).height() > plain.height() * 1.4);
        // Text that fits its box no longer fits once wide margins are kept clear inside it.
        let fixed = |margins| {
            let look = BoxStyle { margins, scaling: Scaling::None, ..base.clone() };
            let mut object = Object::text(Role::primary(), "Light rises over the hills");
            let ObjectKind::Text { style, .. } = &mut object.kind else { panic!("a text object") };
            *style = pc_model::TextStyle::all(&look);
            object.frame = Some(SlideRect { x: 100.0, y: 100.0, w: 1700.0, h: 300.0 });
            build_scene(&mut Fonts::new(), Some(&Slide::new(vec![object])), &theme, 1920, 1080).overflow
        };
        assert!(!fixed([0.0; 4]));
        assert!(fixed([600.0, 120.0, 600.0, 120.0]));
    }

    #[test]
    fn every_shape_is_drawn_inside_its_place() {
        for shape in Shape::ALL {
            let rect = SlideRect { x: 100.0, y: 200.0, w: 400.0, h: 300.0 };
            let bounds = shape_path(shape, rect).bounding_box();
            assert!(bounds.width() > 300.0 && bounds.height() > 200.0, "{shape:?} is too small: {bounds:?}");
            assert!(bounds.x0 >= 99.9 && bounds.y0 >= 199.9 && bounds.x1 <= 500.1 && bounds.y1 <= 500.1, "{shape:?}");
        }
        // A slide with a shape and a stroked text box builds.
        let mut boxed = Object::text(Role::primary(), "Words");
        boxed.stroke = Some(pc_model::StrokeLook { width: 6.0, color: Rgba::WHITE });
        let mut star = Object::shape(Shape::Star);
        star.shadow = Some(pc_model::Shadow::default());
        star.flip_h = true;
        let slide = Slide::new(vec![star, boxed]);
        assert!(!build_scene(&mut Fonts::new(), Some(&slide), &Theme::builtin(), 1920, 1080).overflow);
    }

    #[test]
    fn every_transition_builds_at_every_stage() {
        let theme = Theme::builtin();
        let mut fonts = Fonts::new();
        let from = build_scene(&mut fonts, Some(&slide("Old", None)), &theme, 640, 360).scene;
        let to = build_scene(&mut fonts, Some(&slide("New", None)), &theme, 640, 360).scene;
        for kind in pc_model::TransitionKind::ALL {
            for direction in pc_model::Direction::ALL {
                for amount in [0.0, 0.3, 0.5, 0.7, 1.0] {
                    let _ = transition(kind, direction, &from, &to, amount, 640, 360);
                }
            }
        }
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
