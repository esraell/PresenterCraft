//! THROWAWAY SPIKE. Not product code.
//!
//! Question: can a native GPU renderer (winit + wgpu + vello + parley) draw a bilingual slide
//! with outlined, shadowed Latin and Cyrillic text, to a PNG and fullscreen on a chosen display?
//!
//!   text-spike --png out.png          render 1920x1080 off screen and save it
//!   text-spike --list                 list displays
//!   text-spike --window               open a window (Esc quits)
//!   text-spike --display 1            borderless fullscreen on display 1 (Esc quits)
//!   ... --seconds 5                   quit by itself after 5 seconds

use std::sync::Arc;
use std::time::{Duration, Instant};

use parley::{
    Alignment, AlignmentOptions, FontContext, FontWeight, Layout, LayoutContext, LineHeight,
    PositionedLayoutItem, StyleProperty,
};
use vello::kurbo::{Affine, Join, Point, Rect, Stroke};
use vello::peniko::{Color, Fill, Gradient};
use vello::util::{RenderContext, RenderSurface};
use vello::wgpu;
use vello::{AaConfig, AaSupport, Glyph, RenderParams, Renderer, RendererOptions, Scene};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

/// Slides are authored at this size and scaled to whatever the output is.
const SLIDE_W: f64 = 1920.0;
const SLIDE_H: f64 = 1080.0;

struct TextBox<'a> {
    text: &'a str,
    size: f32,
    color: Color,
    /// Top of the box in slide units.
    top: f64,
    outline: f64,
}

struct Fonts {
    font_cx: FontContext,
    layout_cx: LayoutContext<()>,
}

fn build_scene(fonts: &mut Fonts, target_w: u32, target_h: u32) -> Scene {
    let mut scene = Scene::new();
    // Fit the 16:9 slide inside the target, centred (letterbox).
    let scale = (target_w as f64 / SLIDE_W).min(target_h as f64 / SLIDE_H);
    let dx = (target_w as f64 - SLIDE_W * scale) / 2.0;
    let dy = (target_h as f64 - SLIDE_H * scale) / 2.0;
    let slide = Affine::translate((dx, dy)) * Affine::scale(scale);

    // Background: dark to bright, so the outline is tested on both.
    let bg = Gradient::new_linear(Point::new(0.0, 0.0), Point::new(SLIDE_W, SLIDE_H)).with_stops([
        Color::from_rgb8(8, 16, 40),
        Color::from_rgb8(40, 70, 120),
        Color::from_rgb8(245, 225, 180),
    ]);
    scene.fill(Fill::NonZero, slide, &bg, None, &Rect::new(0.0, 0.0, SLIDE_W, SLIDE_H));

    let boxes = [
        TextBox {
            text: "Light rises over the quiet hills\nAnd the morning sings again",
            size: 96.0,
            color: Color::WHITE,
            top: 250.0,
            outline: 6.0,
        },
        TextBox {
            text: "Свет встаёт над тихими холмами\nІ ранок знову співає: їжак, ґанок, є",
            size: 64.0,
            color: Color::from_rgb8(255, 150, 40),
            top: 610.0,
            outline: 4.0,
        },
    ];
    for b in &boxes {
        draw_text_box(&mut scene, fonts, slide, b);
    }
    scene
}

fn draw_text_box(scene: &mut Scene, fonts: &mut Fonts, slide: Affine, b: &TextBox) {
    let margin = 120.0;
    let width = (SLIDE_W - 2.0 * margin) as f32;
    let mut builder = fonts.layout_cx.ranged_builder(&mut fonts.font_cx, b.text, 1.0, true);
    builder.push_default(StyleProperty::FontSize(b.size));
    builder.push_default(StyleProperty::FontWeight(FontWeight::new(700.0)));
    builder.push_default(StyleProperty::LineHeight(LineHeight::FontSizeRelative(1.25)));
    builder.push_default(StyleProperty::FontFamily("Helvetica Neue".into()));
    let mut layout: Layout<()> = builder.build(b.text);
    layout.set_alignment(Alignment::Center, AlignmentOptions::default());
    layout.break_all_lines(Some(width));

    let origin = slide * Affine::translate((margin, b.top));
    let outline = Stroke::new(b.outline * 2.0).with_join(Join::Round);
    let black = Color::BLACK;

    // Three passes over the same glyphs: soft shadow, outline, fill.
    for pass in 0..3 {
        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(run) = item else { continue };
                let font = run.run().font();
                let size = run.run().font_size();
                let coords: Vec<i16> = run.run().normalized_coords().iter().map(|c| c.to_bits()).collect();
                let coords = coords.as_slice();
                let glyphs: Vec<Glyph> =
                    run.positioned_glyphs().map(|g| Glyph { id: g.id as u32, x: g.x, y: g.y }).collect();
                match pass {
                    0 => {
                        // The renderer has no blur for glyphs, so fake a soft shadow with a few
                        // widening, faint strokes, offset down and right.
                        let shadow = origin * Affine::translate((6.0, 8.0));
                        for i in 1..=6 {
                            let s = Stroke::new(b.outline * 2.0 + i as f64 * 5.0).with_join(Join::Round);
                            scene
                                .draw_glyphs(font)
                                .font_size(size)
                                .transform(shadow)
                                .normalized_coords(coords)
                                .brush(black)
                                .brush_alpha(0.10)
                                .draw(&s, glyphs.iter().copied());
                        }
                    }
                    1 => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .normalized_coords(coords)
                        .brush(black)
                        .draw(&outline, glyphs.iter().copied()),
                    _ => scene
                        .draw_glyphs(font)
                        .font_size(size)
                        .transform(origin)
                        .normalized_coords(coords)
                        .brush(b.color)
                        .draw(Fill::NonZero, glyphs.iter().copied()),
                }
            }
        }
    }
}

