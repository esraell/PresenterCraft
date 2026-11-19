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
    /// Each line but the last stretched to fill the box's width.
    Justify,
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
    /// Each word capitalized, except short words such as "the" and "of" inside a line.
    TitleCase,
    /// Every word capitalized.
    StartCase,
}

/// Words left in lower case inside a line in title case.
const SMALL_WORDS: [&str; 16] =
    ["a", "an", "and", "as", "at", "but", "by", "for", "in", "nor", "of", "on", "or", "the", "to", "with"];

impl Caps {
    /// The text as it is drawn under this setting.
    pub fn apply<'a>(self, text: &'a str) -> std::borrow::Cow<'a, str> {
        match self {
            Caps::None => text.into(),
            Caps::AllCaps => text.to_uppercase().into(),
            Caps::Lowercase => text.to_lowercase().into(),
            Caps::StartCase => start_case(text).into(),
            Caps::TitleCase => text
                .split('\n')
                .map(|line| {
                    let started = start_case(line);
                    let mut first = true;
                    started
                        .split(' ')
                        .map(|word| {
                            let small = !first && SMALL_WORDS.contains(&word.to_lowercase().as_str());
                            first = first && word.is_empty();
                            if small { word.to_lowercase() } else { word.to_owned() }
                        })
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .collect::<Vec<_>>()
                .join("\n")
                .into(),
        }
    }
}

/// Every word with a capital first letter and the rest in lower case.
fn start_case(text: &str) -> String {
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
    out
}

/// What happens when the text is too big or too small for its box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scaling {
    /// Nothing: text that does not fit spills out of its box.
    None,
    /// Text that does not fit is made smaller until it does.
    #[default]
    TextDown,
    /// Text smaller than its box is made bigger until it fills it.
    TextUp,
    /// Both: the text always fills its box.
    TextUpOrDown,
}

/// A change made to the text's lines when it is drawn. The stored text is untouched.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineTransform {
    #[default]
    None,
    /// All the lines are run together into one, which wraps where it must.
    RemoveLineReturns,
    /// Each line break becomes " / ", so a verse reads on one line.
    ReplaceLineReturns,
    OneWordPerLine,
    OneCharacterPerLine,
}

impl LineTransform {
    /// The text as it is drawn under this setting.
    pub fn apply<'a>(self, text: &'a str) -> std::borrow::Cow<'a, str> {
        match self {
            LineTransform::None => text.into(),
            LineTransform::RemoveLineReturns => text.split_whitespace().collect::<Vec<_>>().join(" ").into(),
            LineTransform::ReplaceLineReturns => {
                text.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join(" / ").into()
            }
            LineTransform::OneWordPerLine => text.split_whitespace().collect::<Vec<_>>().join("\n").into(),
            LineTransform::OneCharacterPerLine => {
                let letters: Vec<String> = text.chars().filter(|c| !c.is_whitespace()).map(String::from).collect();
                letters.join("\n").into()
            }
        }
    }
}

impl BoxStyle {
    /// Makes the same change to this look as was made between `before` and `after`: whatever
    /// differs between those two is copied from `after`; the rest of this look is left alone.
    /// Its role, name and place are never touched.
    pub fn follow_change(&mut self, before: &BoxStyle, after: &BoxStyle) {
        macro_rules! follow {
            ($($field:ident),*) => {
                $(if before.$field != after.$field {
                    self.$field = after.$field.clone();
                })*
            };
        }
        follow!(
            font_family,
            weight,
            italic,
            size,
            fill,
            align,
            outline_width,
            outline_color,
            shadow,
            shadow_look,
            valign,
            caps,
            underline,
            strike,
            scaling,
            line_transform,
            line_height,
            letter_spacing,
            margins
        );
        self.min_size = self.min_size.min(self.size);
    }

    /// What the box is called: its own name if it has one, otherwise its role's.
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.role.label())
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
    /// What the box is called in lists and menus, when it has been given a name of its own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
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
    #[serde(default)]
    pub scaling: Scaling,
    #[serde(default)]
    pub line_transform: LineTransform,
    /// The distance from one line to the next, as a multiple of the font size.
    #[serde(default = "usual_line_height")]
    pub line_height: f32,
    /// Extra room between letters, in slide units at the box's font size.
    #[serde(default)]
    pub letter_spacing: f32,
    /// Room kept clear inside the box's edges, in slide units: left, top, right, bottom.
    #[serde(default)]
    pub margins: [f32; 4],
}

