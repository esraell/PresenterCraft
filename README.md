# PresenterCraft

A presentation app for church services, written in Rust: song lyrics in two languages on an
audience screen, run from one home window. Early proof of concept.

## Run

    cargo run --release                       # the bundled sample song
    cargo run --release -- songs/             # every .txt file in a folder
    cargo run --release -- one.txt two.txt

Two windows open: the home window, and the output. With a second display connected the output
goes fullscreen on it; with one display it opens as an ordinary window.

    --list-displays        print the connected displays
    --output-display N     put the output on display N

## Keys

| Key | Does |
|---|---|
| Right arrow, Space | next slide |
| Left arrow | previous slide |
| Home / End | first / last slide of the selected song |
| C | clear the slide (output goes black) |

Click a thumbnail to send that slide to the output.

## Media

Drop pictures (PNG, JPEG, WebP, BMP, GIF) onto the window, or name them on the command line,
and they appear in the media bin along the bottom. Click one to show it behind the words. The
picture button beside the preview takes it off again. Videos are not supported yet.

## Editing

Press Edit in the toolbar, or right-click a slide and choose "Edit words…". The window changes
to the Edit view: the song's slides down the left with the chosen slide's objects under them, a
canvas in the middle, and an inspector on the right.

- Click an object on the canvas (or in the Objects list) to select it. Drag it to move it, drag
  a handle to resize it, use the arrow keys to nudge it (Shift for bigger steps), press Delete
  to remove it.
- The inspector changes the selected object's name, words, position, size, rotation and
  opacity, moves it forward or back, and puts it back where the theme has it.
- "Add text" (or the T above the canvas) adds a text box.
- Cmd+Z undoes, Shift+Cmd+Z redoes.

Changes are saved a second after you stop, and when you press Show to go back, as a
PresenterCraft song file (`.pcpres`) beside the song's text file; the text file is left as it
was. What is on the audience screen does not change while you edit unless Live Edit is ticked.
The bundled sample is not a file, so edits to it last until the app closes.

## Song files

A song is either a plain text file or a PresenterCraft song file (`.pcpres`, a JSON document
that also holds where each object sits and how it looks). Where both exist with the same name,
the PresenterCraft file is the one opened. The text format, UTF-8:

    [Verse 1]
    Light rises over the quiet hills
    And the morning sings again
    --
    Свет встаёт над тихими холмами
    И утро снова поёт

    [Chorus]
    ...

- `[Name]` starts a group.
- A blank line starts a new slide.
- A line with only `--` separates the main language from its translation.

## Layout

    crates/pc-model    slides, groups, themes, the song text parser
    crates/pc-engine   what is live and the commands that change it (no windows)
    crates/pc-render   draws a slide on the GPU: output, thumbnails, preview
    crates/pc-app      the windows

Design: `docs/superpowers/specs/2026-11-18-presentercraft-foundation-design.md`.
Research behind it: `docs/research/00-synthesis.md`.

## Check

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    cargo test --workspace

The picture tests in `pc-render` need a GPU and the macOS system font.
