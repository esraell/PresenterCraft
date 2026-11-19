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

## Song files

Plain text, UTF-8:

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
