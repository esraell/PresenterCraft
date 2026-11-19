# SoftPresenter

**Free, open-source software for putting songs, Bible verses and media on the screens in a church
service, made for services in two languages.**

[![Licence: GPL-3.0-or-later](https://img.shields.io/badge/licence-GPL--3.0--or--later-blue)](LICENSE)
[![Checks](https://github.com/esraell/SoftPresenter/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/esraell/SoftPresenter/actions/workflows/ci.yml)
![macOS now; Windows and Linux planned](https://img.shields.io/badge/platform-macOS-lightgrey)

![The Show view: the Sunday Morning playlist with its songs one after another in English and Russian, a slide on screen over a background picture, two timers and the media bin](docs/images/home.png)

> **Early days.** SoftPresenter is young. Some buttons do less than they look like they should,
> and file formats may still change, so it isn't ready to run a Sunday service on its own yet.
> It is built in the open; bug reports and ideas are welcome in
> [Issues](https://github.com/esraell/SoftPresenter/issues).

## Why SoftPresenter

Churches put words on screens every week: songs, readings, notices. The apps that do it well cost
a lot, are heavy on older computers, and treat a service in two languages as an afterthought.
SoftPresenter is built to be:

- **Free and open.** No licence fee, no account, no subscription. Everything it needs, the Bibles
  it offers included, is hosted with this repository.
- **Made for two languages.** Every slide can carry a main language and a translation, for
  churches that sing in Russian or Ukrainian and English side by side.
- **Right about the Bible.** Translations number some verses differently (the Russian Psalms are
  the best-known case). SoftPresenter lines up the same verse across translations instead of
  pairing chapter and verse numbers blindly.
- **Fast and light.** Slides are drawn on the GPU, the audience screen never waits for the
  operator's window, and it is built to run well on a modest laptop.
- **Familiar.** Laid out the way people who run church screens expect: libraries and playlists
  on the left, slides in the middle, what is on screen on the right, media along the bottom.
- **Open to scripts and AI.** Most of what the operator does during a service, a script or an AI
  agent can do from the command line.

## Who it's for

- **A Sunday service** of songs, readings, notices and pictures, run by one volunteer with the
  keyboard.
- **Congregations in two languages:** both languages on every song slide, and up to four Bible
  translations on one slide, lined up verse by verse.
- **Worship teams and livestreams:** stage screens with the words now, the next slide, the clock
  and a timer; any screen sent over NDI to streaming software or a video switcher.
- **Lobbies and automation:** loops of pictures and videos before and after the service, and a
  command line for scripts, button panels and AI agents.

## What it does

- **Songs and slides.** Songs as slides in groups (Verse, Chorus, Bridge), arrangements, one-key
  jumps, nineteen transitions, libraries of your own, and playlists with headers, shown as one
  scroll from song to song. Undo and redo.
- **Bible.** Look up `John 3:16-18` or `Пс 22:1-3`, search by word, up to four translations on
  one slide; four Bibles come with the app and fifteen more download from inside it.
- **Media.** Pictures and videos behind the words or on their own, media playlists, folders the
  app watches, and an announcements layer that loops by itself. Videos play as they are, never
  converted.
- **Screens.** Any number of audience screens on displays, in windows or over NDI; stage screens
  with layouts you design; a look for each screen (a lobby can show only the media); test
  patterns and Identify Screens.
- **Live tools.** Timers, messages over the slides, props such as a logo, macros that do several
  things in one click, walk-in music with crossfades, and a message for the stage.
- **Editing and themes.** A slide editor with text, shapes, pictures, videos, gradients and
  builds; themes that decide where each kind of text goes, used across songs and Bible slides.
- **Command line.** `softpresenter ctl` controls the running app: slides, clears, media,
  messages, timers, macros, the Bible and every menu item. Install it from SoftPresenter >
  Install Command Line Tool…; [docs/cli.md](docs/cli.md) has the commands.

The full list, area by area, is in [docs/features.md](docs/features.md).

<table>
  <tr>
    <td width="50%"><img src="docs/images/bible.png" alt="The Bible view: John 3:16-18 in the King James Version beside the Russian Synodal, one slide per verse"></td>
    <td width="50%"><img src="docs/images/stage.png" alt="A stage screen: the slide on screen in English and Russian, the next slide below it, a sermon timer and the clock"></td>
  </tr>
  <tr>
    <td align="center">The Bible in two translations, lined up verse by verse</td>
    <td align="center">A stage screen as the band sees it</td>
  </tr>
  <tr>
    <td width="50%"><img src="docs/images/editor.png" alt="The editor: a slide in two languages with its English box selected and the inspector's Text tab open"></td>
    <td width="50%"><img src="docs/images/screens.png" alt="The Screens window: an audience screen, an NDI stream and a stage screen, with the selected screen's output and look"></td>
  </tr>
  <tr>
    <td align="center">The slide editor</td>
    <td align="center">Audience, NDI and stage screens</td>
  </tr>
</table>

![The media bin: three pictures and a video, the picture on screen outlined in red](docs/images/media.png)

## A service from start to finish

1. **Set up the screens.** Open the Screens window (Option-Command-1). Put an audience screen on
   the projector or TV, and a stage screen on a second display or over NDI. Identify Screens
   shows each screen's name on it, so you can check which is which.
2. **Bring in the songs.** New Song (Command-N) asks for a title and a theme and takes the words
   pasted in: a line in square brackets starts a group, a blank line starts a new slide, and a
   line with only `--` separates the main language from the translation. Song text files can
   also be opened from the command line (see [Get started](#get-started) and
   [the format](#song-text-files)).
3. **Build the running order.** Make a playlist with the + beside PLAYLISTS (say "Sunday
   Morning"), drag songs into it in order, and add headers to group them.
4. **Add the readings.** In the Bible view (Control-B), type a reference such as `John 3:16-18`,
   pick a second translation for the next row, and add the passage to the playlist with one
   click.
5. **Add pictures and videos.** Drop them on the window to put them in the media bin, and group
   them in a media playlist (the menu beside MEDIA). For a loop before the service, choose Play
   This Playlist, or right-click a song and choose Play on Announcements Layer. To start it every
   time the app opens, set it under Settings, Screens, When SoftPresenter Opens.
6. **Choose what the stage sees.** In the Stage tab, give each stage screen a layout: Words and
   Next, Words Only, Words and Notes, Clock and Timer, or one you design.
7. **Run it.** In the Show view (Control-S), open the playlist and click the first slide, or press
   Space. The right arrow and Space go forward, the left arrow goes back, and letters jump to a
   group (A for Verse 1, C for the chorus). With View, Continuous Playlist on, the last slide of
   a song leads into the next song. Escape clears everything, walk-in music and announcements
   included; F2 to F6 clear one layer at a time. Timers, messages and macros sit in the tabs
   under the preview.
8. **Afterwards.** There is nothing to save: every change is written as you make it, and the
   playlist is ready for next week.

## Get started

There is no download yet: SoftPresenter is built from source, on a Mac with macOS 13 or later
(Windows and Linux are planned). You need:

- **The Xcode Command Line Tools:** `xcode-select --install`.
- **Rust through [rustup](https://rustup.rs).** `rust-toolchain.toml` pins the version
  (1.100.0), and rustup installs it by itself.

Then:

    git clone https://github.com/esraell/SoftPresenter
    cd SoftPresenter
    cargo run --release

For a proper `SoftPresenter.app`, named correctly in the menu bar and the Dock:

    scripts/macos-app.sh
    open target/release/SoftPresenter.app

Two things are optional:

- **NDI Tools** (free, from [ndi.video](https://ndi.video)) to send a screen over NDI. Without
  it, NDI screens say NDI isn't installed and everything else works.
- **FFmpeg** for videos beyond MP4 and MOV (MKV, WebM, AVI and others). A release will include
  it.

Song text files can be opened from the command line, such as the sample:
`cargo run --release -- samples/sample-song.txt`. Other options: `--list-displays`,
`--output-display N`. From the source, the command line runs as `cargo run --release -- ctl
status`.

## Using it

### Keys

| Key | Does |
|---|---|
| Right arrow, Space | next slide |
| Left arrow | previous slide |
| Home / End | first / last slide of the song |
| Escape (or F1) | clear everything, audio and announcements included |
| F2 / F3 / F4 / F5 / F6 | clear the slide / the media / the props / the audio / the message |
| A, S, D, F… | jump to Verse 1, 2, 3, 4… |
| C, X, Z | jump to Chorus 1, 2, 3 |
| B, N, M | jump to Bridge 1, 2, 3 |
| P, T, I, E, O | Pre-chorus, Tag, Intro, Ending, Outro |
| Cmd+N | new song |
| Cmd+F | search the library (titles and words) |
| Cmd+, | Settings |
| Option+Cmd+1 | Screens |
| Cmd+1 / Cmd+2 | audience / stage screens on or off |
| Control+S, E, B, T | Show, Edit, Bible, Themes |

The letter for each group is shown on its first slide.

### Song text files

A song can be a plain UTF-8 text file:

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

Songs made or edited in the app are saved as `.sppres` files (JSON), which also hold where each
box sits and how it looks.

### Where things are kept

Everything lives in `~/Library/Application Support/SoftPresenter/`: songs made in the app
(`Songs/`), themes (`Themes/`), downloaded Bibles (`Bibles/`), playlists, screens and a few
preferences. The log is in `~/Library/Logs/SoftPresenter/`.

## Where it's heading

SoftPresenter aims to be the presentation app a church never has to pay for and never outgrows:
complete enough for a large Sunday service, simple enough for a volunteer, light enough for an
old laptop, and open to scripts and AI. First on the list: importing songs from files and from a
church's existing software, PowerPoint import, FFmpeg inside the app, and media and
placeholder items in playlists. After that come a network API with a phone remote and an MCP
server for AI assistants, and tested Windows and Linux builds.

The full plan, about 170 features with their size and how much each matters on a Sunday, is in
[docs/roadmap.md](docs/roadmap.md). Nothing in it is promised by a date.

## For developers and AI agents

- [docs/app-map.md](docs/app-map.md) maps every view, panel and window, what each part does, the
  command-line command that does the same, where the files are, and how to try the app safely.
- [docs/cli.md](docs/cli.md) is the command line's reference.
- [AGENTS.md](AGENTS.md) is for AI coding agents: running the app without disturbing anyone,
  the testing switches, and the rules.
- [CONTRIBUTING.md](CONTRIBUTING.md) covers sending a change and the commit standard.

The code is a Rust workspace:

    crates/sp-model    slides, groups, themes, the song text parser
    crates/sp-engine   what is live and the commands that change it (no windows)
    crates/sp-render   draws slides on the GPU: screens, thumbnails, preview
    crates/sp-bible    Bible files, references, search, verse numbering
    crates/sp-ndi      NDI output
    crates/sp-video    videos (macOS's AVFoundation, then FFmpeg) and system picture formats
    crates/sp-app      the app and its windows

[AGENTS.md](AGENTS.md#rules) says where `unsafe` code is allowed. Before sending a change:

    cargo fmt --all --check
    cargo clippy --workspace --all-targets -- -D warnings
    SOFTPRESENTER_QUIET=1 cargo test --workspace

The picture tests in `sp-render` need a GPU and the macOS system font.

## Contributing, security and conduct

Bug reports, ideas and Bible requests go in [Issues](https://github.com/esraell/SoftPresenter/issues/new/choose);
changes are welcome as pull requests, as [CONTRIBUTING.md](CONTRIBUTING.md) describes. Report
security problems privately, as [SECURITY.md](SECURITY.md) says. Everyone taking part follows
the [Code of Conduct](CODE_OF_CONDUCT.md).

## Licence

SoftPresenter is free software under the GNU General Public License, version 3 or (at your
option) any later version ([`LICENSE`](LICENSE)), with an additional permission for the NDI and
DeckLink libraries that people install themselves
([`ADDITIONAL-PERMISSION.md`](ADDITIONAL-PERMISSION.md)). Anyone may use it, change it and pass
it on; a version passed on stays under the same licence. [`NOTICE.md`](NOTICE.md) lists the
parts that are not SoftPresenter's own.

The Bibles keep their own terms ([docs/licences/bibles.md](docs/licences/bibles.md)); FFmpeg runs
as a separate program and NDI's runtime is opened at run time, never linked
([docs/licences/ffmpeg.md](docs/licences/ffmpeg.md), [docs/licences/ndi.md](docs/licences/ndi.md)).
NDI® is a registered trademark of Vizrt NDI AB.

SoftPresenter is not affiliated with or endorsed by Renewed Vision, the makers of ProPresenter.
