//! Where themes are kept on this computer: one file for each in the Themes folder, named by
//! the theme's id, and a note of which one is in use.

use std::path::{Path, PathBuf};

use sp_model::{Theme, Uuid};

fn folder() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("Themes"))
}

fn in_use_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("themes-in-use.txt"))
}

/// Every theme, the places in the list of the ones songs and Bible slides use, and which
/// layout of its theme Bible slides use. The built-in theme is always
/// there, as it was last saved if it has been changed; the others follow in name order.
pub fn load_all() -> (Vec<Theme>, usize, usize, usize) {
    let mut themes = folder().map_or_else(|| vec![Theme::builtin()], |folder| load_from(&folder));
    themes[1..].sort_by_key(|theme| theme.name.to_lowercase());
    // The note has a line for each kind of slide: `songs=<id>` and `bible=<id>`.
    let note = in_use_file().and_then(|file| std::fs::read_to_string(file).ok()).unwrap_or_default();
    let place = |kind: &str| {
        note.lines()
            .find_map(|line| line.strip_prefix(kind)?.strip_prefix('='))
            .and_then(|id| Uuid::parse_str(id.trim()).ok())
            .and_then(|id| themes.iter().position(|theme| theme.id == id))
            .unwrap_or(0)
    };
    let (songs, bible) = (place("songs"), place("bible"));
    // The Bible theme's template is noted by its id. A note from before templates had ids
    // says `bible_layout=alone` for the second of the two there were then.
    let templates = &themes[bible].templates;
    let by_id = note
        .lines()
        .find_map(|line| line.strip_prefix("bible_template="))
        .and_then(|id| Uuid::parse_str(id.trim()).ok())
        .and_then(|id| templates.iter().position(|template| template.id == id));
    let alone = note.lines().any(|line| line.trim() == "bible_layout=alone");
    let layout = by_id.unwrap_or(usize::from(alone)).min(templates.len().saturating_sub(1));
    (themes, songs, bible, layout)
}

fn load_from(folder: &Path) -> Vec<Theme> {
    let mut themes = vec![Theme::builtin()];
    let files = std::fs::read_dir(folder).ok();
    for path in files.into_iter().flatten().flatten().map(|entry| entry.path()) {
        // Themes saved before the project was renamed end in .pctheme; they still open.
        if path.extension().is_none_or(|ext| ext != "sptheme" && ext != "pctheme") {
            continue;
        }
        let read = std::fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|text| Theme::from_json(&text));
        match read {
            // The built-in theme's own file takes its place.
            Ok(theme) if theme.id.is_nil() => themes[0] = theme,
            Ok(theme) if themes.iter().all(|have| have.id != theme.id) => themes.push(theme),
            Ok(_) => {}
            Err(problem) => eprintln!("{}: {problem}", path.display()),
        }
    }
    themes
}

/// Writes a theme to its file. The error is a plain sentence.
pub fn save(theme: &Theme) -> Result<(), String> {
    let folder = folder().ok_or_else(|| "Could not find where to keep themes on this computer.".to_owned())?;
    save_in(&folder, theme)
}

fn save_in(folder: &Path, theme: &Theme) -> Result<(), String> {
    std::fs::create_dir_all(folder).map_err(|e| format!("Could not make the Themes folder: {e}"))?;
    let path = folder.join(format!("{}.sptheme", theme.id));
    // Write beside it and rename, so a half-written file is never left under the real name.
    let partial = path.with_extension("sptheme.part");
    std::fs::write(&partial, theme.to_json())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the theme: {e}"))?;
    // The same theme saved before the rename is replaced by the new file, not left beside it.
    let _ = std::fs::remove_file(folder.join(format!("{}.pctheme", theme.id)));
    Ok(())
}

/// Removes a theme's file. A theme that was never saved has none, which is not a failure.
pub fn delete(theme: &Theme) -> Result<(), String> {
    let Some(folder) = folder() else { return Ok(()) };
    // Its file, whichever ending it was saved with.
    for ending in ["sptheme", "pctheme"] {
        match std::fs::remove_file(folder.join(format!("{}.{ending}", theme.id))) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Could not delete the theme: {e}")),
        }
    }
    Ok(())
}

/// Remembers which themes songs and Bible slides use, for the next start.
pub fn remember_in_use(songs: &Theme, bible: &Theme, bible_template: usize) {
    if let Some(file) = in_use_file() {
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let template = bible.templates.get(bible_template).map(|template| template.id).unwrap_or_default();
        let _ = std::fs::write(file, format!("songs={}\nbible={}\nbible_template={template}\n", songs.id, bible.id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_themes_come_back_and_a_saved_built_in_takes_its_place() {
        let folder = std::env::temp_dir().join(format!("softpresenter-test-themes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        assert_eq!(load_from(&folder), vec![Theme::builtin()]);

        let mut sunday = Theme::builtin();
        sunday.id = Uuid::new_v4();
        sunday.name = "Sunday".to_owned();
        let mut built_in = Theme::builtin();
        built_in.templates[1].boxes[0].rect.y = 10.0;
        save_in(&folder, &sunday).unwrap();
        save_in(&folder, &built_in).unwrap();
        std::fs::write(folder.join("broken.pctheme"), "not a theme").unwrap();

        let loaded = load_from(&folder);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0], built_in);
        assert_eq!(loaded[1], sunday);
        let _ = std::fs::remove_dir_all(&folder);
    }
}
