//! Sends a screen's picture over the network as an NDI® source.
//!
//! NDI® is a registered trademark of Vizrt NDI AB. The NDI runtime is not part of SoftPresenter
//! and is not needed to build it or to start it: it is looked for on the computer when the first
//! NDI source starts, on that source's own thread. Without it, the source says NDI isn't
//! installed and where to get it, and everything else carries on. See `docs/licences/ndi.md`.
//!
//! The work happens on each source's own thread, so handing over a picture never waits on NDI
//! or the network.
//!
//! # Sending only while someone is watching
//!
//! About once a second, each source asks NDI how many receivers are connected to it. While
//! there are none, it sends nothing and keeps no picture: the one it had is let go when the
//! last receiver leaves, and any handed over meanwhile is dropped. A source whose NDI runtime
//! can't count receivers keeps its picture and sends all the time, as before.
//!
//! A receiver that connects must never be shown a slide that has gone. Two things could show
//! it one, and neither does:
//!
//! - The source itself sends nothing from before the receiver came. The receiver gets the
//!   first picture handed over after it connected, and it is for whoever draws the pictures to
//!   hand one over then (see `start_with_wake` below).
//! - NDI keeps the last picture a source sent, and gives it to a receiver the moment it
//!   connects, before the source can know of it. While that picture is still what is on the
//!   screens, that is welcome: the receiver has its picture at once. When it no longer is
//!   (something changed, and nobody was watching to be sent it), the source is told to
//!   [`NdiOutput::forget`], and sends one black picture to take its place. A receiver that
//!   connects then sees black until its picture is drawn, within about a second.
//!
//! What the thread drawing the screens needs, none of which ever makes it wait:
//!
//! - [`NdiOutput::receivers`] (or [`Watch::receivers`]): `Some(0)` means nobody is watching, so
//!   the picture for that source need not be drawn or read back from the GPU at all. `None`
//!   means the count isn't known (the source is still starting, or its runtime can't count):
//!   draw for it as usual.
//! - [`NdiOutput::forget`]: said for a source nobody is watching whenever a picture that may
//!   differ from the last is not drawn for it.
//! - [`NdiOutput::start_with_wake`]: `wake` is called each time the count of receivers is
//!   found to have changed, the first time it is known included. It runs on the source's own
//!   thread, after the new count can be read, and should only pass the word on (send a message
//!   on a channel, say) and return. Whoever it wakes reads the count: when receivers have come
//!   to a source that had none, it draws a fresh picture and hands it over. A source started
//!   with [`NdiOutput::start`] has nobody to wake, so a receiver that connects to it waits for
//!   the next picture handed over.
//! - [`NdiOutput::show_shared`]: one picture for several sources, handed to each without
//!   copying it.
//!
//! A source counts again, there and then, when it is handed a picture or told to forget while
//! it believes nobody is watching: a receiver may have connected since the last count. If one
//! has, the picture is sent to it rather than dropped, and no black picture is sent over what
//! it is looking at.

#[allow(unsafe_code)]
mod runtime;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub use runtime::NOT_INSTALLED;

/// Where NDI can be downloaded, shown wherever NDI is chosen.
pub const DOWNLOAD_URL: &str = "https://ndi.video/";

/// How often the current picture is sent again while nothing changes. A receiver that joins
/// late, or a mixer that expects a steady source, gets a picture within this time. Each send is
/// compressed again by NDI, so this is kept slow: every 100 ms, two idle sources took about a
/// tenth of the computer's time; every second, about a fortieth.
const RESEND_EVERY: Duration = Duration::from_millis(1000);

/// How often a source asks NDI how many receivers it has. A receiver that connects to a source
/// nobody was watching gets its first picture within about this time.
const COUNT_EVERY: Duration = Duration::from_millis(1000);

/// One picture to send: tightly packed RGBA rows, which other sources may be sending too.
struct Picture {
    width: u32,
    height: u32,
    rgba: Arc<Vec<u8>>,
}

