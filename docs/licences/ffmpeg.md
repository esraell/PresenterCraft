# FFmpeg: how SoftPresenter uses it, and what that asks

A working note, not legal advice. Read FFmpeg's own licence pages
(<https://ffmpeg.org/legal.html>) before publishing a build that includes FFmpeg.

## How it is used

SoftPresenter plays the videos macOS's own player can't (MKV, WebM, AVI and the like, or a
codec a Mac lacks) by running the `ffmpeg` and `ffprobe` programs as separate processes and
reading the decoded pictures and sound from them through pipes (`crates/sp-video/src/ffmpeg.rs`).
No FFmpeg code is compiled into or linked with SoftPresenter, and nothing from FFmpeg is in
this repository.

## In the app

The Mac app carries its own FFmpeg: `ffmpeg` and `ffprobe` in `SoftPresenter.app/Contents/MacOS`,
with FFmpeg's licence (`COPYING.LGPLv2.1`, `LICENSE.md`) and a note of the exact source and
configuration (`SOURCE.txt`) in `Contents/Resources/FFmpeg`. The app looks for FFmpeg there
first, then in `/opt/homebrew/bin`, `/usr/local/bin` and `/usr/bin`.

`scripts/build-ffmpeg.sh` makes it, and `scripts/macos-app.sh` runs it:

- **The source:** FFmpeg 9.0.2 as FFmpeg publishes it,
  <https://ffmpeg.org/releases/ffmpeg-9.0.2.tar.xz>, checked against its SHA-256 (the same one
  Homebrew records for that file) before it is built. Nothing in it is changed.
- **LGPL 2.1 or later:** configured without `--enable-gpl`, `--enable-version3` or
  `--enable-nonfree`, and with `--disable-autodetect`, so no outside library (GPL or otherwise)
  is picked up. The script refuses a build whose configuration says `gpl` or `nonfree`.
- **Nothing but macOS:** linked statically; the only libraries it uses are macOS's own (the
  system library, zlib, bzip2, and the VideoToolbox, AudioToolbox and Core Media frameworks).
  The script refuses a build that needs anything else, or that has a folder of the building
  computer written into it.
- **Only what SoftPresenter asks of it:** every decoder, file format and parser FFmpeg has, for
  reading; for writing, only raw pictures and raw 32-bit float sound, which it pipes to the
  app; only the `file` and `pipe` protocols, and no network.

Decoding H.264, HEVC, VP8, VP9, MPEG-1, MPEG-2, MPEG-4, WMV, ProRes, AAC, MP3, Opus, Vorbis,
AC-3, WMA and the rest is FFmpeg's own code. AV1 is decoded by FFmpeg through the Mac's video
hardware, which M3 and later Macs have; an older Mac would need the dav1d decoder (BSD
licence) added to the build.

What the LGPL asks, and how the app meets it:

- **The licence goes with the programs:** `Contents/Resources/FFmpeg/COPYING.LGPLv2.1`.
- **The source is available:** `SOURCE.txt` names the exact file, its checksum and the
  configuration; FFmpeg's own site keeps its releases. A public release should also keep a copy
  of that file with the release, or offer it in writing, in case the link ever goes.
- **It can be replaced:** the programs are separate files, run as separate processes; anyone can
  put their own build of `ffmpeg` and `ffprobe` in their place.
- **No FFmpeg code is in SoftPresenter itself**, so SoftPresenter's licence is its own.

## Rules

- Never copy Homebrew's FFmpeg into the app: it is a GPL build (with x264, x265 and others) and
  depends on Homebrew's own libraries.
- Adding an outside library to the build (dav1d, say) means reading its licence first, adding it
  here, and keeping the build LGPL.