fn new_renderer(device: &wgpu::Device) -> Renderer {
    Renderer::new(
        device,
        RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: None,
        },
    )
    .expect("renderer")
}

fn params(w: u32, h: u32) -> RenderParams {
    RenderParams { base_color: Color::BLACK, width: w, height: h, antialiasing_method: AaConfig::Area }
}

/// Render off screen and read the pixels back: the same path NDI output would use.
fn render_png(path: &str) {
    let (w, h) = (1920u32, 1080u32);
    let mut cx = RenderContext::new();
    let id = pollster::block_on(cx.device(None)).expect("no GPU device");
    let device = &cx.devices[id].device;
    let queue = &cx.devices[id].queue;
    let mut renderer = new_renderer(device);
    let mut fonts = Fonts { font_cx: FontContext::new(), layout_cx: LayoutContext::new() };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("offscreen"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

    let t0 = Instant::now();
    let scene = build_scene(&mut fonts, w, h);
    let t_scene = t0.elapsed();
    renderer.render_to_texture(device, queue, &scene, &view, &params(w, h)).expect("render");

    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: (w * h * 4) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(w * 4), rows_per_image: None },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().expect("map");
    let data = slice.get_mapped_range().expect("mapped range").to_vec();
    let t_total = t0.elapsed();

    let file = std::fs::File::create(path).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&data).unwrap();
    println!("wrote {path}: {w}x{h}; scene {t_scene:?}, scene+render+readback {t_total:?}");
}

struct Opts {
    display: Option<usize>,
    seconds: Option<u64>,
    list: bool,
}

struct Active {
    window: Arc<Window>,
    surface: RenderSurface<'static>,
    renderer: Renderer,
}

struct App {
    opts: Opts,
    cx: RenderContext,
    fonts: Fonts,
    active: Option<Active>,
    started: Instant,
    frames: u32,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.active.is_some() {
            return;
        }
        let monitors: Vec<_> = el.available_monitors().collect();
        for (i, m) in monitors.iter().enumerate() {
            let s = m.size();
            let p = m.position();
            println!(
                "display {i}: {} {}x{} at {},{} scale {}",
                m.name().unwrap_or_default(),
                s.width,
                s.height,
                p.x,
                p.y,
                m.scale_factor()
            );
        }
        if self.opts.list {
            el.exit();
            return;
        }
        let mut attrs = Window::default_attributes().with_title("SoftPresenter text spike");
        match self.opts.display {
            Some(i) => match monitors.get(i) {
                Some(m) => attrs = attrs.with_fullscreen(Some(Fullscreen::Borderless(Some(m.clone())))),
                None => {
                    eprintln!("no display {i}");
                    el.exit();
                    return;
                }
            },
            None => attrs = attrs.with_inner_size(winit::dpi::LogicalSize::new(1280.0, 720.0)),
        }
        let window = Arc::new(el.create_window(attrs).expect("window"));
        let size = window.inner_size();
        let surface = pollster::block_on(self.cx.create_surface(
            window.clone(),
            size.width.max(1),
            size.height.max(1),
            wgpu::PresentMode::AutoVsync,
        ))
        .expect("surface");
        let renderer = new_renderer(&self.cx.devices[surface.dev_id].device);
        println!("surface {}x{} format {:?}", size.width, size.height, surface.format);
        self.active = Some(Active { window, surface, renderer });
        self.started = Instant::now();
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(active) = self.active.as_mut() else { return };
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::KeyboardInput {
                event: KeyEvent { logical_key: Key::Named(NamedKey::Escape), state: ElementState::Pressed, .. },
                ..
            } => el.exit(),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    self.cx.resize_surface(&mut active.surface, size.width, size.height);
                    println!("resized to {}x{}", size.width, size.height);
                }
                active.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let (w, h) = (active.surface.config.width, active.surface.config.height);
                let dev = &self.cx.devices[active.surface.dev_id];
                let scene = build_scene(&mut self.fonts, w, h);
                active
                    .renderer
                    .render_to_texture(&dev.device, &dev.queue, &scene, &active.surface.target_view, &params(w, h))
                    .expect("render");
                let frame = match active.surface.surface.get_current_texture() {
                    wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
                    other => {
                        eprintln!("no surface texture: {other:?}");
                        return;
                    }
                };
                let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
                let mut enc = dev.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                active.surface.blitter.copy(&dev.device, &mut enc, &active.surface.target_view, &view);
                dev.queue.submit([enc.finish()]);
                dev.queue.present(frame);
                self.frames += 1;
                active.window.request_redraw();
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        if let Some(s) = self.opts.seconds {
            if self.active.is_some() && self.started.elapsed() >= Duration::from_secs(s) {
                let secs = self.started.elapsed().as_secs_f64();
                println!("{} frames in {:.1}s = {:.0} fps", self.frames, secs, self.frames as f64 / secs);
                el.exit();
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if let Some(path) = value("--png") {
        render_png(&path);
        return;
    }
    let opts = Opts {
        display: value("--display").and_then(|v| v.parse().ok()),
        seconds: value("--seconds").and_then(|v| v.parse().ok()),
        list: args.iter().any(|a| a == "--list"),
    };
    let event_loop = EventLoop::new().expect("event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        opts,
        cx: RenderContext::new(),
        fonts: Fonts { font_cx: FontContext::new(), layout_cx: LayoutContext::new() },
        active: None,
        started: Instant::now(),
        frames: 0,
    };
    event_loop.run_app(&mut app).expect("run");
}
