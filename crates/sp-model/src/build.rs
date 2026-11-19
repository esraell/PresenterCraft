//! Builds: an object that comes onto its slide, or leaves it, with a movement of its own,
//! when the slide goes on screen or when the operator clicks Next.

use serde::{Deserialize, Serialize};

use crate::{Direction, Object, SLIDE_HEIGHT, SLIDE_WIDTH, Slide, SlideRect, Theme};

/// How an object comes on or goes off.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuildEffect {
    /// Fades in, or out.
    Dissolve,
    /// Slides in from a side of the screen, or out to it.
    Fly,
    /// Grows from small as it fades in, or shrinks as it fades out.
    Zoom,
}

impl BuildEffect {
    pub const ALL: [BuildEffect; 3] = [BuildEffect::Dissolve, BuildEffect::Fly, BuildEffect::Zoom];

    /// The effect's name, coming in or going out.
    pub fn name(self, coming_in: bool) -> &'static str {
        match (self, coming_in) {
            (BuildEffect::Dissolve, _) => "Dissolve",
            (BuildEffect::Fly, true) => "Fly In",
            (BuildEffect::Fly, false) => "Fly Out",
            (BuildEffect::Zoom, _) => "Zoom",
        }
    }
}

/// One build of an object: how, from or to which side, how long, and when.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Build {
    pub effect: BuildEffect,
    /// For Fly: the side it comes from, or goes to.
    #[serde(default)]
    pub direction: Direction,
    pub seconds: f32,
    /// Seconds to wait before it starts.
    #[serde(default)]
    pub delay: f32,
    /// A build in waits for a click on Next instead of starting with the slide. A build out
    /// always waits for a click.
    #[serde(default)]
    pub on_click: bool,
    /// For a build in of text: one paragraph at a time (one each click, or one after another
    /// with the slide) instead of all at once.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_paragraph: bool,
}

impl Build {
    pub fn of(effect: BuildEffect) -> Build {
        Build { effect, direction: Direction::FromLeft, seconds: 0.6, delay: 0.0, on_click: false, by_paragraph: false }
    }

    /// How far along it is, from 0 to 1, this long after it was set going; eased to start and
    /// end gently.
    fn progress(&self, elapsed: f32) -> f32 {
        let t = if self.seconds <= 0.0 { 1.0 } else { ((elapsed - self.delay) / self.seconds).clamp(0.0, 1.0) };
        if elapsed < self.delay {
            return 0.0;
        }
        t * t * (3.0 - 2.0 * t)
    }

    /// True while it is still waiting or moving, this long after it was set going.
    fn under_way(&self, elapsed: f32) -> bool {
        elapsed < self.delay + self.seconds.max(0.0)
    }
}

/// Where the slide is in its builds: which click it is on, and how long ago the slide was
/// fully on screen and the last click came. `since_on` is `None` while the slide is still
/// transitioning on, so builds that start with the slide wait for it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BuildClock {
    pub step: usize,
    pub since_on: Option<f32>,
    pub since_step: f32,
}

impl BuildClock {
    /// Every build done up to this click, with nothing moving.
    pub fn settled(step: usize) -> BuildClock {
        BuildClock { step, since_on: Some(f32::MAX), since_step: f32::MAX }
    }
}

impl Slide {
    /// The builds that wait for a click on this slide, in the order clicks set them going: each
    /// object's build in, in the objects' order, then each build out. Each is the object's
    /// place among the objects, and true for a build in.
    /// A text build delivered paragraph by paragraph takes a click for each paragraph.
    pub fn click_steps(&self) -> Vec<(usize, bool)> {
        let ins = self.objects.iter().enumerate().filter(|(_, o)| o.build_in.is_some_and(|b| b.on_click));
        let outs = self.objects.iter().enumerate().filter(|(_, o)| o.build_out.is_some());
        ins.flat_map(|(at, object)| std::iter::repeat_n((at, true), clicks_to_build(object)))
            .chain(outs.map(|(at, _)| (at, false)))
            .collect()
    }

