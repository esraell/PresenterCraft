//! Song packs (`.sppack`): chosen songs, or a playlist with its songs, and what they need, in
//! one file that can go by AirDrop, email or a USB stick to another computer.
//!
//! A pack is a zip file:
//!
//! ```text
//! manifest.json        "softpresenter-pack", version 1, the app version, what is inside
//! songs/<id>.sppres    each song, as saved
//! themes/<id>.sptheme  only the themes a song sets for itself
//! images/<file>        each picture a song's slides (or those themes) use
//! media/<file>         each video a song's slides use (left out on request)
//! playlist.json        when a playlist was packed: its name and items, songs by id
//! ```
//!
//! No path from this computer and no user name goes into a pack: a picture or video is named
//! by a name made for the pack, and the songs point at that name.
//!
//! A pack from someone else is untrusted input. Opening one reads only entries of the kinds
//! above, each with a plain file name (no folders, no `..`, no links), within a size cap, checked
//! against the free disk space; every song and theme goes through the normal, version-checked
//! readers; anything else is left out and named. The app names every file it writes itself, so a
//! name in a pack is only ever a label to look things up by. Nothing in a pack is run.

use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sp_model::{ObjectKind, Presentation, ShowAction, Theme, Uuid};

use crate::service::Item;

/// A song pack's file ending.
pub const ENDING: &str = "sppack";
const FORMAT: &str = "softpresenter-pack";
const VERSION: u64 = 1;
/// How a song's slide names a video copied into the library: inside its Media folder.
pub const MEDIA_PREFIX: &str = "Media/";
/// The most entries a pack may have, and the largest song, theme, manifest or playlist, and
/// picture, it may hold.
const MOST_ENTRIES: usize = 20_000;
const LARGEST_TEXT: u64 = 5 * 1024 * 1024;
const LARGEST_PICTURE: u64 = 100 * 1024 * 1024;
/// The most the songs, themes, manifest and playlist of one pack may come to, read.
const MOST_WORDS: u64 = 256 * 1024 * 1024;
/// Room left free on the disk after a pack is opened.
const KEEP_FREE: u64 = 200 * 1024 * 1024;

pub fn is_pack(path: &Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case(ENDING))
}

/// A playlist as a pack holds it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlaylistFile {
    pub name: String,
    pub items: Vec<Item>,
}

/// A pack ready to write: copies of the songs and themes, pointing at the pictures and videos by
/// the names the pack gives them.
pub struct Export {
    pub songs: Vec<Presentation>,
    pub themes: Vec<Theme>,
    /// Each picture and video: its name in the pack, and its file on this computer.
    pub pictures: Vec<(String, PathBuf)>,
    pub videos: Vec<(String, PathBuf)>,
    /// Videos the songs use, left out when they aren't wanted.
    pub videos_left_out: Vec<(String, PathBuf)>,
    pub playlist: Option<PlaylistFile>,
    /// Pictures and videos the songs use that aren't on this computer.
    pub missing: usize,
}

impl Export {
    /// Plans a pack of `songs` and the themes among `themes` they set for themselves, with the
    /// playlist given, if any. `id_for` turns a song's key (a "go to song" action's, or a
    /// playlist item's) into its id, if it is a song it knows; a key it doesn't know is written as
    /// missing, never as the path it may be.
    pub fn plan(
        songs: Vec<Presentation>,
        themes: &[Theme],
        playlist: Option<PlaylistFile>,
        id_for: impl Fn(&str) -> Option<Uuid>,
    ) -> Export {
        let mut export = Export {
            songs: Vec::new(),
            themes: Vec::new(),
            pictures: Vec::new(),
            videos: Vec::new(),
            videos_left_out: Vec::new(),
            playlist: None,
            missing: 0,
        };
        let mut named: HashMap<PathBuf, String> = HashMap::new();
        let key_out = |key: &str| -> String {
            if Uuid::parse_str(key).is_ok() {
                key.to_owned()
            } else {
                id_for(key).map_or_else(|| "missing".to_owned(), |id| id.to_string())
            }
        };
        for mut song in songs {
            for group in &mut song.groups {
                for slide in &mut group.slides {
                    for object in &mut slide.objects {
                        if let ObjectKind::Image { file, .. } = &mut object.kind {
                            *file = export.name_file(file, &mut named);
                        }
                    }
                    for action in &mut slide.actions {
                        if let ShowAction::Song { key } = action {
                            *key = key_out(key);
                        }
                    }
                }
            }
            if let Some(id) = song.theme
                && !export.themes.iter().any(|theme| theme.id == id)
                && let Some(theme) = themes.iter().find(|theme| theme.id == id)
            {
                let mut theme = theme.clone();
                for template in &mut theme.templates {
                    for object in &mut template.objects {
                        if let ObjectKind::Image { file, .. } = &mut object.kind {
                            *file = export.name_file(file, &mut named);
                        }
                    }
                }
                if let Some(picture) = theme.background_picture.take() {
                    theme.background_picture = Some(export.name_file(&picture, &mut named)).filter(|n| !n.is_empty());
                }
                export.themes.push(theme);
            }
            export.songs.push(song);
        }
        export.playlist = playlist.map(|mut playlist| {
            for item in &mut playlist.items {
                if let Item::Song { key, .. } = item {
                    *key = key_out(key);
                }
            }
            playlist
        });
        export
    }

