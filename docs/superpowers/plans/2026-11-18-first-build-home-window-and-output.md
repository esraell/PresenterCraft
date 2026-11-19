# First Build (Home Window, Slide Grid, Output) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** `presentercraft song.txt` opens a home window with a clickable grid of bilingual slide thumbnails and a separate output window that shows the live slide.

**Architecture:** A Cargo workspace of four crates: `pc-model` (data and the text parser), `pc-engine` (headless show state and commands), `pc-render` (slide to GPU picture, used for output and thumbnails), `pc-app` (windows, egui home window, output thread). The output is drawn by its own thread and never waits on the home window.

**Tech Stack:** Rust 2024; winit 0.30.13; wgpu 30 (re-exported by vello); vello 0.12.0; parley 0.13.0; egui, egui-wgpu, egui-winit 0.37.0; uuid; png (tests).

**Spec:** `docs/superpowers/specs/2026-11-18-presentercraft-foundation-design.md` (sections 5, 6, 7, 9, 10). Working reference for the graphics calls: `spikes/text-spike/src/main.rs`.

## Global Constraints

- Exact pins: `vello = "=0.12.0"`, `parley = "=0.13.0"`, `winit = "=0.30.13"`, `egui = "=0.37.0"`, `egui-wgpu = "=0.37.0"`, `egui-winit = "=0.37.0"`. Use `vello::wgpu`, never a separate `wgpu` dependency.
- `unsafe_code = "forbid"` in the workspace lints.
- Only `pc-app` may depend on winit or egui. `pc-model` and `pc-engine` have no graphics dependencies. Lower crates never depend on higher ones (model < engine < render < app; render does not depend on engine).
- No macOS-only code outside `pc-app`.
- Slides are authored in 1920 x 1080 units.
- Nothing from ProPresenter: no names, wording, icons or layouts. Test and sample text is written for this project, never song lyrics or scripture.
- Nothing slow on the output thread: it only receives snapshots, draws and presents.
- Commits: the owner has not asked for commits yet. Where a task says "checkpoint", run the listed checks; commit only once the owner says to.
- After every task: `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`.

## Review Focus

1. A song file with a `--` line but nothing after it, or nothing before it: the slide keeps only the non-empty box and never shows an empty box. (Task 1)
2. One very long word or a 12-line slide: text shrinks to the floor, then spills, is never clipped, and the slide is flagged as overflowing. (Task 3)
3. A song with hundreds of slides: thumbnails are drawn only for rows that are visible, so opening it does not stall the window. (Task 5)
4. The output window resized to zero, minimised, or its surface reported lost or occluded: the output thread skips the frame and recovers on the next one, without panicking. (Task 4)
5. A file that is not valid UTF-8: reported and skipped, like a missing file. (Task 6)

---

## File Structure

```text
Cargo.toml                         workspace, pins, lints
crates/pc-model/src/lib.rs         re-exports
crates/pc-model/src/types.rs       Presentation, Group, Slide, TextBox, Role, Rgba
crates/pc-model/src/theme.rs       Theme, BoxStyle, SlideRect, Align, Theme::builtin
crates/pc-model/src/song_text.rs   parse_song_text
crates/pc-engine/src/lib.rs        Show, Command, Changes, SlideRef, Layer
crates/pc-render/src/lib.rs        re-exports
crates/pc-render/src/fonts.rs      Fonts
crates/pc-render/src/scene.rs      build_scene, fit_rect, text fitting
crates/pc-render/src/gpu.rs        Painter, target_texture, read_pixels
crates/pc-render/tests/golden.rs   image tests; crates/pc-render/tests/golden/*.png
crates/pc-app/src/main.rs          arguments, loading, event loop
crates/pc-app/src/displays.rs      DisplayInfo, choose_output
crates/pc-app/src/output.rs        output thread and its messages
crates/pc-app/src/home.rs          egui home window
crates/pc-app/src/thumbs.rs        thumbnail and preview textures
samples/sample-song.txt            bundled sample (original text)
```

