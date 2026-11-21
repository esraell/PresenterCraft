//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use sp_model::{Slide, Theme, Uuid};
use sp_ndi::NdiOutput;
use sp_render::{
    Fonts, Layers, MovingPicture, Painter, Picture, build_layers, read_pixels, target_texture, vello, wgpu,
};

/// How often a playing video is looked at for a new picture: more often than any video's
/// frame rate.
const VIDEO_POLL: Duration = Duration::from_millis(8);
/// How often a video that has given no new picture for a second (paused, or ended and left on
/// screen) is looked at: a picture after it starts again is at most this late.
const VIDEO_IDLE_POLL: Duration = Duration::from_millis(50);

/// Everything the output needs to draw one picture. Immutable once sent.
#[derive(Clone)]
pub struct Frame {
    /// The picture behind the words, if any. For a video, a still from it, shown until its
    /// first picture comes.
    pub media: Option<Picture>,
    /// The video playing behind the words, where its pictures are taken from.
    pub video: Option<sp_video::Frames>,
    /// How the media fills the screen.
    pub media_fit: sp_model::ImageFit,
    /// A video in a box on the slide, by the file the box names, and where its pictures come
    /// from: each new one is shown in the box in place of its still.
    /// The videos in boxes on the slide, by their files, each with where its pictures come from.
    pub slide_video: Vec<(String, sp_video::Frames)>,
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// When false the output is black whatever the slide is.
    pub enabled: bool,
    /// How the picture before gives way to this one.
    pub transition: sp_model::Transition,
    /// How the media before gives way to this, when only the media changed.
    pub media_transition: sp_model::Transition,
    /// The slide after the one on screen, for stage screens.
    pub next: Option<Arc<Slide>>,
    /// The song on screen's title and the slide's group, for stage screens.
    pub about: crate::stage::About,
    /// How many of the slide's click builds have been set going.
    pub build_step: usize,
}

impl Frame {
    /// True while a video plays, as the media or in a box on the slide.
    fn plays_video(&self) -> bool {
        self.video.is_some() || !self.slide_video.is_empty()
    }
}

/// What is shown over the slides: a message on the audience screens; the stage message and the
/// timers on the stage screens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// The message's words, tokens and all (they are filled in as it is drawn), and its look.
    pub message: Option<(String, crate::messages::Look)>,
    pub stage_message: Option<String>,
    pub timers: Vec<crate::timers::Timer>,
    /// The props on screen, in the order they were put on.
    pub props: Vec<Slide>,
    /// The copyright line of the song on screen, when it is to be shown.
    pub copyright: Option<String>,
    /// A test pattern shown on the audience screens in place of everything else.
    pub test_pattern: Option<crate::props::TestPattern>,
}

pub enum OutputMsg {
    Show(Frame),
    /// A window to draw a screen in. The thread takes ownership of its surface.
    AddSurface {
        id: Uuid,
        surface: wgpu::Surface<'static>,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
    },
    /// A screen's window changed size, in pixels.
    Resized(Uuid, u32, u32),
    /// The textures the live preview is drawn into, for the window to show while a video plays.
    PreviewTargets(Vec<wgpu::Texture>),
    /// Draw the current frame again: a window was uncovered or asked to be redrawn.
    Redraw,
    /// Send a screen over NDI under this name, or stop with `None`. Sent again with another
    /// name, it is renamed.
    SetNdi {
        id: Uuid,
        name: Option<String>,
    },
    /// Stop drawing in a screen's window.
    RemoveSurface(Uuid),
    /// What the Announcements layer shows now, or nothing.
    Announcement(Option<Announcement>),
    /// What is shown over the slides.
    Overlay(Overlay),
    /// How each audience screen that has a look of its own shows the show.
    Looks(Vec<(Uuid, crate::screens::Look)>),
    /// The audience screens drawn with a theme of their own, and the theme.
    Themes(Vec<(Uuid, Arc<Theme>)>),
    /// Which screens are stage screens and the layout each is drawn with, and whether stage
    /// screens are on.
    Stage {
        screens: Vec<(Uuid, Arc<Theme>)>,
        on: bool,
    },
    /// Show each screen's own name on it, so the operator can tell them apart; an empty list
    /// goes back to the show.
    Identify(Vec<(Uuid, String, sp_model::Rgba)>),
    Shutdown,
}

/// What an NDI output is doing, for the operator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NdiStatus {
    #[default]
    Off,
    /// Sending under this name.
    On(String),
    /// Asked for, but it could not start; the reason in plain words.
    Failed(String),
}

/// An NDI picture being copied back from the GPU: the sources it is for, the buffer it is
/// copied into (each row padded as the GPU requires), and whether the copy has come back (and
/// worked).
struct NdiReadback {
    ids: Vec<Uuid>,
    buffer: wgpu::Buffer,
    padded_row: usize,
    done: Arc<Mutex<Option<bool>>>,
}

/// The size of the picture sent over NDI, whatever size any window is.
/// How soon the idle output thread looks at the GPU again while it still holds memory the
/// painter gave back.
const GPU_LOOK_AGAIN: Duration = Duration::from_millis(20);

/// A picture the GPU was too slow to work out is drawn again this long after, and up to this
/// many times for one thing the operator asked for: about a second in all, with the picture
/// before it on the screens meanwhile.
const DRAW_AGAIN_AFTER: Duration = Duration::from_millis(150);
const DRAW_AGAIN_TRIES: u8 = 5;

const NDI_SIZE: (u32, u32) = (1920, 1080);

/// Each screen's NDI source as the window sees it: started (by its name, and how it is doing
/// now), or refused before it could start.
#[derive(Clone)]
enum NdiEntry {
    Started(String, sp_ndi::Watch),
    Refused(String),
}

type NdiStatuses = Arc<Mutex<Vec<(Uuid, NdiEntry)>>>;

/// The operator's Program preview is what the screens show: the output thread draws the
/// audience picture a second time, small, into one of two textures the window shows, turn about: which one was drawn last, and how many
/// have been drawn (so the window knows when to show a new one). Nothing is read back from the
/// GPU, and the window only shows what is drawn.
#[derive(Default)]
pub struct LivePreview {
    front: std::sync::atomic::AtomicUsize,
    drawn: std::sync::atomic::AtomicU64,
    /// True while the window has been woken for a new picture and hasn't shown it yet, so a
    /// burst of pictures wakes it once.
    wake_pending: std::sync::atomic::AtomicBool,
}

/// Wakes the operator's window (from the output thread) to show a new live preview.
pub type Waker = Arc<dyn Fn() + Send + Sync>;

impl LivePreview {
    /// Which texture holds the newest picture, and how many have been drawn.
    pub fn newest(&self) -> (usize, u64) {
        use std::sync::atomic::Ordering::Acquire;
        (self.front.load(Acquire), self.drawn.load(Acquire))
    }
}

/// The size of the live preview, and how often a playing video's pictures are drawn in it at
/// most: about thirty a second, so a 24 or 30 fps video shows every picture and a 60 fps one
/// every other (every change of slide or media is drawn at once, and fades as the screens do).
/// Measured in quiet runs (release build, a 30 fps 720p video, two window screens; ten-second
/// averages, three runs each): about 23% CPU for the whole app at this cap, 21% at a 50 ms cap,
/// which shows only every other picture of a 30 fps video.
pub const LIVE_PREVIEW_SIZE: (u32, u32) = (640, 360);
const LIVE_PREVIEW_EVERY: Duration = Duration::from_millis(30);

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
    ndi_status: NdiStatuses,
    live_preview: Arc<LivePreview>,
}

