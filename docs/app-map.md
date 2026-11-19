# A map of SoftPresenter

What the app looks like, what every part of it does, which command-line command does the same,
where it keeps its files, and how the code is laid out. Written for people and AI agents
finding their way around. For the command line itself see [cli.md](cli.md).

## The main window

One window, with four views chosen at its top left. The operator's window never shows on the
audience screens; the screens are separate windows (or NDI sources) drawn by their own thread.

```
┌────────────────────────────────────────────────────────────────────────────────────────────┐
│ [Show | Edit | Bible | Themes]                              [● Audience ON] [● Stage ON] [⚙]│
├──────────────────┬─────────────────────────────────────────┬────────────────────────────────┤
│ LIBRARY        + │ Song title   8 slides     (i) ≡ ∞ ▤      │ ● PROGRAM  Audience ▾  ON SCREEN│
│   All Songs      │ ┌──────┐ ┌──────┐ ┌──────┐                │ ┌────────────────────────────┐ │
│   Christmas      │ │slide │ │slide │ │slide │   slide grid   │ │  what the screens show     │ │
│ PLAYLISTS      + │ │1 V1 A│ │2     │ │3 Ch C│   (or Easy /   │ └────────────────────────────┘ │
│   Sunday         │ └──────┘ └──────┘ └──────┘   Table view)  │ [        Clear All      esc ] │
│ ──────────────── │                                          │ [Clear Slide] [Clear Media]   │
│ ALL SONGS    3 + │                                          │ [Props] [Messages] [Audio]    │
│   Song one       │ [+ Add Slide]   Cut Dissolve Advanced ▾  │ [Previous]  3 / 8  [Next]     │
│   Song two       │                 grid/easy/table  size ── │ Timers Messages Props Macros  │
│                  ├─────────────────────────────────────────┤ Stage Audio  (tabs)           │
│ [Search songs]   │ ▾ MEDIA  All Media ▾   Dissolve ▾   [+]  │   …the open tab's list…       │
│                  │ [pic] [pic] [video ▶]                    │                               │
└──────────────────┴─────────────────────────────────────────┴────────────────────────────────┘
```

### Top bar

| Part | Does | Command line |
|---|---|---|
| Show / Edit / Bible / Themes | Switches the view | `view show\|edit\|bible\|themes` |
| Audience ON/OFF | The audience screens on or off (Command-1) | `audience on\|off` |
| Stage ON/OFF | The stage screens on or off (Command-2) | `stage on\|off` |
| ⚙ | Settings: General, Screens, Groups, Key Mappings (Command-comma) | `menu Settings` |

### Left column: library and playlists

| Part | Does | Command line |
|---|---|---|
| LIBRARY rows | All Songs, and libraries of your own; + makes one; right-click to rename or delete | `songs`, `libraries`, `library open NAME` (or `all`) |
| PLAYLISTS rows | Playlists; + makes one; drop songs on a row to add them | `playlists`, `playlist open NAME` |
| Song list (a library chosen) | Click opens a song; Command/Shift-click and dragging pick several; drag picked songs onto a playlist or library; Delete asks, then deletes; right-click for Duplicate, Theme, Transition, Reflow, Play on Announcements Layer, Delete… | `select SONG`, `song delete SONG --yes`, `announce SONG` |
| Playlist items (a playlist chosen) | Songs, Bible passages and headers in running order; drag to reorder; + adds a header | `playlist [NAME]`, `playlist add SONG to PLAYLIST` |
| Search | Filters the list; Command-F searches titles and words | |

### Middle: the slides

| Part | Does | Command line |
|---|---|---|
| Song header | Title (double-click renames), slide count, and four icons: Info (writer, CCLI), Reflow (edit the words as text), Arrangement (groups in another order), Notes (operator notes) | `slides SONG` |
| Slide grid | Click puts a slide on screen; right-click picks it out and opens its menu (Edit, Cut, Copy, Paste, Delete…, Group, Media, Transition, Actions, Go to Next, Hot Key, Copy/Paste Text Style, Label, Disable, Template); Command/Shift-click and dragging a box pick several without going live; Delete asks, then deletes | `go SONG SLIDE`, `next`, `previous`, `first`, `last` |
| Playlist view | A song opened from a playlist shows the playlist's songs one after another, each under a bar with its name | |
| Bottom bar | + Add Slide; the transition for all slides (Cut, Dissolve, Advanced ▾ with 19 kinds); grid, easy (words only) or table view; slide size | `menu NewSlide` |
| Media bin | Pictures and videos; click shows one behind the words; drag onto a slide (its media) or under the slides (a slide of its own); Command/Shift-click or a box picks several; Delete asks; right-click for Scaling, Loop, playlists, Delete…; MEDIA ▾ picks a media playlist (Play This Playlist, Follow a Folder…); the chevron folds the bin away | `media`, `media show ITEM`, `media add FILE…`, `media delete ITEM --yes`, `media-playlist play NAME` |