---

### Task 1: Workspace, model types and the song text parser

**Files:**
- Create: `Cargo.toml`, `crates/pc-model/Cargo.toml`, `crates/pc-model/src/{lib,types,theme,song_text}.rs`

**Interfaces:**
- Produces:

```rust
// types.rs
pub struct Rgba { pub r: u8, pub g: u8, pub b: u8, pub a: u8 }          // Copy, PartialEq
pub struct Role(pub String);                                              // Role::primary(), Role::translation()
pub struct TextBox { pub role: Role, pub text: String }
pub struct Slide { pub id: Uuid, pub boxes: Vec<TextBox>, pub label: Option<String>,
                   pub notes: Option<String>, pub enabled: bool }
pub struct Group { pub id: Uuid, pub name: String, pub color: Rgba, pub slides: Vec<Slide> }
pub struct Presentation { pub id: Uuid, pub title: String, pub groups: Vec<Group> }
impl Presentation {
    pub fn slide_count(&self) -> usize;
    /// Flat index across groups, in order.
    pub fn slide(&self, index: usize) -> Option<(&Group, &Slide)>;
}
impl Slide { pub fn text_for(&self, role: &Role) -> Option<&str>; }

// theme.rs
pub struct SlideRect { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }  // slide units
pub enum Align { Left, Center, Right }
pub struct BoxStyle { pub role: Role, pub rect: SlideRect, pub font_family: String, pub weight: u16,
    pub size: f32, pub min_size: f32, pub fill: Rgba, pub align: Align,
    pub outline_width: f32, pub outline_color: Rgba, pub shadow: bool }
pub struct Theme { pub name: String, pub boxes: Vec<BoxStyle>,
    /// Where a lone `primary` box goes when the slide has no other box.
    pub solo_primary: SlideRect }
impl Theme { pub fn builtin() -> Theme; pub fn style_for(&self, role: &Role) -> Option<&BoxStyle>; }

// song_text.rs
pub fn parse_song_text(title: &str, text: &str) -> Presentation;
pub fn group_color(name: &str) -> Rgba;
```

- Built-in theme values: `primary` rect (120, 170, 1680, 400), size 96, min 40, white, outline 6; `translation` rect (120, 610, 1680, 320), size 64, min 30, fill (255, 150, 40), outline 4; both weight 700, family "Helvetica Neue", centred, black outline, shadow on; `solo_primary` (120, 240, 1680, 600).
- `group_color`: a fixed palette of eight colours; names starting (case-insensitive) with "verse", "chorus", "bridge", "pre", "intro", "ending"/"outro", "tag" get fixed entries; anything else gets a palette entry chosen by a stable hash of the name; the unnamed group is grey (128, 128, 128).

- [ ] **Step 1: Write the failing tests** in `song_text.rs` (`#[cfg(test)]`):

