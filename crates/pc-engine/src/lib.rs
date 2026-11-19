//! The show: what is live, which song is selected, and the commands that change it.
//!
//! Headless and deterministic. A [`Command`] goes in; the new state and a [`Changes`] report
//! come out. Nothing here knows about windows or drawing.

use std::sync::Arc;

use pc_model::{Presentation, Slide};

#[cfg(test)]
mod tests;

/// Output is a stack of layers, each cleared on its own. Only `Slides` carries content so far.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Audio,
    Media,
    Slides,
    Announcements,
    Props,
    Messages,
    Masks,
}

/// A slide, by the index of its presentation in the show and its flat index in that
/// presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlideRef {
    pub presentation: usize,
    pub slide: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// Show another presentation's slides to the operator. Never changes what is live.
    Select(usize),
    GoTo(SlideRef),
    Next,
    Previous,
    /// The first slide of the selected presentation.
    First,
    /// The last slide of the selected presentation.
    Last,
    Clear(Layer),
    ClearAll,
    SetOutputEnabled(bool),
}

/// What a command changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// The slide layer shows something different.
    pub slides: bool,
    pub selection: bool,
    pub output_enabled: bool,
}

pub struct Show {
    presentations: Vec<Arc<Presentation>>,
    selected: usize,
    live: Option<SlideRef>,
    cursor: Option<SlideRef>,
    output_enabled: bool,
}

impl Show {
    /// Starts with the first presentation selected, nothing live and the output on.
    pub fn new(presentations: Vec<Arc<Presentation>>) -> Show {
        Show { presentations, selected: 0, live: None, cursor: None, output_enabled: true }
    }

    pub fn presentations(&self) -> &[Arc<Presentation>] {
        &self.presentations
    }

    pub fn selected(&self) -> usize {
        self.selected
    }

    pub fn live(&self) -> Option<SlideRef> {
        self.live
    }

    /// The last slide that was live. Kept after a clear so `Next` can continue from it.
    pub fn cursor(&self) -> Option<SlideRef> {
        self.cursor
    }

    pub fn live_slide(&self) -> Option<(&Arc<Presentation>, &Slide)> {
        let at = self.live?;
        let presentation = self.presentations.get(at.presentation)?;
        let (_, slide) = presentation.slide(at.slide)?;
        Some((presentation, slide))
    }

    pub fn output_enabled(&self) -> bool {
        self.output_enabled
    }

    pub fn apply(&mut self, command: Command) -> Changes {
        match command {
            Command::Select(index) => {
                if index < self.presentations.len() && index != self.selected {
                    self.selected = index;
                    return Changes { selection: true, ..Changes::default() };
                }
                Changes::default()
            }
            Command::GoTo(target) => self.go_to(Some(target)),
            Command::Next => self.go_to(self.next_target()),
            Command::Previous => self.go_to(self.previous_target()),
            Command::First => self.go_to(self.edge_of_selected(false)),
            Command::Last => self.go_to(self.edge_of_selected(true)),
            Command::Clear(Layer::Slides) | Command::ClearAll => {
                let was_live = self.live.take().is_some();
                Changes { slides: was_live, ..Changes::default() }
            }
            Command::Clear(_) => Changes::default(),
            Command::SetOutputEnabled(enabled) => {
                let changed = enabled != self.output_enabled;
                self.output_enabled = enabled;
                Changes { output_enabled: changed, ..Changes::default() }
            }
        }
    }

    fn slide_count(&self, presentation: usize) -> usize {
        self.presentations.get(presentation).map_or(0, |p| p.slide_count())
    }

    /// Makes `target` live if it names a real slide that is not already live.
    fn go_to(&mut self, target: Option<SlideRef>) -> Changes {
        let Some(target) = target else {
            return Changes::default();
        };
        if target.slide >= self.slide_count(target.presentation) || self.live == Some(target) {
            return Changes::default();
        }
        self.live = Some(target);
        self.cursor = Some(target);
        Changes { slides: true, ..Changes::default() }
    }

    fn next_target(&self) -> Option<SlideRef> {
        match self.cursor {
            Some(cursor) => {
                let last = self.slide_count(cursor.presentation).checked_sub(1)?;
                Some(SlideRef { slide: (cursor.slide + 1).min(last), ..cursor })
            }
            None => self.edge_of_selected(false),
        }
    }

    fn previous_target(&self) -> Option<SlideRef> {
        let cursor = self.cursor?;
        Some(SlideRef { slide: cursor.slide.saturating_sub(1), ..cursor })
    }

    fn edge_of_selected(&self, last: bool) -> Option<SlideRef> {
        let count = self.slide_count(self.selected);
        let slide = if last { count.checked_sub(1)? } else { 0 };
        (count > 0).then_some(SlideRef { presentation: self.selected, slide })
    }
}
