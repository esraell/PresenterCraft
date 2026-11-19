use serde::{Deserialize, Serialize};

use uuid::Uuid;

use crate::types::{Object, ObjectKind, Rgba, Role, Slide};

/// A rectangle in slide units (the slide is 1920 x 1080).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SlideRect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
}

/// Where the text sits between the top and bottom of its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// How the letters of the text are changed when it is drawn. The stored text is untouched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Caps {
    #[default]
    None,
    AllCaps,
    Lowercase,
    TitleCase,
}

impl Caps {
    /// The text as it is drawn under this setting.
    pub fn apply<'a>(self, text: &'a str) -> std::borrow::Cow<'a, str> {
        match self {
            Caps::None => text.into(),
            Caps::AllCaps => text.to_uppercase().into(),
            Caps::Lowercase => text.to_lowercase().into(),
            Caps::TitleCase => {
                let mut out = String::with_capacity(text.len());
                let mut start = true;
                for c in text.chars() {
                    if start && c.is_alphabetic() {
                        out.extend(c.to_uppercase());
                    } else {
                        out.extend(c.to_lowercase());
                    }
                    // An apostrophe is inside a word: "don't" becomes "Don't", not "Don'T".
                    start = !(c.is_alphanumeric() || c == '\'' || c == '\u{2019}');
                }
                out.into()
            }
        }
    }
}

/// A soft shadow behind text.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Shadow {
    pub color: Rgba,
    /// From 0 (invisible) to 1 (solid).
    pub opacity: f32,
    /// The direction the shadow falls in, in degrees, counted anticlockwise from the right:
    /// 315 is down and to the right.
    pub angle: f32,
    /// How far the shadow falls, in slide units.
    pub offset: f32,
    /// How far the shadow spreads, in slide units.
    pub blur: f32,
}

impl Default for Shadow {
    fn default() -> Shadow {
        Shadow { color: Rgba::BLACK, opacity: 0.5, angle: 315.0, offset: 10.0, blur: 15.0 }
    }
}

/// How the text box with one role is placed and drawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxStyle {
    pub role: Role,
    pub rect: SlideRect,
    pub font_family: String,
    pub weight: u16,
    pub italic: bool,
    /// Font size in slide units.
    pub size: f32,
    /// Text that does not fit is shrunk no further than this.
    pub min_size: f32,
    pub fill: Rgba,
    pub align: Align,
    pub outline_width: f32,
    pub outline_color: Rgba,
    pub shadow: bool,
    pub shadow_look: Shadow,
    pub valign: VAlign,
    pub caps: Caps,
    pub underline: bool,
    pub strike: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Theme {
    /// Stays the same when the theme is renamed; its file is named by it.
    #[serde(default)]
    pub id: Uuid,
    pub name: String,
    pub boxes: Vec<BoxStyle>,
    /// Where a lone `primary` box goes when the slide has no other box.
    pub solo_primary: SlideRect,
    /// The colour behind everything on a slide. `None` is black.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgba>,
}

/// The two layouts a theme has, each shown as a slide in the Theme Editor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Template {
    /// Every box, as slides with two languages use them.
    Full,
    /// The main-language box alone, as slides with one language use it.
    Alone,
}

const THEME_FORMAT: &str = "presentercraft-theme";
/// The newest theme file version this code reads and the one it writes.
pub const THEME_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ThemeFile {
    format: String,
    version: u32,
    theme: Theme,
}

/// A role made from a box's name: lower case, with anything but letters and digits as dashes.
fn role_from_name(name: &str) -> Role {
    let mut slug = String::new();
    for c in name.trim().chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            slug.push(c);
        } else if !slug.ends_with('-') {
            slug.push('-');
        }
    }
    let slug = slug.trim_matches('-');
    Role(if slug.is_empty() { "box".to_owned() } else { slug.to_owned() })
}

