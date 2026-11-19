//! Messages: short notices shown over the slides on the audience screens, such as "Parents of
//! child 12, please come to the nursery". A message's words can hold tokens that are filled in
//! as it is shown: `{Clock}` for the time, `{Timer: name}` for a timer. Messages are kept in
//! `messages.json`.

use std::time::Instant;

use pc_model::{Align, Object, Rgba, Role, Slide, SlideRect, TextStyle, Uuid, VAlign};
use serde::{Deserialize, Serialize};

use crate::timers::Timer;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub id: Uuid,
    pub name: String,
    pub text: String,
    /// Taken off the screen by itself after this many seconds; `None` waits to be cleared.
    #[serde(default)]
    pub dismiss_after: Option<u32>,
}

impl Message {
    pub fn new(name: String) -> Message {
        Message { id: Uuid::new_v4(), name, text: String::new(), dismiss_after: None }
    }
}

/// The token for the time of day.
pub const CLOCK_TOKEN: &str = "{Clock}";

/// The token for a timer's value.
pub fn timer_token(name: &str) -> String {
    format!("{{Timer: {name}}}")
}

/// True when the words have a token whose value changes as time passes.
pub fn has_tokens(text: &str) -> bool {
    text.contains(CLOCK_TOKEN) || text.contains("{Timer:")
}

/// The words with each token replaced by its value now.
pub fn fill_in(text: &str, timers: &[Timer], now: Instant, seconds_into_day: u32) -> String {
    let mut out = text.replace(CLOCK_TOKEN, &chrono::Local::now().format("%-I:%M").to_string());
    for timer in timers {
        let token = timer_token(&timer.name);
        if out.contains(&token) {
            out = out.replace(&token, &timer.text(now, seconds_into_day));
        }
    }
    out
}

/// A message as a layer over the slide: its words in a dark band along the bottom.
pub fn slide(words: &str) -> Slide {
    let builtin = pc_model::Theme::builtin();
    let mut look = builtin.templates[0].boxes[0].clone();
    look.size = 60.0;
    look.min_size = 28.0;
    look.weight = 600;
    look.fill = Rgba::WHITE;
    look.align = Align::Center;
    look.valign = VAlign::Middle;
    look.outline_width = 0.0;
    look.shadow = false;
    look.margins = [60.0, 16.0, 60.0, 16.0];
    let mut object = Object::text(Role("message".to_owned()), words);
    object.frame = Some(SlideRect { x: 0.0, y: 890.0, w: 1920.0, h: 190.0 });
    object.fill = Some(Rgba { r: 0, g: 0, b: 0, a: 200 });
    if let Some(style) = object.text_style_mut() {
        *style = TextStyle::all(&look);
    }
    Slide::new(vec![object])
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("messages.json"))
}

pub fn load() -> Vec<Message> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(messages: &[Message]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(messages).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the messages: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_filled_in_with_the_time_and_timers() {
        let now = Instant::now();
        let timer = Timer::new("Service".to_owned());
        let text = format!("Starts in {} · it is {CLOCK_TOKEN}", timer_token("Service"));
        assert!(has_tokens(&text));
        let filled = fill_in(&text, &[timer], now, 0);
        assert!(filled.starts_with("Starts in 00:05:00 · it is "), "{filled}");
        assert!(!filled.contains('{'));
        assert!(!has_tokens("Welcome"));
        assert_eq!(fill_in("Welcome", &[], now, 0), "Welcome");
        // A message is one box in a band along the bottom.
        let band = slide("Welcome");
        assert_eq!(band.objects.len(), 1);
        assert!(band.objects[0].frame.is_some_and(|frame| frame.y > 800.0));
        let saved = vec![Message { text: "Hi".to_owned(), dismiss_after: Some(10), ..Message::new("M".to_owned()) }];
        assert_eq!(serde_json::from_str::<Vec<Message>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
