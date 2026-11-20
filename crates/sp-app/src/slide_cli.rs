//! Changing a song's slides from the command line, for a quick fix without the editor: read a
//! slide's objects and how they look, set an object's words, its look (any of the inspector's
//! text settings) or its place, and read or replace the whole song as song text. Each change
//! is saved and can be undone, as one made in the window can, and a slide on screen shows it at
//! once.

use std::sync::Arc;

use sp_model::{Object, ObjectKind, Presentation, Rgba, Slide, SlideRect, TextStyle, Theme};

use crate::control::Reply;

impl crate::App {
    /// `slide SONG N`, `slide text …`, `slide style …`, `slide frame …`.
    pub(crate) fn ctl_slide(&mut self, args: &[String]) -> Result<Reply, String> {
        match args.get(1).map(|w| w.to_lowercase()).as_deref() {
            Some("text") => self.ctl_slide_text(&args[2..]),
            Some("style") => self.ctl_slide_style(&args[2..]),
            Some("frame") => self.ctl_slide_frame(&args[2..]),
            Some(_) => self.ctl_slide_show(&args[1..]),
            None => Err(SLIDE_USAGE.to_owned()),
        }
    }

    /// `slide SONG N [--json]`: the slide's objects, and how each looks.
    fn ctl_slide_show(&mut self, args: &[String]) -> Result<Reply, String> {
        let json = args.iter().any(|w| w == "--json");
        let words: Vec<&String> = args.iter().filter(|w| *w != "--json").collect();
        let [song, number] = words.as_slice() else { return Err(SLIDE_USAGE.to_owned()) };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let index = slide_number(number, presentation.slide_count())?;
        let (group, slide) = presentation.slide(index).ok_or("No such slide.")?;
        let theme = self.theme_of(at).clone();
        let mut lines = vec![format!(
            "{}, slide {} of {}{}",
            presentation.title,
            index + 1,
            presentation.slide_count(),
            if group.name.is_empty() { String::new() } else { format!(" ({})", group.name) }
        )];
        let mut objects = Vec::new();
        for (number, object) in slide.objects.iter().enumerate() {
            let (text, data) = describe(slide, number, object, &theme);
            lines.push(text);
            objects.push(data);
        }
        let mut data = serde_json::json!({"song": presentation.title, "slide": index + 1, "group": group.name, "objects": objects});
        if json {
            data["raw"] = serde_json::to_value(slide).unwrap_or_default();
        }
        Ok(Reply::with(lines.join("\n"), data))
    }

    /// `slide text SONG N [OBJECT] WORDS`: one object's words. `\n` in WORDS is a line break.
    fn ctl_slide_text(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "slide text SONG N [OBJECT] WORDS   (\\n in WORDS starts a new line)";
        let (song, number, object, words) = match args {
            [song, number, words] => (song, number, None, words),
            [song, number, object, words] => (song, number, Some(object.as_str()), words),
            _ => return Err(usage.to_owned()),
        };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let index = slide_number(number, presentation.slide_count())?;
        let (_, slide) = presentation.slide(index).ok_or("No such slide.")?;
        let which = find_object(slide, object)?;
        let words = words.replace("\\n", "\n");
        let mut changed = slide.clone();
        match &mut changed.objects[which].kind {
            ObjectKind::Text { text, .. } => *text = words.clone(),
            _ => return Err(format!("Object {} has no words.", which + 1)),
        }
        let name = object_name(&changed.objects[which]);
        let song = presentation.with_slide(index, changed).ok_or("No such slide.")?;
        self.change_song(at, song);
        Ok(Reply::done(format!(
            "{}, slide {}: {name} now says \"{}\".",
            presentation.title,
            index + 1,
            words.replace('\n', " / ")
        )))
    }

