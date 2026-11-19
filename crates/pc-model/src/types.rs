use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::theme::{SlideRect, TextStyle};

/// A colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub const BLACK: Rgba = Rgba::rgb(0, 0, 0);
    pub const WHITE: Rgba = Rgba::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba { r, g, b, a: 255 }
    }
}

/// The name a text box is addressed by. Themes style boxes by role.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Role(pub String);

impl Role {
    /// The line that says where a Bible verse is from.
    pub fn reference() -> Role {
        Role("reference".to_owned())
    }

    /// The role's name as people see it: "Main", "Translation", or the role's own name with
    /// a capital and spaces.
    pub fn label(&self) -> String {
        match self.0.as_str() {
            "primary" => "Main".to_owned(),
            "translation" => "Translation".to_owned(),
            other => {
                let spaced = other.replace('-', " ");
                let mut letters = spaced.chars();
                letters.next().map_or_else(String::new, |first| first.to_uppercase().chain(letters).collect())
            }
        }
    }

    /// The main language.
    pub fn primary() -> Role {
        Role("primary".to_owned())
    }

    /// The second language, shown with the main one.
    pub fn translation() -> Role {
        Role("translation".to_owned())
    }
}

/// What an object on a slide is. More kinds (shapes, pictures, video) are added here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ObjectKind {
    /// A block of text. Line breaks in `text` are kept. `style` holds what this object sets
    /// for itself; the rest of its look comes from the theme.
    Text {
        text: String,
        #[serde(default, skip_serializing_if = "TextStyle::is_default")]
        style: TextStyle,
    },
}

fn full_opacity() -> f32 {
    1.0
}

fn is_zero(value: &f32) -> bool {
    *value == 0.0
}

fn is_full(value: &f32) -> bool {
    *value == 1.0
}

/// One thing on a slide. A slide's objects are drawn in order, the first at the bottom.
///
/// The theme decides where an object goes and how it looks unless the object says otherwise:
/// each field here that is `None` means "as the theme has it".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: Uuid,
    /// The name shown in the editor's object list.
    pub name: String,
    /// Which theme style the object follows, and how other parts of the app find it (the main
    /// language, the translation). `None` for an object that belongs to no role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<Role>,
    /// Where the object sits, in slide units. `None` uses the theme's place for its role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame: Option<SlideRect>,
    /// Turned clockwise about its centre, in degrees.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f32,
    /// From 0 (invisible) to 1 (solid).
    #[serde(default = "full_opacity", skip_serializing_if = "is_full")]
    pub opacity: f32,
    /// The colour the object's box is filled with behind its content. `None` leaves it clear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
    #[serde(flatten)]
    pub kind: ObjectKind,
}

impl Object {
    /// A text object that follows the theme for its role.
    pub fn text(role: Role, text: impl Into<String>) -> Object {
        let name = role.label();
        Object {
            id: Uuid::new_v4(),
            name,
            role: Some(role),
            frame: None,
            rotation: 0.0,
            opacity: 1.0,
            fill: None,
            kind: ObjectKind::Text { text: text.into(), style: TextStyle::default() },
        }
    }

    /// The object's words, if it is a text object.
    pub fn as_text(&self) -> Option<&str> {
        match &self.kind {
            ObjectKind::Text { text, .. } => Some(text),
        }
    }
}

fn enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    pub id: Uuid,
    /// Everything on the slide, bottom first.
    pub objects: Vec<Object>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default = "enabled_by_default")]
    pub enabled: bool,
}

impl Slide {
    pub fn new(objects: Vec<Object>) -> Slide {
        Slide { id: Uuid::new_v4(), objects, label: None, notes: None, enabled: true }
    }

    /// The text of the first text object with this role.
    pub fn text_for(&self, role: &Role) -> Option<&str> {
        self.objects.iter().find(|o| o.role.as_ref() == Some(role)).and_then(Object::as_text)
    }

    /// Each text object's role and words, in order. Two slides with the same words give the
    /// same list whatever their ids.
    pub fn texts(&self) -> Vec<(Option<Role>, String)> {
        self.objects.iter().filter_map(|o| Some((o.role.clone(), o.as_text()?.to_owned()))).collect()
    }
}

/// A named run of slides, such as "Verse 1" or "Chorus". Names may repeat in a presentation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub color: Rgba,
    pub slides: Vec<Slide>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Presentation {
    pub id: Uuid,
    pub title: String,
    pub groups: Vec<Group>,
}

impl Presentation {
    pub fn slide_count(&self) -> usize {
        self.groups.iter().map(|g| g.slides.len()).sum()
    }

    /// The slide at a flat index across all groups, in order, with its group.
    pub fn slide(&self, index: usize) -> Option<(&Group, &Slide)> {
        let mut remaining = index;
        for group in &self.groups {
            if let Some(slide) = group.slides.get(remaining) {
                return Some((group, slide));
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// A copy with the words of one slide's main and translation text replaced. The slide and
    /// its objects keep their ids, places and everything else. An empty text removes that
    /// object; if the slide would be left with no objects, or there is no such slide, returns
    /// `None`.
    pub fn with_slide_text(&self, index: usize, primary: &str, translation: &str) -> Option<Presentation> {
        let mut copy = self.clone();
        let mut remaining = index;
        for group in &mut copy.groups {
            if let Some(slide) = group.slides.get_mut(remaining) {
                // Main goes below the translation when it has to be added.
                for (position, role, text) in
                    [(0, Role::primary(), primary), (usize::MAX, Role::translation(), translation)]
                {
                    let text = crate::clean_box_text(text);
                    let existing = slide.objects.iter().position(|o| o.role.as_ref() == Some(&role));
                    match (existing, text.is_empty()) {
                        (Some(at), true) => {
                            slide.objects.remove(at);
                        }
                        (Some(at), false) => {
                            // Only the words change; the object keeps the look it was given.
                            let ObjectKind::Text { text: words, .. } = &mut slide.objects[at].kind;
                            *words = text;
                        }
                        (None, false) => {
                            let at = position.min(slide.objects.len());
                            slide.objects.insert(at, Object::text(role, text));
                        }
                        (None, true) => {}
                    }
                }
                return (!slide.objects.is_empty()).then_some(copy);
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// A copy with one slide replaced by an edited version of it. Returns `None` if there is no
    /// slide at that position.
    pub fn with_slide(&self, index: usize, slide: Slide) -> Option<Presentation> {
        let mut copy = self.clone();
        let mut remaining = index;
        for group in &mut copy.groups {
            if let Some(slot) = group.slides.get_mut(remaining) {
                *slot = slide;
                return Some(copy);
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// Every slide with its group, in order.
    pub fn slides(&self) -> impl Iterator<Item = (&Group, &Slide)> {
        self.groups.iter().flat_map(|g| g.slides.iter().map(move |s| (g, s)))
    }
}

/// A picture (later a video) that can be shown behind the words. Only what identifies it is
/// kept here; the pixels are loaded by the app.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaItem {
    pub id: Uuid,
    /// The file's name, shown under its thumbnail.
    pub name: String,
    pub path: std::path::PathBuf,
}
