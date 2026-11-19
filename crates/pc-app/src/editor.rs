//! What the Edit view does to a slide, kept apart from how it is drawn: finding the object
//! under the pointer, moving and resizing it, adding and deleting objects, and undo.

use pc_engine::{Show, SlideRef};
use pc_model::{Object, ObjectKind, SLIDE_HEIGHT, SLIDE_WIDTH, Slide, SlideRect, TextStyle, Theme, Uuid};
use pc_render::object_frame;

/// An object cannot be made smaller than this on either side, in slide units.
pub const MIN_SIDE: f32 = 20.0;
/// How many changes can be undone.
const UNDO_LIMIT: usize = 200;

/// One of the eight places an object can be resized from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handle {
    TopLeft,
    Top,
    TopRight,
    Right,
    BottomRight,
    Bottom,
    BottomLeft,
    Left,
}

impl Handle {
    pub const ALL: [Handle; 8] = [
        Handle::TopLeft,
        Handle::Top,
        Handle::TopRight,
        Handle::Right,
        Handle::BottomRight,
        Handle::Bottom,
        Handle::BottomLeft,
        Handle::Left,
    ];

    /// Where the handle sits on a frame, in slide units.
    pub fn position(self, frame: SlideRect) -> (f32, f32) {
        let (left, right, mid_x) = (frame.x, frame.x + frame.w, frame.x + frame.w / 2.0);
        let (top, bottom, mid_y) = (frame.y, frame.y + frame.h, frame.y + frame.h / 2.0);
        match self {
            Handle::TopLeft => (left, top),
            Handle::Top => (mid_x, top),
            Handle::TopRight => (right, top),
            Handle::Right => (right, mid_y),
            Handle::BottomRight => (right, bottom),
            Handle::Bottom => (mid_x, bottom),
            Handle::BottomLeft => (left, bottom),
            Handle::Left => (left, mid_y),
        }
    }

    fn moves_left(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Left | Handle::BottomLeft)
    }

    fn moves_right(self) -> bool {
        matches!(self, Handle::TopRight | Handle::Right | Handle::BottomRight)
    }

    fn moves_top(self) -> bool {
        matches!(self, Handle::TopLeft | Handle::Top | Handle::TopRight)
    }

    fn moves_bottom(self) -> bool {
        matches!(self, Handle::BottomLeft | Handle::Bottom | Handle::BottomRight)
    }
}

/// The handle of a frame within `reach` of a point, if any.
pub fn handle_at(frame: SlideRect, point: (f32, f32), reach: f32) -> Option<Handle> {
    Handle::ALL.into_iter().find(|handle| {
        let (x, y) = handle.position(frame);
        (point.0 - x).abs() <= reach && (point.1 - y).abs() <= reach
    })
}

/// A frame moved by a distance.
pub fn moved(frame: SlideRect, by: (f32, f32)) -> SlideRect {
    SlideRect { x: frame.x + by.0, y: frame.y + by.1, ..frame }
}

/// A frame resized by dragging one handle a distance. The opposite side stays where it is,
/// and the frame never gets smaller than [`MIN_SIDE`].
pub fn resized(frame: SlideRect, handle: Handle, by: (f32, f32)) -> SlideRect {
    let (mut left, mut top) = (frame.x, frame.y);
    let (mut right, mut bottom) = (frame.x + frame.w, frame.y + frame.h);
    if handle.moves_left() {
        left = (left + by.0).min(right - MIN_SIDE);
    }
    if handle.moves_right() {
        right = (right + by.0).max(left + MIN_SIDE);
    }
    if handle.moves_top() {
        top = (top + by.1).min(bottom - MIN_SIDE);
    }
    if handle.moves_bottom() {
        bottom = (bottom + by.1).max(top + MIN_SIDE);
    }
    SlideRect { x: left, y: top, w: right - left, h: bottom - top }
}

