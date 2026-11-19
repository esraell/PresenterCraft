//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use pc_model::{Slide, Theme};
use pc_ndi::NdiOutput;
use pc_render::{Fonts, Layers, Painter, Picture, build_layers, read_pixels, target_texture, wgpu};

/// Everything the output needs to draw one picture. Immutable once sent.
#[derive(Clone)]
pub struct Frame {
    /// The picture behind the words, if any.
    pub media: Option<Picture>,
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// When false the output is black whatever the slide is.
    pub enabled: bool,
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
        frame: Frame { media: None, slide: None, theme, enabled: true },
        ndi: None,
        ndi_target: None,
        ndi_status,
        device,
        queue,
        surface,
        config,
    };
    output.draw();

    while let Ok(first) = rx.recv() {
        // Take everything that has queued up, so a burst of changes costs one draw.
        let mut next = Some(first);
        while let Some(msg) = next {
            match msg {
                OutputMsg::Show(frame) => output.frame = frame,
                OutputMsg::Resized(w, h) => output.resize(w, h),
                OutputMsg::Redraw => {}
                OutputMsg::Ndi(name) => output.set_ndi(name),
                OutputMsg::Shutdown => return,
            }
            next = rx.try_recv().ok();
        }
        output.draw();
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

    fn draw(&mut self) {
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
        let view = self.target.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&self.device, &self.queue, &built.scene, &view, w, h) {
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
                _ => return,
            }
        }
    }
}
