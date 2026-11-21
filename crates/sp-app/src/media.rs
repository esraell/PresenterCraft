//! Loading pictures and videos for the media bin.

use std::path::{Path, PathBuf};

use sp_render::Picture;

/// Pictures larger than this on their longer side are scaled down when loaded. A screen is at
/// most a few thousand pixels wide, and a phone photo kept at full size costs tens of megabytes.
pub const MAX_SIDE: u32 = 2560;

/// A video's still is shown only until its first picture comes (and in thumbnails), so it is
/// kept smaller.
const VIDEO_STILL_SIDE: u32 = 1280;

/// How big the copy of a picture kept for thumbnails and the Program preview is. The media bin
/// keeps this copy of each item; the full-size picture is read when the item goes on screen
/// (see `bin_pictures`).
pub const SMALL_SIDE: u32 = 960;

/// Pictures decoded here.
const OWN_PICTURES: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// Pictures the computer's own system reads: iPhone photos, TIFF, Photoshop and others.
const SYSTEM_PICTURES: [&str; 7] = ["heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// Every picture's file ending, as the file chooser lists them.
pub const PICTURES: [&str; 13] =
    ["png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// The file endings of everything the media bin takes, as the file chooser lists them.
pub const PICTURE_ENDINGS: [&str; 30] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2", "mp4", "mov",
    "m4v", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "ts", "ogv", "3gp", "vob", "divx",
];

fn list_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("media.json"))
}

/// A file in the media bin and how it is shown.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Kept between runs, so slide actions that show the item find it again.
    #[serde(default = "sp_model::Uuid::new_v4")]
    pub id: sp_model::Uuid,
    pub path: PathBuf,
    #[serde(default = "filled")]
    pub fit: sp_model::ImageFit,
    #[serde(default = "yes")]
    pub looping: bool,
}

fn filled() -> sp_model::ImageFit {
    sp_model::ImageFit::Fill
}

fn yes() -> bool {
    true
}

/// A named group of the media bin's files, such as "Backgrounds" or "Announcements". A file
/// can be in any number of them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: sp_model::Uuid,
    pub name: String,
    pub files: Vec<PathBuf>,
    /// A folder the playlist follows: the pictures and videos in it are its files, kept up to
    /// date as files come, go and change (such as an announcements video exported from Canva).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
}

impl Playlist {
    pub fn new(name: String) -> Playlist {
        Playlist { id: sp_model::Uuid::new_v4(), name, files: Vec::new(), folder: None }
    }

    pub fn has(&self, path: &Path) -> bool {
        self.files.iter().any(|have| have == path)
    }

    /// Puts a file in, or takes it out if it is in.
    pub fn toggle(&mut self, path: &Path) {
        if self.has(path) {
            self.files.retain(|have| have != path);
        } else {
            self.files.push(path.to_owned());
        }
    }
}

/// A name no media playlist has: "New Playlist", "New Playlist 2", and so on.
pub fn new_playlist_name(playlists: &[Playlist]) -> String {
    let taken = |name: &str| playlists.iter().any(|playlist| playlist.name == name);
    std::iter::once("New Playlist".to_owned())
        .chain((2..).map(|n| format!("New Playlist {n}")))
        .find(|name| !taken(name))
        .unwrap_or_default()
}

/// Where the media bin item at `at` is once the one at `removed` has been taken out: one place
/// up if it was after it, nowhere if it was that one.
pub fn place_after_removal(at: usize, removed: usize) -> Option<usize> {
    match at.cmp(&removed) {
        std::cmp::Ordering::Less => Some(at),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => Some(at - 1),
    }
}

/// The pictures and videos in a folder, by name, with when each last changed. Hidden files
/// (such as one still being downloaded) are left out.
pub fn folder_files(folder: &Path) -> Vec<(PathBuf, std::time::SystemTime)> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| is_media(path))
        .filter(|path| path.file_name().is_some_and(|name| !name.to_string_lossy().starts_with('.')))
        .filter_map(|path| {
            let changed = std::fs::metadata(&path).and_then(|meta| meta.modified()).ok()?;
            Some((path, changed))
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files
}

/// What `media.json` keeps: the files in the media bin with how each is shown, and the
/// media playlists.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct List {
    pub entries: Vec<Entry>,
    #[serde(default)]
    pub playlists: Vec<Playlist>,
}

