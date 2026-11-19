use std::sync::Arc;

use pc_model::parse_song_text;

use super::*;

fn show() -> Show {
    Show::new(vec![Arc::new(parse_song_text("One", "A\n\nB\n\nC\n")), Arc::new(parse_song_text("Two", "D\n\nE\n"))])
}

fn at(presentation: usize, slide: usize) -> Option<SlideRef> {
    Some(SlideRef { presentation, slide })
}

#[test]
fn starts_with_nothing_live() {
    let s = show();
    assert_eq!(s.live(), None);
    assert!(s.output_enabled());
    assert_eq!(s.selected(), 0);
}

#[test]
fn goto_reports_slide_change() {
    let mut s = show();
    let c = s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(c.slides);
    assert_eq!(s.live(), at(0, 1));
    assert_eq!(s.live_slide().unwrap().1.boxes[0].text, "B");
}

#[test]
fn goto_the_live_slide_reports_nothing() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 })).slides);
}

#[test]
fn next_with_nothing_live_goes_to_first_of_selected() {
    let mut s = show();
    s.apply(Command::Select(1));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(1, 0));
}

#[test]
fn next_and_previous_stop_at_the_ends() {
    let mut s = show();
    for _ in 0..9 {
        s.apply(Command::Next);
    }
    assert_eq!(s.live(), at(0, 2));
    assert!(!s.apply(Command::Next).slides);
    for _ in 0..9 {
        s.apply(Command::Previous);
    }
    assert_eq!(s.live(), at(0, 0));
    assert!(!s.apply(Command::Previous).slides);
}

#[test]
fn clear_then_next_continues() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    assert!(s.apply(Command::Clear(Layer::Slides)).slides);
    assert_eq!(s.live(), None);
    assert_eq!(s.cursor(), at(0, 0));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn clear_on_the_last_slide_then_next_shows_it_again() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    s.apply(Command::ClearAll);
    assert!(s.apply(Command::Next).slides);
    assert_eq!(s.live(), at(0, 2));
}

#[test]
fn clear_then_previous_goes_back_one() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    s.apply(Command::ClearAll);
    s.apply(Command::Previous);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn previous_with_no_cursor_does_nothing() {
    let mut s = show();
    assert!(!s.apply(Command::Previous).slides);
    assert_eq!(s.live(), None);
}

#[test]
fn clear_when_already_clear_reports_nothing() {
    let mut s = show();
    assert!(!s.apply(Command::ClearAll).slides);
}

#[test]
fn clearing_another_layer_keeps_the_slide() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(!s.apply(Command::Clear(Layer::Media)).slides);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn select_does_not_change_live() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    let c = s.apply(Command::Select(1));
    assert!(c.selection && !c.slides);
    assert_eq!(s.live(), at(0, 2));
    assert_eq!(s.selected(), 1);
}

#[test]
fn next_follows_the_live_song_not_the_selected_one() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    s.apply(Command::Select(1));
    s.apply(Command::Next);
    assert_eq!(s.live(), at(0, 1));
}

#[test]
fn first_and_last_use_the_selected_presentation() {
    let mut s = show();
    s.apply(Command::Select(1));
    s.apply(Command::Last);
    assert_eq!(s.live(), at(1, 1));
    s.apply(Command::First);
    assert_eq!(s.live(), at(1, 0));
}

#[test]
fn out_of_range_is_ignored() {
    let mut s = show();
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 5, slide: 0 })).slides);
    assert!(!s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 3 })).slides);
    assert!(!s.apply(Command::Select(9)).selection);
    assert_eq!(s.live(), None);
}

#[test]
fn output_toggle() {
    let mut s = show();
    assert!(s.apply(Command::SetOutputEnabled(false)).output_enabled);
    assert!(!s.apply(Command::SetOutputEnabled(false)).output_enabled);
    assert!(!s.output_enabled());
}

#[test]
fn empty_show_never_panics() {
    let mut s = Show::new(vec![]);
    for c in [Command::Next, Command::Previous, Command::First, Command::Last, Command::ClearAll, Command::Select(0)] {
        assert_eq!(s.apply(c), Changes::default());
    }
}

#[test]
fn a_song_without_slides_is_skipped() {
    let mut s = Show::new(vec![Arc::new(parse_song_text("Empty", ""))]);
    for c in [Command::Next, Command::First, Command::Last] {
        assert!(!s.apply(c).slides);
    }
}