    /// `slide style SONG N|all [OBJECT] key=value … | reset`.
    fn ctl_slide_style(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "slide style SONG N|all [OBJECT] key=value …   (or reset; settings: slide style keys)";
        if args.first().is_some_and(|w| w.eq_ignore_ascii_case("keys")) {
            return Ok(Reply::done(STYLE_KEYS.trim_end()));
        }
        let [song, which, rest @ ..] = args else { return Err(usage.to_owned()) };
        if rest.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let slides: Vec<usize> = if which.eq_ignore_ascii_case("all") {
            (0..presentation.slide_count()).collect()
        } else {
            vec![slide_number(which, presentation.slide_count())?]
        };
        // An object named first, unless the first word is a setting or reset.
        let (object, settings) = match rest {
            [first, more @ ..] if !first.contains('=') && !first.eq_ignore_ascii_case("reset") => {
                (Some(first.as_str()), more)
            }
            _ => (None, rest),
        };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let mut song = presentation.as_ref().clone();
        let mut said = Vec::new();
        for index in &slides {
            let (_, slide) = song.slide(*index).ok_or("No such slide.")?;
            let mut changed = slide.clone();
            let which = match find_object(&changed, object) {
                Ok(which) => which,
                // With all the slides, one without that object is passed over.
                Err(_) if slides.len() > 1 => continue,
                Err(problem) => return Err(problem),
            };
            let ObjectKind::Text { style, .. } = &mut changed.objects[which].kind else {
                return Err(format!("Object {} has no words to style.", which + 1));
            };
            said = apply_settings(style, settings)?;
            song = song.with_slide(*index, changed).ok_or("No such slide.")?;
        }
        if said.is_empty() {
            return Err("No slide has that object.".to_owned());
        }
        let place = if slides.len() > 1 { "every slide".to_owned() } else { format!("slide {}", slides[0] + 1) };
        self.change_song(at, song);
        Ok(Reply::done(format!("{}, {place}: {}.", presentation.title, said.join(", "))))
    }

    /// `slide frame SONG N OBJECT x=… y=… w=… h=…`: where an object sits, in the slide's pixels.
    fn ctl_slide_frame(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "slide frame SONG N OBJECT x=… y=… w=… h=…   (or reset, back to the theme's place)";
        let [song, number, object, settings @ ..] = args else { return Err(usage.to_owned()) };
        if settings.is_empty() {
            return Err(usage.to_owned());
        }
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let index = slide_number(number, presentation.slide_count())?;
        let (_, slide) = presentation.slide(index).ok_or("No such slide.")?;
        let which = find_object(slide, Some(object))?;
        let theme = self.theme_of(at).clone();
        let mut changed = slide.clone();
        let placed = theme.place_on(slide, slide.objects[which].role.as_ref());
        let target = &mut changed.objects[which];
        if settings.iter().any(|w| w.eq_ignore_ascii_case("reset")) {
            target.frame = None;
        } else {
            target.frame = Some(set_frame(target.frame.unwrap_or(placed), settings)?);
        }
        let name = object_name(target);
        let frame = target.frame.unwrap_or(placed);
        let song = presentation.with_slide(index, changed).ok_or("No such slide.")?;
        self.change_song(at, song);
        Ok(Reply::done(format!(
            "{}, slide {}: {name} at x={:.0} y={:.0} w={:.0} h={:.0}.",
            presentation.title,
            index + 1,
            frame.x,
            frame.y,
            frame.w,
            frame.h
        )))
    }

    /// `song text SONG [FILE]`: the song as song text, or its words replaced from a file. Its
    /// slides keep their looks, labels and notes where they still match, as Reflow keeps them.
    pub(crate) fn ctl_song_text(&mut self, args: &[String]) -> Result<Reply, String> {
        let usage = "song text SONG [FILE]";
        let (song, file) = match args {
            [song] => (song, None),
            [song, file] => (song, Some(file)),
            _ => return Err(usage.to_owned()),
        };
        let at = self.find_song(song)?;
        let presentation = self.show.presentations().get(at).cloned().ok_or("That song is gone.")?;
        let Some(file) = file else {
            let text = sp_model::to_song_text(&presentation);
            return Ok(Reply::with(text.clone(), serde_json::json!({"song": presentation.title, "text": text})));
        };
        let bytes = std::fs::read(file).map_err(|e| format!("Could not read {file}: {e}"))?;
        let text = sp_model::decode_text(&bytes);
        let song = presentation.reflowed(&text);
        let count = song.slide_count();
        self.change_song(at, song);
        Ok(Reply::done(format!("{} now has the words from {file}: {count} slides.", presentation.title)))
    }