### Right column: what is on screen

| Part | Does | Command line |
|---|---|---|
| PROGRAM preview | What the audience screens show, drawn by the screens' own renderer (videos move in it); the chooser shows the stage picture instead | `status` |
| Clear All, Clear Slide, Clear Media, Props, Messages, Audio | Clear a layer (Escape clears all; F1 to F6 clear one each); Clear Announcements when that layer plays | `clear all\|slide\|media\|props\|messages\|audio\|announcements` |
| Previous / Next | Step through the slides | `previous`, `next` |
| Video controls | Under the preview while a video plays: pause, seek, loop, mute | |
| Tabs: Timers | Count down, count up, to a time of day; start, stop, reset | `timers`, `timer start\|stop\|reset NAME` |
| Tabs: Messages | Messages with fill-in tokens (the time, a timer), shown over the slides | `messages`, `message show NAME`, `message hide` |
| Tabs: Props | Slides of their own laid over the show (logos, lower thirds) | `props`, `prop show\|hide NAME` |
| Tabs: Macros | Lists of actions done in turn | `macros`, `macro run NAME` |
| Tabs: Stage | Each stage screen's layout, and the stage message | `stage on\|off` |
| Tabs: Audio | Audio playlists: play, pause, skip, volume, shuffle, crossfade | `audio`, `audio play\|pause\|stop`, `audio volume N` |

### Keys in the Show view