impl Theme {
    /// The theme used until themes can be edited: a large white main language above a smaller
    /// orange translation, both outlined and shadowed.
    pub fn builtin() -> Theme {
        let base = |role: Role, rect: SlideRect, size: f32, min_size: f32, fill: Rgba, outline: f32| BoxStyle {
            role,
            rect,
            font_family: "Helvetica Neue".to_owned(),
            weight: 700,
            italic: false,
            size,
            min_size,
            fill,
            align: Align::Center,
            outline_width: outline,
            outline_color: Rgba::BLACK,
            shadow: true,
            shadow_look: Shadow::default(),
            valign: VAlign::Middle,
            caps: Caps::None,
            underline: false,
            strike: false,
        };
        Theme {
            id: Uuid::nil(),
            name: "Built-in".to_owned(),
            boxes: vec![
                base(
                    Role::primary(),
                    SlideRect { x: 120.0, y: 170.0, w: 1680.0, h: 400.0 },
                    96.0,
                    40.0,
                    Rgba::WHITE,
                    6.0,
                ),
                base(
                    Role::translation(),
                    SlideRect { x: 120.0, y: 610.0, w: 1680.0, h: 320.0 },
                    64.0,
                    30.0,
                    Rgba::rgb(255, 150, 40),
                    4.0,
                ),
                // Where a Bible verse is from, small along the bottom. Songs do not use it.
                BoxStyle {
                    weight: 500,
                    ..base(
                        Role::reference(),
                        SlideRect { x: 120.0, y: 950.0, w: 1680.0, h: 80.0 },
                        44.0,
                        24.0,
                        Rgba::WHITE,
                        2.0,
                    )
                },
            ],
            solo_primary: SlideRect { x: 120.0, y: 240.0, w: 1680.0, h: 600.0 },
            background: None,
        }
    }

    /// One of the theme's layouts as a slide the editor can work on: a text box for each of the
    /// theme's boxes, saying "Text", carrying the box's whole look and place.
    pub fn template_slide(&self, template: Template) -> Slide {
        let objects = self
            .boxes
            .iter()
            .filter(|style| template == Template::Full || style.role == Role::primary())
            .map(|style| {
                let mut object = Object::text(style.role.clone(), "Text");
                object.frame = Some(if template == Template::Alone { self.solo_primary } else { style.rect });
                let ObjectKind::Text { style: own, .. } = &mut object.kind;
                *own = TextStyle::all(style);
                object
            })
            .collect();
        Slide::new(objects)
    }

    /// The theme with one of its layouts replaced by what the editor made of it. Each text box
    /// on the slide becomes a box of the theme: its role is kept, or made from its name if it
    /// is new. In the main-language-alone layout only the box's place belongs to the layout;
    /// its look is the main language's everywhere. A theme always keeps a main-language box.
    pub fn with_template(&self, template: Template, slide: &Slide) -> Theme {
        let mut theme = self.clone();
        let starting = |role: &Role| self.style_for(role).or_else(|| self.style_for(&Role::primary())).cloned();
        let made: Vec<BoxStyle> = slide
            .objects
            .iter()
            .filter_map(|object| {
                let ObjectKind::Text { style, .. } = &object.kind;
                let role = object.role.clone().unwrap_or_else(|| role_from_name(&object.name));
                let base = starting(&role)?;
                let mut made = style.over(&base);
                made.rect = object.frame.unwrap_or(base.rect);
                made.role = role;
                Some(made)
            })
            .collect();
        match template {
            Template::Full => {
                let mut boxes: Vec<BoxStyle> = Vec::new();
                for style in made {
                    if !boxes.iter().any(|b| b.role == style.role) {
                        boxes.push(style);
                    }
                }
                if !boxes.iter().any(|b| b.role == Role::primary())
                    && let Some(kept) = self.style_for(&Role::primary())
                {
                    boxes.insert(0, kept.clone());
                }
                theme.boxes = boxes;
            }
            Template::Alone => {
                if let Some(alone) = made.into_iter().find(|style| style.role == Role::primary()) {
                    theme.solo_primary = alone.rect;
                    if let Some(primary) = theme.boxes.iter_mut().find(|b| b.role == Role::primary()) {
                        let place = primary.rect;
                        *primary = BoxStyle { rect: place, ..alone };
                    }
                }
            }
        }
        theme
    }

    /// The theme as a file's text.
    pub fn to_json(&self) -> String {
        let file = ThemeFile { format: THEME_FORMAT.to_owned(), version: THEME_VERSION, theme: self.clone() };
        // A theme has no value that JSON cannot hold, so this cannot fail.
        let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
        text.push('\n');
        text
    }

