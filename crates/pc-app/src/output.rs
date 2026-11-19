//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pc_model::{Object, Role, Slide, Theme, Uuid};
use pc_ndi::NdiOutput;
use pc_render::{Fonts, Layers, Painter, Picture, build_layers, read_pixels, target_texture, vello, wgpu};

/// Everything the output needs to draw one picture. Immutable once sent.
#[derive(Clone)]
pub struct Frame {
    /// The picture behind the words, if any.
    pub media: Option<Picture>,
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// When false the output is black whatever the slide is.
    pub enabled: bool,
    /// How long the picture before fades into this one, in seconds. Zero switches at once.
    pub dissolve: f32,
    /// The slide after the one on screen, for stage screens.
    pub next: Option<Arc<Slide>>,
}

/// What is shown over the slides: a message on the audience screens; the stage message and the
/// timers on the stage screens.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// The message's words, tokens and all; they are filled in as it is drawn.
    pub message: Option<String>,
    pub stage_message: Option<String>,
    pub timers: Vec<crate::timers::Timer>,
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
    /// What is shown over the slides.
    Overlay(Overlay),
    /// Which screens are stage screens, and whether stage screens are on.
    Stage {
        screens: Vec<Uuid>,
        on: bool,
    },
    /// Show each screen's own name on it, so the operator can tell them apart; an empty list
    /// goes back to the show.
    Identify(Vec<(Uuid, String)>),
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

/// The size of the picture sent over NDI, whatever size any window is.
const NDI_SIZE: (u32, u32) = (1920, 1080);

type NdiStatuses = Arc<Mutex<Vec<(Uuid, NdiStatus)>>>;

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
    ndi_status: NdiStatuses,
}

impl OutputHandle {
    /// Starts the thread. It shows nothing until it is given a window or an NDI name.
    pub fn spawn(device: wgpu::Device, queue: wgpu::Queue, theme: Arc<Theme>) -> Result<OutputHandle, String> {
        let (tx, rx) = channel();
        let ndi_status: NdiStatuses = Arc::default();
        let status = ndi_status.clone();
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || run(device, queue, theme, rx, status))
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread), ndi_status })
    }

    /// Never blocks. Returns false when the thread has ended and the message went nowhere.
    pub fn send(&self, msg: OutputMsg) -> bool {
        self.tx.send(msg).is_ok()
    }

    /// What a screen's NDI output is doing now.
    pub fn ndi_status(&self, screen: Uuid) -> NdiStatus {
        let statuses = self.ndi_status.lock().map(|all| all.clone()).unwrap_or_default();
        statuses.into_iter().find(|(id, _)| *id == screen).map(|(_, status)| status).unwrap_or_default()
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
type Content = (Option<Picture>, Option<Arc<Slide>>, Option<Arc<Slide>>, Arc<Theme>);

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
}

struct Output {
    device: wgpu::Device,
    queue: wgpu::Queue,
    painter: Painter,
    fonts: Fonts,
    frame: Frame,
    targets: Vec<Target>,
    /// When the fade under way began and how long it takes. Every window fades by this one
    /// clock, so they stay in step.
    fading: Option<(Instant, f32)>,
    /// The NDI sources that are switched on, and the texture their picture is drawn into.
    ndi: Vec<(Uuid, NdiOutput)>,
    ndi_target: Option<wgpu::Texture>,
    ndi_status: NdiStatuses,
    /// While screens are being identified: the name each one shows instead of the show.
    identify: Vec<(Uuid, Arc<Slide>)>,
    identify_theme: Arc<Theme>,
    /// The stage screens, whether they are on, and the layout they are drawn with.
    stage: Vec<Uuid>,
    stage_on: bool,
    stage_theme: Arc<Theme>,
    overlay: Overlay,
}

