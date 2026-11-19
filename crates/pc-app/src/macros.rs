//! Macros: named, coloured buttons that do several things at once when clicked. Kept in
//! `macros.json`.

use pc_model::{Rgba, ShowAction, Uuid};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Macro {
    pub id: Uuid,
    pub name: String,
    pub color: Rgba,
    pub actions: Vec<ShowAction>,
    /// Run once when PresenterCraft starts.
    #[serde(default)]
    pub on_startup: bool,
}

/// The colours a macro can be given.
pub const COLORS: [(&str, Rgba); 8] = [
    ("Blue", Rgba::rgb(66, 133, 244)),
    ("Green", Rgba::rgb(52, 168, 110)),
    ("Orange", Rgba::rgb(244, 160, 0)),
    ("Purple", Rgba::rgb(155, 89, 182)),
    ("Teal", Rgba::rgb(0, 172, 193)),
    ("Pink", Rgba::rgb(204, 78, 180)),
    ("Grey", Rgba::rgb(120, 144, 156)),
    ("Brown", Rgba::rgb(141, 110, 99)),
];

impl Macro {
    pub fn new(name: String) -> Macro {
        Macro { id: Uuid::new_v4(), name, color: COLORS[0].1, actions: Vec::new(), on_startup: false }
    }

    pub fn duplicate(&self) -> Macro {
        Macro { id: Uuid::new_v4(), name: format!("{} copy", self.name), ..self.clone() }
    }
}

/// What an action does, in words, given the names of what it names.
pub fn describe(
    action: &ShowAction,
    message_name: impl Fn(Uuid) -> Option<String>,
    prop_name: impl Fn(Uuid) -> Option<String>,
    timer_name: impl Fn(Uuid) -> Option<String>,
    song_name: impl Fn(&str) -> Option<String>,
) -> String {
    let gone = || "(gone)".to_owned();
    match action {
        ShowAction::Clear { what } => what.name().to_owned(),
        ShowAction::Message { id, show } => {
            format!("{} Message: {}", if *show { "Show" } else { "Hide" }, message_name(*id).unwrap_or_else(gone))
        }
        ShowAction::Prop { id, show } => {
            format!("{} Prop: {}", if *show { "Show" } else { "Hide" }, prop_name(*id).unwrap_or_else(gone))
        }
        ShowAction::Timer { id, act } => format!("{} Timer: {}", act.name(), timer_name(*id).unwrap_or_else(gone)),
        ShowAction::Audience { on } => format!("Audience Screens {}", if *on { "On" } else { "Off" }),
        ShowAction::Song { key } => format!("Go to Song: {}", song_name(key).unwrap_or_else(gone)),
    }
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("macros.json"))
}

pub fn load() -> Vec<Macro> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(macros: &[Macro]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(macros).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the macros: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pc_model::{ClearWhat, TimerDo};

    #[test]
    fn macros_describe_their_actions_and_survive_their_file() {
        let timer = Uuid::new_v4();
        let mut start = Macro::new("Start Service".to_owned());
        start.actions = vec![
            ShowAction::Clear { what: ClearWhat::All },
            ShowAction::Timer { id: timer, act: TimerDo::Start },
            ShowAction::Audience { on: true },
        ];
        let named = |id: Uuid| (id == timer).then(|| "Countdown".to_owned());
        let words: Vec<String> =
            start.actions.iter().map(|action| describe(action, |_| None, |_| None, named, |_| None)).collect();
        assert_eq!(words, ["Clear All", "Start Timer: Countdown", "Audience Screens On"]);
        let copy = start.duplicate();
        assert_ne!(copy.id, start.id);
        let saved = vec![start, copy];
        assert_eq!(serde_json::from_str::<Vec<Macro>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
