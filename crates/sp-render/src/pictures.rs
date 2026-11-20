//! Pictures that slide objects show, by the file they name. The app says how a file becomes a
//! picture; each is read once and kept, so every screen shares it. What is kept is held to a
//! size: past it, the pictures drawn longest ago are let go, and read again if they are drawn
//! again.
//!
//! Something drawn small (a thumbnail, a preview) gets a small copy of the picture instead.
//! The renderer keeps every picture it draws in a texture of its own until that fills, and a
//! grid of thumbnails of full-size pages would fill it with full-size pictures: a 60-page deck
//! took a gigabyte that way.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::Picture;

/// Reads the picture a file names, or `None` if it can't.
pub type Loader = fn(&str) -> Option<Picture>;

/// How many bytes of pixels are kept at most: room for about sixty full-screen 1080p pictures.
const KEEP_BYTES: usize = 512 * 1024 * 1024;

/// A picture drawn no larger than this (its longer side, in pixels) is drawn from a copy this
/// size.
pub(crate) const SMALL_SIDE: u32 = 640;

/// The name a file's small copy is kept under.
fn small_name(file: &str) -> String {
    format!("{file}\u{0}small")
}

static LOADER: OnceLock<Loader> = OnceLock::new();
static KEPT: OnceLock<Mutex<Kept>> = OnceLock::new();

/// Says how a picture object's file is read. Set once, when the app starts.
pub fn set_picture_loader(loader: Loader) {
    let _ = LOADER.set(loader);
}

/// The picture a file names: read the first time it is asked for, then kept. A file that
/// can't be read is remembered as missing, so it is not tried again on every frame.
pub(crate) fn picture(file: &str) -> Option<Picture> {
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    if let Some(found) = kept.lock().ok().and_then(|mut kept| kept.get(file)) {
        return found;
    }
    // Read without holding the lock, so another screen isn't kept waiting.
    let read = LOADER.get().and_then(|load| load(file));
    if let Ok(mut kept) = kept.lock() {
        kept.insert(file, read.clone());
    }
    read
}

/// The picture a file names, for drawing with its longer side `drawn_side` pixels long: a small
/// copy when that is no more than [`SMALL_SIDE`], made once and kept (the full picture is not
/// kept for it). A picture shown in place of a file's, such as a video's newest frame, is
/// always handed back as it is, so it never stops moving in a thumbnail.
pub(crate) fn picture_at(file: &str, drawn_side: f64) -> Option<Picture> {
    if drawn_side > f64::from(SMALL_SIDE) {
        return picture(file);
    }
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    let small = small_name(file);
    if let Ok(mut kept) = kept.lock() {
        if kept.live.contains(file) {
            return kept.get(file).flatten();
        }
        if let Some(found) = kept.get(&small) {
            return found;
        }
        // The full picture, if it is kept already, makes the copy without reading the file.
        if let Some(Some(full)) = kept.get(file) {
            let copy = full.shrunk(SMALL_SIDE);
            kept.insert(&small, Some(copy.clone()));
            return Some(copy);
        }
    }
    let read = LOADER.get().and_then(|load| load(file)).map(|full| full.shrunk(SMALL_SIDE));
    if let Ok(mut kept) = kept.lock() {
        kept.insert(&small, read.clone());
    }
    read
}

/// Puts a picture in place of a file's, until it is forgotten: how a video on a slide shows its
/// newest frame where its still would be.
pub fn show_picture(file: &str, picture: Picture) {
    let kept = KEPT.get_or_init(|| Mutex::new(Kept::new(KEEP_BYTES)));
    if let Ok(mut kept) = kept.lock() {
        kept.insert(file, Some(picture));
        kept.live.insert(file.to_owned());
        kept.remove(&small_name(file));
    }
}

/// Forgets a file's picture, so it is read again the next time it is drawn.
pub fn forget_picture(file: &str) {
    if let Some(Ok(mut kept)) = KEPT.get().map(Mutex::lock) {
        kept.remove(file);
        kept.remove(&small_name(file));
        kept.live.remove(file);
    }
}

