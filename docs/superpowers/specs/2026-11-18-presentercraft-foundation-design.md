# PresenterCraft: foundation and first slice

Design, 2026-11-18. Status: the owner chose the approach (direct build, native GPU renderer,
macOS first, FFmpeg) and asked for a first runnable build with the home window. The remaining
calls in section 5 are mine and open to correction.

Research this rests on is in `docs/research/` (start with `00-synthesis.md`). The spike that
tested the renderer is in `spikes/text-spike/` (see its `README.md`).

## 1. Intent

Build an original presentation app for church services, in Rust, that does the job the owner
does today in ProPresenter 22: put song lyrics, scripture, sermon slides and media on an audience
screen and a stage screen, run from one operator window.

Why: the owner wants their own app instead of a subscription product. The owner's services are
bilingual (English with Russian or Ukrainian on the same slide), their main song library has
about 800 presentations, their audience screen is HDMI and their stage screen is sent over NDI.

This document covers two things:

1. The foundation: how the code is organised and the decisions every later slice depends on.
2. The first runnable build in full (section 9): slices 1 and 2 together, because the owner
   asked for the home window in the proof of concept. That is bilingual lyrics from a text file,
   a home window with a clickable slide grid, and an output screen.

Later slices are listed in order (section 8) and each gets its own short design before it is
built.

## 2. Goals, non-goals, anti-goals

**Goals**

- One renderer draws the audience output, the slide thumbnails and the stage screen, so the
  operator always sees exactly what goes out.
- A slide has several named text boxes; two languages on one slide is the normal case, not a
  workaround.
- Latin and Cyrillic text (Russian and Ukrainian) with outline and shadow, readable over any
  background.