    /// Puts a song changed from the command line in its place, as a change in the window is
    /// put: it can be undone (from the Show view, or the Edit view's undo), it is saved, its
    /// pictures are drawn again, an editor open on it shows it, and the screens show it at once.
    fn change_song(&mut self, at: usize, song: Presentation) {
        let ids: Vec<sp_model::Uuid> = song.slides().map(|(_, slide)| slide.id).collect();
        self.remember_for_undo(at);
        self.note_song_step();
        self.show.replace_presentation(at, Arc::new(song));
        if !self.unsaved.contains(&at) {
            self.unsaved.push(at);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            for id in ids {
                running.thumbs.forget(id, &mut running.egui_renderer);
            }
            let open = running.home_state.editing.as_ref().map(|draft| draft.at).filter(|open| open.presentation == at);
            if let Some(open) = open {
                running.home_state.editing = crate::editor::EditDraft::of(&self.show, open);
            }
            running.home_window.request_redraw();
        }
        self.refresh_output();
    }
}

const SLIDE_USAGE: &str = "slide SONG N [--json], slide text SONG N [OBJECT] WORDS, slide style SONG N|all [OBJECT] key=value…, slide frame SONG N OBJECT x= y= w= h=";

/// The settings `slide style` takes.
pub const STYLE_KEYS: &str = "\
Settings for slide style (key=value; key=reset goes back to the theme's; reset alone, all of them):
  font=NAME            size=NUMBER          weight=NUMBER|light|regular|semibold|bold
  italic=on|off        underline=on|off     strike=on|off        shadow=on|off
  color=#RRGGBB[AA]    outline=NUMBER       outline_color=#RRGGBB[AA]
  align=left|center|right|justify           valign=top|middle|bottom
  caps=none|all_caps|small_caps|title_case|start_case
  scaling=none|text_down|text_up|text_up_or_down|container_up_or_down
  line_transform=none|remove_line_returns|line_returns_as_slashes
  line_height=NUMBER   letter_spacing=NUMBER   paragraph_spacing=NUMBER   baseline=NUMBER
";

/// A slide by its number (from 1).
fn slide_number(typed: &str, count: usize) -> Result<usize, String> {
    match typed.trim().parse::<usize>() {
        Ok(n) if n >= 1 && n <= count => Ok(n - 1),
        _ => Err(format!("Slide numbers go from 1 to {count}.")),
    }
}

/// What an object is called: its name, or its role, or its kind.
fn object_name(object: &Object) -> String {
    if !object.name.trim().is_empty() {
        return object.name.trim().to_owned();
    }
    match (&object.role, &object.kind) {
        (Some(role), _) => role.label(),
        (None, ObjectKind::Text { .. }) => "Text".to_owned(),
        (None, _) => "Object".to_owned(),
    }
}