```rust
const SONG: &str = "[Verse 1]\nLine a\nLine b\n--\nСтрока а\nСтрока б\n\nSecond slide\n\n\n[Chorus]\nSing\n--\nПой\n";

#[test] fn groups_and_slides() {
    let p = parse_song_text("T", SONG);
    assert_eq!(p.title, "T");
    assert_eq!(p.groups.iter().map(|g| (g.name.as_str(), g.slides.len())).collect::<Vec<_>>(),
               vec![("Verse 1", 2), ("Chorus", 1)]);
    assert_eq!(p.slide_count(), 3);
}
#[test] fn box_split() {
    let p = parse_song_text("T", SONG);
    let (_, s) = p.slide(0).unwrap();
    assert_eq!(s.text_for(&Role::primary()), Some("Line a\nLine b"));
    assert_eq!(s.text_for(&Role::translation()), Some("Строка а\nСтрока б"));
    let (_, s) = p.slide(1).unwrap();
    assert_eq!(s.boxes.len(), 1);
}
#[test] fn text_before_any_header_goes_to_unnamed_group() {
    let p = parse_song_text("T", "Alone\n\n[Verse]\nX\n");
    assert_eq!(p.groups[0].name, ""); assert_eq!(p.groups[0].slides.len(), 1);
}
#[test] fn windows_line_endings_and_trailing_blank_lines() {
    let p = parse_song_text("T", "[V]\r\nA\r\n--\r\nБ\r\n\r\n\r\n");
    assert_eq!(p.slide_count(), 1);
    assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Б"));
}
#[test] fn empty_file_has_no_slides() { assert_eq!(parse_song_text("T", "").slide_count(), 0); }
#[test] fn empty_side_of_split_makes_no_empty_box() {
    let p = parse_song_text("T", "A\n--\n\nB\n--\n");      // review focus 1
    assert_eq!(p.slide(0).unwrap().1.boxes.len(), 1);
    assert_eq!(p.slide(1).unwrap().1.boxes.len(), 1);
    let p = parse_song_text("T", "--\nТолько перевод\n");
    assert_eq!(p.slide(0).unwrap().1.text_for(&Role::translation()), Some("Только перевод"));
    assert_eq!(p.slide(0).unwrap().1.text_for(&Role::primary()), None);
}
#[test] fn empty_groups_are_dropped() {
    assert_eq!(parse_song_text("T", "[A]\n[B]\nX\n").groups.len(), 1);
}
#[test] fn same_group_name_gets_same_color() { assert_eq!(group_color("Chorus"), group_color("chorus 2")); }
```

- [ ] **Step 2:** Run `cargo test -p pc-model`; expect compile failures.
- [ ] **Step 3:** Implement the types, `Theme::builtin` and `parse_song_text`. Parser rules: normalise `\r\n`; a trimmed line of the form `[name]` starts a group; blank lines end a slide; a trimmed line equal to `--` switches from `primary` to `translation` (only the first one in a slide; later ones are text); trailing whitespace on text lines is trimmed; a box with no text is not created; a slide with no boxes is not created; a group with no slides is dropped.
- [ ] **Step 4:** Run `cargo test -p pc-model`; expect all pass.
- [ ] **Step 5:** Checkpoint.

---

### Task 2: The show engine

**Files:**
- Create: `crates/pc-engine/Cargo.toml`, `crates/pc-engine/src/lib.rs`

**Interfaces:**
- Consumes: `pc_model::{Presentation, Slide, Group}`.
- Produces:

```rust
pub enum Layer { Audio, Media, Slides, Announcements, Props, Messages, Masks }   // Copy, PartialEq
pub struct SlideRef { pub presentation: usize, pub slide: usize }                // Copy, PartialEq; indexes
pub enum Command { Select(usize), GoTo(SlideRef), Next, Previous, First, Last,
                   Clear(Layer), ClearAll, SetOutputEnabled(bool) }
pub struct Changes { pub slides: bool, pub selection: bool, pub output_enabled: bool }   // Default
pub struct Show { /* private */ }
impl Show {
    pub fn new(presentations: Vec<Arc<Presentation>>) -> Show;   // selected = 0, nothing live, output on
    pub fn apply(&mut self, command: Command) -> Changes;
    pub fn presentations(&self) -> &[Arc<Presentation>];
    pub fn selected(&self) -> usize;
    pub fn live(&self) -> Option<SlideRef>;
    /// The last slide that was live; kept after a clear so Next can continue.
    pub fn cursor(&self) -> Option<SlideRef>;
    pub fn live_slide(&self) -> Option<(&Arc<Presentation>, &Slide)>;
    pub fn output_enabled(&self) -> bool;
}
```

- Rules: out-of-range `Select` or `GoTo` is ignored (no change reported). `Next`: with a cursor, go to cursor + 1 in the cursor's presentation, staying on the last slide at the end (and if nothing is live, show the cursor slide's successor, or the cursor slide itself when it was the last); with no cursor, go to slide 0 of the selected presentation. `Previous`: cursor − 1, stopping at 0; with no cursor, nothing. `First` / `Last`: first / last slide of the selected presentation. `Clear(Layer::Slides)` and `ClearAll` set live to none and keep the cursor; clearing other layers changes nothing yet. `Select` never changes what is live. A presentation with zero slides is skipped by `Next`, `First`, `Last` (no change).