    /// Reads a theme file. The error is a plain sentence for the operator.
    pub fn from_json(text: &str) -> Result<Theme, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("this is not a PresenterCraft theme file ({e})"))?;
        if value.get("format").and_then(|f| f.as_str()) != Some(THEME_FORMAT) {
            return Err("this is not a PresenterCraft theme file".to_owned());
        }
        let version = value.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if version == 0 || version > u64::from(THEME_VERSION) {
            return Err(format!(
                "the theme file was made by a newer PresenterCraft (format {version}; this one reads up to {THEME_VERSION})"
            ));
        }
        let file: ThemeFile = serde_json::from_value(value).map_err(|e| format!("the theme file is damaged ({e})"))?;
        if file.theme.style_for(&Role::primary()).is_none() {
            return Err("the theme file has no main-language box".to_owned());
        }
        Ok(file.theme)
    }

    /// The style a text object is drawn with: the theme's style for its role (the main
    /// language's when it has no role, or one the theme does not know) with the object's own
    /// settings put over it.
    pub fn text_style(&self, role: Option<&Role>, own: &TextStyle) -> Option<BoxStyle> {
        let base = role.and_then(|role| self.style_for(role)).or_else(|| self.style_for(&Role::primary()))?;
        Some(own.over(base))
    }

    pub fn style_for(&self, role: &Role) -> Option<&BoxStyle> {
        self.boxes.iter().find(|b| &b.role == role)
    }
}

/// What a text object sets for itself. Every value that is `None` comes from the theme, so a
/// change to the theme still reaches every object that has not been restyled by hand.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TextStyle {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Font size in slide units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_width: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outline_color: Option<Rgba>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow_look: Option<Shadow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valign: Option<VAlign>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caps: Option<Caps>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
}

impl TextStyle {
    /// True when nothing is set, so the object looks exactly as the theme has it.
    pub fn is_default(&self) -> bool {
        *self == TextStyle::default()
    }

    /// Every setting of a box's look, so a text object carries the look whole.
    pub fn all(look: &BoxStyle) -> TextStyle {
        TextStyle {
            font_family: Some(look.font_family.clone()),
            size: Some(look.size),
            weight: Some(look.weight),
            italic: Some(look.italic),
            fill: Some(look.fill),
            align: Some(look.align),
            outline_width: Some(look.outline_width),
            outline_color: Some(look.outline_color),
            shadow: Some(look.shadow),
            shadow_look: Some(look.shadow_look),
            valign: Some(look.valign),
            caps: Some(look.caps),
            underline: Some(look.underline),
            strike: Some(look.strike),
        }
    }

    /// A theme style with this object's own settings put over it.
    pub fn over(&self, base: &BoxStyle) -> BoxStyle {
        let size = self.size.unwrap_or(base.size);
        BoxStyle {
            role: base.role.clone(),
            rect: base.rect,
            font_family: self.font_family.clone().unwrap_or_else(|| base.font_family.clone()),
            weight: self.weight.unwrap_or(base.weight),
            italic: self.italic.unwrap_or(base.italic),
            size,
            // A size chosen by hand can be smaller than the theme's floor.
            min_size: base.min_size.min(size),
            fill: self.fill.unwrap_or(base.fill),
            align: self.align.unwrap_or(base.align),
            outline_width: self.outline_width.unwrap_or(base.outline_width),
            outline_color: self.outline_color.unwrap_or(base.outline_color),
            shadow: self.shadow.unwrap_or(base.shadow),
            shadow_look: self.shadow_look.unwrap_or(base.shadow_look),
            valign: self.valign.unwrap_or(base.valign),
            caps: self.caps.unwrap_or(base.caps),
            underline: self.underline.unwrap_or(base.underline),
            strike: self.strike.unwrap_or(base.strike),
        }
    }

    /// The settings that make `wanted` out of `base`: only what differs is kept.
    pub fn between(base: &BoxStyle, wanted: &BoxStyle) -> TextStyle {
        TextStyle {
            font_family: (wanted.font_family != base.font_family).then(|| wanted.font_family.clone()),
            size: (wanted.size != base.size).then_some(wanted.size),
            weight: (wanted.weight != base.weight).then_some(wanted.weight),
            italic: (wanted.italic != base.italic).then_some(wanted.italic),
            fill: (wanted.fill != base.fill).then_some(wanted.fill),
            align: (wanted.align != base.align).then_some(wanted.align),
            outline_width: (wanted.outline_width != base.outline_width).then_some(wanted.outline_width),
            outline_color: (wanted.outline_color != base.outline_color).then_some(wanted.outline_color),
            shadow: (wanted.shadow != base.shadow).then_some(wanted.shadow),
            shadow_look: (wanted.shadow_look != base.shadow_look).then_some(wanted.shadow_look),
            valign: (wanted.valign != base.valign).then_some(wanted.valign),
            caps: (wanted.caps != base.caps).then_some(wanted.caps),
            underline: (wanted.underline != base.underline).then_some(wanted.underline),
            strike: (wanted.strike != base.strike).then_some(wanted.strike),
        }
    }
}