/// The distance from one line to the next that text has unless it is given another.
pub const LINE_HEIGHT: f32 = 1.25;

fn usual_line_height() -> f32 {
    LINE_HEIGHT
}

/// One layout of a theme: a set of named boxes, each with its place and its look. A slide is
/// drawn with one template.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThemeTemplate {
    /// Stays the same when the template is renamed. Slides that ask for a template name it by this.
    pub id: Uuid,
    pub name: String,
    pub boxes: Vec<BoxStyle>,
}

/// A theme: any number of templates, and a colour behind them all.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "ThemeOnDisk")]
pub struct Theme {
    /// Stays the same when the theme is renamed; its file is named by it.
    pub id: Uuid,
    pub name: String,
    /// At least one. The first is the one used when nothing else decides.
    pub templates: Vec<ThemeTemplate>,
    /// The colour behind everything on a slide. `None` is black.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<Rgba>,
}

/// A theme as its file has it. Files written before themes had free templates hold one set
/// of boxes and a second place for the main box; those become two templates.
#[derive(Deserialize)]
struct ThemeOnDisk {
    #[serde(default)]
    id: Uuid,
    name: String,
    #[serde(default)]
    templates: Vec<ThemeTemplate>,
    #[serde(default)]
    background: Option<Rgba>,
    #[serde(default)]
    boxes: Vec<BoxStyle>,
    #[serde(default)]
    solo_primary: Option<SlideRect>,
    #[serde(default)]
    template_names: [Option<String>; 2],
}

impl From<ThemeOnDisk> for Theme {
    fn from(file: ThemeOnDisk) -> Theme {
        let mut templates = file.templates;
        if templates.is_empty() && !file.boxes.is_empty() {
            let [first, second] = file.template_names;
            let alone: Vec<BoxStyle> = file
                .boxes
                .iter()
                .filter(|style| style.role != Role::translation())
                .map(|style| match file.solo_primary {
                    Some(place) if style.role == Role::primary() => BoxStyle { rect: place, ..style.clone() },
                    _ => style.clone(),
                })
                .collect();
            // The two get ids made from the theme's, so they are the same at every start
            // until the theme is next saved, and what points at them still finds them.
            let made = |n: u128| Uuid::from_u128(file.id.as_u128() ^ n);
            templates = vec![
                ThemeTemplate {
                    id: made(1),
                    name: first.unwrap_or_else(|| "Two languages".to_owned()),
                    boxes: file.boxes,
                },
                ThemeTemplate { id: made(2), name: second.unwrap_or_else(|| "One language".to_owned()), boxes: alone },
            ];
        }
        Theme { id: file.id, name: file.name, templates, background: file.background }
    }
}

const THEME_FORMAT: &str = "presentercraft-theme";
/// The newest theme file version this code reads and the one it writes.
pub const THEME_VERSION: u32 = 2;

#[derive(Serialize)]
struct ThemeFileOut<'a> {
    format: &'a str,
    version: u32,
    theme: &'a Theme,
}

#[derive(Deserialize)]
struct ThemeFileIn {
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

/// Where a box with no place of its own goes: the middle of the slide.
const MIDDLE: SlideRect = SlideRect { x: 120.0, y: 240.0, w: 1680.0, h: 600.0 };

impl ThemeTemplate {
    pub fn style_for(&self, role: &Role) -> Option<&BoxStyle> {
        self.boxes.iter().find(|b| &b.role == role)
    }

    /// The box an object with this role is drawn as: the box with that role, or the main
    /// language's box if it has no role or one the template does not know, or the first box.
    pub fn box_for(&self, role: Option<&Role>) -> Option<&BoxStyle> {
        role.and_then(|role| self.style_for(role)).or_else(|| self.style_for(&Role::primary())).or(self.boxes.first())
    }

    /// The look a text object is drawn with: its box's look with the object's own settings
    /// put over it.
    pub fn text_style(&self, role: Option<&Role>, own: &TextStyle) -> Option<BoxStyle> {
        Some(own.over(self.box_for(role)?))
    }

    /// Where an object with this role sits when it has no place of its own.
    pub fn place_for(&self, role: Option<&Role>) -> SlideRect {
        self.box_for(role).map_or(MIDDLE, |style| style.rect)
    }

