//! PresenterCraft: a home window with a slide grid, and a separate output for the audience.

mod bible;
mod bible_store;
mod displays;
mod editor;
mod home;
mod icons;
mod inspector;
mod live_tabs;
mod load;
mod media;
mod menubar;
mod messages;
mod output;
mod screens;
mod service;
mod stage;
mod studio;
mod theme_store;
mod thumbs;
mod timers;

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::{Duration, Instant};

use pc_engine::{Command, Show};
use pc_model::{MediaItem, Slide, Theme, Uuid, parse_song_text};
use pc_render::vello::util::RenderContext;
use pc_render::wgpu;
use pc_render::{Layers, Picture};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Fullscreen, Window, WindowId};

use crate::displays::{DisplayInfo, choose_output};
use crate::editor::EditDraft;
use crate::home::{HomeState, ThumbOf, home_ui};
use crate::output::{Frame, OutputHandle, OutputMsg};
use crate::screens::Placed;
use crate::thumbs::Thumbs;
use crate::timers::Timer;

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

/// The id of the sample song the app starts with.
const SAMPLE_SONG_ID: Uuid = Uuid::from_u128(0x7361_6d70_6c65_5f73_6f6e_6700_0000_0001);

/// The templates of the theme songs use, by id and name, in order.
fn song_templates(theme: &Theme) -> Vec<(Uuid, String)> {
    theme.templates.iter().map(|template| (template.id, template.name.clone())).collect()
}

/// Opens the Screens window: a window of its own, drawn with its own interface.
fn open_panel_window(el: &ActiveEventLoop, running: &mut Running, quiet: bool) -> Result<PanelWindow, String> {
    let attrs = Window::default_attributes()
        .with_title("Screens")
        .with_inner_size(winit::dpi::LogicalSize::new(760.0, 520.0))
        .with_min_inner_size(winit::dpi::LogicalSize::new(640.0, 440.0));
    let attrs = if quiet {
        attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
    } else {
        attrs
    };
    let window = Arc::new(el.create_window(attrs).map_err(|e| format!("Could not open the Screens window: {e}"))?);
    let surface = running
        .context
        .instance
        .create_surface(window.clone())
        .map_err(|e| format!("Could not draw the Screens window: {e}"))?;
    let format = surface_format(&surface, running.context.devices[running.device_id].adapter())?;
    let size = window.inner_size();
    let config = wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        width: size.width.max(1),
        height: size.height.max(1),
        present_mode: wgpu::PresentMode::AutoVsync,
        color_space: wgpu::SurfaceColorSpace::Auto,
        desired_maximum_frame_latency: 2,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![],
    };
    surface.configure(&running.gpu.device, &config);
    let ctx = egui::Context::default();
    studio::apply_style(&ctx);
    let state = egui_winit::State::new(
        ctx.clone(),
        egui::ViewportId::ROOT,
        &window,
        Some(window.scale_factor() as f32),
        None,
        Some(running.gpu.device.limits().max_texture_dimension_2d as usize),
    );
    let renderer = egui_wgpu::Renderer::new(&running.gpu.device, format, egui_wgpu::RendererOptions::default());
    Ok(PanelWindow { window, surface, config, ctx, state, renderer, next_repaint: None, frames: 0 })
}

/// What a theme calls its templates, in order.
fn template_names(theme: &Theme) -> Vec<String> {
    theme.templates.iter().map(|template| template.name.clone()).collect()
}

/// The keys that run the show. They are read straight from the window, before the interface
/// draws, so a key press reaches the output without waiting for the home window. A letter
/// jumps to the group it stands for (A for the first verse, C for the chorus, and so on).
fn show_key_command(key: &Key, show: &Show) -> Option<Command> {
    match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::Space) => Some(Command::Next),
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Previous),
        Key::Named(NamedKey::Home) => Some(Command::First),
        Key::Named(NamedKey::End) => Some(Command::Last),
        // Escape clears everything; F1 does too, for keyboards that have it to hand.
        Key::Named(NamedKey::Escape | NamedKey::F1) => Some(Command::ClearAll),
        Key::Named(NamedKey::F2) => Some(Command::Clear(pc_engine::Layer::Slides)),
        Key::Named(NamedKey::F3) => Some(Command::Clear(pc_engine::Layer::Media)),
        Key::Character(typed) => {
            let mut letters = typed.chars();
            let (Some(letter), None) = (letters.next(), letters.next()) else { return None };
            show.group_start(pc_model::groups_for_key(letter)).map(Command::GoTo)
        }
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

/// A window drawn with the interface toolkit, and what is needed to draw one frame in it.
struct UiTarget<'a> {
    window: &'a Window,
    surface: &'a wgpu::Surface<'static>,
    config: &'a wgpu::SurfaceConfiguration,
    renderer: &'a mut egui_wgpu::Renderer,
    ctx: &'a egui::Context,
    label: &'static str,
}

/// Draws one frame of the interface in a window and returns what was drawn, for a capture.
fn paint_ui(
    gpu: &Gpu,
    target: UiTarget<'_>,
    mut full: egui::FullOutput,
) -> (Vec<egui::ClippedPrimitive>, egui_wgpu::ScreenDescriptor) {
    let UiTarget { window, surface, config, renderer, ctx, label } = target;
    let jobs = ctx.tessellate(std::mem::take(&mut full.shapes), full.pixels_per_point);
    for (id, deltas) in &full.textures_delta.set {
        for delta in deltas {
            renderer.update_texture(&gpu.device, &gpu.queue, *id, delta);
        }
    }
    let screen = egui_wgpu::ScreenDescriptor {
        size_in_pixels: [config.width, config.height],
        pixels_per_point: full.pixels_per_point,
    };
    let mut encoder = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let extra = renderer.update_buffers(&gpu.device, &gpu.queue, &mut encoder, &jobs, &screen);
    let frame = match surface.get_current_texture() {
        wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame) => Some(frame),
        wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
            surface.configure(&gpu.device, config);
            window.request_redraw();
            None
        }
        _ => None,
    };
    if let Some(frame) = frame {
        let view = frame.texture.create_view(&wgpu::TextureViewDescriptor::default());
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some(label),
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
            renderer.render(&mut pass, &jobs, &screen);
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
        renderer.free_texture(id);
    }
    // The toolkit insists on being told its texture changes were applied.
    full.textures_delta.clear();
    (jobs, screen)
}

/// The Screens window: a window of its own with its own interface.
struct PanelWindow {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: egui::Context,
    state: egui_winit::State,
    renderer: egui_wgpu::Renderer,
    next_repaint: Option<Instant>,
    frames: u32,
}

/// Everything that exists once the windows are open.
struct Running {
    home_window: Arc<Window>,
    home_surface: wgpu::Surface<'static>,
    home_config: wgpu::SurfaceConfiguration,
    /// The windows audience screens are drawn in, one per screen that has one.
    screen_windows: Vec<ScreenWindow>,
    /// Windows of screens that were moved elsewhere or switched off, hidden, and dropped a
    /// little later once the output thread has let go of them.
    retired: Vec<(Arc<Window>, Instant)>,
    /// The NDI names the output thread was last told to send under, by screen.
    ndi_sent: Vec<(Uuid, String)>,
    /// The names the screens were last told to show while being identified.
    identify_sent: Option<Vec<(Uuid, String)>>,
    /// The stage screens the output was last told of, and whether they were on.
    stage_sent: Option<(Vec<Uuid>, bool)>,
    /// What the output was last told to show over the slides.
    overlay_sent: Option<output::Overlay>,
    output: Option<OutputHandle>,
    /// The Screens window, while it is open.
    screens_window: Option<PanelWindow>,
    gpu: Gpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    thumbs: Thumbs,
    preview: Option<egui::TextureId>,
    /// The preview picture before the last change, and when that change was, so the preview
    /// fades as the output does.
    preview_before: Option<egui::TextureId>,
    preview_changed: Instant,
    home_state: HomeState,
    next_repaint: Option<Instant>,
    /// The Edit view's picture of the slide being edited, and the words it was drawn from.
    canvas: Option<egui::TextureId>,
    canvas_for: Option<(pc_engine::SlideRef, Slide)>,
    frames: u32,
    /// The GPU instance, which outlives its surfaces and makes the surfaces of new screens,
    /// and which of its devices is in use.
    context: RenderContext,
    device_id: usize,
}

/// A window an audience screen is drawn in.
struct ScreenWindow {
    screen: Uuid,
    window: Arc<Window>,
    placed: Placed,
    name: String,
}

