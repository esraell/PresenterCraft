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

fn picture(name: &str) -> pc_model::MediaItem {
    pc_model::MediaItem { id: pc_model::Uuid::new_v4(), name: name.to_owned(), path: name.into() }
}

#[test]
fn media_is_added_in_order_and_nothing_is_live() {
    let mut s = show();
    assert_eq!(s.add_media(picture("a.png")), 0);
    assert_eq!(s.add_media(picture("b.png")), 1);
    assert_eq!(s.media().len(), 2);
    assert_eq!(s.live_media(), None);
}

#[test]
fn showing_media_reports_a_media_change_and_leaves_the_slide() {
    let mut s = show();
    s.add_media(picture("a.png"));
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    let c = s.apply(Command::ShowMedia(0));
    assert!(c.media && !c.slides);
    assert_eq!(s.live_media(), Some(0));
    assert_eq!(s.live(), at(0, 1));
    assert!(!s.apply(Command::ShowMedia(0)).media, "showing what is already showing changes nothing");
    assert!(!s.apply(Command::ShowMedia(7)).media, "there is no media 7");
}

#[test]
fn layers_clear_separately_and_clear_all_clears_both() {
    let mut s = show();
    s.add_media(picture("a.png"));
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    s.apply(Command::ShowMedia(0));

    let c = s.apply(Command::Clear(Layer::Media));
    assert!(c.media && !c.slides);
    assert_eq!(s.live_media(), None);
    assert_eq!(s.live(), at(0, 0));

    s.apply(Command::ShowMedia(0));
    let c = s.apply(Command::Clear(Layer::Slides));
    assert!(c.slides && !c.media);
    assert_eq!(s.live_media(), Some(0));

    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    let c = s.apply(Command::ClearAll);
    assert!(c.slides && c.media);
    assert_eq!((s.live(), s.live_media()), (None, None));
    assert_eq!(s.apply(Command::ClearAll), Changes::default());
}

#[test]
fn anything_live_says_whether_the_screen_is_empty() {
    let mut s = show();
    assert!(!s.anything_live());
    s.add_media(picture("a.png"));
    s.apply(Command::ShowMedia(0));
    assert!(s.anything_live());
    s.apply(Command::ClearAll);
    s.apply(Command::Next);
    assert!(s.anything_live());
}

#[test]
fn replacing_a_presentation_keeps_what_is_live_when_it_still_exists() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    // Same number of slides: the live slide stays, now with the new words.
    let c = s.replace_presentation(0, Arc::new(parse_song_text("One", "A\n\nB\n\nEdited\n")));
    assert!(c.slides, "the live slide's words changed");
    assert_eq!(s.live(), at(0, 2));
    assert_eq!(s.live_slide().unwrap().1.boxes[0].text, "Edited");
    // Fewer slides: the live slide is gone, so the layer is cleared.
    let c = s.replace_presentation(0, Arc::new(parse_song_text("One", "A\n")));
    assert!(c.slides);
    assert_eq!(s.live(), None);
    assert_eq!(s.cursor(), None);
    // Out of range: nothing happens.
    assert_eq!(s.replace_presentation(9, Arc::new(parse_song_text("X", "A\n"))), Changes::default());
}

#[test]
fn replacing_a_presentation_that_is_not_live_changes_no_layer() {
    let mut s = show();
    s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    let c = s.replace_presentation(1, Arc::new(parse_song_text("Two", "New\n")));
    assert!(!c.slides);
    assert_eq!(s.presentations()[1].slide(0).unwrap().1.boxes[0].text, "New");
}
