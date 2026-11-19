//! The show: what is live, which song is selected, and the commands that change it.
//!
//! Headless and deterministic. A [`Command`] goes in; the new state and a [`Changes`] report
//! come out. Nothing here knows about windows or drawing.

use std::sync::Arc;

use pc_model::{MediaItem, Presentation, Slide};

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
    /// Show the media item with this index behind the words.
    ShowMedia(usize),
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
    /// The media layer shows something different.
    pub media: bool,
    pub selection: bool,
    pub output_enabled: bool,
}

pub struct Show {
    presentations: Vec<Arc<Presentation>>,
    selected: usize,
    live: Option<SlideRef>,
    cursor: Option<SlideRef>,
    media: Vec<MediaItem>,
    live_media: Option<usize>,
    output_enabled: bool,
}

impl Show {
    /// Starts with the first presentation selected, nothing live and the output on.
    pub fn new(presentations: Vec<Arc<Presentation>>) -> Show {
        Show {
            presentations,
            selected: 0,
            live: None,
            cursor: None,
            media: Vec::new(),
            live_media: None,
            output_enabled: true,
        }
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

    /// The media that can be shown, in the order it was added.
    pub fn media(&self) -> &[MediaItem] {
        &self.media
    }

    /// The index of the media item on screen, if any.
    pub fn live_media(&self) -> Option<usize> {
        self.live_media
    }

    /// Adds a presentation after the others and returns its index. Nothing on screen changes.
    pub fn add_presentation(&mut self, presentation: Arc<Presentation>) -> usize {
        self.presentations.push(presentation);
        self.presentations.len() - 1
    }

    /// Adds a media item to the end of the list and returns its index. It is not shown.
    pub fn add_media(&mut self, item: MediaItem) -> usize {
        self.media.push(item);
        self.media.len() - 1
    }

    /// The first slide of the first group, in the presentation in use, whose name is one of
    /// those given (in lower case). The presentation in use is the one with the slide last on
    /// screen, or the selected one if nothing has been shown.
    pub fn group_start(&self, names: &[&str]) -> Option<SlideRef> {
        let presentation = self.live.or(self.cursor).map_or(self.selected, |at| at.presentation);
        let song = self.presentations.get(presentation)?;
        let mut slide = 0;
        for group in &song.groups {
            // The group's first slide that is not disabled.
            if names.contains(&group.name.trim().to_lowercase().as_str())
                && let Some(skip) = group.slides.iter().position(|one| one.enabled)
            {
                return Some(SlideRef { presentation, slide: slide + skip });
            }
            slide += group.slides.len();
        }
        None
    }

    /// True when any layer has something on screen.
    pub fn anything_live(&self) -> bool {
        self.live.is_some() || self.live_media.is_some()
    }

    /// Puts an edited presentation in place of the one at `index`. A live slide stays live if
    /// the new presentation still has a slide at that position; otherwise the slide layer is
    /// cleared. Reports a slide change when the presentation was the live one.
    pub fn replace_presentation(&mut self, index: usize, presentation: Arc<Presentation>) -> Changes {
        let Some(slot) = self.presentations.get_mut(index) else { return Changes::default() };
        let count = presentation.slide_count();
        *slot = presentation;
        if self.cursor.is_some_and(|c| c.presentation == index && c.slide >= count) {
            self.cursor = None;
        }
        match self.live {
            Some(live) if live.presentation == index => {
                if live.slide >= count {
                    self.live = None;
                }
                Changes { slides: true, ..Changes::default() }
            }
            _ => Changes::default(),
        }
    }

    /// Keeps what is live and the cursor on the same slides after a slide was removed from, or
    /// added to, a song: `at` is where, `added` says which. A removed slide that was live is
    /// taken off the screen.
    pub fn slides_shifted(&mut self, presentation: usize, at: usize, added: bool) -> Changes {
        let shift = |place: &mut Option<SlideRef>| {
            if let Some(here) = place.as_mut().filter(|here| here.presentation == presentation) {
                if added && here.slide >= at {
                    here.slide += 1;
                } else if !added && here.slide > at {
                    here.slide -= 1;
                } else if !added && here.slide == at {
                    *place = None;
                }
            }
        };
        let was = self.live;
        shift(&mut self.live);
        shift(&mut self.cursor);
        Changes { slides: was.is_some() && self.live.is_none(), ..Changes::default() }
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
            Command::ShowMedia(index) => {
                if index < self.media.len() && self.live_media != Some(index) {
                    self.live_media = Some(index);
                    return Changes { media: true, ..Changes::default() };
                }
                Changes::default()
            }
            Command::Clear(Layer::Slides) => Changes { slides: self.live.take().is_some(), ..Changes::default() },
            Command::Clear(Layer::Media) => Changes { media: self.live_media.take().is_some(), ..Changes::default() },
            Command::ClearAll => Changes {
                slides: self.live.take().is_some(),
                media: self.live_media.take().is_some(),
                ..Changes::default()
            },
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
        if target.slide >= self.slide_count(target.presentation) || self.live == Some(target) || !self.enabled(target) {
            return Changes::default();
        }
        self.live = Some(target);
        self.cursor = Some(target);
        Changes { slides: true, ..Changes::default() }
    }

    /// False for a slide that has been disabled: it stays in its song but is never shown.
    fn enabled(&self, at: SlideRef) -> bool {
        self.presentations
            .get(at.presentation)
            .and_then(|song| song.slide(at.slide))
            .is_some_and(|(_, slide)| slide.enabled)
    }

    /// The next slide after the cursor that is not disabled. At the end of the song it stays
    /// where it is, so after a clear the last slide can be shown again.
    fn next_target(&self) -> Option<SlideRef> {
        match self.cursor {
            Some(cursor) => (cursor.slide + 1..self.slide_count(cursor.presentation))
                .map(|slide| SlideRef { slide, ..cursor })
                .find(|at| self.enabled(*at))
                .or(Some(cursor)),
            None => self.edge_of_selected(false),
        }
    }

    /// The slide before the cursor that is not disabled; at the start of the song it stays.
    fn previous_target(&self) -> Option<SlideRef> {
        let cursor = self.cursor?;
        (0..cursor.slide).rev().map(|slide| SlideRef { slide, ..cursor }).find(|at| self.enabled(*at)).or(Some(cursor))
    }

    /// The selected song's first (or last) slide that is not disabled.
    fn edge_of_selected(&self, last: bool) -> Option<SlideRef> {
        let at = |slide| SlideRef { presentation: self.selected, slide };
        let count = self.slide_count(self.selected);
        if last {
            (0..count).rev().map(at).find(|at| self.enabled(*at))
        } else {
            (0..count).map(at).find(|at| self.enabled(*at))
        }
    }
}