- [ ] **Step 1: Write the failing tests** (two presentations with 3 and 2 slides built by a helper):

```rust
#[test] fn starts_with_nothing_live() { let s = show(); assert_eq!(s.live(), None); assert!(s.output_enabled()); }
#[test] fn goto_reports_slide_change() {
    let mut s = show(); let c = s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 1 }));
    assert!(c.slides); assert_eq!(s.live(), Some(SlideRef { presentation: 0, slide: 1 }));
}
#[test] fn next_with_nothing_live_goes_to_first_of_selected() {
    let mut s = show(); s.apply(Command::Select(1)); s.apply(Command::Next);
    assert_eq!(s.live(), Some(SlideRef { presentation: 1, slide: 0 }));
}
#[test] fn next_and_previous_stop_at_the_ends() {
    let mut s = show(); for _ in 0..9 { s.apply(Command::Next); }
    assert_eq!(s.live(), Some(SlideRef { presentation: 0, slide: 2 }));
    assert!(!s.apply(Command::Next).slides);
    for _ in 0..9 { s.apply(Command::Previous); }
    assert_eq!(s.live(), Some(SlideRef { presentation: 0, slide: 0 }));
}
#[test] fn clear_then_next_continues() {
    let mut s = show(); s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 0 }));
    assert!(s.apply(Command::Clear(Layer::Slides)).slides);
    assert_eq!(s.live(), None);
    s.apply(Command::Next);
    assert_eq!(s.live(), Some(SlideRef { presentation: 0, slide: 1 }));
}
#[test] fn clear_when_already_clear_reports_nothing() { let mut s = show(); assert!(!s.apply(Command::ClearAll).slides); }
#[test] fn select_does_not_change_live() {
    let mut s = show(); s.apply(Command::GoTo(SlideRef { presentation: 0, slide: 2 }));
    let c = s.apply(Command::Select(1)); assert!(c.selection && !c.slides);
    assert_eq!(s.live(), Some(SlideRef { presentation: 0, slide: 2 }));
}
#[test] fn first_and_last_use_the_selected_presentation() {
    let mut s = show(); s.apply(Command::Select(1)); s.apply(Command::Last);
    assert_eq!(s.live(), Some(SlideRef { presentation: 1, slide: 1 }));
    s.apply(Command::First); assert_eq!(s.live(), Some(SlideRef { presentation: 1, slide: 0 }));
}
#[test] fn out_of_range_is_ignored() {
    let mut s = show(); assert!(!s.apply(Command::GoTo(SlideRef { presentation: 5, slide: 0 })).slides);
    assert!(!s.apply(Command::Select(9)).selection);
}
#[test] fn output_toggle() { let mut s = show(); assert!(s.apply(Command::SetOutputEnabled(false)).output_enabled);
    assert!(!s.apply(Command::SetOutputEnabled(false)).output_enabled); }
#[test] fn empty_show_never_panics() { let mut s = Show::new(vec![]);
    for c in [Command::Next, Command::Previous, Command::First, Command::Last, Command::ClearAll] { s.apply(c); } }
```

- [ ] **Step 2:** Run `cargo test -p pc-engine`; expect compile failures.
- [ ] **Step 3:** Implement `Show` and `apply` to the rules above.
- [ ] **Step 4:** Run `cargo test -p pc-engine`; expect all pass.
- [ ] **Step 5:** Checkpoint.

---

### Task 3: The renderer

**Files:**
- Create: `crates/pc-render/Cargo.toml`, `crates/pc-render/src/{lib,fonts,scene,gpu}.rs`, `crates/pc-render/tests/golden.rs`, `crates/pc-render/tests/golden/*.png` (generated)