| Key | Does |
|---|---|
| Space, → | Next slide |
| ← | Previous slide |
| Home, End | First, last slide of the song |
| Escape (F1) | Clear all |
| F2, F3, F4, F5, F6 | Clear slide, media, props, audio, messages |
| A letter | The slide with that hot key, or the group it stands for (A for the first verse, S for the second, C for the chorus, E for the ending…; the letter shows at the right of a group's bar) |
| Delete | Delete what is picked out, after asking |
| Command-N | New song |
| File › Import Songs… (or drop song files on the window) | Import songs from text, lyric, ChordPro, rich-text or Word files |
| File › Import from ProPresenter… (or drop a folder) | Bring a library folder across; each folder becomes a library |
| Command-L | Back to the song on screen |

Every menu bar item, with its shortcut, is listed by `softpresenter ctl menu`; any of them can be
chosen with `softpresenter ctl menu ITEM`.

## The Edit view (and the theme editor)

```
┌──────────────────┬─────────────────────────────────────────────┬───────────────────────────┐
│ Song title       │ ruler ─────────────────────────────────────  │ [Shape] [Text] [Build]   │
│ ┌──────────────┐ │        [Add Text] [Shapes ▾] [Media…]        │  (the open tab is light) │
│ │ slide 1      │ │ ┌───────────────────────────────────────┐    │  font, size, colour,     │
│ ├──────────────┤ │ │                                       │    │  outline, shadow, fill,  │
│ │ slide 2      │ │ │         the slide, full size          │    │  gradient, spacing,      │
│ └──────────────┘ │ │   drag to move, handles to resize     │    │  linked text, builds…    │
│ Objects          │ └───────────────────────────────────────┘    │                          │
│   T Main         │ X  Y  W  H                     Grid   Fit    │ [Slide Notes] [Live Edit]│
│   ▢ photo        │                                              │                   [Show] │
└──────────────────┴──────────────────────────────────────────────┴───────────────────────────┘
```

- **Left**: the song's slides (click to edit another), then the slide's objects, front first
  (lock, hide, rename, drag to reorder).
- **Canvas**: Add Text, Shapes, Media… (a picture or a video, which plays in its box when the
  slide is on screen); pictures and videos can be dropped on it from the Finder. Double-click a
  text box to type in it. The letter keys add objects (T for a text box, one key per shape).
- **Inspector**: Shape (place, fill, outline, shadow, flip), Text (words, font, size, colour,
  alignment, spacing, outline, shadow, gradient, linked text such as a clock or a timer), Build
  (how an object comes on and goes off).
- **Footer**: Slide Notes, Live Edit (changes reach the screens as they are made), Show.
- **Theme editor** (the Themes view): the same canvas, tools and inspector, editing a theme's
  layouts (templates). A layout's text boxes say where each kind of words goes; its shapes,
  pictures and videos are drawn behind the words on every slide that uses it. The list on the
  left holds the themes (tagged Songs and Bible by what uses them) and the layouts.

## The Bible view

```
┌────────────────────────────────────────────────────────────────────────────────────────┐
│ [John 3:16-18      ] [Books]  [Search          ]   [Add to "Sunday" ] [▾]   [Options] │
│ [King James ▾]  Verse [Main ▾]   Reference [Reference ▾]                 License  [+] │
│ [Synodal    ▾]  Verse [Translation ▾] Reference [None ▾]                 License  [−] │
├────────────────────────────────────────────────────────────────────────────────────────┤
│  verses as slides; click one to put it on screen                                       │
├────────────────────────────────────────────────────────────────────────────────────────┤
│ [‹ Verse] [Verse ›]                    Cut Dissolve Advanced ▾    grid/easy/table  size │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

- Up to four Bibles side by side; each row picks a Bible (None frees it for another row), the
  box its verse goes in, and the box its reference goes in.
- Options: how verses become slides, the theme and layout for Bible slides, and the Bibles
  installed (more can be downloaded).
- Add to "…" keeps the passage in a playlist, to open again with these settings.
- Command line: `bible REFERENCE [--show]`.

## Other windows

| Window | Opened from | Holds |
|---|---|---|
| Screens | Settings, Screens; or Screens, Configure Screens… | Audience and stage screens: where each goes (a display, a window, NDI, off), its name, its look (which layers, colour, covered edges); Identify |
| Settings | ⚙ or Command-comma | General (copyright line, start-up choices), Screens, Groups (names and colours), Key Mappings |
| Stage Layouts | Screens, Edit Stage Layouts… | Layouts for the stage screens: now, next, notes, clock, timer, message |

## Where the app keeps things

On a Mac, in `~/Library/Application Support/SoftPresenter/` (or the folder
`SOFTPRESENTER_DATA` names):

| File or folder | Holds |
|---|---|
| `Songs/*.sppres` | Songs made or changed in the app (JSON) |
| `Themes/*.sptheme` | Themes (JSON); `themes-in-use.txt` says which theme songs and Bible slides use |
| `Images/` | Pictures put on slides |
| `services.json` | Playlists |
| `media.json` | The media bin and media playlists |
| `timers.json`, `messages.json`, `macros.json` | Timers, messages and macros |
| `screens.json` | The screens and their looks |
| `preferences.json`, `bible-page.json` | Small choices, and how the Bible page was left |
| `control/ctl.sock` | The command line's socket, while the app runs |

The log is `~/Library/Logs/SoftPresenter/SoftPresenter.log`: what went on screen, commands,
problems, and once a minute how long drawing took.

## How the code is laid out

| Crate | Holds |
|---|---|
| `sp-model` | The data: songs (presentations, groups, slides, objects), themes and layouts, transitions, builds, actions |
| `sp-engine` | The show's state: what is selected, what is on screen, next and previous, clearing layers |
| `sp-render` | Drawing slides on the GPU with vello: screens, thumbnails, transitions |
| `sp-video` | Playing videos: macOS's own player, FFmpeg for other formats |
| `sp-ndi` | Sending screens over NDI (the runtime is opened when an NDI screen starts) |
| `sp-bible` | Bibles: references, verse numbering, search |
| `sp-app` | The app: the window (egui), the screens' thread (`output.rs`), the views (`home.rs`, `studio.rs`, `editor.rs`, `inspector.rs`), the command line (`control.rs`), the menu bar (`menubar.rs`) |

## Trying the app safely (for agents and tests)

- Every test run sets `SOFTPRESENTER_QUIET=1`: its windows open behind everything, its screens
  never go full screen, and it sends no NDI. Point it at a copy of the data with
  `SOFTPRESENTER_DATA`, and for the command line set `SOFTPRESENTER_CONTROL_SOCKET` to a short
  path, so nothing reaches the copy someone is using.
- `softpresenter --capture-home FILE.png` saves a picture of the window and quits;
  `SOFTPRESENTER_CAPTURE_AFTER=SECONDS` waits before taking it, and
  `SOFTPRESENTER_CAPTURE_OUTPUT=FILE.png` saves the first screen's picture.
- Testing aids (environment variables) open a view or do something at start:
  `SOFTPRESENTER_GO_LIVE=1` (a slide on screen), `_GO_LIVE_MEDIA=1` (the first media item),
  `_CAPTURE_EDITOR`, `_CAPTURE_BIBLE`, `_CAPTURE_THEMES`, `_CAPTURE_SETTINGS`,
  `_PLAYLIST`, `_OPEN_ITEM=N`, `_MENU=ItemId,ItemId`, `_DROP=FILE`, `_PICK=songs:0,2`,
  `_WINDOW_SIZE=WxH`, `_NDI=NAME`, `_HIDE_NDI=1`, `_TRACE_PREVIEW=1`.
- The command line drives a quiet copy as it would the real one:
  `SOFTPRESENTER_DATA=… SOFTPRESENTER_CONTROL_SOCKET=… softpresenter ctl status`.