/// The pictures kept, each with when it was last asked for, and how many bytes they come to.
struct Kept {
    limit: usize,
    bytes: usize,
    clock: u64,
    pictures: HashMap<String, (Option<Picture>, u64)>,
    /// Files whose picture is shown in place of theirs (see [`show_picture`]).
    live: std::collections::HashSet<String>,
}

fn bytes_of(picture: Option<&Picture>) -> usize {
    picture.map_or(0, |p| (p.width() as usize) * (p.height() as usize) * 4)
}

impl Kept {
    fn new(limit: usize) -> Kept {
        Kept { limit, bytes: 0, clock: 0, pictures: HashMap::new(), live: std::collections::HashSet::new() }
    }

    /// The picture kept for a file (`Some(None)` for one known to be missing), marked as just used.
    fn get(&mut self, file: &str) -> Option<Option<Picture>> {
        self.clock += 1;
        let clock = self.clock;
        self.pictures.get_mut(file).map(|(picture, used)| {
            *used = clock;
            picture.clone()
        })
    }

    /// Keeps a picture, letting go of the ones used longest ago while there are too many bytes.
    /// The one just kept always stays, however big it is.
    fn insert(&mut self, file: &str, picture: Option<Picture>) {
        self.remove(file);
        self.clock += 1;
        self.bytes += bytes_of(picture.as_ref());
        self.pictures.insert(file.to_owned(), (picture, self.clock));
        while self.bytes > self.limit {
            let oldest = self
                .pictures
                .iter()
                .filter(|(name, (picture, _))| name.as_str() != file && picture.is_some())
                .min_by_key(|(_, (_, used))| *used)
                .map(|(name, _)| name.clone());
            let Some(oldest) = oldest else { break };
            self.remove(&oldest);
        }
    }

    fn remove(&mut self, file: &str) {
        if let Some((picture, _)) = self.pictures.remove(file) {
            self.bytes -= bytes_of(picture.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(side: u32) -> Option<Picture> {
        Picture::from_rgba(side, side, vec![0; (side * side * 4) as usize])
    }

    fn wide_picture(file: &str) -> Option<Picture> {
        file.starts_with("wide").then(|| Picture::from_rgba(1280, 720, vec![255; 1280 * 720 * 4])).flatten()
    }

    #[test]
    fn something_drawn_small_gets_a_small_copy_and_a_live_picture_comes_as_it_is() {
        set_picture_loader(wide_picture);
        let small = picture_at("wide-a", 300.0).unwrap();
        assert_eq!((small.width(), small.height()), (SMALL_SIDE, 360));
        assert_eq!(picture_at("wide-a", 1920.0).unwrap().width(), 1280, "drawn large, the whole picture");
        show_picture("wide-live", Picture::from_rgba(1280, 720, vec![0; 1280 * 720 * 4]).unwrap());
        assert_eq!(picture_at("wide-live", 300.0).unwrap().width(), 1280, "a video's frame is never a stale copy");
        forget_picture("wide-live");
        assert!(picture_at("missing", 300.0).is_none());
    }

    #[test]
    fn the_pictures_used_longest_ago_are_let_go_past_the_limit() {
        // Room for two 10×10 pictures (400 bytes each).
        let mut kept = Kept::new(800);
        kept.insert("a", square(10));
        kept.insert("b", square(10));
        kept.insert("missing", None);
        assert_eq!(kept.bytes, 800);
        // "a" is drawn again, so "b" is now the one used longest ago.
        assert!(kept.get("a").is_some_and(|p| p.is_some()));
        kept.insert("c", square(10));
        assert_eq!(kept.bytes, 800);
        assert!(kept.get("b").is_none(), "b was let go");
        assert!(kept.get("a").is_some() && kept.get("c").is_some());
        // A missing file is still remembered as missing; it takes no room.
        assert!(matches!(kept.get("missing"), Some(None)));
        // One picture bigger than the limit is kept on its own.
        kept.insert("huge", square(20));
        assert_eq!(kept.bytes, 1600);
        assert!(kept.get("huge").is_some() && kept.get("a").is_none() && kept.get("c").is_none());
        kept.remove("huge");
        assert_eq!(kept.bytes, 0);
    }
}