    /// The slide as it looks at this point in its builds: objects not yet built in are hidden,
    /// ones built out are gone, and ones on their way are faded, moved or sized part-way.
    /// Also says whether anything is still moving (or waiting to), so it is drawn again.
    pub fn built(&self, theme: &Theme, clock: BuildClock) -> (Slide, bool) {
        let steps = self.click_steps();
        let has_builds = self.objects.iter().any(|o| o.build_in.is_some() || o.build_out.is_some());
        if !has_builds {
            return (self.clone(), false);
        }
        let template = theme.template_for(self);
        let mut moving = false;
        let mut slide = self.clone();
        for (at, object) in slide.objects.iter_mut().enumerate() {
            // How much of the object shows, from 0 (none) to 1 (all), and the build doing it.
            let mut shown: Option<(f32, Build)> = None;
            // Text delivered paragraph by paragraph: as many paragraphs show as clicks (or
            // turns of its time, with the slide) have reached.
            if let Some(build) = object.build_in.filter(|build| build.by_paragraph)
                && let Some(count) = paragraphs(object)
            {
                let reached = if build.on_click {
                    steps.iter().take(clock.step).filter(|step| **step == (at, true)).count()
                } else {
                    clock.since_on.map_or(0, |since| {
                        let each = build.seconds.max(0.1);
                        if since < build.delay { 0 } else { ((since - build.delay) / each) as usize + 1 }
                    })
                };
                let reached = reached.min(count);
                moving |= !build.on_click && clock.since_on.is_some() && reached < count;
                moving |= !build.on_click && clock.since_on.is_none();
                if reached == 0 {
                    object.hidden = true;
                } else if reached < count {
                    object.shown_paragraphs = Some(reached);
                }
                continue;
            }
            if let Some(build) = object.build_in {
                let elapsed = if build.on_click {
                    let place = steps.iter().position(|step| *step == (at, true));
                    match place {
                        Some(place) if place + 1 < clock.step => Some(f32::MAX),
                        Some(place) if place + 1 == clock.step => Some(clock.since_step),
                        _ => None,
                    }
                } else {
                    clock.since_on
                };
                let amount = elapsed.map_or(0.0, |elapsed| build.progress(elapsed));
                moving |= elapsed.is_some_and(|elapsed| build.under_way(elapsed));
                shown = Some((amount, build));
            }
            if let Some(build) = object.build_out {
                let place = steps.iter().position(|step| *step == (at, false));
                let elapsed = match place {
                    Some(place) if place + 1 < clock.step => Some(f32::MAX),
                    Some(place) if place + 1 == clock.step => Some(clock.since_step),
                    _ => None,
                };
                if let Some(elapsed) = elapsed {
                    moving |= build.under_way(elapsed);
                    let left = 1.0 - build.progress(elapsed);
                    let before = shown.map_or(1.0, |(amount, _)| amount);
                    shown = Some((left.min(before), build));
                }
            }
            let Some((amount, build)) = shown else { continue };
            let rect = object.frame.or_else(|| template.map(|t| t.place_for(object.role.as_ref())));
            apply(object, build, amount, rect);
        }
        (slide, moving)
    }
}

/// How many paragraphs a text object built paragraph by paragraph has: one more than its line
/// breaks. `None` for an object that isn't text.
fn paragraphs(object: &Object) -> Option<usize> {
    object.as_text().map(|text| text.trim_end().matches('\n').count() + 1)
}

/// How many clicks an object's build in takes: one, or one for each paragraph.
fn clicks_to_build(object: &Object) -> usize {
    match object.build_in {
        Some(build) if build.by_paragraph => paragraphs(object).unwrap_or(1),
        _ => 1,
    }
}

