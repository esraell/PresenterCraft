//! How one picture gives way to the next on the screens.

use serde::{Deserialize, Serialize};

/// The kinds of transition: the ones that can be drawn without a shader of their own (3D and
/// distortion effects are not built). Kinds saved by an earlier PresenterCraft that are no
/// longer offered are read as the nearest one that is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransitionKind {
    /// At once.
    Cut,
    /// The new picture fades in over the old.
    #[default]
    Dissolve,
    /// The old picture fades out as the new one fades in, darker in the middle.
    Fade,
    /// The old picture fades to black, then the new one fades up.
    FadeBlack,
    /// The same through white.
    FadeWhite,
    /// The same through grey.
    FadeGray,
    /// A dissolve that brightens in the middle.
    FadeBright,
    /// A dissolve that darkens in the middle.
    FadeDark,
    /// The new picture pushes the old one off.
    Push,
    /// The new picture slides in over the old one.
    MoveIn,
    /// The old picture slides away, showing the new one under it.
    Reveal,
    /// The new picture flies in from a side, growing as it comes.
    FlyIn,
    /// The new picture grows from a side (or the middle) as it fades in.
    #[serde(alias = "zoom_out")]
    ZoomIn,
    /// The old picture parts in the middle like two doors, showing the new one behind it.
    #[serde(alias = "doors")]
    Door,
    /// The new picture opens out from a line through the middle, like doors opening on it.
    #[serde(alias = "split")]
    DoorWipe,
    /// The old picture turns edge-on and the new one turns to face the room.
    Flip,
    /// The old picture steps back and away as the new one comes forward from a side.
    Swap,
    /// The new picture is uncovered by an edge sweeping across.
    #[serde(alias = "blinds", alias = "bars")]
    Wipe,
    /// The new picture is uncovered by a square growing from a side.
    SquareWipe,
    /// The new picture is uncovered by a circle growing from the middle.
    #[serde(alias = "clock", alias = "diamond")]
    Iris,
    /// The new picture is uncovered square by square, in no order.
    #[serde(alias = "checkerboard")]
    RandomSquares,
    /// The same with small squares, like pixels.
    RandomPixels,
}

impl TransitionKind {
    pub const ALL: [TransitionKind; 22] = [
        TransitionKind::Cut,
        TransitionKind::Dissolve,
        TransitionKind::Fade,
        TransitionKind::FadeBlack,
        TransitionKind::FadeBright,
        TransitionKind::FadeDark,
        TransitionKind::FadeGray,
        TransitionKind::FadeWhite,
        TransitionKind::Push,
        TransitionKind::MoveIn,
        TransitionKind::Reveal,
        TransitionKind::FlyIn,
        TransitionKind::ZoomIn,
        TransitionKind::Door,
        TransitionKind::Flip,
        TransitionKind::Swap,
        TransitionKind::Wipe,
        TransitionKind::DoorWipe,
        TransitionKind::SquareWipe,
        TransitionKind::Iris,
        TransitionKind::RandomSquares,
        TransitionKind::RandomPixels,
    ];

    pub fn name(self) -> &'static str {
        match self {
            TransitionKind::Cut => "Cut",
            TransitionKind::Dissolve => "Dissolve",
            TransitionKind::Fade => "Fade",
            TransitionKind::FadeBlack => "Fade Black",
            TransitionKind::FadeWhite => "Fade White",
            TransitionKind::FadeGray => "Fade Gray",
            TransitionKind::FadeBright => "Fade Bright",
            TransitionKind::FadeDark => "Fade Dark",
            TransitionKind::Push => "Push",
            TransitionKind::MoveIn => "Move In",
            TransitionKind::Reveal => "Reveal",
            TransitionKind::FlyIn => "Fly In",
            TransitionKind::ZoomIn => "Zoom In",
            TransitionKind::Door => "Door",
            TransitionKind::DoorWipe => "Door Wipe",
            TransitionKind::Flip => "Flip",
            TransitionKind::Swap => "Swap",
            TransitionKind::Wipe => "Wipe",
            TransitionKind::SquareWipe => "Square Wipe",
            TransitionKind::Iris => "Iris",
            TransitionKind::RandomSquares => "Random Squares",
            TransitionKind::RandomPixels => "Random Pixels",
        }
    }

    /// The group it is listed under in the picker.
    pub fn category(self) -> &'static str {
        match self {
            TransitionKind::Cut => "None",
            TransitionKind::Dissolve
            | TransitionKind::Fade
            | TransitionKind::FadeBlack
            | TransitionKind::FadeWhite
            | TransitionKind::FadeGray
            | TransitionKind::FadeBright
            | TransitionKind::FadeDark => "Dissolves",
            TransitionKind::Push
            | TransitionKind::MoveIn
            | TransitionKind::Reveal
            | TransitionKind::FlyIn
            | TransitionKind::ZoomIn => "Movements",
            TransitionKind::Door | TransitionKind::Flip | TransitionKind::Swap => "Objects",
            TransitionKind::Wipe
            | TransitionKind::DoorWipe
            | TransitionKind::SquareWipe
            | TransitionKind::Iris
            | TransitionKind::RandomSquares
            | TransitionKind::RandomPixels => "Wipes",
        }
    }

    /// True for the kinds that move in a direction.
    pub fn has_direction(self) -> bool {
        matches!(
            self,
            TransitionKind::Push
                | TransitionKind::MoveIn
                | TransitionKind::Reveal
                | TransitionKind::FlyIn
                | TransitionKind::ZoomIn
                | TransitionKind::Wipe
                | TransitionKind::SquareWipe
                | TransitionKind::Swap
        )
    }

    /// How long it takes unless it is given another time, in seconds.
    pub fn usual_seconds(self) -> f32 {
        match self {
            TransitionKind::Cut => 0.0,
            TransitionKind::Dissolve => 0.4,
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
