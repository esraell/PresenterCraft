//! The output thread. It owns the output's surface and its own painter, draws only when told
//! what to show, and never waits on the home window.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;

use pc_model::{Slide, Theme};
use pc_render::{Fonts, Painter, build_scene, target_texture, wgpu};

/// Everything the output needs to draw one picture. Immutable once sent.
#[derive(Clone)]
pub struct Frame {
    pub slide: Option<Arc<Slide>>,
    pub theme: Arc<Theme>,
    /// When false the output is black whatever the slide is.
    pub enabled: bool,
}

pub enum OutputMsg {
    Show(Frame),
    /// The output's surface changed size, in pixels.
    Resized(u32, u32),
    Shutdown,
}

pub struct OutputHandle {
    tx: Sender<OutputMsg>,
    thread: Option<JoinHandle<()>>,
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
        let thread = std::thread::Builder::new()
            .name("output".to_owned())
            .spawn(move || run(device, queue, surface, format, w, h, theme, rx))
            .map_err(|e| format!("could not start the output thread: {e}"))?;
        Ok(OutputHandle { tx, thread: Some(thread) })
    }

    /// Never blocks. A message to a thread that has ended is dropped.
    pub fn send(&self, msg: OutputMsg) {
        let _ = self.tx.send(msg);
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
        frame: Frame { slide: None, theme, enabled: true },
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

    fn draw(&mut self) {
        let (w, h) = (self.config.width, self.config.height);
        let slide = if self.frame.enabled { self.frame.slide.as_deref() } else { None };
        let built = build_scene(&mut self.fonts, slide, &self.frame.theme, w, h);
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
