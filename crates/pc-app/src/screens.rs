//! The audience screens: what each one is called and where its picture goes. The list is kept
//! in `screens.json` in the app's folder.

use pc_model::Uuid;
use serde::{Deserialize, Serialize};

use crate::displays::{DisplayInfo, Placement, choose_output};

/// Where a screen's picture goes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Output {
    /// The first display the operator's window is not on, filled; a window if there is none.
    Automatic,
    /// A display, by the name the system gives it, filled.
    Display(String),
    /// An ordinary window.
    Window,
    /// The network, over NDI, under this name.
    Ndi(String),
    /// Nowhere. The screen is kept in the list for later.
    Off,
}

/// What a screen is for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    /// For the people in the room, the stream and the recording: the slides themselves.
    #[default]
    Audience,
    /// For the people on stage: the slide on screen, the next slide and the time.
    Stage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Screen {
    pub id: Uuid,
    pub name: String,
    pub output: Output,
    #[serde(default)]
    pub kind: Kind,
}

/// Where a screen ends up on this computer as it is now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Placed {
    /// Filling the display with this index.
    Fullscreen(usize),
    Window,
    Ndi(String),
    /// Not shown anywhere, and why, in plain words. Empty when the screen is simply off.
    Nothing(String),
}

impl Screen {
    pub fn new(name: String, output: Output) -> Screen {
        Screen { id: Uuid::new_v4(), name, output, kind: Kind::Audience }
    }

    pub fn stage(name: String, output: Output) -> Screen {
        Screen { kind: Kind::Stage, ..Screen::new(name, output) }
    }
}

/// The screens a fresh start has: one, placed automatically. `ndi` adds a second sent over
/// NDI under that name, for someone who had NDI switched on before screens could be listed.
pub fn starting(ndi: Option<&str>) -> Vec<Screen> {
    let mut screens = vec![Screen::new("Screen 1".to_owned(), Output::Automatic)];
    if let Some(name) = ndi {
        screens.push(Screen::new("Screen 2".to_owned(), Output::Ndi(name.to_owned())));
    }
    screens
}

/// A name no screen in the list has for a new screen of a kind: "Screen 3" when there are two
/// audience screens, "Stage Screen 1" for the first stage screen, and so on.
pub fn next_name(screens: &[Screen], kind: Kind) -> String {
    let base = match kind {
        Kind::Audience => "Screen",
        Kind::Stage => "Stage Screen",
    };
    let of_kind = screens.iter().filter(|screen| screen.kind == kind).count();
    (of_kind + 1..)
        .map(|n| format!("{base} {n}"))
        .find(|name| screens.iter().all(|screen| screen.name != *name))
        .unwrap_or_default()
}

/// Where each screen goes, in the list's order. `home` is the index of the display the
/// operator's window is on. A display is filled by one screen only and an NDI name is used
/// once: the first screen in the list that asks gets it. A display that is not connected is
/// never replaced by another, so a screen can never land on the operator's display by surprise.
pub fn place_all(screens: &[Screen], displays: &[DisplayInfo], home: usize) -> Vec<Placed> {
    let mut placed: Vec<Placed> = Vec::with_capacity(screens.len());
    for screen in screens {
        let wanted = match &screen.output {
            Output::Automatic => match choose_output(displays, home, None) {
                Ok(Placement::Fullscreen(index)) => Placed::Fullscreen(index),
                _ => Placed::Window,
            },
            Output::Display(name) => match displays.iter().find(|display| display.name == *name) {
                Some(display) => Placed::Fullscreen(display.index),
                None => Placed::Nothing(format!("Disconnected: \"{name}\" is not plugged in.")),
            },
            Output::Window => Placed::Window,
            Output::Ndi(name) if name.trim().is_empty() => Placed::Nothing("Give the NDI output a name.".to_owned()),
            Output::Ndi(name) => Placed::Ndi(name.trim().to_owned()),
            Output::Off => Placed::Nothing(String::new()),
        };
        let taken = placed
            .iter()
            .position(|earlier| matches!(earlier, Placed::Fullscreen(_) | Placed::Ndi(_)) && *earlier == wanted);
        placed.push(match (taken, &wanted) {
            (Some(by), Placed::Fullscreen(_)) => {
                Placed::Nothing(format!("\"{}\" is already on that display.", screens[by].name))
            }
            (Some(by), _) => Placed::Nothing(format!("\"{}\" already uses that NDI name.", screens[by].name)),
            (None, _) => wanted,
        });
    }
    placed
}

/// What a placed screen is doing, in words for the operator.
pub fn describe(placed: &Placed, displays: &[DisplayInfo]) -> String {
    match placed {
        Placed::Fullscreen(index) => match displays.iter().find(|display| display.index == *index) {
            Some(display) => format!("Filling \"{}\", {} x {}", display.name, display.w, display.h),
            None => "Filling a display".to_owned(),
        },
        Placed::Window => "In a window".to_owned(),
        Placed::Ndi(name) => format!("Starting NDI as \"{name}\""),
        Placed::Nothing(why) if why.is_empty() => "Off".to_owned(),
        Placed::Nothing(why) => why.clone(),
    }
}

