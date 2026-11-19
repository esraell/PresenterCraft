# FFmpeg: how SoftPresenter uses it, and what that asks

A working note, not legal advice. Read FFmpeg's own licence pages
(<https://ffmpeg.org/legal.html>) before publishing a build that includes FFmpeg.

## How it is used

SoftPresenter plays the videos macOS's own player can't (MKV, WebM, AVI and the like, or a
codec a Mac lacks) by running the `ffmpeg` and `ffprobe` programs as separate processes and
reading the decoded pictures and sound from them through pipes (`crates/sp-video/src/ffmpeg.rs`).
No FFmpeg code is compiled into or linked with SoftPresenter, and nothing from FFmpeg is in
this repository.

## Today

The app uses an FFmpeg already installed on the computer: one inside the app first, then
`/opt/homebrew/bin`, `/usr/local/bin` and `/usr/bin`. Without one, those videos say they need
FFmpeg; MP4 and MOV play without it.

## Before a public release

- Ship an **LGPL build** of `ffmpeg` and `ffprobe` inside the app (configured without
  `--enable-gpl` and without `--enable-nonfree`), with its licence text and a written offer of,
  or link to, the exact FFmpeg source it was built from.
- Never copy Homebrew's FFmpeg into the app: it is a GPL build (with x264, x265 and others) and
  depends on Homebrew's own libraries.
- Because FFmpeg runs as a separate program that SoftPresenter only talks to through pipes,
  SoftPresenter's own licence can be chosen freely; the bundled FFmpeg keeps its own (LGPL).
- Check the codecs the LGPL build includes: VP9 (libvpx or FFmpeg's own decoder), AV1 (dav1d),
  and the decoders in FFmpeg itself cover what churches bring; H.264 and HEVC decoding is
  FFmpeg's own and needs no GPL library.
