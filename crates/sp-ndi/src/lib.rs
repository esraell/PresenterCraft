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
//! there are none, it sends nothing and keeps only the newest picture it was given. When one
//! connects, that picture goes at once, so the receiver has a picture within about a second. A
//! source whose NDI runtime can't count receivers sends all the time, as before.
//!
//! What the thread drawing the screens needs, none of which ever makes it wait:
//!
//! - [`NdiOutput::receivers`] (or [`Watch::receivers`]): `Some(0)` means nobody is watching, so
//!   the picture for that source need not be drawn or read back from the GPU at all. `None`
//!   means the count isn't known (the source is still starting, or its runtime can't count):
//!   draw for it as usual.
//! - [`NdiOutput::start_with_wake`]: `wake` is called when receivers connect to a source that
//!   had none. It runs on the source's own thread, after the new count can be read, and should
//!   only pass the word on (send a message on a channel, say) and return. Whoever it wakes then
//!   draws a fresh picture and hands it over. Until that arrives, the receiver sees the newest
//!   picture the source kept: if nothing was handed over while nobody watched, that is an older
//!   one, the slide on screen when the last receiver left.
//! - [`NdiOutput::show_shared`]: one picture for several sources, handed to each without
//!   copying it.

#[allow(unsafe_code)]
mod runtime;

use std::sync::atomic::{AtomicU32, Ordering};
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
    tx: Option<Sender<Picture>>,
    thread: Option<JoinHandle<()>>,
}

impl NdiOutput {
    /// Starts an NDI source with this name. Never waits for NDI: the source starts on its own
    /// thread, and `state` says how that went. The error is a plain sentence for the operator.
    pub fn start(name: &str) -> Result<NdiOutput, String> {
        NdiOutput::start_with_wake(name, || {})
    }

    /// Starts an NDI source as `start` does, which also calls `wake` each time receivers
    /// connect after a spell with none, so a picture can be drawn for them. `wake` runs on the
    /// source's own thread once the new count can be read from `receivers`, never on the
    /// caller's; it should only pass the word on and return.
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
    /// A picture whose size and pixel count disagree is ignored.
    pub fn show(&self, width: u32, height: u32, rgba: Vec<u8>) {
        self.show_shared(width, height, Arc::new(rgba));
    }

