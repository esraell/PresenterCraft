//! Libraries: named collections of songs, such as Christmas or Youth, beside All Songs. A song
//! can be in any number of them; a library only lists songs, by their keys. Kept in
//! `libraries.json`.

use pc_model::Uuid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Library {
    pub id: Uuid,
    pub name: String,
    /// The songs in it, by the keys the app knows them by.
    pub songs: Vec<String>,
}

impl Library {
    pub fn new(name: String) -> Library {
        Library { id: Uuid::new_v4(), name, songs: Vec::new() }
    }

    pub fn has(&self, key: &str) -> bool {
        self.songs.iter().any(|have| have == key)
    }

    /// Puts a song in, or takes it out if it is in.
    pub fn toggle(&mut self, key: &str) {
        if self.has(key) {
            self.songs.retain(|have| have != key);
        } else {
            self.songs.push(key.to_owned());
        }
    }
}

/// A name no library has: "New Library", "New Library 2", and so on.
pub fn new_name(libraries: &[Library]) -> String {
    let taken = |name: &str| libraries.iter().any(|library| library.name == name);
    std::iter::once("New Library".to_owned())
        .chain((2..).map(|n| format!("New Library {n}")))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

fn file() -> Option<std::path::PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("libraries.json"))
}

pub fn load() -> Vec<Library> {
    file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(libraries: &[Library]) -> Result<(), String> {
    let path = file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(libraries).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the libraries: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn songs_go_in_and_out_of_libraries_and_names_do_not_repeat() {
        let mut christmas = Library::new(new_name(&[]));
        assert_eq!(christmas.name, "New Library");
        christmas.toggle("songs/silent-night");
        christmas.toggle("songs/joy");
        assert!(christmas.has("songs/joy"));
        christmas.toggle("songs/joy");
        assert_eq!(christmas.songs, ["songs/silent-night"]);
        assert_eq!(new_name(std::slice::from_ref(&christmas)), "New Library 2");
        let saved = vec![christmas];
        assert_eq!(serde_json::from_str::<Vec<Library>>(&serde_json::to_string(&saved).unwrap()).unwrap(), saved);
    }
}