    /// The name a song's picture or video has in the pack, noting the file to put in it.
    fn name_file(&mut self, file: &str, named: &mut HashMap<PathBuf, String>) -> String {
        // Only a picture or a video goes in a pack: a song brought in some other way may name any
        // file (a key, a document), which must never leave the computer this way.
        let Some(path) = crate::images::resolve(file).filter(|path| crate::media::is_media(path)) else {
            self.missing += 1;
            return String::new();
        };
        if let Some(name) = named.get(&path) {
            return name.clone();
        }
        let ending =
            path.extension().map_or_else(String::new, |ext| format!(".{}", ext.to_string_lossy().to_lowercase()));
        let name = format!("{}{ending}", Uuid::new_v4());
        let here = path.is_file();
        if !here {
            self.missing += 1;
        }
        let shown = if crate::media::is_video(&path) {
            if here {
                self.videos.push((name.clone(), path.clone()));
            }
            format!("{MEDIA_PREFIX}{name}")
        } else {
            if !here {
                return String::new();
            }
            self.pictures.push((name.clone(), path.clone()));
            name
        };
        named.insert(path, shown.clone());
        shown
    }

    /// The size of the videos the pack would hold.
    pub fn video_bytes(&self) -> u64 {
        self.videos.iter().filter_map(|(_, path)| std::fs::metadata(path).ok()).map(|meta| meta.len()).sum()
    }

    /// Leaves the videos out: the slides keep them, shown as missing where the pack is opened.
    pub fn leave_out_videos(&mut self) {
        self.videos_left_out.append(&mut self.videos);
    }

    /// Writes the pack to `to` (beside it first, then moved into place), and says what is in it.
    pub fn write(&self, to: &Path) -> Result<String, String> {
        let partial = to.with_extension(format!("{ENDING}.part"));
        let result = self.write_to(&partial);
        match result.and_then(|()| std::fs::rename(&partial, to).map_err(|e| e.to_string())) {
            Ok(()) => Ok(self.summary(to)),
            Err(e) => {
                let _ = std::fs::remove_file(&partial);
                Err(format!("The song pack could not be written: {e}"))
            }
        }
    }

    fn write_to(&self, path: &Path) -> Result<(), String> {
        use zip::write::SimpleFileOptions;
        let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipWriter::new(std::io::BufWriter::new(file));
        let words = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        // Pictures and videos are compressed already.
        let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let manifest = serde_json::json!({
            "format": FORMAT,
            "version": VERSION,
            "app": env!("CARGO_PKG_VERSION"),
            "songs": self.songs.iter().map(|s| serde_json::json!({"id": s.id, "title": s.title})).collect::<Vec<_>>(),
            "themes": self.themes.iter().map(|t| serde_json::json!({"id": t.id, "name": t.name})).collect::<Vec<_>>(),
            "pictures": self.pictures.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            "videos": self.videos.iter().map(|(name, _)| name).collect::<Vec<_>>(),
            "playlist": self.playlist.as_ref().map(|playlist| playlist.name.clone()),
        });
        let mut put = |name: String, text: String| -> Result<(), String> {
            zip.start_file(name, words).map_err(|e| e.to_string())?;
            zip.write_all(text.as_bytes()).map_err(|e| e.to_string())
        };
        put("manifest.json".to_owned(), serde_json::to_string_pretty(&manifest).unwrap_or_default())?;
        for song in &self.songs {
            put(format!("songs/{}.sppres", song.id), sp_model::to_json(song))?;
        }
        for theme in &self.themes {
            put(format!("themes/{}.sptheme", theme.id), theme.to_json())?;
        }
        if let Some(playlist) = &self.playlist {
            put("playlist.json".to_owned(), serde_json::to_string_pretty(playlist).unwrap_or_default())?;
        }
        for (folder, files) in [("images", &self.pictures), ("media", &self.videos)] {
            for (name, source) in files {
                let size = std::fs::metadata(source).map(|meta| meta.len()).unwrap_or(0);
                // A file of 4 GB or more needs the larger form of zip entry.
                let options = stored.large_file(size >= u64::from(u32::MAX));
                zip.start_file(format!("{folder}/{name}"), options).map_err(|e| e.to_string())?;
                let mut from = std::fs::File::open(source).map_err(|e| format!("{}: {e}", file_name(source)))?;
                std::io::copy(&mut from, &mut zip).map_err(|e| e.to_string())?;
            }
        }
        zip.finish().map_err(|e| e.to_string())?.flush().map_err(|e| e.to_string())
    }