**Interfaces:**
- Consumes: `pc_model::{Slide, Theme, BoxStyle, Role, Rgba, SlideRect, Align}`.
- Produces:

```rust
pub use vello; pub use vello::wgpu;
pub struct Fonts { /* parley FontContext + LayoutContext<()> */ }
impl Fonts { pub fn new() -> Fonts; }                      // system fonts
/// Largest x, y, w, h (in target pixels) of a 16:9 slide centred inside the target.
pub fn fit_rect(target_w: u32, target_h: u32) -> (f64, f64, f64, f64);
pub struct Built { pub scene: vello::Scene, pub overflow: bool }
/// `slide == None` draws black only. `overflow` is true when a box's text did not fit at min_size.
pub fn build_scene(fonts: &mut Fonts, slide: Option<&Slide>, theme: &Theme,
                   target_w: u32, target_h: u32) -> Built;
pub struct Painter { /* vello::Renderer */ }
impl Painter {
    pub fn new(device: &wgpu::Device) -> Painter;
    pub fn paint(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, scene: &vello::Scene,
                 target: &wgpu::TextureView, w: u32, h: u32);
}
/// Rgba8Unorm with STORAGE_BINDING | TEXTURE_BINDING | COPY_SRC.
pub fn target_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture;
/// Tightly packed RGBA rows.
pub fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8>;
```

- Drawing, per box, as the spike does: soft shadow (six strokes, widths `2*outline + i*5`, alpha 0.10, offset (6, 8) slide units) if `shadow`, then outline stroke (`2 * outline_width`, round joins), then fill. Slide background is black.
- A slide whose only box is `primary` uses `theme.solo_primary` instead of the style's rect.
- A box whose role has no style in the theme is drawn with the `primary` style in `solo_primary`.
- Text is centred vertically inside its box.
- Fitting: lay out at `size`; while the laid-out height exceeds the box height and the size is above `min_size`, reduce the size by 4% and lay out again; if it still exceeds at `min_size`, keep `min_size`, set `overflow`, and draw without clipping.

- [ ] **Step 1: Write the failing tests.** Unit tests in `scene.rs`:

```rust
#[test] fn fit_rect_same_shape() { assert_eq!(fit_rect(1920, 1080), (0.0, 0.0, 1920.0, 1080.0)); }
#[test] fn fit_rect_4_3_letterboxes() { let (x, y, w, h) = fit_rect(1600, 1200);
    assert_eq!((x, w), (0.0, 1600.0)); assert_eq!(h, 900.0); assert_eq!(y, 150.0); }
#[test] fn fit_rect_wide_pillarboxes() { let (x, _, w, h) = fit_rect(3840, 1080);
    assert_eq!((w, h), (1920.0, 1080.0)); assert_eq!(x, 960.0); }
#[test] fn short_text_does_not_overflow() { /* build_scene on a two-line bilingual slide → !overflow */ }
#[test] fn huge_text_overflows_not_panics() { /* 40 lines in primary, and one 400-character word → overflow */ }   // review focus 2
#[test] fn empty_slide_builds() { /* build_scene(None) and a Slide with no boxes → !overflow */ }
```

Image tests in `tests/golden.rs`, each rendering through `Painter` and `read_pixels` and comparing with `tests/golden/<name>.png`; a missing file, or `PC_UPDATE_GOLDEN=1`, writes it instead. A comparison passes when the mean absolute channel difference is under 2.0 and fewer than 0.5% of pixels differ by more than 32 in any channel:

```rust
#[test] fn bilingual_1920x1080()   // "Light rises over the quiet hills\nAnd the morning sings again" / "Свет встаёт над тихими холмами\nІ ранок знову співає: їжак, ґанок, є"
#[test] fn wrapping_long_line()    // one 160-character primary line
#[test] fn shrink_to_fit()         // 9 primary lines
#[test] fn solo_primary_centred()  // one box
#[test] fn letterbox_4_3()         // target 1600x1200; rows 0..150 and 1050..1200 are black
#[test] fn cleared_is_black()      // slide None → every pixel (0,0,0,255)
```