impl List {
    /// Reads the file's words. A file from before media settings is a plain list of files.
    pub fn from_json(text: &str) -> Option<List> {
        if let Ok(paths) = serde_json::from_str::<Vec<PathBuf>>(text) {
            let entries = paths
                .into_iter()
                .map(|path| Entry { id: sp_model::Uuid::new_v4(), path, fit: filled(), looping: true })
                .collect();
            return Some(List { entries, playlists: Vec::new() });
        }
        serde_json::from_str(text).ok()
    }
}

/// The media bin as it was when the app last closed.
pub fn load_list() -> List {
    list_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| List::from_json(&text))
        .unwrap_or_default()
}

/// Remembers the media bin, for the next start.
pub fn save_list(list: &List) -> Result<(), String> {
    let path = list_file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    let partial = path.with_extension("json.part");
    std::fs::write(&partial, serde_json::to_string_pretty(list).unwrap_or_default())
        .and_then(|()| std::fs::rename(&partial, &path))
        .map_err(|e| format!("Could not save the media list: {e}"))
}
/// Videos by their names. macOS's own player plays MP4 and MOV; FFmpeg plays the rest (and any
/// MP4 or MOV the system's player can't).
const VIDEOS: [&str; 17] = [
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "wmv", "flv", "mpg", "mpeg", "m2ts", "mts", "ts", "ogv", "3gp", "vob",
    "divx",
];

fn extension(path: &Path) -> String {
    path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

/// True for a file whose name says it is a picture or a video, so it belongs in the media bin
/// and is not a song.
pub fn is_media(path: &Path) -> bool {
    let ext = extension(path);
    PICTURES.contains(&ext.as_str()) || VIDEOS.contains(&ext.as_str())
}

/// True for a video, which plays when it goes on screen.
pub fn is_video(path: &Path) -> bool {
    VIDEOS.contains(&extension(path).as_str())
}

/// Reads and decodes a picture file; for a video, a still from it. The error is a plain
/// sentence for the operator.
pub fn load_picture(path: &Path) -> Result<Picture, String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let ext = extension(path);
    let from_system = |frame: Result<sp_video::Frame, String>| {
        let frame = frame?;
        Picture::from_rgba(frame.width, frame.height, frame.rgba)
            .ok_or_else(|| format!("{name}: the picture is empty."))
    };
    if VIDEOS.contains(&ext.as_str()) {
        return from_system(sp_video::still(path, VIDEO_STILL_SIDE));
    }
    if SYSTEM_PICTURES.contains(&ext.as_str()) {
        return from_system(sp_video::picture(path, MAX_SIDE));
    }
    if !OWN_PICTURES.contains(&ext.as_str()) {
        return Err(format!(
            "{name}: not a picture or video SoftPresenter can open (such as PNG, JPEG, HEIC, TIFF, MP4 or MOV)."
        ));
    }
    let decoded = image::open(path).map_err(|e| format!("{name}: could not be opened ({e})."))?;
    let decoded = if decoded.width().max(decoded.height()) > MAX_SIDE {
        decoded.resize(MAX_SIDE, MAX_SIDE, image::imageops::FilterType::Triangle)
    } else {
        decoded
    };
    let rgba = decoded.to_rgba8();
    let (width, height) = rgba.dimensions();
    Picture::from_rgba(width, height, rgba.into_raw()).ok_or_else(|| format!("{name}: the picture is empty."))
}

/// Reads a picture file, or a video's still, as the small copy the media bin keeps. True with
/// it when the copy is the whole picture (it was no larger), or a video's still (shown on the
/// screens only until the video's first picture comes), so the screens can show it as it is.
pub fn load_small(path: &Path) -> Result<(Picture, bool), String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let from_system = |frame: Result<sp_video::Frame, String>| {
        let frame = frame?;
        Picture::from_rgba(frame.width, frame.height, frame.rgba)
            .ok_or_else(|| format!("{name}: the picture is empty."))
    };
    let ext = extension(path);
    if VIDEOS.contains(&ext.as_str()) {
        return Ok((from_system(sp_video::still(path, SMALL_SIDE))?, true));
    }
    if SYSTEM_PICTURES.contains(&ext.as_str()) {
        // The system makes a copy no larger than asked; one smaller is the whole picture.
        let picture = from_system(sp_video::picture(path, SMALL_SIDE))?;
        let whole = picture.width().max(picture.height()) < SMALL_SIDE;
        return Ok((picture, whole));
    }
    let full = load_picture(path)?;
    let whole = full.width().max(full.height()) <= SMALL_SIDE;
    Ok((full.shrunk(SMALL_SIDE), whole))
}