    /// A role no box of this template has yet, made from a name: "Verse Text" gives
    /// `verse-text`, or `verse-text-2` if that is taken.
    pub fn unused_role(&self, name: &str, also_taken: &[Role]) -> Role {
        let base = role_from_name(name);
        let taken = |role: &Role| self.style_for(role).is_some() || also_taken.contains(role);
        if !taken(&base) {
            return base;
        }
        (2..).map(|n| Role(format!("{}-{n}", base.0))).find(|role| !taken(role)).unwrap_or(base)
    }

    /// The template as a slide the editor can work on: a text box for each of its boxes,
    /// saying "Text", carrying the box's whole look, place and name.
    pub fn as_slide(&self) -> Slide {
        let objects = self
            .boxes
            .iter()
            .map(|style| {
                let mut object = Object::text(style.role.clone(), "Text");
                object.name = style.label();
                object.frame = Some(style.rect);
                if let ObjectKind::Text { style: own, .. } = &mut object.kind {
                    *own = TextStyle::all(style);
                }
                object
            })
            .collect();
        let mut slide = Slide::new(objects);
        slide.template = Some(self.id);
        slide
    }

    /// The template with its boxes replaced by what the editor made of that slide. Each text
    /// box becomes a box of the template, keeping its role (or taking one from its name if it
    /// is new), its place, its look and its name. Two boxes cannot share a role: the first on
    /// the slide keeps it.
    pub fn with_slide(&self, slide: &Slide) -> ThemeTemplate {
        let mut boxes: Vec<BoxStyle> = Vec::new();
        for object in &slide.objects {
            // A theme is made of text boxes only.
            let ObjectKind::Text { style, .. } = &object.kind else { continue };
            let role = object.role.clone().unwrap_or_else(|| role_from_name(&object.name));
            if boxes.iter().any(|have| have.role == role) {
                continue;
            }
            // A new box starts from the look of the box its role would otherwise fall back to.
            let Some(base) = self
                .box_for(Some(&role))
                .or(boxes.first())
                .cloned()
                .or_else(|| Theme::builtin().templates.first().and_then(|t| t.boxes.first()).cloned())
            else {
                continue;
            };
            let mut look = style.over(&base);
            look.rect = object.frame.unwrap_or(base.rect);
            // A name is kept only when it says more than the role does.
            let name = object.name.trim();
            look.name = (!name.is_empty() && name != role.label()).then(|| name.to_owned());
            look.role = role;
            boxes.push(look);
        }
        ThemeTemplate { id: self.id, name: self.name.clone(), boxes }
    }
}

impl Theme {
    /// The theme the app starts with: a large white main language above a smaller orange
    /// translation, both outlined and shadowed, with a small line for a Bible reference; and
    /// a second template for slides with one language, where the main language is centred.
    pub fn builtin() -> Theme {
        let base = |role: Role, rect: SlideRect, size: f32, min_size: f32, fill: Rgba, outline: f32| BoxStyle {
            role,
            name: None,
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
            scaling: Scaling::TextDown,
            line_transform: LineTransform::None,
            line_height: LINE_HEIGHT,
            letter_spacing: 0.0,
            margins: [0.0; 4],
        };
        let main =
            base(Role::primary(), SlideRect { x: 120.0, y: 170.0, w: 1680.0, h: 400.0 }, 96.0, 40.0, Rgba::WHITE, 6.0);
        let translation = base(
            Role::translation(),
            SlideRect { x: 120.0, y: 610.0, w: 1680.0, h: 320.0 },
            64.0,
            30.0,
            Rgba::rgb(255, 150, 40),
            4.0,
        );
        // Where a Bible verse is from, small along the bottom. Songs do not use it.
        let reference = BoxStyle {
            weight: 500,
            ..base(
                Role::reference(),
                SlideRect { x: 120.0, y: 950.0, w: 1680.0, h: 80.0 },
                44.0,
                24.0,
                Rgba::WHITE,
                2.0,
            )
        };
        let main_alone = BoxStyle { rect: MIDDLE, ..main.clone() };
        Theme {
            id: Uuid::nil(),
            name: "Built-in".to_owned(),
            templates: vec![
                ThemeTemplate {
                    id: Uuid::from_u128(1),
                    name: "Two languages".to_owned(),
                    boxes: vec![main, translation, reference.clone()],
                },
                ThemeTemplate {
                    id: Uuid::from_u128(2),
                    name: "One language".to_owned(),
                    boxes: vec![main_alone, reference],
                },
            ],
            background: None,
        }
    }