const FORMAT: &str = "presentercraft-screens";

#[derive(Serialize, Deserialize)]
struct File {
    format: String,
    version: u32,
    screens: Vec<Screen>,
}

pub fn to_json(screens: &[Screen]) -> String {
    let file = File { format: FORMAT.to_owned(), version: 1, screens: screens.to_vec() };
    let mut text = serde_json::to_string_pretty(&file).unwrap_or_default();
    text.push('\n');
    text
}

/// Reads the list. A file that is not a screens file, or lists none, gives `None`.
pub fn from_json(text: &str) -> Option<Vec<Screen>> {
    let file: File = serde_json::from_str(text).ok()?;
    (file.format == FORMAT && file.version == 1 && !file.screens.is_empty()).then_some(file.screens)
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("screens.json"))
}

/// The saved list, if there is one.
pub fn load() -> Option<Vec<Screen>> {
    from_json(&std::fs::read_to_string(file()?).ok()?)
}

pub fn save(screens: &[Screen]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, to_json(screens))
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the screens: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn displays(names: &[&str]) -> Vec<DisplayInfo> {
        names
            .iter()
            .enumerate()
            .map(|(index, name)| DisplayInfo { index, name: (*name).to_owned(), x: 0, y: 0, w: 1920, h: 1080 })
            .collect()
    }

    fn screen(name: &str, output: Output) -> Screen {
        Screen::new(name.to_owned(), output)
    }

    #[test]
    fn automatic_takes_the_other_display_or_a_window() {
        let one = [screen("A", Output::Automatic)];
        assert_eq!(place_all(&one, &displays(&["Built-in"]), 0), [Placed::Window]);
        assert_eq!(place_all(&one, &displays(&["Built-in", "Projector"]), 0), [Placed::Fullscreen(1)]);
        assert_eq!(place_all(&one, &displays(&["Built-in", "Projector"]), 1), [Placed::Fullscreen(0)]);
    }

    #[test]
    fn a_display_that_is_not_plugged_in_is_never_swapped_for_another() {
        let screens = [screen("A", Output::Display("Projector".to_owned()))];
        let placed = place_all(&screens, &displays(&["Built-in"]), 0);
        assert!(matches!(&placed[0], Placed::Nothing(why) if why.contains("Disconnected")), "{placed:?}");
        // Plugged in again, it is found by name wherever it now sits in the list.
        assert_eq!(place_all(&screens, &displays(&["Projector", "Built-in"]), 1), [Placed::Fullscreen(0)]);
    }

    #[test]
    fn one_display_and_one_ndi_name_serve_one_screen_each() {
        let screens = [
            screen("A", Output::Automatic),
            screen("B", Output::Display("Projector".to_owned())),
            screen("C", Output::Ndi("Main".to_owned())),
            screen("D", Output::Ndi(" Main ".to_owned())),
            screen("E", Output::Window),
            screen("F", Output::Window),
            screen("G", Output::Off),
            screen("H", Output::Ndi("  ".to_owned())),
        ];
        let placed = place_all(&screens, &displays(&["Built-in", "Projector"]), 0);
        assert_eq!(placed[0], Placed::Fullscreen(1));
        assert!(matches!(&placed[1], Placed::Nothing(why) if why.contains("\"A\"")), "{placed:?}");
        assert_eq!(placed[2], Placed::Ndi("Main".to_owned()));
        assert!(matches!(&placed[3], Placed::Nothing(why) if why.contains("\"C\"")));
        // Any number of windows is fine.
        assert_eq!((&placed[4], &placed[5]), (&Placed::Window, &Placed::Window));
        assert_eq!(placed[6], Placed::Nothing(String::new()));
        assert!(matches!(&placed[7], Placed::Nothing(why) if why.contains("name")));
        assert_eq!(describe(&placed[6], &[]), "Off");
    }

    #[test]
    fn the_list_survives_its_file_and_other_files_are_refused() {
        let screens = starting(Some("PresenterCraft Audience"));
        assert_eq!(screens.len(), 2);
        assert_eq!(from_json(&to_json(&screens)), Some(screens.clone()));
        assert_eq!(from_json("{}"), None);
        assert_eq!(from_json(&to_json(&[])), None);
        assert_eq!(next_name(&screens, Kind::Audience), "Screen 3");
        assert_eq!(next_name(&screens, Kind::Stage), "Stage Screen 1");
        // A file from before stage screens reads, its screens being audience screens.
        let old = r#"{"format":"presentercraft-screens","version":1,"screens":[{"id":"6f0e1c52-8d1e-4d39-9f38-1a2b3c4d5e01","name":"A","output":"window"}]}"#;
        assert_eq!(from_json(old).map(|list| list[0].kind), Some(Kind::Audience));
        let mixed = vec![Screen::new("A".to_owned(), Output::Window), Screen::stage("B".to_owned(), Output::Off)];
        assert_eq!(from_json(&to_json(&mixed)), Some(mixed));
        assert_eq!(starting(None).len(), 1);
    }
}