    /// Hands over the picture to send from now on, as `show` does, for a picture that several
    /// sources send: each is handed the same pixels, and none copies them. Never blocks. A
    /// picture whose size and pixel count disagree is ignored.
    pub fn show_shared(&self, width: u32, height: u32, rgba: Arc<Vec<u8>>) {
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
    Ok(NdiOutput { name: name.to_owned(), shared, tx: Some(tx), thread: Some(thread) })
}

/// The newest of the pictures that have queued up, `first` among them: only the last matters.
fn newest(rx: &Receiver<Picture>, first: Picture) -> Picture {
    let mut newest = first;
    while let Ok(later) = rx.try_recv() {
        newest = later;
    }
    newest
}

/// Sends the pictures handed over to `sink` until the output is dropped: each new one at once
/// and the current one again every second, while anyone is watching.
fn serve(sink: &impl Sink, rx: &Receiver<Picture>, shared: &Shared, wake: &mut impl FnMut()) {
    let mut gate = Gate::new(Instant::now());
    loop {
        match rx.recv_timeout(gate.wait(Instant::now())) {
            Ok(first) => gate.picture(newest(rx, first)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        if gate.count_due(now) {
            let receivers = sink.receivers();
            // Told before `wake`, so whoever is woken reads the new count.
            shared.set_receivers(receivers);
            if gate.counted(now, receivers) {
                wake();
            }
        }
        if let Some(picture) = gate.picture_due(now) {
            sink.send(picture.width, picture.height, &picture.rgba);
        }
    }
}

/// What a source's thread remembers between one wake and the next, and its rule for what to
/// send when. Told the time rather than reading the clock, so it can be tested on its own.
struct Gate {
    /// The newest picture, kept while nobody is watching.
    current: Option<Picture>,
    /// How many receivers were connected when last counted; `None`, sent to as if someone
    /// were watching, when that isn't known.
    receivers: Option<u32>,
    /// The current picture is still to be sent at once: it is new, or receivers have just
    /// come back.
    fresh: bool,
    next_count: Instant,
    next_resend: Instant,
}

impl Gate {
    /// Receivers are counted straight away, before anything is sent.
    fn new(now: Instant) -> Gate {
        Gate { current: None, receivers: None, fresh: false, next_count: now, next_resend: now }
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

    /// A new picture: it takes the current one's place, and goes as soon as anyone watches.
    fn picture(&mut self, picture: Picture) {
        self.current = Some(picture);
        self.fresh = true;
    }

    /// Whether it is time to count the receivers again.
    fn count_due(&self, now: Instant) -> bool {
        now >= self.next_count
    }

    /// The receivers have been counted. True when they have come back after a spell with none:
    /// the newest picture then goes at once, and whoever draws the pictures is told, so it can
    /// draw a fresh one.
    fn counted(&mut self, now: Instant, receivers: Option<u32>) -> bool {
        self.next_count = now + COUNT_EVERY;
        let back = self.receivers == Some(0) && receivers != Some(0);
        self.receivers = receivers;
        if back {
            self.fresh = true;
        }
        back
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
        Picture { width, height: 1, rgba: Arc::new(vec![0; width as usize * 4]) }
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
            tx.send(picture(width)).unwrap();
        }
        assert_eq!(newest(&rx, picture(1)).width, 3);
        assert!(rx.try_recv().is_err(), "the older ones are gone");
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
        assert!(!gate.counted(t, Some(2)));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + SECOND), Some(1));
        assert!(!gate.counted(t + SECOND, Some(1)), "fewer receivers, but some, change nothing");
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(1));
    }

