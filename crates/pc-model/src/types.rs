use uuid::Uuid;

/// A colour with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Role(pub String);

impl Role {
    /// The main language.
    pub fn primary() -> Role {
        Role("primary".to_owned())
    }

    /// The second language, shown with the main one.
    pub fn translation() -> Role {
        Role("translation".to_owned())
    }
}

/// One block of text on a slide. Line breaks in `text` are kept.
#[derive(Clone, Debug, PartialEq)]
pub struct TextBox {
    pub role: Role,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Slide {
    pub id: Uuid,
    pub boxes: Vec<TextBox>,
    pub label: Option<String>,
    pub notes: Option<String>,
    pub enabled: bool,
}

impl Slide {
    pub fn new(boxes: Vec<TextBox>) -> Slide {
        Slide { id: Uuid::new_v4(), boxes, label: None, notes: None, enabled: true }
    }

    /// The text of the first box with this role.
    pub fn text_for(&self, role: &Role) -> Option<&str> {
        self.boxes.iter().find(|b| &b.role == role).map(|b| b.text.as_str())
    }
}

/// A named run of slides, such as "Verse 1" or "Chorus". Names may repeat in a presentation.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: Uuid,
    pub name: String,
    pub color: Rgba,
    pub slides: Vec<Slide>,
}

#[derive(Clone, Debug, PartialEq)]
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

    /// A copy with the words of one slide replaced. The slide keeps its id. An empty text
    /// removes that box; if both are empty, or there is no such slide, returns `None`.
    pub fn with_slide_text(&self, index: usize, primary: &str, translation: &str) -> Option<Presentation> {
        let primary = crate::clean_box_text(primary);
        let translation = crate::clean_box_text(translation);
        let mut boxes = Vec::new();
        if !primary.is_empty() {
            boxes.push(TextBox { role: Role::primary(), text: primary });
        }
        if !translation.is_empty() {
            boxes.push(TextBox { role: Role::translation(), text: translation });
        }
        if boxes.is_empty() {
            return None;
        }
        let mut copy = self.clone();
        let mut remaining = index;
        for group in &mut copy.groups {
            if let Some(slide) = group.slides.get_mut(remaining) {
                slide.boxes = boxes;
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
