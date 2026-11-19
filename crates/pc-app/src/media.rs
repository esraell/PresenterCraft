//! Loading pictures for the media bin.

use std::path::{Path, PathBuf};

use pc_render::Picture;

/// Pictures larger than this on their longer side are scaled down when loaded. A screen is at
/// most a few thousand pixels wide, and a phone photo kept at full size costs tens of megabytes.
pub const MAX_SIDE: u32 = 2560;

const PICTURES: [&str; 6] = ["png", "jpg", "jpeg", "webp", "bmp", "gif"];

/// The file endings of pictures, as the file chooser lists them.
pub const PICTURE_ENDINGS: [&str; 6] = PICTURES;

fn list_file() -> Option<PathBuf> {
    crate::bible_store::data_dir().map(|dir| dir.join("media.json"))
}

/// The pictures that were in the media bin when the app last closed, by their files.
pub fn load_list() -> Vec<PathBuf> {
    list_file()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Remembers which pictures are in the media bin, for the next start.
pub fn save_list(paths: &[PathBuf]) -> Result<(), String> {
    let path = list_file().ok_or_else(|| "Could not find where to keep settings on this computer.".to_owned())?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("Could not make the app's folder: {e}"))?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(paths).unwrap_or_default())
        .map_err(|e| format!("Could not save the media list: {e}"))
}
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

/// Reads and decodes a picture file. The error is a plain sentence for the operator.
pub fn load_picture(path: &Path) -> Result<Picture, String> {
    let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
    let ext = extension(path);
    if VIDEOS.contains(&ext.as_str()) {
        return Err(format!("{name}: videos are not supported yet, only pictures."));
    }
    if !PICTURES.contains(&ext.as_str()) {
        return Err(format!("{name}: not a picture PresenterCraft can open (PNG, JPEG, WebP, BMP or GIF)."));
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
    fn videos_and_other_files_are_refused_in_plain_words() {
        assert!(load_picture(Path::new("/x/clip.MP4")).unwrap_err().contains("videos are not supported yet"));
        assert!(load_picture(Path::new("/x/notes.txt")).unwrap_err().contains("not a picture"));
        assert!(load_picture(Path::new("/x/no-extension")).unwrap_err().contains("not a picture"));
    }

    #[test]
    fn a_broken_picture_file_is_an_error_not_a_crash() {
        let path = temp("broken.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        assert!(load_picture(&path).unwrap_err().contains("could not be opened"));
        assert!(load_picture(&temp("missing.png")).unwrap_err().contains("could not be opened"));
    }
}
