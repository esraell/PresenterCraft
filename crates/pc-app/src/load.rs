//! Loading songs from text files.

use std::path::{Path, PathBuf};

use pc_model::{Presentation, parse_song_text, to_song_text};

/// The song shown when the program is started without any files.
pub const SAMPLE_SONG: &str = include_str!("../../../samples/sample-song.txt");

/// A song and the file it came from, so edits can be saved back.
pub struct Loaded {
    pub song: Presentation,
    pub path: PathBuf,
}

/// Saves a song over its file. The new text is written beside the file first and then moved
/// into place, so a crash cannot leave half a song.
pub fn save_song(path: &Path, song: &Presentation) -> Result<(), String> {
    let name = path.file_name().map_or_else(|| "song".into(), |n| n.to_string_lossy().into_owned());
    let temporary = path.with_file_name(format!(".{name}.saving"));
    std::fs::write(&temporary, to_song_text(song))
        .and_then(|()| std::fs::rename(&temporary, path))
        .map_err(|e| format!("{}: could not be saved ({e}).", path.display()))
}

/// Loads every path given: a file is one song, a folder is every `.txt` file directly inside
/// it in name order. Returns the songs in order and one plain message for each path that
/// could not be used.
pub fn load_songs(paths: &[PathBuf]) -> (Vec<Loaded>, Vec<String>) {
    let mut songs = Vec::new();
    let mut problems = Vec::new();
    for path in paths {
        if path.is_dir() {
            match text_files_in(path) {
                Ok(files) if files.is_empty() => {
                    problems.push(format!("{}: no .txt files in this folder", path.display()))
                }
                Ok(files) => files.iter().for_each(|f| load_one(f, &mut songs, &mut problems)),
                Err(reason) => problems.push(format!("{}: {reason}", path.display())),
            }
        } else {
            load_one(path, &mut songs, &mut problems);
        }
    }
    (songs, problems)
}

fn text_files_in(folder: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(folder)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("txt")))
        .collect();
    files.sort();
    Ok(files)
}

fn load_one(path: &Path, songs: &mut Vec<Loaded>, problems: &mut Vec<String>) {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) => return problems.push(format!("{}: {e}", path.display())),
    };
    let Ok(text) = String::from_utf8(bytes) else {
        return problems.push(format!("{}: not a UTF-8 text file", path.display()));
    };
    let title = path.file_stem().map_or_else(|| "Untitled".to_owned(), |s| s.to_string_lossy().into_owned());
    // A byte order mark some editors add would otherwise become part of the first line.
    let song = parse_song_text(&title, text.trim_start_matches('\u{feff}'));
    if song.slide_count() == 0 {
        return problems.push(format!("{}: no slides found", path.display()));
    }
    songs.push(Loaded { song, path: path.to_owned() });
}

#[cfg(test)]
mod tests {
    use pc_model::Role;

    use super::*;

    /// A fresh folder under the system temporary folder, removed when dropped.
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(name: &str) -> TempDir {
            let dir = std::env::temp_dir().join(format!("presentercraft-test-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, bytes).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn loads_a_file_with_title_from_name() {
        let dir = TempDir::new("file");
        let path = dir.write("Morning Song.txt", "[V]\nA\n--\nБ\n".as_bytes());
        let (songs, problems) = load_songs(&[path]);
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(songs[0].song.title, "Morning Song");
        assert_eq!(songs[0].song.slide_count(), 1);
    }

    #[test]
    fn loads_a_folder_sorted_and_only_txt() {
        let dir = TempDir::new("folder");
        dir.write("b.txt", b"B\n");
        dir.write("a.TXT", b"A\n");
        dir.write("notes.md", b"not a song\n");
        let (songs, problems) = load_songs(std::slice::from_ref(&dir.0));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(songs.iter().map(|s| s.song.title.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
    }

    #[test]
    fn missing_file_is_reported_and_skipped() {
        let dir = TempDir::new("missing");
        let good = dir.write("good.txt", b"A\n");
        let (songs, problems) = load_songs(&[dir.0.join("nope.txt"), good]);
        assert_eq!(songs.len(), 1);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("nope.txt"));
    }

    #[test]
    fn invalid_utf8_is_reported_and_skipped() {
        let dir = TempDir::new("utf8");
        let path = dir.write("bad.txt", &[0xff, 0xfe, 0x00]);
        let (songs, problems) = load_songs(&[path]);
        assert!(songs.is_empty());
        assert!(problems[0].contains("not a UTF-8 text file"), "{problems:?}");
    }

    #[test]
    fn file_without_slides_is_reported() {
        let dir = TempDir::new("empty");
        let path = dir.write("empty.txt", b"\n\n[Verse]\n\n");
        let (songs, problems) = load_songs(&[path]);
        assert!(songs.is_empty());
        assert!(problems[0].contains("no slides found"));
    }

    #[test]
    fn empty_folder_is_reported() {
        let dir = TempDir::new("emptyfolder");
        let (songs, problems) = load_songs(std::slice::from_ref(&dir.0));
        assert!(songs.is_empty());
        assert!(problems[0].contains("no .txt files"));
    }

    #[test]
    fn byte_order_mark_is_not_part_of_the_text() {
        let dir = TempDir::new("bom");
        let path = dir.write("bom.txt", "\u{feff}[Verse]\nA\n".as_bytes());
        let (songs, _) = load_songs(&[path]);
        assert_eq!(songs[0].song.groups[0].name, "Verse");
    }

    #[test]
    fn a_saved_song_reads_back_the_same_and_leaves_no_temporary_file() {
        let dir = TempDir::new("save");
        let path = dir.write("song.txt", "[Verse]\nOld\n".as_bytes());
        let (mut songs, _) = load_songs(std::slice::from_ref(&path));
        let edited = songs.remove(0).song.with_slide_text(0, "New words", "Новые слова").unwrap();
        save_song(&path, &edited).unwrap();
        let (again, problems) = load_songs(std::slice::from_ref(&path));
        assert!(problems.is_empty());
        assert_eq!(again[0].song.slide(0).unwrap().1.boxes, edited.slide(0).unwrap().1.boxes);
        let left: Vec<_> = std::fs::read_dir(&dir.0).unwrap().filter_map(Result::ok).map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "only the song should remain: {left:?}");
    }

    #[test]
    fn saving_into_a_missing_folder_is_reported() {
        let dir = TempDir::new("nosave");
        let song = parse_song_text("x", "A\n");
        assert!(save_song(&dir.0.join("gone").join("song.txt"), &song).unwrap_err().contains("could not be saved"));
    }

    #[test]
    fn sample_song_parses_with_both_languages() {
        let song = parse_song_text("Sample song", SAMPLE_SONG);
        assert!(song.slide_count() >= 6);
        assert!(song.slides().any(|(_, s)| s.text_for(&Role::translation()).is_some()));
        assert!(song.slides().any(|(_, s)| s.boxes.len() == 1));
    }
}
