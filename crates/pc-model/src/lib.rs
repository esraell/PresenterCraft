//! Data only: presentations, slides, text boxes, groups and themes, plus the plain-text song
//! parser. No drawing, no windows.

mod action;
mod build;
mod format;
mod link;
mod song_text;
mod theme;
mod transition;
mod types;

pub use action::{ClearWhat, ShowAction, TimerDo};
pub use build::{Build, BuildClock, BuildEffect};
pub use format::{FORMAT_VERSION, from_json, to_json};
pub use link::TextLink;
pub use song_text::{
    GROUP_KEYS, clean_box_text, group_color, groups_for_key, key_for_group, parse_song_text, to_song_text,
};
pub use theme::{
    Align, BoxStyle, Caps, LINE_HEIGHT, LineFill, LineTransform, ListStyle, Scaling, Scroll, Shadow, SlideRect,
    TextStyle, Theme, ThemeTemplate, VAlign, fonts_used, missing_fonts,
};
pub use transition::{Direction, Transition, TransitionKind};
pub use types::{
    Arrangement, Copyright, Group, ImageFit, MediaItem, Object, ObjectKind, Presentation, Rgba, Role, Shape, Slide,
    StrokeLook,
};
pub use uuid::Uuid;

/// Slides are authored at this size and scaled to the screen.
pub const SLIDE_WIDTH: f32 = 1920.0;
/// See [`SLIDE_WIDTH`].
pub const SLIDE_HEIGHT: f32 = 1080.0;
