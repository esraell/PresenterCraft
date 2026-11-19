//! PresenterCraft: a home window with a slide grid, and a separate output for the audience.

mod displays;
mod editor;
mod home;
mod icons;
mod inspector;
mod load;
mod media;
mod output;
mod thumbs;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pc_engine::{Command, Layer, Show};
use pc_model::{MediaItem, Slide, Theme, Uuid, parse_song_text};
use pc_render::vello::util::RenderContext;
use pc_render::wgpu;
use pc_render::{Layers, Picture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::displays::{DisplayInfo, Placement, choose_output};
use crate::editor::EditDraft;
use crate::home::{HomeState, ThumbOf, home_ui};
use crate::output::{Frame, OutputHandle, OutputMsg};
use crate::thumbs::Thumbs;

/// The GPU device and its queue: one per process, shared with the output thread.
pub struct Gpu {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

const HELP: &str = "\
PresenterCraft

Usage: presentercraft [options] [song.txt | folder | picture]...

Songs are text files; pictures (PNG, JPEG, WebP, BMP, GIF) go into the media bin.
With no songs, a sample song is loaded. Pictures can also be dropped onto the window.

Options:
  --list-displays        Print the connected displays and quit
  --output-display N     Put the output fullscreen on display N
  --capture-home FILE    For testing: go to the third slide, save a picture of the
                         home window to FILE (PNG) and quit
  -h, --help             Show this help

Keys: Right or Space next, Left previous, Home first, End last, C clear slide,
      F1 clear all.
";

#[derive(Debug)]
struct Options {
    list_displays: bool,
    output_display: Option<usize>,
    capture_home: Option<PathBuf>,
    paths: Vec<PathBuf>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Option<Options>, String> {
    let mut options = Options { list_displays: false, output_display: None, capture_home: None, paths: Vec::new() };
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(None),
            "--list-displays" => options.list_displays = true,
            "--output-display" => {
                let value = args.next().ok_or("--output-display needs a display number")?;
                let index =
                    value.parse().map_err(|_| format!("--output-display: '{value}' is not a display number"))?;
                options.output_display = Some(index);
            }
            "--capture-home" => {
                options.capture_home = Some(PathBuf::from(args.next().ok_or("--capture-home needs a file name")?));
            }
            other if other.starts_with("--") => {
                return Err(format!("Unknown option {other}; try --help."));
            }
            path => options.paths.push(PathBuf::from(path)),
        }
    }
    Ok(Some(options))
}

/// The keys that run the show. They are read straight from the window, before the interface
/// draws, so a key press reaches the output without waiting for the home window.
fn show_key_command(key: &Key) -> Option<Command> {
    match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::Space) => Some(Command::Next),
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Previous),
        Key::Named(NamedKey::Home) => Some(Command::First),
        Key::Named(NamedKey::End) => Some(Command::Last),
        Key::Named(NamedKey::F1) => Some(Command::ClearAll),
        Key::Character(c) if c.eq_ignore_ascii_case("c") => Some(Command::Clear(Layer::Slides)),
        _ => None,
    }
}

fn surface_format(surface: &wgpu::Surface<'_>, adapter: &wgpu::Adapter) -> Result<wgpu::TextureFormat, String> {
    surface
        .get_capabilities(adapter)
        .formats
        .into_iter()
        .find(|f| matches!(f, wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Rgba8Unorm))
        .ok_or_else(|| "This display does not offer a colour format PresenterCraft can draw to.".to_owned())
}