    /// What the pack holds, in words.
    pub fn summary(&self, to: &Path) -> String {
        let count = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
        let mut parts = vec![count(self.songs.len(), "song", "songs")];
        if !self.themes.is_empty() {
            parts.push(count(self.themes.len(), "theme", "themes"));
        }
        if !self.pictures.is_empty() {
            parts.push(count(self.pictures.len(), "picture", "pictures"));
        }
        if !self.videos.is_empty() {
            parts.push(format!("{} ({})", count(self.videos.len(), "video", "videos"), size_words(self.video_bytes())));
        }
        let mut text = format!("Saved {}: {}", file_name(to), parts.join(", "));
        if let Some(playlist) = &self.playlist {
            text.push_str(&format!(", and the playlist {}", playlist.name));
        }
        text.push('.');
        if !self.videos_left_out.is_empty() {
            text.push_str(&format!(" {} left out.", count(self.videos_left_out.len(), "video", "videos")));
        }
        if self.missing > 0 {
            text.push_str(&match self.missing {
                1 => " A picture or video the songs name couldn't go in (not on this computer, or not a picture or video).".to_owned(),
                n => format!(" {n} pictures or videos the songs name couldn't go in (not on this computer, or not pictures or videos)."),
            });
        }
        text
    }
}

/// A size in words: "1.2 GB", "340 MB".
pub fn size_words(bytes: u64) -> String {
    let mb = bytes as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 { format!("{:.1} GB", mb / 1024.0) } else { format!("{:.0} MB", mb.max(1.0)) }
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

/// What an entry in a pack is, by its name.
#[derive(Debug, PartialEq, Eq)]
enum Entry {
    Manifest,
    Playlist,
    Song,
    Theme,
    Picture(String),
    Video(String),
}

/// A name that is a plain file name: no folders, no `.` or `..`, not hidden, not too long.
fn plain(name: &str) -> Option<&str> {
    let fine = !name.is_empty()
        && name.len() <= 255
        && !name.starts_with('.')
        && !name.chars().any(|c| matches!(c, '/' | '\\' | ':') || c.is_control());
    fine.then_some(name)
}

/// What a pack's entry is, from its name; `None` for anything a pack doesn't hold.
fn classify(name: &str) -> Option<Entry> {
    match name {
        "manifest.json" => return Some(Entry::Manifest),
        "playlist.json" => return Some(Entry::Playlist),
        _ => {}
    }
    let (folder, rest) = name.split_once('/')?;
    let rest = plain(rest)?;
    let path = Path::new(rest);
    match folder {
        "songs" if rest.ends_with(".sppres") => Some(Entry::Song),
        "themes" if rest.ends_with(".sptheme") => Some(Entry::Theme),
        "images" if crate::media::is_media(path) && !crate::media::is_video(path) => {
            Some(Entry::Picture(rest.to_owned()))
        }
        "media" if crate::media::is_video(path) => Some(Entry::Video(rest.to_owned())),
        _ => None,
    }
}

/// A name from a pack as it may be shown: no control characters, not too long.
fn shown(name: &str) -> String {
    let clean: String = name.chars().filter(|c| !c.is_control()).take(80).collect();
    if clean.is_empty() { "(an entry with no name)".to_owned() } else { clean }
}

/// A pack read and checked, its pictures and videos put in a folder of the app's own.
pub struct Opened {
    pub songs: Vec<Presentation>,
    pub themes: HashMap<Uuid, Theme>,
    /// Each picture and video by its name in the pack, and where it was put.
    pub pictures: HashMap<String, PathBuf>,
    pub videos: HashMap<String, PathBuf>,
    pub playlist: Option<PlaylistFile>,
    /// What was left out, and why.
    pub ignored: Vec<String>,
    /// The folder the pictures and videos were put in, removed once the pack is in.
    pub staging: PathBuf,
}

/// Reads a pack into `staging` (a new folder of the app's), checking everything as the module's
/// notes say. The error is a sentence for the operator.
pub fn open(path: &Path, staging: &Path) -> Result<Opened, String> {
    let name = file_name(path);
    let not_a_pack = || format!("{name} isn't a SoftPresenter song pack.");
    let file = std::fs::File::open(path).map_err(|e| format!("{name} could not be opened: {e}"))?;
    let mut zip = zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|_| not_a_pack())?;
    if zip.len() > MOST_ENTRIES {
        return Err(format!("{name} has too many files in it to be a song pack."));
    }
    // First, what each entry is, and the room the pictures and videos say they need.
    let mut kinds = Vec::new();
    let mut ignored = Vec::new();
    let mut seen = HashSet::new();
    let mut said_size: u64 = 0;
    for at in 0..zip.len() {
        let entry = zip.by_index(at).map_err(|_| not_a_pack())?;
        let raw = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        if entry.is_dir() {
            kinds.push(None);
            continue;
        }
        let kind = if entry.is_symlink() || entry.encrypted() { None } else { classify(&raw) };
        let kind = match kind {
            Some(kind) if seen.insert(raw.to_lowercase()) => Some(kind),
            Some(_) => {
                ignored.push(format!("{} (twice)", shown(&raw)));
                None
            }
            None => {
                ignored.push(shown(&raw));
                None
            }
        };
        if matches!(kind, Some(Entry::Picture(_) | Entry::Video(_))) {
            said_size = said_size.saturating_add(entry.size());
        }
        kinds.push(kind);
    }
    if !kinds.contains(&Some(Entry::Manifest)) {
        return Err(not_a_pack());
    }
    std::fs::create_dir_all(staging).map_err(|e| format!("Could not make room for the pack: {e}"))?;
    // What may be written, whatever the entries say about their size.
    let room = free_space(staging).map_or(u64::MAX, |free| free.saturating_sub(KEEP_FREE));
    if said_size > room {
        return Err(format!(
            "{name} needs {} free, and this disk has {}.",
            size_words(said_size + KEEP_FREE),
            size_words(room + KEEP_FREE)
        ));
    }
    let mut opened = Opened {
        songs: Vec::new(),
        themes: HashMap::new(),
        pictures: HashMap::new(),
        videos: HashMap::new(),
        playlist: None,
        ignored,
        staging: staging.to_path_buf(),
    };
    let mut written: u64 = 0;
    let mut words_read: u64 = 0;
    let mut manifest = None;
    for (at, kind) in kinds.into_iter().enumerate() {
        let Some(kind) = kind else { continue };
        let mut entry = zip.by_index(at).map_err(|_| not_a_pack())?;
        let raw = entry.name().map(|n| n.into_owned()).unwrap_or_default();
        if !matches!(kind, Entry::Picture(_) | Entry::Video(_)) {
            words_read = words_read.saturating_add(entry.size().min(LARGEST_TEXT));
            if words_read > MOST_WORDS {
                return Err(format!("{name} holds more songs and themes than a song pack can."));
            }
        }
        match kind {
            Entry::Manifest => manifest = Some(read_words(&mut entry).map_err(|why| format!("{name}: {why}"))?),
            Entry::Playlist => {
                match read_words(&mut entry).and_then(|text| serde_json::from_str(&text).map_err(|e| e.to_string())) {
                    Ok(playlist) => opened.playlist = Some(playlist),
                    Err(why) => opened.ignored.push(format!("the playlist ({why})")),
                }
            }
            Entry::Song => match read_words(&mut entry).and_then(|text| sp_model::from_json(&text)) {
                Ok(song) if opened.songs.iter().any(|have| have.id == song.id) => {
                    opened.ignored.push(format!("{} (a second copy of a song)", shown(&raw)));
                }
                Ok(song) => opened.songs.push(song),
                Err(why) => opened.ignored.push(format!("{} ({why})", shown(&raw))),
            },
            Entry::Theme => match read_words(&mut entry).and_then(|text| Theme::from_json(&text)) {
                Ok(theme) => {
                    opened.themes.insert(theme.id, theme);
                }
                Err(why) => opened.ignored.push(format!("{} ({why})", shown(&raw))),
            },
            Entry::Picture(label) | Entry::Video(label) => {
                let video = crate::media::is_video(Path::new(&label));
                let cap = if video {
                    room.saturating_sub(written)
                } else {
                    LARGEST_PICTURE.min(room.saturating_sub(written))
                };
                let ending = Path::new(&label)
                    .extension()
                    .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
                let into = staging.join(format!("{}{ending}", Uuid::new_v4()));
                match put_file(&mut entry, &into, cap) {
                    Ok(size) => {
                        written += size;
                        let list = if video { &mut opened.videos } else { &mut opened.pictures };
                        list.insert(label, into);
                    }
                    Err(why) => opened.ignored.push(format!("{} ({why})", shown(&raw))),
                }
            }
        }
    }
    let manifest: serde_json::Value =
        manifest.as_deref().and_then(|text| serde_json::from_str(text).ok()).ok_or_else(not_a_pack)?;
    if manifest.get("format").and_then(|f| f.as_str()) != Some(FORMAT) {
        return Err(not_a_pack());
    }
    let version = manifest.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
    if version == 0 || version > VERSION {
        return Err(format!(
            "{name} was made by a newer SoftPresenter (pack version {version}); update SoftPresenter to open it."
        ));
    }
    Ok(opened)
}

