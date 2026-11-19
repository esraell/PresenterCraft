//! How one picture gives way to the next on the screens.

use serde::{Deserialize, Serialize};

/// The kinds of transition.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// At once.
    Cut,
    /// The new picture fades in over the old.
    #[default]
    Dissolve,
    /// The old picture fades to black, then the new one fades up.
    FadeBlack,
    /// The same through white.
    FadeWhite,
    /// The new picture pushes the old one off.
    Push,
    /// The new picture slides in over the old one.
    MoveIn,
    /// The old picture slides away, showing the new one under it.
    Reveal,
    /// The new picture is uncovered by an edge sweeping across.
    Wipe,
    /// The new picture is uncovered by a circle growing from the middle.
    Iris,
    /// The new picture grows from the middle as it fades in.
    ZoomIn,
    /// The old picture shrinks away and fades, showing the new one under it.
    ZoomOut,
    /// The new picture is uncovered by a hand sweeping round like a clock's.
    Clock,
    /// The new picture is uncovered in strips, like blinds opening.
    Blinds,
    /// Bars come in from both sides in turn, uncovering the new picture.
    Bars,
    /// The new picture is uncovered square by square, in two passes.
    Checkerboard,
    /// The new picture opens out from a line through the middle.
    Split,
    /// The new picture is uncovered by a diamond growing from the middle.
    Diamond,
    /// The old picture parts in the middle like two doors, showing the new one behind it.
    Doors,
    /// The old picture turns edge-on and the new one turns to face the room.
    Flip,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 19] = [
        TransitionKind::Cut,
        TransitionKind::Dissolve,
        TransitionKind::FadeBlack,
        TransitionKind::FadeWhite,
        TransitionKind::Push,
        TransitionKind::MoveIn,
        TransitionKind::Reveal,
        TransitionKind::ZoomIn,
        TransitionKind::ZoomOut,
        TransitionKind::Doors,
        TransitionKind::Flip,
        TransitionKind::Wipe,
        TransitionKind::Iris,
        TransitionKind::Clock,
        TransitionKind::Blinds,
        TransitionKind::Bars,
        TransitionKind::Checkerboard,
        TransitionKind::Split,
        TransitionKind::Diamond,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Cut => "Cut",
            TransitionKind::Dissolve => "Dissolve",
            TransitionKind::FadeBlack => "Fade Black",
            TransitionKind::FadeWhite => "Fade White",
            TransitionKind::Push => "Push",
            TransitionKind::MoveIn => "Move In",
            TransitionKind::Reveal => "Reveal",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::Iris => "Iris",
            TransitionKind::ZoomIn => "Zoom In",
            TransitionKind::ZoomOut => "Zoom Out",
            TransitionKind::Clock => "Clock",
            TransitionKind::Blinds => "Blinds",
            TransitionKind::Bars => "Bars",
            TransitionKind::Checkerboard => "Checkerboard",
            TransitionKind::Split => "Split",
            TransitionKind::Diamond => "Diamond",
            TransitionKind::Doors => "Doors",
            TransitionKind::Flip => "Flip",
        }
    }

    /// The group it is listed under, as ProPresenter groups its transitions.
    pub fn category(self) -> &'static str {
        match self {
            TransitionKind::Cut => "None",
            TransitionKind::Dissolve | TransitionKind::FadeBlack | TransitionKind::FadeWhite => "Dissolves",
            TransitionKind::Push
            | TransitionKind::MoveIn
            | TransitionKind::Reveal
            | TransitionKind::ZoomIn
            | TransitionKind::ZoomOut
            | TransitionKind::Doors
            | TransitionKind::Flip => "Movements",
            TransitionKind::Wipe
            | TransitionKind::Iris
            | TransitionKind::Clock
            | TransitionKind::Blinds
            | TransitionKind::Bars
            | TransitionKind::Checkerboard
            | TransitionKind::Split
            | TransitionKind::Diamond => "Wipes",
        }
    }

    /// True for the kinds that move in a direction.
    pub fn has_direction(self) -> bool {
        matches!(
            self,
            TransitionKind::Push
                | TransitionKind::MoveIn
                | TransitionKind::Reveal
                | TransitionKind::Wipe
                | TransitionKind::Blinds
                | TransitionKind::Bars
                | TransitionKind::Checkerboard
                | TransitionKind::Split
                | TransitionKind::Doors
                | TransitionKind::Flip
        )
    }

    /// How long it takes unless it is given another time, in seconds.
    pub fn usual_seconds(self) -> f32 {
        match self {
            TransitionKind::Cut => 0.0,
            TransitionKind::Dissolve => 0.4,
            TransitionKind::FadeBlack | TransitionKind::FadeWhite => 1.0,
            _ => 0.6,
        }
    }
}

/// Where a moving transition comes from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    FromLeft,
    FromRight,
    #[default]
    FromTop,
    FromBottom,
}

impl Direction {
    pub const ALL: [Direction; 4] =
        [Direction::FromTop, Direction::FromLeft, Direction::FromRight, Direction::FromBottom];

    pub fn name(self) -> &'static str {
        match self {
            Direction::FromLeft => "From Left",
            Direction::FromRight => "From Right",
            Direction::FromTop => "From Top",
            Direction::FromBottom => "From Bottom",
        }
    }

    /// The way the new picture travels, as a unit step across the screen.
    pub fn travel(self) -> (f64, f64) {
        match self {
            Direction::FromLeft => (1.0, 0.0),
            Direction::FromRight => (-1.0, 0.0),
            Direction::FromTop => (0.0, 1.0),
            Direction::FromBottom => (0.0, -1.0),
        }
    }
}

/// A transition: its kind, the way it moves, and how long it takes.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transition {
    pub kind: TransitionKind,
    #[serde(default)]
    pub direction: Direction,
    pub seconds: f32,
}

impl Default for Transition {
    fn default() -> Transition {
        Transition::of(TransitionKind::Dissolve)
    }
}

impl Transition {
    /// A transition of a kind with its usual direction and time.
    pub fn of(kind: TransitionKind) -> Transition {
        Transition { kind, direction: Direction::default(), seconds: kind.usual_seconds() }
    }

    /// How long it takes; nothing for a cut.
    pub fn duration(self) -> f32 {
        if self.kind == TransitionKind::Cut { 0.0 } else { self.seconds.clamp(0.0, 10.0) }
    }

    /// As the label beside the transition buttons says it: "Push: 0.6s".
    pub fn label(self) -> String {
        format!("{}: {:.1}s", self.kind.name(), self.seconds)
    }
}