/// An object of a slide by its number (from 1), its name or its role; with none named, the
/// main text (the box for the main language, or the first with words).
pub fn find_object(slide: &Slide, typed: Option<&str>) -> Result<usize, String> {
    let is_text = |object: &Object| matches!(object.kind, ObjectKind::Text { .. });
    let Some(typed) = typed.map(str::trim).filter(|t| !t.is_empty()) else {
        return slide
            .objects
            .iter()
            .position(|o| o.role == Some(sp_model::Role::primary()))
            .or_else(|| slide.objects.iter().position(is_text))
            .ok_or_else(|| "The slide has no words.".to_owned());
    };
    if let Ok(n) = typed.parse::<usize>() {
        return if n >= 1 && n <= slide.objects.len() {
            Ok(n - 1)
        } else {
            Err(format!("Object numbers on this slide go from 1 to {}.", slide.objects.len()))
        };
    }
    let wanted = typed.to_lowercase();
    let main_words = ["main", "primary"];
    slide
        .objects
        .iter()
        .position(|o| {
            o.name.trim().to_lowercase() == wanted
                || o.role.as_ref().is_some_and(|role| {
                    role.0.to_lowercase() == wanted
                        || role.label().to_lowercase() == wanted
                        || (main_words.contains(&wanted.as_str()) && *role == sp_model::Role::primary())
                })
        })
        .ok_or_else(|| {
            let names: Vec<String> =
                slide.objects.iter().enumerate().map(|(n, o)| format!("{} {}", n + 1, object_name(o))).collect();
            format!("This slide has no object called {typed}. It has: {}.", names.join(", "))
        })
}

/// A colour written #RGB, #RRGGBB or #RRGGBBAA.
pub fn parse_color(typed: &str) -> Option<Rgba> {
    let hex = typed.trim().trim_start_matches('#');
    let byte = |at: usize| u8::from_str_radix(hex.get(at..at + 2)?, 16).ok();
    match hex.len() {
        3 => {
            let digit = |at: usize| u8::from_str_radix(hex.get(at..=at)?, 16).ok().map(|d| d * 17);
            Some(Rgba { r: digit(0)?, g: digit(1)?, b: digit(2)?, a: 255 })
        }
        6 => Some(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: 255 }),
        8 => Some(Rgba { r: byte(0)?, g: byte(2)?, b: byte(4)?, a: byte(6)? }),
        _ => None,
    }
}

/// Changes a text object's own look by settings written key=value (see [`STYLE_KEYS`]), or
/// `reset` for the theme's look. Returns each change in words, or what is wrong.
pub fn apply_settings(style: &mut TextStyle, settings: &[String]) -> Result<Vec<String>, String> {
    let mut said = Vec::new();
    let mut fields = serde_json::to_value(&*style).map_err(|e| e.to_string())?;
    for setting in settings {
        if setting.eq_ignore_ascii_case("reset") {
            fields = serde_json::to_value(TextStyle::default()).map_err(|e| e.to_string())?;
            said.push("back to the theme's look".to_owned());
            continue;
        }
        let Some((key, value)) = setting.split_once('=') else {
            return Err(format!("{setting}: write settings as key=value (see slide style keys)."));
        };
        let (key, value) = (key.trim().to_lowercase(), value.trim());
        let field = match key.as_str() {
            "font" => "font_family",
            "color" | "colour" => "fill",
            "outline" => "outline_width",
            "outline_colour" => "outline_color",
            other => other,
        };
        let known = [
            "font_family",
            "size",
            "weight",
            "italic",
            "fill",
            "align",
            "outline_width",
            "outline_color",
            "shadow",
            "valign",
            "caps",
            "underline",
            "strike",
            "scaling",
            "line_transform",
            "line_height",
            "letter_spacing",
            "paragraph_spacing",
            "baseline",
        ];
        if !known.contains(&field) {
            return Err(format!("{key} isn't a setting slide style knows (see slide style keys)."));
        }
        let parsed = if value.eq_ignore_ascii_case("reset") {
            serde_json::Value::Null
        } else {
            match field {
                "italic" | "shadow" | "underline" | "strike" => match value.to_lowercase().as_str() {
                    "on" | "yes" | "true" => serde_json::json!(true),
                    "off" | "no" | "false" => serde_json::json!(false),
                    _ => return Err(format!("{key} is on or off, not {value}.")),
                },
                "fill" | "outline_color" => {
                    let color =
                        parse_color(value).ok_or_else(|| format!("{value} isn't a colour; write it #RRGGBB."))?;
                    serde_json::to_value(color).map_err(|e| e.to_string())?
                }
                "weight" => {
                    let weight = match value.to_lowercase().as_str() {
                        "light" => 300,
                        "regular" | "normal" => 400,
                        "medium" => 500,
                        "semibold" => 600,
                        "bold" => 700,
                        "heavy" | "black" => 900,
                        number => {
                            number.parse::<u16>().map_err(|_| format!("weight is a number or bold, not {value}."))?
                        }
                    };
                    serde_json::json!(weight)
                }
                "size" | "outline_width" | "line_height" | "letter_spacing" | "paragraph_spacing" | "baseline" => {
                    let number = value.parse::<f32>().map_err(|_| format!("{key} is a number, not {value}."))?;
                    serde_json::json!(number)
                }
                "font_family" => serde_json::json!(value),
                // A choice among names: written as the theme files write them.
                _ => serde_json::json!(value.to_lowercase().replace([' ', '-'], "_")),
            }
        };
        fields[field] = parsed;
        said.push(if value.eq_ignore_ascii_case("reset") {
            format!("{key} as the theme has it")
        } else {
            format!("{key} {value}")
        });
    }
    let changed: TextStyle = serde_json::from_value(fields).map_err(|e| {
        let text = e.to_string();
        format!("That isn't a value slide style takes: {}", text.split(" at line").next().unwrap_or(&text))
    })?;
    *style = changed;
    Ok(said)
}