struct App {
    options: Options,
    show: Show,
    /// The file each song came from, in the show's order; `None` for the bundled sample.
    sources: Vec<Option<PathBuf>>,
    /// The presentations kept for Bible passages.
    bible_slots: [usize; 2],
    /// The decoded picture of each media item, in the show's order.
    pictures: Vec<Picture>,
    /// Songs changed in the Edit view and not yet written to their files, and when the last
    /// change was made.
    unsaved: Vec<usize>,
    unsaved_since: Option<Instant>,
    /// The theme songs are drawn with, and the one Bible slides are drawn with: copies of
    /// the themes at `songs_theme` and `bible_theme` in `themes`.
    theme: Arc<Theme>,
    theme_for_bible: Arc<Theme>,
    themes: Vec<Theme>,
    songs_theme: usize,
    bible_theme: usize,
    /// Which template of its theme Bible slides use, by its place among them.
    bible_layout: usize,
    /// The themes with changes not yet written to their files, by place.
    themes_unsaved: Vec<usize>,
    /// The Bible page's set-up as last written to its file, to notice when it changes.
    bible_page_saved: String,
    /// The small choices as last written, and how many frames have been drawn: NDI takes a
    /// moment to start, and the choices are not written until it has had that moment.
    prefs_saved: Option<service::Prefs>,
    frames_since_start: u32,
    /// The modifier keys held down now. A letter pressed with Command or Control is a
    /// shortcut, not a jump to a group.
    held: winit::keyboard::ModifiersState,
    /// Testing aid: select a box once the Theme Editor has opened.
    capture_selects_box: bool,
    /// The window that has the keyboard, for Close Window.
    focused: Option<WindowId>,
    /// The menu bar along the top of the screen.
    #[cfg(target_os = "macos")]
    menu: Option<menubar::MenuBar>,
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
        // A run that only saves a picture of the window stays out of the way: its windows open
        // behind everything else and do not take the keyboard, so whoever is using the
        // computer is not interrupted.
        let quiet = self.options.capture_home.is_some() || std::env::var_os("PRESENTERCRAFT_QUIET").is_some();
        let behind = |attrs: winit::window::WindowAttributes| {
            if quiet {
                attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
            } else {
                attrs
            }
        };
        let home_attrs = behind(home_attrs);
        let home_window =
            Arc::new(el.create_window(home_attrs).map_err(|e| format!("Could not open the home window: {e}"))?);
        let home_display =
            home_window.current_monitor().and_then(|home| monitors.iter().position(|m| *m == home)).unwrap_or(0);

        // A display asked for on the command line must exist. The screens' windows are opened
        // once the app has started.
        choose_output(&displays, home_display, self.options.output_display)?;

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

        let output = OutputHandle::spawn(gpu.device.clone(), gpu.queue.clone(), self.theme.clone())?;

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
        studio::apply_style(&egui_ctx);
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
        let (preview, _) = thumbs.preview(&gpu, &mut egui_renderer, &Layers::default(), &self.theme);

