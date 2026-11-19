//! PresenterCraft: a home window with a slide grid, and a separate output for the audience.

mod applog;
mod audio;
mod bible;
mod bible_store;
mod displays;
mod editor;
mod home;
mod icons;
mod images;
mod inspector;
mod libraries;
mod links;
mod live_tabs;
mod load;
mod macros;
mod media;
mod menubar;
mod messages;
mod output;
mod props;
mod screens;
mod service;
mod stage;
mod stage_editor;
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
use pc_render::Layers;
use pc_render::vello::util::RenderContext;
use pc_render::wgpu;
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

/// A media playlist playing item after item: which, where in it, since when, and the media bin
/// place shown.
struct MediaCycle {
    playlist: Uuid,
    at: usize,
    since: Instant,
    shown: usize,
}

/// How long a picture stays up in a playing media playlist.
const PICTURE_SECONDS: f32 = 8.0;

/// A song going round on the Announcements layer.
struct Announcing {
    song: usize,
    slide: usize,
    since: Instant,
    shown: output::Announcement,
}

/// How long an announcement slide stays up when it doesn't say.
const ANNOUNCEMENT_SECONDS: f32 = 8.0;

/// The shortest time between two drawings of a window for an animation: one refresh of a
/// 60 Hz screen.
const FRAME: Duration = Duration::from_micros(16_667);

/// The id of the sample song the app starts with.
const SAMPLE_SONG_ID: Uuid = Uuid::from_u128(0x7361_6d70_6c65_5f73_6f6e_6700_0000_0001);

/// The templates of the theme songs use, by id and name, in order.
fn song_templates(theme: &Theme) -> Vec<(Uuid, String)> {
    theme.templates.iter().map(|template| (template.id, template.name.clone())).collect()
}

/// Opens the Screens window: a window of its own, drawn with its own interface.
fn open_panel_window(
    el: &ActiveEventLoop,
    running: &mut Running,
    quiet: bool,
    kind: Panel,
) -> Result<PanelWindow, String> {
    let attrs = Window::default_attributes()
        .with_title(kind.title())
        .with_inner_size(match kind {
            Panel::StageLayouts => winit::dpi::LogicalSize::new(1160.0, 620.0),
            Panel::Screens => winit::dpi::LogicalSize::new(780.0, 640.0),
            Panel::Settings => winit::dpi::LogicalSize::new(760.0, 520.0),
        })
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
    Ok(PanelWindow { kind, window, surface, config, ctx, state, renderer, next_repaint: None, frames: 0 })
}

/// Where the show is, for timing a slide that goes on by itself: the slide on screen, how many
/// of its click builds are done, and its place in the play order.
type AdvanceAt = (pc_engine::SlideRef, usize, Option<(usize, usize)>);

/// How many slides in a row can set each other going through their actions.
const MAX_ACTION_CHAIN: u8 = 3;

/// What Clear All, Clear Messages and Clear Props take off: the message and the props. Returns
/// whether anything was showing.
fn clear_overlays(state: &mut home::HomeState, command: &Command) -> bool {
    let mut cleared = false;
    if matches!(command, Command::ClearAll | Command::Clear(pc_engine::Layer::Messages)) {
        cleared |= state.message_live.take().is_some();
    }
    if matches!(command, Command::ClearAll | Command::Clear(pc_engine::Layer::Props)) && !state.props_live.is_empty() {
        state.props_live.clear();
        cleared = true;
    }
    cleared
}

/// The show commands an action stands for, or `None` for an action that changes the window's
/// own state (a message, a prop, a timer).
fn action_commands(show: &Show, state: &home::HomeState, action: &pc_model::ShowAction) -> Option<Vec<Command>> {
    use pc_model::{ClearWhat, ShowAction};
    match action {
        ShowAction::Clear { what } => Some(vec![match what {
            ClearWhat::All => Command::ClearAll,
            ClearWhat::Slide => Command::Clear(pc_engine::Layer::Slides),
            ClearWhat::Media => Command::Clear(pc_engine::Layer::Media),
            ClearWhat::Props => Command::Clear(pc_engine::Layer::Props),
            ClearWhat::Messages => Command::Clear(pc_engine::Layer::Messages),
        }]),
        ShowAction::Audience { on } => Some(vec![Command::SetOutputEnabled(*on)]),
        ShowAction::Media { id } => {
            Some(show.media().iter().position(|item| item.id == *id).map(Command::ShowMedia).into_iter().collect())
        }
        ShowAction::Song { key } => {
            // The song's first slide in its play order that is not disabled.
            let at = state.song_keys.iter().position(|have| have == key);
            let first = at.and_then(|at| {
                let song = show.presentations().get(at)?;
                song.order().into_iter().find(|slide| song.slide(*slide).is_some_and(|(_, s)| s.enabled))
            });
            Some(match (at, first) {
                (Some(at), Some(slide)) => {
                    vec![Command::Select(at), Command::GoTo(pc_engine::SlideRef { presentation: at, slide })]
                }
                (Some(at), None) => vec![Command::Select(at)],
                _ => Vec::new(),
            })
        }
        ShowAction::Message { .. } | ShowAction::Prop { .. } | ShowAction::Timer { .. } => None,
    }
}

/// Does an action that changes the window's own state: shows or hides a message or a prop,
/// starts, stops or resets a timer.
fn do_state_action(state: &mut home::HomeState, action: &pc_model::ShowAction, now: Instant) {
    use pc_model::{ShowAction, TimerDo};
    match action {
        ShowAction::Message { id, show } => {
            if *show && state.messages.iter().any(|m| m.id == *id) {
                state.message_live = Some((*id, now));
            } else if !*show && state.message_live.is_some_and(|(live, _)| live == *id) {
                state.message_live = None;
            }
        }
        ShowAction::Prop { id, show } => {
            state.props_live.retain(|(live, _)| live != id);
            if *show && state.props.iter().any(|p| p.id == *id) {
                state.props_live.push((*id, now));
            }
        }
        ShowAction::Timer { id, act } => {
            if let Some(timer) = state.timers.iter_mut().find(|t| t.id == *id) {
                match act {
                    TimerDo::Start => timer.start(now),
                    TimerDo::Stop => timer.stop(now),
                    TimerDo::Reset => timer.reset(),
                }
            }
        }
        ShowAction::Clear { .. } | ShowAction::Audience { .. } | ShowAction::Song { .. } | ShowAction::Media { .. } => {
        }
    }
}

/// The changes made to songs in the Show view: each step is a song's place and the song as it
/// was. Undo puts the song back and keeps how it is now, to redo.
#[derive(Default)]
struct SongHistory {
    undo: Vec<(usize, Arc<pc_model::Presentation>)>,
    redo: Vec<(usize, Arc<pc_model::Presentation>)>,
}