/// True when a point is inside a frame that has been turned about its centre.
fn contains(frame: SlideRect, rotation: f32, point: (f32, f32)) -> bool {
    let (cx, cy) = (frame.x + frame.w / 2.0, frame.y + frame.h / 2.0);
    // Turn the point back by the object's rotation and test against the upright frame.
    let (sin, cos) = (-rotation).to_radians().sin_cos();
    let (dx, dy) = (point.0 - cx, point.1 - cy);
    let (x, y) = (cx + dx * cos - dy * sin, cy + dx * sin + dy * cos);
    x >= frame.x && x <= frame.x + frame.w && y >= frame.y && y <= frame.y + frame.h
}

/// The topmost object under a point, in slide units.
pub fn object_at(slide: &Slide, theme: &Theme, point: (f32, f32)) -> Option<Uuid> {
    slide.objects.iter().rev().find(|o| contains(object_frame(slide, o, theme), o.rotation, point)).map(|o| o.id)
}

/// What a drag on the canvas is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Drag {
    Move { id: Uuid, from: SlideRect, start: (f32, f32) },
    Resize { id: Uuid, handle: Handle, from: SlideRect, start: (f32, f32) },
}

/// The slide being edited: a working copy, what is selected, and the changes that can be undone.
#[derive(Clone, Debug)]
pub struct EditDraft {
    pub at: SlideRef,
    pub slide: Slide,
    pub selected: Option<Uuid>,
    pub drag: Option<Drag>,
    undo: Vec<Slide>,
    redo: Vec<Slide>,
    /// When the last change was recorded for undo, so a run of typing or nudging is one step.
    last_recorded: f64,
    /// Set when undo or redo has just changed the slide, so that change is not itself
    /// recorded as something to undo.
    pub stepped: bool,
}

impl EditDraft {
    /// Starts editing a slide of the show.
    pub fn of(show: &Show, at: SlideRef) -> Option<EditDraft> {
        let (_, slide) = show.presentations().get(at.presentation)?.slide(at.slide)?;
        Some(EditDraft {
            at,
            slide: slide.clone(),
            selected: None,
            drag: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_recorded: f64::NEG_INFINITY,
            stepped: false,
        })
    }

    pub fn selected_object(&self) -> Option<&Object> {
        let id = self.selected?;
        self.slide.objects.iter().find(|o| o.id == id)
    }

    pub fn selected_object_mut(&mut self) -> Option<&mut Object> {
        let id = self.selected?;
        self.slide.objects.iter_mut().find(|o| o.id == id)
    }

    /// Remembers the slide as it is now, so the change about to be made can be undone.
    pub fn record(&mut self) {
        self.record_slide(self.slide.clone());
    }