`letterbox_4_3` and `cleared_is_black` also assert on the pixels directly, so they check behaviour and not only the stored image.

- [ ] **Step 2:** Run `cargo test -p pc-render`; expect compile failures.
- [ ] **Step 3:** Implement `fonts.rs`, `scene.rs`, `gpu.rs`, following `spikes/text-spike/src/main.rs` for the vello, parley and wgpu calls (set the alignment before `break_all_lines`; convert `normalized_coords` with `to_bits`; `get_mapped_range()` returns a `Result`).
- [ ] **Step 4:** Run `cargo test -p pc-render` twice (the first run writes the images, the second compares); expect all pass. Open `bilingual_1920x1080.png` and check it by eye against `spikes/text-spike/result-1920x1080.png`.
- [ ] **Step 5:** Checkpoint.

---

### Task 4: The app skeleton: displays and the output thread

**Files:**
- Create: `crates/pc-app/Cargo.toml` (binary name `presentercraft`), `crates/pc-app/src/{main,displays,output}.rs`

**Interfaces:**
- Consumes: `pc_render::{Fonts, Painter, build_scene, target_texture, wgpu}`, `pc_model::{Slide, Theme}`.
- Produces:

```rust
// displays.rs
pub struct DisplayInfo { pub index: usize, pub name: String, pub x: i32, pub y: i32, pub w: u32, pub h: u32 }
pub enum Placement { Fullscreen(usize), Windowed }       // PartialEq, Debug
/// `home` is the index of the display holding the home window.
pub fn choose_output(displays: &[DisplayInfo], home: usize, requested: Option<usize>) -> Result<Placement, String>;

// main.rs
pub struct Gpu { pub device: wgpu::Device, pub queue: wgpu::Queue }   // one per process, cloned to the output thread

// output.rs
pub struct Frame { pub slide: Option<Arc<Slide>>, pub theme: Arc<Theme>, pub enabled: bool }
pub enum OutputMsg { Show(Frame), Resized(u32, u32), Shutdown }
pub struct OutputHandle { /* Sender<OutputMsg>, JoinHandle */ }
impl OutputHandle {
    /// Takes ownership of the surface; the thread owns it from here on.
    pub fn spawn(device: wgpu::Device, queue: wgpu::Queue, surface: wgpu::Surface<'static>,
                 format: wgpu::TextureFormat, w: u32, h: u32) -> OutputHandle;
    pub fn send(&self, msg: OutputMsg);
    pub fn shutdown(self);                                  // sends Shutdown and joins
}
```

- `choose_output`: a requested index that exists gives `Fullscreen(index)`; one that does not gives `Err("There is no display N; run with --list-displays.")`. With no request: the lowest-indexed display other than `home` gives `Fullscreen`; with only one display, `Windowed`.
- The output thread keeps the last `Frame`. On `Show` or `Resized` it redraws once: `enabled == false` or `slide == None` draws black. It draws the scene at the surface's pixel size into a `target_texture`, copies it to the surface with a `wgpu::util::TextureBlitter`, and presents. It blocks on the channel between messages (no continuous redraw). Zero-size `Resized` is ignored. A surface that is lost or outdated is reconfigured and the frame retried once; occluded or timed-out frames are skipped. No `unwrap` or `expect` on surface or channel results in the thread loop. In this build a screen has exactly one output, so the screen's texture is the size of that output's surface and letterboxing is done by `build_scene`.
- `main.rs` in this task: parse `--list-displays`, `--output-display N`, create the GPU instance, device and queue once, open the output window (borderless fullscreen or a 960 x 540 window titled "PresenterCraft Output", cursor hidden while over it), spawn the output thread, forward window resizes as `Resized`, and show a fixed sample slide. The home window arrives in task 5.

- [ ] **Step 1: Write the failing tests** in `displays.rs`:

