//! Actions: things a macro does when it is run, or a slide does when it goes on screen.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// What a Clear action takes off the screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClearWhat {
    All,
    Slide,
    Media,
    Props,
    Messages,
}

impl ClearWhat {
    pub const ALL: [ClearWhat; 5] =
        [ClearWhat::All, ClearWhat::Slide, ClearWhat::Media, ClearWhat::Props, ClearWhat::Messages];

    pub fn name(self) -> &'static str {
        match self {
            ClearWhat::All => "Clear All",
            ClearWhat::Slide => "Clear Slide",
            ClearWhat::Media => "Clear Media",
            ClearWhat::Props => "Clear Props",
            ClearWhat::Messages => "Clear Messages",
        }
    }
}

/// What a Timer action does to its timer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimerDo {
    Start,
    Stop,
    Reset,
}

impl TimerDo {
    pub const ALL: [TimerDo; 3] = [TimerDo::Start, TimerDo::Stop, TimerDo::Reset];

    pub fn name(self) -> &'static str {
        match self {
            TimerDo::Start => "Start",
            TimerDo::Stop => "Stop",
            TimerDo::Reset => "Reset",
        }
    }
}

/// One thing a macro or a slide does. Messages, props and timers are named by their ids, a song
/// by its library key.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "snake_case")]
pub enum ShowAction {
    Clear {
        what: ClearWhat,
    },
    Message {
        id: Uuid,
        show: bool,
    },
    Prop {
        id: Uuid,
        show: bool,
    },
    Timer {
        id: Uuid,
        act: TimerDo,
    },
    Audience {
        on: bool,
    },
    Song {
        key: String,
    },
    /// Puts a picture from the media bin behind the words, by its id.
    Media {
        id: Uuid,
    },
}
