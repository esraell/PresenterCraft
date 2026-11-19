//! Data only: presentations, slides, text boxes, groups and themes, plus the plain-text song
//! parser. No drawing, no windows.

mod format;
mod song_text;
mod theme;
mod types;

pub use format::{FORMAT_VERSION, from_json, to_json};
pub use song_text::{clean_box_text, group_color, parse_song_text, to_song_text};
pub use theme::{Align, BoxStyle, SlideRect, TextStyle, Theme};
pub use types::{Group, MediaItem, Object, ObjectKind, Presentation, Rgba, Role, Slide};
pub use uuid::Uuid;

/// Slides are authored at this size and scaled to the screen.
pub const SLIDE_WIDTH: f32 = 1920.0;
/// See [`SLIDE_WIDTH`].
pub const SLIDE_HEIGHT: f32 = 1080.0;