```rust
#[test] fn one_display_is_windowed()          // [d0], home 0, None → Windowed
#[test] fn two_displays_picks_the_other()     // [d0, d1], home 0 → Fullscreen(1); home 1 → Fullscreen(0)
#[test] fn three_displays_picks_lowest_other()// home 1 → Fullscreen(0)
#[test] fn request_is_honoured_even_if_home() // requested Some(0), home 0 → Fullscreen(0)
#[test] fn bad_request_is_an_error()          // requested Some(4) → Err containing "no display 4"
#[test] fn no_displays_is_windowed()          // [] → Windowed
```

- [ ] **Step 2:** Run `cargo test -p pc-app`; expect compile failures.
- [ ] **Step 3:** Implement `displays.rs`, `output.rs` and the task-4 `main.rs`.
- [ ] **Step 4:** Run `cargo test -p pc-app`; expect pass. Run `cargo run -p pc-app -- --list-displays`; expect one line per display. Run `cargo run -p pc-app`; expect a window showing the sample slide; resize it and minimise it (review focus 4); expect no panic and a correct picture after restoring. Close it; expect a clean exit.
- [ ] **Step 5:** Checkpoint.

---

### Task 5: The home window

**Files:**
- Create: `crates/pc-app/src/{home,thumbs}.rs`
- Modify: `crates/pc-app/src/main.rs`

**Interfaces:**
- Consumes: `pc_engine::{Show, Command, SlideRef, Layer}`, `OutputHandle`, `pc_render::{Painter, Fonts, build_scene, target_texture}`.
- Produces:

```rust
// thumbs.rs
pub struct Thumbs { /* main-thread Painter, Fonts, cache keyed by (slide id, width) */ }
impl Thumbs {
    pub fn new(device: &wgpu::Device) -> Thumbs;
    /// Draws the slide at w x (w * 9 / 16) if not cached and returns the egui texture and whether it overflows.
    pub fn get(&mut self, gpu: &Gpu, egui_renderer: &mut egui_wgpu::Renderer,
               slide: &Slide, theme: &Theme, w: u32) -> (egui::TextureId, bool);
    pub fn clear(&mut self, egui_renderer: &mut egui_wgpu::Renderer);
}
// home.rs
/// Draws the home window for one frame and returns the commands the operator issued.
pub fn home_ui(ctx: &egui::Context, show: &Show, ui_state: &mut HomeState,
               thumbs: &mut dyn FnMut(&Slide, u32) -> (egui::TextureId, bool),
               preview: Option<egui::TextureId>) -> Vec<Command>;
pub struct HomeState { pub thumb_width: f32 }      // default 240, range 120..=480
```

- Layout: a left panel (200 wide) titled "Songs" listing presentation titles, the selected one highlighted, click sends `Select`. A right panel (280 wide) with the heading "Live", the preview picture at 16:9, a "Clear slide" button, an "Output on" checkbox, and a line "Nothing live" or "<title>: slide N of M". The centre shows the selected presentation's title and a wrapping grid of thumbnails in a vertical scroll area; under each thumbnail its number, its group name, and a bar in the group's colour; a thumbnail whose slide overflows shows a small warning mark; the live slide has a 3-pixel accent outline. A slider under the grid sets `thumb_width`. Clicking a thumbnail sends `GoTo`.
- Thumbnails are requested only for cells that egui reports as visible (`ui.is_rect_visible`), so a long song costs nothing off screen (review focus 3). Thumbnail textures are drawn at 480 wide regardless of the displayed size. The cache is cleared when the selection changes.
- Keys, when no text field has focus: Right or Space `Next`; Left `Previous`; Home `First`; End `Last`; C `Clear(Layer::Slides)`.
- `main.rs`: the home window opens at 1280 x 800 titled "PresenterCraft" on the primary display; each frame it runs egui, applies the returned commands to `Show`, and on `Changes.slides` or `Changes.output_enabled` sends the output thread a new `Frame` and redraws the preview (640 x 360). Closing the home window shuts the output down and exits. egui redraws only on input or when it asks for a repaint.
- Colours and spacing use egui's defaults in dark mode with one accent colour, (64, 156, 255). No icons or wording taken from any other product.