/// Adds a wide-coverage system font behind the interface's own, so song titles in any script
/// show as letters. Without it the interface still works with its built-in font.
fn add_fallback_font(ctx: &egui::Context) {
    const CANDIDATES: [&str; 2] =
        ["/System/Library/Fonts/Supplemental/Arial Unicode.ttf", "C:\\Windows\\Fonts\\arialuni.ttf"];
    let Some(bytes) = CANDIDATES.iter().find_map(|path| std::fs::read(path).ok()) else { return };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("fallback".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
    for family in fonts.families.values_mut() {
        family.push("fallback".to_owned());
    }
    ctx.set_fonts(fonts);
}

/// The connected displays as the window system lists them.
fn display_list(monitors: impl Iterator<Item = winit::monitor::MonitorHandle>) -> Vec<DisplayInfo> {
    monitors
        .enumerate()
        .map(|(index, m)| DisplayInfo {
            index,
            name: m.name().unwrap_or_else(|| "Display".to_owned()),
            x: m.position().x,
            y: m.position().y,
            w: m.size().width,
            h: m.size().height,
        })
        .collect()
}

/// Draws the home window's last frame again into a texture and saves it as a PNG.
fn capture(
    gpu: &Gpu,
    egui_renderer: &egui_wgpu::Renderer,
    jobs: &[egui::ClippedPrimitive],
    screen: &egui_wgpu::ScreenDescriptor,
    format: wgpu::TextureFormat,
    path: &std::path::Path,
) -> Result<(), String> {
    let [w, h] = screen.size_in_pixels;
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("home capture"),
        size: wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("home capture"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            })
            .forget_lifetime();
        egui_renderer.render(&mut pass, jobs, screen);
    }
    gpu.queue.submit([encoder.finish()]);
    let mut pixels = pc_render::read_pixels(&gpu.device, &gpu.queue, &texture)?;
    if format == wgpu::TextureFormat::Bgra8Unorm {
        pixels.as_chunks_mut::<4>().0.iter_mut().for_each(|p| p.swap(0, 2));
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut png = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    png.set_color(png::ColorType::Rgba);
    png.set_depth(png::BitDepth::Eight);
    png.write_header().and_then(|mut writer| writer.write_image_data(&pixels)).map_err(|e| e.to_string())
}

/// Everything that exists once the windows are open.
struct Running {
    home_window: Arc<Window>,
    home_surface: wgpu::Surface<'static>,
    home_config: wgpu::SurfaceConfiguration,
    output_window: Arc<Window>,
    output: Option<OutputHandle>,
    gpu: Gpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    thumbs: Thumbs,
    preview: Option<egui::TextureId>,
    home_state: HomeState,
    next_repaint: Option<Instant>,
    /// The Edit view's picture of the slide being edited, and the words it was drawn from.
    canvas: Option<egui::TextureId>,
    canvas_for: Option<(pc_engine::SlideRef, Slide)>,
    frames: u32,
    // Kept so the GPU instance outlives its surfaces.
    _context: RenderContext,
}

struct App {
    options: Options,
    show: Show,
    /// The file each song came from, in the show's order; `None` for the bundled sample.
    sources: Vec<Option<PathBuf>>,
    /// The decoded picture of each media item, in the show's order.
    pictures: Vec<Picture>,
    /// Songs changed in the Edit view and not yet written to their files, and when the last
    /// change was made.
    unsaved: Vec<usize>,
    unsaved_since: Option<Instant>,
    theme: Arc<Theme>,
    running: Option<Running>,
    failed: bool,
    /// Set when a test capture has been saved and the program should end.
    capture_done: bool,
}

