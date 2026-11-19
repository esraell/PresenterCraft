use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::theme::{Shadow, SlideRect, TextStyle};

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

/// What an object on a slide is. More kinds (pictures, video) are added here.
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
    /// A drawn shape filling the object's place, in the object's fill colour.
    Shape { shape: Shape },
}

/// The shapes the editor can draw.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    RoundedRect,
    Rectangle,
    Ellipse,
    Triangle,
    RightTriangle,
    Rhombus,
    Polygon,
    Star,
    DoubleArrow,
    Arrow,
}

impl Shape {
    pub const ALL: [Shape; 10] = [
        Shape::RoundedRect,
        Shape::Rectangle,
        Shape::Ellipse,
        Shape::Triangle,
        Shape::RightTriangle,
        Shape::Rhombus,
        Shape::Polygon,
        Shape::Star,
        Shape::DoubleArrow,
        Shape::Arrow,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Shape::RoundedRect => "Rounded Rect",
            Shape::Rectangle => "Rectangle",
            Shape::Ellipse => "Ellipse",
            Shape::Triangle => "Triangle",
            Shape::RightTriangle => "Right Triangle",
            Shape::Rhombus => "Rhombus",
            Shape::Polygon => "Polygon",
            Shape::Star => "Star",
            Shape::DoubleArrow => "Double Arrow",
            Shape::Arrow => "Arrow",
        }
    }

    /// The shape's outline as points in a box from (0, 0) to (1, 1), for the shapes made of
    /// straight lines. Rounded shapes are drawn as curves instead and give `None`.
    pub fn corners(self) -> Option<Vec<(f64, f64)>> {
        use std::f64::consts::PI;
        let ring = |count: usize, inner: Option<f64>| -> Vec<(f64, f64)> {
            let steps = if inner.is_some() { count * 2 } else { count };
            (0..steps)
                .map(|step| {
                    let radius = match inner {
                        Some(inner) if step % 2 == 1 => inner,
                        _ => 0.5,
                    };
                    let angle = -PI / 2.0 + 2.0 * PI * step as f64 / steps as f64;
                    (0.5 + radius * angle.cos(), 0.5 + radius * angle.sin())
                })
                .collect()
        };
        Some(match self {
            Shape::RoundedRect | Shape::Ellipse => return None,
            Shape::Rectangle => vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::Triangle => vec![(0.5, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::RightTriangle => vec![(0.0, 0.0), (1.0, 1.0), (0.0, 1.0)],
            Shape::Rhombus => vec![(0.5, 0.0), (1.0, 0.5), (0.5, 1.0), (0.0, 0.5)],
            Shape::Polygon => ring(5, None),
            Shape::Star => ring(5, Some(0.2)),
            Shape::Arrow => vec![(0.0, 0.3), (0.6, 0.3), (0.6, 0.0), (1.0, 0.5), (0.6, 1.0), (0.6, 0.7), (0.0, 0.7)],
            Shape::DoubleArrow => vec![
                (0.0, 0.5),
                (0.3, 0.0),
                (0.3, 0.3),
                (0.7, 0.3),
                (0.7, 0.0),
                (1.0, 0.5),
                (0.7, 1.0),
                (0.7, 0.7),
                (0.3, 0.7),
                (0.3, 1.0),
            ],
        })
    }
}

/// A line drawn round an object's box or shape.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StrokeLook {
    /// In slide units.
    pub width: f32,
    pub color: Rgba,
}

fn full_opacity() -> f32 {
    1.0
}

fn is_false(value: &bool) -> bool {
    !*value
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
    /// A hidden object is kept on the slide but not drawn.
    #[serde(default, skip_serializing_if = "is_false")]
    pub hidden: bool,
    /// A locked object cannot be selected or moved by clicking on the slide.
    #[serde(default, skip_serializing_if = "is_false")]
    pub locked: bool,
    /// The colour the object's box is filled with behind its content. `None` leaves it clear.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
    /// A line round the object's box or shape. `None` draws none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<StrokeLook>,
    /// A shadow behind the object's box or shape. `None` draws none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<Shadow>,
    /// Mirrored left to right, and top to bottom, about its centre.
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_h: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub flip_v: bool,
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
            hidden: false,
            locked: false,
            fill: None,
            stroke: None,
            shadow: None,
            flip_h: false,
            flip_v: false,
            kind: ObjectKind::Text { text: text.into(), style: TextStyle::default() },
        }
    }

    /// A shape in the middle of the slide, filled light blue as a new shape is.
    pub fn shape(shape: Shape) -> Object {
        Object {
            id: Uuid::new_v4(),
            name: shape.name().to_owned(),
            role: None,
            frame: Some(SlideRect { x: 660.0, y: 340.0, w: 600.0, h: 400.0 }),
            rotation: 0.0,
            opacity: 1.0,
            hidden: false,
            locked: false,
            fill: Some(Rgba::rgb(110, 170, 240)),
            stroke: None,
            shadow: None,
            flip_h: false,
            flip_v: false,
            kind: ObjectKind::Shape { shape },
        }
    }

    /// The object's words, if it is a text object.
    /// The object's own text settings, over its theme box.
    pub fn text_style(&self) -> Option<&crate::TextStyle> {
        match &self.kind {
            ObjectKind::Text { style, .. } => Some(style),
            ObjectKind::Shape { .. } => None,
        }
    }

    /// The object's own text settings, to change; `None` for an object that is not text.
    pub fn text_style_mut(&mut self) -> Option<&mut crate::TextStyle> {
        match &mut self.kind {
            ObjectKind::Text { style, .. } => Some(style),
            ObjectKind::Shape { .. } => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match &self.kind {
            ObjectKind::Text { text, .. } => Some(text),
            ObjectKind::Shape { .. } => None,
        }
    }
}

