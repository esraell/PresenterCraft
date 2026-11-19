//! The project was called PresenterCraft before it was SoftPresenter. On the first start under
//! the new name, the folders the old name kept (the app's data, and on a Mac its log) are
//! renamed to the new name, so nothing is lost and nothing is copied twice.
//!
//! The rules: only when the new folder does not exist yet (never merged into one that does);
//! nothing is ever deleted; and paths written inside the moved files that point into the old
//! folder (playlists naming songs, the media bin's pictures) are pointed into the new one.
//! A test run with its own data folder (`SOFTPRESENTER_DATA`) is left alone.

use std::path::{Path, PathBuf};

/// The folders under the old name and the new, for the app's data and its log, on this
/// computer. Empty when a data folder of its own was named, or the home folder is unknown.
fn pairs() -> Vec<(PathBuf, PathBuf)> {
    if std::env::var_os("SOFTPRESENTER_DATA").is_some_and(|v| !v.is_empty()) {
        return Vec::new();
    }
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty()).map(PathBuf::from);
    let mut pairs = Vec::new();
    if cfg!(target_os = "macos") {
        if let Some(home) = var("HOME") {
            let support = home.join("Library/Application Support");
            pairs.push((support.join("PresenterCraft"), support.join("SoftPresenter")));
            let logs = home.join("Library/Logs");
            pairs.push((logs.join("PresenterCraft"), logs.join("SoftPresenter")));
        }
    } else if cfg!(target_os = "windows") {
        if let Some(appdata) = var("APPDATA") {
            pairs.push((appdata.join("PresenterCraft"), appdata.join("SoftPresenter")));
        }
    } else if let Some(share) = var("XDG_DATA_HOME").or_else(|| var("HOME").map(|home| home.join(".local/share"))) {
        pairs.push((share.join("presentercraft"), share.join("softpresenter")));
    }
    pairs
}

/// Renames the old folders to the new name where that is safe, and says what was done, for
/// the log once it is open.
pub fn move_old_folders() -> Vec<String> {
    pairs()
        .into_iter()
        .filter_map(|(old, new)| move_folder(&old, &new).transpose())
        .map(|done| done.unwrap_or_else(|e| e))
        .collect()
}

/// Renames `old` to `new` when `old` is a folder and `new` does not exist, then points the
/// paths inside the moved files at the new place. `None` when there was nothing to do.
fn move_folder(old: &Path, new: &Path) -> Result<Option<String>, String> {
    let is_folder = std::fs::symlink_metadata(old).is_ok_and(|meta| meta.is_dir());
    if !is_folder || std::fs::symlink_metadata(new).is_ok() {
        return Ok(None);
    }
    std::fs::rename(old, new).map_err(|e| format!("Could not rename {} to {}: {e}", old.display(), new.display()))?;
    let changed = repoint(new, &old.to_string_lossy(), &new.to_string_lossy());
    Ok(Some(format!(
        "moved {} to {} (paths in {changed} files pointed at the new place)",
        old.display(),
        new.display()
    )))
}

/// Points every path inside the folder's own files (settings, playlists, songs, themes) that
/// starts with `old` at `new`. Bibles, pictures and videos are not opened. Returns how many
/// files were changed. Each is written beside itself and moved into place.
fn repoint(folder: &Path, old: &str, new: &str) -> usize {
    let mut changed = 0;
    let mut stack = vec![(folder.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = std::fs::symlink_metadata(&path) else { continue };
            if meta.is_dir() {
                if depth < 3 {
                    stack.push((path, depth + 1));
                }
                continue;
            }
            let text_file = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| matches!(ext, "json" | "txt" | "pcpres" | "sppres" | "pctheme" | "sptheme"));
            if !meta.is_file() || !text_file || meta.len() > 64 * 1024 * 1024 {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let moved = point_at_new(&text, old, new);
            if moved != text {
                let partial = path.with_extension("renaming");
                if std::fs::write(&partial, &moved).and_then(|()| std::fs::rename(&partial, &path)).is_ok() {
                    changed += 1;
                } else {
                    let _ = std::fs::remove_file(&partial);
                }
            }
        }
    }
    changed
}

/// The text with every path that starts with the folder `old` (then a `/`, a `\`, or the end of
/// a quoted string or a line) pointed at `new`.
fn point_at_new(text: &str, old: &str, new: &str) -> String {
    let mut out = text.to_owned();
    for end in ["/", "\\\\", "\"", "\n"] {
        out = out.replace(&format!("{old}{end}"), &format!("{new}{end}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sp-rename-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_old_folder_is_renamed_and_the_paths_inside_follow_it() {
        let root = temp("move");
        let (old, new) = (root.join("PresenterCraft"), root.join("SoftPresenter"));
        std::fs::create_dir_all(old.join("Songs")).unwrap();
        let song_key = format!("{}/Songs/abc", old.display());
        std::fs::write(old.join("services.json"), format!(r#"{{"items":[{{"key":"{song_key}"}}]}}"#)).unwrap();
        std::fs::write(old.join("Songs/abc.pcpres"), "{}").unwrap();
        let outside = r#"{"path":"/Users/someone/Videos/intro.mp4"}"#;
        std::fs::write(old.join("media.json"), outside).unwrap();
        let done = move_folder(&old, &new).unwrap();
        assert!(done.is_some());
        assert!(!old.exists() && new.join("Songs/abc.pcpres").is_file(), "renamed, nothing copied");
        let services = std::fs::read_to_string(new.join("services.json")).unwrap();
        assert!(services.contains(&format!("{}/Songs/abc", new.display())), "{services}");
        assert_eq!(std::fs::read_to_string(new.join("media.json")).unwrap(), outside, "paths elsewhere untouched");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_new_folder_already_there_is_never_merged_into_and_nothing_is_deleted() {
        let root = temp("both");
        let (old, new) = (root.join("PresenterCraft"), root.join("SoftPresenter"));
        std::fs::create_dir_all(&old).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        std::fs::write(old.join("a.json"), "old").unwrap();
        std::fs::write(new.join("b.json"), "new").unwrap();
        assert_eq!(move_folder(&old, &new).unwrap(), None);
        assert!(old.join("a.json").is_file() && new.join("b.json").is_file() && !new.join("a.json").exists());
        // No old folder: nothing to do.
        assert_eq!(move_folder(&root.join("missing"), &root.join("other")).unwrap(), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn only_whole_folder_paths_are_pointed_elsewhere() {
        let text = r#"{"a":"/x/PresenterCraft/Songs/s","b":"/x/PresenterCraftOther/t","c":"/x/PresenterCraft"}"#;
        let moved = point_at_new(text, "/x/PresenterCraft", "/x/SoftPresenter");
        assert_eq!(moved, r#"{"a":"/x/SoftPresenter/Songs/s","b":"/x/PresenterCraftOther/t","c":"/x/SoftPresenter"}"#);
    }
}