    /// The template a slide is drawn with. A slide that names a template gets it. Otherwise
    /// the template that suits it best: the first that has a box for every role on the slide
    /// and no language box the slide leaves empty. So with the built-in theme a slide with a
    /// translation uses "Two languages" and one without uses "One language".
    pub fn template_for(&self, slide: &Slide) -> Option<&ThemeTemplate> {
        if let Some(named) = slide.template.and_then(|id| self.templates.iter().find(|t| t.id == id)) {
            return Some(named);
        }
        let on_slide: Vec<&Role> = slide.objects.iter().filter_map(|o| o.role.as_ref()).collect();
        let languages = [Role::primary(), Role::translation()];
        self.templates.iter().min_by_key(|template| {
            let missing = on_slide.iter().filter(|role| template.style_for(role).is_none()).count();
            let unused =
                languages.iter().filter(|role| template.style_for(role).is_some() && !on_slide.contains(role)).count();
            (missing, unused)
        })
    }

    /// The look a text object on a slide is drawn with.
    pub fn text_style_on(&self, slide: &Slide, role: Option<&Role>, own: &TextStyle) -> Option<BoxStyle> {
        self.template_for(slide)?.text_style(role, own)
    }

    /// Where an object on a slide sits when it has no place of its own.
    pub fn place_on(&self, slide: &Slide, role: Option<&Role>) -> SlideRect {
        self.template_for(slide).map_or(MIDDLE, |template| template.place_for(role))
    }

    /// The box with a role in the theme's first template.
    pub fn style_for(&self, role: &Role) -> Option<&BoxStyle> {
        self.templates.first()?.style_for(role)
    }

    /// The look of a text object under the theme's first template.
    pub fn text_style(&self, role: Option<&Role>, own: &TextStyle) -> Option<BoxStyle> {
        self.templates.first()?.text_style(role, own)
    }

    /// Adds a template: a copy of the one at `index`, placed after it. Returns its place.
    pub fn add_template(&mut self, index: usize) -> usize {
        let taken = |name: &str| self.templates.iter().any(|t| t.name == name);
        let name = (self.templates.len() + 1..).map(|n| format!("Template {n}")).find(|name| !taken(name));
        let boxes = self.templates.get(index).map(|t| t.boxes.clone()).unwrap_or_default();
        let at = (index + 1).min(self.templates.len());
        self.templates.insert(at, ThemeTemplate { id: Uuid::new_v4(), name: name.unwrap_or_default(), boxes });
        at
    }

    /// Removes a template. The last one cannot be removed. Returns true if it was removed.
    pub fn remove_template(&mut self, index: usize) -> bool {
        if self.templates.len() <= 1 || index >= self.templates.len() {
            return false;
        }
        self.templates.remove(index);
        true
    }

    /// The theme as a file's text.
    pub fn to_json(&self) -> String {
        let file = ThemeFileOut { format: THEME_FORMAT, version: THEME_VERSION, theme: self };
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
        let file: ThemeFileIn =
            serde_json::from_value(value).map_err(|e| format!("the theme file is damaged ({e})"))?;
        if file.theme.templates.is_empty() {
            return Err("the theme file has no templates".to_owned());
        }
        Ok(file.theme)
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scaling: Option<Scaling>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_transform: Option<LineTransform>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_height: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub letter_spacing: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub margins: Option<[f32; 4]>,
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
            scaling: Some(look.scaling),
            line_transform: Some(look.line_transform),
            line_height: Some(look.line_height),
            letter_spacing: Some(look.letter_spacing),
            margins: Some(look.margins),
        }
    }

