//! Linked text: a text box that shows something that changes, such as the time or a timer,
//! instead of the words typed in it.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "to", rename_all = "snake_case")]
pub enum TextLink {
    /// The time of day.
    Clock,
    /// Today's date.
    Date,
    /// A timer's time, by the timer's id.
    Timer { id: Uuid },
    /// The title of the song on screen.
    SongTitle,
    /// The group of the slide on screen.
    Group,
    /// The label of the slide on screen.
    SlideLabel,
    /// The words of the next slide.
    NextSlide,
    /// The notes of the slide on screen.
    Notes,
    /// The stage message, while it is showing.
    StageMessage,
    /// The song's copyright line.
    Copyright,
}

impl TextLink {
    /// The links that need nothing more to choose, in menu order. A timer is chosen by name.
    pub const SIMPLE: [TextLink; 9] = [
        TextLink::Clock,
        TextLink::Date,
        TextLink::SongTitle,
        TextLink::Group,
        TextLink::SlideLabel,
        TextLink::NextSlide,
        TextLink::Notes,
        TextLink::StageMessage,
        TextLink::Copyright,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TextLink::Clock => "Clock",
            TextLink::Date => "Date",
            TextLink::Timer { .. } => "Timer",
            TextLink::SongTitle => "Song Title",
            TextLink::Group => "Group",
            TextLink::SlideLabel => "Slide Label",
            TextLink::NextSlide => "Next Slide",
            TextLink::Notes => "Slide Notes",
            TextLink::StageMessage => "Stage Message",
            TextLink::Copyright => "Copyright",
        }
    }

    /// True when what it shows changes as time passes, so the screens redraw it each second.
    pub fn ticks(self) -> bool {
        matches!(self, TextLink::Clock | TextLink::Timer { .. })
    }
}