impl OutputHandle {
    /// Starts the thread. It shows nothing until it is given a window or an NDI name.
    pub fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        theme: Arc<Theme>,
        waker: Option<Waker>,
    ) -> Result<OutputHandle, String> {
        let (tx, rx) = channel();
        let ndi_status: NdiStatuses = Arc::default();
        let status = ndi_status.clone();
        let live_preview: Arc<LivePreview> = Arc::default();
        let shared = live_preview.clone();
        // A picture read for the screens meanwhile has them drawn again once it is there.
        let wake = tx.clone();
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || {
                sp_render::draw_kept_pictures_only(move || {
                    let _ = wake.send(OutputMsg::Redraw);
                });
                run(device, queue, theme, rx, status, shared, waker)
            })
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread), ndi_status, live_preview })
    }

    /// Which live preview texture holds the newest picture, and how many have been drawn.
    pub fn live_preview(&self) -> (usize, u64) {
        self.live_preview.newest()
    }

    /// The window has woken for the newest live preview: the next new one wakes it again.
    pub fn live_preview_woken(&self) {
        self.live_preview.wake_pending.store(false, std::sync::atomic::Ordering::Release);
    }

    /// Never blocks. Returns false when the thread has ended and the message went nowhere.
    pub fn send(&self, msg: OutputMsg) -> bool {
        self.tx.send(msg).is_ok()
    }

    /// What a screen's NDI output is doing now. `Off` while it is still starting.
    pub fn ndi_status(&self, screen: Uuid) -> NdiStatus {
        let entry = self
            .ndi_status
            .lock()
            .ok()
            .and_then(|all| all.iter().find(|(id, _)| *id == screen).map(|(_, entry)| entry.clone()));
        match entry {
            None => NdiStatus::Off,
            Some(NdiEntry::Refused(reason)) => NdiStatus::Failed(reason),
            Some(NdiEntry::Started(name, watch)) => match watch.state() {
                sp_ndi::State::Starting => NdiStatus::Off,
                sp_ndi::State::Sending => NdiStatus::On(name),
                sp_ndi::State::Failed(reason) => NdiStatus::Failed(reason),
            },
        }
    }

    /// True once the thread has stopped, which it only does on shutdown or a fatal error.
    pub fn is_finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Stops the thread and waits for it.
    pub fn shutdown(mut self) {
        let _ = self.tx.send(OutputMsg::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// What a screen shows: the picture behind, the slide, words over them, and the theme.
type Content = (Option<Picture>, Option<Arc<Slide>>, Option<Arc<Slide>>, Arc<Theme>, Option<Announcement>);

/// What the Announcements layer shows: a slide and the theme it is drawn with.
pub type Announcement = (Arc<Slide>, Arc<Theme>);

/// A window a screen is drawn in.
struct Target {
    id: Uuid,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    texture: wgpu::Texture,
    /// The picture now in the window in full, kept so the next one can fade in over it.
    shown: Option<vello::Scene>,
    /// A fade under way: the picture it started from and the one it ends at.
    fade: Option<(vello::Scene, vello::Scene)>,
    /// False when setting the window's surface up failed (the GPU too busy, say); it is tried
    /// again before each frame, and the window is skipped until it works.
    configured: bool,
    /// A picture for this window that could not be drawn: said in the log once while it lasts,
    /// not for every picture.
    trouble: crate::applog::Lasting,
}

/// A copy of the picture a video lends, to draw: its pixels as they are, in whichever order
/// their bytes come (the GPU reads either), so copying is all the work there is.
fn copied(lent: &sp_video::Lent<'_>) -> Option<Picture> {
    Picture::from_rows(lent.width, lent.height, lent.bytes, lent.bytes_per_row, pixel_order(lent.order), lent.opaque)
}

fn pixel_order(order: sp_video::PixelOrder) -> sp_render::PixelOrder {
    match order {
        sp_video::PixelOrder::Rgba => sp_render::PixelOrder::Rgba,
        sp_video::PixelOrder::Bgra => sp_render::PixelOrder::Bgra,
    }
}

/// A window's next picture to draw into. The graphics library panics when the window's surface
/// isn't set up (its setup failed); that is caught here, so one window that can't be drawn this
/// moment doesn't stop the thread drawing every screen. `None` then: set it up and try again.
pub(crate) fn next_texture(surface: &wgpu::Surface<'static>) -> Option<wgpu::CurrentSurfaceTexture> {
    let got = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| surface.get_current_texture())).ok();
    if got.is_none() {
        crate::applog::event("output", "recovered from a window that could not be drawn; it is set up again");
    }
    got
}

/// Sets a window's surface up, saying whether that worked. A failure is caught here, not left to
/// stop the thread: a screen that can't be drawn this moment is tried again next frame.
pub(crate) fn configure(
    device: &wgpu::Device,
    surface: &wgpu::Surface<'static>,
    config: &wgpu::SurfaceConfiguration,
) -> bool {
    let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    surface.configure(device, config);
    match pollster::block_on(scope.pop()) {
        None => true,
        Some(error) => {
            crate::applog::problem("output", &format!("A screen's window could not be set up yet: {error}"));
            false
        }
    }
}

struct Output {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
    fonts: Fonts,
    frame: Frame,
    targets: Vec<Target>,
    /// When the transition under way began, and which it is. Every window follows this one
    /// clock, so they stay in step.
    fading: Option<(Instant, sp_model::Transition)>,
    /// The NDI sources that are switched on, and the texture their picture is drawn into.
    ndi: Vec<(Uuid, NdiOutput)>,
    ndi_target: Option<wgpu::Texture>,
    /// NDI pictures on their way back from the GPU, buffers kept to copy the next ones into,
    /// and whether a newer picture is waiting for the round under way to finish.
    ndi_pending: Vec<NdiReadback>,
    ndi_buffers: Vec<wgpu::Buffer>,
    ndi_dirty: bool,
    ndi_status: NdiStatuses,
    /// While screens are being identified: the name each one shows instead of the show.
    /// While screens are identified: each screen's name (as a slide), its colour, and when it
    /// started, for the wash of colour that breathes over it.
    identify: Vec<(Uuid, Arc<Slide>, sp_model::Rgba)>,
    identify_since: Instant,
    identify_theme: Arc<Theme>,
    /// The audience screens with a look of their own.
    looks: Vec<(Uuid, crate::screens::Look)>,
    themes: Vec<(Uuid, Arc<Theme>)>,
    /// The stage screens with the layout each is drawn with, and whether they are on.
    stage: Vec<(Uuid, Arc<Theme>)>,
    stage_on: bool,
    /// When the slide on screen was fully on (after its transition), for builds that start with
    /// it; and when its last click build was set going.
    slide_on: Instant,
    step_at: Instant,
    overlay: Overlay,
    /// The playing video's latest picture, and the texture on the GPU its pixels are sent
    /// into: one for each video, so the picture a fade starts from keeps the old video's.
    video_picture: Option<Picture>,
    video_texture: Option<MovingPicture>,
    /// The Announcements layer's slide, if one is on.
    announcement: Option<Announcement>,
    /// How the next new picture fades in: the slides' transition, or the media's when only the
    /// media changed.
    look: sp_model::Transition,
    /// An NDI picture that could not be drawn: said in the log once while it lasts, not for
    /// every picture. (Each window has its own.)
    ndi_trouble: crate::applog::Lasting,
    /// When to draw the screens again because the GPU did not work a picture out in time (the
    /// old picture stays up meanwhile), and how many times that has been tried since the
    /// operator last asked for anything.
    draw_again_at: Option<Instant>,
    tries_again: u8,
    /// How often and how long the screens are drawn, and how many video pictures came, for the log.
    timing: crate::applog::Timing,
    video_pictures: u64,
    /// The live preview: what the window is told, the two textures drawn into, and when the
    /// last was drawn.
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
    preview_targets: Vec<wgpu::Texture>,
    previewed_at: Instant,
    /// When a video last gave a new picture, or a message last came (so a video just shown is
    /// looked at often from the start).
    video_moved_at: Instant,
    /// The live preview's whole picture, and a fade under way in it: from, to.
    preview_shown: Option<vello::Scene>,
    preview_fade: Option<(vello::Scene, vello::Scene)>,
}