/// Reads a song, theme, manifest or playlist: at most `LARGEST_TEXT`, whatever the entry says.
fn read_words(entry: &mut impl Read) -> Result<String, String> {
    let mut bytes = Vec::new();
    entry.take(LARGEST_TEXT + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LARGEST_TEXT {
        return Err("too large".to_owned());
    }
    String::from_utf8(bytes).map_err(|_| "not text".to_owned())
}

/// Copies a picture or video out of the pack into `into`: at most `cap` bytes, whatever the
/// entry says. Returns its size.
fn put_file(entry: &mut impl Read, into: &Path, cap: u64) -> Result<u64, String> {
    let mut out = std::fs::File::create(into).map_err(|e| e.to_string())?;
    let copied = std::io::copy(&mut entry.take(cap.saturating_add(1)), &mut out);
    match copied {
        Ok(size) if size <= cap => Ok(size),
        Ok(_) => {
            let _ = std::fs::remove_file(into);
            Err("too large".to_owned())
        }
        Err(e) => {
            let _ = std::fs::remove_file(into);
            Err(e.to_string())
        }
    }
}

/// The free space on the disk holding `folder`, from `df` (the standard library has no call
/// for it).
fn free_space(folder: &Path) -> Option<u64> {
    let out = std::process::Command::new("/bin/df").arg("-k").arg(folder).output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let line = text.lines().nth(1)?;
    let kb: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(kb * 1024)
}

/// Where a song or theme from a pack points at a picture or video: only at one the pack itself
/// holds, copied into the library under a name the app makes (pictures in Images, videos in
/// Media). Anything else (a path on the computer, `..`, a name the pack doesn't have) becomes
/// nothing, so the slide shows the picture as missing. `brought` remembers what was copied.
pub fn bring_file(file: &str, opened: &Opened, library: &Path, brought: &mut HashMap<String, String>) -> String {
    if let Some(done) = brought.get(file) {
        return done.clone();
    }
    let (from, label, folder, prefix) = match file.strip_prefix(MEDIA_PREFIX) {
        Some(name) if plain(name).is_some() => (opened.videos.get(name), name, "Media", MEDIA_PREFIX),
        Some(_) => (None, "", "", ""),
        None if plain(file).is_some() => (opened.pictures.get(file), file, "Images", ""),
        None => (None, "", "", ""),
    };
    let Some(from) = from else { return String::new() };
    // The ending is the pack's name's (checked to be a picture's or a video's when it was read).
    let ending =
        Path::new(label).extension().map_or_else(String::new, |e| format!(".{}", e.to_string_lossy().to_lowercase()));
    let name = format!("{}{ending}", Uuid::new_v4());
    let dir = library.join(folder);
    let to = dir.join(&name);
    let moved = std::fs::create_dir_all(&dir)
        .and_then(|()| std::fs::rename(from, &to).or_else(|_| std::fs::copy(from, &to).map(|_| ())));
    let named = if moved.is_ok() { format!("{prefix}{name}") } else { String::new() };
    brought.insert(file.to_owned(), named.clone());
    named
}

/// Points every picture and video of a song from a pack at the library's copies (see
/// `bring_file`).
pub fn bring_song_files(
    song: &mut Presentation,
    opened: &Opened,
    library: &Path,
    brought: &mut HashMap<String, String>,
) {
    for group in &mut song.groups {
        for slide in &mut group.slides {
            for object in &mut slide.objects {
                if let ObjectKind::Image { file, .. } = &mut object.kind {
                    *file = bring_file(file, opened, library, brought);
                }
            }
        }
    }
}

/// The same for a theme from a pack.
pub fn bring_theme_files(theme: &mut Theme, opened: &Opened, library: &Path, brought: &mut HashMap<String, String>) {
    for template in &mut theme.templates {
        for object in &mut template.objects {
            if let ObjectKind::Image { file, .. } = &mut object.kind {
                *file = bring_file(file, opened, library, brought);
            }
        }
    }
    if let Some(picture) = theme.background_picture.take() {
        theme.background_picture = Some(bring_file(&picture, opened, library, brought)).filter(|n| !n.is_empty());
    }
}

/// What to do with a song from a pack that the library has already (the same id).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Same {
    Skip,
    Replace,
    KeepBoth,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_kinds_of_entry_a_pack_holds_are_read() {
        assert_eq!(classify("manifest.json"), Some(Entry::Manifest));
        assert_eq!(classify("songs/abc.sppres"), Some(Entry::Song));
        assert_eq!(classify("themes/abc.sptheme"), Some(Entry::Theme));
        assert_eq!(classify("images/a.png"), Some(Entry::Picture("a.png".to_owned())));
        assert_eq!(classify("media/a.mp4"), Some(Entry::Video("a.mp4".to_owned())));
        for bad in [
            "../songs/a.sppres",
            "songs/../a.sppres",
            "/songs/a.sppres",
            "songs/sub/a.sppres",
            "songs/.hidden.sppres",
            "songs/a.txt",
            "images/a.mp4",
            "media/a.png",
            "images/..",
            "images/a\\b.png",
            "images/C:a.png",
            "other.json",
            "songs/",
        ] {
            assert_eq!(classify(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_picture_or_video_is_only_ever_one_the_pack_holds() {
        let dir = std::env::temp_dir().join(format!("sp-pack-bring-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let staging = dir.join("staging");
        let library = dir.join("library");
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("p"), [1u8]).unwrap();
        std::fs::write(staging.join("v"), [2u8]).unwrap();
        let opened = Opened {
            songs: Vec::new(),
            themes: HashMap::new(),
            pictures: HashMap::from([("a.png".to_owned(), staging.join("p"))]),
            videos: HashMap::from([("b.mp4".to_owned(), staging.join("v"))]),
            playlist: None,
            ignored: Vec::new(),
            staging: staging.clone(),
        };
        let mut brought = HashMap::new();
        let picture = bring_file("a.png", &opened, &library, &mut brought);
        assert!(library.join("Images").join(&picture).is_file() && picture.ends_with(".png") && picture != "a.png");
        assert_eq!(bring_file("a.png", &opened, &library, &mut brought), picture, "once");
        let video = bring_file("Media/b.mp4", &opened, &library, &mut brought);
        assert!(video.starts_with(MEDIA_PREFIX) && library.join(&video).is_file());
        for hostile in
            ["/etc/passwd", "../a.png", "Media/../a.png", "Media/missing.mp4", "missing.png", "Images/a.png", ""]
        {
            assert_eq!(bring_file(hostile, &opened, &library, &mut brought), "", "{hostile}");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sizes_read_as_words() {
        assert_eq!(size_words(340 * 1024 * 1024), "340 MB");
        assert_eq!(size_words(1288 * 1024 * 1024), "1.3 GB");
        assert_eq!(size_words(10), "1 MB");
    }
}
