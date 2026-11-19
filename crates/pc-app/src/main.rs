//! PresenterCraft: a home window with a slide grid, and a separate output for the audience.

mod bible;
mod bible_store;
mod displays;
mod editor;
mod home;
mod icons;
mod inspector;
mod load;
mod media;
mod output;
mod service;
mod studio;
mod theme_store;
mod thumbs;

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
/// draws, so a key press reaches the output without waiting for the home window. A letter
/// jumps to the group it stands for (A for the first verse, C for the chorus, and so on).
fn show_key_command(key: &Key, show: &Show) -> Option<Command> {
    match key {
        Key::Named(NamedKey::ArrowRight | NamedKey::Space) => Some(Command::Next),
        Key::Named(NamedKey::ArrowLeft) => Some(Command::Previous),
        Key::Named(NamedKey::Home) => Some(Command::First),
        Key::Named(NamedKey::End) => Some(Command::Last),
        Key::Named(NamedKey::F1) => Some(Command::ClearAll),
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
    /// The themes with changes not yet written to their files, by place.
    themes_unsaved: Vec<usize>,
    /// The modifier keys held down now. A letter pressed with Command or Control is a
    /// shortcut, not a jump to a group.
    held: winit::keyboard::ModifiersState,
    /// Testing aid: select a box once the Theme Editor has opened.
    capture_selects_box: bool,
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
        let quiet = self.options.capture_home.is_some();
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

        let placement = choose_output(&displays, home_display, self.options.output_display)?;
        let output_attrs = Window::default_attributes().with_title("PresenterCraft Output");
        let output_attrs = match placement {
            Placement::Fullscreen(index) => {
                output_attrs.with_fullscreen(Some(Fullscreen::Borderless(monitors.get(index).cloned())))
            }
            Placement::Windowed => output_attrs.with_inner_size(winit::dpi::LogicalSize::new(960.0, 540.0)),
        };
        let output_attrs = behind(output_attrs);
        let output_window =
            Arc::new(el.create_window(output_attrs).map_err(|e| format!("Could not open the output window: {e}"))?);
        output_window.set_cursor_visible(false);
        if !quiet {
            home_window.focus_window();
        }

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
                theme_names: self.themes.iter().map(|theme| theme.name.clone()).collect(),
                theme_for_songs: self.songs_theme,
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
                    boxes: self.theme_for_bible.boxes.iter().map(|style| style.role.clone()).collect(),
                    ..home::BiblePage::default()
                },
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
        let theme = self.show.live().map_or(&self.theme, |at| self.theme_of(at.presentation)).clone();
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
                theme: theme.clone(),
                enabled: self.show.output_enabled(),
            };
            if !output.send(OutputMsg::Show(frame)) {
                running.home_state.output_stopped = true;
            }
        }
        let layers = Layers { media: media.as_ref(), slide: slide.as_deref() };
        running.preview = running.thumbs.preview(&running.gpu, &mut running.egui_renderer, &layers, &theme);
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

    /// The theme a presentation's slides are drawn with: the Bible theme for a Bible passage,
    /// the songs theme for everything else.
    fn theme_of(&self, presentation: usize) -> &Arc<Theme> {
        if self.bible_slots.contains(&presentation) { &self.theme_for_bible } else { &self.theme }
    }

    /// Brings everything that depends on the themes up to date after one was changed or
    /// chosen: the copies slides are drawn with, every thumbnail, and what the window lists.
    /// The output is left as it is; `refresh_output` brings it up to date.
    fn themes_changed(&mut self) {
        self.songs_theme = self.songs_theme.min(self.themes.len().saturating_sub(1));
        self.bible_theme = self.bible_theme.min(self.themes.len().saturating_sub(1));
        self.theme = Arc::new(self.themes[self.songs_theme].clone());
        self.theme_for_bible = Arc::new(self.themes[self.bible_theme].clone());
        if let Some(running) = self.running.as_mut() {
            running.thumbs.clear_except(&mut running.egui_renderer, |_| false);
            running.canvas_for = None;
            let state = &mut running.home_state;
            state.theme_names = self.themes.iter().map(|theme| theme.name.clone()).collect();
            state.theme_for_songs = self.songs_theme;
            state.theme_for_bible = self.bible_theme;
            state.bible.boxes = self.theme_for_bible.boxes.iter().map(|style| style.role.clone()).collect();
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
                theme_store::remember_in_use(&self.theme, &self.theme_for_bible);
                if !editing {
                    self.refresh_output();
                }
            }
            Some(ThemeRequest::UseForBible(index)) if index < count && index != self.bible_theme => {
                self.bible_theme = index;
                self.themes_changed();
                theme_store::remember_in_use(&self.theme, &self.theme_for_bible);
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
            let theme = edit.theme.clone();
            if let Some(slot) = self.themes.get_mut(index) {
                *slot = theme;
            }
            if !self.themes_unsaved.contains(&index) {
                self.themes_unsaved.push(index);
            }
            self.unsaved_since = Some(Instant::now());
            self.themes_changed();
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
        running.home_state.editing = Some(EditDraft::of_slide(edit.theme.template_slide(edit.template), 0));
        running.home_state.theme_edit = Some(edit);
        running.home_state.bible_open = false;
        running.canvas_for = None;
        running.home_window.request_redraw();
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
        let (show, preview, pictures) = (&self.show, running.preview, &self.pictures);
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
        if let WindowEvent::ModifiersChanged(now) = &event {
            self.held = now.state();
        }
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
                // A Bible passage just looked up goes into its slot. It is never the slot on
                // screen, so the output is untouched.
                if let Some((slot, passage)) = self.running.as_mut().and_then(|r| r.home_state.bible.pending.take()) {
                    self.show.replace_presentation(slot, Arc::new(passage));
                    if let Some(running) = self.running.as_ref() {
                        running.home_window.request_redraw();
                    }
                }
                self.handle_themes();
                // The services are written as soon as they change; the file is small.
                if let Some(running) = self.running.as_mut()
                    && std::mem::take(&mut running.home_state.services.changed)
                    && let Err(problem) = service::save(&running.home_state.services)
                {
                    running.home_state.notice = Some(problem);
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
    let (mut songs, mut sources): (Vec<_>, Vec<_>) = songs.into_iter().unzip();
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

    let mut builder = EventLoop::builder();
    // A run that only saves a picture must not come to the front of whatever is in use.
    #[cfg(target_os = "macos")]
    if options.capture_home.is_some() {
        use winit::platform::macos::{ActivationPolicy, EventLoopBuilderExtMacOS};
        builder.with_activation_policy(ActivationPolicy::Accessory).with_activate_ignoring_other_apps(false);
    }
    let event_loop = match builder.build() {
        Ok(event_loop) => event_loop,
        Err(e) => {
            eprintln!("Could not start the window system: {e}");
            return ExitCode::FAILURE;
        }
    };
    event_loop.set_control_flow(ControlFlow::Wait);
    let (themes, songs_theme, bible_theme) = theme_store::load_all();
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
        themes_unsaved: Vec::new(),
        held: winit::keyboard::ModifiersState::empty(),
        capture_selects_box: false,
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