impl Slide {
    /// Replaces the words of the main and translation text. The objects keep their ids, places
    /// and looks. An empty text removes that object; a missing one is added.
    pub fn set_words(&mut self, primary: &str, translation: &str) {
        // Main goes below the translation when it has to be added.
        for (position, role, text) in [(0, Role::primary(), primary), (usize::MAX, Role::translation(), translation)] {
            let text = crate::clean_box_text(text);
            let existing = self.objects.iter().position(|o| o.role.as_ref() == Some(&role));
            match (existing, text.is_empty()) {
                (Some(at), true) => {
                    self.objects.remove(at);
                }
                (Some(at), false) => {
                    // Only the words change; the object keeps the look it was given.
                    if let ObjectKind::Text { text: words, .. } = &mut self.objects[at].kind {
                        *words = text;
                    }
                }
                (None, false) => {
                    let at = position.min(self.objects.len());
                    self.objects.insert(at, Object::text(role, text));
                }
                (None, true) => {}
            }
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
    /// The theme template the slide is drawn with, by its id. `None` lets the theme choose
    /// the template that suits what is on the slide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<Uuid>,
}

impl Slide {
    pub fn new(objects: Vec<Object>) -> Slide {
        Slide { id: Uuid::new_v4(), objects, label: None, notes: None, enabled: true, template: None }
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
    /// Who wrote the song and who holds its rights, for the copyright line.
    #[serde(default, skip_serializing_if = "Copyright::is_empty")]
    pub copyright: Copyright,
    /// The theme the song is drawn with, by its id; `None` uses the theme songs use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Uuid>,
}

/// A song's copyright details, as a CCLI report asks for them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Copyright {
    pub artist: String,
    pub author: String,
    pub publisher: String,
    pub year: String,
    /// The song's number in the CCLI catalogue.
    pub ccli_song: String,
}

impl Copyright {
    pub fn is_empty(&self) -> bool {
        *self == Copyright::default()
    }

    /// The copyright line shown on the screens: "Title" by the author, the year and publisher,
    /// the CCLI song number, and the church's CCLI licence number. Parts not known are left out.
    pub fn line(&self, title: &str, licence: &str) -> String {
        let mut parts = vec![format!("\u{201c}{title}\u{201d}")];
        let writer = if self.author.is_empty() { &self.artist } else { &self.author };
        if !writer.is_empty() {
            parts.push(writer.clone());
        }
        let held =
            [self.year.as_str(), self.publisher.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>();
        if !held.is_empty() {
            parts.push(format!("\u{a9} {}", held.join(" ")));
        }
        if !self.ccli_song.is_empty() {
            parts.push(format!("CCLI Song # {}", self.ccli_song));
        }
        if !licence.is_empty() {
            parts.push(format!("CCLI License # {licence}"));
        }
        parts.join("  \u{b7}  ")
    }
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
                slide.set_words(primary, translation);
                return (!slide.objects.is_empty()).then_some(copy);
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// The song rebuilt from its words as text (the song text format: `[Group]` lines, a blank
    /// line between slides, `--` before a translation), as the Reflow editor makes it. A slide
    /// at a place the song already had keeps everything but its words: its looks, label,
    /// notes, template and shapes. A group named as before keeps its colour.
    pub fn reflowed(&self, text: &str) -> Presentation {
        let parsed = crate::parse_song_text(&self.title, text);
        let before: Vec<&Slide> = self.slides().map(|(_, slide)| slide).collect();
        let mut index = 0;
        let mut groups = parsed.groups;
        for (at, group) in groups.iter_mut().enumerate() {
            if let Some(same) = self.groups.iter().find(|old| old.name == group.name) {
                group.color = same.color;
            }
            // A group in the same place with the same name is the same group.
            if let Some(same) = self.groups.get(at).filter(|old| old.name == group.name) {
                group.id = same.id;
            }
            for slide in &mut group.slides {
                if let Some(old) = before.get(index) {
                    let words = |role: Role| slide.text_for(&role).unwrap_or("").to_owned();
                    let (primary, translation) = (words(Role::primary()), words(Role::translation()));
                    let mut kept = (*old).clone();
                    kept.set_words(&primary, &translation);
                    if !kept.objects.is_empty() {
                        *slide = kept;
                    }
                }
                index += 1;
            }
        }
        Presentation {
            id: self.id,
            title: self.title.clone(),
            groups,
            copyright: self.copyright.clone(),
            theme: self.theme,
        }
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

    /// The group a slide is in, and where in that group, by its place in the whole song.
    fn locate(&self, index: usize) -> Option<(usize, usize)> {
        let mut remaining = index;
        for (at, group) in self.groups.iter().enumerate() {
            if remaining < group.slides.len() {
                return Some((at, remaining));
            }
            remaining -= group.slides.len();
        }
        None
    }

    /// A copy without one slide. A group left empty goes too. `None` if there is no such slide.
    pub fn without_slide(&self, index: usize) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        copy.groups[group].slides.remove(at);
        if copy.groups[group].slides.is_empty() {
            copy.groups.remove(group);
        }
        Some(copy)
    }

    /// A copy with a slide put just after the one at `index`, in the same group, given new ids
    /// so it is a slide of its own. `None` if there is no slide there.
    pub fn with_slide_after(&self, index: usize, slide: &Slide) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        let mut fresh = slide.clone();
        fresh.id = Uuid::new_v4();
        for object in &mut fresh.objects {
            object.id = Uuid::new_v4();
        }
        copy.groups[group].slides.insert(at + 1, fresh);
        Some(copy)
    }

    /// A copy where a group with this name starts at the slide at `index`: the slide and the
    /// rest of its group become a group of that name, in its colour. A slide that already
    /// starts its group renames the group. `None` if there is no slide there.
    pub fn with_group_starting(&self, index: usize, name: &str) -> Option<Presentation> {
        let (group, at) = self.locate(index)?;
        let mut copy = self.clone();
        let color = crate::group_color(name);
        if at == 0 {
            let renamed = &mut copy.groups[group];
            renamed.name = name.to_owned();
            renamed.color = color;
        } else {
            let tail = copy.groups[group].slides.split_off(at);
            let started = Group { id: Uuid::new_v4(), name: name.to_owned(), color, slides: tail };
            copy.groups.insert(group + 1, started);
        }
        Some(copy)
    }

    /// A copy with a new, empty slide at the end, and that slide's position. The new slide is
    /// laid out like the one before it (the same boxes, in the same places, with the same
    /// look) with no words in them, and joins the last group. A song with no slides gets a
    /// first one with a main-language box.
    pub fn with_slide_added(&self) -> (Presentation, usize) {
        let mut copy = self.clone();
        let slide = match copy.groups.iter().rev().find_map(|group| group.slides.last()) {
            Some(last) => {
                let objects = last
                    .objects
                    .iter()
                    .map(|object| {
                        // Text boxes come empty; shapes come as they are.
                        let kind = match &object.kind {
                            ObjectKind::Text { style, .. } => {
                                ObjectKind::Text { text: String::new(), style: style.clone() }
                            }
                            shape @ ObjectKind::Shape { .. } => shape.clone(),
                        };
                        Object { id: Uuid::new_v4(), kind, ..object.clone() }
                    })
                    .collect();
                // It uses the template the one before it uses.
                Slide { template: last.template, ..Slide::new(objects) }
            }
            None => Slide::new(vec![Object::text(Role::primary(), "")]),
        };
        if copy.groups.is_empty() {
            copy.groups.push(Group {
                id: Uuid::new_v4(),
                name: String::new(),
                color: Rgba::rgb(128, 128, 128),
                slides: Vec::new(),
            });
        }
        if let Some(group) = copy.groups.last_mut() {
            group.slides.push(slide);
        }
        let index = copy.slide_count() - 1;
        (copy, index)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reflowed_song_keeps_each_slides_look_and_takes_the_new_words() {
        let mut song = crate::parse_song_text("Song", "[Verse 1]\nA\n--\nА\n\nB\n\n[Chorus]\nC\n");
        song.groups[0].slides[0].label = Some("Repeat".to_owned());
        let crate::ObjectKind::Text { style, .. } = &mut song.groups[0].slides[0].objects[0].kind else { panic!() };
        style.size = Some(40.0);
        let text =
            crate::to_song_text(&song).replace("A\n", "A changed\n").replace("[Chorus]\nC\n", "[Chorus]\nC\n\nD\n");
        let again = song.reflowed(&text);
        assert_eq!(again.slide_count(), 4);
        let (_, first) = again.slide(0).unwrap();
        assert_eq!(first.text_for(&Role::primary()), Some("A changed"));
        assert_eq!(first.label.as_deref(), Some("Repeat"));
        assert_eq!(first.id, song.slide(0).unwrap().1.id, "the slide keeps its id");
        let crate::ObjectKind::Text { style, .. } = &first.objects[0].kind else { panic!() };
        assert_eq!(style.size, Some(40.0), "and its look");
        assert_eq!(again.slide(3).unwrap().1.text_for(&Role::primary()), Some("D"));
        // Unchanged text gives the same song.
        assert_eq!(song.reflowed(&crate::to_song_text(&song)), song);
    }

    #[test]
    fn the_copyright_line_leaves_out_what_is_not_known() {
        let full = Copyright {
            artist: "The Band".to_owned(),
            author: "A. Writer".to_owned(),
            publisher: "Songs Ltd".to_owned(),
            year: "2024".to_owned(),
            ccli_song: "1234567".to_owned(),
        };
        assert_eq!(
            full.line("Steady Ground", "998877"),
            "\u{201c}Steady Ground\u{201d}  \u{b7}  A. Writer  \u{b7}  \u{a9} 2024 Songs Ltd  \u{b7}  CCLI Song # 1234567  \u{b7}  CCLI License # 998877"
        );
        let little = Copyright { artist: "The Band".to_owned(), ..Copyright::default() };
        assert_eq!(little.line("Song", ""), "\u{201c}Song\u{201d}  \u{b7}  The Band");
        assert!(Copyright::default().is_empty());
    }

    #[test]
    fn slides_are_removed_copied_in_and_start_groups() {
        let song = crate::parse_song_text("Song", "[Verse 1]\nA\n\nB\n\nC\n\n[Chorus]\nD\n");
        let words = |song: &Presentation| {
            song.slides()
                .map(|(g, s)| format!("{}:{}", g.name, s.objects[0].as_text().unwrap_or("")))
                .collect::<Vec<_>>()
        };
        // Removing the chorus's only slide removes the chorus.
        let fewer = song.without_slide(3).unwrap();
        assert_eq!(words(&fewer), ["Verse 1:A", "Verse 1:B", "Verse 1:C"]);
        assert!(song.without_slide(9).is_none());
        // A copied slide goes in after another, in its group, with ids of its own.
        let copied = song.slide(0).unwrap().1.clone();
        let more = song.with_slide_after(1, &copied).unwrap();
        assert_eq!(words(&more), ["Verse 1:A", "Verse 1:B", "Verse 1:A", "Verse 1:C", "Chorus:D"]);
        assert_ne!(more.slide(2).unwrap().1.id, copied.id);
        // Marking B as the start of a Pre-chorus splits the verse there.
        let split = song.with_group_starting(1, "Pre-chorus").unwrap();
        assert_eq!(words(&split), ["Verse 1:A", "Pre-chorus:B", "Pre-chorus:C", "Chorus:D"]);
        assert_eq!(split.groups[1].color, crate::group_color("Pre-chorus"));
        // On a group's first slide it renames the group.
        let renamed = song.with_group_starting(3, "Bridge").unwrap();
        assert_eq!(words(&renamed), ["Verse 1:A", "Verse 1:B", "Verse 1:C", "Bridge:D"]);
    }

    #[test]
    fn a_slide_added_at_the_end_is_laid_out_like_the_one_before_with_no_words() {
        let song = crate::parse_song_text("Song", "[Verse 1]\nOne\n--\nОдин\n\n[Chorus]\nTwo\n--\nДва\n");
        let (longer, at) = song.with_slide_added();
        assert_eq!((longer.slide_count(), at), (3, 2));
        let (group, added) = longer.slide(at).unwrap();
        assert_eq!(group.name, "Chorus");
        let (_, before) = longer.slide(1).unwrap();
        assert_eq!(added.objects.len(), before.objects.len());
        for (new, old) in added.objects.iter().zip(&before.objects) {
            assert_eq!((new.role.clone(), new.frame, new.as_text()), (old.role.clone(), old.frame, Some("")));
            assert_ne!(new.id, old.id);
        }
        // A song with nothing in it gets a first slide with a main box.
        let empty = Presentation {
            id: Uuid::new_v4(),
            title: "Empty".to_owned(),
            groups: Vec::new(),
            copyright: Copyright::default(),
            theme: None,
        };
        let (first, at) = empty.with_slide_added();
        assert_eq!((first.slide_count(), at), (1, 0));
        assert_eq!(first.slide(0).unwrap().1.objects[0].role, Some(Role::primary()));
    }
}