fn run(
    device: wgpu::Device,
    queue: wgpu::Queue,
    theme: Arc<Theme>,
    rx: Receiver<OutputMsg>,
    ndi_status: NdiStatuses,
    live_preview: Arc<LivePreview>,
    waker: Option<Waker>,
) {
    let painter = match Painter::new(&device) {
        Ok(painter) => painter,
        Err(e) => return eprintln!("The output cannot draw on this computer: {e}"),
    };
    let mut output = Output {
        painter,
        fonts: Fonts::new(),
        frame: Frame {
            media: None,
            video: None,
            media_fit: sp_model::ImageFit::Fill,
            slide_video: Vec::new(),
            slide: None,
            theme,
            enabled: true,
            transition: sp_model::Transition::of(sp_model::TransitionKind::Cut),
            media_transition: sp_model::Transition::of(sp_model::TransitionKind::Cut),
            next: None,
            about: crate::stage::About::default(),
            build_step: 0,
        },
        targets: Vec::new(),
        fading: None,
        ndi: Vec::new(),
        ndi_target: None,
        ndi_pending: Vec::new(),
        ndi_buffers: Vec::new(),
        ndi_dirty: false,
        ndi_status,
        identify: Vec::new(),
        identify_since: Instant::now(),
        identify_theme: Arc::new(Theme::builtin()),
        stage: Vec::new(),
        stage_on: true,
        looks: Vec::new(),
        themes: Vec::new(),
        slide_on: Instant::now(),
        step_at: Instant::now(),
        overlay: Overlay::default(),
        video_picture: None,
        video_texture: None,
        announcement: None,
        look: sp_model::Transition::of(sp_model::TransitionKind::Cut),
        ndi_trouble: crate::applog::Lasting::default(),
        draw_again_at: None,
        tries_again: 0,
        timing: crate::applog::Timing::new("screen drawings"),
        video_pictures: 0,
        live_preview,
        waker,
        preview_targets: Vec::new(),
        previewed_at: Instant::now(),
        video_moved_at: Instant::now(),
        preview_shown: None,
        preview_fade: None,
        device,
        queue,
    };

    loop {
        // The GPU library lets go of what a finished drawing used (a bind group and a texture
        // view for every picture the renderer paints) only when the device is asked to look,
        // and nothing else asks: without this, each frame drawn left about a kilobyte behind,
        // for good (a few thousand objects for every half minute of video). It never waits.
        let _ = output.device.poll(wgpu::PollType::Poll);
        // A picture the GPU was too slow with a moment ago is drawn again when its time comes,
        // whatever else the thread is waiting for.
        output.draw_again_if_due();
        // While a fade is under way, look for messages without waiting, so it keeps moving;
        // otherwise sleep until there is one.
        let first = if output.fading.is_some() || output.building() || output.scrolling() || output.identify_breathing()
        {
            match rx.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else if output.ndi_waiting() {
            // NDI pictures on their way back from the GPU are looked for every couple of
            // milliseconds (never waited on), with a playing video kept going meanwhile.
            match rx.recv_timeout(Duration::from_millis(2)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    output.finish_ndi();
                    if output.frame.plays_video() && output.take_video_picture() {
                        output.draw(false);
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if capture_wanted() {
            // Testing aid: while a picture of the output is still to be taken, keep drawing, so
            // it is taken even where no window asks to be drawn (a quiet run's are hidden).
            match rx.recv_timeout(Duration::from_millis(200)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // With a video's (or web page's) newest picture, as the screens show it.
                    output.take_video_picture();
                    output.draw(false);
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if output.ticks() {
            // Stage screens show the time: wake at the start of each second to redraw them.
            let into_second = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(Duration::ZERO, |since| Duration::from_nanos(u64::from(since.subsec_nanos())));
            // A playing video is looked at for a new picture far more often.
            let wait = if output.frame.plays_video() {
                output.video_poll()
            } else {
                Duration::from_secs(1).saturating_sub(into_second) + Duration::from_millis(5)
            };
            // No longer than until a picture the GPU was too slow with is due again.
            let wait = output.draw_again_in().map_or(wait, |due| wait.min(due));
            match rx.recv_timeout(wait) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Woken for that picture: it redraws every screen, the time included.
                    if output.draw_again_if_due() {
                        continue;
                    }
                    // A new picture of the video redraws everything, the time included.
                    if output.take_video_picture() {
                        output.draw(false);
                        continue;
                    }
                    // Otherwise the time changes only as a second begins (a video, or a web page
                    // or video in a box, wakes this far more often than that).
                    let new_second =
                        !output.frame.plays_video() || into_second + output.video_poll() >= Duration::from_secs(1);
                    if !new_second {
                        continue;
                    }
                    if output.overlay.message.as_ref().is_some_and(|(text, _)| crate::messages::has_tokens(text))
                        || output.links_tick()
                    {
                        output.draw(false);
                    } else {
                        output.draw_stage();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else if output.frame.plays_video() {
            match rx.recv_timeout(output.video_poll()) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if output.take_video_picture() {
                        output.draw(false);
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            // Nothing moves until the next message: memory a heavy picture took is given back.
            // That never waits here; while the GPU still holds some of it, or a picture it was
            // too slow with is to be drawn again, the thread comes back in a moment to look.
            let rested = output.painter.rest(&output.device, &output.queue);
            match output.look_again_in(rested) {
                None => match rx.recv() {
                    Ok(msg) => Some(msg),
                    Err(_) => return,
                },
                // The top of the loop looks at the GPU, and draws the picture if it is due.
                Some(wait) => match rx.recv_timeout(wait) {
                    Ok(msg) => Some(msg),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
                },
            }
        };
        let Some(first) = first else {
            // Drawing waits for the screen's next refresh, which paces the fade and the builds.
            if output.fading.is_some() {
                output.step_fade();
            } else {
                output.take_video_picture();
                output.draw(false);
                // With no window to wait for (only NDI), keep to about the screens' pace.
                if output.targets.is_empty() {
                    std::thread::sleep(Duration::from_millis(16));
                }
            }
            continue;
        };
        output.video_moved_at = Instant::now();
        // Whatever the operator asked for now gets its own tries, should the GPU be slow again.
        output.tries_again = 0;
        // Take everything that has queued up, so a burst of changes costs one draw.
        let mut next = Some(first);
        let mut new_picture = false;
        while let Some(msg) = next {
            match msg {
                OutputMsg::Show(frame) => {
                    let now = Instant::now();
                    // The next slide's pictures are read meanwhile, so it goes on at once.
                    if let Some(next) = frame.next.as_deref() {
                        sp_render::read_ahead(next);
                    }
                    let same_slide = output.frame.slide.as_deref() == frame.slide.as_deref()
                        && output.frame.enabled == frame.enabled;
                    if !same_slide {
                        // Builds that start with the slide wait for its transition to end.
                        output.slide_on = now + Duration::from_secs_f32(frame.transition.duration());
                        output.step_at = now;
                        new_picture = true;
                    } else if frame.build_step > output.frame.build_step {
                        // A click build is drawn moving on the slide already there, not faded to.
                        output.step_at = now;
                    } else if frame.build_step < output.frame.build_step {
                        // Stepping back shows the builds still done as they were, without moving
                        // them again.
                        output.step_at = now.checked_sub(Duration::from_secs(3600)).unwrap_or(now);
                    } else {
                        new_picture = true;
                    }
                    // Another video, or none: its last picture goes, and its still shows until
                    // the new one's first picture comes.
                    let same_video = match (&output.frame.video, &frame.video) {
                        (Some(was), Some(now)) => was.same(now),
                        (None, None) => true,
                        _ => false,
                    };
                    if !same_video {
                        output.video_picture = None;
                        output.video_texture = None;
                    }
                    // A video box that stops playing goes back to its still.
                    for (file, _) in &output.frame.slide_video {
                        if !frame.slide_video.iter().any(|(still, _)| still == file) {
                            sp_render::forget_picture(file);
                        }
                    }
                    let same_media = match (&output.frame.media, &frame.media) {
                        (Some(was), Some(now)) => was.same(now),
                        (None, None) => true,
                        _ => false,
                    };
                    // Only the media changed: it gives way by the media's own transition.
                    output.look = if same_slide && !same_media { frame.media_transition } else { frame.transition };
                    output.frame = frame;
                }
                OutputMsg::AddSurface { id, surface, format, w, h } => output.add_surface(id, surface, format, w, h),
                OutputMsg::PreviewTargets(targets) => output.preview_targets = targets,
                OutputMsg::Resized(id, w, h) => output.resize(id, w, h),
                OutputMsg::Redraw => {}
                OutputMsg::SetNdi { id, name } => output.set_ndi(id, name),
                OutputMsg::RemoveSurface(id) => output.targets.retain(|target| target.id != id),
                OutputMsg::Announcement(announcement) => {
                    // A new announcement dissolves in over what is there.
                    output.announcement = announcement;
                    output.look = sp_model::Transition::default();
                    new_picture = true;
                }
                OutputMsg::Overlay(overlay) => output.overlay = overlay,
                OutputMsg::Looks(looks) => output.looks = looks,
                OutputMsg::Themes(themes) => output.themes = themes,
                OutputMsg::Stage { screens, on } => {
                    output.stage = screens;
                    output.stage_on = on;
                }
                OutputMsg::Identify(names) => {
                    output.identify = names
                        .into_iter()
                        .map(|(id, name, color)| {
                            (id, Arc::new(crate::messages::slide(&name, crate::messages::Look::Centre)), color)
                        })
                        .collect();
                    output.identify_since = Instant::now();
                    // Names appear and go at once, whatever was fading.
                    output.fading = None;
                    for target in &mut output.targets {
                        target.fade = None;
                        target.shown = None;
                    }
                }
                OutputMsg::Shutdown => return,
            }
            next = rx.try_recv().ok();
        }
        output.draw(new_picture);
    }
}

/// How far in a fade is, from 0 to 1, eased so it starts and ends gently.
fn fade_amount(started: Instant, seconds: f32) -> f32 {
    let t = (started.elapsed().as_secs_f32() / seconds.max(0.001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Output {
    fn add_surface(&mut self, id: Uuid, surface: wgpu::Surface<'static>, format: wgpu::TextureFormat, w: u32, h: u32) {
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: w.max(1),
            height: h.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        let configured = configure(&self.device, &surface, &config);
        // A screen has one window: a new one takes the old one's place.
        self.targets.retain(|target| target.id != id);
        self.targets.push(Target {
            id,
            blitter: wgpu::util::TextureBlitter::new(&self.device, format),
            texture: target_texture(&self.device, config.width, config.height),
            surface,
            config,
            shown: None,
            fade: None,
            configured,
            trouble: crate::applog::Lasting::default(),
        });
    }

    fn resize(&mut self, id: Uuid, w: u32, h: u32) {
        let Some(target) = self.targets.iter_mut().find(|target| target.id == id) else { return };
        // A minimised window reports zero; keep the last real size until it comes back.
        if w == 0 || h == 0 || (w == target.config.width && h == target.config.height) {
            return;
        }
        target.config.width = w;
        target.config.height = h;
        target.configured = configure(&self.device, &target.surface, &target.config);
        target.texture = target_texture(&self.device, w, h);
        // Pictures built for the old size cannot be faded at the new one.
        target.shown = None;
        target.fade = None;
    }

    /// Starts, renames or stops a screen's NDI source and records how that went for the operator.
    fn set_ndi(&mut self, id: Uuid, name: Option<String>) {
        // Stop the old source first, so a renamed one does not appear twice.
        self.ndi.retain(|(have, _)| *have != id);
        // Starting never waits on NDI: the source starts on its own thread.
        let status = name.map(|name| match NdiOutput::start(&name) {
            Ok(ndi) => {
                let entry = NdiEntry::Started(ndi.name().to_owned(), ndi.watch());
                self.ndi.push((id, ndi));
                entry
            }
            Err(reason) => NdiEntry::Refused(reason),
        });
        if self.ndi.is_empty() {
            self.ndi_target = None;
            self.ndi_pending.clear();
            self.ndi_buffers.clear();
            self.ndi_dirty = false;
        }
        // A new source gets the picture on screen now.
        if !self.ndi.is_empty() {
            let frame = self.frame.clone();
            self.send_ndi(&frame);
        }
        if let Ok(mut shared) = self.ndi_status.lock() {
            shared.retain(|(have, _)| *have != id);
            shared.extend(status.map(|status| (id, status)));
        }
    }

    /// What a screen shows now. Cheap copies: the picture, the slide and the theme are shared.
    fn content_for(&self, id: Uuid, frame: &Frame) -> Content {
        if let Some((_, layout)) = self.stage.iter().find(|(have, _)| *have == id) {
            let words = self.stage_on.then(|| layout.templates.first()).flatten().map(|template| {
                let (now, next) = (frame.slide.as_deref(), frame.next.as_deref());
                Arc::new(crate::stage::slide(now, next, &self.overlay, &frame.about, template))
            });
            return (None, words, None, layout.clone(), None);
        }
        if let Some(pattern) = self.overlay.test_pattern {
            return (None, Some(Arc::new(pattern.slide())), None, self.identify_theme.clone(), None);
        }
        if !frame.enabled {
            return (None, None, None, frame.theme.clone(), None);
        }
        // A screen's look can leave layers out, and its own theme lays the words out its way.
        let look = self.look_of(id);
        let theme = screen_theme(&self.themes, id, &frame.theme);
        // A message shows over whatever is on the audience screens.
        let message = self.overlay.message.as_ref().filter(|_| look.messages).map(|(text, look)| {
            let words =
                crate::messages::fill_in(text, &self.overlay.timers, Instant::now(), crate::timers::seconds_into_day());
            crate::messages::slide(&words, *look)
        });
        // Props and the message share the layer over the slide.
        let copyright = self.overlay.copyright.as_deref().map(crate::props::copyright_slide);
        let values = crate::links::Values {
            timers: &self.overlay.timers,
            title: frame.about.title.as_deref(),
            group: frame.about.group.as_deref(),
            next: frame.next.as_deref(),
            stage_message: self.overlay.stage_message.as_deref(),
            copyright: self.overlay.copyright.as_deref(),
        };
        let mut under: Vec<Slide> = if look.props {
            self.overlay.props.iter().map(|prop| crate::links::fill(prop, &values).into_owned()).collect()
        } else {
            Vec::new()
        };
        // The copyright line goes with the slide's words.
        under.extend(copyright.filter(|_| look.slides));
        let over = crate::props::overlay(&under, message.as_ref()).map(Arc::new);
        // The slide as far as its builds have got, with its linked text filled in.
        let slide = frame.slide.as_ref().filter(|_| look.slides).map(|slide| {
            let (built, _) = slide.built(&theme, self.build_clock());
            Arc::new(crate::links::fill(&built, &values).into_owned())
        });
        let media = self.video_picture.clone().or_else(|| frame.media.clone());
        let announcement = self.announcement.clone().filter(|_| look.announcements);
        (media.filter(|_| look.media), slide, over, theme, announcement)
    }

    /// How an audience screen shows the show: its own look, or every layer.
    fn look_of(&self, id: Uuid) -> crate::screens::Look {
        self.looks.iter().find(|(have, _)| *have == id).map_or_else(crate::screens::Look::default, |(_, look)| *look)
    }

    /// Where the slide on screen is in its builds now.
    fn build_clock(&self) -> sp_model::BuildClock {
        let now = Instant::now();
        sp_model::BuildClock {
            step: self.frame.build_step,
            since_on: now.checked_duration_since(self.slide_on).map(|since| since.as_secs_f32()),
            since_step: now.duration_since(self.step_at).as_secs_f32(),
        }
    }

    /// True while a build on the slide on screen is moving, or waiting to, so it is drawn again
    /// at each refresh.
    fn building(&self) -> bool {
        self.frame.enabled
            && self.frame.slide.as_ref().is_some_and(|slide| {
                let clock = self.build_clock();
                // During the transition, builds wait for it to end: keep drawing until they're done.
                clock.since_on.is_none() && slide.objects.iter().any(|o| o.build_in.is_some_and(|b| !b.on_click))
                    || slide.built(&self.frame.theme, clock).1
            })
    }

    /// Builds what a screen shows now for a target of a size.
    fn scene_for(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> vello::Scene {
        let (media, slide, overlay, theme, announcement) = self.content_for(id, frame);
        // While screens are identified, each is marked over what it shows.
        let identified =
            self.identify.iter().find(|(have, ..)| *have == id).map(|(_, name, color)| (name.clone(), *color));
        // Scrolling text moves on from when the slide was fully on screen.
        let time = Instant::now().checked_duration_since(self.slide_on).map_or(0.0, |since| since.as_secs_f32());
        // A stage screen keeps the plain look.
        let look = if self.is_stage(id) { crate::screens::Look::default() } else { self.look_of(id) };
        let backdrop = (look.color != sp_model::Rgba::BLACK).then_some(look.color);
        let layers = Layers {
            media_fit: frame.media_fit,
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: overlay.as_deref(),
            time,
            backdrop,
            mask: look.mask,
            announcement: announcement.as_ref().map(|(slide, theme)| (&**slide, &**theme)),
        };
        let mut scene = build_layers(&mut self.fonts, &layers, &theme, w, h).scene;
        // Its colour and border, then its name on top, clear of the colour.
        if let Some((name, color)) = identified {
            identify_marks(&mut scene, color, identify_strength(self.identify_since.elapsed().as_secs_f32()), w, h);
            let label = sp_render::build_overlay(&mut self.fonts, &name, &self.identify_theme, w, h);
            scene.append(&label, None);
        }
        scene
    }

    /// True while the wash over identified screens is still breathing, so they are drawn at
    /// each refresh until it settles.
    fn identify_breathing(&self) -> bool {
        !self.identify.is_empty() && self.identify_since.elapsed().as_secs_f32() < IDENTIFY_BREATHS * IDENTIFY_BREATH
    }

    /// Draws the picture at NDI's size for every NDI source and starts copying it back from the
    /// GPU, without waiting for the copy: `finish_ndi` hands it over once it is there. One
    /// round of copies is under way at a time; a picture that comes while one is under way is
    /// not waited for, and the newest is drawn as soon as the round is done. This does not
    /// depend on any window: with none open, NDI still sends.
    fn send_ndi(&mut self, frame: &Frame) {
        self.finish_ndi();
        if self.ndi.is_empty() {
            return;
        }
        if !self.ndi_pending.is_empty() {
            self.ndi_dirty = true;
            return;
        }
        self.ndi_dirty = false;
        let (w, h) = NDI_SIZE;
        // The show is drawn once for every source that shows it as it is; a screen being
        // identified, a stage screen or one with its own look gets its own picture.
        let mut shared: Vec<Uuid> = Vec::new();
        let mut own: Vec<Uuid> = Vec::new();
        for (id, ndi) in &self.ndi {
            // A source that couldn't start (NDI not installed, say) gets nothing drawn for it.
            if matches!(ndi.state(), sp_ndi::State::Failed(_)) {
                continue;
            }
            let alone = self.identify.iter().any(|(have, ..)| have == id)
                || self.is_stage(*id)
                || self.looks.iter().any(|(have, _)| have == id)
                || self.themes.iter().any(|(have, _)| have == id);
            if alone { own.push(*id) } else { shared.push(*id) }
        }
        let mut groups: Vec<Vec<Uuid>> = own.into_iter().map(|id| vec![id]).collect();
        if !shared.is_empty() {
            groups.push(shared);
        }
        for ids in groups {
            let scene = self.scene_for(ids[0], frame, w, h);
            let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h)).clone();
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            if let Err(e) = self.painter.paint(&self.device, &self.queue, &scene, &view, w, h) {
                let message = format!("The NDI picture could not be drawn: {e}");
                if self.ndi_trouble.is_news(&message) {
                    crate::applog::problem("output", &message);
                }
                return;
            }
            // The copy is queued after the drawing, so it takes this picture even though the next
            // group is drawn into the same texture straight after.
            let padded_row = (w as usize * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize)
                * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
            let buffer = self.ndi_buffers.pop().unwrap_or_else(|| {
                self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("NDI readback"),
                    size: (padded_row * h as usize) as u64,
                    usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                    mapped_at_creation: false,
                })
            });
            let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            encoder.copy_texture_to_buffer(
                target.as_image_copy(),
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(padded_row as u32),
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
            );
            self.queue.submit([encoder.finish()]);
            let done: Arc<Mutex<Option<bool>>> = Arc::default();
            let flag = done.clone();
            buffer.slice(..).map_async(wgpu::MapMode::Read, move |result| {
                if let Ok(mut flag) = flag.lock() {
                    *flag = Some(result.is_ok());
                }
            });
            self.ndi_pending.push(NdiReadback { ids, buffer, padded_row, done });
        }
        // Every picture was drawn: whatever went wrong before is over.
        if let Some(more) = self.ndi_trouble.over() {
            crate::applog::event("output", &format!("The NDI picture is drawn again, after {more} more not drawn"));
        }
    }

    /// True while pictures for NDI are on their way back from the GPU.
    fn ndi_waiting(&self) -> bool {
        !self.ndi_pending.is_empty()
    }

    /// Hands every NDI picture that has come back from the GPU to its sources, without
    /// waiting for any still on the way; then, if a newer picture came meanwhile, starts it.
    fn finish_ndi(&mut self) {
        if self.ndi_pending.is_empty() {
            return;
        }
        let _ = self.device.poll(wgpu::PollType::Poll);
        let (w, h) = NDI_SIZE;
        let row = w as usize * 4;
        let mut still = Vec::new();
        for readback in std::mem::take(&mut self.ndi_pending) {
            let done = readback.done.lock().ok().and_then(|done| *done);
            match done {
                None => still.push(readback),
                Some(false) => crate::applog::problem("output", "An NDI picture could not be read back."),
                Some(true) => {
                    let pixels = {
                        let slice = readback.buffer.slice(..);
                        let Ok(mapped) = slice.get_mapped_range() else { continue };
                        let mut pixels = Vec::with_capacity(row * h as usize);
                        for padded in mapped.chunks(readback.padded_row) {
                            pixels.extend_from_slice(&padded[..row]);
                        }
                        pixels
                    };
                    readback.buffer.unmap();
                    for id in &readback.ids {
                        if let Some((_, ndi)) = self.ndi.iter().find(|(have, _)| have == id) {
                            ndi.show(w, h, pixels.clone());
                        }
                    }
                    // Kept for the next picture, two at most.
                    if self.ndi_buffers.len() < 2 {
                        self.ndi_buffers.push(readback.buffer);
                    }
                }
            }
        }
        self.ndi_pending = still;
        if self.ndi_pending.is_empty() && self.ndi_dirty {
            let frame = self.frame.clone();
            self.send_ndi(&frame);
        }
    }

    /// Draws the current frame everywhere. A new picture fades in over the one on screen when
    /// the frame asks for a dissolve; anything else (a resize, an uncovered window) is drawn
    /// at once.
    fn draw(&mut self, new_picture: bool) {
        let started = Instant::now();
        self.draw_now(new_picture);
        self.timing.add(started.elapsed());
        let (videos, screens, ndi) = (self.video_pictures, self.targets.len(), self.ndi.len());
        let extra = || format!("{videos} video pictures, {screens} windows, {ndi} NDI");
        if self.timing.report_each_minute("output", extra) {
            self.video_pictures = 0;
        }
    }

    /// Draws the audience picture small for the operator's window, as the screens show it: a
    /// new picture fades in as theirs does; a playing video's pictures are drawn at most thirty
    /// times a second.
    fn draw_live_preview(&mut self, frame: &Frame, new_picture: bool) {
        if self.preview_targets.len() < 2 {
            return;
        }
        if !new_picture && frame.plays_video() && self.previewed_at.elapsed() < LIVE_PREVIEW_EVERY {
            return;
        }
        let (w, h) = LIVE_PREVIEW_SIZE;
        let scene = self.scene_for(Uuid::nil(), frame, w, h);
        // A video's new picture during a fade: the fade goes on, towards the newer picture.
        if !new_picture && let Some((from, _)) = self.preview_fade.take() {
            self.preview_fade = Some((from, scene));
            return self.step_live_preview(None);
        }
        let before = self.preview_fade.take().map(|(_, to)| to).or_else(|| self.preview_shown.take());
        match before {
            Some(from) if new_picture && self.fading.is_some() => {
                self.preview_fade = Some((from, scene));
                self.step_live_preview(Some(0.0));
            }
            _ => {
                self.paint_live_preview(&scene);
                self.preview_shown = Some(scene);
            }
        }
    }

    /// Moves a fade in the live preview on to `amount` (as far as the screens' fade has got
    /// when `None`); at the end it shows the whole new picture.
    fn step_live_preview(&mut self, amount: Option<f32>) {
        let Some((from, to)) = self.preview_fade.take() else { return };
        let amount =
            amount.unwrap_or_else(|| self.fading.map_or(1.0, |(started, look)| fade_amount(started, look.duration())));
        if amount >= 1.0 {
            self.paint_live_preview(&to);
            self.preview_shown = Some(to);
            return;
        }
        let (w, h) = LIVE_PREVIEW_SIZE;
        let look = self.fading.map_or(sp_model::Transition::default(), |(_, look)| look);
        let mixed = sp_render::transition(look.kind, look.direction, &from, &to, amount, w, h);
        self.paint_live_preview(&mixed);
        self.preview_fade = Some((from, to));
    }

    /// Paints a picture into the live preview texture the window isn't showing, then tells the
    /// window to show it.
    fn paint_live_preview(&mut self, scene: &vello::Scene) {
        use std::sync::atomic::Ordering::{Acquire, Release};
        let (w, h) = LIVE_PREVIEW_SIZE;
        let back = 1 - self.live_preview.front.load(Acquire).min(1);
        let view = self.preview_targets[back].create_view(&wgpu::TextureViewDescriptor::default());
        if self.painter.paint(&self.device, &self.queue, scene, &view, w, h).is_ok() {
            self.live_preview.front.store(back, Release);
            self.live_preview.drawn.fetch_add(1, Release);
            // Whatever moved (a video, a build, scrolling words, a timer), the window is woken
            // to show it; once, until it has.
            if !self.live_preview.wake_pending.swap(true, Acquire)
                && let Some(wake) = &self.waker
            {
                wake();
            }
        }
        self.previewed_at = Instant::now();
    }

    fn draw_now(&mut self, new_picture: bool) {
        // Cheap copies: the picture and the slide are shared, not duplicated.
        let frame = self.frame.clone();
        self.send_ndi(&frame);
        self.capture(&frame);
        // How far a fade already under way has got, before the clock is started again.
        let amount = self.fading.map(|(started, look)| fade_amount(started, look.duration()));
        let mut fading = false;
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            let scene = self.scene_for(id, &frame, w, h);
            let stage = self.is_stage(id);
            // What the next fade starts from: the last whole picture, or for a fade cut short,
            // the whole picture nearer to what the window shows.
            let target = &mut self.targets[index];
            let before = match target.fade.take() {
                Some((from, to)) => Some(fade_from(from, to, amount.unwrap_or(1.0))),
                None => target.shown.take(),
            };
            // Stage screens change at once: the people on stage need the words now.
            match before {
                Some(from) if new_picture && self.look.duration() > 0.0 && !stage => {
                    target.fade = Some((from, scene));
                    fading = true;
                }
                _ => {
                    self.present(index, &scene);
                    self.targets[index].shown = Some(scene);
                }
            }
        }
        self.fading = fading.then(|| (Instant::now(), self.look));
        // The preview follows, fading if the screens do.
        self.draw_live_preview(&frame, new_picture);
        if fading {
            self.step_fade();
        }
    }

    /// Testing aid: saves what the audience screens show, once, when something is on them a few
    /// seconds after the start, to the file SOFTPRESENTER_CAPTURE_OUTPUT names
    /// (SOFTPRESENTER_CAPTURE_OUTPUT_AFTER seconds, 3 if not given).
    fn capture(&mut self, frame: &Frame) {
        static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
        let Some(path) = std::env::var_os("SOFTPRESENTER_CAPTURE_OUTPUT") else { return };
        let after = std::env::var("SOFTPRESENTER_CAPTURE_OUTPUT_AFTER").ok().and_then(|s| s.parse().ok());
        let started = *STARTED.get_or_init(Instant::now);
        // Taken once something is on screen.
        let showing = frame.slide.is_some() || frame.media.is_some();
        if started.elapsed().as_secs_f32() < after.unwrap_or(3.0)
            || !showing
            || CAPTURE_DONE.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            return;
        }
        let (w, h) = NDI_SIZE;
        // The first screen's picture, as its window shows it (its look, its identify marks).
        let id = self.targets.first().map_or(Uuid::nil(), |target| target.id);
        let scene = self.scene_for(id, frame, w, h);
        let target = target_texture(&self.device, w, h);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        let saved = self
            .painter
            .paint(&self.device, &self.queue, &scene, &view, w, h)
            .map_err(|e| e.to_string())
            .and_then(|()| read_pixels(&self.device, &self.queue, &target).map_err(|e| e.to_string()))
            .and_then(|pixels| {
                let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
                let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
                png.set_color(png::ColorType::Rgba);
                png.set_depth(png::BitDepth::Eight);
                png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).map_err(|e| e.to_string())
            });
        match saved {
            Ok(()) => eprintln!("saved the output to {}", std::path::Path::new(&path).display()),
            Err(e) => eprintln!("could not save the output: {e}"),
        }
    }

    /// Takes the playing video's newest picture, if a new one has come. True when it has.
    /// How long to wait before looking at a playing video for a new picture: a refresh's
    /// worth while pictures come, longer once none has come for a second.
    fn video_poll(&self) -> Duration {
        if self.video_moved_at.elapsed() < Duration::from_secs(1) { VIDEO_POLL } else { VIDEO_IDLE_POLL }
    }

    fn take_video_picture(&mut self) -> bool {
        let mut new = false;
        // The video behind the slide: its pixels go straight from the player into a texture on
        // the GPU, the same one for every picture, with nothing copied or put in another order
        // on the way. Only this thread's painter draws it (the screens, the live preview, NDI).
        let Output { frame, painter, device, queue, video_texture, .. } = self;
        let shown = frame.video.as_ref().and_then(|frames| {
            frames.next_with(|lent| {
                // The GPU draws each pixel's fourth byte as its alpha. A picture shown opaque
                // whatever those bytes say goes there as it is only when every one is full
                // (as the system's player decodes all but a video with clear parts); a picture
                // where one isn't is drawn from a copy made opaque.
                if lent.opaque && !lent.fourth_bytes_full() {
                    return copied(lent);
                }
                let order = pixel_order(lent.order);
                // The video's first picture, or one of another size than its last: a new texture.
                if !video_texture.as_ref().is_some_and(|kept| kept.fits(lent.width, lent.height, order)) {
                    *video_texture = painter.moving_picture(device, lent.width, lent.height, order);
                }
                video_texture
                    .as_ref()
                    .filter(|kept| painter.show_rows(queue, kept, lent.bytes, lent.bytes_per_row))
                    .map(MovingPicture::picture)
                    // A picture too large for a texture is drawn from a copy.
                    .or_else(|| copied(lent))
            })
        });
        if let Some(shown) = shown {
            self.video_picture = shown;
            self.video_pictures += 1;
            new |= self.video_picture.is_some();
        }
        // Each video in a box on the slide: its newest picture stands in for the box's still.
        // It is a copy, because every thread that draws the slide (the operator's window too)
        // shows it.
        for (file, frames) in &self.frame.slide_video {
            if let Some(Some(picture)) = frames.next_with(copied) {
                sp_render::show_picture(file, picture);
                self.video_pictures += 1;
                new = true;
            }
        }
        if new {
            self.video_moved_at = Instant::now();
        }
        new
    }

    /// True when something on the screens changes by itself each second: the time on a stage
    /// screen, or a message with the time or a timer in it.
    fn ticks(&self) -> bool {
        self.has_stage()
            || self.overlay.message.as_ref().is_some_and(|(text, _)| crate::messages::has_tokens(text))
            || self.links_tick()
    }

    /// True while text on the audience screens scrolls, so it is drawn again at each refresh.
    fn scrolling(&self) -> bool {
        self.frame.enabled
            && self.overlay.test_pattern.is_none()
            && (self.frame.slide.as_deref().is_some_and(|slide| sp_render::scrolls(slide, &self.frame.theme))
                || self.overlay.props.iter().any(|prop| sp_render::scrolls(prop, &self.frame.theme)))
    }

    /// True when linked text on the audience screens shows the time or a timer.
    fn links_tick(&self) -> bool {
        self.frame.enabled
            && (self.frame.slide.as_deref().is_some_and(crate::links::ticks)
                || self.overlay.props.iter().any(crate::links::ticks))
    }

    /// True for a stage screen.
    fn is_stage(&self, id: Uuid) -> bool {
        self.stage.iter().any(|(have, _)| *have == id)
    }

    /// True when there is a stage screen to keep the time on.
    fn has_stage(&self) -> bool {
        self.stage_on
            && (self.targets.iter().any(|target| self.is_stage(target.id))
                || self.ndi.iter().any(|(id, _)| self.is_stage(*id)))
    }

    /// Draws the stage screens again, for the time, leaving the others as they are.
    fn draw_stage(&mut self) {
        let frame = self.frame.clone();
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            if !self.is_stage(id) {
                continue;
            }
            let scene = self.scene_for(id, &frame, w, h);
            self.present(index, &scene);
            self.targets[index].shown = Some(scene);
        }
        // A stage screen sent over NDI gets the new time too, the same way as every NDI picture
        // (never waited on).
        if self.ndi.iter().any(|(id, _)| self.is_stage(*id)) {
            self.send_ndi(&frame);
        }
    }

    /// Draws the next step of the fade under way in every window, and finishes it when its
    /// time is up.
    fn step_fade(&mut self) {
        let Some((started, look)) = self.fading else { return };
        let amount = fade_amount(started, look.duration());
        // A video playing behind the new slide keeps moving through the fade: each new picture
        // of it builds the picture being faded to again.
        if self.take_video_picture() {
            let frame = self.frame.clone();
            for index in 0..self.targets.len() {
                let (id, w, h) = {
                    let target = &self.targets[index];
                    (target.id, target.config.width, target.config.height)
                };
                if self.targets[index].fade.is_some() {
                    let fresh = self.scene_for(id, &frame, w, h);
                    if let Some((_, to)) = self.targets[index].fade.as_mut() {
                        *to = fresh;
                    }
                }
            }
        }
        for index in 0..self.targets.len() {
            let Some((from, to)) = self.targets[index].fade.take() else { continue };
            if amount >= 1.0 {
                self.present(index, &to);
                self.targets[index].shown = Some(to);
            } else {
                let (w, h) = (self.targets[index].config.width, self.targets[index].config.height);
                let scene = sp_render::transition(look.kind, look.direction, &from, &to, amount, w, h);
                self.present(index, &scene);
                self.targets[index].fade = Some((from, to));
            }
        }
        self.step_live_preview(Some(amount));
        if amount >= 1.0 || self.targets.iter().all(|target| target.fade.is_none()) {
            self.fading = None;
        }
    }

    /// How long the thread may sleep with nothing to draw before it must look again: not at
    /// all (`None`) unless the GPU still holds memory the painter gave back (`rested` is
    /// false), or a picture the GPU was too slow with is waiting to be drawn again.
    fn look_again_in(&self, rested: bool) -> Option<Duration> {
        let for_memory = (!rested).then_some(GPU_LOOK_AGAIN);
        [for_memory, self.draw_again_in()].into_iter().flatten().min()
    }

    /// How long until a picture the GPU was too slow with is due to be drawn again, if one is.
    fn draw_again_in(&self) -> Option<Duration> {
        self.draw_again_at.map(|at| at.saturating_duration_since(Instant::now()))
    }

    /// Draws the screens again if a picture the GPU was too slow with is due another try, and
    /// says whether it did.
    fn draw_again_if_due(&mut self) -> bool {
        let due = self.draw_again_at.is_some_and(|at| at <= Instant::now());
        if due {
            self.draw_again_at = None;
            self.draw(false);
        }
        due
    }

    /// Paints a picture and shows it in one window. A picture that cannot be painted leaves the
    /// one before it on the screen, and is said in the log once for as long as that lasts.
    fn present(&mut self, index: usize, scene: &vello::Scene) {
        let Output { device, queue, painter, targets, .. } = self;
        let Some(target) = targets.get_mut(index) else { return };
        let (w, h) = (target.config.width, target.config.height);
        let view = target.texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = painter.paint(device, queue, scene, &view, w, h) {
            let message = format!("The output could not draw a frame, and keeps showing the one before: {e}");
            if target.trouble.is_news(&message) {
                crate::applog::problem("output", &message);
            }
            // Too heavy stays too heavy. Too slow may pass: when nothing else will draw the
            // screens soon, they are drawn again shortly, a few times at most.
            if e.worth_trying_again() && self.tries_again < DRAW_AGAIN_TRIES {
                self.tries_again += 1;
                self.draw_again_at = Some(Instant::now() + DRAW_AGAIN_AFTER);
            }
            return;
        }
        if let Some(more) = target.trouble.over() {
            crate::applog::event("output", &format!("The output draws again, after {more} more frames not drawn"));
        }
        // A window whose surface isn't set up is set up first, or skipped this frame.
        if !target.configured {
            target.configured = configure(device, &target.surface, &target.config);
            if !target.configured {
                return std::thread::sleep(Duration::from_millis(8));
            }
        }
        // A lost or outdated surface is set up again and tried once more; anything else
        // (hidden window, timeout) skips this frame and the next message redraws.
        for attempt in 0..2 {
            let Some(next) = next_texture(&target.surface) else {
                target.configured = false;
                return;
            };
            match next {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    target.blitter.copy(device, &mut encoder, &view, &surface_view);
                    queue.submit([encoder.finish()]);
                    queue.present(frame);
                    return;
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                    target.configured = configure(device, &target.surface, &target.config);
                    if !target.configured {
                        return;
                    }
                }
                // A hidden window shows nothing; wait a moment so a fade does not spin.
                _ => return std::thread::sleep(Duration::from_millis(8)),
            }
        }
    }
}

/// What a fade cut short by a new picture is followed from: the whole picture nearer to what the
/// screen shows (the old one before half-way, the new one after), never the part-way mix itself.
/// A mix would hold both pictures, and the next one cut short would hold that mix and another,
/// so an operator clicking on faster than fades finish would pile slide on slide into every
/// frame, each slower to draw than the last, and keep the memory for all of them.
fn fade_from<T>(from: T, to: T, amount: f32) -> T {
    if amount < 0.5 { from } else { to }
}

/// How a screen being identified is marked: three quick breaths of its colour, from a tenth to a
/// fifth of the way over the picture and back, about 0.7 s each; then a steady tenth.
const IDENTIFY_BREATHS: f32 = 3.0;
const IDENTIFY_BREATH: f32 = 0.7;

/// How strongly an identified screen's colour lies over it, `seconds` after identifying began.
fn identify_strength(seconds: f32) -> f32 {
    if seconds >= IDENTIFY_BREATHS * IDENTIFY_BREATH {
        return 0.10;
    }
    0.15 - 0.05 * (seconds / IDENTIFY_BREATH * std::f32::consts::TAU).cos()
}

/// Lays an identified screen's marks over its picture: its colour over the whole picture at
/// `strength`, and a solid border of it round the edge.
fn identify_marks(scene: &mut vello::Scene, color: sp_model::Rgba, strength: f32, w: u32, h: u32) {
    use vello::kurbo::{Affine, Rect, Stroke};
    use vello::peniko::{Color, Fill};
    let (w, h) = (f64::from(w), f64::from(h));
    let solid = Color::from_rgba8(color.r, color.g, color.b, 255);
    scene.fill(Fill::NonZero, Affine::IDENTITY, solid.with_alpha(strength), None, &Rect::new(0.0, 0.0, w, h));
    let border = (h / 1080.0 * 16.0).max(6.0);
    let half = border / 2.0;
    scene.stroke(&Stroke::new(border), Affine::IDENTITY, solid, None, &Rect::new(half, half, w - half, h - half));
}

/// The theme a screen's slides are drawn with: its own, if it has one, or the one the slide
/// came with (the main theme, or a song's own).
fn screen_theme(themes: &[(Uuid, Arc<Theme>)], screen: Uuid, slide_theme: &Arc<Theme>) -> Arc<Theme> {
    themes.iter().find(|(have, _)| *have == screen).map_or_else(|| slide_theme.clone(), |(_, own)| own.clone())
}

/// Testing aid: whether the picture of the output SOFTPRESENTER_CAPTURE_OUTPUT asks for is
/// still to be taken.
static CAPTURE_DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn capture_wanted() -> bool {
    std::env::var_os("SOFTPRESENTER_CAPTURE_OUTPUT").is_some()
        && !CAPTURE_DONE.load(std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_with_a_theme_of_its_own_draws_with_it_and_the_rest_with_the_slides() {
        let main = Arc::new(Theme::builtin());
        let mut lower = Theme::builtin();
        lower.id = Uuid::new_v4();
        lower.name = "Lower third".to_owned();
        let lower = Arc::new(lower);
        let (stream, audience) = (Uuid::new_v4(), Uuid::new_v4());
        let themes = vec![(stream, lower.clone())];
        assert_eq!(screen_theme(&themes, stream, &main).name, "Lower third");
        assert_eq!(screen_theme(&themes, audience, &main).name, "Built-in");
    }

    #[test]
    fn slides_clicked_through_faster_than_fades_never_pile_up_in_a_frame() {
        use vello::kurbo::{Affine, Rect};
        use vello::peniko::{Color, Fill};
        let slide = |shade: u8| {
            let mut scene = vello::Scene::new();
            scene.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                Color::from_rgb8(shade, 0, 0),
                None,
                &Rect::new(0.0, 0.0, 10.0, 10.0),
            );
            scene
        };
        let size = |scene: &vello::Scene| scene.encoding().draw_tags.len();
        let kind = sp_model::TransitionKind::Dissolve;
        let one = size(&sp_render::transition(kind, sp_model::Direction::default(), &slide(0), &slide(1), 0.3, 10, 10));
        // A hundred fades, each cut short by the next picture, some before half-way, some after.
        let mut from = slide(0);
        let mut frame = vello::Scene::new();
        for n in 1..=100u8 {
            let to = slide(n);
            let amount = if n % 2 == 0 { 0.3 } else { 0.7 };
            frame = sp_render::transition(kind, sp_model::Direction::default(), &from, &to, amount, 10, 10);
            from = fade_from(from, to, amount);
        }
        assert_eq!(size(&frame), one, "each frame holds two slides, however many fades were cut short");
        assert_eq!(fade_from(1, 2, 0.49), 1);
        assert_eq!(fade_from(1, 2, 0.5), 2);
    }
}
