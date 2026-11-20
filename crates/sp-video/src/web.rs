//! Web pages, drawn off-screen and copied into frames for the screens, as a video's are: a box
//! on a slide that shows a live web page, and (later) a VoxLive caption screen.
//!
//! A [`WebPage`] is made and driven on the main thread, like a video [`crate::Player`]; its
//! pictures are taken from a [`crate::Frames`], which can go to the thread drawing the screens.
//! Call [`WebPage::capture`] as often as new pictures are wanted (15 to 30 times a second); a
//! copy already on its way is not asked for again.
//!
//! What a page may do is kept small, because the address can come from anywhere (a song pack
//! made on another computer): only `https` pages (and plain `http` on this computer, for tests)
//! are loaded or followed, nothing is downloaded, no window opens, camera, microphone and other
//! permissions are refused, nothing is kept between pages (no cookies on disk), and nothing in
//! a page can call into the app.
//!
//! On macOS the system's web view (WebKit) draws the page in a window far off every screen.
//! Other systems have no web pages yet: [`WebPage::open`] says so.

use std::sync::{Arc, Mutex};

use crate::Frame;

/// The newest picture of a page, shared with the thread drawing the screens.
#[derive(Clone, Default)]
pub struct WebFrames(Arc<Mutex<Newest>>);

#[derive(Default)]
struct Newest {
    frame: Option<Frame>,
}

impl WebFrames {
    /// Keeps a new picture, in place of one not yet taken.
    pub(crate) fn put(&self, frame: Frame) {
        if let Ok(mut newest) = self.0.lock() {
            newest.frame = Some(frame);
        }
    }

    /// The newest picture, if one has come since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        self.0.lock().ok().and_then(|mut newest| newest.frame.take())
    }

    /// True when both take pictures from the same page.
    pub fn same(&self, other: &WebFrames) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// How far a page has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Loading {
    Loading,
    Loaded,
    /// It couldn't be loaded (no network, a bad address, a refused one).
    Failed,
}

/// True for an address a page may be loaded from or go to: `https`, or plain `http` on this
/// computer (127.0.0.1, localhost, [::1]). No name and password in it.
pub fn allowed(address: &str) -> bool {
    let Some((scheme, rest)) = address.split_once("://") else { return false };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.is_empty() || authority.contains('@') || address.chars().any(char::is_control) {
        return false;
    }
    match scheme.to_ascii_lowercase().as_str() {
        "https" => true,
        "http" => loopback(authority),
        _ => false,
    }
}

/// True for this computer's own address, with or without a port.
pub(crate) fn loopback(authority: &str) -> bool {
    let host = match authority.strip_prefix('[') {
        Some(v6) => v6.split(']').next().unwrap_or(""),
        None => authority.split(':').next().unwrap_or(""),
    };
    matches!(host.to_ascii_lowercase().as_str(), "127.0.0.1" | "localhost" | "::1")
}

/// A web page drawn off-screen at a size, in pixels.
pub struct WebPage {
    #[cfg(target_os = "macos")]
    page: crate::web_mac::Page,
    frames: WebFrames,
    address: String,
    size: (u32, u32),
}

impl WebPage {
    /// Opens a page at `address`, `width` × `height` pixels, with a see-through background
    /// where the page has none. On the main thread only. The error is a sentence for the
    /// operator.
    pub fn open(address: &str, width: u32, height: u32) -> Result<WebPage, String> {
        if !allowed(address) {
            return Err(format!("{address} isn't a web address SoftPresenter opens (it must start with https://)."));
        }
        let (width, height) = (width.clamp(64, 3840), height.clamp(64, 2160));
        let frames = WebFrames::default();
        #[cfg(target_os = "macos")]
        {
            let page = crate::web_mac::Page::open(address, width, height, frames.clone())?;
            Ok(WebPage { page, frames, address: address.to_owned(), size: (width, height) })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = frames;
            Err("Web pages on slides need macOS for now.".to_owned())
        }
    }

    /// Where the page's pictures are taken from.
    pub fn frames(&self) -> crate::Frames {
        crate::Frames::Web(self.frames.clone())
    }

    /// The address it was opened at.
    pub fn address(&self) -> &str {
        &self.address
    }

    /// The size it is drawn at, in pixels.
    pub fn size(&self) -> (u32, u32) {
        self.size
    }

    /// Asks for a new picture of the page, unless one is on its way.
    pub fn capture(&self) {
        #[cfg(target_os = "macos")]
        self.page.capture();
    }

    /// How far the page has got.
    pub fn loading(&self) -> Loading {
        #[cfg(target_os = "macos")]
        return self.page.loading();
        #[cfg(not(target_os = "macos"))]
        Loading::Failed
    }

    /// Runs a script in the page, for its answer as text (a script should end in a string,
    /// such as `JSON.stringify(…)`). `reply` is called on the main thread, with `None` when
    /// there was no text.
    pub fn ask(&self, script: &str, reply: impl FnOnce(Option<String>) + 'static) {
        #[cfg(target_os = "macos")]
        self.page.ask(script, reply);
        #[cfg(not(target_os = "macos"))]
        {
            let _ = script;
            reply(None);
        }
    }
}

/// Lets the main thread's run loop work for a while (loading pages, taking their pictures).
/// For checks outside the app; the app's own event loop does this for it.
pub fn pump(seconds: f64) {
    #[cfg(target_os = "macos")]
    crate::web_mac::pump(seconds);
    #[cfg(not(target_os = "macos"))]
    let _ = seconds;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_https_pages_and_this_computers_own_are_opened() {
        for good in [
            "https://example.com",
            "https://example.com/page?x=1#top",
            "HTTPS://Example.com:8443/a",
            "http://127.0.0.1:8000/page.html",
            "http://localhost/test",
            "http://[::1]:9000/",
        ] {
            assert!(allowed(good), "{good}");
        }
        for bad in [
            "http://example.com",
            "file:///etc/passwd",
            "javascript:alert(1)",
            "ftp://example.com",
            "https://",
            "https://user:pass@example.com",
            "https://example.com/\nmore",
            "example.com",
            "http://127.0.0.1.evil.com/",
            "http://localhost.evil.com/",
            "facetime://someone",
        ] {
            assert!(!allowed(bad), "{bad}");
        }
    }

    #[test]
    fn a_picture_not_yet_taken_gives_way_to_a_newer_one_and_is_taken_once() {
        let frames = WebFrames::default();
        assert!(frames.next().is_none());
        frames.put(Frame { width: 1, height: 1, rgba: vec![1, 2, 3, 4] });
        frames.put(Frame { width: 1, height: 1, rgba: vec![5, 6, 7, 8] });
        assert_eq!(frames.next().map(|frame| frame.rgba), Some(vec![5, 6, 7, 8]));
        assert!(frames.next().is_none(), "taken once");
        assert!(frames.same(&frames.clone()) && !frames.same(&WebFrames::default()));
    }
}