impl SongHistory {
    /// Keeps a song as it is before a change, and drops what could be redone.
    fn remember(&mut self, index: usize, song: Arc<pc_model::Presentation>) {
        self.undo.push((index, song));
        if self.undo.len() > 100 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    /// Drops every step for a song, after it was changed in a way that isn't kept here.
    fn forget(&mut self, index: usize) {
        self.undo.retain(|(at, _)| *at != index);
        self.redo.retain(|(at, _)| *at != index);
    }

    /// The song the next undo (or redo) would change.
    fn peek(&self, redo: bool) -> Option<usize> {
        if redo { self.redo.last() } else { self.undo.last() }.map(|(index, _)| *index)
    }

    /// Takes the next undo (or redo) step, keeping the song as it is now to step back to.
    fn step(
        &mut self,
        redo: bool,
        current: Arc<pc_model::Presentation>,
    ) -> Option<(usize, Arc<pc_model::Presentation>)> {
        let (from, to) = if redo { (&mut self.redo, &mut self.undo) } else { (&mut self.undo, &mut self.redo) };
        let (index, song) = from.pop()?;
        to.push((index, current));
        Some((index, song))
    }
}

/// The user event the output thread sends when it has drawn a new live preview.
const LIVE_PREVIEW_EVENT: &str = "presentercraft.live-preview";

/// What a theme calls its templates, in order.
fn template_names(theme: &Theme) -> Vec<String> {
    theme.templates.iter().map(|template| template.name.clone()).collect()
}

/// The keys that run the show. They are read straight from the window, before the interface
/// draws, so a key press reaches the output without waiting for the home window. A slide's own
/// hot key puts it on screen; otherwise a letter jumps to the group it stands for (A for the
/// first verse, C for the chorus, and so on).
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
        Key::Named(NamedKey::F4) => Some(Command::Clear(pc_engine::Layer::Props)),
        Key::Named(NamedKey::F5) => Some(Command::Clear(pc_engine::Layer::Audio)),
        Key::Named(NamedKey::F6) => Some(Command::Clear(pc_engine::Layer::Messages)),
        Key::Character(typed) => {
            let mut letters = typed.chars();
            let (Some(letter), None) = (letters.next(), letters.next()) else { return None };
            show.hot_key_slide(letter).or_else(|| show.group_start(pc_model::groups_for_key(letter))).map(Command::GoTo)
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
    let frame = match output::next_texture(surface) {
        Some(wgpu::CurrentSurfaceTexture::Success(frame) | wgpu::CurrentSurfaceTexture::Suboptimal(frame)) => {
            Some(frame)
        }
        Some(wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost) | None => {
            output::configure(&gpu.device, surface, config);
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

/// Which window of its own a panel window is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Panel {
    Screens,
    Settings,
    StageLayouts,
}

impl Panel {
    const ALL: [Panel; 3] = [Panel::Screens, Panel::Settings, Panel::StageLayouts];

    fn title(self) -> &'static str {
        match self {
            Panel::Screens => "Screens",
            Panel::Settings => "Settings",
            Panel::StageLayouts => "Stage Layouts",
        }
    }

    /// True while the window is asked for.
    fn wanted(self, state: &HomeState) -> bool {
        match self {
            Panel::Screens => state.settings_open,
            Panel::Settings => state.preferences_open,
            Panel::StageLayouts => state.stage_editor_open,
        }
    }

    /// Says the window is no longer wanted, as closing it does.
    fn close(self, state: &mut HomeState) {
        match self {
            Panel::Screens => {
                state.settings_open = false;
                state.identify = false;
            }
            Panel::Settings => state.preferences_open = false,
            Panel::StageLayouts => state.stage_editor_open = false,
        }
    }
}

/// A window of its own with its own interface: Screens, or Settings.
struct PanelWindow {
    kind: Panel,
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
    identify_sent: Option<Vec<(Uuid, String, pc_model::Rgba)>>,
    /// The screen looks last sent to the output.
    looks_sent: Vec<(Uuid, screens::Look)>,
    /// The stage screens the output was last told of, and whether they were on.
    stage_sent: Option<(Vec<(Uuid, pc_model::ThemeTemplate)>, bool)>,
    /// What the output was last told to show over the slides.
    overlay_sent: Option<output::Overlay>,
    output: Option<OutputHandle>,
    /// The Screens and Settings windows, while they are open.
    panels: Vec<PanelWindow>,
    gpu: Gpu,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    thumbs: Thumbs,
    preview: Option<egui::TextureId>,
    /// The live preview's two textures, as the window knows them.
    live_preview_ids: Vec<egui::TextureId>,

    /// The preview picture before the last change, and when that change was, so the preview
    /// fades as the output does.
    preview_before: Option<egui::TextureId>,
    preview_changed: Instant,
    /// True when the Program preview last drew the stage picture, and when it did.
    preview_shows_stage: bool,
    stage_preview_at: Instant,
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
    /// Wakes the event loop from other threads (the output thread, for a new live preview).
    waker: Option<winit::event_loop::EventLoopProxy<String>>,
    show: Show,
    /// The file each song came from, in the show's order; `None` for the bundled sample.
    sources: Vec<Option<PathBuf>>,
    /// The presentations kept for Bible passages.
    bible_slots: [usize; 2],
    /// The decoded picture of each media item, in the show's order: full size for the screens,
    /// and small for the window.
    pictures: Vec<media::MediaPicture>,
    /// Songs changed in the Edit view and not yet written to their files, and when the last
    /// change was made.
    unsaved: Vec<usize>,
    /// The changes made to songs in the Show view, to undo and redo.
    history: SongHistory,
    /// While actions are being done, how far along a chain of slides' actions they are.
    action_depth: Option<u8>,
    /// Where the show is (the slide on screen, its builds, its place in the play order) and
    /// since when, for a slide that goes on to the next by itself; and true once it has.
    live_since: Option<(AdvanceAt, Instant)>,
    advanced: bool,
    /// When the app started.
    started: Instant,
    /// The video on screen, by its media item, playing.
    video: Option<(Uuid, pc_video::Player)>,
    /// A video in a box on the slide on screen, by the file the box names, playing.
    slide_video: Option<(String, pc_video::Player)>,
    /// The song on the Announcements layer: which, the slide shown, since when, and what was
    /// sent to the screens.
    announcing: Option<Announcing>,
    /// A media playlist playing, the files seen in followed folders with when each changed, and
    /// when the folders were last looked at.
    media_cycle: Option<MediaCycle>,
    folders_seen: std::collections::HashMap<std::path::PathBuf, std::time::SystemTime>,
    folders_checked: Instant,
    /// The media playlists read at the start, handed to the window's state once it is made.
    media_playlists: Vec<media::Playlist>,
    unsaved_since: Option<Instant>,
    /// The theme songs are drawn with, and the one Bible slides are drawn with: copies of
    /// the themes at `songs_theme` and `bible_theme` in `themes`.
    theme: Arc<Theme>,
    theme_for_bible: Arc<Theme>,
    themes: Vec<Theme>,
    /// The themes again, shared, for a song that names its own theme.
    theme_arcs: Vec<Arc<Theme>>,
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
    /// Plays the Audio tab's tracks.
    player: audio::Player,
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
    /// How often and how long the operator window is drawn, and the last notice, for the log.
    home_timing: applog::Timing,
    notice_logged: Option<String>,
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

        // Testing aid: open the window at another size, such as 1100x700.
        let (width, height) = std::env::var("PRESENTERCRAFT_WINDOW_SIZE")
            .ok()
            .and_then(|size| {
                let (w, h) = size.split_once('x')?;
                Some((w.parse::<f64>().ok()?, h.parse::<f64>().ok()?))
            })
            .unwrap_or((1440.0, 900.0));
        let home_attrs = Window::default_attributes()
            .with_title("PresenterCraft")
            .with_inner_size(winit::dpi::LogicalSize::new(width, height))
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

        let waker: Option<output::Waker> = self.waker.clone().map(|proxy| {
            let proxy = std::sync::Mutex::new(proxy);
            let wake: output::Waker = Arc::new(move || {
                if let Ok(proxy) = proxy.lock() {
                    let _ = proxy.send_event(LIVE_PREVIEW_EVENT.to_owned());
                }
            });
            wake
        });
        let output = OutputHandle::spawn(gpu.device.clone(), gpu.queue.clone(), self.theme.clone(), waker)?;

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
        // The live preview: two small textures the output thread draws the audience picture
        // into, turn about, for the Program preview to show what the screens show.
        let (live_w, live_h) = output::LIVE_PREVIEW_SIZE;
        let live_textures: Vec<wgpu::Texture> =
            (0..2).map(|_| pc_render::target_texture(&gpu.device, live_w, live_h)).collect();
        let live_preview_ids: Vec<egui::TextureId> = live_textures
            .iter()
            .map(|texture| {
                let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
                egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear)
            })
            .collect();
        output.send(OutputMsg::PreviewTargets(live_textures));

        Ok(Some(Running {
            home_window,
            home_surface,
            home_config,
            screen_windows: Vec::new(),
            retired: Vec::new(),
            ndi_sent: Vec::new(),
            identify_sent: None,
            looks_sent: Vec::new(),
            stage_sent: None,
            overlay_sent: None,
            output: Some(output),
            panels: Vec::new(),
            gpu,
            egui_ctx,
            egui_state,
            egui_renderer,
            thumbs,
            preview,
            live_preview_ids,

            preview_before: None,
            preview_changed: Instant::now(),
            preview_shows_stage: false,
            stage_preview_at: Instant::now(),
            home_state: HomeState {
                fonts: Arc::new(pc_render::Fonts::new().family_names()),
                theme_names: self.themes.iter().map(|theme| theme.name.clone()).collect(),
                theme_ids: self.themes.iter().map(|theme| theme.id).collect(),
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
        let commands: Vec<Command> = commands.into_iter().map(|command| self.continue_playlist(command)).collect();
        let live_before = self.show.live();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let mut any = false;
        let mut output_changed = false;
        if !commands.is_empty() {
            applog::event("command", &format!("{commands:?}"));
        }
        for command in commands {
            // Clear All, and Clear Audio, stop the audio.
            if matches!(command, Command::ClearAll | Command::Clear(pc_engine::Layer::Audio)) {
                self.player.stop();
            }
            // Clear All, and Clear Announcements, take the Announcements layer off.
            if matches!(command, Command::ClearAll | Command::Clear(pc_engine::Layer::Announcements))
                && self.announcing.take().is_some()
            {
                running.home_state.announcement_live = None;
                if let Some(output) = &running.output {
                    output.send(OutputMsg::Announcement(None));
                }
                applog::event("announcements", "cleared");
                output_changed = true;
            }
            if clear_overlays(&mut running.home_state, &command) {
                output_changed = true;
                any = true;
            }
            let changes = self.show.apply(command);
            output_changed |= changes.slides || changes.media || changes.output_enabled || changes.builds;
            if changes.selection {
                let media: Vec<Uuid> = self.show.media().iter().map(|m| m.id).collect();
                running.thumbs.clear_except(&mut running.egui_renderer, |id| media.contains(id));
            }
            any |= changes.slides || changes.media || changes.output_enabled || changes.selection || changes.builds;
        }
        if output_changed {
            self.refresh_output();
        }
        // A slide that has just gone on screen does what it says. One reached by another slide's
        // actions counts one step further along a chain, which stops after a few steps.
        let depth = self.action_depth.map_or(0, |depth| depth + 1);
        if self.show.live() != live_before {
            let what = self.show.live().and_then(|at| {
                let song = self.show.presentations().get(at.presentation)?;
                let group = song.slide(at.slide).map_or("", |(group, _)| group.name.as_str());
                Some(format!("{} slide {} ({group})", song.title, at.slide + 1))
            });
            applog::event("live", &what.unwrap_or_else(|| "cleared".to_owned()));
        }
        if self.show.live() != live_before
            && let Some((_, slide)) = self.show.live_slide()
            && !slide.actions.is_empty()
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.actions_to_run.extend(slide.actions.iter().cloned());
            running.home_state.actions_depth = depth;
        }
        if any && let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// Goes on to the next slide when the one on screen has been there as long as it asks.
    fn tick_advance(&mut self) {
        // Any step counts: a new slide, a click build, or the same slide at another place in
        // the play order (a group an arrangement plays twice in a row).
        let here = self.show.live().map(|at| (at, self.show.build_step(), self.show.position()));
        if here != self.live_since.map(|(at, _)| at) {
            // Testing aid: say when each slide went on screen.
            if std::env::var_os("PRESENTERCRAFT_TRACE_LIVE").is_some() {
                let drawn = self.running.as_ref().map_or(0, |r| r.frames);
                let slide = here.map(|(at, _, _)| at.slide);
                eprintln!("live {slide:?} at {:?}, operator window drawn {drawn} times", self.started.elapsed());
            }
            self.live_since = here.map(|at| (at, Instant::now()));
            self.advanced = false;
        }
        let Some((_, since)) = self.live_since else { return };
        let Some(seconds) = self.show.live_slide().and_then(|(_, slide)| slide.advance).filter(|s| *s > 0.0) else {
            return;
        };
        let due = since + Duration::from_secs_f32(seconds);
        if Instant::now() >= due {
            if !self.advanced {
                self.advanced = true;
                self.apply(vec![Command::Next]);
            }
        } else if let Some(running) = self.running.as_mut() {
            running.next_repaint = Some(running.next_repaint.map_or(due, |at| at.min(due)));
        }
    }

    /// Plays a media playlist item after item on the media layer, starting with its first.
    fn play_media_playlist(&mut self, id: Uuid) {
        self.check_folders(true);
        let Some(first) = self.playlist_media(id).first().copied() else {
            if let Some(running) = self.running.as_mut() {
                running.home_state.notice = Some("That media playlist has nothing in it to play.".to_owned());
            }
            return;
        };
        self.media_cycle = Some(MediaCycle { playlist: id, at: 0, since: Instant::now(), shown: first });
        if let Some(running) = self.running.as_mut() {
            running.home_state.media_playing = Some(id);
        }
        applog::event("media", &format!("playing media playlist {id}"));
        self.apply(vec![Command::ShowMedia(first)]);
    }

    fn stop_media_playlist(&mut self) {
        if self.media_cycle.take().is_some() {
            applog::event("media", "stopped the media playlist");
        }
        if let Some(running) = self.running.as_mut() {
            running.home_state.media_playing = None;
        }
    }

    /// The media bin places of a media playlist's files, in its order.
    fn playlist_media(&self, id: Uuid) -> Vec<usize> {
        let Some(running) = self.running.as_ref() else { return Vec::new() };
        let Some(playlist) = running.home_state.media_playlists.iter().find(|p| p.id == id) else { return Vec::new() };
        playlist.files.iter().filter_map(|file| self.show.media().iter().position(|item| &item.path == file)).collect()
    }

    /// Moves a playing media playlist on: a picture after 8 seconds, a video when it ends. It
    /// stops if the operator puts other media on, or clears it. Says when to look again.
    fn tick_media_cycle(&mut self) -> Option<Instant> {
        let cycle = self.media_cycle.as_ref()?;
        let (id, at, since, shown) = (cycle.playlist, cycle.at, cycle.since, cycle.shown);
        if self.show.live_media() != Some(shown) {
            self.stop_media_playlist();
            return None;
        }
        let items = self.playlist_media(id);
        let is_video = self.show.media().get(shown).is_some_and(|item| media::is_video(&item.path));
        let done = if is_video {
            // One video on its own loops, so it never ends; with others, it plays to its end.
            items.len() > 1 && self.video.as_ref().is_some_and(|(_, player)| player.ended())
        } else {
            since.elapsed() >= Duration::from_secs_f32(PICTURE_SECONDS)
        };
        if done && items.len() > 1 {
            let next_at = (at + 1) % items.len();
            let next = items[next_at];
            self.media_cycle = Some(MediaCycle { playlist: id, at: next_at, since: Instant::now(), shown: next });
            self.apply(vec![Command::ShowMedia(next)]);
        }
        Some(Instant::now() + Duration::from_millis(if is_video { 200 } else { 500 }))
    }

    /// Keeps the media playlists that follow folders up to date: new files are added to the
    /// media bin and the playlist, gone ones leave the playlist, and a file that changed (a new
    /// export written over the old) is read again, and played again if it is on screen. Looks
    /// every few seconds, or at once when `now`.
    fn check_folders(&mut self, now: bool) {
        if !now && self.folders_checked.elapsed() < Duration::from_secs(5) {
            return;
        }
        self.folders_checked = Instant::now();
        let Some(running) = self.running.as_ref() else { return };
        let followed: Vec<(Uuid, std::path::PathBuf)> = running
            .home_state
            .media_playlists
            .iter()
            .filter_map(|playlist| playlist.folder.clone().map(|folder| (playlist.id, folder)))
            .collect();
        let mut changed_list = false;
        for (id, folder) in followed {
            let files = media::folder_files(&folder);
            for (path, modified) in &files {
                let known = self.folders_seen.get(path).copied();
                let in_bin = self.show.media().iter().position(|item| &item.path == path);
                match (in_bin, known) {
                    (None, _) => {
                        if let Ok(picture) = media::load_picture(path) {
                            let name = path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                            self.show.add_media(MediaItem::new(name, path.clone()));
                            self.pictures.push(media::MediaPicture::of(picture));
                            applog::event("media", &format!("{} appeared in a followed folder", path.display()));
                            changed_list = true;
                        }
                    }
                    (Some(index), Some(seen)) if seen != *modified => {
                        // Written over: read it again; if it is on screen, show the new one.
                        if let Ok(picture) = media::load_picture(path)
                            && let Some(slot) = self.pictures.get_mut(index)
                        {
                            *slot = media::MediaPicture::of(picture);
                            applog::event("media", &format!("{} changed in a followed folder", path.display()));
                            if let (Some(running), Some(item)) = (self.running.as_mut(), self.show.media().get(index)) {
                                running.thumbs.forget(item.id, &mut running.egui_renderer);
                            }
                            if self.show.live_media() == Some(index) {
                                self.video = None;
                                self.refresh_output();
                            }
                        }
                    }
                    _ => {}
                }
                self.folders_seen.insert(path.clone(), *modified);
            }
            if let Some(running) = self.running.as_mut()
                && let Some(playlist) = running.home_state.media_playlists.iter_mut().find(|p| p.id == id)
            {
                let now_files: Vec<std::path::PathBuf> = files.into_iter().map(|(path, _)| path).collect();
                if playlist.files != now_files {
                    playlist.files = now_files;
                    changed_list = true;
                }
            }
        }
        if changed_list {
            self.save_media_list();
            if let Some(running) = self.running.as_ref() {
                running.home_window.request_redraw();
            }
        }
    }

    /// Starts what Settings says to play when the app opens.
    fn start_playing(&mut self) {
        use service::StartWith;
        let Some(start) = self.running.as_ref().map(|r| r.home_state.start_with.clone()) else { return };
        match start {
            StartWith::Nothing => {}
            StartWith::MediaPlaylist { id } => self.play_media_playlist(id),
            StartWith::Announcements { song } => {
                if let Some(running) = self.running.as_mut()
                    && let Some(index) = running.home_state.song_keys.iter().position(|key| *key == song)
                {
                    running.home_state.announcement_request = Some(Some(index));
                }
            }
        }
    }

    /// Whether a video loops now: as it is set, unless a media playlist of several items is
    /// playing, when it plays to its end so the next can follow.
    fn loops_now(&self, item: &MediaItem) -> bool {
        let many = self.media_cycle.as_ref().is_some_and(|cycle| self.playlist_media(cycle.playlist).len() > 1);
        item.looping && !many
    }

    /// How the media on screen fills the screen.
    fn live_media_fit(&self) -> pc_model::ImageFit {
        self.show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .map_or(pc_model::ImageFit::Fill, |m| m.fit)
    }

    /// Plays the video on screen, if the media on screen is one, and stops the one that was.
    fn sync_video(&mut self) {
        let live = self
            .show
            .live_media()
            .and_then(|index| self.show.media().get(index))
            .filter(|item| media::is_video(&item.path))
            .cloned();
        if self.video.as_ref().map(|(id, _)| *id) == live.as_ref().map(|item| item.id) {
            // The same video: it may have been told to loop or not.
            let loops = live.as_ref().is_some_and(|item| self.loops_now(item));
            if let (Some((_, player)), Some(running)) = (self.video.as_mut(), self.running.as_mut())
                && let Some(shown) = running.home_state.video_live.as_mut()
                && shown.looping != loops
            {
                player.set_looping(loops);
                shown.looping = loops;
            }
            return;
        }
        if self.video.take().is_some() {
            applog::event("video", "stopped");
        }
        let Some(running) = self.running.as_mut() else { return };
        running.home_state.video_live = None;
        let Some(item) = live else { return };
        let started = Instant::now();
        let loops = self.loops_now(&item);
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else { return };
        match pc_video::Player::open(&item.path) {
            Ok(mut player) => {
                let engine = if player.uses_ffmpeg() { "FFmpeg" } else { "the system's player" };
                applog::event(
                    "video",
                    &format!("opened {} with {engine} in {:?}", item.path.display(), started.elapsed()),
                );
                player.set_looping(loops);
                // A quiet run (a test) makes no sound: someone may be working at this computer.
                if quiet {
                    player.set_volume(0.0);
                }
                player.play();
                running.home_state.video_live = Some(home::VideoLive {
                    name: item.name.clone(),
                    position: 0.0,
                    duration: None,
                    playing: true,
                    looping: loops,
                    muted: false,
                });
                self.video = Some((item.id, player));
            }
            Err(problem) => {
                applog::event("video", &format!("could not open {}: {problem}", item.path.display()));
                running.home_state.notice = Some(problem);
            }
        }
    }

    /// Plays the video in a box on the slide on screen, if it has one, and stops the one that was.
    /// The video loops, with its sound; it is the first visible picture box naming a video.
    fn sync_slide_video(&mut self) {
        let wanted = self.show.live_slide().and_then(|(_, slide)| {
            slide.objects.iter().filter(|object| !object.hidden).find_map(|object| match &object.kind {
                pc_model::ObjectKind::Image { file, .. } if media::is_video(std::path::Path::new(file)) => {
                    Some(file.clone())
                }
                _ => None,
            })
        });
        if self.slide_video.as_ref().map(|(file, _)| file) == wanted.as_ref() {
            return;
        }
        if self.slide_video.take().is_some() {
            applog::event("video", "stopped the video on the slide");
        }
        let Some(file) = wanted else { return };
        let Some(path) = images::resolve(&file) else { return };
        let quiet = self.quiet();
        match pc_video::Player::open(&path) {
            Ok(mut player) => {
                applog::event("video", &format!("playing {} in a box on the slide", path.display()));
                player.set_looping(true);
                if quiet {
                    player.set_volume(0.0);
                }
                player.play();
                self.slide_video = Some((file, player));
            }
            Err(problem) => {
                if let Some(running) = self.running.as_mut() {
                    running.home_state.notice = Some(problem);
                }
            }
        }
    }

    /// Keeps the video in a box on the slide going; if the system's player can't play it,
    /// FFmpeg takes over. Says when to look again.
    fn tick_slide_video(&mut self) -> Option<Instant> {
        let (file, player) = self.slide_video.as_mut()?;
        let wait = player.tick();
        if player.problem().is_some() && !player.uses_ffmpeg() {
            let file = file.clone();
            let path = images::resolve(&file)?;
            if let Ok(mut fallback) = pc_video::Player::open_with_ffmpeg(&path) {
                applog::event("video", &format!("playing {} in a box with FFmpeg instead", path.display()));
                fallback.set_looping(true);
                if self.quiet() {
                    fallback.set_volume(0.0);
                }
                fallback.play();
                self.slide_video = Some((file, fallback));
                self.refresh_output();
            }
        }
        Some(Instant::now() + wait)
    }

    /// Keeps the video on screen going (a looping one starts again at its end) and the
    /// operator's view of it up to date. Says when to look again.
    fn tick_video(&mut self) -> Option<Instant> {
        let (_, player) = self.video.as_mut()?;
        let wait = player.tick();
        let running = self.running.as_mut()?;
        if let Some(problem) = player.problem() {
            applog::event("video", &problem);
            // The system's player couldn't play it (an AV1 video on a Mac without AV1 hardware,
            // say): FFmpeg plays it instead, from where it was.
            if !player.uses_ffmpeg()
                && let Some((id, _)) = self.video.take()
                && let Some(item) = self.show.media().iter().find(|item| item.id == id).cloned()
            {
                let loops = self.loops_now(&item);
                let quiet = self.quiet();
                let running = self.running.as_mut()?;
                match pc_video::Player::open_with_ffmpeg(&item.path) {
                    Ok(mut fallback) => {
                        applog::event("video", &format!("playing {} with FFmpeg instead", item.path.display()));
                        fallback.set_looping(loops);
                        if quiet {
                            fallback.set_volume(0.0);
                        }
                        fallback.play();
                        self.video = Some((id, fallback));
                        // The screens take their pictures from the new player.
                        self.refresh_output();
                    }
                    Err(problem) => {
                        running.home_state.notice = Some(problem);
                        running.home_state.video_live = None;
                    }
                }
                return Some(Instant::now() + Duration::from_millis(100));
            }
            running.home_state.notice = Some(problem);
            running.home_state.video_live = None;
            self.video = None;
            running.home_window.request_redraw();
            return None;
        }
        if let Some(live) = running.home_state.video_live.as_mut() {
            let (position, duration, playing) = (player.position(), player.duration(), player.playing());
            // The window shows whole seconds, so it is drawn again only when one passes.
            let changed =
                live.position.floor() != position.floor() || live.duration != duration || live.playing != playing;
            *live = home::VideoLive { position, duration, playing, ..live.clone() };
            if changed {
                running.home_window.request_redraw();
            }
        }
        Some(Instant::now() + wait)
    }

    /// Carries out what the operator asked of the video on screen.
    fn handle_video(&mut self) {
        use home::VideoRequest;
        let Some(running) = self.running.as_mut() else { return };
        let Some(request) = running.home_state.video_request.take() else { return };
        let (Some((_, player)), Some(live)) = (self.video.as_mut(), running.home_state.video_live.as_mut()) else {
            return;
        };
        match request {
            VideoRequest::TogglePause if player.playing() => player.pause(),
            VideoRequest::TogglePause => player.play(),
            VideoRequest::Restart => {
                player.seek(0.0);
                player.play();
            }
            VideoRequest::Looping(on) => {
                player.set_looping(on);
                live.looping = on;
            }
            VideoRequest::Muted(on) => {
                player.set_volume(if on { 0.0 } else { 1.0 });
                live.muted = on;
            }
            VideoRequest::Seek(to) => player.seek(to),
        }
        live.playing = player.playing();
        live.position = player.position();
        running.home_window.request_redraw();
    }

    /// Carries out what the Audio tab asked of the player, and tells the tab what is playing.
    fn handle_audio(&mut self) {
        use home::AudioRequest;
        let Some(running) = self.running.as_mut() else { return };
        let state = &mut running.home_state;
        match state.audio_request.take() {
            Some(AudioRequest::Add) => {
                let chosen = rfd::FileDialog::new()
                    .set_title("Add Audio")
                    .add_filter("Audio", &audio::AUDIO_ENDINGS)
                    .pick_files()
                    .unwrap_or_default();
                let tracks = &mut state.audio.shown_mut().tracks;
                for path in chosen {
                    if !tracks.iter().any(|track| track.path == path) {
                        tracks.push(audio::Track::of(path));
                        state.audio_changed = true;
                    }
                }
            }
            Some(AudioRequest::Play(id)) => {
                // A track chosen while another plays crossfades from it, if a crossfade is set.
                let fade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
                if let Some(track) = state.audio.track(id) {
                    applog::event("audio", &format!("play {} (crossfade {fade:?})", track.path.display()));
                }
                if let Some(track) = state.audio.track(id)
                    && let Err(problem) = self.player.play_fading(track, fade)
                {
                    state.notice = Some(problem);
                }
            }
            Some(AudioRequest::TogglePause) => self.player.toggle_pause(),
            Some(AudioRequest::Stop) => self.player.stop(),
            Some(AudioRequest::Skip(seconds)) => self.player.skip(seconds),
            Some(AudioRequest::Volume(volume)) => self.player.set_volume(volume),
            Some(AudioRequest::Remove(id)) => {
                if state.audio_playing.track == Some(id) {
                    self.player.stop();
                }
                state.audio.shown_mut().tracks.retain(|track| track.id != id);
                state.audio_changed = true;
            }
            None => {}
        }
        if std::mem::take(&mut state.audio_changed)
            && let Err(problem) = audio::save(&state.audio)
        {
            state.notice = Some(problem);
        }
        self.tick_audio();
    }

    /// Keeps the audio going by the clock: fades finish, and with Play Through on the next track
    /// starts as one ends (fading in over its end, if a crossfade is set). Tells the Audio tab
    /// what is playing, and says when to look again.
    fn tick_audio(&mut self) -> Option<Instant> {
        let fading = self.player.tick();
        let running = self.running.as_mut()?;
        let state = &mut running.home_state;
        let fade = Duration::from_secs_f32(state.audio.crossfade.clamp(0.0, 10.0));
        let pick = |count: usize| (Uuid::new_v4().as_u128() % count as u128) as usize;
        if let Some(ended) = self.player.ended() {
            let name = state.audio.track(ended).map_or("", |track| track.name.as_str());
            applog::event("audio", &format!("ended {name}"));
            if state.audio.through
                && let Some(next) = state.audio.after(ended, pick)
                && let Err(problem) = self.player.play(next)
            {
                state.notice = Some(problem);
            }
        } else if let playing = self.player.playing()
            && state.audio.through
            && !fade.is_zero()
            && !playing.paused
            && let Some(id) = playing.track
            // A track too short to crossfade from just plays to its end.
            && playing.length.is_some_and(|length| length > fade * 2)
            && self.player.left().is_some_and(|left| left <= fade)
            && let Some(next) = state.audio.after(id, pick)
            && let Err(problem) = self.player.play_fading(next, fade)
        {
            state.notice = Some(problem);
        }
        let playing = self.player.playing();
        let shown = |p: &audio::Playing| (p.track, p.position.as_secs(), p.paused);
        if shown(&playing) != shown(&state.audio_playing) {
            running.home_window.request_redraw();
        }
        state.audio_playing = playing;
        // While a track plays or fades, look again soon.
        ((playing.track.is_some() && !playing.paused) || fading).then(|| Instant::now() + Duration::from_millis(100))
    }

    /// Carries out the actions waiting to be done: a macro's, or a slide's.
    fn run_actions(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        let actions = std::mem::take(&mut running.home_state.actions_to_run);
        let depth = std::mem::take(&mut running.home_state.actions_depth);
        // Two slides whose actions go to each other's songs would send the show back and forth
        // for ever; a chain of slides' actions stops after a few steps.
        if actions.is_empty() || depth >= MAX_ACTION_CHAIN {
            return;
        }
        self.action_depth = Some(depth);
        // Each action is done in its turn, so a later one sees what an earlier one did:
        // Clear All, then Show Prop, leaves the prop on screen.
        for action in actions {
            let Some(running) = self.running.as_mut() else { break };
            match action_commands(&self.show, &running.home_state, &action) {
                Some(commands) => self.apply(commands),
                None => do_state_action(&mut running.home_state, &action, Instant::now()),
            }
        }
        self.action_depth = None;
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    /// With Continuous Playlist on, Next on a song's last slide goes to the first slide of the
    /// next song in the service instead of staying put.
    fn continue_playlist(&self, command: Command) -> Command {
        let Some(running) = self.running.as_ref() else { return command };
        let state = &running.home_state;
        if command != Command::Next || !state.continuous {
            return command;
        }
        // Only at the end of the song's play order, with the last slide's builds all done.
        let Some(live) = self.show.live() else { return command };
        if self.show.upcoming().is_some() || self.show.builds_left() {
            return command;
        }
        let Some(key) = state.song_keys.get(live.presentation) else { return command };
        let items = &state.services.front().items;
        let Some(here) = items.iter().position(|item| matches!(item, service::Item::Song { key: k, .. } if k == key))
        else {
            return command;
        };
        let next = items[here + 1..].iter().find_map(|item| match item {
            service::Item::Song { key, .. } => state.song_keys.iter().position(|have| have == key),
            _ => None,
        });
        match next.and_then(|at| {
            let song = self.show.presentations().get(at)?;
            let first = song.order().into_iter().find(|slide| song.slide(*slide).is_some_and(|(_, s)| s.enabled))?;
            Some(pc_engine::SlideRef { presentation: at, slide: first })
        }) {
            Some(target) => Command::GoTo(target),
            None => command,
        }
    }

    /// Tells the output what is on each layer now, then redraws the operator's preview.
    fn refresh_output(&mut self) {
        self.sync_video();
        self.sync_slide_video();
        let video = self.video.as_ref().map(|(_, player)| player.frames());
        let slide_video = self.slide_video.as_ref().map(|(file, player)| (file.clone(), player.frames()));
        let media_fit = self.live_media_fit();
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
        let about = self.stage_about();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let slide = self.show.live_slide().map(|(_, slide)| Arc::new(slide.clone()));
        // The slide Next would show, in the song's play order, for the stage screens.
        let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide).map(|(_, slide)| Arc::new(slide.clone()))
        });
        let media = self.show.live_media().and_then(|index| self.pictures.get(index)).map(|p| p.full.clone());
        // The audience comes first: tell the output before drawing anything for the operator.
        if let Some(output) = &running.output {
            let frame = Frame {
                media: media.clone(),
                video,
                media_fit,
                slide_video,
                slide: slide.clone(),
                theme: theme.clone(),
                enabled: self.show.output_enabled(),
                // A slide's own transition, if it has one; then its song's; otherwise the one
                // chosen for all.
                transition: self
                    .show
                    .live_slide()
                    .and_then(|(song, slide)| slide.transition.or(song.transition))
                    .unwrap_or(running.home_state.transition),
                media_transition: running.home_state.media_transition,
                next,
                about,
                build_step: self.show.build_step(),
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_stopped = true;
            }
        }
        self.refresh_preview(true);
    }

    /// The song on screen's title and its slide's group, for the stage screens.
    fn stage_about(&self) -> stage::About {
        match self.show.live_slide() {
            Some((song, _)) => {
                let group = self.show.live().and_then(|at| song.slide(at.slide)).map(|(group, _)| group.name.clone());
                stage::About { title: Some(song.title.clone()), group }
            }
            None => stage::About::default(),
        }
    }

    /// Redraws the Program preview: what the audience sees, or what the stage sees. `fade` lets
    /// it fade from the last picture as the screens do; a clock's tick doesn't fade.
    fn refresh_preview(&mut self, fade: bool) {
        let media_fit = self.live_media_fit();
        let announcement = self.announcing.as_ref().map(|a| a.shown.clone());
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
        let about = self.stage_about();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        // The preview is small: it takes the small copy.
        let media = self.show.live_media().and_then(|index| self.pictures.get(index)).map(|p| p.small.clone());
        let state = &running.home_state;
        // The slide as far as its builds have got, with nothing moving, and its linked text filled in.
        let step = self.show.build_step();
        let next_slide = self.show.live().and(self.show.upcoming()).and_then(|at| {
            let song = self.show.presentations().get(at.presentation)?;
            song.slide(at.slide).map(|(_, slide)| slide.clone())
        });
        let copyright = running.overlay_sent.as_ref().and_then(|sent| sent.copyright.clone());
        let values = links::Values {
            timers: &state.timers,
            title: about.title.as_deref(),
            group: about.group.as_deref(),
            next: next_slide.as_ref(),
            stage_message: state.stage_message_live.then_some(state.stage_message.as_str()),
            copyright: copyright.as_deref(),
        };
        let slide = self.show.live_slide().map(|(_, slide)| {
            let built = slide.built(&theme, pc_model::BuildClock::settled(step)).0;
            Arc::new(links::fill(&built, &values).into_owned())
        });
        running.stage_preview_at = Instant::now();
        running.preview_shows_stage = state.preview_stage;
        if state.preview_stage {
            // The stage screens' picture: the time, this slide, the next, a timer, the stage message.
            let next = self.show.live().and(self.show.upcoming()).and_then(|at| {
                let song = self.show.presentations().get(at.presentation)?;
                song.slide(at.slide).map(|(_, slide)| slide.clone())
            });
            let overlay = running.overlay_sent.clone().unwrap_or_default();
            // The layout of the first stage screen, as the people on stage see it.
            let wanted =
                state.screens.iter().find(|screen| screen.kind == screens::Kind::Stage).and_then(|s| s.stage_layout);
            let template = stage::layout_for(&state.stage_layouts, wanted);
            let picture = stage::slide(slide.as_deref(), next.as_ref(), &overlay, &about, &template);
            let layers = Layers {
                media_fit: pc_model::ImageFit::Fill,
                media: None,
                slide: Some(&picture),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
                announcement: None,
            };
            let layout = stage::theme_for(&template);
            let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &layout);
            running.preview = now;
            running.preview_before = before;
            let now = Instant::now();
            running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
            running.stage_preview_at = now;
            return;
        }
        // The operator's preview shows the props and a message over the slide, as the audience
        // sees them.
        let message = state.message_live.and_then(|(id, _)| {
            let message = state.messages.iter().find(|message| message.id == id)?;
            let words = messages::fill_in(&message.text, &state.timers, Instant::now(), timers::seconds_into_day());
            Some(messages::slide(&words, message.look))
        });
        let mut live_props: Vec<Slide> = state
            .props_live
            .iter()
            .filter_map(|(id, _)| state.props.iter().find(|prop| prop.id == *id).map(|prop| prop.slide.clone()))
            .collect();
        // The copyright line as last worked out; a change to it redraws the preview again.
        live_props.extend(
            running.overlay_sent.as_ref().and_then(|sent| sent.copyright.as_deref()).map(props::copyright_slide),
        );
        let over = props::overlay(&live_props, message.as_ref());
        let layers = Layers {
            media_fit,
            media: media.as_ref(),
            slide: slide.as_deref(),
            overlay: over.as_ref().filter(|_| self.show.output_enabled()),
            time: 0.0,
            backdrop: None,
            mask: [0.0; 4],
            announcement: announcement.as_ref().map(|(slide, theme)| (&**slide, &**theme)),
        };
        let (now, before) = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &theme);
        running.preview = now;
        running.preview_before = before;
        let now = Instant::now();
        running.preview_changed = if fade { now } else { now.checked_sub(Duration::from_secs(60)).unwrap_or(now) };
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
        // In the editor, a picture or video dropped on the window goes on the slide, as Media…
        // puts it there; in the theme editor, a picture becomes the theme's background.
        if let Some(running) = self.running.as_mut()
            && running.home_state.editing.is_some()
            && media::is_media(path)
        {
            let state = &mut running.home_state;
            state.drop_hover = false;
            let for_theme = state.theme_edit.is_some();
            match images::import(path) {
                Ok(imported) if for_theme => {
                    if media::is_video(path) {
                        state.notice = Some("A theme's background can be a picture; this is a video.".to_owned());
                    } else if let Some(edit) = state.theme_edit.as_mut() {
                        edit.theme.background_picture = Some(imported.file);
                        edit.changed = true;
                        state.notice = Some("The picture is now the theme's background.".to_owned());
                    }
                }
                Ok(imported) => {
                    applog::event("editor", &format!("dropped {} on the slide", path.display()));
                    state.picture_to_add.push(imported);
                }
                Err(problem) => state.notice = Some(problem),
            }
            running.home_window.request_redraw();
            return;
        }
        // A file already in the bin isn't added twice; it joins the playlist shown, if any.
        let already = self.show.media().iter().any(|item| item.path == path);
        let started = Instant::now();
        let result = if already { Ok(None) } else { media::load_picture(path).map(Some) };
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.home_state.drop_hover = false;
        match result {
            Ok(loaded) => {
                if let Some(picture) = loaded {
                    let name =
                        path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
                    self.show.add_media(MediaItem::new(name, path.to_owned()));
                    self.pictures.push(media::MediaPicture::of(picture));
                    running.home_state.notice = None;
                    applog::event("media", &format!("added {} in {:?}", path.display(), started.elapsed()));
                } else {
                    running.home_state.notice = Some("That file is already in the media bin.".to_owned());
                }
                // Added while a media playlist is shown, it goes into that playlist, so it shows.
                let state = &mut running.home_state;
                if let Some(playlist) =
                    state.media_playlist_shown.and_then(|id| state.media_playlists.iter_mut().find(|p| p.id == id))
                    && !playlist.has(path)
                {
                    playlist.toggle(path);
                }
                self.save_media_list();
            }
            Err(message) => {
                applog::event("media", &format!("could not add {}: {message}", path.display()));
                running.home_state.notice = Some(message);
            }
        }
        if let Some(running) = self.running.as_mut() {
            running.home_window.request_redraw();
        }
    }