        Ok(Some(Running {
            home_window,
            home_surface,
            home_config,
            screen_windows: Vec::new(),
            retired: Vec::new(),
            ndi_sent: Vec::new(),
            identify_sent: None,
            stage_sent: None,
            overlay_sent: None,
            output: Some(output),
            screens_window: None,
            gpu,
            egui_ctx,
            egui_state,
            egui_renderer,
            thumbs,
            preview,
            preview_before: None,
            preview_changed: Instant::now(),
            home_state: HomeState {
                fonts: Arc::new(pc_render::Fonts::new().family_names()),
                theme_names: self.themes.iter().map(|theme| theme.name.clone()).collect(),
                theme_for_songs: self.songs_theme,
                theme_layouts: self.theme_layouts(),
                template_names: self.themes.iter().map(template_names).collect(),
                song_templates: song_templates(&self.theme),
                bible_layout: self.bible_layout,
                services: service::load(),
                // A song is found again by its file, without the ending (which changes when
                // it is first saved); one with no file, by its title.
                song_keys: self
                    .sources
                    .iter()
                    .zip(self.show.presentations())
                    .map(|(source, song)| match source {
                        Some(path) => path.with_extension("").to_string_lossy().into_owned(),
                        None => format!("title:{}", song.title),
                    })
                    .collect(),
                theme_for_bible: self.bible_theme,
                bible: home::BiblePage {
                    slots: Some(self.bible_slots),
                    shelf: bible::Shelf::with_installed(),
                    boxes: home::layout_boxes(&self.theme_for_bible, self.bible_layout),
                    box_labels: home::layout_labels(&self.theme_for_bible, self.bible_layout),
                    template: self.bible_template(),
                    ..home::BiblePage::default()
                },
                ..HomeState::default()
            },
            next_repaint: None,
            canvas: None,
            canvas_for: None,
            frames: 0,
            context,
            device_id,
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
            // Clear All, and Clear Messages, take a message off the screens.
            if matches!(command, Command::ClearAll | Command::Clear(pc_engine::Layer::Messages))
                && running.home_state.message_live.take().is_some()
            {
                output_changed = true;
                any = true;
            }
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
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let slide = self.show.live_slide().map(|(_, slide)| Arc::new(slide.clone()));
        // The slide after the one on screen, for the stage screens.
        let next = self.show.live().and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide + 1).map(|(_, slide)| Arc::new(slide.clone()))
        });
        let media = self.show.live_media().and_then(|index| self.pictures.get(index)).cloned();
        // The audience comes first: tell the output before drawing anything for the operator.
        if let Some(output) = &running.output {
            let frame = Frame {
                media: media.clone(),
                slide: slide.clone(),
                theme: theme.clone(),
                enabled: self.show.output_enabled(),
                transition: running.home_state.transition,
                next,
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_stopped = true;
            }
        }
        // The operator's preview shows a message over the slide, as the audience sees it.
        let message = running.home_state.message_live.and_then(|(id, _)| {
            let message = running.home_state.messages.iter().find(|message| message.id == id)?;
            let words = messages::fill_in(
                &message.text,
                &running.home_state.timers,
                Instant::now(),
                timers::seconds_into_day(),
            );
            Some(messages::slide(&words))
        });
        let layers = Layers {
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: message.as_ref().filter(|_| self.show.output_enabled()),
        };
        let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &theme);
        running.preview = now;
        running.preview_before = before;
        running.preview_changed = Instant::now();
    }

    /// Adds a file dropped on the window to the media bin, or says why it could not be added.
    fn add_dropped_file(&mut self, path: &std::path::Path) {
        // A Bible file is installed rather than added as media.
        if path.extension().is_some_and(|ext| ext == "pcbible") {
            let Some(running) = self.running.as_mut() else {
                return;
            };
            running.home_state.drop_hover = false;
            match bible_store::install_file(path) {
                Ok((kept, info)) => {
                    running.home_state.notice = Some(format!("Installed the {}.", info.name));
                    running.home_state.bible.shelf.add_file(kept, info);
                }
                Err(message) => running.home_state.notice = Some(message),
            }
            running.home_window.request_redraw();
            return;
        }
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
                self.save_media_list();
            }
            Err(message) => running.home_state.notice = Some(message),
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
    }

    /// Remembers which pictures are in the media bin.
    fn save_media_list(&mut self) {
        let paths: Vec<PathBuf> = self.show.media().iter().map(|item| item.path.clone()).collect();
        if let Err(problem) = media::save_list(&paths)
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notice = Some(problem);
        }
    }

    /// Rebuilds a song from the words finished in the Reflow editor.
    fn handle_reflow(&mut self) {
        let Some((index, text)) = self.running.as_mut().and_then(|r| r.home_state.reflow_done.take()) else { return };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let rebuilt = song.reflowed(&text);
        if rebuilt == *song {
            return;
        }
        let changes = self.show.replace_presentation(index, Arc::new(rebuilt));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if changes.slides {
            self.refresh_output();
        }
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
    }

    /// Carries out a change to the media bin: choosing pictures to add, or removing one.
    fn handle_media_request(&mut self) {
        use home::MediaRequest;
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.media_request.take()) else { return };
        match request {
            MediaRequest::Choose => {
                // The system's own window for choosing files, as any Mac app has.
                let chosen = rfd::FileDialog::new()
                    .set_title("Add Pictures")
                    .add_filter("Pictures", &media::PICTURE_ENDINGS)
                    .pick_files()
                    .unwrap_or_default();
                for path in chosen {
                    self.add_dropped_file(&path);
                }
            }
            MediaRequest::Remove(index) => {
                let changes = self.show.remove_media(index);
                if index < self.pictures.len() {
                    self.pictures.remove(index);
                }
                if changes.media {
                    self.refresh_output();
                }
                self.save_media_list();
                if let Some(running) = self.running.as_mut() {
                    running.home_window.request_redraw();
                }
            }
        }
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
        // The song is written at once, so nothing is lost if the app stops without warning.
        // While a box is being dragged it waits for the drag to end.
        let dragging =
            self.running.as_ref().is_some_and(|r| r.home_state.editing.as_ref().is_some_and(|d| d.drag.is_some()));
        if !dragging {
            self.save_unsaved();
        }
    }

    /// Writes every song changed in the Edit view to its file. The bundled sample has no file.
    fn save_unsaved(&mut self) {
        let mut problem = None;
        for index in std::mem::take(&mut self.unsaved) {
            // A song with no file yet (the sample) is given one in the app's Songs folder, so
            // a change to it is kept like any other. Bible passages are never saved as songs.
            if !self.bible_slots.contains(&index)
                && let (Some(None), Some(song)) = (self.sources.get(index), self.show.presentations().get(index))
                && let Some(folder) = bible_store::data_dir().map(|dir| dir.join("Songs"))
            {
                let _ = std::fs::create_dir_all(&folder);
                self.sources[index] = Some(folder.join(format!("{}.pcpres", song.id)));
            }
            let (Some(Some(path)), Some(song)) = (self.sources.get(index), self.show.presentations().get(index)) else {
                continue;
            };
            match load::save_song(path, song) {
                // From now on the song lives in the file that was just written.
                Ok(written) => self.sources[index] = Some(written),
                Err(e) => problem = Some(format!("The change was not saved. {e}")),
            }
        }
        for index in std::mem::take(&mut self.themes_unsaved) {
            if let Some(Err(e)) = self.themes.get(index).map(theme_store::save) {
                problem = Some(e);
            }
        }
        self.unsaved_since = None;
        if let Some(running) = self.running.as_mut() {
            running.home_state.notice = problem;
            running.home_window.request_redraw();
        }
    }

    /// Makes the new song that was asked for, and gives a song the title just typed for it.
    fn handle_songs(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if let Some((index, title)) = running.home_state.pending_title.take()
            && let Some(song) = self.show.presentations().get(index)
        {
            let renamed = pc_model::Presentation { title, ..(**song).clone() };
            self.show.replace_presentation(index, Arc::new(renamed));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
        }
        // A group renamed in the editor: its name changes, and its colour with it.
        if let Some((index, group, name)) = running.home_state.pending_group_name.take()
            && let Some(song) = self.show.presentations().get(index)
            && group < song.groups.len()
        {
            let mut renamed = (**song).clone();
            renamed.groups[group].color = pc_model::group_color(&name);
            renamed.groups[group].name = name;
            let ids: Vec<Uuid> = renamed.groups[group].slides.iter().map(|slide| slide.id).collect();
            self.show.replace_presentation(index, Arc::new(renamed));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            for id in ids {
                running.thumbs.forget(id, &mut running.egui_renderer);
            }
            running.home_window.request_redraw();
        }
        // A slide added at the end of a song in the editor: it is made, and opened to edit.
        if let Some(index) = running.home_state.add_slide.take()
            && let Some(song) = self.show.presentations().get(index)
        {
            let (longer, slide) = song.with_slide_added();
            self.show.replace_presentation(index, Arc::new(longer));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            let at = pc_engine::SlideRef { presentation: index, slide };
            running.home_state.editing = EditDraft::of(&self.show, at);
            running.home_window.request_redraw();
        }
        if !std::mem::take(&mut running.home_state.new_song) {
            return;
        }
        // A name no other song has: "New Song", "New Song 2", and so on.
        let taken = |name: &str| self.show.presentations().iter().any(|song| song.title == name);
        let title = std::iter::once("New Song".to_owned())
            .chain((2..).map(|n| format!("New Song {n}")))
            .find(|name| !taken(name))
            .unwrap_or_default();
        let id = Uuid::new_v4();
        let verse = pc_model::Group {
            id: Uuid::new_v4(),
            name: "Verse 1".to_owned(),
            color: pc_model::group_color("Verse 1"),
            slides: vec![Slide::new(vec![pc_model::Object::text(pc_model::Role::primary(), "")])],
        };
        let song = pc_model::Presentation { id, title: title.clone(), groups: vec![verse] };
        // New songs live in the app's Songs folder, each in a file named by its id, so two
        // songs with one title never collide.
        let path = bible_store::data_dir().map(|dir| dir.join("Songs").join(format!("{id}.pcpres")));
        if let Some(folder) = path.as_ref().and_then(|path| path.parent()) {
            let _ = std::fs::create_dir_all(folder);
        }
        let index = self.show.add_presentation(Arc::new(song));
        self.sources.push(path.clone());
        self.unsaved.push(index);
        self.unsaved_since = Some(Instant::now());
        self.apply(vec![Command::Select(index)]);
        if let Some(running) = self.running.as_mut() {
            let state = &mut running.home_state;
            state.song_keys.push(match &path {
                Some(path) => path.with_extension("").to_string_lossy().into_owned(),
                None => format!("title:{title}"),
            });
            // Back to the slides, with the title ready to be typed over.
            state.editing = None;
            state.bible_open = false;
            state.renaming_song = Some(title);
            state.rename_started = true;
            running.home_window.request_redraw();
        }
    }

    /// The theme a presentation's slides are drawn with: the Bible theme for a Bible passage,
    /// the songs theme for everything else.
    fn theme_of(&self, presentation: usize) -> &Arc<Theme> {
        if self.bible_slots.contains(&presentation) { &self.theme_for_bible } else { &self.theme }
    }

    /// Each theme's templates as boxes, for the pictures in the theme picker.
    fn theme_layouts(&self) -> Vec<Vec<Vec<(pc_model::Role, pc_model::SlideRect)>>> {
        let boxes = |theme: &Theme| (0..theme.templates.len()).map(|at| home::layout_boxes(theme, at)).collect();
        self.themes.iter().map(boxes).collect()
    }

    /// The template Bible slides are drawn with, by its id.
    fn bible_template(&self) -> Option<Uuid> {
        self.theme_for_bible.templates.get(self.bible_layout).map(|template| template.id)
    }

    /// Brings everything that depends on the themes up to date after one was changed or
    /// chosen: the copies slides are drawn with, every thumbnail, and what the window lists.
    /// The output is left as it is; `refresh_output` brings it up to date.
    fn themes_changed(&mut self) {
        self.songs_theme = self.songs_theme.min(self.themes.len().saturating_sub(1));
        self.bible_theme = self.bible_theme.min(self.themes.len().saturating_sub(1));
        self.theme = Arc::new(self.themes[self.songs_theme].clone());
        self.theme_for_bible = Arc::new(self.themes[self.bible_theme].clone());
        self.bible_layout = self.bible_layout.min(self.theme_for_bible.templates.len().saturating_sub(1));
        let (layouts, pinned) = (self.theme_layouts(), self.bible_template());
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            let state = &mut running.home_state;
            state.theme_names = self.themes.iter().map(|theme| theme.name.clone()).collect();
            state.theme_for_songs = self.songs_theme;
            state.theme_for_bible = self.bible_theme;
            state.theme_layouts = layouts;
            state.template_names = self.themes.iter().map(template_names).collect();
            state.song_templates = song_templates(&self.theme);
            state.bible_layout = self.bible_layout;
            state.bible.boxes = home::layout_boxes(&self.theme_for_bible, self.bible_layout);
            state.bible.box_labels = home::layout_labels(&self.theme_for_bible, self.bible_layout);
            state.bible.template = pinned;
            running.home_window.request_redraw();
        }
    }

    /// Carries out what the Theme menu, Bible Options and the Theme Editor asked for this frame.
    fn handle_themes(&mut self) {
        use home::ThemeRequest;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let request = running.home_state.theme_request.take();
        let editing = running.home_state.theme_edit.is_some();
        let count = self.themes.len();
        match request {
            Some(ThemeRequest::UseForSongs(index)) if index < count && index != self.songs_theme => {
                self.songs_theme = index;
                self.themes_changed();
                theme_store::remember_in_use(&self.theme, &self.theme_for_bible, self.bible_layout);
                if !editing {
                    self.refresh_output();
                }
            }
            Some(ThemeRequest::UseForBible(index, layout))
                if index < count && (index != self.bible_theme || layout != self.bible_layout) =>
            {
                self.bible_theme = index;
                self.bible_layout = layout;
                self.themes_changed();
                theme_store::remember_in_use(&self.theme, &self.theme_for_bible, self.bible_layout);
                // Choosing a theme points the translations at its boxes, as a fresh start.
                if let Some(running) = self.running.as_mut() {
                    running.home_state.bible.assign_boxes();
                }
                if !editing {
                    self.refresh_output();
                }
            }
            Some(ThemeRequest::New) => {
                let mut theme = (*self.theme).clone();
                theme.id = Uuid::new_v4();
                theme.name = "New Theme".to_owned();
                self.themes.push(theme);
                self.themes_unsaved.push(self.themes.len() - 1);
                self.unsaved_since = Some(Instant::now());
                self.themes_changed();
                self.open_theme_editor(self.themes.len() - 1);
            }
            Some(ThemeRequest::Edit(index)) if index < count => self.open_theme_editor(index),
            // The built-in theme, at the front of the list, is always kept.
            Some(ThemeRequest::Delete(index)) if index > 0 && index < count => {
                // Write what is waiting first, so the places in the list still mean what they did.
                self.save_unsaved();
                let gone = self.themes.remove(index);
                let problem = theme_store::delete(&gone).err();
                // Whatever used it falls back to the built-in theme; later themes move up one.
                let moved = |at: usize| match at.cmp(&index) {
                    std::cmp::Ordering::Less => at,
                    std::cmp::Ordering::Equal => 0,
                    std::cmp::Ordering::Greater => at - 1,
                };
                self.songs_theme = moved(self.songs_theme);
                self.bible_theme = moved(self.bible_theme);
                self.themes_changed();
                theme_store::remember_in_use(&self.theme, &self.theme_for_bible, self.bible_layout);
                let was_open =
                    self.running.as_ref().and_then(|r| r.home_state.theme_edit.as_ref().map(|edit| edit.index));
                if let Some(open) = was_open {
                    self.open_theme_editor(moved(open));
                } else {
                    self.refresh_output();
                }
                if let (Some(problem), Some(running)) = (problem, self.running.as_mut()) {
                    running.home_state.notice = Some(problem);
                }
            }
            _ => {}
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some(edit) = running.home_state.theme_edit.as_mut() else {
            return;
        };
        let (changed, leave, index) = (std::mem::take(&mut edit.changed), edit.leave, edit.index);
        if changed {
            let mut bible_template_gone = false;
            let theme = edit.theme.clone();
            // The template Bible slides use keeps its link when templates are added or
            // removed around it; if it was removed, the first one takes its place.
            if index == self.bible_theme {
                let linked = self.bible_template();
                let now = theme.templates.iter().position(|template| Some(template.id) == linked);
                let moved = now != Some(self.bible_layout);
                self.bible_layout = now.unwrap_or(0);
                if moved {
                    theme_store::remember_in_use(&self.theme, &theme, self.bible_layout);
                }
                bible_template_gone = now.is_none();
            }
            if let Some(slot) = self.themes.get_mut(index) {
                *slot = theme;
            }
            if !self.themes_unsaved.contains(&index) {
                self.themes_unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            self.themes_changed();
            // The template Bible slides used was deleted: the translations are pointed at
            // the boxes of the one that took its place, as when a template is chosen.
            if bible_template_gone && let Some(running) = self.running.as_mut() {
                running.home_state.bible.assign_boxes();
            }
            // A theme is written at once, so nothing is lost if the app stops without
            // warning. While a box is being dragged it waits for the drag to end.
            let dragging =
                self.running.as_ref().is_some_and(|r| r.home_state.editing.as_ref().is_some_and(|d| d.drag.is_some()));
            if !dragging {
                self.save_unsaved();
            }
        }
        if leave {
            if let Some(running) = self.running.as_mut() {
                running.home_state.theme_edit = None;
                running.home_state.editing = None;
            }
            // The theme is finished with: write it, and let the audience see it.
            self.save_unsaved();
            self.refresh_output();
        }
    }

    /// Opens the Theme Editor on the theme at a place in the list.
    fn open_theme_editor(&mut self, index: usize) {
        let (Some(theme), Some(running)) = (self.themes.get(index), self.running.as_mut()) else {
            return;
        };
        let edit = home::ThemeEdit::of(index, theme.clone());
        running.home_state.editing = edit.theme.templates.first().map(|first| EditDraft::of_slide(first.as_slide(), 0));
        running.home_state.theme_edit = Some(edit);
        running.home_state.bible_open = false;
        running.canvas_for = None;
        running.home_window.request_redraw();
    }

    /// True for a run that only saves a picture, or is asked to stay out of the way.
    fn quiet(&self) -> bool {
        self.options.capture_home.is_some() || std::env::var_os("PRESENTERCRAFT_QUIET").is_some()
    }

    /// Brings the screens' windows and NDI sources in line with the list of screens and the
    /// displays connected now: opens, moves and closes windows, starts, renames and stops NDI,
    /// and says what each screen is doing. Cheap when nothing has changed, so it runs every
    /// frame and notices a display being plugged in or out.
    fn sync_screens(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let monitors: Vec<_> = el.available_monitors().collect();
        let displays = display_list(monitors.iter().cloned());
        let home = running
            .home_window
            .current_monitor()
            .and_then(|home| monitors.iter().position(|m| *m == home))
            .unwrap_or(0);
        if displays != running.home_state.displays {
            running.home_state.displays = displays.clone();
        }
        if std::mem::take(&mut running.home_state.screens_changed)
            && !quiet
            && let Err(problem) = screens::save(&running.home_state.screens)
        {
            running.home_state.notice = Some(problem);
        }
        let list = running.home_state.screens.clone();
        let mut placed = screens::place_all(&list, &displays, home);
        // A capture never fills a display or sends over the network, whatever the list says.
        if quiet {
            let ndi_aid = std::env::var_os("PRESENTERCRAFT_NDI").is_some();
            for place in &mut placed {
                match place {
                    Placed::Fullscreen(_) => *place = Placed::Window,
                    Placed::Ndi(_) if !ndi_aid => *place = Placed::Nothing("Not sent during a capture.".to_owned()),
                    _ => {}
                }
            }
        }
        running.retired.retain(|(_, since)| since.elapsed() < Duration::from_secs(2));
        let Running { output, screen_windows, retired, context, device_id, home_state, .. } = running;
        let Some(output) = output.as_ref() else { return };

        // Windows: keep, move, open or close each one.
        let mut old = std::mem::take(screen_windows);
        let mut opened = false;
        for (screen, place) in list.iter().zip(&placed) {
            if !matches!(place, Placed::Fullscreen(_) | Placed::Window) {
                continue;
            }
            if let Some(at) = old.iter().position(|have| have.screen == screen.id) {
                let mut kept = old.remove(at);
                if kept.placed != *place {
                    match place {
                        Placed::Fullscreen(index) => {
                            kept.window.set_fullscreen(Some(Fullscreen::Borderless(monitors.get(*index).cloned())));
                        }
                        _ => kept.window.set_fullscreen(None),
                    }
                    kept.window.set_cursor_visible(!matches!(place, Placed::Fullscreen(_)));
                    kept.placed = place.clone();
                }
                if kept.name != screen.name {
                    kept.window.set_title(&format!("{} — PresenterCraft", screen.name));
                    kept.name = screen.name.clone();
                }
                screen_windows.push(kept);
                continue;
            }
            let attrs = Window::default_attributes().with_title(format!("{} — PresenterCraft", screen.name));
            let attrs = match place {
                Placed::Fullscreen(index) => {
                    attrs.with_fullscreen(Some(Fullscreen::Borderless(monitors.get(*index).cloned())))
                }
                _ => attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0)),
            };
            let attrs = if quiet {
                attrs.with_active(false).with_window_level(winit::window::WindowLevel::AlwaysOnBottom)
            } else {
                attrs
            };
            let window = match el.create_window(attrs) {
                Ok(window) => Arc::new(window),
                Err(e) => {
                    home_state.notice = Some(format!("Could not open a window for \"{}\": {e}", screen.name));
                    continue;
                }
            };
            window.set_cursor_visible(!matches!(place, Placed::Fullscreen(_)));
            let surface = match context.instance.create_surface(window.clone()) {
                Ok(surface) => surface,
                Err(e) => {
                    home_state.notice = Some(format!("Could not draw to \"{}\": {e}", screen.name));
                    continue;
                }
            };
            let format = match surface_format(&surface, context.devices[*device_id].adapter()) {
                Ok(format) => format,
                Err(e) => {
                    home_state.notice = Some(e);
                    continue;
                }
            };
            let size = window.inner_size();
            output.send(OutputMsg::AddSurface { id: screen.id, surface, format, w: size.width, h: size.height });
            screen_windows.push(ScreenWindow {
                screen: screen.id,
                window,
                placed: place.clone(),
                name: screen.name.clone(),
            });
            opened = true;
        }
        for gone in old {
            output.send(OutputMsg::RemoveSurface(gone.screen));
            gone.window.set_visible(false);
            retired.push((gone.window, Instant::now()));
        }
        // A new window takes the keyboard; give it back to the operator.
        if opened && !quiet {
            running.home_window.focus_window();
        }
        let Some(running) = self.running.as_mut() else { return };
        let Running { output, ndi_sent, identify_sent, home_state, .. } = running;
        let Some(output) = output.as_ref() else { return };

        // NDI: start, rename and stop sources.
        let wanted: Vec<(Uuid, String)> = list
            .iter()
            .zip(&placed)
            .filter_map(|(screen, place)| match place {
                Placed::Ndi(name) => Some((screen.id, name.clone())),
                _ => None,
            })
            .collect();
        let mut ndi_moved = false;
        for (id, name) in &wanted {
            if !ndi_sent.contains(&(*id, name.clone())) {
                output.send(OutputMsg::SetNdi { id: *id, name: Some(name.clone()) });
                ndi_moved = true;
            }
        }
        for (id, _) in ndi_sent.iter() {
            if !wanted.iter().any(|(still, _)| still == id) {
                output.send(OutputMsg::SetNdi { id: *id, name: None });
                ndi_moved = true;
            }
        }
        *ndi_sent = wanted;
        if ndi_moved {
            // A new source needs the current picture at once.
            output.send(OutputMsg::Redraw);
        }

        // Stage screens: which they are, and whether they are on.
        let stage = (
            list.iter()
                .filter(|screen| screen.kind == screens::Kind::Stage)
                .map(|screen| screen.id)
                .collect::<Vec<_>>(),
            home_state.stage_on,
        );
        if running.stage_sent.as_ref() != Some(&stage) {
            output.send(OutputMsg::Stage { screens: stage.0.clone(), on: stage.1 });
            running.stage_sent = Some(stage);
        }

        // Identify Screens: every screen shows its own name while it is ticked.
        let names = home_state.identify.then(|| list.iter().map(|screen| (screen.id, screen.name.clone())).collect());
        if names != *identify_sent {
            output.send(OutputMsg::Identify(names.clone().unwrap_or_default()));
            *identify_sent = names;
        }

        // What each screen is doing, for the Screens window.
        let mut starting = false;
        let status_before = std::mem::take(&mut home_state.screen_status);
        home_state.screen_status = list
            .iter()
            .zip(&placed)
            .map(|(screen, place)| match place {
                Placed::Ndi(_) => match output.ndi_status(screen.id) {
                    output::NdiStatus::On(name) => (format!("Sending as \"{name}\" at 1920 x 1080"), true),
                    output::NdiStatus::Failed(reason) => (reason, false),
                    output::NdiStatus::Off => {
                        starting = true;
                        ("Starting NDI…".to_owned(), false)
                    }
                },
                Placed::Fullscreen(_) | Placed::Window => (screens::describe(place, &displays), true),
                Placed::Nothing(_) => (screens::describe(place, &displays), false),
            })
            .collect();
        if home_state.screen_status != status_before
            && let Some(panel) = &running.screens_window
        {
            panel.window.request_redraw();
        }
        // Starting NDI takes a moment on the output thread; look again shortly.
        if starting || ndi_moved {
            running.next_repaint = Some(Instant::now() + Duration::from_millis(400));
        }
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
        if let Some((at, slide)) = &editing {
            // A theme's layout is drawn with that theme; a slide with its presentation's.
            let theme = match &running.home_state.theme_edit {
                Some(edit) => self.themes.get(edit.index).unwrap_or(&self.theme),
                None if self.bible_slots.contains(&at.presentation) => &self.theme_for_bible,
                None => &self.theme,
            };
            running.canvas = running.thumbs.canvas(&running.gpu, &mut running.egui_renderer, slide, theme);
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
        if running.output.as_ref().is_none_or(OutputHandle::is_finished) {
            running.home_state.output_stopped = true;
        }
        let mut commands = Vec::new();
        // How far the preview's fade has got: the same time and easing as the output's.
        let seconds = running.home_state.dissolve_seconds();
        let amount = if seconds > 0.0 {
            let t = (running.preview_changed.elapsed().as_secs_f32() / seconds).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        } else {
            1.0
        };
        let preview = home::Preview { now: running.preview, before: running.preview_before, amount };
        let (show, pictures) = (&self.show, &self.pictures);
        let (songs_theme, bible_theme): (&Theme, &Theme) = (&self.theme, &self.theme_for_bible);
        // The theme the part of the window in front works with: the one being edited, the
        // Bible's on the Bible page or when a Bible slide is being edited, otherwise the songs'.
        let state = &running.home_state;
        let edited = state.theme_edit.as_ref().and_then(|edit| self.themes.get(edit.index));
        let on_bible = match &state.editing {
            Some(draft) => self.bible_slots.contains(&draft.at.presentation),
            None => state.bible_open,
        };
        let theme = edited.unwrap_or(if on_bible { bible_theme } else { songs_theme });
        let canvas = running.canvas;
        let home_state = &mut running.home_state;
        let mut full = egui_ctx.run_ui(raw_input, |ui| {
            // The toolkit may run this closure twice in one frame, and input arrives only the
            // first time, so add to the list rather than replace it.
            let mut thumb = |which: ThumbOf| match which {
                ThumbOf::Slide(slide) => thumbs.get(gpu, egui_renderer, slide, songs_theme),
                ThumbOf::BibleSlide(slide) => thumbs.get(gpu, egui_renderer, slide, bible_theme),
                ThumbOf::Layout(index, slide) => {
                    // A layout is made anew each frame; a fixed id lets its thumbnail be kept
                    // until the theme changes, when every thumbnail is dropped.
                    let mut kept = slide.clone();
                    kept.id = Uuid::from_u128(0x7068_656d_655f_6c61_796f_7574_0000_0000 + index as u128);
                    thumbs.get(gpu, egui_renderer, &kept, theme)
                }
                ThumbOf::Media(index) => {
                    let (item, picture) = (show.media().get(index)?, pictures.get(index)?);
                    thumbs.get_media(gpu, egui_renderer, item.id, picture, songs_theme).map(|texture| (texture, false))
                }
            };
            commands.extend(home_ui(ui, show, home_state, theme, &mut thumb, preview, canvas));
        });
        egui_state.handle_platform_output(home_window, std::mem::take(&mut full.platform_output));

        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // Testing aid: redraw continuously, to check that memory stays flat under load.
        let delay = if std::env::var_os("PRESENTERCRAFT_STRESS").is_some() { Duration::ZERO } else { delay };
        running.next_repaint = Instant::now().checked_add(delay);

        let target = UiTarget {
            window: home_window,
            surface: home_surface,
            config: home_config,
            renderer: egui_renderer,
            ctx: egui_ctx,
            label: "home window",
        };
        let (jobs, screen) = paint_ui(gpu, target, full);

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
            // Testing aid: open a tab under the Program panel, and show the first message.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(tab) = std::env::var("PRESENTERCRAFT_LIVE_TAB")
            {
                let state = &mut running.home_state;
                state.live_tab = match tab.as_str() {
                    "messages" => 1,
                    "stage" => 2,
                    _ => 0,
                };
                if let Some(first) = state.messages.first() {
                    state.message_selected = Some(first.id);
                    state.message_live = Some((first.id, Instant::now()));
                }
                if let Some(first) = state.timers.first_mut() {
                    state.timer_open = Some(first.id);
                    first.start(Instant::now() - Duration::from_secs(83));
                }
            }
            // Testing aid: open the Reflow editor on the first song.
            if running.frames == GO_LIVE_AT + 2
                && std::env::var_os("PRESENTERCRAFT_REFLOW").is_some()
                && let Some(song) = self.show.presentations().first()
            {
                running.home_state.reflow = Some((0, pc_model::to_song_text(song)));
            }
            // Testing aid: show the Bible page.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_BIBLE").is_some() {
                running.home_state.bible_open = true;
                // Testing aid: look a reference up as if it had been typed.
                if let Ok(reference) = std::env::var("PRESENTERCRAFT_BIBLE_LOOKUP") {
                    running.home_state.bible.lookup = reference;
                    running.home_state.bible.look_up(None);
                }
                // Testing aid: open the Options pop-up on the named pane.
                if let Ok(pane) = std::env::var("PRESENTERCRAFT_BIBLE_OPTIONS") {
                    running.home_state.bible.options.open = true;
                    running.home_state.bible.options.pane = usize::from(pane == "bibles");
                }
            }
            // Testing aid: as if Command-N had been pressed.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_NEW_SONG").is_some() {
                running.home_state.new_song = true;
            }
            // Testing aid: show the slides in another view.
            if running.frames == GO_LIVE_AT + 2 {
                match std::env::var("PRESENTERCRAFT_SLIDE_VIEW").as_deref() {
                    Ok("easy") => running.home_state.slide_view = home::SlideView::Easy,
                    Ok("table") => running.home_state.slide_view = home::SlideView::Table,
                    _ => {}
                }
            }
            // Testing aid: open the Theme Editor, optionally with its first box selected.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(what) = std::env::var("PRESENTERCRAFT_CAPTURE_THEMES")
            {
                running.home_state.theme_request = Some(home::ThemeRequest::Edit(0));
                running.home_state.inspector.tab = inspector::InspectorTab::Text;
                self.capture_selects_box = what == "box";
            }
            if running.frames == GO_LIVE_AT + 4
                && self.capture_selects_box
                && let Some(draft) = running.home_state.editing.as_mut()
            {
                draft.selected = draft.slide.objects.first().map(|object| object.id);
            }
            // Testing aid: as if the theme's name and its first template's name had been
            // clicked to rename them.
            if running.frames == GO_LIVE_AT + 4
                && std::env::var("PRESENTERCRAFT_CAPTURE_THEMES").as_deref() == Ok("rename")
                && let Some(edit) = running.home_state.theme_edit.as_ref()
            {
                let template = edit.theme.templates.first().map(|t| t.name.clone()).unwrap_or_default();
                running.home_state.renaming_theme = Some(edit.theme.name.clone());
                running.home_state.renaming_template = Some((0, template));
            }
            // Testing aid: also open the slide editor, to check how it looks.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_EDITOR").is_some() {
                let at = pc_engine::SlideRef { presentation: 0, slide: 2 };
                running.home_state.editing = EditDraft::of(&self.show, at);
                if let Some(draft) = running.home_state.editing.as_mut() {
                    draft.selected = draft.slide.objects.first().map(|object| object.id);
                    // Testing aid: as if the box had been double-clicked.
                    if std::env::var("PRESENTERCRAFT_CAPTURE_EDITOR").as_deref() == Ok("typing") {
                        draft.typing = draft.selected;
                        draft.typing_started = true;
                    }
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

    /// Opens the Screens window when it is asked for, and closes it when it is not.
    fn sync_screens_window(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let wanted = running.home_state.settings_open;
        if !wanted {
            running.screens_window = None;
            return;
        }
        if running.screens_window.is_some() {
            return;
        }
        match open_panel_window(el, running, quiet) {
            Ok(panel) => {
                panel.window.request_redraw();
                running.screens_window = Some(panel);
            }
            Err(problem) => {
                running.home_state.settings_open = false;
                running.home_state.notice = Some(problem);
            }
        }
    }

    /// Draws one frame of the Screens window and returns what was asked for in it.
    fn draw_screens_window(&mut self) -> Vec<Command> {
        let output_on = self.show.output_enabled();
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running { screens_window, gpu, home_state, .. } = running;
        let Some(panel) = screens_window.as_mut() else {
            return Vec::new();
        };
        let raw_input = panel.state.take_egui_input(&panel.window);
        let mut commands = Vec::new();
        let mut full = panel.ctx.run_ui(raw_input, |ui| commands.extend(home::screens_ui(ui, home_state, output_on)));
        panel.state.handle_platform_output(&panel.window, std::mem::take(&mut full.platform_output));
        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        panel.next_repaint = Instant::now().checked_add(delay);
        let target = UiTarget {
            window: &panel.window,
            surface: &panel.surface,
            config: &panel.config,
            renderer: &mut panel.renderer,
            ctx: &panel.ctx,
            label: "screens window",
        };
        let (jobs, screen) = paint_ui(gpu, target, full);
        panel.frames += 1;
        // Testing aid: a capture run with Settings asked for also saves this window, every
        // frame once it has settled, so the last one saved is the latest.
        if let Some(path) = &self.options.capture_home
            && panel.frames >= 2
            && let Err(e) =
                capture(gpu, &panel.renderer, &jobs, &screen, panel.config.format, &path.with_extension("screens.png"))
        {
            eprintln!("Could not save the Screens window: {e}");
        }
        commands
    }

    /// Carries out a change asked for from a slide's right-click menu.
    fn handle_slide_request(&mut self) {
        use home::SlideRequest;
        let Some((at, request)) = self.running.as_mut().and_then(|r| r.home_state.slide_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(at.presentation).cloned() else { return };
        let (changed, shift) = match &request {
            SlideRequest::Delete => (song.without_slide(at.slide), Some(false)),
            SlideRequest::PasteAfter(slide) => (song.with_slide_after(at.slide, slide), Some(true)),
            SlideRequest::Group(name) => (song.with_group_starting(at.slide, name), None),
            SlideRequest::Label(label) => {
                let slide = song.slide(at.slide).map(|(_, slide)| Slide { label: label.clone(), ..slide.clone() });
                (slide.and_then(|slide| song.with_slide(at.slide, slide)), None)
            }
        };
        let Some(changed) = changed else { return };
        let mut changes = self.show.replace_presentation(at.presentation, Arc::new(changed));
        match shift {
            Some(false) => changes.slides |= self.show.slides_shifted(at.presentation, at.slide, false).slides,
            Some(true) => changes.slides |= self.show.slides_shifted(at.presentation, at.slide + 1, true).slides,
            None => {}
        }
        if !self.unsaved.contains(&at.presentation) {
            self.unsaved.push(at.presentation);
        }
        self.save_unsaved();
        if changes.slides {
            self.refresh_output();
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.selected_slide = None;
            running.home_window.request_redraw();
        }
    }

    /// Carries out a change asked for from a song's right-click menu in the library.
    fn handle_song_request(&mut self) {
        use home::SongRequest;
        let Some((index, request)) = self.running.as_mut().and_then(|r| r.home_state.song_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        match request {
            SongRequest::Duplicate => {
                let mut copy = (*song).clone();
                copy.id = Uuid::new_v4();
                copy.title = format!("{} copy", song.title);
                for group in &mut copy.groups {
                    group.id = Uuid::new_v4();
                    for slide in &mut group.slides {
                        slide.id = Uuid::new_v4();
                        for object in &mut slide.objects {
                            object.id = Uuid::new_v4();
                        }
                    }
                }
                let path = bible_store::data_dir().map(|dir| dir.join("Songs").join(format!("{}.pcpres", copy.id)));
                if let Some(folder) = path.as_ref().and_then(|path| path.parent()) {
                    let _ = std::fs::create_dir_all(folder);
                }
                let title = copy.title.clone();
                let at = self.show.add_presentation(Arc::new(copy));
                self.sources.push(path.clone());
                self.unsaved.push(at);
                self.save_unsaved();
                self.apply(vec![Command::Select(at)]);
                if let Some(running) = self.running.as_mut() {
                    running.home_state.song_keys.push(match &path {
                        Some(path) => path.with_extension("").to_string_lossy().into_owned(),
                        None => format!("title:{title}"),
                    });
                }
            }
            SongRequest::Delete => {
                // A song made in the app goes to the Trash; one opened from elsewhere stays put.
                let own = bible_store::data_dir().map(|dir| dir.join("Songs"));
                let mut problem = None;
                if let (Some(Some(path)), Some(own)) = (self.sources.get(index), own)
                    && path.starts_with(&own)
                    && path.exists()
                    && let Err(e) = trash::delete(path)
                {
                    problem = Some(format!("Could not move the song to the Trash: {e}"));
                }
                if problem.is_none() {
                    if self.show.live().is_some_and(|live| live.presentation == index) {
                        self.apply(vec![Command::Clear(pc_engine::Layer::Slides)]);
                    }
                    self.unsaved.retain(|at| *at != index);
                    if let Some(running) = self.running.as_mut() {
                        running.home_state.removed_songs.push(index);
                    }
                    // Another song is shown in its place.
                    let removed = self.running.as_ref().map(|r| r.home_state.removed_songs.clone()).unwrap_or_default();
                    let next = (0..self.show.presentations().len())
                        .find(|at| !removed.contains(at) && !self.bible_slots.contains(at));
                    if self.show.selected() == index
                        && let Some(next) = next
                    {
                        self.apply(vec![Command::Select(next)]);
                    }
                }
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notice = problem;
                }
            }
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
    }

    /// Saves the timers and messages when they change, takes a message off when its time is up,
    /// and tells the output what to show over the slides when that changes.
    fn sync_overlay(&mut self) {
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &mut running.home_state;
        if std::mem::take(&mut state.timers_changed)
            && let Err(problem) = timers::save(&state.timers)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.messages_changed)
            && let Err(problem) = messages::save(&state.messages)
        {
            state.notice = Some(problem);
        }
        // A message that leaves by itself, once its time is up.
        if let Some((id, since)) = state.message_live
            && let Some(after) = state.messages.iter().find(|message| message.id == id).and_then(|m| m.dismiss_after)
            && since.elapsed().as_secs() >= u64::from(after)
        {
            state.message_live = None;
        }
        let message = state
            .message_live
            .and_then(|(id, _)| state.messages.iter().find(|message| message.id == id))
            .map(|message| message.text.clone());
        let overlay = output::Overlay {
            message,
            stage_message: state.stage_message_live.then(|| state.stage_message.clone()),
            timers: state.timers.clone(),
        };
        // While a timer runs, or a message counts down to leaving, the window shows it each second.
        let ticking = state.timers.iter().any(Timer::running) || state.message_live.is_some();
        if ticking {
            let soon = Instant::now() + Duration::from_millis(500);
            running.next_repaint = Some(running.next_repaint.map_or(soon, |at| at.min(soon)));
        }
        if running.overlay_sent.as_ref() == Some(&overlay) {
            return;
        }
        let message_changed = running.overlay_sent.as_ref().is_none_or(|sent| sent.message != overlay.message);
        if let Some(output) = &running.output {
            output.send(OutputMsg::Overlay(overlay.clone()));
        }
        running.overlay_sent = Some(overlay);
        if message_changed {
            self.refresh_output();
        }
    }

    /// Carries out a choice from the menu bar.
    fn menu_action(&mut self, el: &ActiveEventLoop, action: menubar::Action) {
        use menubar::Action as A;
        if action == A::Quit {
            return self.quit(el);
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &mut running.home_state;
        let mut commands = Vec::new();
        // Shortcuts the views act on are handed to them as the key press they stand for.
        let mut press = |key: egui::Key, modifiers: egui::Modifiers| {
            let events = &mut running.egui_state.egui_input_mut().events;
            for pressed in [true, false] {
                events.push(egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers });
            }
        };
        let command = egui::Modifiers::COMMAND;
        // A change to the selected objects is made by the editor, in its own frame.
        let request = |which| state.editing.is_some().then_some(which);
        match action {
            A::DeselectAll
            | A::Bold
            | A::Italic
            | A::Underline
            | A::Bigger
            | A::Smaller
            | A::AlignLeft
            | A::AlignCenter
            | A::AlignRight
            | A::Justify
            | A::UpperCase
            | A::LowerCase
            | A::Capitalize
            | A::CopyStyle
            | A::PasteStyle
            | A::BringForward
            | A::BringToFront
            | A::SendBackward
            | A::SendToBack
            | A::Lock
            | A::UnlockAll => {
                use home::EditRequest as E;
                state.edit_request = request(match action {
                    A::DeselectAll => E::DeselectAll,
                    A::Bold => E::Bold,
                    A::Italic => E::Italic,
                    A::Underline => E::Underline,
                    A::Bigger => E::Bigger,
                    A::Smaller => E::Smaller,
                    A::AlignLeft => E::Align(pc_model::Align::Left),
                    A::AlignCenter => E::Align(pc_model::Align::Center),
                    A::AlignRight => E::Align(pc_model::Align::Right),
                    A::Justify => E::Align(pc_model::Align::Justify),
                    A::UpperCase => E::UpperCase,
                    A::LowerCase => E::LowerCase,
                    A::Capitalize => E::Capitalize,
                    A::CopyStyle => E::CopyStyle,
                    A::PasteStyle => E::PasteStyle,
                    A::BringForward => E::BringForward,
                    A::BringToFront => E::BringToFront,
                    A::SendBackward => E::SendBackward,
                    A::SendToBack => E::SendToBack,
                    A::Lock => E::Lock,
                    _ => E::UnlockAll,
                });
            }
            A::Quit => {}
            A::Settings | A::ConfigureScreens => state.settings_open = true,
            A::NewSong => state.new_song = true,
            A::NewSlide => {
                let song = match (&state.editing, &state.theme_edit) {
                    (Some(draft), None) => draft.at.presentation,
                    _ => self.show.selected(),
                };
                if !self.bible_slots.contains(&song) && song < self.show.presentations().len() {
                    state.add_slide = Some(song);
                }
            }
            A::CloseWindow => {
                // The front-most thing that can close: the Screens window, a question, or a
                // screen's own window, which switches that screen off.
                if state.settings_open {
                    state.settings_open = false;
                    state.identify = false;
                } else if state.confirm_delete_template.is_some() || state.confirm_delete_theme.is_some() {
                    state.confirm_delete_template = None;
                    state.confirm_delete_theme = None;
                } else if let Some(screen) =
                    running.screen_windows.iter().find(|have| Some(have.window.id()) == self.focused).map(|w| w.screen)
                    && let Some(closed) = state.screens.iter_mut().find(|have| have.id == screen)
                {
                    closed.output = screens::Output::Off;
                    state.screens_changed = true;
                }
            }
            A::Undo => press(egui::Key::Z, command),
            A::Redo => press(egui::Key::Z, command | egui::Modifiers::SHIFT),
            A::Duplicate => press(egui::Key::D, command),
            A::SelectAll => press(egui::Key::A, command),
            A::Delete => press(egui::Key::Delete, egui::Modifiers::NONE),
            A::Cut => running.egui_state.egui_input_mut().events.push(egui::Event::Cut),
            A::Copy => running.egui_state.egui_input_mut().events.push(egui::Event::Copy),
            A::Paste => {
                if let Some(text) = running.egui_state.clipboard_text().filter(|text| !text.is_empty()) {
                    running.egui_state.egui_input_mut().events.push(egui::Event::Paste(text.replace("\r\n", "\n")));
                }
            }
            A::ShowView => state.view_request = Some(0),
            A::EditView => state.view_request = Some(1),
            A::BibleView => state.view_request = Some(2),
            A::ThemesView => state.view_request = Some(3),
            A::ReflowView => {
                let song = self.show.selected();
                if !self.bible_slots.contains(&song)
                    && let Some(presentation) = self.show.presentations().get(song)
                {
                    state.editing = None;
                    state.bible_open = false;
                    state.reflow = Some((song, pc_model::to_song_text(presentation)));
                }
            }
            A::TimersTab => state.live_tab = 0,
            A::MessagesTab => state.live_tab = 1,
            A::StageTab => state.live_tab = 2,
            A::GridView => state.slide_view = home::SlideView::Grid,
            A::EasyView => state.slide_view = home::SlideView::Easy,
            A::TableView => state.slide_view = home::SlideView::Table,
            A::NextSlide => commands.push(Command::Next),
            A::PreviousSlide => commands.push(Command::Previous),
            A::ClearAll => commands.push(Command::ClearAll),
            A::ClearSlide => commands.push(Command::Clear(pc_engine::Layer::Slides)),
            A::ClearMedia => commands.push(Command::Clear(pc_engine::Layer::Media)),
            A::Audience => commands.push(Command::SetOutputEnabled(!self.show.output_enabled())),
            A::Stage => state.stage_on = !state.stage_on,
            A::IdentifyScreens => state.identify = !state.identify,
            A::Website | A::WhatsNew | A::ReportIssue => {
                let page = match action {
                    A::WhatsNew => "https://github.com/esraell/PresenterCraft/commits/master",
                    A::ReportIssue => "https://github.com/esraell/PresenterCraft/issues/new",
                    _ => "https://github.com/esraell/PresenterCraft#readme",
                };
                if let Err(e) = std::process::Command::new("open").arg(page).spawn() {
                    state.notice = Some(format!("Could not open the web page: {e}"));
                }
            }
        }
        running.home_window.request_redraw();
        if !commands.is_empty() {
            self.apply(commands);
        }
    }

    /// Ticks the menu bar's items that are on now: the view, the slide view, the audience
    /// output and Identify Screens.
    fn tick_menu(&mut self) {
        #[cfg(target_os = "macos")]
        if let (Some(menu), Some(running)) = (self.menu.as_mut(), self.running.as_ref()) {
            use menubar::Action as A;
            let state = &running.home_state;
            let view = if state.theme_edit.is_some() {
                A::ThemesView
            } else if state.editing.is_some() {
                A::EditView
            } else if state.bible_open {
                A::BibleView
            } else {
                A::ShowView
            };
            let slide_view = match state.slide_view {
                home::SlideView::Grid => A::GridView,
                home::SlideView::Easy => A::EasyView,
                home::SlideView::Table => A::TableView,
            };
            let output_on = self.show.output_enabled();
            menu.set_editing(state.editing.is_some());
            menu.set_checked(|action| match action {
                A::Audience => output_on,
                A::Stage => state.stage_on,
                A::TimersTab => state.live_tab == 0,
                A::MessagesTab => state.live_tab == 1,
                A::StageTab => state.live_tab == 2,
                A::IdentifyScreens => state.identify,
                _ => action == view || action == slide_view,
            });
        }
    }

    fn quit(&mut self, el: &ActiveEventLoop) {
        self.save_unsaved();
        if let Some(output) = self.running.as_mut().and_then(|r| r.output.take()) {
            output.shutdown();
        }
        el.exit();
    }
}

impl ApplicationHandler<String> for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.running.is_some() {
            return;
        }
        match self.start(el) {
            Ok(Some(mut running)) => {
                // The Bible page and the small choices are as they were left.
                if let Some(page) = service::load_bible_page() {
                    running.home_state.bible.apply_settings(&page);
                }
                self.bible_page_saved = running.home_state.bible.choices();
                let prefs = service::load_prefs();
                running.home_state.apply_prefs(&prefs);
                // The screens, as they were left. The first start after screens could be
                // listed makes the list from how the output was set up before: one screen
                // placed automatically, and one sent over NDI if NDI was on.
                let quiet = self.quiet();
                running.home_state.screens = screens::load().unwrap_or_else(|| {
                    let ndi = (prefs.ndi_on && running.home_state.ndi_available).then_some(prefs.ndi_name.as_str());
                    let made = screens::starting(ndi);
                    if !quiet {
                        let _ = screens::save(&made);
                    }
                    made
                });
                self.prefs_saved = Some(prefs);
                running.home_state.timers = timers::load();
                running.home_state.messages = messages::load();
                // A display asked for on the command line takes the first screen, this run only.
                if let Some(index) = self.options.output_display
                    && let Some(display) = display_list(el.available_monitors()).into_iter().find(|d| d.index == index)
                    && let Some(first) = running.home_state.screens.first_mut()
                {
                    first.output = screens::Output::Display(display.name);
                }
                // Testing aid: also send over NDI under this name, this run only.
                if let Ok(name) = std::env::var("PRESENTERCRAFT_NDI") {
                    let screen = screens::Screen::new("NDI".to_owned(), screens::Output::Ndi(name));
                    running.home_state.screens.push(screen);
                }
                running.home_window.request_redraw();
                self.running = Some(running);
                self.sync_screens(el);
                // The menu bar, for a run someone is using.
                #[cfg(target_os = "macos")]
                if !self.quiet() {
                    match menubar::MenuBar::install() {
                        Ok(menu) => self.menu = Some(menu),
                        Err(problem) => eprintln!("{problem}"),
                    }
                }
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

    fn user_event(&mut self, el: &ActiveEventLoop, id: String) {
        if let Some(action) = menubar::action_of(&id) {
            self.menu_action(el, action);
            self.sync_screens_window(el);
            if let Some(panel) = self.running.as_ref().and_then(|r| r.screens_window.as_ref()) {
                panel.window.request_redraw();
            }
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::ModifiersChanged(now) => self.held = now.state(),
            WindowEvent::Focused(true) => self.focused = Some(id),
            _ => {}
        }
        // A key that is a menu bar shortcut belongs to the menu, which acts on it once.
        #[cfg(target_os = "macos")]
        if self.menu.is_some()
            && let WindowEvent::KeyboardInput { event: key, .. } = &event
            && key.state == ElementState::Pressed
            && let winit::keyboard::PhysicalKey::Code(code) = key.physical_key
            && menubar::claims(code, self.held)
        {
            return;
        }
        let Some(running) = self.running.as_mut() else {
            return;
        };
        if let Some(panel) = running.screens_window.as_mut().filter(|panel| panel.window.id() == id) {
            match &event {
                WindowEvent::CloseRequested => {
                    running.home_state.settings_open = false;
                    running.home_state.identify = false;
                    running.screens_window = None;
                    running.home_window.request_redraw();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        panel.config.width = size.width;
                        panel.config.height = size.height;
                        panel.surface.configure(&running.gpu.device, &panel.config);
                    }
                    panel.window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let commands = self.draw_screens_window();
                    self.apply(commands);
                    self.sync_screens(el);
                }
                _ => {
                    if panel.state.on_window_event(&panel.window, &event).repaint {
                        panel.window.request_redraw();
                    }
                }
            }
            return;
        }
        if let Some(screen) = running.screen_windows.iter().find(|have| have.window.id() == id).map(|have| have.screen)
        {
            match event {
                WindowEvent::Resized(size) => {
                    if let Some(output) = &running.output {
                        output.send(OutputMsg::Resized(screen, size.width, size.height));
                    }
                }
                // Closing a screen's window switches that screen off; it stays in the list.
                WindowEvent::CloseRequested => {
                    if let Some(closed) = running.home_state.screens.iter_mut().find(|have| have.id == screen) {
                        closed.output = screens::Output::Off;
                    }
                    running.home_state.screens_changed = true;
                    running.home_window.request_redraw();
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
                    if !self.held.super_key()
                        && !self.held.control_key()
                        && let Some(command) = show_key_command(&event.logical_key, &self.show)
                    {
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
            && !self.held.super_key()
            && !self.held.control_key()
            && let Some(command) = show_key_command(&key.logical_key, &self.show)
        {
            // Escape clears the screen only when there is nothing else for it to close: a
            // pop-up or the Settings window takes it first.
            let state = &running.home_state;
            let something_open =
                state.settings_open || state.bible.options.open || egui::Popup::is_any_open(&running.egui_ctx);
            let escape = matches!(key.logical_key, Key::Named(NamedKey::Escape));
            if !(escape && something_open) {
                self.apply(vec![command]);
                return;
            }
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
                self.handle_slide_request();
                self.handle_reflow();
                self.handle_media_request();
                self.handle_song_request();
                // A template chosen from a slide's menu: the slide is changed and, if it is on
                // screen, the screen too.
                if let Some((at, template)) = self.running.as_mut().and_then(|r| r.home_state.pending_template.take())
                    && let Some((_, slide)) =
                        self.show.presentations().get(at.presentation).and_then(|p| p.slide(at.slide))
                {
                    let slide = Slide { template, ..slide.clone() };
                    self.apply_edit(at, slide);
                    if self.show.live() == Some(at) {
                        self.refresh_output();
                    }
                }
                // A Bible passage just looked up goes into its slot. It is never the slot on
                // screen, so the output is untouched.
                if let Some((slot, passage)) = self.running.as_mut().and_then(|r| r.home_state.bible.pending.take()) {
                    self.show.replace_presentation(slot, Arc::new(passage));
                    if let Some(running) = self.running.as_ref() {
                        running.home_window.request_redraw();
                    }
                }
                self.handle_themes();
                self.handle_songs();
                // The Bible page's set-up (translations, boxes, options) is remembered the
                // moment it changes, so it is as it was left at the next start.
                if let Some(running) = self.running.as_mut() {
                    let now = running.home_state.bible.choices();
                    if now != self.bible_page_saved {
                        let page =
                            service::SavedPassage { lookup: String::new(), ..running.home_state.bible.settings() };
                        if let Err(problem) = service::save_bible_page(&page) {
                            running.home_state.notice = Some(problem);
                        }
                        self.bible_page_saved = now;
                    }
                }
                // So are the small choices: transition, view, slide size.
                if let Some(running) = self.running.as_mut() {
                    let now = running.home_state.prefs();
                    if self.prefs_saved.as_ref() != Some(&now) && self.frames_since_start > 10 {
                        if let Err(problem) = service::save_prefs(&now) {
                            running.home_state.notice = Some(problem);
                        }
                        self.prefs_saved = Some(now);
                    }
                    self.frames_since_start = self.frames_since_start.saturating_add(1);
                }
                // The services are written as soon as they change; the file is small.
                if let Some(running) = self.running.as_mut()
                    && std::mem::take(&mut running.home_state.services.changed)
                    && let Err(problem) = service::save(&running.home_state.services)
                {
                    running.home_state.notice = Some(problem);
                }
                self.update_canvas();
                self.sync_screens(el);
                self.sync_screens_window(el);
                self.sync_overlay();
                self.tick_menu();
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
        let mut panel_at = None;
        if let Some(panel) = running.screens_window.as_mut() {
            if panel.next_repaint.is_some_and(|at| at <= Instant::now()) {
                panel.next_repaint = None;
                panel.window.request_redraw();
            }
            panel_at = panel.next_repaint;
        }
        match [running.next_repaint, panel_at, save_at].into_iter().flatten().min() {
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
        // The sample always has the same id, so that once it has been changed and saved the
        // saved one is known to be it.
        let mut sample = parse_song_text("Sample song", load::SAMPLE_SONG);
        sample.id = SAMPLE_SONG_ID;
        vec![(sample, None)]
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
    let (mut songs, mut sources): (Vec<_>, Vec<_>) = songs.into_iter().unzip();
    // The songs made in the app are kept in its Songs folder; they are always loaded.
    let own: Vec<PathBuf> = bible_store::data_dir()
        .and_then(|dir| std::fs::read_dir(dir.join("Songs")).ok())
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "pcpres"))
        .collect();
    let (mut made_here, problems) = load::load_songs(&own);
    for problem in &problems {
        eprintln!("{problem}");
    }
    made_here.sort_by_key(|loaded| loaded.song.title.to_lowercase());
    // The sample song stays in the library. Once it has been changed it is saved like any
    // other song, and the saved one is shown in place of the one built in.
    if song_paths.is_empty() && made_here.iter().any(|loaded| loaded.song.id == SAMPLE_SONG_ID) {
        songs.clear();
        sources.clear();
    }
    for loaded in made_here {
        if !sources.contains(&Some(loaded.path.clone())) {
            songs.push(loaded.song);
            sources.push(Some(loaded.path));
        }
    }
    // Two empty presentations after the songs hold Bible passages. They have no file.
    let bible_slots = [songs.len(), songs.len() + 1];
    for _ in bible_slots {
        songs.push(pc_model::Presentation {
            id: pc_model::Uuid::new_v4(),
            title: "Bible".to_owned(),
            groups: Vec::new(),
        });
        sources.push(None);
    }

    // Menu bar choices arrive as user events carrying the item's id.
    let mut builder = EventLoop::<String>::with_user_event();
    // A run that only saves a picture must not come to the front of whatever is in use.
    #[cfg(target_os = "macos")]
    {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        // The app puts up its own menu bar once it has started.
        builder.with_default_menu(false);
        if options.capture_home.is_some() || std::env::var_os("PRESENTERCRAFT_QUIET").is_some() {
            builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
        }
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("Could not start the window system: {e}");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    // A menu bar choice wakes the event loop with the item's id.
    #[cfg(target_os = "macos")]
    {
        let proxy = std::sync::Mutex::new(event_loop.create_proxy());
        muda::MenuEvent::set_event_handler(Some(move |event: muda::MenuEvent| {
            if let Ok(proxy) = proxy.lock() {
                let _ = proxy.send_event(event.id().as_ref().to_owned());
            }
        }));
    }
    let (themes, songs_theme, bible_theme, bible_layout) = theme_store::load_all();
    let mut app = App {
        options,
        show: Show::new(songs.into_iter().map(Arc::new).collect()),
        sources,
        bible_slots,
        pictures: Vec::new(),
        unsaved: Vec::new(),
        unsaved_since: None,
        theme: Arc::new(themes[songs_theme].clone()),
        theme_for_bible: Arc::new(themes[bible_theme].clone()),
        themes,
        songs_theme,
        bible_theme,
        bible_layout,
        themes_unsaved: Vec::new(),
        bible_page_saved: String::new(),
        prefs_saved: None,
        frames_since_start: 0,
        held: winit::keyboard::ModifiersState::empty(),
        capture_selects_box: false,
        focused: None,
        #[cfg(target_os = "macos")]
        menu: None,
        running: None,
        failed: false,
        capture_done: false,
    };
    // The pictures that were in the media bin last time, then any named on the command line.
    let mut pictures_to_load = media::load_list();
    for path in &media_paths {
        if !pictures_to_load.contains(path) {
            pictures_to_load.push(path.clone());
        }
    }
    for path in &pictures_to_load {
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
        let song = pc_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n\nThree\n\n[Verse 2]\nFour\n");
        let show = Show::new(vec![Arc::new(song)]);
        let key = |key: Key| show_key_command(&key, &show);
        let jump = |slide| Some(Command::GoTo(pc_engine::SlideRef { presentation: 0, slide }));
        assert_eq!(key(Key::Named(NamedKey::ArrowRight)), Some(Command::Next));
        assert_eq!(key(Key::Named(NamedKey::Space)), Some(Command::Next));
        assert_eq!(key(Key::Named(NamedKey::ArrowLeft)), Some(Command::Previous));
        assert_eq!(key(Key::Named(NamedKey::Home)), Some(Command::First));
        assert_eq!(key(Key::Named(NamedKey::End)), Some(Command::Last));
        assert_eq!(key(Key::Named(NamedKey::F1)), Some(Command::ClearAll));
        assert_eq!(key(Key::Named(NamedKey::Escape)), Some(Command::ClearAll));
        assert_eq!(key(Key::Named(NamedKey::Tab)), None);
        // Letters jump to the group they stand for, in either case.
        assert_eq!(key(Key::Character("a".into())), jump(0));
        assert_eq!(key(Key::Character("C".into())), jump(1));
        assert_eq!(key(Key::Character("s".into())), jump(3));
        // A group the song does not have, and a key that stands for none.
        assert_eq!(key(Key::Character("b".into())), None);
        assert_eq!(key(Key::Character("q".into())), None);
    }
}