/// What a source's thread is told by whoever draws its pictures.
enum Told {
    /// The picture to send from now on.
    Picture(Picture),
    /// The picture last handed over is out of date, and no newer one is coming for now.
    Forget,
}

/// What a source is doing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    /// Looking for the NDI runtime and starting the source.
    Starting,
    /// On the network, sending whenever anyone is watching.
    Sending,
    /// It could not start, or stopped; the reason in plain words.
    Failed(String),
}

/// `Shared::receivers` when the count isn't known.
const UNKNOWN: u32 = u32::MAX;

/// What a source's thread tells everyone else.
#[derive(Debug)]
struct Shared {
    state: Mutex<State>,
    /// How many receivers were connected when last counted, or `UNKNOWN`.
    receivers: AtomicU32,
}

impl Shared {
    fn new() -> Shared {
        Shared { state: Mutex::new(State::Starting), receivers: AtomicU32::new(UNKNOWN) }
    }

    fn state(&self) -> State {
        self.state.lock().map_or_else(|_| State::Failed("NDI stopped.".to_owned()), |state| state.clone())
    }

    fn set_state(&self, now: State) {
        if let Ok(mut state) = self.state.lock() {
            *state = now;
        }
    }

    fn receivers(&self) -> Option<u32> {
        match self.receivers.load(Ordering::Relaxed) {
            UNKNOWN => None,
            count => Some(count),
        }
    }

    fn set_receivers(&self, count: Option<u32>) {
        self.receivers.store(count.map_or(UNKNOWN, |count| count.min(UNKNOWN - 1)), Ordering::Relaxed);
    }
}

/// What a source is doing, seen from anywhere.
#[derive(Clone, Debug)]
pub struct Watch(Arc<Shared>);

impl Watch {
    pub fn state(&self) -> State {
        self.0.state()
    }

    /// How many receivers are connected to the source, as last counted (about once a second).
    /// `None` while it is starting, once it has stopped, or when its NDI runtime can't count.
    /// Never waits.
    pub fn receivers(&self) -> Option<u32> {
        self.0.receivers()
    }
}