    #[test]
    fn with_nobody_watching_nothing_goes_and_the_newest_picture_is_kept() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        assert!(!gate.counted(t, Some(0)));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), None);
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + 5 * SECOND), None, "nor is anything sent again");
        assert_eq!(gate.current.as_ref().map(|kept| kept.width), Some(2));
    }

    #[test]
    fn a_receiver_connecting_gets_the_newest_picture_at_once_and_the_drawing_is_woken() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(0));
        gate.picture(picture(1));
        gate.picture(picture(2));
        assert_eq!(sent(&mut gate, t + SECOND / 2), None);
        assert!(gate.count_due(t + SECOND));
        assert!(gate.counted(t + SECOND, Some(1)), "wake whoever draws");
        assert_eq!(sent(&mut gate, t + SECOND), Some(2), "the newest picture, at once");
        assert_eq!(sent(&mut gate, t + SECOND + SECOND / 2), None);
        assert_eq!(sent(&mut gate, t + 2 * SECOND), Some(2), "then as always");
        assert!(!gate.counted(t + 2 * SECOND, Some(2)), "more receivers wake nobody");
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
        assert!(gate.counted(t + SECOND, None));
        assert_eq!(sent(&mut gate, t + SECOND), Some(1));
        assert!(!gate.counted(t + 2 * SECOND, None), "unknown to unknown wakes nobody");
    }

    #[test]
    fn the_last_receiver_leaving_stops_the_sending() {
        let t = Instant::now();
        let mut gate = Gate::new(t);
        gate.counted(t, Some(1));
        gate.picture(picture(1));
        assert_eq!(sent(&mut gate, t), Some(1));
        assert!(!gate.counted(t + SECOND / 2, Some(0)));
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
        assert_eq!(sent(&mut gate, t + SECOND / 2), Some(1));
        assert_eq!(gate.wait(t + SECOND / 2), SECOND, "the next count and the next send coincide");
        gate.counted(t + SECOND, Some(1));
        assert_eq!(gate.wait(t + SECOND), SECOND / 2, "the next send comes first");
    }

    /// Stands in for NDI: counts as many receivers as the test says (`UNKNOWN` for a runtime
    /// that can't count), and says what it was sent and when it was asked for the count.
    struct Stand {
        receivers: Arc<AtomicU32>,
        counted: Sender<()>,
        sent: Sender<(u32, usize)>,
    }

    impl Sink for Stand {
        fn receivers(&self) -> Option<u32> {
            let _ = self.counted.send(());
            match self.receivers.load(Ordering::Relaxed) {
                UNKNOWN => None,
                count => Some(count),
            }
        }

        fn send(&self, width: u32, _height: u32, rgba: &[u8]) {
            let _ = self.sent.send((width, rgba.as_ptr().addr()));
        }
    }

    /// An output sending to a stand-in, with the count it gives, its word that it counted, and
    /// what it sent: each picture's width and where its pixels are.
    struct Standing {
        output: NdiOutput,
        receivers: Arc<AtomicU32>,
        counted: Receiver<()>,
        sent: Receiver<(u32, usize)>,
    }

    fn standing(receivers: u32, wake: impl FnMut() + Send + 'static) -> Standing {
        let receivers = Arc::new(AtomicU32::new(receivers));
        let ((counted_tx, counted), (sent_tx, sent)) = (channel(), channel());
        let theirs = receivers.clone();
        let open = move |_: &str| Ok(Stand { receivers: theirs, counted: counted_tx, sent: sent_tx });
        let output = spawn("SoftPresenter test", open, wake).unwrap();
        Standing { output, receivers, counted, sent }
    }

    #[test]
    fn one_picture_shown_on_two_sources_is_sent_by_both_from_the_same_pixels() {
        let (one, two) = (standing(UNKNOWN, || {}), standing(1, || {}));
        let pixels = Arc::new(vec![7; 4 * 2 * 4]);
        let at = pixels.as_ptr().addr();
        one.output.show_shared(4, 2, pixels.clone());
        two.output.show_shared(4, 2, pixels.clone());
        assert_eq!(one.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
        assert_eq!(two.sent.recv_timeout(PATIENCE).unwrap(), (4, at));
    }

    #[test]
    fn a_picture_shown_as_its_own_pixels_is_sent_without_a_copy() {
        let source = standing(UNKNOWN, || {});
        let pixels = vec![1; 2 * 2 * 4];
        let at = pixels.as_ptr().addr();
        source.output.show(2, 2, pixels);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap(), (2, at));
    }

    #[test]
    fn a_picture_whose_size_and_pixels_disagree_is_ignored() {
        let source = standing(UNKNOWN, || {});
        source.output.show_shared(3, 2, Arc::new(vec![0; 5]));
        source.output.show(0, 2, Vec::new());
        source.output.show(1, 1, vec![0; 4]);
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 1, "only the good picture goes");
    }

    #[test]
    fn a_receiver_connecting_to_a_quiet_source_gets_the_newest_picture_within_about_a_second() {
        let (woken_tx, woken) = channel();
        let source = standing(0, move || {
            let _ = woken_tx.send(());
        });
        source.counted.recv_timeout(PATIENCE).unwrap();
        let until = Instant::now() + PATIENCE;
        while source.output.receivers() != Some(0) {
            assert!(Instant::now() < until, "the count of none is told");
            std::thread::yield_now();
        }
        source.output.show(1, 1, vec![0; 4]);
        source.output.show(2, 1, vec![0; 8]);
        // How soon is the gate tests' to say; a busy computer may take longer here.
        source.receivers.store(1, Ordering::Relaxed);
        woken.recv_timeout(PATIENCE).unwrap();
        assert_eq!(source.output.receivers(), Some(1), "the new count is there for whoever is woken");
        assert_eq!(source.sent.recv_timeout(PATIENCE).unwrap().0, 2, "the newest picture, and nothing before it");
    }
}