/// Shows an object part-way through a build: `amount` 1 is all there, 0 is gone.
fn apply(object: &mut Object, build: Build, amount: f32, rect: Option<SlideRect>) {
    if amount <= 0.0 {
        object.hidden = true;
        return;
    }
    if amount >= 1.0 {
        return;
    }
    match (build.effect, rect) {
        (BuildEffect::Fly, Some(rect)) => {
            let away = 1.0 - amount;
            let moved = match build.direction {
                Direction::FromLeft => SlideRect { x: rect.x - (rect.x + rect.w) * away, ..rect },
                Direction::FromRight => SlideRect { x: rect.x + (SLIDE_WIDTH - rect.x) * away, ..rect },
                Direction::FromTop => SlideRect { y: rect.y - (rect.y + rect.h) * away, ..rect },
                Direction::FromBottom => SlideRect { y: rect.y + (SLIDE_HEIGHT - rect.y) * away, ..rect },
            };
            object.frame = Some(moved);
        }
        (BuildEffect::Zoom, Some(rect)) => {
            let scale = 0.3 + 0.7 * amount;
            let (w, h) = (rect.w * scale, rect.h * scale);
            object.frame = Some(SlideRect { x: rect.x + (rect.w - w) / 2.0, y: rect.y + (rect.h - h) / 2.0, w, h });
            object.opacity *= amount;
        }
        _ => object.opacity *= amount,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Role, Shape};

    fn shape_with(build_in: Option<Build>, build_out: Option<Build>) -> Object {
        let mut object = Object::shape(Shape::Rectangle);
        object.frame = Some(SlideRect { x: 100.0, y: 100.0, w: 200.0, h: 100.0 });
        object.build_in = build_in;
        object.build_out = build_out;
        object
    }

    #[test]
    fn a_build_with_the_slide_waits_for_the_slide_then_fades_in() {
        let slide = Slide::new(vec![shape_with(Some(Build::of(BuildEffect::Dissolve)), None)]);
        let theme = Theme::builtin();
        let at = |since_on| slide.built(&theme, BuildClock { step: 0, since_on, since_step: 0.0 });
        let (during_transition, moving) = at(None);
        assert!(during_transition.objects[0].hidden && !moving);
        let (half, moving) = at(Some(0.3));
        assert!(!half.objects[0].hidden && moving);
        assert!((half.objects[0].opacity - 0.5).abs() < 0.01, "{}", half.objects[0].opacity);
        let (done, moving) = at(Some(5.0));
        assert_eq!((done.objects[0].opacity, done.objects[0].hidden, moving), (1.0, false, false));
        // A slide without builds is left as it is.
        let plain = Slide::new(vec![Object::text(Role::primary(), "Hi")]);
        assert_eq!(plain.built(&theme, BuildClock::settled(0)), (plain.clone(), false));
    }

    #[test]
    fn text_built_by_paragraph_shows_one_more_paragraph_each_click() {
        let mut text = Object::text(Role::primary(), "One\nTwo\nThree");
        text.build_in = Some(Build { on_click: true, by_paragraph: true, ..Build::of(BuildEffect::Dissolve) });
        let slide = Slide::new(vec![text]);
        assert_eq!(slide.click_steps(), [(0, true), (0, true), (0, true)]);
        let theme = Theme::builtin();
        let at = |step| slide.built(&theme, BuildClock::settled(step)).0.objects[0].clone();
        assert!(at(0).hidden);
        assert_eq!((at(1).hidden, at(1).shown_paragraphs), (false, Some(1)));
        assert_eq!(at(2).shown_paragraphs, Some(2));
        assert_eq!((at(3).hidden, at(3).shown_paragraphs), (false, None), "all three: shown whole");
        // With the slide: one paragraph each turn of its time.
        let mut timed = Object::text(Role::primary(), "One\nTwo");
        timed.build_in = Some(Build { seconds: 1.0, by_paragraph: true, ..Build::of(BuildEffect::Dissolve) });
        let slide = Slide::new(vec![timed]);
        let after = |since| slide.built(&theme, BuildClock { step: 0, since_on: Some(since), since_step: 0.0 });
        assert_eq!(after(0.5).0.objects[0].shown_paragraphs, Some(1));
        assert!(after(0.5).1, "still to come");
        assert_eq!((after(1.5).0.objects[0].shown_paragraphs, after(1.5).1), (None, false));
    }

    #[test]
    fn clicks_bring_objects_in_then_take_them_out_in_order() {
        let click_in = Build { on_click: true, ..Build::of(BuildEffect::Fly) };
        let slide = Slide::new(vec![
            shape_with(Some(click_in), Some(Build::of(BuildEffect::Dissolve))),
            shape_with(Some(click_in), None),
        ]);
        assert_eq!(slide.click_steps(), [(0, true), (1, true), (0, false)]);
        let theme = Theme::builtin();
        let hidden = |step: usize| {
            let (built, _) = slide.built(&theme, BuildClock::settled(step));
            built.objects.iter().map(|o| o.hidden).collect::<Vec<_>>()
        };
        assert_eq!(hidden(0), [true, true]);
        assert_eq!(hidden(1), [false, true]);
        assert_eq!(hidden(2), [false, false]);
        assert_eq!(hidden(3), [true, false], "the first one has gone out");
        // A build set going just now is at its start; one long done stays still. (Stepping back
        // to a step must not count as setting its build going again.)
        let fresh = slide.built(&theme, BuildClock { step: 1, since_on: Some(9.0), since_step: 0.0 }).0;
        assert!(fresh.objects[0].hidden);
        let still = slide.built(&theme, BuildClock { step: 1, since_on: Some(9.0), since_step: 3600.0 }).0;
        assert!(!still.objects[0].hidden && still.objects[0].frame == shape_with(None, None).frame);
        // Flying in from the left, part-way: it is to the left of its place.
        let (flying, moving) = slide.built(&theme, BuildClock { step: 1, since_on: Some(9.0), since_step: 0.3 });
        assert!(moving);
        let frame = flying.objects[0].frame.unwrap();
        assert!(frame.x < 100.0 && frame.x > -200.0, "{frame:?}");
    }
}
