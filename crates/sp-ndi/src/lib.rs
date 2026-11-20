//! Sends a screen's picture over the network as an NDI® source.
//!
//! NDI® is a registered trademark of Vizrt NDI AB. The NDI runtime is not part of SoftPresenter
//! and is not needed to build it or to start it: it is looked for on the computer when the first
//! NDI source starts, on that source's own thread. Without it, the source says NDI isn't
//! installed and where to get it, and everything else carries on. See `docs/licences/ndi.md`.
//!
//! The work happens on each source's own thread, so handing over a picture never waits on NDI
//! or the network.

#[allow(unsafe_code)]
mod runtime;

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

pub use runtime::NOT_INSTALLED;

/// Where NDI can be downloaded, shown wherever NDI is chosen.
pub const DOWNLOAD_URL: &str = "https://ndi.video/";

/// How often the current picture is sent again while nothing changes. A receiver that joins
/// late, or a mixer that expects a steady source, gets a picture within this time. Each send is
/// compressed again by NDI, so this is kept slow: every 100 ms, two idle sources took about a
/// tenth of the computer's time; every second, about a fortieth.
const RESEND_EVERY: Duration = Duration::from_millis(1000);

/// One picture to send: tightly packed RGBA rows.
struct Picture {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

/// What a source is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Looking for the NDI runtime and starting the source.
    Starting,
    Sending,
    /// It could not start, or stopped; the reason in plain words.
    Failed(String),
}

/// What a source is doing, seen from anywhere.
#[derive(Clone, Debug)]
pub struct Watch(Arc<Mutex<State>>);

impl Watch {
    pub fn state(&self) -> State {
        self.0.lock().map_or_else(|_| State::Failed("NDI stopped.".to_owned()), |state| state.clone())
    }
}

/// A running NDI source. Dropping it stops the source.
pub struct NdiOutput {
    name: String,
    state: Arc<Mutex<State>>,
    tx: Option<Sender<Picture>>,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// Starts an NDI source with this name. Never waits for NDI: the source starts on its own
    /// thread, and `state` says how that went. The error is a plain sentence for the operator.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("The NDI source needs a name.".to_owned());
        }
        let (tx, rx) = channel();
        let state = Arc::new(Mutex::new(State::Starting));
        let (source, shared) = (name.to_owned(), state.clone());
        let thread = std::thread::Builder::new()
            .name("ndi".to_owned())
            .spawn(move || run(&source, &rx, &shared))
            .map_err(|e| format!("Could not start the NDI thread: {e}"))?;
        Ok(NdiOutput { name: name.to_owned(), state, tx: Some(tx), thread: Some(thread) })
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the source is doing now.
    pub fn state(&self) -> State {
        self.watch().state()
    }

    /// A way to see what the source is doing from another thread.
    pub fn watch(&self) -> Watch {
        Watch(self.state.clone())
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
        // Closing the channel ends the thread's loop, and it takes its source down on its own.
        // It isn't waited for: it may be part-way through sending a picture (paced to the
        // frame rate) or still starting NDI, and the thread dropping this is the one drawing
        // the screens, which must never wait on NDI.
        self.tx = None;
        drop(self.thread.take());
    }
}

/// Whether the NDI runtime is on this computer. Only looks for its file (a few quick checks),
/// so it can be asked from anywhere.
pub fn installed() -> bool {
    runtime::locate().is_some()
}

/// Waits for the next picture, or for the time to send the current one again. Returns `None`
/// when the output has been dropped.
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

fn run(name: &str, rx: &Receiver<Picture>, state: &Mutex<State>) {
    let set = |now: State| {
        if let Ok(mut state) = state.lock() {
            *state = now;
        }
    };
    let source = match runtime::get().and_then(|runtime| runtime::Source::new(runtime, name)) {
        Ok(source) => source,
        Err(reason) => return set(State::Failed(reason)),
    };
    set(State::Sending);
    let mut current: Option<Picture> = None;
    while let Some(next) = next_picture(rx, current.take()) {
        current = next;
        if let Some(picture) = current.as_mut() {
            source.send(picture.width, picture.height, &mut picture.rgba);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_name_is_refused() {
        assert!(NdiOutput::start("   ").err().is_some_and(|e| e.contains("needs a name")));
    }

    #[test]
    fn starting_never_waits_for_ndi() {
        let started = std::time::Instant::now();
        let output = NdiOutput::start("SoftPresenter test").unwrap();
        assert!(started.elapsed() < Duration::from_millis(50), "start returns at once");
        drop(output);
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