/// The font families a presentation is drawn with under a theme, each once, in name order.
pub fn fonts_used(presentation: &crate::Presentation, theme: &Theme) -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    for (_, slide) in presentation.slides() {
        for object in &slide.objects {
            let crate::ObjectKind::Text { style, .. } = &object.kind;
            if let Some(drawn) = theme.text_style(object.role.as_ref(), style)
                && !families.contains(&drawn.font_family)
            {
                families.push(drawn.font_family);
            }
        }
    }
    families.sort_by_key(|name| name.to_lowercase());
    families
}

/// The fonts in `used` that are not among `installed`, comparing names without regard to case.
/// A song that names one of these is drawn with a substitute on this computer.
pub fn missing_fonts(used: &[String], installed: &[String]) -> Vec<String> {
    used.iter().filter(|name| !installed.iter().any(|have| have.eq_ignore_ascii_case(name))).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_theme_survives_its_file_and_other_files_are_refused() {
        let mut theme = Theme::builtin();
        theme.name = "Sunday".to_owned();
        theme.background = Some(Rgba::rgb(10, 20, 30));
        assert_eq!(Theme::from_json(&theme.to_json()), Ok(theme));
        assert!(Theme::from_json("{}").unwrap_err().contains("not a PresenterCraft theme file"));
        assert!(Theme::from_json("rubbish").is_err());
        let newer = Theme::builtin().to_json().replace("\"version\": 1", "\"version\": 99");
        assert!(Theme::from_json(&newer).unwrap_err().contains("newer"));
    }

    #[test]
    fn a_layout_goes_to_the_editor_and_back_unchanged() {
        let theme = Theme::builtin();
        for template in [Template::Full, Template::Alone] {
            let slide = theme.template_slide(template);
            assert_eq!(theme.with_template(template, &slide), theme);
        }
        assert_eq!(theme.template_slide(Template::Full).objects.len(), 3);
        assert_eq!(theme.template_slide(Template::Alone).objects.len(), 1);
    }

    #[test]
    fn editing_a_layout_changes_the_theme() {
        let theme = Theme::builtin();
        let mut slide = theme.template_slide(Template::Full);
        slide.objects[1].frame = Some(SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 100.0 });
        let ObjectKind::Text { style, .. } = &mut slide.objects[1].kind;
        style.size = Some(48.0);
        // A new box, with no role yet, takes one from its name.
        let mut added = Object::text(Role::primary(), "Text");
        added.role = None;
        added.name = "Verse Reference".to_owned();
        slide.objects.push(added);
        let edited = theme.with_template(Template::Full, &slide);
        let translation = edited.style_for(&Role::translation()).unwrap();
        assert_eq!((translation.rect.x, translation.size), (10.0, 48.0));
        assert!(edited.style_for(&Role("verse-reference".to_owned())).is_some());
        assert_eq!(edited.boxes.len(), 4);

        // Deleting the main box in the editor does not take it out of the theme.
        slide.objects.remove(0);
        assert!(theme.with_template(Template::Full, &slide).style_for(&Role::primary()).is_some());
    }

    #[test]
    fn the_alone_layout_moves_only_the_lone_box_and_shares_its_look() {
        let theme = Theme::builtin();
        let mut slide = theme.template_slide(Template::Alone);
        slide.objects[0].frame = Some(SlideRect { x: 0.0, y: 0.0, w: 1920.0, h: 1080.0 });
        let ObjectKind::Text { style, .. } = &mut slide.objects[0].kind;
        style.italic = Some(true);
        let edited = theme.with_template(Template::Alone, &slide);
        assert_eq!(edited.solo_primary.w, 1920.0);
        let primary = edited.style_for(&Role::primary()).unwrap();
        assert!(primary.italic);
        assert_eq!(primary.rect, theme.style_for(&Role::primary()).unwrap().rect);
    }

    #[test]
    fn capitalisation_changes_only_how_text_is_drawn() {
        assert_eq!(Caps::None.apply("Свет rises"), "Свет rises");
        assert_eq!(Caps::AllCaps.apply("Свет rises"), "СВЕТ RISES");
        assert_eq!(Caps::Lowercase.apply("Свет RISES"), "свет rises");
        assert_eq!(Caps::TitleCase.apply("i don't KNOW\nсвет встаёт"), "I Don't Know\nСвет Встаёт");
    }

    #[test]
    fn new_style_settings_are_kept_only_where_they_differ_from_the_theme() {
        let theme = Theme::builtin();
        let base = theme.style_for(&Role::primary()).unwrap();
        let mut wanted = base.clone();
        wanted.valign = VAlign::Bottom;
        wanted.underline = true;
        wanted.shadow_look.offset = 3.0;
        let own = TextStyle::between(base, &wanted);
        assert_eq!(own.valign, Some(VAlign::Bottom));
        assert_eq!(own.underline, Some(true));
        assert_eq!(own.shadow_look.map(|s| s.offset), Some(3.0));
        assert_eq!((own.caps, own.strike, own.size), (None, None, None));
        assert_eq!(own.over(base), wanted);
        // An old file with none of these fields still reads, and looks as the theme has it.
        let old: TextStyle = serde_json::from_str("{\"size\": 80.0}").unwrap();
        assert_eq!(old.over(base).valign, VAlign::Middle);
    }

    #[test]
    fn the_fonts_a_song_uses_are_listed_once_each() {
        let theme = Theme::builtin();
        let mut song = crate::parse_song_text("S", "A\n--\nБ\n\nC\n");
        assert_eq!(fonts_used(&song, &theme), vec![theme.boxes[0].font_family.clone()]);
        let crate::ObjectKind::Text { style, .. } = &mut song.groups[0].slides[1].objects[0].kind;
        style.font_family = Some("Georgia".to_owned());
        assert_eq!(fonts_used(&song, &theme), vec!["Georgia".to_owned(), theme.boxes[0].font_family.clone()]);
        assert!(fonts_used(&crate::parse_song_text("S", ""), &theme).is_empty());
    }

    #[test]
    fn missing_fonts_are_those_this_computer_does_not_have() {
        let used = vec!["Georgia".to_owned(), "Helvetica Neue".to_owned(), "Brand Sans".to_owned()];
        let installed = vec!["georgia".to_owned(), "Helvetica Neue".to_owned(), "Arial".to_owned()];
        assert_eq!(missing_fonts(&used, &installed), vec!["Brand Sans".to_owned()]);
        assert!(missing_fonts(&[], &installed).is_empty());
        assert_eq!(missing_fonts(&used, &[]).len(), 3);
    }

    #[test]
    fn an_unset_style_is_exactly_the_theme() {
        let theme = Theme::builtin();
        let base = &theme.boxes[1];
        assert_eq!(TextStyle::default().over(base), *base);
        assert!(TextStyle::default().is_default());
    }

    #[test]
    fn only_what_was_changed_is_kept_and_the_rest_still_follows_the_theme() {
        let mut theme = Theme::builtin();
        let base = theme.boxes[0].clone();
        let mut wanted = base.clone();
        wanted.fill = Rgba::rgb(255, 0, 0);
        wanted.size = 20.0;
        let own = TextStyle::between(&base, &wanted);
        assert_eq!(own, TextStyle { fill: Some(Rgba::rgb(255, 0, 0)), size: Some(20.0), ..TextStyle::default() });
        assert_eq!(own.over(&base), BoxStyle { min_size: 20.0, ..wanted });
        // The theme changes its font: the object follows, and keeps its own colour and size.
        theme.boxes[0].font_family = "Another".to_owned();
        let drawn = theme.text_style(Some(&Role::primary()), &own).unwrap();
        assert_eq!((drawn.font_family.as_str(), drawn.fill, drawn.size), ("Another", Rgba::rgb(255, 0, 0), 20.0));
    }

    #[test]
    fn an_object_without_a_known_role_uses_the_main_style() {
        let theme = Theme::builtin();
        let none = theme.text_style(None, &TextStyle::default()).unwrap();
        let unknown = theme.text_style(Some(&Role("notes".to_owned())), &TextStyle::default()).unwrap();
        assert_eq!(none, theme.boxes[0]);
        assert_eq!(unknown, theme.boxes[0]);
    }
}