- Every slice ends in something the owner can run on their Mac.
- The show logic runs without any window, so it can be tested and later driven by a remote.
- **Everything the home window can do, a program can do.** (Owner's goal, 2026-11-18.) Every
  operator action is an engine command, and the same commands are offered through an HTTP API,
  a command-line tool and an MCP server, so scripts and AI agents can run a service. The home
  window is one client among several and gets no private powers.
- **Fast and light.** (Owner's goal, 2026-11-18.) Start quickly, open a large library quickly,
  put a slide on screen within one frame of the key press, and keep memory low. Concretely:
  load presentations on demand instead of the whole library at start; draw thumbnails only
  for what is on screen and cap what is cached; redraw outputs only when something changes;
  use the computer's GPU for all drawing and its hardware decoder for video. Each slice that
  adds data or media states its start-up time and memory budget and measures them.

**Non-goals for now**

- Windows and Linux builds. The code must not make them impossible (see constraints), but only
  macOS is built and tested.
- Feature parity with ProPresenter. We build what the owner's services need, in the order in
  section 8.
- Cloud sync, accounts, licensing, stock media.

**Anti-goals** (these would make the project a failure even if it works)

- Any code, schema, asset, name, logo or wording taken from ProPresenter, or anything derived
  from decompiling it or from schemas extracted from its binary.
- A copy of ProPresenter's look. The home window works the way operators of presentation
  software expect, with the job laid out as such software commonly does (libraries and
  playlists on the left, slide grid in the middle, preview and layer controls on the right,
  media along the bottom). Those arrangements are common across presentation software. What we
  do not take is its artwork, icons, exact wording, colours or pixel-level styling.
- An output that can glitch, freeze or show the wrong thing in a live service because the
  operator interface was busy.

## 3. Constraints

- **Platform:** macOS first (the owner runs macOS 27 on Apple silicon). The model, engine and
  renderer crates contain no macOS-only code; platform-specific code is confined to the app
  crate and, later, the media and NDI crates.
- **Safe Rust:** `unsafe` is forbidden in our own crates, except in a later, isolated crate
  that binds FFmpeg or NDI if no safe binding exists.
- **Pinned versions:** the graphics crates change their APIs between releases (the spike hit
  four differences). Versions are pinned exactly and `Cargo.lock` is committed.
- **Legal guardrails:** see section 10.
- **Live safety:** nothing slow (file reads, imports, decoding) runs on the thread that draws
  an output.

## 4. Approaches considered

### Who writes the code

| Option | Trade-off |
|---|---|
| **Build directly in this repo with Claude Code** (chosen by the owner) | Full use of the research, real tests, review at each slice. |
| Have Craft Anything drive the build | In Clean room mode it passes the builder only screenshots and words, and its only check for an app is whether it compiles. |

### How the app is drawn

The deciding requirement: one renderer must draw outputs, thumbnails and the stage screen;
open fullscreen on a chosen display; draw Cyrillic with outline and shadow; and later put
video under text and hand finished frames to NDI.

| Option | Trade-off |
|---|---|
| **A. Native GPU renderer** (chosen by the owner) | We own every frame, so video compositing and NDI readback are straightforward. Text effects and the operator interface are our work. |
| B. Rust core with a web-view shell (Tauri) | Text effects and video come free from the browser. No documented way to capture web-view frames for NDI was found; the one open-source rival that does this uses Electron. Rejected because the owner's stage screen is NDI. |
| C. Web operator interface with a native output renderer | Two renderers, so thumbnails would not exactly match the output. Rejected. |

The spike confirmed option A: outlined, shadowed English, Russian and Ukrainian text rendered
correctly off screen and fullscreen at over 100 frames per second on the owner's Mac.

## 5. Decisions

**Decided by the owner**

- Build directly in the `presentercraft` repo.
- Native GPU renderer (option A).
- macOS first; keep the door open for Windows.
- FFmpeg for video decoding.

**My calls** (correct any of these)

- **Graphics stack:** `winit` 0.30.13 for windows, `wgpu` 30 for the GPU, `vello` 0.12.0 for
  2D drawing, `parley` 0.13.0 for text layout and font lookup. These are what the spike used.
- **Operator interface toolkit:** `egui` 0.37 (with `egui-wgpu` and `egui-winit`), from the
  first build. A compile check confirmed that egui-wgpu 0.37 and vello 0.12 use the same
  `wgpu` 30 and `winit` 0.30.13 types, so they share one GPU device and the home window can
  show renderer-drawn thumbnails as textures. Running them together in one window is first
  proven by the first build itself.
- **Outputs are isolated from the home window.** Windows are created on the main thread (macOS
  requires it), but every output is drawn and presented by a dedicated output thread that owns
  the output surfaces and its own renderer. The home window, egui and thumbnail drawing stay on
  the main thread with a second renderer. Both share one GPU device and queue, which are safe
  to use from several threads. The engine sends the output thread a complete, immutable
  description of what to show; the output thread never waits on the main thread. This is in
  place from the first build because adding it later would mean rewriting the app crate.
- **One picture per screen.** A screen is rendered once, to one texture, at the screen's pixel
  size. Each output copies that texture, scaled and letterboxed if its shape differs; NDI will
  read the same texture back. A screen's pixel size defaults to the native size of its first
  display output (1920 x 1080 when it has none), so text is sharp on the real display.
  Slides are authored in 1920 x 1080 units and scaled as vectors to the screen's size.
  Thumbnails and the home window's live preview are separate, smaller renders made by the same
  renderer code from the same slide and theme: the same drawing, not the same pixels.
- **Soft shadow:** drawn as several widening, faint strokes under the text, as in the spike.
  The renderer has no glyph blur; a true blur pass is deferred until someone finds the fake
  one lacking.
- **Our own file format:** one human-readable JSON file per presentation, with a `version`
  field from day one, in a library folder the owner chooses. No database.
- **Import route:** the first importer reads from the owner's running ProPresenter through its
  public HTTP API (read-only requests), which returns each slide's text with the two text boxes
  separated by a carriage return, plus groups and their colours. No ProPresenter file parsing.
- **Names:** crates are `pc-model`, `pc-engine`, `pc-render`, `pc-app`; the binary is
  `presentercraft`.
- **Tests:** unit tests for the model and engine; image tests for the renderer (render to
  pixels, compare with a stored image within a tolerance). Image tests use the system font and
  need a GPU, so they are only required to pass on the owner's Mac for now; there is no CI
  yet. When they must pass elsewhere, bundle an open-licence font covering Latin and Cyrillic
  for the tests.
- **Text that will not fit:** it is shrunk to a floor size; if it still overflows it spills
  past its box and is never clipped, because cut-off lyrics are worse than untidy ones. The home
  window marks such a slide with a warning.
- **A slide with one text box:** the built-in theme centres a lone `primary` box on the slide
  instead of leaving it in the upper half.

## 6. Architecture

A Cargo workspace. Lower crates never depend on higher ones.

```text
pc-model    Data only: presentations, slides, text boxes, groups, themes, playlists.
            Serialisation and file-format versions. No drawing, no windows, no I/O beyond
            reading and writing its own files.
pc-engine   The show: what is live on each layer, the current playlist and slide, and the
            commands that change it (go to slide, next, previous, clear). Headless and
            deterministic: a command goes in, a new state and a list of changes come out.
pc-render   Turns "a slide plus a theme plus a target size" into a drawn scene, and draws
            scenes to a GPU texture. Used for outputs, thumbnails and (later) stage screens
            and NDI frames. Knows nothing about windows.
pc-app      The program: windows, displays, keyboard, wiring the engine to the renderer.
            The only crate allowed to use winit, and later egui.
```

Later crates, each added by the slice that needs it: `pc-import` (importers), `pc-media`
(FFmpeg decoding to GPU textures), `pc-ndi` (NDI send), `pc-api` (remote-control HTTP API).

**How a slide reaches the screen**

1. The operator presses a key; `pc-app` turns it into an engine command.
2. `pc-engine` updates the show state and reports that the slide layer changed.
3. `pc-app` sends the output thread an immutable snapshot of what each screen now shows.
4. The output thread has `pc-render` draw each changed screen once, to that screen's texture,
   then copies the texture to each of the screen's outputs and presents them.

Outputs redraw only when their content changes or an animation is running, not continuously.

**Layers.** The engine models output as a stack of layers from the start, even though slice 1
fills only one: media (bottom), slides, announcements, props, messages, masks (top), plus
audio. Each layer can be cleared on its own, and there is a clear-all. This is the shape the
research found (`00-synthesis.md` section 1) and it is cheap to have in place before media
arrives.

**Screens and outputs.** A screen is one rendered picture at a logical size (for example the
audience screen at 1920 x 1080). An output is where a screen's picture is sent: a display, a
window, later an NDI stream. Slice 1 has one audience screen with one output.

## 7. Data model

Only what slice 1 to 3 need is specified; later slices extend it.

- **Library:** a folder of presentation files, with a name.
- **Presentation:** id, title, an ordered list of groups, optional notes.
- **Group:** id, name (for example "Verse 1", "Chorus"), colour, an ordered list of slides.
  The same name may appear more than once in a presentation.
- **Slide:** id, an ordered list of text boxes, optional label, optional notes, enabled flag.
- **Text box:** a **role** and the text. Roles are names; the two built-in ones are `primary`
  and `translation`. Themes, and later the Bible and stage screens, address boxes by role.
  Text keeps its line breaks.
- **Theme:** for each role, a box position and size in slide units, font family, weight, size,
  fill colour, alignment, outline width and colour, shadow on or off. Slides are authored at
  1920 x 1080 and scaled to the output.
- **Playlist** (slice 3): a name and an ordered list of items, each a header or a reference to
  a presentation in a library. A playlist holds references, never copies.

Ids are UUIDs so references survive renames.

### File format

One UTF-8 JSON file per presentation, `<title>.pcpres`, containing `"version": 1` and the
fields above. A file with a higher version than the program understands is refused with a
clear message, not half-read. Writes are atomic (write a temporary file in the same folder,
then rename).

### Plain-text song format (slice 1 input, kept as an import format)

```text
[Verse 1]
Light rises over the quiet hills
And the morning sings again
--
Свет встаёт над тихими холмами
И утро снова поёт

[Chorus]
...
```

- A line in square brackets starts a group with that name.
- Blank lines separate slides.
- Inside a slide, a line containing only `--` separates the `primary` box from the
  `translation` box. A slide without it has only a `primary` box.
- Lines before the first group header belong to an unnamed group.

## 8. Build order

Each slice is usable by itself. Names in brackets say which later crate it adds. Slices 1 and
2 are built together as the first runnable build (section 9).

1. **Bilingual lyrics on an output.** Detailed in section 9.
2. **Slide grid.** An operator window with a clickable grid of slide thumbnails drawn by
   `pc-render`, grouped and coloured by group; click or arrow keys to go live. Brings in egui.
3. **Libraries and playlists.** Our file format on disk, several libraries, playlists with
   headers, autosave, restore after a crash.
4. **Import from ProPresenter.** [`pc-import`] Read the owner's libraries through the public
   API into our format, splitting text boxes on the carriage return.
5. **Themes and text editing.** Edit a slide's text and a theme's box styles in the app.
6. **Backgrounds and video.** [`pc-media`] The media layer: images, then video through FFmpeg,
   under the text.
7. **Stage screen.** A second screen with its own layouts: current and next slide, clock,
   timers, a stage message.
8. **NDI output.** [`pc-ndi`] Send a screen as an NDI stream (the owner's stage screen).
9. **Remaining layers and Looks.** Messages, props, announcements, masks, per-screen layer
   switches.
10. **Scripture.** A Bible panel with stacked translations routed to text boxes by role.
11. **Control from outside.** [`pc-api`] An HTTP API, a command-line client and an MCP server
    over the same engine commands, with the HTTP API compatible with the common remote-control
    clients as scoped in `03-api-and-protocols.md`. The owner has made this a headline goal, so
    it may be pulled forward to follow slice 3; it needs nothing from the media slices.

Risks, in the order they bite: a real second display (slice 1, untested so far), video decode
and sync (slice 6), NDI's library and licence terms (slice 8).

## 9. First build: home window, slide grid and output (slices 1 and 2)

### What the owner can do when it is done

Run `presentercraft path/to/song.txt` (or several files, or a folder of them). Two windows
open:

```text
 Home window (on the main display)                       Output (fullscreen, other display)
+--------------------------------------------------+    +-----------------------------+
| Songs        | Song title                | LIVE  |    |                             |
| > Song one   | +-----+ +-----+ +-----+   |+-----+|    |  Light rises over the       |
|   Song two   | |  1  | |  2  | |  3  |   || out ||    |  quiet hills                |
|   Song three | +-----+ +-----+ +-----+   |+-----+|    |                             |
|              | Verse 1  Verse 1 Chorus   | Clear |    |  Свет встаёт над тихими     |
|              | +-----+ +-----+           | slide |    |  холмами                    |
|              | |  4  | |  5  |           |       |    |                             |
|              | +-----+ +-----+           | Output|    +-----------------------------+
|              |                thumb size | on/off|
+--------------------------------------------------+
```

- **Left:** the list of loaded songs. Click one to show its slides.
- **Centre:** the selected song's slides as numbered thumbnails, drawn by the same renderer as
  the output, each with its group name and a group colour bar. The live slide is outlined. A
  slider changes thumbnail size.
- **Right:** a live preview of the audience screen, a "Clear slide" button, and a switch that
  turns the output on and off.
- Click a thumbnail: that slide goes live.
- Right arrow or Space: next slide. Left arrow: previous slide. Home / End: first / last.
- C or the button: clear the slide layer (output goes black); choosing a slide shows it again.
- Closing the home window quits, after closing the output.
- The output shows nothing but the slide: no cursor, no window frame.
- The home window's look is our own; it is not modelled on any other product's interface.

### Behaviour

- **Which display:** with two or more displays, the output goes fullscreen on the first
  display that is not the one holding the operator window. `--output-display N` chooses one
  explicitly; `--list-displays` prints them. With one display, the output opens as an ordinary
  resizable window so the app is still usable for testing.
- **Scaling:** the slide is authored at 1920 x 1080 and scaled to fit the output, centred,
  with black bars if the shape differs.
- **Theme:** one built-in theme. `primary`: upper box, bold, white, large. `translation`:
  lower box, bold, orange, smaller. Both centred, with black outline and soft shadow. Long
  lines wrap inside their box. Text that still does not fit is shrunk to fit its box, down to
  a floor size.
- **Font:** a system font that covers Latin and Cyrillic (Helvetica Neue on macOS). If a
  character has no glyph in it, the layout falls back to another system font.
- **Start state:** the first song is selected and nothing is live until the operator chooses a
  slide, so opening the app never puts unexpected words on the screen.
- **Bad input:** a missing or unreadable file, or a file with no slides, is reported in a plain
  message and skipped. If nothing could be loaded, the program ends with a non-zero exit code
  and no window opens. With no arguments, it loads the bundled sample song.
- **Display unplugged while running:** the output window moves to a remaining display or
  becomes an ordinary window; the app does not crash.

### How the pieces are used

- `pc-model`: slide, text box, group, presentation, theme types; the plain-text parser.
- `pc-engine`: show state with the layer stack; commands `GoTo`, `Next`, `Previous`, `First`,
  `Last`, `ClearSlide`; reports what changed.
- `pc-render`: scene for (slide, theme, target size); draw to texture; read pixels back (used
  by the image tests, later by NDI).
- `pc-app`: argument parsing, display choice, the home window (egui), the output thread and
  its windows, key handling.

### Tests that must pass

- Parser: groups, blank-line slide breaks, `--` box split, a slide with one box, text before
  any header, Windows line endings, a trailing newline, an empty file.
- Engine: go to a slide; next and previous stop at the ends; next with nothing live goes to
  the first slide; clear, then next, continues from where it was; first and last; switching
  songs does not change what is live.
- Renderer: an image test for a bilingual slide (English + Russian + the Ukrainian letters
  ї ґ є), one for a long line that wraps, one for text that must shrink to fit, and one for
  letterboxing at a 4:3 target.
- App: display choice logic, tested as a pure function over a list of displays.

A person checks by eye, once, on a real second display.

## 10. Legal guardrails

These apply to every slice.

- ProPresenter is the owner's licensed copy and is used only as any user would: looking at it,
  and calling its documented public API.
- No decompiling, no reading its binary, and no use of the community's extracted schemas of
  its file format. The research found that every public schema of the `.pro` format comes from
  the app's binary, and that the vendor's terms forbid reverse engineering.
- No ProPresenter names, logos, icons, artwork, sample content or interface wording in the
  product. Generic terms of the trade (slide, playlist, library, stage display) are fine.
- Screenshots of ProPresenter stay out of the repository (`docs/screenshots/` is ignored by
  git).
- Song lyrics and Bible translations are their publishers' property. Tests use text written
  for the purpose.
- NDI and FFmpeg licence terms are read and recorded in the slice that adds them, before any
  code that links them.
- This is not legal advice; anything to be sold or published should get a lawyer's review.

## 11. Left to the builder

- Module layout inside each crate, error types, and logging.
- The exact numbers in the built-in theme (box positions, sizes, outline width), tuned by eye
  against the spike's image.
- How scenes are cached between frames.
- How thumbnail textures are cached and when they are redrawn.

## 12. Open questions for the owner

None of these block slice 1.

- What licence should PresenterCraft itself carry (private, or open source and which)?
- What is "Translation on Screen" in the Sunday playlist, and what is "Live" set to do?
- The 46 screenshots look like they came from a default workspace; the real Screens, Looks
  and stage layout windows from the church workspace are still worth capturing before slices
  7 to 9.
