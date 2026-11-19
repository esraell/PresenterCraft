//! Pictures that slide objects show, by the file they name. The app says how a file becomes a
//! picture; each is read once and kept, so every screen and thumbnail shares it.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use crate::Picture;

/// Reads the picture a file names, or `None` if it can't.
pub type Loader = fn(&str) -> Option<Picture>;

static LOADER: OnceLock<Loader> = OnceLock::new();
static KEPT: OnceLock<Mutex<HashMap<String, Option<Picture>>>> = OnceLock::new();

/// Says how a picture object's file is read. Set once, when the app starts.
pub fn set_picture_loader(loader: Loader) {
    let _ = LOADER.set(loader);
}

/// The picture a file names: read the first time it is asked for, then kept. A file that
/// can't be read is remembered as missing, so it is not tried again on every frame.
pub(crate) fn picture(file: &str) -> Option<Picture> {
    let kept = KEPT.get_or_init(Mutex::default);
    if let Some(found) = kept.lock().ok().and_then(|kept| kept.get(file).cloned()) {
        return found;
    }
    // Read without holding the lock, so another screen isn't kept waiting.
    let read = LOADER.get().and_then(|load| load(file));
    if let Ok(mut kept) = kept.lock() {
        kept.insert(file.to_owned(), read.clone());
    }
    read
}

/// Forgets a file's picture, so it is read again the next time it is drawn.
pub fn forget_picture(file: &str) {
    if let Some(Ok(mut kept)) = KEPT.get().map(Mutex::lock) {
        kept.remove(file);
    }
}
