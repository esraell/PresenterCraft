# Controlling PresenterCraft from the command line

While PresenterCraft runs, `presentercraft ctl COMMAND …` controls it from a terminal, a script
or an AI agent. Every command is carried out by the same code as the window's own buttons, so
what the command line does and what a click does are always the same.

Command names are a first version and may still change before the first release.

## Using it

The `presentercraft` program is the app itself. In a built Mac app it is
`PresenterCraft.app/Contents/MacOS/PresenterCraft`; from the source, `cargo run --release -- ctl …`.

    presentercraft ctl status
    presentercraft ctl go "Amazing Grace" 3
    presentercraft ctl next
    presentercraft ctl clear all
    presentercraft ctl status --json

- **Names** can be typed in full or in part, in any case: `go amazing` finds "Amazing Grace".
  A name that fits several things is never guessed: the reply lists them, and nothing changes.
- **Numbers** are as the lists show them, from 1 (`ctl songs`, `ctl slides SONG`).
- **Names with spaces** can be quoted (`"Amazing Grace"`) or typed as separate words.
- **`--json`** prints the whole reply as JSON (see below), for scripts and agents.
- **Exit code** is 0 when the command was done, 1 when it was refused (an unknown name, a
  missing `--yes`, the app not running).

## Commands

### What is going on

| Command | Does |
|---|---|
| `status` | What is on screen (song, slide, its words), the next slide, the media, any message and props, whether the audience and stage screens are on, the song shown in the window |
| `songs` | Every song in the library, numbered |
| `slides [SONG]` | A song's slides (the one shown in the window if none is named): number, group, words, and which is on screen |
| `playlists` | The playlists |
| `playlist [NAME]` | A playlist's items (the one in front if none is named) |
| `media` | The media bin's items |
| `themes`, `screens`, `timers`, `messages`, `props`, `macros` | Each list, numbered |
| `audio` | Every audio track |
| `menu` | Every menu bar item: its id and the words on it |

### Running the show

| Command | Does |
|---|---|
| `next`, `previous` (`prev`), `first`, `last` | As the arrow keys, Home and End do (Next goes on into the next song with Continuous Playlist on) |
| `go SONG [SLIDE]` | Puts a slide on screen (the song's first slide if SLIDE is left out) |
| `select SONG` | Shows a song's slides in the window without putting anything on screen |
| `clear [all\|slide\|media\|props\|messages\|audio\|announcements]` | Clears a layer, or everything (the default) |
| `audience on\|off` | The audience screens |
| `stage on\|off` | The stage screens |
| `identify on\|off` | Identify Screens |
| `view show\|edit\|bible\|themes` | Switches the window's view |

### Layers and controls

| Command | Does |
|---|---|
| `media show ITEM` | Puts a media item behind the words |
| `media add FILE…` | Adds pictures or videos to the media bin, as dropping them on the window does |
| `media delete ITEM --yes` | Takes an item out of the media bin (the file stays where it is) |
| `media-playlist play NAME`, `media-playlist stop` | Plays a media playlist item after item |
| `message show NAME`, `message hide` | A message over the slides |
| `prop show NAME`, `prop hide NAME` | A prop |
| `timer start\|stop\|reset NAME` | A timer |
| `macro run NAME` | Runs a macro's actions in turn |
| `announce SONG`, `announce stop` | Plays a song's slides on the Announcements layer |
| `audio play TRACK`, `audio pause`, `audio stop`, `audio volume 0-100` | The audio player |
| `bible REFERENCE [--show]` | Looks a passage up on the Bible page, and with `--show` puts its first verse on screen |

### Songs and playlists

| Command | Does |
|---|---|
| `song new TITLE [FILE.txt]` | A new song, its words from a song text file (a line in square brackets starts a group, a blank line a new slide) |
| `song delete SONG --yes` | Deletes a song (one made in PresenterCraft goes to the Trash) |
| `playlist open NAME` | Puts a playlist in front |
| `playlist add SONG [to PLAYLIST]` | Adds a song to a playlist (the one in front if none is named) |

### Everything on the menu bar

`menu ITEM` chooses a menu bar item, by its id (`ClearAll`, `NewSong`, `IdentifyScreens`) or by
the words on it ("Clear All"). `menu` lists them all. Menu items that open a window or a file
chooser open it on the Mac, as a click does.

Not yet on the command line (they need the window for now): the slide and theme editors'
object tools, the Screens and Settings windows' individual settings, stage layouts, and the
inspector. Until they have their own commands, the menu bar items reach some of them.

## Replies as JSON

With `--json` the reply is:

```json
{
  "ok": true,
  "text": "On screen: Sample song, slide 2 of 8 (Chorus).",
  "data": { "...": "the same as data, for the commands that have any" }
}
```

`status` gives `live` (`song`, `slide`, `slides`, `words`), `next_slide`, `media`,
`message`, `props`, `audience_on`, `stage_on` and `selected_song`. The lists give an array of
names in their numbered order.

## How it works, and who can use it

- When the app starts it listens on a socket file, `control/ctl.sock` in its data folder
  (`~/Library/Application Support/PresenterCraft/control/` on a Mac). The folder can be entered
  only by the user the app runs as, and the socket can be opened only by that user.
- Nothing listens on the network: no other computer, and no web page in a browser, can reach
  it. A program running as the same user can, which is no more than that program could do by
  changing the app's files.
- A request is one line of JSON, `{"v": 1, "args": ["go", "Amazing Grace", "3"]}`, at most
  64 KB; the reply is one line of JSON. `v` is the protocol's version: a command line from a
  different build is told so.
- A second copy of the app leaves the first one's socket alone and runs without one.
- The socket is removed when the app quits. With the app not running, `ctl` says so.
- Commands that delete something can't be undone, so they need `--yes`.
- Each command is logged in the app's log (`~/Library/Logs/PresenterCraft/PresenterCraft.log`).

For tests, `PRESENTERCRAFT_CONTROL_SOCKET=/short/path/ctl.sock` puts the socket elsewhere (a
socket's path must be short, about 100 bytes), and `PRESENTERCRAFT_DATA` points the app and the
command line at another data folder, so a test never reaches the copy in use.

## Still to come

An API over the network (HTTP and WebSocket, with access tokens, switched on in Settings), a
stream of changes as they happen, and an MCP server so AI agents can use these commands as
tools. All of them will use the same commands as the command line.
