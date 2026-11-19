//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pc_model::{Slide, Theme};
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
}

pub enum OutputMsg {
    Show(Frame),
    /// The output's surface changed size, in pixels.
    Resized(u32, u32),
    /// Draw the current frame again: the window was uncovered or asked to be redrawn.
    Redraw,
    /// Start sending the screen over NDI under this name, or stop with `None`.
    Ndi(Option<String>),
    Shutdown,
}

/// What the NDI output is doing, for the operator.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum NdiStatus {
    #[default]
    Off,
    /// Sending under this name.
    On(String),
    /// Asked for, but it could not start; the reason in plain words.
    Failed(String),
}

/// The size of the picture sent over NDI, whatever size the output window is.
const NDI_SIZE: (u32, u32) = (1920, 1080);

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
    ndi_status: Arc<Mutex<NdiStatus>>,
}

impl OutputHandle {
    /// Starts the thread, which takes ownership of the surface.
    pub fn spawn(
        device: wgpu::Device,
        queue: wgpu::Queue,
        surface: wgpu::Surface<'static>,
        format: wgpu::TextureFormat,
        w: u32,
        h: u32,
        theme: Arc<Theme>,
    ) -> Result<OutputHandle, String> {
        let (tx, rx) = channel();
        let ndi_status = Arc::new(Mutex::new(NdiStatus::Off));
        let status = ndi_status.clone();
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || run(device, queue, surface, format, w, h, theme, rx, status))
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread), ndi_status })
    }

    /// Never blocks. Returns false when the thread has ended and the message went nowhere.
    pub fn send(&self, msg: OutputMsg) -> bool {
        self.tx.send(msg).is_ok()
    }

    /// What the NDI output is doing now.
    pub fn ndi_status(&self) -> NdiStatus {
        self.ndi_status.lock().map(|status| status.clone()).unwrap_or_default()
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

/// One picture fading into the next.
struct Fade {
    from: vello::Scene,
    to: vello::Scene,
    started: Instant,
    seconds: f32,
}

struct Output {
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    blitter: wgpu::util::TextureBlitter,
    painter: Painter,
    fonts: Fonts,
    target: wgpu::Texture,
    frame: Frame,
    /// The picture now on the screen in full, kept so the next one can fade in over it.
    shown: Option<vello::Scene>,
    /// A fade under way.
    fade: Option<Fade>,
    /// The NDI source, while it is switched on, and the texture its picture is drawn into.
    ndi: Option<NdiOutput>,
    ndi_target: Option<wgpu::Texture>,
    ndi_status: Arc<Mutex<NdiStatus>>,
}

#[expect(clippy::too_many_arguments, reason = "one call site; these are the thread's whole world")]
fn run(
    device: wgpu::Device,
    queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    format: wgpu::TextureFormat,
    w: u32,
    h: u32,
    theme: Arc<Theme>,
    rx: Receiver<OutputMsg>,
    ndi_status: Arc<Mutex<NdiStatus>>,
) {
    let painter = match Painter::new(&device) {
        Ok(painter) => painter,
        Err(e) => return eprintln!("The output cannot draw on this computer: {e}"),
    };
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
    surface.configure(&device, &config);
    let mut output = Output {
        blitter: wgpu::util::TextureBlitter::new(&device, format),
        target: target_texture(&device, config.width, config.height),
        painter,
        fonts: Fonts::new(),
        frame: Frame { media: None, slide: None, theme, enabled: true, dissolve: 0.0 },
        shown: None,
        fade: None,
        ndi: None,
        ndi_target: None,
        ndi_status,
        device,
        queue,
        surface,
        config,
    };
    output.draw(false);

    loop {
        // While a fade is under way, look for messages without waiting, so it keeps moving;
        // otherwise sleep until there is one.
        let first = if output.fade.is_some() {
            match rx.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return,
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
                OutputMsg::Resized(w, h) => output.resize(w, h),
                OutputMsg::Redraw => {}
                OutputMsg::Ndi(name) => output.set_ndi(name),
                OutputMsg::Shutdown => return,
            }
            next = rx.try_recv().ok();
        }
        output.draw(new_picture);
    }
}