impl App {
    fn start(&mut self, el: &ActiveEventLoop) -> Result<Option<Running>, String> {
        let monitors: Vec<_> = el.available_monitors().collect();
        let displays = display_list(monitors.iter().cloned());
        if self.options.list_displays {
            for d in &displays {
                println!("display {}: {} {}x{} at {},{}", d.index, d.name, d.w, d.h, d.x, d.y);
            }
            return Ok(None);
        }

        let home_attrs = Window::default_attributes()
            .with_title("PresenterCraft")
            .with_inner_size(winit::dpi::LogicalSize::new(1440.0, 900.0))
            .with_min_inner_size(winit::dpi::LogicalSize::new(1100.0, 700.0));
        let home_window =
            Arc::new(el.create_window(home_attrs).map_err(|e| format!("Could not open the home window: {e}"))?);
        let home_display =
            home_window.current_monitor().and_then(|home| monitors.iter().position(|m| *m == home)).unwrap_or(0);

        let placement = choose_output(&displays, home_display, self.options.output_display)?;
        let output_attrs = Window::default_attributes().with_title("PresenterCraft Output");
        let output_attrs = match placement {
            Placement::Fullscreen(index) => {
                output_attrs.with_fullscreen(Some(Fullscreen::Borderless(monitors.get(index).cloned())))
            }
            Placement::Windowed => output_attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0)),
        };
        let output_window =
            Arc::new(el.create_window(output_attrs).map_err(|e| format!("Could not open the output window: {e}"))?);
        output_window.set_cursor_visible(false);
        home_window.focus_window();

        let mut context = RenderContext::new();
        let home_surface = context
            .instance
            .create_surface(home_window.clone())
            .map_err(|e| format!("Could not draw to the home window: {e}"))?;
        let device_id = pollster::block_on(context.device(Some(&home_surface)))
            .ok_or("No graphics device was found that PresenterCraft can use.")?;
        let handle = &context.devices[device_id];
        let gpu = Gpu { device: handle.device.clone(), queue: handle.queue.clone() };
        // By default a graphics error ends the program; in a service it is better to say so and carry on.
        gpu.device.on_uncaptured_error(Arc::new(|error| eprintln!("Graphics error: {error}")));
        let home_format = surface_format(&home_surface, handle.adapter())?;

        let output_surface = context
            .instance
            .create_surface(output_window.clone())
            .map_err(|e| format!("Could not draw to the output window: {e}"))?;
        let output_format = surface_format(&output_surface, handle.adapter())?;
        let output_size = output_window.inner_size();
        let output = OutputHandle::spawn(
            gpu.device.clone(),
            gpu.queue.clone(),
            output_surface,
            output_format,
            output_size.width,
            output_size.height,
            self.theme.clone(),
        )?;

        let size = home_window.inner_size();
        let home_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: home_format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            color_space: wgpu::SurfaceColorSpace::Auto,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
        };
        home_surface.configure(&gpu.device, &home_config);

        let egui_ctx = egui::Context::default();
        add_fallback_font(&egui_ctx);
        let mut visuals = egui::Visuals::dark();
        visuals.selection.bg_fill = home::ACCENT.gamma_multiply(0.6);
        visuals.selection.stroke.color = egui::Color32::WHITE;
        egui_ctx.set_visuals(visuals);
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &home_window,
            Some(home_window.scale_factor() as f32),
            None,
            Some(gpu.device.limits().max_texture_dimension_2d as usize),
        );
        let mut egui_renderer =
            egui_wgpu::Renderer::new(&gpu.device, home_format, egui_wgpu::RendererOptions::default());
        let mut thumbs = Thumbs::new(&gpu.device)?;
        let preview = thumbs.preview(&gpu, &mut egui_renderer, &Layers::default(), &self.theme);

        Ok(Some(Running {
            home_window,
            home_surface,
            home_config,
            output_window,
            output: Some(output),
            gpu,
            egui_ctx,
            egui_state,
            egui_renderer,
            thumbs,
            preview,
            home_state: HomeState {
                fonts: Arc::new(pc_render::Fonts::new().family_names()),
                output_placement: placement,
                ..HomeState::default()
            },
            next_repaint: None,
            canvas: None,
            canvas_for: None,
            frames: 0,
            _context: context,
        }))
    }

    /// Applies operator commands and tells the output and the home window what changed.
    fn apply(&mut self, commands: Vec<Command>) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let mut any = false;
        let mut output_changed = false;
        for command in commands {
            let changes = self.show.apply(command);
            output_changed |= changes.slides || changes.media || changes.output_enabled;
            if changes.selection {
                let media: Vec<Uuid> = self.show.media().iter().map(|m| m.id).collect();
                running.thumbs.clear_except(&mut running.egui_renderer, |id| media.contains(id));
            }
            any |= changes.slides || changes.media || changes.output_enabled || changes.selection;
        }
        if output_changed {
            self.refresh_output();
        }
        if any && let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// Tells the output what is on each layer now, then redraws the operator's preview.
    fn refresh_output(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let slide = self.show.live_slide().map(|(_, slide)| Arc::new(slide.clone()));
        let media = self.show.live_media().and_then(|index| self.pictures.get(index)).cloned();
        // The audience comes first: tell the output before drawing anything for the operator.
        if let Some(output) = &running.output {
            let frame = Frame {
                media: media.clone(),
                slide: slide.clone(),
                theme: self.theme.clone(),
                enabled: self.show.output_enabled(),
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_stopped = true;
            }
        }
        let layers = Layers { media: media.as_ref(), slide: slide.as_deref() };
        running.preview = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &self.theme);
    }

    /// Adds a file dropped on the window to the media bin, or says why it could not be added.
    fn add_dropped_file(&mut self, path: &std::path::Path) {
        let result = media::load_picture(path);
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.home_state.drop_hover = false;
        match result {
            Ok(picture) => {
                let name = path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
                self.show.add_media(MediaItem { id: Uuid::new_v4(), name, path: path.to_owned() });
                self.pictures.push(picture);
                running.home_state.notice = None;
            }
            Err(message) => running.home_state.notice = Some(message),
        }
        running.home_window.request_redraw();
    }

    /// Puts a slide changed in the Edit view into its song. The song's file is written a
    /// moment later (see `save_unsaved`). The output is left alone unless Live Edit is on, so
    /// editing never changes what the audience sees by surprise.
    fn apply_edit(&mut self, at: pc_engine::SlideRef, slide: Slide) {
        let Some(song) = self.show.presentations().get(at.presentation).cloned() else {
            return;
        };
        let slide_id = slide.id;
        let Some(edited) = song.with_slide(at.slide, slide) else {
            return;
        };
        let changes = self.show.replace_presentation(at.presentation, Arc::new(edited));
        if !self.unsaved.contains(&at.presentation) {
            self.unsaved.push(at.presentation);
        }
        self.unsaved_since = Some(Instant::now());
        let mut live_edit = false;
        if let Some(running) = self.running.as_mut() {
            running.thumbs.forget(slide_id, &mut running.egui_renderer);
            live_edit = running.home_state.live_edit;
            running.home_window.request_redraw();
        }
        if changes.slides && live_edit {
            self.refresh_output();
        }
    }

    /// Writes every song changed in the Edit view to its file. The bundled sample has no file.
    fn save_unsaved(&mut self) {
        let mut problem = None;
        for index in std::mem::take(&mut self.unsaved) {
            let (Some(Some(path)), Some(song)) = (self.sources.get(index), self.show.presentations().get(index)) else {
                continue;
            };
            match load::save_song(path, song) {
                // From now on the song lives in the file that was just written.
                Ok(written) => self.sources[index] = Some(written),
                Err(e) => problem = Some(format!("The change was not saved. {e}")),
            }
        }
        self.unsaved_since = None;
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = problem;
            running.home_window.request_redraw();
        }
    }

    /// Switches the NDI output on or off as asked in Settings.
    fn switch_ndi(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some(wanted) = running.home_state.requested_ndi.take() else {
            return;
        };
        if let Some(output) = &running.output {
            output.send(OutputMsg::Ndi(wanted));
            // NDI needs the current picture as soon as it starts.
            output.send(OutputMsg::Redraw);
        }
        // Starting NDI takes a moment on the output thread; look again shortly to show how it went.
        running.next_repaint = Some(Instant::now() + Duration::from_millis(400));
    }

    /// Moves the audience output to the display chosen in Settings, or into a window.
    fn move_output(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some(wanted) = running.home_state.requested_placement.take() else {
            return;
        };
        match wanted {
            Placement::Fullscreen(index) => {
                // The display may have been unplugged since the list was drawn.
                let Some(monitor) = running.output_window.available_monitors().nth(index) else {
                    running.home_state.notice = Some(format!("Display {} is no longer connected.", index + 1));
                    return;
                };
                running.output_window.set_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
            }
            Placement::Windowed => running.output_window.set_fullscreen(None),
        }
        running.home_state.output_placement = wanted;
        // The output's size changes with its display; the window tells the output thread.
        running.home_window.request_redraw();
    }

    /// Redraws the Edit view's picture of the slide when the slide being edited changes.
    fn update_canvas(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let editing = running.home_state.editing.as_ref().map(|draft| (draft.at, draft.slide.clone()));
        if editing == running.canvas_for {
            return;
        }
        if let Some((_, slide)) = &editing {
            running.canvas = running.thumbs.canvas(&running.gpu, &mut running.egui_renderer, slide, &self.theme);
        }
        running.canvas_for = editing;
        running.home_window.request_redraw();
    }

    /// Draws one frame of the home window and returns what the operator asked for.
    fn draw_home(&mut self) -> Vec<Command> {
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running {
            home_window, home_surface, home_config, gpu, egui_ctx, egui_state, egui_renderer, thumbs, ..
        } = running;
        let raw_input = egui_state.take_egui_input(home_window);
        thumbs.begin_frame();
        if running.home_state.settings_open {
            running.home_state.displays = display_list(home_window.available_monitors());
        }
        if let Some(output) = &running.output {
            running.home_state.ndi_status = output.ndi_status();
        }
        if running.output.as_ref().is_none_or(OutputHandle::is_finished) {
            running.home_state.output_stopped = true;
        }
        let mut commands = Vec::new();
        let (show, theme, preview, pictures) = (&self.show, &self.theme, running.preview, &self.pictures);
        let canvas = running.canvas;
        let home_state = &mut running.home_state;
        let mut full = egui_ctx.run_ui(raw_input, |ui| {
            // The toolkit may run this closure twice in one frame, and input arrives only the
            // first time, so add to the list rather than replace it.
            let mut thumb = |which: ThumbOf| match which {
                ThumbOf::Slide(slide) => thumbs.get(gpu, egui_renderer, slide, theme),
                ThumbOf::Media(index) => {
                    let (item, picture) = (show.media().get(index)?, pictures.get(index)?);
                    thumbs.get_media(gpu, egui_renderer, item.id, picture, theme).map(|texture| (texture, false))
                }
            };
            commands.extend(home_ui(ui, show, home_state, theme, &mut thumb, preview, canvas));
        });
        egui_state.handle_platform_output(home_window, full.platform_output);

        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // Testing aid: redraw continuously, to check that memory stays flat under load.
        let delay = if std::env::var_os("PRESENTERCRAFT_STRESS").is_some() { Duration::ZERO } else { delay };
        running.next_repaint = Instant::now().checked_add(delay);

        let jobs = egui_ctx.tessellate(full.shapes, full.pixels_per_point);
        for (id, deltas) in &full.textures_delta.set {
            for delta in deltas {
                egui_renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [home_config.width, home_config.height],
            pixels_per_point: full.pixels_per_point,
        };
        let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let extra = egui_renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);

        let frame = match home_surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                home_surface.configure(&gpu.device, home_config);
                home_window.request_redraw();
                None
            }
            _ => None,
        };
        if let Some(frame) = frame {
            let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
            {
                let mut pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("home window"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        ..Default::default()
                    })
                    .forget_lifetime();
                egui_renderer.render(&mut pass, &jobs, &screen);
            }
            gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
            gpu.queue.present(frame);
        } else {
            // No frame to draw into (the window is hidden or covered). The interface's data for
            // this frame has already been handed to the GPU queue, and it stays in memory until
            // something is submitted, so submit it anyway. Skipping this leaked memory on every
            // frame drawn while the window could not be seen.
            gpu.queue.submit(extra.into_iter().chain(std::iter::once(encoder.finish())));
        }
        for id in &full.textures_delta.free {
            egui_renderer.free_texture(id);
        }
        // The toolkit insists on being told its texture changes were applied.
        full.textures_delta.clear();

        // Testing aid: show a slide, let the thumbnails fill in, then save what was drawn.
        running.frames += 1;
        if let Some(path) = &self.options.capture_home {
            const GO_LIVE_AT: u32 = 3;
            const CAPTURE_AT: u32 = 30;
            if running.frames == GO_LIVE_AT {
                commands.push(Command::GoTo(pc_engine::SlideRef { presentation: 0, slide: 2 }));
                commands.push(Command::ShowMedia(0));
            }
            // Testing aid: open Settings.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_SETTINGS").is_some() {
                running.home_state.settings_open = true;
            }
            // Testing aid: show the Bible page.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_BIBLE").is_some() {
                running.home_state.bible_open = true;
            }
            // Testing aid: also open the slide editor, to check how it looks.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_EDITOR").is_some() {
                let at = pc_engine::SlideRef { presentation: 0, slide: 2 };
                running.home_state.editing = EditDraft::of(&self.show, at);
                if let Some(draft) = running.home_state.editing.as_mut() {
                    draft.selected = draft.slide.objects.first().map(|object| object.id);
                }
                // Testing aid: which tab of the inspector to show.
                running.home_state.inspector.tab = match std::env::var("PRESENTERCRAFT_INSPECTOR_TAB").as_deref() {
                    Ok("shape") => inspector::InspectorTab::Shape,
                    Ok("build") => inspector::InspectorTab::Build,
                    _ => inspector::InspectorTab::Text,
                };
            }
            if running.frames == CAPTURE_AT {
                match capture(gpu, egui_renderer, &jobs, &screen, home_config.format, path) {
                    Ok(()) => println!("saved {}", path.display()),
                    Err(e) => {
                        eprintln!("Could not save the picture: {e}");
                        self.failed = true;
                    }
                }
                self.capture_done = true;
            }
            home_window.request_redraw();
        }
        commands
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        self.save_unsaved();
        if let Some(output) = self.running.as_mut().and_then(|r| r.output.take()) {
            output.shutdown();
        }
        el.exit();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match self.start(el) {
            Ok(Some(mut running)) => {
                // Testing aid: start with NDI on under this name.
                if let Ok(name) = std::env::var("PRESENTERCRAFT_NDI") {
                    running.home_state.requested_ndi = Some(Some(name));
                }
                running.home_window.request_redraw();
                self.running = Some(running);
                // Testing aid: start with the first slide on screen.
                if std::env::var_os("PRESENTERCRAFT_GO_LIVE").is_some() {
                    self.apply(vec![Command::GoTo(pc_engine::SlideRef { presentation: 0, slide: 0 })]);
                }
            }
            Ok(None) => el.exit(),
            Err(message) => {
                eprintln!("{message}");
                self.failed = true;
                el.exit();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if id == running.output_window.id() {
            match event {
                WindowEvent::Resized(size) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Resized(size.width, size.height));
                    }
                }
                // Uncovered again: a frame skipped while the window was hidden must not leave old
                // words on screen. Do not also redraw on RedrawRequested: presenting a frame makes
                // the system ask for another, which loops without end.
                WindowEvent::Occluded(false) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Redraw);
                    }
                }
                WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                    if let Some(command) = show_key_command(&event.logical_key) {
                        self.apply(vec![command]);
                    }
                }
                _ => {}
            }
            return;
        }
        if id != running.home_window.id() {
            return;
        }
        // Show keys act at once and are not passed on, so Space can never also press whatever
        // control happens to have the keyboard. Typing in a text box is left alone.
        if let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && !running.egui_ctx.text_edit_focused()
            && running.home_state.editing.is_none()
            && let Some(command) = show_key_command(&key.logical_key)
        {
            self.apply(vec![command]);
            return;
        }
        // The interface toolkit answers "repaint" to a redraw request itself. Asking for a redraw
        // in reply to a redraw never ends: the window then redraws as fast as it can for ever,
        // which is what filled the computer's memory. A redraw request is not passed to it.
        if !matches!(event, WindowEvent::RedrawRequested)
            && running.egui_state.on_window_event(&running.home_window, &event).repaint
        {
            running.home_window.request_redraw();
        }
        match event {
            WindowEvent::HoveredFile(_) => {
                running.home_state.drop_hover = true;
                running.home_window.request_redraw();
            }
            WindowEvent::HoveredFileCancelled => {
                running.home_state.drop_hover = false;
                running.home_window.request_redraw();
            }
            WindowEvent::DroppedFile(path) => self.add_dropped_file(&path),
            WindowEvent::CloseRequested => self.quit(el),
            WindowEvent::Resized(size) => {
                if size.width > 0 && size.height > 0 {
                    running.home_config.width = size.width;
                    running.home_config.height = size.height;
                    running.home_surface.configure(&running.gpu.device, &running.home_config);
                }
                running.home_window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                let commands = self.draw_home();
                self.apply(commands);
                if let Some((at, slide)) = self.running.as_mut().and_then(|r| r.home_state.pending_edit.take()) {
                    self.apply_edit(at, slide);
                }
                self.update_canvas();
                self.move_output();
                self.switch_ndi();
                // Leaving the Edit view writes the songs changed there straight away.
                let editing = self.running.as_ref().is_some_and(|r| r.home_state.editing.is_some());
                if !editing && !self.unsaved.is_empty() {
                    self.save_unsaved();
                }
                if self.capture_done {
                    self.quit(el);
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Songs changed in the Edit view are written once typing has paused for a second.
        const SAVE_AFTER: Duration = Duration::from_secs(1);
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        if save_at.is_some_and(|at| at <= Instant::now()) {
            self.save_unsaved();
        }
        let save_at = self.unsaved_since.map(|since| since + SAVE_AFTER);
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The interface asks to be drawn again after a delay (animations); otherwise sleep
        // until then, or until a save is due.
        if running.next_repaint.is_some_and(|at| at <= Instant::now()) {
            running.next_repaint = None;
            running.home_window.request_redraw();
        }
        match [running.next_repaint, save_at].into_iter().flatten().min() {
            Some(at) => el.set_control_flow(ControlFlow::WaitUntil(at)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn main() -> ExitCode {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(Some(options)) => options,
        Ok(None) => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };

    let (media_paths, song_paths): (Vec<PathBuf>, Vec<PathBuf>) =
        options.paths.iter().cloned().partition(|path| media::is_media(path));
    let songs = if song_paths.is_empty() {
        vec![(parse_song_text("Sample song", load::SAMPLE_SONG), None)]
    } else {
        let (songs, problems) = load::load_songs(&song_paths);
        for problem in &problems {
            eprintln!("{problem}");
        }
        if songs.is_empty() && !options.list_displays {
            eprintln!("Nothing to show.");
            return ExitCode::from(2);
        }
        songs.into_iter().map(|loaded| (loaded.song, Some(loaded.path))).collect()
    };
    let (songs, sources): (Vec<_>, Vec<_>) = songs.into_iter().unzip();

    let event_loop = match EventLoop::new() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("Could not start the window system: {e}");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        options,
        show: Show::new(songs.into_iter().map(Arc::new).collect()),
        sources,
        pictures: Vec::new(),
        unsaved: Vec::new(),
        unsaved_since: None,
        theme: Arc::new(Theme::builtin()),
        running: None,
        failed: false,
        capture_done: false,
    };
    for path in &media_paths {
        match media::load_picture(path) {
            Ok(picture) => {
                let name = path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
                app.show.add_media(MediaItem { id: Uuid::new_v4(), name, path: path.clone() });
                app.pictures.push(picture);
            }
            Err(message) => eprintln!("{message}"),
        }
    }
    if let Err(e) = event_loop.run_app(&mut app) {
        eprintln!("The window system stopped: {e}");
        return ExitCode::FAILURE;
    }
    if app.failed { ExitCode::FAILURE } else { ExitCode::SUCCESS }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Result<Option<Options>, String> {
        parse_args(list.iter().map(|s| (*s).to_owned()))
    }

    #[test]
    fn no_arguments_means_the_sample() {
        let options = args(&[]).unwrap().unwrap();
        assert!(options.paths.is_empty() && !options.list_displays && options.output_display.is_none());
    }

    #[test]
    fn paths_and_options_mix() {
        let options = args(&["a.txt", "--output-display", "1", "songs"]).unwrap().unwrap();
        assert_eq!(options.paths, vec![PathBuf::from("a.txt"), PathBuf::from("songs")]);
        assert_eq!(options.output_display, Some(1));
    }

    #[test]
    fn help_and_bad_options() {
        assert!(args(&["--help"]).unwrap().is_none());
        assert!(args(&["--output-display"]).is_err());
        assert!(args(&["--output-display", "two"]).is_err());
        assert!(args(&["--wat"]).unwrap_err().contains("--wat"));
    }

    #[test]
    fn show_keys() {
        assert_eq!(show_key_command(&Key::Named(NamedKey::ArrowRight)), Some(Command::Next));
        assert_eq!(show_key_command(&Key::Named(NamedKey::Space)), Some(Command::Next));
        assert_eq!(show_key_command(&Key::Named(NamedKey::ArrowLeft)), Some(Command::Previous));
        assert_eq!(show_key_command(&Key::Character("C".into())), Some(Command::Clear(Layer::Slides)));
        assert_eq!(show_key_command(&Key::Character("x".into())), None);
        assert_eq!(show_key_command(&Key::Named(NamedKey::Home)), Some(Command::First));
        assert_eq!(show_key_command(&Key::Named(NamedKey::End)), Some(Command::Last));
        assert_eq!(show_key_command(&Key::Named(NamedKey::F1)), Some(Command::ClearAll));
        assert_eq!(show_key_command(&Key::Character("c".into())), Some(Command::Clear(Layer::Slides)));
        assert_eq!(show_key_command(&Key::Named(NamedKey::Tab)), None);
    }
}