/// A frame changed by settings written x=, y=, w= (or width=), h= (or height=).
pub fn set_frame(mut frame: SlideRect, settings: &[String]) -> Result<SlideRect, String> {
    for setting in settings {
        let Some((key, value)) = setting.split_once('=') else {
            return Err(format!("{setting}: write the place as x=… y=… w=… h=…."));
        };
        let number = value.trim().parse::<f32>().map_err(|_| format!("{key} is a number, not {value}."))?;
        match key.trim().to_lowercase().as_str() {
            "x" => frame.x = number,
            "y" => frame.y = number,
            "w" | "width" => frame.w = number.max(1.0),
            "h" | "height" => frame.h = number.max(1.0),
            other => return Err(format!("{other} isn't x, y, w or h.")),
        }
    }
    Ok(frame)
}

/// One object of a slide, in words and as data: its number, name, role, kind, words, place,
/// and how its text looks, each setting the slide's own or the theme's.
fn describe(slide: &Slide, number: usize, object: &Object, theme: &Theme) -> (String, serde_json::Value) {
    let place = object.frame.unwrap_or_else(|| theme.place_on(slide, object.role.as_ref()));
    let whose_place = if object.frame.is_some() { "its own" } else { "the theme's" };
    let role = object.role.as_ref().map(|role| role.0.clone()).unwrap_or_default();
    let mut text = format!("{:>2}. {}", number + 1, object_name(object));
    if !role.is_empty() && object_name(object) != object.role.as_ref().map(|r| r.label()).unwrap_or_default() {
        text.push_str(&format!(" (role {role})"));
    }
    let mut data = serde_json::json!({
        "object": number + 1,
        "name": object.name,
        "role": role,
        "frame": {"x": place.x, "y": place.y, "w": place.w, "h": place.h, "own": object.frame.is_some()},
    });
    match &object.kind {
        ObjectKind::Text { text: words, style } => {
            text.push_str(&format!(": \"{}\"", words.replace('\n', " / ")));
            text.push_str(&format!(
                "\n    at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("text");
            data["text"] = serde_json::json!(words);
            if let Some(look) = theme.text_style_on(slide, object.role.as_ref(), style) {
                let own = serde_json::to_value(style).unwrap_or_default();
                let mark = |field: &str| if own.get(field).is_some_and(|v| !v.is_null()) { "*" } else { "" };
                text.push_str(&format!(
                    "\n    font {}{}, size {:.0}{}, weight {}{}, italic {}{}, colour #{:02x}{:02x}{:02x}{}, align {:?}{}, scaling {:?}{}",
                    look.font_family,
                    mark("font_family"),
                    look.size,
                    mark("size"),
                    look.weight,
                    mark("weight"),
                    if look.italic { "on" } else { "off" },
                    mark("italic"),
                    look.fill.r,
                    look.fill.g,
                    look.fill.b,
                    mark("fill"),
                    look.align,
                    mark("align"),
                    look.scaling,
                    mark("scaling"),
                ));
                data["style"] = own;
                data["look"] = serde_json::to_value(&look).unwrap_or_default();
            }
        }
        _ => {
            text.push_str(&format!(
                " at x={:.0} y={:.0} w={:.0} h={:.0} ({whose_place})",
                place.x, place.y, place.w, place.h
            ));
            data["kind"] = serde_json::json!("other");
        }
    }
    (text, data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sp_model::Role;

    fn slide() -> Slide {
        let mut named = Object::text(Role::translation(), "Перевод");
        named.name = "Russian".to_owned();
        Slide::new(vec![Object::text(Role::primary(), "Main words"), named])
    }

    #[test]
    fn an_object_is_found_by_number_name_or_role_and_the_main_text_by_default() {
        let slide = slide();
        assert_eq!(find_object(&slide, None), Ok(0));
        assert_eq!(find_object(&slide, Some("2")), Ok(1));
        assert_eq!(find_object(&slide, Some("russian")), Ok(1));
        assert_eq!(find_object(&slide, Some("Translation")), Ok(1));
        assert_eq!(find_object(&slide, Some("main")), Ok(0));
        assert!(find_object(&slide, Some("3")).is_err());
        let missing = find_object(&slide, Some("logo")).unwrap_err();
        assert!(missing.contains("1 Main") && missing.contains("2 Russian"), "{missing}");
    }

    #[test]
    fn settings_change_the_look_and_reset_goes_back_to_the_theme() {
        let mut style = TextStyle::default();
        let said = apply_settings(
            &mut style,
            &[
                "size=90".into(),
                "weight=bold".into(),
                "italic=on".into(),
                "color=#ff8800".into(),
                "align=left".into(),
                "scaling=none".into(),
            ],
        )
        .unwrap();
        assert_eq!(said.len(), 6);
        assert_eq!(style.size, Some(90.0));
        assert_eq!(style.weight, Some(700));
        assert_eq!(style.italic, Some(true));
        assert_eq!(style.fill, Some(Rgba { r: 255, g: 136, b: 0, a: 255 }));
        assert_eq!(style.align, Some(sp_model::Align::Left));
        assert_eq!(style.scaling, Some(sp_model::Scaling::None));
        apply_settings(&mut style, &["size=reset".into()]).unwrap();
        assert_eq!(style.size, None);
        assert_eq!(style.weight, Some(700), "the rest kept");
        apply_settings(&mut style, &["reset".into()]).unwrap();
        assert_eq!(style, TextStyle::default());
        // What is wrong is said plainly, and nothing is changed.
        assert!(apply_settings(&mut style, &["sise=90".into()]).unwrap_err().contains("sise"));
        assert!(apply_settings(&mut style, &["align=sideways".into()]).is_err());
        assert!(apply_settings(&mut style, &["color=orange".into()]).unwrap_err().contains("colour"));
        assert_eq!(style, TextStyle::default());
    }

    #[test]
    fn colours_and_places_are_read_as_written() {
        assert_eq!(parse_color("#fff"), Some(Rgba { r: 255, g: 255, b: 255, a: 255 }));
        assert_eq!(parse_color("00000080"), Some(Rgba { r: 0, g: 0, b: 0, a: 128 }));
        assert_eq!(parse_color("#12345"), None);
        let frame = SlideRect { x: 0.0, y: 0.0, w: 100.0, h: 100.0 };
        let moved = set_frame(frame, &["x=10".into(), "h=50".into()]).unwrap();
        assert_eq!((moved.x, moved.y, moved.w, moved.h), (10.0, 0.0, 100.0, 50.0));
        assert!(set_frame(frame, &["z=1".into()]).is_err());
    }
}
