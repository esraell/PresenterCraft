//! Sends a screen's picture over the network as an NDI® source.
//!
//! NDI® is a registered trademark of Vizrt NDI AB. This crate calls the NDI runtime installed
//! on the computer through the `grafton-ndi` library; it contains nothing from the NDI SDK.
//! See `docs/licences/ndi.md` for what the SDK's licence asks.
//!
//! The work happens on its own thread, so handing over a picture never waits on the network.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::thread::JoinHandle;
use std::time::Duration;

/// How often the current picture is sent again while nothing changes. A receiver that joins
/// late, or a mixer that expects a steady source, gets a picture within this time.
// Unused in a build without the SDK, which has nothing to send to.
#[cfg_attr(not(feature = "sdk"), allow(dead_code))]
const RESEND_EVERY: Duration = Duration::from_millis(100);

/// One picture to send: tightly packed RGBA rows.
#[cfg_attr(not(feature = "sdk"), allow(dead_code))]
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// A running NDI source. Dropping it stops the source.
pub struct NdiOutput {
    name: String,
    tx: Option<Sender<Picture>>,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// True when this build can send NDI at all.
    pub const AVAILABLE: bool = cfg!(feature = "sdk");

    /// Starts an NDI source with this name. The error is a plain sentence for the operator.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("The NDI source needs a name.".to_owned());
        }
        let (tx, rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let source = name.to_owned();
        let thread = std::thread::Builder::new()
            .name("ndi".to_owned())
            .spawn(move || run(&source, &rx, &ready_tx))
            .map_err(|e| format!("Could not start the NDI thread: {e}"))?;
        // The thread says whether the NDI runtime could be started before we report success.
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(NdiOutput { name: name.to_owned(), tx: Some(tx), thread: Some(thread) }),
            Ok(Err(reason)) => Err(reason),
            Err(_) => Err("NDI stopped before it could start.".to_owned()),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Hands over the picture to send from now on. Never blocks. A picture whose size and
    /// pixel count disagree is ignored.
    pub fn show(&self, width: u32, height: u32, rgba: Vec<u8>) {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        if let Some(tx) = &self.tx {
            let _ = tx.send(Picture { width, height, rgba });
        }
    }
}

impl Drop for NdiOutput {
    fn drop(&mut self) {
        // Closing the channel ends the thread's loop.
        self.tx = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Waits for the next picture, or for the time to send the current one again. Returns `None`
/// when the output has been dropped.
#[cfg_attr(not(feature = "sdk"), allow(dead_code))]
fn next_picture(rx: &Receiver<Picture>, current: Option<Picture>) -> Option<Option<Picture>> {
    match rx.recv_timeout(RESEND_EVERY) {
        Ok(mut newest) => {
            // If several queued up, only the last matters.
            while let Ok(later) = rx.try_recv() {
                newest = later;
            }
            Some(Some(newest))
        }
        Err(RecvTimeoutError::Timeout) => Some(current),
        Err(RecvTimeoutError::Disconnected) => None,
    }
}

#[cfg(feature = "sdk")]
fn run(name: &str, rx: &Receiver<Picture>, ready: &Sender<Result<(), String>>) {
    use grafton_ndi::{NDI, PixelFormat, Sender as NdiSender, SenderOptions, VideoFrame};

    let started = NDI::new().map_err(|e| format!("The NDI runtime could not be started: {e}")).and_then(|ndi| {
        let options = SenderOptions::builder(name).build();
        NdiSender::new(&ndi, &options)
            .map(|sender| (ndi, sender))
            .map_err(|e| format!("The NDI source could not be created: {e}"))
    });
    let (_ndi, sender) = match started {
        Ok(pair) => {
            let _ = ready.send(Ok(()));
            pair
        }
        Err(reason) => {
            let _ = ready.send(Err(reason));
            return;
        }
    };

    let mut current: Option<Picture> = None;
    let mut frame: Option<VideoFrame> = None;
    while let Some(next) = next_picture(rx, current.take()) {
        current = next;
        let Some(picture) = &current else { continue };
        let (w, h) = (picture.width as i32, picture.height as i32);
        // Keep one frame buffer and make a new one only when the size changes.
        if frame.as_ref().is_none_or(|f| f.width() != w || f.height() != h) {
            // The fourth byte is ignored: the picture is opaque.
            frame =
                VideoFrame::builder().resolution(w, h).pixel_format(PixelFormat::RGBX).frame_rate(30, 1).build().ok();
        }
        let Some(frame) = frame.as_mut() else { continue };
        let data = frame.data_mut();
        if data.len() == picture.rgba.len() {
            data.copy_from_slice(&picture.rgba);
            sender.send_video(frame);
        }
    }
}

#[cfg(not(feature = "sdk"))]
fn run(_name: &str, _rx: &Receiver<Picture>, ready: &Sender<Result<(), String>>) {
    let _ = ready.send(Err("This build of PresenterCraft does not include NDI.".to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_name_is_refused() {
        assert!(NdiOutput::start("   ").err().is_some_and(|e| e.contains("needs a name")));
    }

    #[cfg(not(feature = "sdk"))]
    #[test]
    fn a_build_without_the_sdk_says_so() {
        assert!(NdiOutput::start("Test").err().is_some_and(|e| e.contains("does not include NDI")));
    }

    #[test]
    fn the_newest_queued_picture_wins_and_a_quiet_spell_repeats_the_current_one() {
        let (tx, rx) = channel();
        for width in [1, 2, 3] {
            tx.send(Picture { width, height: 1, rgba: vec![0; width as usize * 4] }).unwrap();
        }
        let newest = next_picture(&rx, None).unwrap().unwrap();
        assert_eq!(newest.width, 3);
        // Nothing new: the current picture is the one to send again.
        let again = next_picture(&rx, Some(newest)).unwrap().unwrap();
        assert_eq!(again.width, 3);
        // The sender is gone: the loop should end.
        drop(tx);
        assert!(next_picture(&rx, Some(again)).is_none());
    }
}