fn run(device: wgpu::Device, queue: wgpu::Queue, theme: Arc<Theme>, rx: Receiver<OutputMsg>, ndi_status: NdiStatuses) {
    let painter = match Painter::new(&device) {
        Ok(painter) => painter,
        Err(e) => return eprintln!("The output cannot draw on this computer: {e}"),
    };
    let mut output = Output {
        painter,
        fonts: Fonts::new(),
        frame: Frame { media: None, slide: None, theme, enabled: true, dissolve: 0.0, next: None },
        targets: Vec::new(),
        fading: None,
        ndi: Vec::new(),
        ndi_target: None,
        ndi_status,
        identify: Vec::new(),
        identify_theme: Arc::new(Theme::builtin()),
        stage: Vec::new(),
        stage_on: true,
        stage_theme: Arc::new(crate::stage::theme()),
        overlay: Overlay::default(),
        device,
        queue,
    };

    loop {
        // While a fade is under way, look for messages without waiting, so it keeps moving;
        // otherwise sleep until there is one.
        let first = if output.fading.is_some() {
            match rx.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
            }
        } else if output.ticks() {
            // Stage screens show the time: wake at the start of each second to redraw them.
            let into_second = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(Duration::ZERO, |since| Duration::from_nanos(u64::from(since.subsec_nanos())));
            match rx.recv_timeout(Duration::from_secs(1).saturating_sub(into_second) + Duration::from_millis(5)) {
                Ok(msg) => Some(msg),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if output.overlay.message.as_deref().is_some_and(crate::messages::has_tokens) {
                        output.draw(false);
                    } else {
                        output.draw_stage();
                    }
                    continue;
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
            }
        } else {
            match rx.recv() {
                Ok(msg) => Some(msg),
                Err(_) => return,
            }
        };
        let Some(first) = first else {
            // Drawing waits for the screen's next refresh, which paces the fade.
            output.step_fade();
            continue;
        };
        // Take everything that has queued up, so a burst of changes costs one draw.
        let mut next = Some(first);
        let mut new_picture = false;
        while let Some(msg) = next {
            match msg {
                OutputMsg::Show(frame) => {
                    output.frame = frame;
                    new_picture = true;
                }
                OutputMsg::AddSurface { id, surface, format, w, h } => output.add_surface(id, surface, format, w, h),
                OutputMsg::Resized(id, w, h) => output.resize(id, w, h),
                OutputMsg::Redraw => {}
                OutputMsg::SetNdi { id, name } => output.set_ndi(id, name),
                OutputMsg::RemoveSurface(id) => output.targets.retain(|target| target.id != id),
                OutputMsg::Overlay(overlay) => output.overlay = overlay,
                OutputMsg::Stage { screens, on } => {
                    output.stage = screens;
                    output.stage_on = on;
                }
                OutputMsg::Identify(names) => {
                    output.identify = names
                        .into_iter()
                        .map(|(id, name)| (id, Arc::new(Slide::new(vec![Object::text(Role::primary(), name)]))))
                        .collect();
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
        surface.configure(&self.device, &config);
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
        target.surface.configure(&self.device, &target.config);
        target.texture = target_texture(&self.device, w, h);
        // Pictures built for the old size cannot be faded at the new one.
        target.shown = None;
        target.fade = None;
    }

    /// Starts, renames or stops a screen's NDI source and records how that went for the operator.
    fn set_ndi(&mut self, id: Uuid, name: Option<String>) {
        // Stop the old source first, so a renamed one does not appear twice.
        self.ndi.retain(|(have, _)| *have != id);
        let status = name.map(|name| match NdiOutput::start(&name) {
            Ok(ndi) => {
                let status = NdiStatus::On(ndi.name().to_owned());
                self.ndi.push((id, ndi));
                status
            }
            Err(reason) => NdiStatus::Failed(reason),
        });
        if self.ndi.is_empty() {
            self.ndi_target = None;
        }
        if let Ok(mut shared) = self.ndi_status.lock() {
            shared.retain(|(have, _)| *have != id);
            shared.extend(status.map(|status| (id, status)));
        }
    }

    /// What a screen shows now: its own name while screens are being identified, the show
    /// otherwise. Cheap copies: the picture, the slide and the theme are shared.
    fn content_for(&self, id: Uuid, frame: &Frame) -> Content {
        if let Some((_, name)) = self.identify.iter().find(|(have, _)| *have == id) {
            return (None, Some(name.clone()), None, self.identify_theme.clone());
        }
        if self.stage.contains(&id) {
            let words = self
                .stage_on
                .then(|| Arc::new(crate::stage::slide(frame.slide.as_deref(), frame.next.as_deref(), &self.overlay)));
            return (None, words, None, self.stage_theme.clone());
        }
        if !frame.enabled {
            return (None, None, None, frame.theme.clone());
        }
        // A message shows over whatever is on the audience screens.
        let message = self.overlay.message.as_deref().map(|text| {
            let words =
                crate::messages::fill_in(text, &self.overlay.timers, Instant::now(), crate::timers::seconds_into_day());
            Arc::new(crate::messages::slide(&words))
        });
        (frame.media.clone(), frame.slide.clone(), message, frame.theme.clone())
    }

    /// Builds what a screen shows now for a target of a size.
    fn scene_for(&mut self, id: Uuid, frame: &Frame, w: u32, h: u32) -> vello::Scene {
        let (media, slide, overlay, theme) = self.content_for(id, frame);
        let layers = Layers { media: media.as_ref(), slide: slide.as_deref(), overlay: overlay.as_deref() };
        build_layers(&mut self.fonts, &layers, &theme, w, h).scene
    }

    /// Draws the picture at NDI's size and hands it to every NDI source. This does not depend
    /// on any window: with none open, NDI still sends.
    fn send_ndi(&mut self, frame: &Frame) {
        let (w, h) = NDI_SIZE;
        let ids: Vec<Uuid> = self.ndi.iter().map(|(id, _)| *id).collect();
        // The show is drawn once and sent to all; a screen being identified gets its own picture.
        let mut shared: Option<Vec<u8>> = None;
        for id in ids {
            let own = self.identify.iter().any(|(have, _)| *have == id) || self.stage.contains(&id);
            let pixels = match shared.as_ref().filter(|_| !own) {
                Some(pixels) => pixels.clone(),
                None => {
                    let scene = self.scene_for(id, frame, w, h);
                    let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h));
                    let view = target.create_view(&wgpu::TextureViewDescriptor::default());
                    if let Err(e) = self.painter.paint(&self.device, &self.queue, &scene, &view, w, h) {
                        return eprintln!("The NDI picture could not be drawn: {e}");
                    }
                    match read_pixels(&self.device, &self.queue, target) {
                        Ok(pixels) => {
                            if !own {
                                shared = Some(pixels.clone());
                            }
                            pixels
                        }
                        Err(e) => return eprintln!("The NDI picture could not be read back: {e}"),
                    }
                }
            };
            if let Some((_, ndi)) = self.ndi.iter().find(|(have, _)| *have == id) {
                ndi.show(w, h, pixels);
            }
        }
    }

    /// Draws the current frame everywhere. A new picture fades in over the one on screen when
    /// the frame asks for a dissolve; anything else (a resize, an uncovered window) is drawn
    /// at once.
    fn draw(&mut self, new_picture: bool) {
        // Cheap copies: the picture and the slide are shared, not duplicated.
        let frame = self.frame.clone();
        self.send_ndi(&frame);
        // How far a fade already under way has got, before the clock is started again.
        let amount = self.fading.map(|(started, seconds)| fade_amount(started, seconds));
        let mut fading = false;
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            let scene = self.scene_for(id, &frame, w, h);
            // What is in the window this instant: the last whole picture, or a fade part-way.
            let target = &mut self.targets[index];
            let before = match target.fade.take() {
                Some((from, to)) => Some(pc_render::cross_fade(&from, &to, amount.unwrap_or(1.0), w, h)),
                None => target.shown.take(),
            };
            // Stage screens change at once: the people on stage need the words now.
            let stage = self.stage.contains(&id);
            match before {
                Some(from) if new_picture && frame.dissolve > 0.0 && !stage => {
                    target.fade = Some((from, scene));
                    fading = true;
                }
                _ => {
                    self.present(index, &scene);
                    self.targets[index].shown = Some(scene);
                }
            }
        }
        self.fading = fading.then(|| (Instant::now(), frame.dissolve));
        if fading {
            self.step_fade();
        }
    }

    /// True when something on the screens changes by itself each second: the time on a stage
    /// screen, or a message with the time or a timer in it.
    fn ticks(&self) -> bool {
        self.has_stage() || self.overlay.message.as_deref().is_some_and(crate::messages::has_tokens)
    }

    /// True when there is a stage screen to keep the time on.
    fn has_stage(&self) -> bool {
        self.stage_on
            && (self.targets.iter().any(|target| self.stage.contains(&target.id))
                || self.ndi.iter().any(|(id, _)| self.stage.contains(id)))
    }

    /// Draws the stage screens again, for the time, leaving the others as they are.
    fn draw_stage(&mut self) {
        let frame = self.frame.clone();
        for index in 0..self.targets.len() {
            let (id, w, h) = {
                let target = &self.targets[index];
                (target.id, target.config.width, target.config.height)
            };
            if !self.stage.contains(&id) {
                continue;
            }
            let scene = self.scene_for(id, &frame, w, h);
            self.present(index, &scene);
            self.targets[index].shown = Some(scene);
        }
        let (w, h) = NDI_SIZE;
        let stage_ndi: Vec<Uuid> = self.ndi.iter().map(|(id, _)| *id).filter(|id| self.stage.contains(id)).collect();
        for id in stage_ndi {
            let scene = self.scene_for(id, &frame, w, h);
            let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h));
            let view = target.create_view(&wgpu::TextureViewDescriptor::default());
            if let Err(e) = self.painter.paint(&self.device, &self.queue, &scene, &view, w, h) {
                return eprintln!("The NDI picture could not be drawn: {e}");
            }
            match read_pixels(&self.device, &self.queue, target) {
                Ok(pixels) => {
                    if let Some((_, ndi)) = self.ndi.iter().find(|(have, _)| *have == id) {
                        ndi.show(w, h, pixels);
                    }
                }
                Err(e) => return eprintln!("The NDI picture could not be read back: {e}"),
            }
        }
    }

    /// Draws the next step of the fade under way in every window, and finishes it when its
    /// time is up.
    fn step_fade(&mut self) {
        let Some((started, seconds)) = self.fading else { return };
        let amount = fade_amount(started, seconds);
        for index in 0..self.targets.len() {
            let Some((from, to)) = self.targets[index].fade.take() else { continue };
            if amount >= 1.0 {
                self.present(index, &to);
                self.targets[index].shown = Some(to);
            } else {
                let (w, h) = (self.targets[index].config.width, self.targets[index].config.height);
                let scene = pc_render::cross_fade(&from, &to, amount, w, h);
                self.present(index, &scene);
                self.targets[index].fade = Some((from, to));
            }
        }
        if amount >= 1.0 || self.targets.iter().all(|target| target.fade.is_none()) {
            self.fading = None;
        }
    }

    /// Paints a picture and shows it in one window.
    fn present(&mut self, index: usize, scene: &vello::Scene) {
        let Output { device, queue, painter, targets, .. } = self;
        let Some(target) = targets.get_mut(index) else { return };
        let (w, h) = (target.config.width, target.config.height);
        let view = target.texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = painter.paint(device, queue, scene, &view, w, h) {
            return eprintln!("The output could not draw a frame: {e}");
        }
        // A lost or outdated surface is set up again and tried once more; anything else
        // (hidden window, timeout) skips this frame and the next message redraws.
        for attempt in 0..2 {
            match target.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    target.blitter.copy(device, &mut encoder, &view, &surface_view);
                    queue.submit([encoder.finish()]);
                    queue.present(frame);
                    return;
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                    target.surface.configure(device, &target.config);
                }
                // A hidden window shows nothing; wait a moment so a fade does not spin.
                _ => return std::thread::sleep(Duration::from_millis(8)),
            }
        }
    }
}