    /// Remembers what is in the media bin, how each item is shown, and the media playlists.
    fn save_media_list(&mut self) {
        let entries = self
            .show
            .media()
            .iter()
            .map(|item| media::Entry { id: item.id, path: item.path.clone(), fit: item.fit, looping: item.looping })
            .collect();
        let playlists = self.running.as_ref().map(|r| r.home_state.media_playlists.clone()).unwrap_or_default();
        if let Err(problem) = media::save_list(&media::List { entries, playlists })
            && let Some(running) = self.running.as_mut()
        {
            running.home_state.notice = Some(problem);
        }
    }

    /// Keeps a song as it is now, so the change about to be made to it can be undone.
    fn remember_for_undo(&mut self, index: usize) {
        if let Some(song) = self.show.presentations().get(index).cloned() {
            self.history.remember(index, song);
        }
    }

    /// Undoes the last change made to a song in the Show view, or redoes the last one undone.
    fn undo_show(&mut self, redo: bool) {
        let Some(current) = self.history.peek(redo).and_then(|index| self.show.presentations().get(index).cloned())
        else {
            return;
        };
        let Some((index, song)) = self.history.step(redo, current) else { return };
        let changes = self.show.replace_presentation_following(index, song);
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.home_window.request_redraw();
        }
        if changes.slides {
            self.refresh_output();
        }
    }

    /// Turns a song's looping on or off, from its menu.
    /// Starts a song on the Announcements layer, or stops it.
    fn handle_announcement(&mut self) {
        let Some(asked) = self.running.as_mut().and_then(|r| r.home_state.announcement_request.take()) else {
            return;
        };
        match asked {
            Some(song) => {
                self.announcing = None;
                if let Some(first) = self.next_announcement_slide(song, None) {
                    self.show_announcement(song, first);
                }
            }
            None => self.apply(vec![Command::Clear(pc_engine::Layer::Announcements)]),
        }
    }

    /// The slide that follows `after` on the Announcements layer: the next one that is on, round
    /// to the first after the last. `None` when the song has no slide that is on.
    fn next_announcement_slide(&self, song: usize, after: Option<usize>) -> Option<usize> {
        let song = self.show.presentations().get(song)?;
        let enabled: Vec<usize> =
            song.slides().enumerate().filter(|(_, (_, slide))| slide.enabled).map(|(at, _)| at).collect();
        let start = after.map_or(0, |after| after + 1);
        enabled.iter().copied().find(|at| *at >= start).or_else(|| enabled.first().copied())
    }

    /// Puts one slide of a song on the Announcements layer.
    fn show_announcement(&mut self, song: usize, slide: usize) {
        let Some((_, shown)) = self.show.presentations().get(song).and_then(|s| s.slide(slide)) else {
            self.announcing = None;
            return;
        };
        let theme = self.theme_of(song).clone();
        let shown: output::Announcement = (Arc::new(shown.clone()), theme);
        if let Some(running) = self.running.as_mut() {
            if let Some(output) = &running.output {
                output.send(OutputMsg::Announcement(Some(shown.clone())));
            }
            running.home_state.announcement_live = Some(song);
            running.home_window.request_redraw();
        }
        if self.announcing.as_ref().is_none_or(|was| was.song != song) {
            let title = self.show.presentations().get(song).map_or("", |s| s.title.as_str());
            applog::event("announcements", &format!("playing {title}"));
        }
        self.announcing = Some(Announcing { song, slide, since: Instant::now(), shown });
        self.refresh_preview(false);
    }

    /// Moves the Announcements layer on when its slide has been up long enough. Says when to
    /// look again.
    fn tick_announcements(&mut self) -> Option<Instant> {
        let (song, slide, since) = self.announcing.as_ref().map(|a| (a.song, a.slide, a.since))?;
        let seconds = self
            .show
            .presentations()
            .get(song)
            .and_then(|s| s.slide(slide))
            .and_then(|(_, s)| s.advance)
            .filter(|seconds| *seconds > 0.0)
            .unwrap_or(ANNOUNCEMENT_SECONDS);
        let due = since + Duration::from_secs_f32(seconds);
        if Instant::now() < due {
            return Some(due);
        }
        match self.next_announcement_slide(song, Some(slide)) {
            Some(next) => self.show_announcement(song, next),
            None => self.apply(vec![Command::Clear(pc_engine::Layer::Announcements)]),
        }
        // Looked at again straight away, to work out when the new slide is due.
        self.announcing.as_ref().map(|_| Instant::now() + Duration::from_millis(50))
    }

    /// Gives a song a transition of its own, or takes it away.
    fn handle_song_transition(&mut self) {
        let Some((index, transition)) = self.running.as_mut().and_then(|r| r.home_state.song_transition_request.take())
        else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        if song.transition == transition {
            return;
        }
        // Dragging the duration or turning the direction of the song's own transition is one
        // step to undo, not one for each frame of the drag.
        let same_kind = matches!((song.transition, transition), (Some(was), Some(now)) if was.kind == now.kind);
        if !(same_kind && self.history.peek(false) == Some(index)) {
            self.history.remember(index, song.clone());
        }
        let changed = pc_model::Presentation { transition, ..(*song).clone() };
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        // Written once the changes pause, not on every frame of a drag.
        self.unsaved_since = Some(Instant::now());
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
    }

    fn handle_song_loop(&mut self) {
        let Some(index) = self.running.as_mut().and_then(|r| r.home_state.song_loop_request.take()) else { return };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = pc_model::Presentation { loops: !song.loops, ..(*song).clone() };
        self.history.remember(index, song);
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if self.show.live().is_some_and(|live| live.presentation == index) {
            self.refresh_output();
        }
    }

    /// Keeps a song's arrangements as changed in the Show view, and the one it is played in.
    fn handle_arrangement(&mut self) {
        let Some((index, arrangements, chosen)) =
            self.running.as_mut().and_then(|r| r.home_state.arrangement_request.take())
        else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = pc_model::Presentation { arrangements, arrangement: chosen, ..(*song).clone() };
        if changed == *song {
            return;
        }
        // Typing an arrangement's name is one step to undo, not one for each letter.
        let only_named = |a: &pc_model::Presentation, b: &pc_model::Presentation| {
            a.arrangement == b.arrangement
                && a.arrangements.len() == b.arrangements.len()
                && a.arrangements.iter().zip(&b.arrangements).all(|(x, y)| x.id == y.id && x.groups == y.groups)
        };
        let renaming = only_named(&changed, &song) && self.history.peek(false) == Some(index);
        if !renaming {
            self.history.remember(index, song);
        }
        let changes = self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_ref() {
            running.home_window.request_redraw();
        }
        // The stage screens show the next slide in the new order.
        if changes.slides {
            self.refresh_output();
        }
    }

    /// Gives a song a theme of its own, or puts it back on the songs theme.
    fn handle_song_theme(&mut self) {
        let Some((index, theme)) = self.running.as_mut().and_then(|r| r.home_state.song_theme_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let id = theme.and_then(|at| self.themes.get(at)).map(|theme| theme.id);
        if song.theme == id {
            return;
        }
        let changed = pc_model::Presentation { theme: id, ..(*song).clone() };
        self.remember_for_undo(index);
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.save_unsaved();
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            running.home_window.request_redraw();
        }
        if self.show.live().is_some_and(|live| live.presentation == index) {
            self.refresh_output();
        }
    }

    /// Gives a song the copyright details typed in its Info pop-over.
    fn handle_copyright(&mut self) {
        let Some((index, details)) = self.running.as_mut().and_then(|r| r.home_state.copyright_request.take()) else {
            return;
        };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let changed = pc_model::Presentation { copyright: details, ..(*song).clone() };
        self.history.remember(index, song.clone());
        self.show.replace_presentation(index, Arc::new(changed));
        if !self.unsaved.contains(&index) {
            self.unsaved.push(index);
        }
        self.unsaved_since = Some(Instant::now());
    }

    /// Rebuilds a song from the words finished in the Reflow editor.
    fn handle_reflow(&mut self) {
        let Some((index, text)) = self.running.as_mut().and_then(|r| r.home_state.reflow_done.take()) else { return };
        let Some(song) = self.show.presentations().get(index).cloned() else { return };
        let rebuilt = song.reflowed(&text);
        if rebuilt == *song {
            return;
        }
        self.history.remember(index, song.clone());
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
    /// Asks for a picture to put on the slide being edited (or behind the theme being edited),
    /// and copies it into the Images folder.
    fn handle_picture_request(&mut self) {
        let Some(running) = self.running.as_mut() else { return };
        if std::mem::take(&mut running.home_state.theme_picture_request) {
            let chosen = rfd::FileDialog::new()
                .set_title("Choose a Picture for the Theme")
                .add_filter("Pictures", &media::PICTURES)
                .pick_file();
            let state = &mut running.home_state;
            match chosen.map(|path| images::import(&path)) {
                Some(Ok(picture)) => {
                    if let Some(edit) = state.theme_edit.as_mut() {
                        edit.theme.background_picture = Some(picture.file);
                        edit.changed = true;
                    }
                }
                Some(Err(problem)) => state.notice = Some(problem),
                None => {}
            }
            running.home_window.request_redraw();
            return;
        }
        if !std::mem::take(&mut running.home_state.picture_request) {
            return;
        }
        let chosen = rfd::FileDialog::new()
            .set_title("Choose a Picture or Video")
            .add_filter("Pictures and Videos", &media::PICTURE_ENDINGS)
            .pick_file();
        let state = &mut running.home_state;
        match chosen.map(|path| images::import(&path)) {
            Some(Ok(picture)) => state.picture_to_add.push(picture),
            Some(Err(problem)) => state.notice = Some(problem),
            None => {}
        }
        running.home_window.request_redraw();
    }

    fn handle_media_request(&mut self) {
        use home::MediaRequest;
        if self.running.as_mut().is_some_and(|r| std::mem::take(&mut r.home_state.media_playlists_changed)) {
            self.save_media_list();
        }
        let Some(request) = self.running.as_mut().and_then(|r| r.home_state.media_request.take()) else { return };
        match request {
            MediaRequest::PlayPlaylist(Some(id)) => self.play_media_playlist(id),
            MediaRequest::PlayPlaylist(None) => self.stop_media_playlist(),
            MediaRequest::WatchFolder(id, follow) => {
                let folder = if follow {
                    let Some(folder) = rfd::FileDialog::new().set_title("Choose a Folder to Follow").pick_folder()
                    else {
                        return;
                    };
                    Some(folder)
                } else {
                    None
                };
                if let Some(running) = self.running.as_mut()
                    && let Some(playlist) = running.home_state.media_playlists.iter_mut().find(|p| p.id == id)
                {
                    applog::event("media", &format!("playlist {} follows {:?}", playlist.name, folder));
                    playlist.folder = folder;
                }
                self.folders_seen.clear();
                self.check_folders(true);
                self.save_media_list();
            }
            MediaRequest::Choose => {
                // The system's own window for choosing files, as any Mac app has.
                let chosen = rfd::FileDialog::new()
                    .set_title("Add Pictures and Videos")
                    .add_filter("Pictures and Videos", &media::PICTURE_ENDINGS)
                    .pick_files()
                    .unwrap_or_default();
                for path in chosen {
                    self.add_dropped_file(&path);
                }
            }
            MediaRequest::AddAsSlide(index) => {
                let Some(item) = self.show.media().get(index).cloned() else { return };
                let song = self.show.selected();
                let Some(count) = self.show.presentations().get(song).map(|song| song.slide_count()) else { return };
                if count == 0 || self.bible_slots.contains(&song) {
                    return;
                }
                // After the slide picked out in the song shown, or at its end.
                let after = self.show.cursor().filter(|at| at.presentation == song).map_or(count - 1, |at| at.slide);
                let name = std::path::Path::new(&item.name)
                    .file_stem()
                    .map_or(item.name.clone(), |stem| stem.to_string_lossy().into_owned());
                let slide =
                    Slide { label: Some(name), ..home::slide_with_media(&Slide::new(Vec::new()), Some(item.id)) };
                if let Some(running) = self.running.as_mut() {
                    running.home_state.slide_request = Some((
                        pc_engine::SlideRef { presentation: song, slide: after },
                        home::SlideRequest::PasteAfter(slide),
                    ));
                    running.home_window.request_redraw();
                }
                applog::event("media", &format!("added {} to a song as a slide", item.path.display()));
            }
            MediaRequest::Look(index, fit, looping) => {
                let changes = self.show.set_media_look(index, fit, looping);
                if changes.media {
                    self.refresh_output();
                }
                self.save_media_list();
                if let Some(running) = self.running.as_mut() {
                    running.home_window.request_redraw();
                }
            }
            MediaRequest::Delete(mut items) => {
                // From the last to the first, so each place still names the same item.
                items.sort_unstable();
                items.dedup();
                let mut media_changed = false;
                for index in items.into_iter().rev() {
                    if let Some(item) = self.show.media().get(index) {
                        applog::event("media", &format!("deleted {} from the bin", item.path.display()));
                    }
                    media_changed |= self.show.remove_media(index).media;
                    if index < self.pictures.len() {
                        self.pictures.remove(index);
                    }
                }
                if media_changed {
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
        // A change made in the Show view can be undone there. The Edit view keeps its own steps,
        // and the Show view's steps for the song are dropped, so they can't undo its edits.
        if self.running.as_ref().is_some_and(|r| r.home_state.editing.is_none()) {
            self.remember_for_undo(at.presentation);
        } else {
            self.history.forget(at.presentation);
        }
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
            self.history.remember(index, song.clone());
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
            self.history.forget(index);
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
            self.history.forget(index);
            self.show.replace_presentation(index, Arc::new(longer));
            if !self.unsaved.contains(&index) {
                self.unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            let at = pc_engine::SlideRef { presentation: index, slide };
            running.home_state.editing = EditDraft::of(&self.show, at);
            running.home_window.request_redraw();
        }
        let Some(asked) = running.home_state.new_song_request.take() else {
            return;
        };
        let title = asked.title.clone();
        let id = Uuid::new_v4();
        // The words given, made into slides; with none, one empty slide in a first verse.
        let parsed = pc_model::parse_song_text(&title, &asked.words);
        let groups = if parsed.slide_count() > 0 {
            parsed.groups
        } else {
            vec![pc_model::Group {
                id: Uuid::new_v4(),
                name: "Verse 1".to_owned(),
                color: pc_model::group_color("Verse 1"),
                slides: vec![Slide::new(vec![pc_model::Object::text(pc_model::Role::primary(), "")])],
            }]
        };
        let theme = asked.theme.and_then(|at| self.themes.get(at)).map(|theme| theme.id);
        let song = pc_model::Presentation {
            id,
            title: title.clone(),
            groups,
            copyright: pc_model::Copyright::default(),
            theme,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
        };
        self.add_new_song(song);
    }

    /// Makes the passage on the Bible page a song of its own, drawn with the Bible's theme, and
    /// shows it.
    fn handle_bible_song(&mut self) {
        let Some((showing, title)) = self.running.as_mut().and_then(|r| r.home_state.bible_song_request.take()) else {
            return;
        };
        let Some(passage) = self.bible_slots.get(showing).and_then(|slot| self.show.presentations().get(*slot)) else {
            return;
        };
        if passage.slide_count() == 0 {
            return;
        }
        // New ids throughout, so the song and the Bible page never share a slide.
        let mut song = (**passage).clone();
        song.id = Uuid::new_v4();
        song.title = title;
        song.theme = self.themes.get(self.bible_theme).map(|theme| theme.id);
        for group in &mut song.groups {
            group.id = Uuid::new_v4();
            for slide in &mut group.slides {
                slide.id = Uuid::new_v4();
                for object in &mut slide.objects {
                    object.id = Uuid::new_v4();
                }
            }
        }
        self.add_new_song(song);
    }

    /// Puts a song made in the app into the library, gives it a file in the Songs folder, and
    /// shows its slides.
    fn add_new_song(&mut self, song: pc_model::Presentation) {
        let (id, title) = (song.id, song.title.clone());
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
            let key = match &path {
                Some(path) => path.with_extension("").to_string_lossy().into_owned(),
                None => format!("title:{title}"),
            };
            // A song made while a library is shown goes into that library.
            if let Some(library) = state.library_shown.and_then(|id| state.libraries.iter_mut().find(|l| l.id == id)) {
                library.toggle(&key);
                state.libraries_changed = true;
            }
            state.song_keys.push(key);
            // Back to the slides of the new song.
            state.editing = None;
            state.bible_open = false;
            state.rename_started = true;
            running.home_window.request_redraw();
        }
    }

    /// The theme a presentation's slides are drawn with: the Bible theme for a Bible passage,
    /// the song's own theme if it names one, otherwise the songs theme.
    fn theme_of(&self, presentation: usize) -> &Arc<Theme> {
        if self.bible_slots.contains(&presentation) {
            return &self.theme_for_bible;
        }
        let own = self.show.presentations().get(presentation).and_then(|song| song.theme);
        own.and_then(|id| self.theme_arcs.iter().find(|theme| theme.id == id)).unwrap_or(&self.theme)
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
        self.theme_arcs = self.themes.iter().cloned().map(Arc::new).collect();
        self.bible_layout = self.bible_layout.min(self.theme_for_bible.templates.len().saturating_sub(1));
        let (layouts, pinned) = (self.theme_layouts(), self.bible_template());
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            let state = &mut running.home_state;
            state.theme_names = self.themes.iter().map(|theme| theme.name.clone()).collect();
            state.theme_ids = self.themes.iter().map(|theme| theme.id).collect();
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
        let Running { output, ndi_sent, identify_sent, home_state, looks_sent, .. } = running;
        let Some(output) = output.as_ref() else { return };

        // Looks: the audience screens that show the show their own way.
        let looks: Vec<(Uuid, screens::Look)> = list
            .iter()
            .filter(|screen| screen.kind == screens::Kind::Audience && !screen.look.is_default())
            .map(|screen| (screen.id, screen.look))
            .collect();
        if looks != *looks_sent {
            output.send(OutputMsg::Looks(looks.clone()));
            output.send(OutputMsg::Redraw);
            *looks_sent = looks;
        }

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

        // Stage screens: which they are, the layout each is drawn with, and whether they are on.
        let stage = (
            list.iter()
                .filter(|screen| screen.kind == screens::Kind::Stage)
                .map(|screen| (screen.id, stage::layout_for(&home_state.stage_layouts, screen.stage_layout)))
                .collect::<Vec<_>>(),
            home_state.stage_on,
        );
        if running.stage_sent.as_ref() != Some(&stage) {
            let screens = stage.0.iter().map(|(id, layout)| (*id, Arc::new(stage::theme_for(layout)))).collect();
            output.send(OutputMsg::Stage { screens, on: stage.1 });
            running.stage_sent = Some(stage);
        }

        // Identify Screens: every screen shows its own name, in its own colour, while it is on.
        let names: Option<Vec<(Uuid, String, pc_model::Rgba)>> = home_state.identify.then(|| {
            list.iter()
                .enumerate()
                .map(|(place, screen)| (screen.id, screen.name.clone(), screens::identify_color(place)))
                .collect()
        });
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
        if home_state.screen_status != status_before {
            for panel in running.panels.iter().filter(|panel| panel.kind == Panel::Screens) {
                panel.window.request_redraw();
            }
        }
        // Starting NDI takes a moment on the output thread; look again shortly.
        if starting || ndi_moved {
            running.next_repaint = Some(Instant::now() + Duration::from_millis(400));
        }
    }

    /// Redraws the Edit view's picture of the slide when the slide being edited changes.
    fn update_canvas(&mut self) {
        // The theme of the song being edited, worked out before the window's state is borrowed.
        let editing_song = self.running.as_ref().and_then(|r| r.home_state.editing.as_ref()).map(|d| d.at.presentation);
        let song_theme = editing_song.map(|song| self.theme_of(song).clone());
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let editing = running.home_state.editing.as_ref().map(|draft| (draft.at, draft.slide.clone()));
        if editing == running.canvas_for {
            return;
        }
        if let Some((_, slide)) = &editing {
            // A theme's layout is drawn with that theme; a slide with its presentation's.
            let theme = match &running.home_state.theme_edit {
                Some(edit) => self.themes.get(edit.index).unwrap_or(&self.theme),
                None => song_theme.as_deref().unwrap_or(&self.theme),
            };
            running.canvas = running.thumbs.canvas(&running.gpu, &mut running.egui_renderer, slide, theme);
        }
        running.canvas_for = editing;
        running.home_window.request_redraw();
    }

    /// Draws one frame of the home window and returns what the operator asked for.
    fn draw_home(&mut self) -> Vec<Command> {
        // The song in front (being edited, or chosen) may have a theme of its own.
        let in_front = self
            .running
            .as_ref()
            .and_then(|r| r.home_state.editing.as_ref())
            .map_or(self.show.selected(), |draft| draft.at.presentation);
        let front_theme = self.theme_of(in_front).clone();
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
        // The Program preview is what the screens show: the live preview the output thread draws,
        // once it has drawn one. (The stage picture, and the start, use the window's own.)
        let live = running.output.as_ref().map(OutputHandle::live_preview).filter(|(_, drawn)| *drawn > 0);
        let preview = match live.and_then(|(front, _)| running.live_preview_ids.get(front)) {
            Some(texture) if !running.home_state.preview_stage => {
                home::Preview { now: Some(*texture), before: None, amount: 1.0 }
            }
            _ => home::Preview { now: running.preview, before: running.preview_before, amount },
        };
        let (show, pictures) = (&self.show, &self.pictures);
        let (songs_theme, bible_theme): (&Theme, &Theme) = (&front_theme, &self.theme_for_bible);
        running.home_state.song_templates = song_templates(songs_theme);
        // Every song by its key and title, for choosing one in Settings.
        let removed = &running.home_state.removed_songs;
        running.home_state.song_choices = show
            .presentations()
            .iter()
            .enumerate()
            .filter(|(index, _)| !self.bible_slots.contains(index) && !removed.contains(index))
            .filter_map(|(index, song)| Some((running.home_state.song_keys.get(index)?.clone(), song.title.clone())))
            .collect();
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
            // A slide that shows a media item has it behind its words in its thumbnail.
            let media_for = |slide: &Slide| {
                let id = home::media_of(slide)?;
                let at = show.media().iter().position(|item| item.id == id)?;
                Some((&pictures.get(at)?.small, show.media()[at].fit))
            };
            let mut thumb = |which: ThumbOf| match which {
                ThumbOf::Slide(slide) => thumbs.get(gpu, egui_renderer, slide, media_for(slide), songs_theme),
                ThumbOf::BibleSlide(slide) => thumbs.get(gpu, egui_renderer, slide, None, bible_theme),
                ThumbOf::Layout(index, slide) => {
                    // A layout is made anew each frame; a fixed id lets its thumbnail be kept
                    // until the theme changes, when every thumbnail is dropped.
                    let mut kept = slide.clone();
                    kept.id = Uuid::from_u128(0x7068_656d_655f_6c61_796f_7574_0000_0000 + index as u128);
                    thumbs.get(gpu, egui_renderer, &kept, None, theme)
                }
                ThumbOf::Media(index) => {
                    let (item, picture) = (show.media().get(index)?, &pictures.get(index)?.small);
                    thumbs.get_media(gpu, egui_renderer, item.id, picture, songs_theme).map(|texture| (texture, false))
                }
            };
            commands.extend(home_ui(ui, show, home_state, theme, &mut thumb, preview, canvas));
        });
        egui_state.handle_platform_output(home_window, std::mem::take(&mut full.platform_output));

        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        // An animation is drawn at most a frame a screen refresh apart. A window macOS isn't
        // showing does not wait for the screen, and would otherwise be drawn thousands of times
        // a second while the preview fades.
        let delay = delay.max(FRAME);
        // Testing aid: say what keeps asking the window to be drawn again.
        if std::env::var_os("PRESENTERCRAFT_TRACE_REPAINT").is_some() && running.frames % 100 == 0 {
            eprintln!("frame {}: delay {delay:?}, causes {:?}", running.frames, egui_ctx.repaint_causes());
        }
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
            // A smoke run stays up this many seconds before its picture is taken and it quits.
            static STARTED: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            let started = *STARTED.get_or_init(Instant::now);
            let wait = std::env::var("PRESENTERCRAFT_CAPTURE_AFTER").ok().and_then(|s| s.parse::<f32>().ok());
            let waited = wait.is_none_or(|seconds| started.elapsed().as_secs_f32() >= seconds);
            if running.frames == GO_LIVE_AT {
                commands.push(Command::GoTo(pc_engine::SlideRef { presentation: 0, slide: 2 }));
                commands.push(Command::ShowMedia(0));
            }
            // Testing aid: open the Settings window.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_PREFERENCES").is_some() {
                running.home_state.preferences_open = true;
            }
            // Testing aid: open Settings.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_SETTINGS").is_some() {
                running.home_state.settings_open = true;
            }
            // Testing aid: arrange the first song Verse, Chorus, Verse, Chorus, and show the
            // second chorus.
            if std::env::var_os("PRESENTERCRAFT_ARRANGE").is_some() {
                if running.frames == GO_LIVE_AT + 1
                    && let Some(song) = self.show.presentations().first()
                    && song.groups.len() >= 2
                {
                    let (verse, chorus) = (song.groups[0].id, song.groups[1].id);
                    let mut arrangement = song.new_arrangement();
                    arrangement.name = "Sunday".to_owned();
                    arrangement.groups = vec![verse, chorus, verse, chorus];
                    let chosen = Some(arrangement.id);
                    running.home_state.arrangement_request = Some((0, vec![arrangement], chosen));
                }
                if running.frames == GO_LIVE_AT + 4
                    && let Some(song) = self.show.presentations().first()
                {
                    let chorus = song.groups.get(1).map(|group| group.id);
                    let in_chorus =
                        |slide: &usize| song.slide(*slide).is_some_and(|(group, _)| Some(group.id) == chorus);
                    if let Some(step) = song.order().iter().rposition(in_chorus) {
                        commands.push(Command::GoToStep { presentation: 0, step });
                    }
                }
            }
            // Testing aid: show a library, by its name.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(name) = std::env::var("PRESENTERCRAFT_LIBRARY")
            {
                let state = &mut running.home_state;
                state.library_shown = state.libraries.iter().find(|library| library.name == name).map(|l| l.id);
            }
            // Testing aid: play a song, by its place, on the Announcements layer.
            if running.frames == GO_LIVE_AT + 2
                && let Some(song) = std::env::var("PRESENTERCRAFT_ANNOUNCE").ok().and_then(|s| s.parse().ok())
            {
                running.home_state.announcement_request = Some(Some(song));
            }
            // Testing aid: pick out songs or media items by their places ("songs:0,2" or
            // "media:0,1"), as Command-clicks on them do; then the Delete menu item (or key) asks.
            if running.frames == 2
                && let Ok(pick) = std::env::var("PRESENTERCRAFT_PICK")
                && let Some((list, places)) = pick.split_once(':')
            {
                let places: Vec<usize> = places.split(',').filter_map(|p| p.trim().parse().ok()).collect();
                let state = &mut running.home_state;
                if list == "media" {
                    state.picked_media = places;
                    state.picking = home::Picking::Media;
                } else {
                    state.picked_songs = places;
                    state.picking = home::Picking::Songs;
                }
            }
            // Testing aid: open an item of the playlist in front, by its place, as a click on it
            // in the left column does.
            if running.frames == GO_LIVE_AT + 3
                && let Some(item) = std::env::var("PRESENTERCRAFT_OPEN_ITEM").ok().and_then(|s| s.parse::<usize>().ok())
            {
                let state = &mut running.home_state;
                state.playlist_open = true;
                let key = match state.services.front().items.get(item) {
                    Some(service::Item::Song { key, .. }) => Some(key.clone()),
                    _ => None,
                };
                if let Some(at) = key.and_then(|key| studio::playlist_song(&self.show, state, &key)) {
                    state.services.chosen = Some(item);
                    state.bible_open = false;
                    commands.push(Command::Select(at));
                }
            }
            // Testing aid: list the playlist in front in the left column.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_PLAYLIST").is_some() {
                running.home_state.playlist_open = true;
            }
            // Testing aid: open the Stage Layouts window.
            if running.frames == GO_LIVE_AT + 2 && std::env::var_os("PRESENTERCRAFT_CAPTURE_STAGE_LAYOUTS").is_some() {
                running.home_state.stage_editor_open = true;
                running.home_state.stage_edit = (0, Some(0));
            }
            // Testing aid: show the stage picture in the Program preview.
            if running.frames == GO_LIVE_AT + 2 && std::env::var("PRESENTERCRAFT_PREVIEW").as_deref() == Ok("stage") {
                running.home_state.preview_stage = true;
            }
            // Testing aid: open a tab under the Program panel, and show the first message.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(tab) = std::env::var("PRESENTERCRAFT_LIVE_TAB")
            {
                let state = &mut running.home_state;
                state.live_tab = match tab.as_str() {
                    "messages" => 1,
                    "props" => 2,
                    "macros" => 3,
                    "stage" => 4,
                    "audio" => 5,
                    _ => 0,
                };
                if let Some(first) = state.messages.first() {
                    state.message_selected = Some(first.id);
                    state.message_live = Some((first.id, Instant::now()));
                }
                if let Some(first) = state.props.first() {
                    state.props_live.push((first.id, Instant::now()));
                }
                if let Some(first) = state.timers.first_mut() {
                    state.timer_open = Some(first.id);
                    first.start(Instant::now() - Duration::from_secs(83));
                }
            }
            // Testing aid: open the library search with these words typed.
            if running.frames == GO_LIVE_AT + 2
                && let Ok(words) = std::env::var("PRESENTERCRAFT_SEARCH")
            {
                running.home_state.library_search = Some((words, 0));
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
            // Testing aid: save the passage on the Bible page as a presentation, as Save As… ›
            // Save as Presentation does.
            if running.frames == GO_LIVE_AT + 6 && std::env::var_os("PRESENTERCRAFT_BIBLE_SAVE_SONG").is_some() {
                let state = &mut running.home_state;
                state.bible_song_request = Some((state.bible.showing, "John 3:16-18 (KJV, SYN)".to_owned()));
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
                // Testing aid: put a picture from a file on the slide, as Picture… does.
                if let Some(path) = std::env::var_os("PRESENTERCRAFT_CAPTURE_PICTURE") {
                    match images::import(std::path::Path::new(&path)) {
                        Ok(picture) => running.home_state.picture_to_add.push(picture),
                        Err(problem) => eprintln!("{problem}"),
                    }
                }
                // Testing aid: give the selected object builds, to see the Build tab filled in.
                if std::env::var_os("PRESENTERCRAFT_CAPTURE_BUILD").is_some()
                    && let Some(object) = running.home_state.editing.as_mut().and_then(|d| d.selected_object_mut())
                {
                    let fly = pc_model::Build { on_click: true, ..pc_model::Build::of(pc_model::BuildEffect::Fly) };
                    object.build_in = Some(fly);
                    object.build_out = Some(pc_model::Build::of(pc_model::BuildEffect::Dissolve));
                }
                // Testing aids: show the grid, and zoom in.
                running.home_state.edit_grid = std::env::var_os("PRESENTERCRAFT_EDIT_GRID").is_some();
                if let Some(zoom) = std::env::var("PRESENTERCRAFT_EDIT_ZOOM").ok().and_then(|z| z.parse().ok()) {
                    running.home_state.edit_zoom = zoom;
                }
                // Testing aid: which tab of the inspector to show.
                running.home_state.inspector.tab = match std::env::var("PRESENTERCRAFT_INSPECTOR_TAB").as_deref() {
                    Ok("shape") => inspector::InspectorTab::Shape,
                    Ok("build") => inspector::InspectorTab::Build,
                    _ => inspector::InspectorTab::Text,
                };
            }
            if running.frames >= CAPTURE_AT && waited && !self.capture_done {
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

    /// Opens the Screens and Settings windows when they are asked for, and closes them when
    /// they are not.
    fn sync_panels(&mut self, el: &ActiveEventLoop) {
        let quiet = self.quiet();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let state = &running.home_state;
        running.panels.retain(|panel| panel.kind.wanted(state));
        for kind in Panel::ALL {
            if !kind.wanted(&running.home_state) || running.panels.iter().any(|panel| panel.kind == kind) {
                continue;
            }
            match open_panel_window(el, running, quiet, kind) {
                Ok(panel) => {
                    panel.window.request_redraw();
                    running.panels.push(panel);
                }
                Err(problem) => {
                    kind.close(&mut running.home_state);
                    running.home_state.notice = Some(problem);
                }
            }
        }
    }

    /// Draws one frame of a panel window and returns what was asked for in it.
    fn draw_panel(&mut self, id: WindowId) -> Vec<Command> {
        let output_on = self.show.output_enabled();
        let Some(running) = self.running.as_mut() else {
            return Vec::new();
        };
        let Running { panels, gpu, home_state, .. } = running;
        let Some(panel) = panels.iter_mut().find(|panel| panel.window.id() == id) else {
            return Vec::new();
        };
        let raw_input = panel.state.take_egui_input(&panel.window);
        let mut commands = Vec::new();
        let kind = panel.kind;
        let mut full = panel.ctx.run_ui(raw_input, |ui| match kind {
            Panel::Screens => commands.extend(home::screens_ui(ui, home_state, output_on)),
            Panel::Settings => home::settings_ui(ui, home_state),
            Panel::StageLayouts => stage_editor::ui(ui, home_state),
        });
        panel.state.handle_platform_output(&panel.window, std::mem::take(&mut full.platform_output));
        let delay = full.viewport_output.get(&egui::ViewportId::ROOT).map_or(Duration::MAX, |v| v.repaint_delay);
        panel.next_repaint = Instant::now().checked_add(delay.max(FRAME));
        let target = UiTarget {
            window: &panel.window,
            surface: &panel.surface,
            config: &panel.config,
            renderer: &mut panel.renderer,
            ctx: &panel.ctx,
            label: "panel window",
        };
        let (jobs, screen) = paint_ui(gpu, target, full);
        panel.frames += 1;
        // Testing aid: a capture run with a panel asked for also saves that window, every
        // frame once it has settled, so the last one saved is the latest.
        let ending = match kind {
            Panel::Screens => "screens.png",
            Panel::Settings => "settings.png",
            Panel::StageLayouts => "stage.png",
        };
        if let Some(path) = &self.options.capture_home
            && panel.frames >= 2
            && let Err(e) =
                capture(gpu, &panel.renderer, &jobs, &screen, panel.config.format, &path.with_extension(ending))
        {
            eprintln!("Could not save the {} window: {e}", kind.title());
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
        self.remember_for_undo(at.presentation);
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

    /// Carries out a change asked for from a song's right-click menu in the library, and
    /// deletes songs Delete was pressed on, one by one, as a song's own Delete does.
    fn handle_song_request(&mut self) {
        let many = self.running.as_mut().map(|r| std::mem::take(&mut r.home_state.songs_to_delete)).unwrap_or_default();
        for index in many {
            if let Some(running) = self.running.as_mut() {
                running.home_state.song_request = Some((index, home::SongRequest::Delete));
            }
            self.handle_one_song_request();
        }
        self.handle_one_song_request();
    }

    fn handle_one_song_request(&mut self) {
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
                    // A deleted song can't be brought back by Undo, which would save it again.
                    self.history.forget(index);
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
        // Linked text showing the time or a timer keeps the preview ticking.
        let links_tick = self.show.live_slide().is_some_and(|(_, slide)| links::ticks(slide));
        // The copyright line of the song on screen, on the slides Settings says.
        let copyright_line = self.running.as_ref().and_then(|running| {
            let state = &running.home_state;
            let at = self.show.live()?;
            if self.bible_slots.contains(&at.presentation) {
                return None;
            }
            let song = self.show.presentations().get(at.presentation)?;
            (!song.copyright.is_empty() && state.copyright_show.shows_on(at.slide, song.slide_count()))
                .then(|| song.copyright.line(&song.title, state.ccli_licence.trim()))
        });
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
        if std::mem::take(&mut state.props_changed)
            && let Err(problem) = props::save(&state.props)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.macros_changed)
            && let Err(problem) = macros::save(&state.macros)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.stage_layouts_changed)
            && let Err(problem) = stage::save_layouts(&state.stage_layouts)
        {
            state.notice = Some(problem);
        }
        if std::mem::take(&mut state.libraries_changed)
            && let Err(problem) = libraries::save(&state.libraries)
        {
            state.notice = Some(problem);
        }
        // Props that clear themselves, once their time is up.
        let props = state.props.clone();
        state.props_live.retain(|(id, since)| {
            let after = props.iter().find(|prop| prop.id == *id).and_then(|prop| prop.auto_clear);
            props.iter().any(|prop| prop.id == *id)
                && after.is_none_or(|after| since.elapsed().as_secs() < u64::from(after))
        });
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
            .map(|message| (message.text.clone(), message.look));
        let overlay = output::Overlay {
            message,
            stage_message: state.stage_message_live.then(|| state.stage_message.clone()),
            timers: state.timers.clone(),
            props: state
                .props_live
                .iter()
                .filter_map(|(id, _)| state.props.iter().find(|prop| prop.id == *id).map(|prop| prop.slide.clone()))
                .collect(),
            copyright: copyright_line,
            test_pattern: state.test_pattern,
        };
        // While a timer runs, or a message counts down to leaving, the window shows it each second.
        let ticking = state.timers.iter().any(Timer::running)
            || state.message_live.is_some()
            || state.props_live.iter().any(|(id, _)| state.props.iter().any(|p| p.id == *id && p.auto_clear.is_some()));
        if ticking {
            let soon = Instant::now() + Duration::from_millis(500);
            running.next_repaint = Some(running.next_repaint.map_or(soon, |at| at.min(soon)));
        }
        // The stage picture in the Program preview shows the time, so it is redrawn each second;
        // switching between the audience and the stage redraws it straight away.
        let switched = running.preview_shows_stage != state.preview_stage;
        let ticking = state.preview_stage || links_tick;
        let stage_tick = ticking && running.stage_preview_at.elapsed() >= Duration::from_secs(1);
        if ticking {
            let soon = Instant::now() + Duration::from_millis(500);
            running.next_repaint = Some(running.next_repaint.map_or(soon, |at| at.min(soon)));
        }
        if running.overlay_sent.as_ref() == Some(&overlay) {
            if switched || stage_tick {
                self.refresh_preview(switched);
            }
            return;
        }
        let message_changed = running.overlay_sent.as_ref().is_none_or(|sent| {
            sent.message != overlay.message || sent.props != overlay.props || sent.copyright != overlay.copyright
        });
        if let Some(output) = &running.output {
            output.send(OutputMsg::Overlay(overlay.clone()));
        }
        running.overlay_sent = Some(overlay);
        if message_changed {
            self.refresh_output();
        } else if switched || stage_tick || running.preview_shows_stage {
            // The stage picture shows timers and the stage message, so any change redraws it.
            self.refresh_preview(switched);
        }
    }

    /// Carries out a choice from the menu bar.
    fn menu_action(&mut self, el: &ActiveEventLoop, action: menubar::Action) {
        use menubar::Action as A;
        if action == A::Quit {
            return self.quit(el);
        }
        // In the Show view, with nothing being typed, Undo and Redo step through the changes
        // made to songs there.
        let in_show = self.running.as_ref().is_some_and(|r| {
            let state = &r.home_state;
            state.editing.is_none()
                && state.theme_edit.is_none()
                && !state.bible_open
                && !r.egui_ctx.text_edit_focused()
        });
        if in_show && matches!(action, A::Undo | A::Redo) {
            return self.undo_show(action == A::Redo);
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
            A::InsertText => state.edit_request = Some(home::EditRequest::AddText),
            A::AlignObjects(edge) => state.edit_request = Some(home::EditRequest::AlignObjects(edge)),
            A::Flip(across) => state.edit_request = Some(home::EditRequest::Flip(across)),
            A::NewWindowedOutput => {
                // A new audience screen in a window of its own.
                let name = screens::next_name(&state.screens, screens::Kind::Audience);
                state.screens.push(screens::Screen::new(name, screens::Output::Window));
                state.screens_changed = true;
            }
            A::InsertShape(at) => {
                if let Some(shape) = pc_model::Shape::ALL.get(usize::from(at)) {
                    state.edit_request = Some(home::EditRequest::AddShape(*shape));
                }
            }
            A::InsertPicture => state.picture_request = state.editing.is_some() && state.theme_edit.is_none(),
            A::Quit => {}
            A::Settings => state.preferences_open = true,
            A::ConfigureScreens => state.settings_open = true,
            A::StageLayouts => state.stage_editor_open = true,
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
                } else if state.alert_open() {
                    state.confirm_delete_template = None;
                    state.confirm_delete_theme = None;
                    state.confirm_delete_songs.clear();
                    state.confirm_delete_media.clear();
                    state.confirm_delete_slide = None;
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
            A::Paste | A::PasteMatchStyle => {
                if let Some(text) = running.egui_state.clipboard_text().filter(|text| !text.is_empty()) {
                    running.egui_state.egui_input_mut().events.push(egui::Event::Paste(text.replace("\r\n", "\n")));
                }
            }
            A::ShowView => state.view_request = Some(0),
            A::EditView => state.view_request = Some(1),
            A::BibleView => state.view_request = Some(2),
            A::ThemesView => state.view_request = Some(3),
            A::Find => state.library_search = Some((String::new(), 0)),
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
            A::PropsTab => state.live_tab = 2,
            A::MacrosTab => state.live_tab = 3,
            A::StageTab => state.live_tab = 4,
            A::AudioTab => state.live_tab = 5,
            A::MediaBin => state.media_hidden = !state.media_hidden,
            A::NextBackground | A::PreviousBackground => {
                // The next (or last) picture in the media bin, round to the start (or end).
                let count = self.show.media().len();
                if count > 0 {
                    let at = match (self.show.live_media(), action == A::NextBackground) {
                        (Some(live), true) => (live + 1) % count,
                        (Some(live), false) => (live + count - 1) % count,
                        (None, true) => 0,
                        (None, false) => count - 1,
                    };
                    commands.push(Command::ShowMedia(at));
                }
            }
            A::ShowLiveSong => {
                // The song on screen, chosen in the Show view.
                if let Some(live) = self.show.live() {
                    commands.push(Command::Select(live.presentation));
                    state.editing = None;
                    state.bible_open = false;
                }
            }
            A::ClearAudio => commands.push(Command::Clear(pc_engine::Layer::Audio)),
            A::ClearAnnouncements => commands.push(Command::Clear(pc_engine::Layer::Announcements)),
            A::ClearProps => commands.push(Command::Clear(pc_engine::Layer::Props)),
            A::ClearMessages => commands.push(Command::Clear(pc_engine::Layer::Messages)),
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
            A::Continuous => state.continuous = !state.continuous,
            A::TestBars | A::TestGrid | A::TestWhite => {
                let pattern = match action {
                    A::TestBars => props::TestPattern::ColorBars,
                    A::TestGrid => props::TestPattern::Grid,
                    _ => props::TestPattern::White,
                };
                state.test_pattern = if state.test_pattern == Some(pattern) { None } else { Some(pattern) };
            }
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
                A::Continuous => state.continuous,
                A::TestBars => state.test_pattern == Some(props::TestPattern::ColorBars),
                A::TestGrid => state.test_pattern == Some(props::TestPattern::Grid),
                A::TestWhite => state.test_pattern == Some(props::TestPattern::White),
                A::TimersTab => state.live_tab == 0,
                A::MessagesTab => state.live_tab == 1,
                A::PropsTab => state.live_tab == 2,
                A::MacrosTab => state.live_tab == 3,
                A::StageTab => state.live_tab == 4,
                A::AudioTab => state.live_tab == 5,
                A::MediaBin => !state.media_hidden,
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
                // The audience screens start off when Settings says so.
                let start_off = prefs.start_audience_off;
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
                running.home_state.props = props::load();
                running.home_state.macros = macros::load();
                running.home_state.libraries = libraries::load();
                running.home_state.media_playlists = std::mem::take(&mut self.media_playlists);
                running.home_state.stage_layouts = stage::load_layouts();
                running.home_state.audio = audio::load();
                // Macros set to run when PresenterCraft starts.
                let startup: Vec<pc_model::ShowAction> = running
                    .home_state
                    .macros
                    .iter()
                    .filter(|m| m.on_startup)
                    .flat_map(|m| m.actions.iter().cloned())
                    .collect();
                running.home_state.actions_to_run.extend(startup);
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
                if start_off {
                    self.apply(vec![Command::SetOutputEnabled(false)]);
                }
                self.sync_screens(el);
                // The menu bar, for a run someone is using.
                #[cfg(target_os = "macos")]
                if !self.quiet() {
                    match menubar::MenuBar::install() {
                        Ok(menu) => self.menu = Some(menu),
                        Err(problem) => eprintln!("{problem}"),
                    }
                }
                // Testing aid: start with the first slide on screen, and the first media item
                // behind it.
                if std::env::var_os("PRESENTERCRAFT_GO_LIVE").is_some() {
                    self.apply(vec![Command::GoTo(pc_engine::SlideRef { presentation: 0, slide: 0 })]);
                }
                if std::env::var_os("PRESENTERCRAFT_GO_LIVE_MEDIA").is_some() {
                    self.apply(vec![Command::ShowMedia(0)]);
                }
                // Followed folders are read, and what Settings says to play at the start plays.
                self.check_folders(true);
                self.start_playing();
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
        // The output thread drew a new live preview: the window shows it.
        if id == LIVE_PREVIEW_EVENT {
            if let Some(running) = self.running.as_ref()
                && let Some(output) = running.output.as_ref()
            {
                output.live_preview_woken();
                running.home_window.request_redraw();
                // Testing aid: say once a second how many new previews the window was woken for.
                if std::env::var_os("PRESENTERCRAFT_TRACE_PREVIEW").is_some() {
                    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                    static SINCE: std::sync::OnceLock<std::sync::Mutex<Instant>> = std::sync::OnceLock::new();
                    let count = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                    let since = SINCE.get_or_init(|| std::sync::Mutex::new(Instant::now()));
                    if let Ok(mut since) = since.lock()
                        && since.elapsed() >= Duration::from_secs(1)
                    {
                        let (_, drawn) = output.live_preview();
                        eprintln!("preview wakes {count} in {:.1}s, {drawn} drawn", since.elapsed().as_secs_f32());
                        COUNT.store(0, std::sync::atomic::Ordering::Relaxed);
                        *since = Instant::now();
                    }
                }
            }
            return;
        }
        if let Some(action) = menubar::action_of(&id) {
            self.menu_action(el, action);
            self.sync_panels(el);
            for panel in self.running.iter().flat_map(|r| r.panels.iter()) {
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
        if let Some(at) = running.panels.iter().position(|panel| panel.window.id() == id) {
            let panel = &mut running.panels[at];
            match &event {
                WindowEvent::CloseRequested => {
                    panel.kind.close(&mut running.home_state);
                    running.panels.remove(at);
                    running.home_window.request_redraw();
                }
                WindowEvent::Resized(size) => {
                    if size.width > 0 && size.height > 0 {
                        panel.config.width = size.width;
                        panel.config.height = size.height;
                        output::configure(&running.gpu.device, &panel.surface, &panel.config);
                    }
                    panel.window.request_redraw();
                }
                WindowEvent::RedrawRequested => {
                    let commands = self.draw_panel(id);
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
            && running.home_state.new_song_sheet.is_none()
            && !running.home_state.alert_open()
            && !self.held.super_key()
            && !self.held.control_key()
            && let Some(command) = show_key_command(&key.logical_key, &self.show)
        {
            // Escape clears the screen only when there is nothing else for it to close: a
            // pop-up or the Settings window takes it first.
            let state = &running.home_state;
            let something_open = state.settings_open
                || state.bible.options.open
                || state.library_search.is_some()
                || egui::Popup::is_any_open(&running.egui_ctx);
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
                    output::configure(&running.gpu.device, &running.home_surface, &running.home_config);
                }
                running.home_window.request_redraw();
            }
            // Testing aid: stop drawing the operator window after a few seconds, as macOS does
            // when it is minimised, to check that what runs by the clock goes on without it.
            WindowEvent::RedrawRequested
                if std::env::var("PRESENTERCRAFT_STOP_DRAWING_AFTER")
                    .ok()
                    .and_then(|s| s.parse::<f32>().ok())
                    .is_some_and(|after| self.started.elapsed().as_secs_f32() >= after) => {}
            WindowEvent::RedrawRequested => {
                let drawing = Instant::now();
                let commands = self.draw_home();
                self.home_timing.add(drawing.elapsed());
                self.home_timing.report_each_minute("window", || {
                    applog::memory_mb().map_or_else(String::new, |mb| format!("{mb} MB resident"))
                });
                // Every notice shown to the operator goes in the log too.
                let notice = self.running.as_ref().and_then(|r| r.home_state.notice.clone());
                if notice != self.notice_logged {
                    if let Some(notice) = &notice {
                        applog::event("notice", notice);
                    }
                    self.notice_logged = notice;
                }
                self.apply(commands);
                // Testing aid: drop a file on the window, as dragging it from the Finder does.
                if let Ok(path) = std::env::var("PRESENTERCRAFT_DROP")
                    && self.running.as_ref().is_some_and(|r| r.frames == 12)
                {
                    self.add_dropped_file(std::path::Path::new(&path));
                }
                // Testing aid: choose menu bar items by their ids, one every few frames, the way
                // a click on them reaches the app.
                if let Ok(ids) = std::env::var("PRESENTERCRAFT_MENU") {
                    let frame = self.running.as_ref().map_or(0, |r| r.frames);
                    if let Some(id) =
                        frame.checked_sub(6).filter(|n| n % 4 == 0).and_then(|n| ids.split(',').nth((n / 4) as usize))
                    {
                        eprintln!("menu {id}");
                        self.user_event(el, id.to_owned());
                    }
                }
                if let Some((at, slide)) = self.running.as_mut().and_then(|r| r.home_state.pending_edit.take()) {
                    self.apply_edit(at, slide);
                }
                self.run_actions();
                self.tick_advance();
                self.handle_audio();
                self.handle_video();
                self.handle_slide_request();
                self.handle_song_theme();
                self.handle_song_loop();
                self.handle_song_transition();
                self.handle_announcement();
                self.handle_arrangement();
                self.handle_copyright();
                self.handle_reflow();
                self.handle_media_request();
                self.handle_picture_request();
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
                self.handle_bible_song();
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
                self.sync_panels(el);
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
        // What runs by the clock is done here too, not only when the operator window draws:
        // macOS stops drawing a window that is minimised or covered, and a timed slide, a
        // slide's actions, or a message that leaves by itself must not wait for it.
        self.run_actions();
        self.tick_advance();
        self.sync_overlay();
        let video_at = self.tick_video();
        let box_video_at = self.tick_slide_video();
        let audio_at = self.tick_audio();
        let announce_at = self.tick_announcements();
        let cycle_at = self.tick_media_cycle();
        self.check_folders(false);
        let folders_at = self
            .running
            .as_ref()
            .filter(|r| r.home_state.media_playlists.iter().any(|p| p.folder.is_some()))
            .map(|_| self.folders_checked + Duration::from_secs(5));
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
        let mut panel_at: Option<Instant> = None;
        for panel in &mut running.panels {
            if panel.next_repaint.is_some_and(|at| at <= Instant::now()) {
                panel.next_repaint = None;
                panel.window.request_redraw();
            }
            panel_at = [panel_at, panel.next_repaint].into_iter().flatten().min();
        }
        let wake = [
            running.next_repaint,
            panel_at,
            save_at,
            video_at,
            box_video_at,
            audio_at,
            announce_at,
            cycle_at,
            folders_at,
        ];
        match wake.into_iter().flatten().min() {
            Some(at) => el.set_control_flow(ControlFlow::WaitUntil(at)),
            None => el.set_control_flow(ControlFlow::Wait),
        }
    }
}

fn main() -> ExitCode {
    applog::start();
    // Picture objects on slides are read from the app's Images folder.
    pc_render::set_picture_loader(images::load);
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
            copyright: pc_model::Copyright::default(),
            theme: None,
            arrangements: Vec::new(),
            arrangement: None,
            loops: false,
            transition: None,
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
        waker: Some(event_loop.create_proxy()),
        show: Show::new(songs.into_iter().map(Arc::new).collect()),
        sources,
        bible_slots,
        pictures: Vec::new(),
        unsaved: Vec::new(),
        history: SongHistory::default(),
        action_depth: None,
        live_since: None,
        advanced: false,
        started: Instant::now(),
        video: None,
        slide_video: None,
        media_playlists: Vec::new(),
        announcing: None,
        media_cycle: None,
        folders_seen: std::collections::HashMap::new(),
        folders_checked: Instant::now(),
        unsaved_since: None,
        theme: Arc::new(themes[songs_theme].clone()),
        theme_for_bible: Arc::new(themes[bible_theme].clone()),
        theme_arcs: themes.iter().cloned().map(Arc::new).collect(),
        themes,
        songs_theme,
        bible_theme,
        bible_layout,
        themes_unsaved: Vec::new(),
        bible_page_saved: String::new(),
        prefs_saved: None,
        frames_since_start: 0,
        player: audio::Player::default(),
        held: winit::keyboard::ModifiersState::empty(),
        capture_selects_box: false,
        focused: None,
        #[cfg(target_os = "macos")]
        menu: None,
        running: None,
        failed: false,
        capture_done: false,
        home_timing: applog::Timing::new("operator window drawings"),
        notice_logged: None,
    };
    // The pictures that were in the media bin last time, then any named on the command line.
    let saved = media::load_list();
    app.media_playlists = saved.playlists;
    let mut pictures_to_load = saved.entries;
    for path in &media_paths {
        if !pictures_to_load.iter().any(|entry| &entry.path == path) {
            pictures_to_load.push(media::Entry {
                id: Uuid::new_v4(),
                path: path.clone(),
                fit: pc_model::ImageFit::Fill,
                looping: true,
            });
        }
    }
    for entry in &pictures_to_load {
        let path = &entry.path;
        // A file listed twice (as an earlier version could save it) is shown once.
        if app.show.media().iter().any(|item| &item.path == path) {
            continue;
        }
        match media::load_picture(path) {
            Ok(picture) => {
                let name = path.file_name().map_or_else(|| "Picture".to_owned(), |n| n.to_string_lossy().into_owned());
                // Its id is kept between runs, so a slide's action that shows it still finds it.
                app.show.add_media(MediaItem {
                    id: entry.id,
                    fit: entry.fit,
                    looping: entry.looping,
                    ..MediaItem::new(name, path.clone())
                });
                app.pictures.push(media::MediaPicture::of(picture));
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
    fn a_macros_actions_are_done_in_turn() {
        use pc_model::{ClearWhat, ShowAction};
        let mut state = home::HomeState::default();
        let logo = props::Prop::new("Logo".to_owned());
        let old = props::Prop::new("Old".to_owned());
        let now = Instant::now();
        state.props = vec![logo.clone(), old.clone()];
        state.props_live = vec![(old.id, now)];
        let song = pc_model::parse_song_text("Song", "One\n\nTwo\n");
        let show = Show::new(vec![Arc::new(song)]);
        // Clear All, then Show Prop: the old prop goes, the logo stays.
        for action in [ShowAction::Clear { what: ClearWhat::All }, ShowAction::Prop { id: logo.id, show: true }] {
            match action_commands(&show, &state, &action) {
                Some(commands) => {
                    for command in &commands {
                        clear_overlays(&mut state, command);
                    }
                }
                None => do_state_action(&mut state, &action, now),
            }
        }
        assert_eq!(state.props_live.iter().map(|(id, _)| *id).collect::<Vec<_>>(), [logo.id]);
    }

    #[test]
    fn going_to_a_song_lands_on_its_first_slide_that_is_not_disabled() {
        let mut song = pc_model::parse_song_text("Song", "One\n\nTwo\n");
        song.groups[0].slides[0].enabled = false;
        let show = Show::new(vec![Arc::new(song)]);
        let state = home::HomeState { song_keys: vec!["song".to_owned()], ..home::HomeState::default() };
        let go = pc_model::ShowAction::Song { key: "song".to_owned() };
        assert_eq!(
            action_commands(&show, &state, &go),
            Some(vec![Command::Select(0), Command::GoTo(pc_engine::SlideRef { presentation: 0, slide: 1 })])
        );
        let gone = pc_model::ShowAction::Song { key: "nowhere".to_owned() };
        assert_eq!(action_commands(&show, &state, &gone), Some(Vec::new()));
        // A media action shows its picture from the media bin, found by its id.
        let mut show = show;
        let picture = pc_model::MediaItem::new("Sky".to_owned(), "/sky.png".into());
        let at = show.add_media(picture.clone());
        let media = pc_model::ShowAction::Media { id: picture.id };
        assert_eq!(action_commands(&show, &state, &media), Some(vec![Command::ShowMedia(at)]));
        let missing = pc_model::ShowAction::Media { id: Uuid::new_v4() };
        assert_eq!(action_commands(&show, &state, &missing), Some(Vec::new()));
    }

    #[test]
    fn undo_steps_go_back_and_forth_and_are_dropped_with_their_song() {
        let a = Arc::new(pc_model::parse_song_text("A", "One\n"));
        let b = Arc::new(pc_model::parse_song_text("B", "Two\n"));
        let mut history = SongHistory::default();
        history.remember(0, a.clone());
        history.remember(1, b.clone());
        assert_eq!(history.peek(false), Some(1));
        let (index, song) = history.step(false, b.clone()).unwrap();
        assert_eq!((index, song.title.as_str()), (1, "B"));
        assert_eq!(history.peek(true), Some(1));
        // A song changed elsewhere (or deleted) loses its steps; another song keeps its own.
        history.forget(1);
        assert_eq!(history.peek(true), None);
        assert_eq!(history.peek(false), Some(0));
        // A new change drops what could be redone.
        history.step(false, a.clone());
        history.remember(0, a);
        assert_eq!(history.peek(true), None);
    }

    /// Writes a song that uses every moving part of the output: a build with the slide, a click
    /// build, a scrolling box, a clock, a slide that goes on by itself, and looping. For smoke
    /// runs: `SMOKE_SONG=/tmp/smoke.pcpres cargo test -p pc-app write_smoke_song -- --ignored`.
    #[test]
    #[ignore = "writes a file, for smoke runs"]
    fn write_smoke_song() {
        let Ok(path) = std::env::var("SMOKE_SONG") else { return };
        let mut song = pc_model::parse_song_text("Smoke", "[Verse 1]\nOne\n\nTwo\n\nThree\n\n[Chorus]\nFour\n");
        song.loops = true;
        let slides: Vec<&mut Slide> = song.groups.iter_mut().flat_map(|group| group.slides.iter_mut()).collect();
        let [first, second, third, fourth] = <[&mut Slide; 4]>::try_from(slides).ok().unwrap();
        first.objects[0].build_in = Some(pc_model::Build::of(pc_model::BuildEffect::Fly));
        first.advance = Some(3.0);
        second.objects[0].build_in =
            Some(pc_model::Build { on_click: true, ..pc_model::Build::of(pc_model::BuildEffect::Zoom) });
        second.advance = Some(2.0);
        if let pc_model::ObjectKind::Text { style, .. } = &mut third.objects[0].kind {
            style.scroll = Some(pc_model::Scroll::Left);
        }
        let mut clock = pc_model::Object::text(pc_model::Role("clock".to_owned()), "[Clock]");
        clock.link = Some(pc_model::TextLink::Clock);
        clock.frame = Some(pc_model::SlideRect { x: 60.0, y: 40.0, w: 600.0, h: 120.0 });
        fourth.objects.push(clock);
        load::save_song(std::path::Path::new(&path), &song).unwrap();
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
        // A slide's own hot key comes before a group's letter, and a disabled slide has none.
        let mut keyed = pc_model::parse_song_text("Song", "[Verse 1]\nOne\n\n[Chorus]\nTwo\n\nThree\n");
        keyed.groups[1].slides[1].hot_key = Some('A');
        keyed.groups[0].slides[0].hot_key = Some('7');
        keyed.groups[0].slides[0].enabled = false;
        let show = Show::new(vec![Arc::new(keyed)]);
        let key = |key: Key| show_key_command(&key, &show);
        assert_eq!(key(Key::Character("a".into())), jump(2));
        assert_eq!(key(Key::Character("7".into())), None);
    }
}
