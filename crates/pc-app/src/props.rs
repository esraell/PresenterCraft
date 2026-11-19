//! Props: things shown over the slides on the audience screens until they are taken off, such as
//! a logo's name in a corner or a "LIVE" tag. A prop is a slide of its own, made in the editor.
//! Props are kept in `props.json`.

use pc_model::{Object, Role, Slide, SlideRect, TextStyle, Uuid};
use serde::{Deserialize, Serialize};

/// The place an editor draft says it belongs to when it is a prop being edited; `slide` then
/// says which prop.
pub const PROP_SLOT: usize = usize::MAX - 1;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Prop {
    pub id: Uuid,
    pub name: String,
    pub slide: Slide,
    /// Taken off the screen by itself after this many seconds; `None` stays until cleared.
    #[serde(default)]
    pub auto_clear: Option<u32>,
}

impl Prop {
    /// A new prop: a small box of words in the top left corner, to be changed in the editor.
    pub fn new(name: String) -> Prop {
        let builtin = pc_model::Theme::builtin();
        let mut look = builtin.templates[0].boxes[0].clone();
        look.size = 56.0;
        look.min_size = 20.0;
        look.align = pc_model::Align::Left;
        look.outline_width = 0.0;
        let mut words = Object::text(Role("prop".to_owned()), name.clone());
        words.name = "Words".to_owned();
        words.frame = Some(SlideRect { x: 60.0, y: 40.0, w: 700.0, h: 110.0 });
        if let Some(style) = words.text_style_mut() {
            *style = TextStyle::all(&look);
        }
        Prop { id: Uuid::new_v4(), name, slide: Slide::new(vec![words]), auto_clear: None }
    }

    /// A copy with ids of its own.
    pub fn duplicate(&self) -> Prop {
        let mut copy = self.clone();
        copy.id = Uuid::new_v4();
        copy.name = format!("{} copy", self.name);
        copy.slide.id = Uuid::new_v4();
        for object in &mut copy.slide.objects {
            object.id = Uuid::new_v4();
        }
        copy
    }
}

/// The props on screen and a message, as one layer over the slide: the props in the order they
/// were put on, the message on top.
pub fn overlay(props: &[Slide], message: Option<&Slide>) -> Option<Slide> {
    if props.is_empty() && message.is_none() {
        return None;
    }
    let objects = props.iter().chain(message).flat_map(|slide| slide.objects.iter().cloned()).collect();
    Some(Slide::new(objects))
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("props.json"))
}

pub fn load() -> Vec<Prop> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(props: &[Prop]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(props).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the props: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn props_and_a_message_make_one_layer_and_survive_their_file() {
        let a = Prop::new("Logo".to_owned());
        let b = a.duplicate();
        assert_ne!((a.id, a.slide.objects[0].id), (b.id, b.slide.objects[0].id));
        assert_eq!(b.name, "Logo copy");
        let message = crate::messages::slide("Hello");
        let layer = overlay(&[a.slide.clone(), b.slide.clone()], Some(&message)).unwrap();
        assert_eq!(layer.objects.len(), 3);
        assert_eq!(layer.objects[2].as_text(), Some("Hello"), "the message is on top");
        assert!(overlay(&[], None).is_none());
        let saved = vec![Prop { auto_clear: Some(10), ..a }];
        assert_eq!(serde_json::from_str::<Vec<Prop>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