- [ ] **Step 1: Write the failing test** in `home.rs`, using a headless `egui::Context`:

```rust
#[test] fn keys_map_to_commands() {
    // feed RawInput with Key::ArrowRight, ArrowLeft, Space, Home, End, C pressed in turn through
    // home_ui with stub textures; assert the returned commands are
    // [Next], [Previous], [Next], [First], [Last], [Clear(Layer::Slides)].
}
#[test] fn empty_show_draws_without_panicking() { /* Show::new(vec![]) → home_ui returns [] */ }
#[test] fn offscreen_thumbnails_are_not_requested() {
    // a presentation with 400 slides in a 1280 x 800 context: the thumbs closure is called fewer than 80 times.
}
```

- [ ] **Step 2:** Run `cargo test -p pc-app`; expect compile failures.
- [ ] **Step 3:** Implement `thumbs.rs`, `home.rs` and the wiring in `main.rs`. Read the egui-wgpu 0.37 and egui-winit 0.37 sources in the Cargo registry for the exact `Renderer`, `ScreenDescriptor` and `State` calls before writing them.
- [ ] **Step 4:** Run `cargo test -p pc-app`; expect pass. Run `cargo run -p pc-app`; click thumbnails, use every key, toggle "Output on", press "Clear slide", drag the size slider, resize both windows. Expect the output to follow every action and the preview to match it.
- [ ] **Step 5:** Checkpoint.

---

### Task 6: Loading songs, the sample, and the run check

**Files:**
- Create: `samples/sample-song.txt`, `README.md`
- Modify: `crates/pc-app/src/main.rs`

**Interfaces:**
- Produces: `pub fn load_songs(paths: &[PathBuf]) -> (Vec<Presentation>, Vec<String>)` in `main.rs` (or a `load.rs` beside it): presentations in argument order, plus one plain message per path that failed.

- Rules: a path that is a file is read as UTF-8 and parsed, with the file name without its extension as the title; a path that is a folder loads every `*.txt` directly inside it, sorted by name; a missing or unreadable file, a file that is not valid UTF-8 (review focus 5), or one with no slides produces a message (`"<path>: <reason>"`) on stderr and is skipped; with no path arguments the bundled sample (`include_str!`) is loaded with the title "Sample song"; if arguments were given and nothing loaded, print "Nothing to show." and exit with code 2 without opening a window.
- `samples/sample-song.txt`: three groups (Verse 1, Chorus, Verse 2), about eight slides, English with Russian or Ukrainian under `--`, including one slide with a single box. Original text written for this project.
- `README.md`: what it is, how to run it, the keys, the song text format, and a pointer to the design.

- [ ] **Step 1: Write the failing tests** (temporary folders via `std::env::temp_dir` plus a unique name):

```rust
#[test] fn loads_a_file_with_title_from_name()
#[test] fn loads_a_folder_sorted_and_only_txt()
#[test] fn missing_file_is_reported_and_skipped()
#[test] fn invalid_utf8_is_reported_and_skipped()     // write bytes [0xff, 0xfe, 0x00]
#[test] fn file_without_slides_is_reported()
#[test] fn sample_song_parses_with_both_languages()   // the bundled sample has ≥ 6 slides and a translation box
```

- [ ] **Step 2:** Run `cargo test -p pc-app`; expect compile failures.
- [ ] **Step 3:** Implement loading, write the sample and the README.
- [ ] **Step 4:** Run the whole check: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `cargo build --release`. Then `./target/release/presentercraft` (sample), `./target/release/presentercraft samples/` and `./target/release/presentercraft /no/such/file` (expect exit code 2 and no window).
- [ ] **Step 5:** Checkpoint, then tell the owner it is ready to try, with what was and was not verified (a real second display has not been tested).