/// A running NDI source. Dropping it stops the source.
pub struct NdiOutput {
    name: String,
    shared: Arc<Shared>,
    tx: Option<Sender<Told>>,
    /// True from `forget` until the next picture is handed over: the source's thread is told
    /// once, however often it is said.
    forgotten: AtomicBool,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// Starts an NDI source with this name. Never waits for NDI: the source starts on its own
    /// thread, and `state` says how that went. The error is a plain sentence for the operator.
    /// Nobody is told when receivers connect: use `start_with_wake` to hand them a picture.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        NdiOutput::start_with_wake(name, || {})
    }

    /// Starts an NDI source as `start` does, which also calls `wake` each time the count of
    /// receivers is found to have changed, so that a picture can be drawn for receivers that
    /// connect after a spell with none (the source sends them nothing until one is handed
    /// over), and so that whoever shows the count can show the new one. `wake` runs on the source's own
    /// thread once the new count can be read from `receivers`, never on the caller's; it
    /// should only pass the word on and return.
    pub fn start_with_wake(name: &str, wake: impl FnMut() + Send + 'static) -> Result<NdiOutput, String> {
        spawn(name, |name| runtime::get().and_then(|runtime| runtime::Source::new(runtime, name)), wake)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// What the source is doing now.
    pub fn state(&self) -> State {
        self.shared.state()
    }

    /// How many receivers are connected, as [`Watch::receivers`] says. Never waits.
    pub fn receivers(&self) -> Option<u32> {
        self.shared.receivers()
    }

    /// A way to see what the source is doing from another thread.
    pub fn watch(&self) -> Watch {
        Watch(self.shared.clone())
    }

    /// Hands over the picture to send from now on. Never blocks, and never copies the pixels.
    /// A picture whose size and pixel count disagree is ignored, and so is one handed over
    /// while nobody is watching.
    pub fn show(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.show_shared(width, height, Arc::new(rgba));
    }

    /// Hands over the picture to send from now on, as `show` does, for a picture that several
    /// sources send: each is handed the same pixels, and none copies them. Never blocks. A
    /// picture whose size and pixel count disagree is ignored, and so is one handed over
    /// while nobody is watching.
    pub fn show_shared(&self, width: u32, height: u32, rgba: Arc<Vec<u8>>) {
        if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
            return;
        }
        self.forgotten.store(false, Ordering::Relaxed);
        if let Some(tx) = &self.tx {
            let _ = tx.send(Told::Picture(Picture { width, height, rgba }));
        }
    }

    /// Says that the picture last handed over is no longer what should be seen, and that no
    /// newer one is being handed over, because nobody is watching. NDI itself keeps the last
    /// picture a source sent and gives it to a receiver the moment it connects; after this, a
    /// receiver that connects is given a black picture instead, until one is handed over for
    /// it. Never blocks. Said again before the next picture is handed over, it does nothing,
    /// so it can be said for every picture that is not drawn.
    pub fn forget(&self) {
        if !self.forgotten.swap(true, Ordering::Relaxed)
            && let Some(tx) = &self.tx
        {
            let _ = tx.send(Told::Forget);
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

/// Where a source's pictures go: NDI, or a stand-in in the tests.
trait Sink {
    /// How many receivers are connected, or `None` when that can't be known. Never waits.
    fn receivers(&self) -> Option<u32>;
    /// Sends one picture; may wait until it is time for the next.
    fn send(&self, width: u32, height: u32, rgba: &[u8]);
}

impl Sink for runtime::Source {
    fn receivers(&self) -> Option<u32> {
        runtime::Source::receivers(self)
    }

    fn send(&self, width: u32, height: u32, rgba: &[u8]) {
        runtime::Source::send(self, width, height, rgba);
    }
}

/// Starts a source's thread, which opens where its pictures go with `open` and then sends.
fn spawn<S: Sink + 'static>(
    name: &str,
    open: impl FnOnce(&str) -> Result<S, String> + Send + 'static,
    mut wake: impl FnMut() + Send + 'static,
) -> Result<NdiOutput, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("The NDI source needs a name.".to_owned());
    }
    let (tx, rx) = channel();
    let shared = Arc::new(Shared::new());
    let (source, theirs) = (name.to_owned(), shared.clone());
    let thread = std::thread::Builder::new()
        .name("ndi".to_owned())
        .spawn(move || match open(&source) {
            Ok(sink) => {
                theirs.set_state(State::Sending);
                serve(&sink, &rx, &theirs, &mut wake);
                theirs.set_receivers(None);
            }
            Err(reason) => theirs.set_state(State::Failed(reason)),
        })
        .map_err(|e| format!("Could not start the NDI thread: {e}"))?;
    Ok(NdiOutput {
        name: name.to_owned(),
        shared,
        tx: Some(tx),
        forgotten: AtomicBool::new(false),
        thread: Some(thread),
    })
}

/// The newest of what has queued up, `first` included: only the last matters, a picture or
/// the word that there is none any more.
fn newest(rx: &Receiver<Told>, first: Told) -> Told {
    let mut newest = first;
    while let Ok(later) = rx.try_recv() {
        newest = later;
    }
    newest
}