impl Output {
    fn resize(&mut self, w: u32, h: u32) {
        // A minimised window reports zero; keep the last real size until it comes back.
        if w == 0 || h == 0 || (w == self.config.width && h == self.config.height) {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
        self.target = target_texture(&self.device, w, h);
        // Pictures built for the old size cannot be faded at the new one.
        self.shown = None;
        self.fade = None;
    }

    /// Starts or stops the NDI source and records how that went for the operator.
    fn set_ndi(&mut self, name: Option<String>) {
        // Stop the old source first, so a renamed one does not appear twice.
        self.ndi = None;
        let status = match name {
            None => {
                self.ndi_target = None;
                NdiStatus::Off
            }
            Some(name) => match NdiOutput::start(&name) {
                Ok(ndi) => {
                    let status = NdiStatus::On(ndi.name().to_owned());
                    self.ndi = Some(ndi);
                    status
                }
                Err(reason) => NdiStatus::Failed(reason),
            },
        };
        if let Ok(mut shared) = self.ndi_status.lock() {
            *shared = status;
        }
    }

    /// Draws the screen at NDI's size and hands the picture to the NDI source. This does not
    /// depend on the output window: a hidden or minimised window still sends.
    fn send_ndi(&mut self, layers: &Layers) {
        let Some(ndi) = &self.ndi else { return };
        let (w, h) = NDI_SIZE;
        let target = self.ndi_target.get_or_insert_with(|| target_texture(&self.device, w, h));
        let built = build_layers(&mut self.fonts, layers, &self.frame.theme, w, h);
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&self.device, &self.queue, &built.scene, &view, w, h) {
            return eprintln!("The NDI picture could not be drawn: {e}");
        }
        match read_pixels(&self.device, &self.queue, target) {
            Ok(pixels) => ndi.show(w, h, pixels),
            Err(e) => eprintln!("The NDI picture could not be read back: {e}"),
        }
    }

    /// Draws the current frame. A new picture fades in over the one on screen when the frame
    /// asks for a dissolve; anything else (a resize, an uncovered window) is drawn at once.
    fn draw(&mut self, new_picture: bool) {
        let (w, h) = (self.config.width, self.config.height);
        // Cheap copies: the picture and the slide are shared, not duplicated.
        let frame = self.frame.clone();
        let layers = if frame.enabled {
            Layers { media: frame.media.as_ref(), slide: frame.slide.as_deref() }
        } else {
            Layers::default()
        };
        self.send_ndi(&layers);
        let built = build_layers(&mut self.fonts, &layers, &self.frame.theme, w, h);
        // What is on screen this instant: the last whole picture, or a fade part-way through.
        let before = match self.fade.take() {
            Some(fade) => Some(pc_render::cross_fade(&fade.from, &fade.to, fade.amount(), w, h)),
            None => self.shown.take(),
        };
        match before {
            Some(from) if new_picture && frame.dissolve > 0.0 => {
                self.fade = Some(Fade { from, to: built.scene, started: Instant::now(), seconds: frame.dissolve });
                self.step_fade();
            }
            _ => {
                self.present(&built.scene);
                self.shown = Some(built.scene);
            }
        }
    }

    /// Draws the next step of the fade under way, and finishes it when its time is up.
    fn step_fade(&mut self) {
        let Some(fade) = self.fade.take() else { return };
        let amount = fade.amount();
        if amount >= 1.0 {
            self.present(&fade.to);
            self.shown = Some(fade.to);
        } else {
            let scene = pc_render::cross_fade(&fade.from, &fade.to, amount, self.config.width, self.config.height);
            self.present(&scene);
            self.fade = Some(fade);
        }
    }

    /// Paints a picture and shows it on the screen.
    fn present(&mut self, scene: &vello::Scene) {
        let (w, h) = (self.config.width, self.config.height);
        let view = self.target.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&self.device, &self.queue, scene, &view, w, h) {
            return eprintln!("The output could not draw a frame: {e}");
        }
        // A lost or outdated surface is set up again and tried once more; anything else
        // (hidden window, timeout) skips this frame and the next message redraws.
        for attempt in 0..2 {
            match self.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => {
                    let surface_view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                    let mut encoder =
                        self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                    self.blitter.copy(&self.device, &mut encoder, &view, &surface_view);
                    self.queue.submit([encoder.finish()]);
                    self.queue.present(frame);
                    return;
                }
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost if attempt == 0 => {
                    self.surface.configure(&self.device, &self.config);
                }
                // A hidden window shows nothing; wait a moment so a fade does not spin.
                _ => return std::thread::sleep(Duration::from_millis(8)),
            }
        }
    }
}

impl Fade {
    /// How far in the fade is, from 0 to 1, eased so it starts and ends gently.
    fn amount(&self) -> f32 {
        let t = (self.started.elapsed().as_secs_f32() / self.seconds.max(0.001)).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}
