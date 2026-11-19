use serde::{Deserialize, Serialize};

use crate::types::{Rgba, Role};

/// A rectangle in slide units (the slide is 1920 x 1080).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlideRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// How the text box with one role is placed and drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct BoxStyle {
    pub role: Role,
    pub rect: SlideRect,
    pub font_family: String,
    pub weight: u16,
    pub italic: bool,
    /// Font size in slide units.
    pub size: f32,
    /// Text that does not fit is shrunk no further than this.
    pub min_size: f32,
    pub fill: Rgba,
    pub align: Align,
    pub outline_width: f32,
    pub outline_color: Rgba,
    pub shadow: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub boxes: Vec<BoxStyle>,
    /// Where a lone `primary` box goes when the slide has no other box.
    pub solo_primary: SlideRect,
}

impl Theme {
    /// The theme used until themes can be edited: a large white main language above a smaller
    /// orange translation, both outlined and shadowed.
    pub fn builtin() -> Theme {
        let base = |role: Role, rect: SlideRect, size: f32, min_size: f32, fill: Rgba, outline: f32| BoxStyle {
            role,
            rect,
            font_family: "Helvetica Neue".to_owned(),
            weight: 700,
            italic: false,
            size,
            min_size,
            fill,
            align: Align::Center,
            outline_width: outline,
            outline_color: Rgba::BLACK,
            shadow: true,
        };
        Theme {
            name: "Built-in".to_owned(),
            boxes: vec![
                base(
                    Role::primary(),
                    SlideRect { x: 120.0, y: 170.0, w: 1680.0, h: 400.0 },
                    96.0,
                    40.0,
                    Rgba::WHITE,
                    6.0,
                ),
                base(
                    Role::translation(),
                    SlideRect { x: 120.0, y: 610.0, w: 1680.0, h: 320.0 },
                    64.0,
                    30.0,
                    Rgba::rgb(255, 150, 40),
                    4.0,
                ),
            ],
            solo_primary: SlideRect { x: 120.0, y: 240.0, w: 1680.0, h: 600.0 },
        }
    }

    /// The style a text object is drawn with: the theme's style for its role (the main
    /// language's when it has no role, or one the theme does not know) with the object's own
    /// settings put over it.
    pub fn text_style(&self, role: Option<&Role>, own: &TextStyle) -> Option<BoxStyle> {
        let base = role.and_then(|role| self.style_for(role)).or_else(|| self.style_for(&Role::primary()))?;
        Some(own.over(base))
    }

    pub fn style_for(&self, role: &Role) -> Option<&BoxStyle> {
        self.boxes.iter().find(|b| &b.role == role)
    }
}

/// What a text object sets for itself. Every value that is `None` comes from the theme, so a
/// change to the theme still reaches every object that has not been restyled by hand.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Font size in slide units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_color: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
}

impl TextStyle {
    /// True when nothing is set, so the object looks exactly as the theme has it.
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
    }

    /// A theme style with this object's own settings put over it.
    pub fn over(&self, base: &BoxStyle) -> BoxStyle {
        let size = self.size.unwrap_or(base.size);
        BoxStyle {
            role: base.role.clone(),
            rect: base.rect,
            font_family: self.font_family.clone().unwrap_or_else(|| base.font_family.clone()),
            weight: self.weight.unwrap_or(base.weight),
            italic: self.italic.unwrap_or(base.italic),
            size,
            // A size chosen by hand can be smaller than the theme's floor.
            min_size: base.min_size.min(size),
            fill: self.fill.unwrap_or(base.fill),
            align: self.align.unwrap_or(base.align),
            outline_width: self.outline_width.unwrap_or(base.outline_width),
            outline_color: self.outline_color.unwrap_or(base.outline_color),
            shadow: self.shadow.unwrap_or(base.shadow),
        }
    }

    /// The settings that make `wanted` out of `base`: only what differs is kept.
    pub fn between(base: &BoxStyle, wanted: &BoxStyle) -> TextStyle {
        TextStyle {
            font_family: (wanted.font_family != base.font_family).then(|| wanted.font_family.clone()),
            size: (wanted.size != base.size).then_some(wanted.size),
            weight: (wanted.weight != base.weight).then_some(wanted.weight),
            italic: (wanted.italic != base.italic).then_some(wanted.italic),
            fill: (wanted.fill != base.fill).then_some(wanted.fill),
            align: (wanted.align != base.align).then_some(wanted.align),
            outline_width: (wanted.outline_width != base.outline_width).then_some(wanted.outline_width),
            outline_color: (wanted.outline_color != base.outline_color).then_some(wanted.outline_color),
            shadow: (wanted.shadow != base.shadow).then_some(wanted.shadow),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_style_is_exactly_the_theme() {
        let theme = Theme::builtin();
        let base = &theme.boxes[1];
        assert_eq!(TextStyle::default().over(base), *base);
        assert!(TextStyle::default().is_default());
    }

    #[test]
    fn only_what_was_changed_is_kept_and_the_rest_still_follows_the_theme() {
        let mut theme = Theme::builtin();
        let base = theme.boxes[0].clone();
        let mut wanted = base.clone();
        wanted.fill = Rgba::rgb(255, 0, 0);
        wanted.size = 20.0;
        let own = TextStyle::between(&base, &wanted);
        assert_eq!(own, TextStyle { fill: Some(Rgba::rgb(255, 0, 0)), size: Some(20.0), ..TextStyle::default() });
        assert_eq!(own.over(&base), BoxStyle { min_size: 20.0, ..wanted });
        // The theme changes its font: the object follows, and keeps its own colour and size.
        theme.boxes[0].font_family = "Another".to_owned();
        let drawn = theme.text_style(Some(&Role::primary()), &own).unwrap();
        assert_eq!((drawn.font_family.as_str(), drawn.fill, drawn.size), ("Another", Rgba::rgb(255, 0, 0), 20.0));
    }

    #[test]
    fn an_object_without_a_known_role_uses_the_main_style() {
        let theme = Theme::builtin();
        let none = theme.text_style(None, &TextStyle::default()).unwrap();
        let unknown = theme.text_style(Some(&Role("notes".to_owned())), &TextStyle::default()).unwrap();
        assert_eq!(none, theme.boxes[0]);
        assert_eq!(unknown, theme.boxes[0]);
    }
}