/// Sends the pictures handed over to `sink` until the output is dropped: each new one at once
/// and the current one again every second, while anyone is watching. Calls `wake` whenever the
/// count of receivers has changed.
fn serve(sink: &impl Sink, rx: &Receiver<Told>, shared: &Shared, wake: &mut impl FnMut()) {
    let mut gate = Gate::new(Instant::now());
    // Counts the receivers, and tells everyone the count before `wake`, so that whoever is
    // woken reads the new one.
    let mut count = |gate: &mut Gate| {
        let receivers = sink.receivers();
        shared.set_receivers(receivers);
        if gate.counted(Instant::now(), receivers) {
            wake();
        }
    };
    loop {
        match rx.recv_timeout(gate.wait(Instant::now())) {
            Ok(first) => {
                let told = newest(rx, first);
                // Word for a source nobody was watching when last counted. Someone may have
                // come since: counted again now, a picture is sent to them rather than
                // dropped, and a black one is not sent over what they are looking at.
                if !gate.watched() {
                    count(&mut gate);
                }
                match told {
                    Told::Picture(picture) => gate.picture(picture),
                    Told::Forget => gate.forget(),
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        if gate.count_due(Instant::now()) {
            count(&mut gate);
        }
        if let Some((width, height)) = gate.blank_due() {
            sink.send(width, height, &vec![0; width as usize * height as usize * 4]);
        }
        if let Some(picture) = gate.picture_due(Instant::now()) {
            sink.send(picture.width, picture.height, &picture.rgba);
        }
    }
}

/// What a source's thread remembers between one wake and the next, and its rule for what to
/// send when. Told the time rather than reading the clock, so it can be tested on its own.
struct Gate {
    /// The newest picture; none while nobody is watching.
    current: Option<Picture>,
    /// How many receivers were connected when last counted; `None`, sent to as if someone
    /// were watching, when that isn't known.
    receivers: Option<u32>,
    /// The current picture is new, and still to be sent at once.
    fresh: bool,
    /// The size of the last picture sent, while NDI still has it to give to a receiver the
    /// moment one connects; `None` before any is sent and once a black one has taken its place.
    given: Option<(u32, u32)>,
    /// That picture is out of date: a newer one was dropped or never drawn, nobody watching.
    stale: bool,
    next_count: Instant,
    next_resend: Instant,
}

impl Gate {
    /// Receivers are counted straight away, before anything is sent.
    fn new(now: Instant) -> Gate {
        Gate {
            current: None,
            receivers: None,
            fresh: false,
            given: None,
            stale: false,
            next_count: now,
            next_resend: now,
        }
    }

    fn watched(&self) -> bool {
        self.receivers != Some(0)
    }

    /// How long the thread may wait for a new picture before it has something else to do.
    fn wait(&self, now: Instant) -> Duration {
        let next = if self.watched() && self.current.is_some() {
            self.next_count.min(self.next_resend)
        } else {
            self.next_count
        };
        next.saturating_duration_since(now)
    }

    /// A new picture: it takes the current one's place and goes at once. While nobody is
    /// watching it is dropped: by the time anyone watches, it may be a slide long gone. The
    /// one NDI was last sent is then out of date too.
    fn picture(&mut self, picture: Picture) {
        if self.watched() {
            self.current = Some(picture);
            self.fresh = true;
            self.stale = false;
        } else {
            self.stale = true;
        }
    }

    /// The picture last handed over is out of date, and no newer one is coming for now.
    fn forget(&mut self) {
        self.current = None;
        self.fresh = false;
        self.stale = true;
    }

    /// The size of the black picture to send now, if one is due: when nobody is watching and
    /// the last picture sent is out of date. NDI gives a receiver the last picture a source
    /// sent the moment it connects, before the source can know of it; this puts black in the
    /// place of a slide that has gone, once. While someone is watching none is due: they
    /// were given the picture when it was the newest, and a new one is on its way to them.
    /// Should they leave before it comes (it could not be drawn, say), black is due then.
    fn blank_due(&mut self) -> Option<(u32, u32)> {
        if !self.stale || self.watched() {
            return None;
        }
        self.stale = false;
        self.given.take()
    }

    /// Whether it is time to count the receivers again.
    fn count_due(&self, now: Instant) -> bool {
        now >= self.next_count
    }

    /// The receivers have been counted. True when the count has changed, which whoever draws
    /// the pictures is told. With nobody watching, the picture kept is let go, so receivers
    /// that come later are sent nothing from before they came: only what is handed over then.
    fn counted(&mut self, now: Instant, receivers: Option<u32>) -> bool {
        self.next_count = now + COUNT_EVERY;
        let changed = self.receivers != receivers;
        self.receivers = receivers;
        if !self.watched() {
            self.current = None;
            // A picture not sent yet goes unsent, so the one sent before it is out of date.
            self.stale |= std::mem::take(&mut self.fresh);
        }
        changed
    }

    /// The picture to send now, if any: a fresh one at once, the current one again when it is
    /// time, and nothing while nobody is watching.
    fn picture_due(&mut self, now: Instant) -> Option<&Picture> {
        if !self.watched() || !(self.fresh || now >= self.next_resend) {
            return None;
        }
        let picture = self.current.as_ref()?;
        self.fresh = false;
        self.next_resend = now + RESEND_EVERY;
        self.given = Some((picture.width, picture.height));
        Some(picture)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECOND: Duration = Duration::from_secs(1);
    /// The longest a test waits for a source's thread: far longer than it should take.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn picture(width: u32) -> Picture {
        Picture { width, height: 1, rgba: Arc::new(vec![9; width as usize * 4]) }
    }

    /// The width of the picture the gate sends now, which tells the tests' pictures apart.
    fn sent(gate: &mut Gate, now: Instant) -> Option<u32> {
        gate.picture_due(now).map(|picture| picture.width)
    }

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
    fn the_newest_queued_picture_wins() {
        let (tx, rx) = channel();
        for width in [2, 3] {
            tx.send(Told::Picture(picture(width))).unwrap();
        }
        assert!(matches!(newest(&rx, Told::Picture(picture(1))), Told::Picture(Picture { width: 3, .. })));
        assert!(rx.try_recv().is_err(), "the older ones are gone");
        // The word to forget, coming last, is what matters; a picture after it is.
        tx.send(Told::Forget).unwrap();
        assert!(matches!(newest(&rx, Told::Picture(picture(1))), Told::Forget));
        tx.send(Told::Picture(picture(4))).unwrap();
        assert!(matches!(newest(&rx, Told::Forget), Told::Picture(Picture { width: 4, .. })));
    }

    #[test]
    fn a_picture_gone_out_of_date_while_nobody_watches_is_covered_with_black_once() {
        let t = Instant::now();
        // NDI gives a receiver the last picture a source sent the moment it connects. While
        // that is still the newest, it is left alone.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t), Some(3));
        assert_eq!(gate.blank_due(), None);
        gate.counted(t + SECOND, Some(0));
        assert_eq!(gate.blank_due(), None, "the last receiver leaving puts nothing out of date");
        // The slide changes and is not drawn, nobody watching: black, the size of that picture.
        gate.forget();
        assert_eq!(gate.blank_due(), Some((3, 1)));
        gate.forget();
        assert_eq!(gate.blank_due(), None, "once: NDI has the black one now");
        // A receiver comes and is drawn a picture, leaves, and the slide changes again.
        gate.counted(t + 2 * SECOND, Some(1));
        gate.picture(picture(4));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(4));
        gate.counted(t + 3 * SECOND, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), Some((4, 1)));
        // A picture handed over for nobody is dropped, which puts the last one sent out of
        // date as well.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.counted(t + SECOND, Some(0));
        gate.picture(picture(5));
        assert_eq!(gate.blank_due(), Some((2, 1)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        // So does a picture handed over while someone watched that was still to be sent when
        // the last receiver was found gone.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.picture(picture(6));
        gate.counted(t + SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((2, 1)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        // A source that never sent a picture has nothing to cover.
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
    }

    #[test]
    fn black_is_never_sent_over_what_someone_is_watching() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.counted(t + SECOND, Some(0));
        // Told to forget, but counted again first (as the thread does) there is a receiver:
        // it was given that picture as it connected, and a new one is being drawn for it.
        assert!(gate.counted(t + SECOND + SECOND / 2, Some(1)), "wake whoever draws");
        gate.forget();
        assert_eq!(gate.blank_due(), None);
        assert_eq!(sent(&mut gate, t + 2 * SECOND), None, "and the picture forgotten is not sent again");
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(3));
        // Nor later, when that receiver leaves: what NDI has then is the newest.
        gate.counted(t + 3 * SECOND, Some(0));
        assert_eq!(gate.blank_due(), None);
        // Told to forget with a receiver there, and no new picture comes (it could not be
        // drawn, say) before the receiver leaves: black then, for whoever comes next.
        gate.counted(t + 4 * SECOND, Some(1));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
        gate.counted(t + 5 * SECOND, Some(0));
        assert_eq!(gate.blank_due(), Some((3, 1)));
        assert_eq!(gate.blank_due(), None);
        // With the count unknown, someone may always be watching.
        let mut gate = Gate::new(t);
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t), Some(2));
        gate.forget();
        assert_eq!(gate.blank_due(), None);
    }

    #[test]
    fn with_the_count_unknown_pictures_go_as_they_always_have() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.count_due(t), "counted before anything is sent");
        assert!(!gate.counted(t, None));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1), "a new picture goes at once");
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + SECOND), Some(1), "and again after a second");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 10), Some(2));
    }

    #[test]
    fn with_receivers_pictures_go_as_they_always_have() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.counted(t, Some(2)), "the first count known is told");
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + SECOND), Some(1));
        assert!(gate.counted(t + SECOND, Some(1)), "fewer receivers are told");
        assert!(!gate.counted(t + SECOND, Some(1)), "the same count again is not");
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 2), None, "and change nothing of what is sent");
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(1));
    }

    #[test]
    fn with_nobody_watching_nothing_goes_and_no_picture_is_kept() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(gate.counted(t, Some(0)));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), None);
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + 5 * SECOND), None, "nor is anything sent again");
        assert!(gate.current.is_none(), "a picture handed over for nobody is dropped");
    }

    #[test]
    fn a_receiver_connecting_is_sent_nothing_from_before_it_came_and_the_drawing_is_woken() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        // A picture sent while someone watched, and two handed over after they left.
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert!(gate.counted(t + SECOND, Some(0)));
        assert!(gate.current.is_none(), "the picture kept is let go with the last receiver");
        gate.picture(picture(2));
        gate.picture(picture(3));
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 2), None);
        // A receiver connects: whoever draws is woken, and none of those pictures goes, then
        // or when it would be time to send again.
        assert!(gate.count_due(t + 2 * SECOND));
        assert!(gate.counted(t + 2 * SECOND, Some(1)), "wake whoever draws");
        assert_eq!(sent(&mut gate, t + 2 * SECOND), None, "nothing from before it came");
        assert_eq!(sent(&mut gate, t + 4 * SECOND), None, "however long the fresh one takes");
        // The picture drawn for it goes at once, then as always.
        gate.picture(picture(4));
        assert_eq!(sent(&mut gate, t + 4 * SECOND), Some(4));
        assert_eq!(sent(&mut gate, t + 4 * SECOND + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + 5 * SECOND), Some(4));
        assert!(gate.counted(t + 5 * SECOND, Some(2)), "more receivers are told too");
        assert_eq!(sent(&mut gate, t + 6 * SECOND), Some(4), "and are sent the picture kept");
    }

    #[test]
    fn a_receiver_connecting_before_any_picture_wakes_the_drawing_and_sends_nothing() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        assert!(gate.counted(t + SECOND, Some(1)));
        assert_eq!(sent(&mut gate, t + SECOND), None);
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t + SECOND), Some(1), "the first picture goes when it comes");
    }

    #[test]
    fn the_count_becoming_unknown_after_none_sends_again() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.picture(picture(1));
        assert!(gate.counted(t + SECOND, None), "wake whoever draws");
        assert_eq!(sent(&mut gate, t + SECOND), None, "nothing from before");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND), Some(2));
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(2), "kept and sent again, as always");
        assert!(!gate.counted(t + 2 * SECOND, None), "unknown to unknown wakes nobody");
    }

    #[test]
    fn the_last_receiver_leaving_stops_the_sending() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert!(gate.counted(t + SECOND / 2, Some(0)), "told, so the drawing for it can stop");
        assert_eq!(sent(&mut gate, t + SECOND), None, "not sent again");
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND), None, "nor a new one");
    }

    #[test]
    fn pictures_coming_all_the_time_never_put_off_counting() {
        // A playing video hands over thirty pictures a second while nobody watches: the
        // receivers are still counted each second, and the thread never waits past that.
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        let mut counts = Vec::new();
        for frame in 1..=90 {
            let now = t + Duration::from_millis(33 * frame);
            gate.picture(picture(1));
            assert!(gate.wait(now) <= SECOND);
            if gate.count_due(now) {
                counts.push(now - t);
                gate.counted(now, Some(0));
            }
            assert_eq!(sent(&mut gate, now), None);
        }
        assert_eq!(counts.len(), 2, "counted at about one and two seconds: {counts:?}");
        assert!(counts.iter().zip([SECOND, 2 * SECOND]).all(|(at, due)| *at >= due && *at - due < SECOND / 10));
    }

    #[test]
    fn the_thread_waits_for_whichever_comes_first_of_counting_and_sending_again() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert_eq!(gate.wait(t), Duration::ZERO, "count at once");
        gate.counted(t, Some(0));
        gate.picture(picture(1));
        assert_eq!(gate.wait(t), SECOND, "nobody watching: only the next count");
        gate.counted(t + SECOND / 2, Some(1));
        assert_eq!(gate.wait(t + SECOND / 2), SECOND, "watched, with no picture yet: the next count");
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t + SECOND / 2), Some(1));
        assert_eq!(gate.wait(t + SECOND / 2), SECOND, "the next count and the next send coincide");
        gate.counted(t + SECOND, Some(1));
        assert_eq!(gate.wait(t + SECOND), SECOND / 2, "the next send comes first");
    }

    /// Stands in for NDI: counts as many receivers as the test says (`UNKNOWN` for a runtime
    /// that can't count), and says what it was sent: the pictures, and apart from them the
    /// black ones.
    struct Stand {
        receivers: Arc<AtomicU32>,
        sent: Sender<(u32, usize)>,
        black: Sender<u32>,
    }

    impl Sink for Stand {
        fn receivers(&self) -> Option<u32> {
            match self.receivers.load(Ordering::Relaxed) {
                UNKNOWN => None,
                count => Some(count),
            }
        }

        fn send(&self, width: u32, _height: u32, rgba: &[u8]) {
            if rgba.iter().all(|byte| *byte == 0) {
                let _ = self.black.send(width);
            } else {
                let _ = self.sent.send((width, rgba.as_ptr().addr()));
            }
        }
    }

    /// An output sending to a stand-in, with the count it gives and what it sent: each
    /// picture's width and where its pixels are, and each black picture's width.
    struct Standing {
        output: NdiOutput,
        receivers: Arc<AtomicU32>,
        sent: Receiver<(u32, usize)>,
        black: Receiver<u32>,
    }

    fn standing(receivers: u32, wake: impl FnMut() + Send + 'static) -> Standing {
        let receivers = Arc::new(AtomicU32::new(receivers));
        let ((sent_tx, sent), (black_tx, black)) = (channel(), channel());
        let theirs = receivers.clone();
        let open = move |_: &str| Ok(Stand { receivers: theirs, sent: sent_tx, black: black_tx });
        let output = spawn("SoftPresenter test", open, wake).unwrap();
        Standing { output, receivers, sent, black }
    }

    #[test]
    fn one_picture_shown_on_two_sources_is_sent_by_both_from_the_same_pixels() {
        let (one, two) = (standing(UNKNOWN, || {}), standing(1, || {}));
        let pixels = Arc::new(vec![9; 4 * 2 * 4]);
        let at = pixels.as_ptr().addr();
        one.output.show_shared(4, 2, pixels.clone());
        two.output.show_shared(4, 2, pixels.clone());
        assert_eq!(one.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
        assert_eq!(two.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
    }

    #[test]
    fn a_picture_shown_as_its_own_pixels_is_sent_without_a_copy() {
        let source = standing(UNKNOWN, || {});
        let pixels = vec![9; 2 * 2 * 4];
        let at = pixels.as_ptr().addr();
        source.output.show(2, 2, pixels);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap(), (2, at));
    }

    #[test]
    fn a_picture_whose_size_and_pixels_disagree_is_ignored() {
        let source = standing(UNKNOWN, || {});
        source.output.show_shared(3, 2, Arc::new(vec![9; 5]));
        source.output.show(0, 2, Vec::new());
        source.output.show(1, 1, vec![9; 4]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1, "only the good picture goes");
    }

    #[test]
    fn a_receiver_connecting_to_a_quiet_source_is_sent_only_what_is_drawn_for_it() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1), "woken for the first count, which can be read");
        source.output.show(1, 1, vec![9; 4]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        while source.sent.try_recv().is_ok() {}
        // A picture handed over for nobody is dropped; the black one says that it has been.
        source.output.show(2, 1, vec![9; 8]);
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 1);
        // How soon is the gate tests' to say; a busy computer may take longer here.
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1), "the new count is there for whoever is woken");
        // Whoever was woken hands over a fresh picture. Had either picture from before been
        // kept, the thread would have sent it as it woke us, ahead of this one.
        source.output.show(3, 1, vec![9; 12]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3, "the fresh picture, and nothing before it");
    }

    #[test]
    fn a_picture_handed_over_just_as_a_receiver_connects_is_sent_not_dropped() {
        let (woken_tx, woken) = channel();
        let source = standing(0, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        // A receiver connects, and before the source next counts it is handed a picture: it
        // counts there and then, and the picture goes.
        source.receivers.store(1, Ordering::Relaxed);
        source.output.show(2, 1, vec![9; 8]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        woken.recv_timeout(PATIENCE).unwrap();
        assert!(source.black.try_recv().is_err());
    }

    #[test]
    fn a_source_is_woken_when_its_last_receiver_leaves_and_when_more_come() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(1, 1, vec![9; 4]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1);
        for receivers in [2, 0] {
            source.receivers.store(receivers, Ordering::Relaxed);
            woken.recv_timeout(PATIENCE).unwrap();
            assert_eq!(source.output.receivers(), Some(receivers));
        }
        // Woken for the count of none, the picture it had is gone: nothing more of it is sent,
        // to nobody or to whoever comes next.
        while source.sent.try_recv().is_ok() {}
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2, "only the picture drawn for the newcomer");
    }

    #[test]
    fn a_source_told_to_forget_sends_black_once_and_a_receiver_that_comes_gets_only_a_fresh_picture() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        assert!(source.black.try_recv().is_err(), "the picture NDI has is still the newest");
        // The slide changes with nobody watching, three times: said each time, done once.
        for _ in 0..3 {
            source.output.forget();
        }
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 2, "black, the size of the picture it covers");
        while source.sent.try_recv().is_ok() {}
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(3, 1, vec![9; 12]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3);
        assert!(source.black.try_recv().is_err(), "black went once");
        // After a picture, forgetting is said and done anew.
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.forget();
        assert_eq!(source.black.recv_timeout(PATIENCE).unwrap(), 3);
    }

    #[test]
    fn told_to_forget_just_as_a_receiver_connects_a_source_counts_again_and_sends_no_black() {
        let (woken_tx, woken) = channel();
        let source = standing(1, move || {
            let _ = woken_tx.send(());
        });
        woken.recv_timeout(PATIENCE).unwrap();
        source.output.show(2, 1, vec![9; 8]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2);
        source.receivers.store(0, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        // A receiver connects, and before the source next counts it is told to forget: it
        // counts there and then, wakes whoever draws, and sends no black picture to the
        // receiver, now or after.
        source.receivers.store(1, Ordering::Relaxed);
        source.output.forget();
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1));
        source.output.show(3, 1, vec![9; 12]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 3, "the fresh picture, and not the old one");
        assert!(source.black.try_recv().is_err());
    }
}
