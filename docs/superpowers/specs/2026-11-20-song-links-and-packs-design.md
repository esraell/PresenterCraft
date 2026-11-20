# Song links, outside edits and song packs: design

Date: 2026-11-20. Status: proposed, for the owner's approval.

## Why

The owner wants three things:

1. **The library in ~/Documents/SoftPresenter** (already decided: everything moves, logs stay in
   `~/Library/Logs`).
2. **Songs prepared at home brought to the church computer** as one file: chosen songs, or a
   playlist with its songs, with what they need.
3. **A song file edited outside the app** (by a person, a script or an agent) **takes effect**
   when the song is next opened in SoftPresenter, and is never lost to the app saving over it.

Today none of the three can be done well, for one reason: playlists, libraries and slide actions
find songs by their **file's full path** (`/Users/…/Application Support/SoftPresenter/Songs/<id>`)
or by title. Paths change when the folder moves and mean nothing on another computer.

## What stays

- **One readable JSON file per song** (`Songs/<id>.sppres`), pretty-printed, with only what a
  song sets itself. Files are small (8 KB a song on average), easy to read and compare, and
  open to scripts and AI. A database or a binary format would give that up for nothing we need.
- Saving as now: a change is written a moment later, to a temporary file that is then moved into
  place.
- Group colours stay in the files (they can be set in Settings › Groups, so they can't be
  worked out from the name). The files are not rewritten to drop default values.

## 1. Songs are found by their id

Every song file holds an `id`, and a song in the library keeps it for good. From now on:

- **Playlists** (`services.json`) name a library song as `{"kind": "song", "id": "<uuid>",
  "title": "…"}`. The title is kept only so a missing song can be shown by name.
- **Libraries** (`libraries.json`) list song ids.
- **A slide's "go to song" action** names the song's id.
- **A song outside the library** (a text file opened from the command line or the Finder) is
  still named by its path, as now: it is the owner's own file, somewhere of their choosing.
- A playlist item, library entry or action whose song can't be found **is kept and shown as
  missing** (by its title), never dropped silently.

**When a song gets its id for good:** a song made in the app, imported, or copied in from a pack
is saved in the library with its id at once. A text file opened by path gets a new id each time
it is read until it is saved as a song file; adding such a song to a playlist saves it first.

**Two files with one id** (a song file copied in the Finder): on loading, the second, in name
order, is given a new id, written back to its file, and the log says so.

## 2. One migration, with the move to Documents

The first start of the new build, on a Mac whose library is still in
`~/Library/Application Support/SoftPresenter`:

1. If `~/Documents/SoftPresenter` already exists, nothing moves: the app says so and uses the
   old place until the owner sorts it out.
2. The folder is **copied** to `~/Documents/SoftPresenter`, and the copy is checked (every file
   there, the same size).
3. In the copy, path and title keys become ids:
   - a path key becomes the id in the song file at that path;
   - a `title:` key becomes the id of the one song with exactly that title; with none or
     several, the item is kept as missing.
4. The files whose meaning changed get a new `version`: `services.json` and `libraries.json`
   2, and a song file with a "go to song" action 2. An older build then refuses those files
   instead of misreading them.
5. The old folder is renamed `SoftPresenter (moved to Documents)` and left as the backup.
6. The log lists each key changed and each item left missing; a notice in the app says the
   library moved and where the backup is.

The church Mac goes through the same steps the first time its app is updated.

**Tested first** on a copy of the owner's data with a test copy of the app (the command-line QA
setup), before the build reaches `/Applications`: every playlist item and library entry
resolves, the leftover "New Song" item shows as missing, and a second start changes nothing.

## 3. Edits from outside the app

- **The app never saves over a file that changed on disk.** It remembers each song file's
  modification time and size from when it read or last wrote it. If they differ when it is
  about to save, it doesn't write. Instead it says "River Song was changed outside
  SoftPresenter" and offers **Reload** (take the file's version) or **Keep mine** (write the
  app's version over it).
- **Opening a song re-reads its file if it changed**, unless that song is on screen or open in
  the editor (then the notice above, with Reload, is shown instead). Reloading a song clears its
  undo history.
- **`sp song reload [SONG|all]`** (and a File menu item) re-reads one song or the whole library,
  including song files added to the folder while the app runs. The same rule for a song on screen
  or in the editor applies.
- Themes follow the same rule (never saved over; re-read with `sp song reload all`).
- No live folder watching: a change appears when the song is opened or reloaded, never in the
  middle of a slide that is on screen.

## 4. Song packs (`.sppack`)

One file that holds chosen songs, or a playlist with its songs, and what they need. It can go by
AirDrop, email or a USB stick.

**Making one:** File › Export Songs…, or right-click songs or a playlist › Export…, or
`sp song export SONG… [--playlist NAME] --to FILE.sppack [--no-videos]`.

**What's inside** (a zip):

    manifest.json        format "softpresenter-pack", version 1, the app version that made it,
                         and lists of the songs, themes, pictures and videos inside
    songs/<id>.sppres    each song, as saved
    themes/<id>.sptheme  only the themes a song sets for itself (see below)
    images/<file>        each picture a song's slides use
    media/<file>         each video a song's slides use (left out with --no-videos)
    playlist.json        when a playlist was exported: its name and items, songs by id

- **Looks:** a song that uses the main theme (most songs) takes on the **receiving computer's
  main theme**, so songs prepared at home look like everything else at church. A song set to a
  particular theme brings that theme, which is added on the receiving computer as a new theme
  (with a new id if one with its id is already there). The receiving computer's own themes,
  Built-in included, are never changed.
- **No paths:** videos are written into the pack as `media/<file>` and the songs point there.
  No full path or user name goes into a pack.
- **Videos** are offered, with their total size ("Include videos (1.2 GB)"), and left out on
  request. A song whose video was left out keeps the slide and shows the video as missing.

**Opening one:** drop it on the window, File › Import Songs…, or
`sp song import FILE.sppack [--skip | --replace | --keep-both] [--library NAME|none]`.

- Songs go into the library as usual; pictures into `Images/`; videos into a new `Media/` folder
  in the library; a playlist becomes a new playlist (named "Sunday (2)" if "Sunday" exists).
- **A song that is already there** (the same id): the app asks **Replace / Keep both / Skip**,
  with "Do this for all". On the command line the default is `--skip`. Keep both gives the copy a
  new id.
- **Slide actions that point at this computer's things** (a message, prop, timer or media item
  that the receiving computer doesn't have) are kept and shown as missing on that slide, and the
  reply lists how many.
- **A pack is untrusted input.** Each entry's name is checked: one of the folders above, a plain
  file name, no `..`, no absolute path, no links. Each entry has a size cap (a song or theme
  5 MB, a picture 100 MB), the whole import is checked against the free disk space first, and
  every song and theme is read through the normal, version-checked readers. Anything else in the
  zip is ignored and named in the reply. Nothing in a pack is run.

## Not in this design

- Live folder watching (decided against: changes come in on open or reload).
- Changing the song file format, or dropping default values from it.
- Packs for media playlists, macros, stage layouts or settings.

## For the builder

- A zip library needs the licence check and a NOTICE line, as every new crate does.
- Docs that change: docs/features.md, docs/cli.md (song export, reload, pack import), docs/app-map.md
  ("Where the app keeps things", including the new `Media/` folder and the missing
  `libraries.json`, `bible-history.json` and `Bibles/`), the README's "Where things are kept",
  and AGENTS.md's default data folder.
- Suggested order: (1) ids and the migration with the move to Documents, tested on the copy;
  (2) outside edits; (3) song packs.