#[cfg(test)]
mod tests {

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("softpresenter-media-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    fn write_png(path: &Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 200, 30, 255])).save(path).unwrap();
    }

    #[test]
    fn an_old_media_list_still_loads_and_a_new_one_keeps_settings_and_playlists() {
        let old = List::from_json(r#"["/a/sky.png", "/a/loop.mp4"]"#).unwrap();
        assert_eq!(old.entries.len(), 2);
        assert!(old.entries.iter().all(|entry| entry.fit == sp_model::ImageFit::Fill && entry.looping));
        let mut backgrounds = Playlist::new(new_playlist_name(&[]));
        backgrounds.toggle(Path::new("/a/sky.png"));
        assert_eq!(new_playlist_name(std::slice::from_ref(&backgrounds)), "New Playlist 2");
        let list = List {
            entries: vec![Entry {
                id: sp_model::Uuid::new_v4(),
                path: "/a/sky.png".into(),
                fit: sp_model::ImageFit::Fit,
                looping: false,
            }],
            playlists: vec![backgrounds],
        };
        assert_eq!(List::from_json(&serde_json::to_string(&list).unwrap()), Some(list));
    }

    #[test]
    fn a_folder_lists_its_pictures_and_videos_by_name_without_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("softpresenter-folder-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["b.mp4", "a.png", ".downloading.mp4", "notes.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let names: Vec<String> = folder_files(&dir)
            .into_iter()
            .map(|(path, _)| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.png", "b.mp4"]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn places_after_an_item_leaves_the_bin_move_up_and_its_own_is_gone() {
        let picked: Vec<usize> = [0, 2, 3, 5].iter().filter_map(|at| place_after_removal(*at, 3)).collect();
        assert_eq!(picked, [0, 2, 4]);
        assert_eq!(place_after_removal(3, 3), None);
    }

    #[test]
    fn media_files_are_told_from_songs_by_their_names() {
        assert!(is_media(Path::new("a/Sunrise.JPG")));
        assert!(is_media(Path::new("clip.mov")));
        assert!(!is_media(Path::new("song.txt")));
        assert!(!is_media(Path::new("folder")));
    }

    #[test]
    fn loads_a_png_at_its_own_size() {
        let path = temp("small.png");
        write_png(&path, 64, 36);
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (64, 36));
    }

    #[test]
    fn the_small_copy_keeps_the_shape_and_says_when_it_is_the_whole_picture() {
        let (big, small) = (temp("small-copy-big.png"), temp("small-copy-small.png"));
        write_png(&big, 1920, 1080);
        write_png(&small, 640, 360);
        let (copy, whole) = load_small(&big).unwrap();
        assert_eq!((copy.width(), copy.height(), whole), (SMALL_SIDE, 540, false));
        let (copy, whole) = load_small(&small).unwrap();
        assert_eq!((copy.width(), copy.height(), whole), (640, 360, true));
        assert!(load_small(Path::new("/x/notes.txt")).unwrap_err().contains("not a picture"));
    }

    #[test]
    fn a_huge_picture_is_scaled_down_keeping_its_shape() {
        let path = temp("huge.png");
        write_png(&path, 5120, 1280);
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (MAX_SIDE, 640));
    }

    #[test]
    fn other_files_are_refused_in_plain_words() {
        assert!(is_video(Path::new("/x/clip.WEBM")) && is_video(Path::new("/x/clip.mkv")));
        assert!(load_picture(Path::new("/x/notes.txt")).unwrap_err().contains("not a picture"));
        assert!(load_picture(Path::new("/x/no-extension")).unwrap_err().contains("not a picture"));
        assert!(is_video(Path::new("a/Clip.MOV")) && !is_video(Path::new("a/photo.heic")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_system_reads_tiff_and_a_missing_video_is_said_so() {
        let path = temp("small.tiff");
        image::RgbaImage::from_pixel(40, 30, image::Rgba([10, 200, 30, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        // A PNG named .tiff still opens: the system reads what the file is, not its name.
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (40, 30));
        assert!(load_picture(Path::new("/x/clip.MP4")).unwrap_err().contains("could not be found"));
    }

    #[test]
    fn a_broken_picture_file_is_an_error_not_a_crash() {
        let path = temp("broken.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        assert!(load_picture(&path).unwrap_err().contains("could not be opened"));
        assert!(load_picture(&temp("missing.png")).unwrap_err().contains("could not be opened"));
    }
}