    /// A theme style with this object's own settings put over it.
    pub fn over(&self, base: &BoxStyle) -> BoxStyle {
        let size = self.size.unwrap_or(base.size);
        BoxStyle {
            role: base.role.clone(),
            name: base.name.clone(),
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
            scaling: self.scaling.unwrap_or(base.scaling),
            line_transform: self.line_transform.unwrap_or(base.line_transform),
            line_height: self.line_height.unwrap_or(base.line_height),
            letter_spacing: self.letter_spacing.unwrap_or(base.letter_spacing),
            margins: self.margins.unwrap_or(base.margins),
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
            scaling: (wanted.scaling != base.scaling).then_some(wanted.scaling),
            line_transform: (wanted.line_transform != base.line_transform).then_some(wanted.line_transform),
            line_height: (wanted.line_height != base.line_height).then_some(wanted.line_height),
            letter_spacing: (wanted.letter_spacing != base.letter_spacing).then_some(wanted.letter_spacing),
            margins: (wanted.margins != base.margins).then_some(wanted.margins),
        }
    }
}

/// The font families a presentation is drawn with under a theme, each once, in name order.
pub fn fonts_used(presentation: &crate::Presentation, theme: &Theme) -> Vec<String> {
    let mut families: Vec<String> = Vec::new();
    for (_, slide) in presentation.slides() {
        for object in &slide.objects {
            let crate::ObjectKind::Text { style, .. } = &object.kind else { continue };
            if let Some(drawn) = theme.text_style_on(slide, object.role.as_ref(), style)
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

    fn slide_with(roles: &[Role]) -> Slide {
        Slide::new(roles.iter().map(|role| Object::text(role.clone(), "words")).collect())
    }

    #[test]
    fn a_theme_survives_its_file_and_other_files_are_refused() {
        let mut theme = Theme::builtin();
        theme.name = "Sunday".to_owned();
        theme.background = Some(Rgba::rgb(10, 20, 30));
        theme.add_template(0);
        assert_eq!(Theme::from_json(&theme.to_json()), Ok(theme));
        assert!(Theme::from_json("{}").unwrap_err().contains("not a PresenterCraft theme file"));
        assert!(Theme::from_json("rubbish").is_err());
        let newer = Theme::builtin().to_json().replace("\"version\": 2", "\"version\": 99");
        assert!(Theme::from_json(&newer).unwrap_err().contains("newer"));
    }

    #[test]
    fn a_file_from_before_free_templates_becomes_two_templates() {
        let builtin = Theme::builtin();
        let boxes = serde_json::to_value(&builtin.templates[0].boxes).unwrap();
        let old = serde_json::json!({
            "format": "presentercraft-theme",
            "version": 1,
            "theme": {
                "id": "cc15f92e-1009-4005-a005-c60a8a341f8a",
                "name": "Old",
                "boxes": boxes,
                "solo_primary": { "x": 1.0, "y": 2.0, "w": 3.0, "h": 4.0 },
                "template_names": ["Lyrics", null]
            }
        });
        let theme = Theme::from_json(&old.to_string()).unwrap();
        assert_eq!(theme.templates.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(), ["Lyrics", "One language"]);
        assert_eq!(theme.templates[0].boxes, builtin.templates[0].boxes);
        // Written out, it is a file of the new kind that reads back the same.
        assert_eq!(Theme::from_json(&theme.to_json()), Ok(theme.clone()));
        assert!(theme.to_json().contains("\"version\": 2"));
        // Read again, the templates have the same ids.
        let again = Theme::from_json(&old.to_string()).unwrap();
        assert_eq!(again.templates[1].id, theme.templates[1].id);
        assert_ne!(theme.templates[0].id, theme.templates[1].id);
        // The second has no translation, and the main box where it used to go alone.
        assert!(theme.templates[1].style_for(&Role::translation()).is_none());
        assert_eq!(theme.templates[1].style_for(&Role::primary()).unwrap().rect.x, 1.0);
    }

    #[test]
    fn a_slide_gets_the_template_that_suits_it_or_the_one_it_names() {
        let theme = Theme::builtin();
        let name = |slide: &Slide| theme.template_for(slide).unwrap().name.clone();
        assert_eq!(name(&slide_with(&[Role::primary(), Role::translation()])), "Two languages");
        assert_eq!(name(&slide_with(&[Role::primary()])), "One language");
        assert_eq!(name(&slide_with(&[Role::primary(), Role::reference()])), "One language");
        // A slide that names a template gets that one, whatever is on it.
        let mut named = slide_with(&[Role::primary()]);
        named.template = Some(theme.templates[0].id);
        assert_eq!(name(&named), "Two languages");
        // A name the theme does not have falls back to what suits the slide.
        named.template = Some(Uuid::from_u128(999));
        assert_eq!(name(&named), "One language");
        // The main box sits in a different place in each.
        let two = slide_with(&[Role::primary(), Role::translation()]);
        let one = slide_with(&[Role::primary()]);
        assert_ne!(theme.place_on(&two, Some(&Role::primary())), theme.place_on(&one, Some(&Role::primary())));
    }

    #[test]
    fn a_template_goes_to_the_editor_and_back_unchanged_and_edits_change_only_it() {
        let theme = Theme::builtin();
        for template in &theme.templates {
            assert_eq!(&template.with_slide(&template.as_slide()), template);
        }
        let mut slide = theme.templates[0].as_slide();
        slide.objects[1].frame = Some(SlideRect { x: 10.0, y: 20.0, w: 300.0, h: 100.0 });
        let ObjectKind::Text { style, .. } = &mut slide.objects[1].kind else { panic!("a text box") };
        style.size = Some(48.0);
        slide.objects[0].name = "English".to_owned();
        // Two new boxes with one name, given their roles as the editor gives them.
        for _ in 0..2 {
            let mut added = Object::text(Role::primary(), "Text");
            added.name = "Notes".to_owned();
            let taken: Vec<Role> = slide.objects.iter().filter_map(|o| o.role.clone()).collect();
            added.role = Some(theme.templates[0].unused_role(&added.name, &taken));
            slide.objects.push(added);
        }
        let edited = theme.templates[0].with_slide(&slide);
        let translation = edited.style_for(&Role::translation()).unwrap();
        assert_eq!((translation.rect.x, translation.size), (10.0, 48.0));
        assert_eq!(edited.style_for(&Role::primary()).unwrap().label(), "English");
        assert_eq!(edited.boxes.len(), 5);
        assert!(
            edited.style_for(&Role("notes".to_owned())).is_some()
                && edited.style_for(&Role("notes-2".to_owned())).is_some()
        );
        // Removing a box in the editor removes it from the template.
        slide.objects.remove(0);
        assert!(theme.templates[0].with_slide(&slide).style_for(&Role::primary()).is_none());
        // Names come back when the template is opened again.
        let names: Vec<String> = edited.as_slide().objects.iter().map(|o| o.name.clone()).collect();
        assert_eq!(names, ["English", "Translation", "Reference", "Notes", "Notes"]);
    }

    #[test]
    fn templates_are_added_as_copies_and_removed_but_never_all() {
        let mut theme = Theme::builtin();
        let at = theme.add_template(0);
        assert_eq!((at, theme.templates.len()), (1, 3));
        assert_eq!(theme.templates[1].boxes, theme.templates[0].boxes);
        assert_ne!(theme.templates[1].id, theme.templates[0].id);
        assert_eq!(theme.templates[1].name, "Template 3");
        assert!(theme.remove_template(1));
        assert!(theme.remove_template(1));
        assert!(!theme.remove_template(0));
        assert_eq!(theme.templates.len(), 1);
    }

    #[test]
    fn a_change_to_one_look_is_followed_by_another_without_disturbing_the_rest() {
        let theme = Theme::builtin();
        let main = theme.style_for(&Role::primary()).unwrap().clone();
        let mut translation = theme.style_for(&Role::translation()).unwrap().clone();
        let orange = translation.fill;
        let mut changed = main.clone();
        changed.italic = true;
        changed.size = 120.0;
        translation.follow_change(&main, &changed);
        // What changed is taken; its own colour, place and role are kept.
        assert!(translation.italic);
        assert_eq!(translation.size, 120.0);
        assert_eq!(translation.fill, orange);
        assert_eq!(translation.role, Role::translation());
        assert_eq!(translation.rect, theme.style_for(&Role::translation()).unwrap().rect);
    }

    #[test]
    fn line_transforms_change_only_how_text_is_drawn() {
        let text = "Light rises\nover  the hills";
        assert_eq!(LineTransform::None.apply(text), text);
        assert_eq!(LineTransform::RemoveLineReturns.apply(text), "Light rises over the hills");
        assert_eq!(LineTransform::OneWordPerLine.apply(text), "Light\nrises\nover\nthe\nhills");
        assert_eq!(LineTransform::OneCharacterPerLine.apply("a b\nв"), "a\nb\nв");
    }

    #[test]
    fn title_case_keeps_small_words_low_and_start_case_does_not() {
        assert_eq!(Caps::TitleCase.apply("the joy of the lord"), "The Joy of the Lord");
        assert_eq!(Caps::StartCase.apply("the joy of the lord"), "The Joy Of The Lord");
        assert_eq!(Caps::TitleCase.apply("light of the world\nand of the sea"), "Light of the World\nAnd of the Sea");
        assert_eq!(LineTransform::ReplaceLineReturns.apply("One line\n\nTwo line\n"), "One line / Two line");
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
        assert_eq!((old.over(base).line_height, old.over(base).letter_spacing), (LINE_HEIGHT, 0.0));
    }

    #[test]
    fn spacing_is_kept_only_where_changed_and_survives_the_file() {
        let base = Theme::builtin().templates[0].boxes[0].clone();
        let wanted =
            BoxStyle { line_height: 1.6, letter_spacing: 4.0, margins: [10.0, 20.0, 10.0, 0.0], ..base.clone() };
        let own = TextStyle::between(&base, &wanted);
        assert_eq!(own.line_height, Some(1.6));
        assert_eq!(own.margins, Some([10.0, 20.0, 10.0, 0.0]));
        assert_eq!(own.size, None);
        assert_eq!(own.over(&base), wanted);
        let again: TextStyle = serde_json::from_str(&serde_json::to_string(&own).unwrap()).unwrap();
        assert_eq!(again, own);
        // A box style from a file written before spacing could be set reads with the usual spacing.
        let mut json = serde_json::to_value(&base).unwrap();
        for field in ["line_height", "letter_spacing", "margins"] {
            json.as_object_mut().unwrap().remove(field);
        }
        let old: BoxStyle = serde_json::from_value(json).unwrap();
        assert_eq!((old.line_height, old.letter_spacing, old.margins), (LINE_HEIGHT, 0.0, [0.0; 4]));
        // A change to spacing made on one box is carried to another.
        let mut other = Theme::builtin().templates[0].boxes[1].clone();
        other.follow_change(&base, &wanted);
        assert_eq!((other.line_height, other.letter_spacing), (1.6, 4.0));
    }

    #[test]
    fn the_fonts_a_song_uses_are_listed_once_each() {
        let theme = Theme::builtin();
        let mut song = crate::parse_song_text("S", "A\n--\nБ\n\nC\n");
        assert_eq!(fonts_used(&song, &theme), vec![theme.templates[0].boxes[0].font_family.clone()]);
        let crate::ObjectKind::Text { style, .. } = &mut song.groups[0].slides[1].objects[0].kind else {
            panic!("a text object")
        };
        style.font_family = Some("Georgia".to_owned());
        assert_eq!(
            fonts_used(&song, &theme),
            vec!["Georgia".to_owned(), theme.templates[0].boxes[0].font_family.clone()]
        );
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
        let base = &theme.templates[0].boxes[1];
        assert_eq!(TextStyle::default().over(base), *base);
        assert!(TextStyle::default().is_default());
    }

    #[test]
    fn only_what_was_changed_is_kept_and_the_rest_still_follows_the_theme() {
        let mut theme = Theme::builtin();
        let base = theme.templates[0].boxes[0].clone();
        let mut wanted = base.clone();
        wanted.fill = Rgba::rgb(255, 0, 0);
        wanted.size = 20.0;
        let own = TextStyle::between(&base, &wanted);
        assert_eq!(own, TextStyle { fill: Some(Rgba::rgb(255, 0, 0)), size: Some(20.0), ..TextStyle::default() });
        assert_eq!(own.over(&base), BoxStyle { min_size: 20.0, ..wanted });
        // The theme changes its font: the object follows, and keeps its own colour and size.
        theme.templates[0].boxes[0].font_family = "Another".to_owned();
        let drawn = theme.text_style(Some(&Role::primary()), &own).unwrap();
        assert_eq!((drawn.font_family.as_str(), drawn.fill, drawn.size), ("Another", Rgba::rgb(255, 0, 0), 20.0));
    }

    #[test]
    fn an_object_without_a_known_role_uses_the_main_style() {
        let theme = Theme::builtin();
        let none = theme.text_style(None, &TextStyle::default()).unwrap();
        let unknown = theme.text_style(Some(&Role("notes".to_owned())), &TextStyle::default()).unwrap();
        assert_eq!(none, theme.templates[0].boxes[0]);
        assert_eq!(unknown, theme.templates[0].boxes[0]);
    }
}
