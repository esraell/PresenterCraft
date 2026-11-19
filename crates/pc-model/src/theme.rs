use crate::types::{Rgba, Role};

/// A rectangle in slide units (the slide is 1920 x 1080).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SlideRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

    pub fn style_for(&self, role: &Role) -> Option<&BoxStyle> {
        self.boxes.iter().find(|b| &b.role == role)
    }
}