    /// Remembers an earlier state of the slide as one undo step.
    pub fn record_slide(&mut self, before: Slide) {
        if self.undo.last() == Some(&before) {
            return;
        }
        self.undo.push(before);
        if self.undo.len() > UNDO_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Like [`record_slide`](Self::record_slide), but changes that follow each other within
    /// a second count as one step, so typing a word is undone as a word and not letter by letter.
    pub fn record_run(&mut self, before: Slide, now: f64) {
        if now - self.last_recorded > 1.0 {
            self.record_slide(before);
        }
        self.last_recorded = now;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Goes back one step. Returns true if anything changed.
    pub fn undo(&mut self) -> bool {
        let Some(earlier) = self.undo.pop() else { return false };
        self.redo.push(std::mem::replace(&mut self.slide, earlier));
        self.after_history();
        true
    }

    /// Goes forward one step again. Returns true if anything changed.
    pub fn redo(&mut self) -> bool {
        let Some(later) = self.redo.pop() else { return false };
        self.undo.push(std::mem::replace(&mut self.slide, later));
        self.after_history();
        true
    }

    fn after_history(&mut self) {
        self.stepped = true;
        self.drag = None;
        self.last_recorded = f64::NEG_INFINITY;
        if self.selected_object().is_none() {
            self.selected = None;
        }
    }

    /// Adds a text object in the middle of the slide, on top, and selects it.
    pub fn add_text(&mut self) {
        self.record();
        let (w, h) = (900.0, 220.0);
        let number = self.slide.objects.len() + 1;
        let object = Object {
            id: Uuid::new_v4(),
            name: format!("Text {number}"),
            role: None,
            frame: Some(SlideRect { x: (SLIDE_WIDTH - w) / 2.0, y: (SLIDE_HEIGHT - h) / 2.0, w, h }),
            rotation: 0.0,
            opacity: 1.0,
            fill: None,
            kind: ObjectKind::Text { text: "Text".to_owned(), style: TextStyle::default() },
        };
        self.selected = Some(object.id);
        self.slide.objects.push(object);
    }

    /// Removes the selected object. Returns true if there was one.
    pub fn delete_selected(&mut self) -> bool {
        let Some(id) = self.selected else { return false };
        let Some(index) = self.slide.objects.iter().position(|o| o.id == id) else { return false };
        self.record();
        self.slide.objects.remove(index);
        self.selected = None;
        self.drag = None;
        true
    }

    /// Moves the selected object one place up (`true`) or down the stack. Returns true if it moved.
    pub fn restack(&mut self, forward: bool) -> bool {
        let Some(id) = self.selected else { return false };
        let Some(index) = self.slide.objects.iter().position(|o| o.id == id) else { return false };
        let target = if forward { index + 1 } else { index.wrapping_sub(1) };
        if target >= self.slide.objects.len() {
            return false;
        }
        self.record();
        self.slide.objects.swap(index, target);
        true
    }

    /// Gives the selected object back to the theme: its own place, turn and fade are dropped.
    pub fn reset_to_theme(&mut self) -> bool {
        let Some(object) = self.selected_object() else { return false };
        if object.frame.is_none() && object.rotation == 0.0 && object.opacity == 1.0 {
            return false;
        }
        self.record();
        if let Some(object) = self.selected_object_mut() {
            object.frame = None;
            object.rotation = 0.0;
            object.opacity = 1.0;
        }
        true
    }

    /// Begins a drag at a point on the slide: on a handle of the selected object it resizes,
    /// on an object it selects and moves it, on empty slide it clears the selection.
    pub fn press(&mut self, theme: &Theme, point: (f32, f32), handle_reach: f32) {
        self.drag = None;
        if let Some(object) = self.selected_object()
            && object.rotation == 0.0
        {
            let from = object_frame(&self.slide, object, theme);
            if let Some(handle) = handle_at(from, point, handle_reach) {
                self.drag = Some(Drag::Resize { id: object.id, handle, from, start: point });
                return;
            }
        }
        self.selected = object_at(&self.slide, theme, point);
        if let Some(object) = self.selected_object() {
            let from = object_frame(&self.slide, object, theme);
            self.drag = Some(Drag::Move { id: object.id, from, start: point });
        }
    }

    /// Continues the drag to a point. Returns true if the slide changed.
    pub fn drag_to(&mut self, point: (f32, f32)) -> bool {
        let Some(drag) = self.drag else { return false };
        let (id, frame) = match drag {
            Drag::Move { id, from, start } => (id, moved(from, (point.0 - start.0, point.1 - start.1))),
            Drag::Resize { id, handle, from, start } => {
                (id, resized(from, handle, (point.0 - start.0, point.1 - start.1)))
            }
        };
        let Some(index) = self.slide.objects.iter().position(|o| o.id == id) else { return false };
        if self.slide.objects[index].frame == Some(frame) {
            return false;
        }
        // The first movement of a drag is the moment to remember where things were.
        let before = self.slide.clone();
        self.slide.objects[index].frame = Some(frame);
        let (Drag::Move { from, .. } | Drag::Resize { from, .. }) = drag;
        if before.objects[index].frame.is_none() || before.objects[index].frame == Some(from) {
            self.record_slide(before);
        }
        true
    }

    pub fn release(&mut self) {
        self.drag = None;
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use pc_model::parse_song_text;

    use super::*;

    const F: SlideRect = SlideRect { x: 100.0, y: 200.0, w: 400.0, h: 300.0 };

    fn draft() -> EditDraft {
        let show = Show::new(vec![Arc::new(parse_song_text("S", "Main words\n--\nПеревод\n"))]);
        EditDraft::of(&show, SlideRef { presentation: 0, slide: 0 }).unwrap()
    }

    #[test]
    fn handles_sit_on_the_corners_and_the_middles_of_the_sides() {
        assert_eq!(Handle::TopLeft.position(F), (100.0, 200.0));
        assert_eq!(Handle::Right.position(F), (500.0, 350.0));
        assert_eq!(Handle::Bottom.position(F), (300.0, 500.0));
        assert_eq!(handle_at(F, (503.0, 497.0), 6.0), Some(Handle::BottomRight));
        assert_eq!(handle_at(F, (300.0, 350.0), 6.0), None);
    }

    #[test]
    fn resizing_keeps_the_opposite_side_still() {
        assert_eq!(resized(F, Handle::Right, (50.0, 999.0)), SlideRect { w: 450.0, ..F });
        assert_eq!(resized(F, Handle::TopLeft, (10.0, 20.0)), SlideRect { x: 110.0, y: 220.0, w: 390.0, h: 280.0 });
        assert_eq!(resized(F, Handle::Bottom, (999.0, -100.0)), SlideRect { h: 200.0, ..F });
    }

    #[test]
    fn resizing_stops_at_the_smallest_size_without_flipping() {
        let squeezed = resized(F, Handle::Left, (5000.0, 0.0));
        assert_eq!(squeezed, SlideRect { x: 500.0 - MIN_SIDE, w: MIN_SIDE, ..F });
        let squeezed = resized(F, Handle::Top, (0.0, 5000.0));
        assert_eq!(squeezed, SlideRect { y: 500.0 - MIN_SIDE, h: MIN_SIDE, ..F });
        let squeezed = resized(F, Handle::BottomRight, (-5000.0, -5000.0));
        assert_eq!(squeezed, SlideRect { w: MIN_SIDE, h: MIN_SIDE, ..F });
    }

    #[test]
    fn a_turned_frame_is_hit_where_it_is_drawn() {
        // A wide, flat frame turned a quarter stands tall and narrow about the same centre.
        let flat = SlideRect { x: 100.0, y: 300.0, w: 400.0, h: 40.0 };
        assert!(contains(flat, 0.0, (120.0, 320.0)));
        assert!(!contains(flat, 90.0, (120.0, 320.0)));
        assert!(contains(flat, 90.0, (300.0, 150.0)));
        assert!(!contains(flat, 0.0, (300.0, 150.0)));
    }

    #[test]
    fn the_topmost_object_under_the_pointer_is_found() {
        let theme = Theme::builtin();
        let mut d = draft();
        let main = d.slide.objects[0].id;
        let translation = d.slide.objects[1].id;
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), Some(main));
        assert_eq!(object_at(&d.slide, &theme, (960.0, 700.0)), Some(translation));
        assert_eq!(object_at(&d.slide, &theme, (5.0, 5.0)), None);
        // Where two overlap, the one drawn last is on top.
        d.slide.objects[1].frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        assert_eq!(object_at(&d.slide, &theme, (960.0, 300.0)), Some(translation));
    }

    #[test]
    fn dragging_an_object_moves_it_and_is_one_undo_step() {
        let theme = Theme::builtin();
        let mut d = draft();
        let original = d.slide.clone();
        d.press(&theme, (960.0, 300.0), 8.0);
        assert_eq!(d.selected, Some(d.slide.objects[0].id));
        assert!(d.drag_to((970.0, 310.0)));
        assert!(d.drag_to((1000.0, 350.0)));
        d.release();
        let themed = theme.boxes[0].rect;
        assert_eq!(d.slide.objects[0].frame, Some(SlideRect { x: themed.x + 40.0, y: themed.y + 50.0, ..themed }));
        assert!(d.undo());
        assert_eq!(d.slide, original, "one undo puts the whole drag back");
        assert!(!d.undo());
        assert!(d.redo());
        assert_eq!(d.slide.objects[0].frame.map(|f| f.x), Some(themed.x + 40.0));
    }

    #[test]
    fn dragging_a_handle_resizes_the_selected_object() {
        let theme = Theme::builtin();
        let mut d = draft();
        d.press(&theme, (960.0, 300.0), 8.0);
        d.release();
        let themed = theme.boxes[0].rect;
        let corner = (themed.x + themed.w, themed.y + themed.h);
        d.press(&theme, corner, 8.0);
        assert!(matches!(d.drag, Some(Drag::Resize { handle: Handle::BottomRight, .. })));
        d.drag_to((corner.0 - 100.0, corner.1 + 60.0));
        assert_eq!(d.slide.objects[0].frame, Some(SlideRect { w: themed.w - 100.0, h: themed.h + 60.0, ..themed }));
    }

    #[test]
    fn pressing_empty_slide_clears_the_selection_and_a_press_without_a_drag_changes_nothing() {
        let theme = Theme::builtin();
        let mut d = draft();
        let original = d.slide.clone();
        d.press(&theme, (960.0, 300.0), 8.0);
        d.release();
        assert!(d.selected.is_some());
        assert_eq!(d.slide, original);
        assert!(!d.can_undo());
        d.press(&theme, (5.0, 5.0), 8.0);
        assert_eq!(d.selected, None);
        assert!(!d.drag_to((50.0, 50.0)));
    }

    #[test]
    fn adding_deleting_and_restacking() {
        let mut d = draft();
        d.add_text();
        assert_eq!(d.slide.objects.len(), 3);
        assert_eq!(d.selected, Some(d.slide.objects[2].id));
        assert!(d.slide.objects[2].frame.is_some() && d.slide.objects[2].role.is_none());
        assert!(d.restack(false));
        assert_eq!(d.selected, Some(d.slide.objects[1].id));
        assert!(d.restack(false));
        assert!(!d.restack(false), "already at the bottom");
        assert!(d.delete_selected());
        assert_eq!(d.slide.objects.len(), 2);
        assert!(!d.delete_selected(), "nothing is selected now");
        // Undo walks back through delete, the two moves and the add.
        for _ in 0..4 {
            assert!(d.undo());
        }
        assert_eq!(d.slide.objects.len(), 2);
        assert!(!d.can_undo());
    }

    #[test]
    fn reset_gives_the_object_back_to_the_theme() {
        let theme = Theme::builtin();
        let mut d = draft();
        d.press(&theme, (960.0, 300.0), 8.0);
        assert!(!d.reset_to_theme(), "nothing to reset yet");
        d.drag_to((900.0, 300.0));
        d.release();
        d.selected_object_mut().unwrap().rotation = 12.0;
        assert!(d.reset_to_theme());
        let object = d.selected_object().unwrap();
        assert_eq!((object.frame, object.rotation, object.opacity), (None, 0.0, 1.0));
    }

    #[test]
    fn a_run_of_quick_changes_is_one_undo_step() {
        let mut d = draft();
        let first = d.slide.clone();
        for (i, now) in [10.0, 10.3, 10.6, 10.9].into_iter().enumerate() {
            let before = d.slide.clone();
            d.slide.objects[0].kind = ObjectKind::Text { text: "a".repeat(i + 1), style: TextStyle::default() };
            d.record_run(before, now);
        }
        // A pause, then more typing: a second step.
        let before = d.slide.clone();
        d.slide.objects[0].kind = ObjectKind::Text { text: "later".to_owned(), style: TextStyle::default() };
        d.record_run(before, 13.0);
        assert!(d.undo());
        assert_eq!(d.slide.objects[0].as_text(), Some("aaaa"));
        assert!(d.undo());
        assert_eq!(d.slide, first);
        assert!(!d.undo());
    }
}
