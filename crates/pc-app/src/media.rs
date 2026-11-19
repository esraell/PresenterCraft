//! Loading pictures and videos for the media bin.

use std::path::{Path, PathBuf};

use pc_render::Picture;

/// Pictures larger than this on their longer side are scaled down when loaded. A screen is at
/// most a few thousand pixels wide, and a phone photo kept at full size costs tens of megabytes.
pub const MAX_SIDE: u32 = 2560;

/// A video's still is shown only until its first picture comes (and in thumbnails), so it is
/// kept smaller.
const VIDEO_STILL_SIDE: u32 = 1280;

/// How big the copy of a picture used for thumbnails and the Program preview is.
pub const SMALL_SIDE: u32 = 960;

/// A media item's picture at full size, for the screens, and a small copy, for thumbnails and
/// the Program preview, so the window doesn't hold full-size pictures it shows small.
#[derive(Clone)]
pub struct MediaPicture {
    pub full: Picture,
    pub small: Picture,
}

impl MediaPicture {
    pub fn of(full: Picture) -> MediaPicture {
        let small = full.shrunk(SMALL_SIDE);
        MediaPicture { full, small }
    }
}

/// Pictures decoded here.
const OWN_PICTURES: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// Pictures the computer's own system reads: iPhone photos, TIFF, Photoshop and others.
const SYSTEM_PICTURES: [&str; 7] = ["heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// Every picture's file ending, as the file chooser lists them.
pub const PICTURES: [&str; 13] =
    ["png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2"];

/// Videos the system plays.
const PLAYABLE: [&str; 3] = ["mp4", "mov", "m4v"];

/// The file endings of everything the media bin takes, as the file chooser lists them.
pub const PICTURE_ENDINGS: [&str; 16] = [
    "png", "jpg", "jpeg", "webp", "bmp", "gif", "heic", "heif", "tif", "tiff", "psd", "avif", "jp2", "mp4", "mov",
    "m4v",
];

fn list_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("media.json"))
}

/// A file in the media bin and how it is shown.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Entry {
    /// Kept between runs, so slide actions that show the item find it again.
    #[serde(default = "pc_model::Uuid::new_v4")]
    pub id: pc_model::Uuid,
    pub path: PathBuf,
    #[serde(default = "filled")]
    pub fit: pc_model::ImageFit,
    #[serde(default = "yes")]
    pub looping: bool,
}

fn filled() -> pc_model::ImageFit {
    pc_model::ImageFit::Fill
}

fn yes() -> bool {
    true
}

/// A named group of the media bin's files, such as "Backgrounds" or "Announcements". A file
/// can be in any number of them.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Playlist {
    pub id: pc_model::Uuid,
    pub name: String,
    pub files: Vec<PathBuf>,
    /// A folder the playlist follows: the pictures and videos in it are its files, kept up to
    /// date as files come, go and change (such as an announcements video exported from Canva).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub folder: Option<PathBuf>,
}

impl Playlist {
    pub fn new(name: String) -> Playlist {
        Playlist { id: pc_model::Uuid::new_v4(), name, files: Vec::new(), folder: None }
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
                .map(|path| Entry { id: pc_model::Uuid::new_v4(), path, fit: filled(), looping: true })
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
/// Videos by their names: the ones the system plays, and others it doesn't.
const VIDEOS: [&str; 6] = ["mp4", "mov", "m4v", "mkv", "avi", "webm"];

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
    let from_system = |frame: Result<pc_video::Frame, String>| {
        let frame = frame?;
        Picture::from_rgba(frame.width, frame.height, frame.rgba)
            .ok_or_else(|| format!("{name}: the picture is empty."))
    };
    if PLAYABLE.contains(&ext.as_str()) {
        return from_system(pc_video::still(path, VIDEO_STILL_SIDE));
    }
    if VIDEOS.contains(&ext.as_str()) {
        return Err(format!("{name}: PresenterCraft plays MP4 and MOV videos. Convert this one to MP4 first."));
    }
    if SYSTEM_PICTURES.contains(&ext.as_str()) {
        return from_system(pc_video::picture(path, MAX_SIDE));
    }
    if !OWN_PICTURES.contains(&ext.as_str()) {
        return Err(format!(
            "{name}: not a picture or video PresenterCraft can open (such as PNG, JPEG, HEIC, TIFF, MP4 or MOV)."
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

#[cfg(test)]
mod tests {

    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("presentercraft-media-test-{}", std::process::id()));
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
        assert!(old.entries.iter().all(|entry| entry.fit == pc_model::ImageFit::Fill && entry.looping));
        let mut backgrounds = Playlist::new(new_playlist_name(&[]));
        backgrounds.toggle(Path::new("/a/sky.png"));
        assert_eq!(new_playlist_name(std::slice::from_ref(&backgrounds)), "New Playlist 2");
        let list = List {
            entries: vec![Entry {
                id: pc_model::Uuid::new_v4(),
                path: "/a/sky.png".into(),
                fit: pc_model::ImageFit::Fit,
                looping: false,
            }],
            playlists: vec![backgrounds],
        };
        assert_eq!(List::from_json(&serde_json::to_string(&list).unwrap()), Some(list));
    }

    #[test]
    fn a_folder_lists_its_pictures_and_videos_by_name_without_hidden_ones() {
        let dir = std::env::temp_dir().join(format!("presentercraft-folder-test-{}", std::process::id()));
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
    fn a_huge_picture_is_scaled_down_keeping_its_shape() {
        let path = temp("huge.png");
        write_png(&path, 5120, 1280);
        let picture = load_picture(&path).unwrap();
        assert_eq!((picture.width(), picture.height()), (MAX_SIDE, 640));
    }

    #[test]
    fn other_files_are_refused_in_plain_words() {
        assert!(load_picture(Path::new("/x/clip.webm")).unwrap_err().contains("Convert this one to MP4"));
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
