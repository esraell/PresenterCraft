//! Videos, and pictures in formats beyond the common few. macOS's own player (AVFoundation)
//! plays what it can, decoding in hardware: MP4, MOV, HEVC, ProRes and the like. Everything
//! else (MKV, WebM, AVI, or a codec this Mac's player lacks) is played by FFmpeg, run as a
//! separate program, without converting it first. Pictures are read with ImageIO, which reads
//! what Preview reads (HEIC, TIFF, PSD and others).
//!
//! A [`Player`] is made and controlled on the main thread. The pictures it makes are taken from
//! a [`Frames`], which can go to another thread: the one drawing the screens.
//!
//! This is the one crate in SoftPresenter that may use `unsafe` code, for calls into the
//! system's frameworks; each such call says why it is sound.

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac;

mod ffmpeg;

#[cfg(target_os = "macos")]
pub use mac::{pdf_pages, picture};

#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(not(target_os = "macos"))]
pub use other::{pdf_pages, picture};

use std::path::Path;
use std::time::Duration;

/// Videos the system's player is asked to play first; anything else goes to FFmpeg.
const SYSTEM_VIDEOS: [&str; 4] = ["mp4", "mov", "m4v", "3gp"];

fn system_first(path: &Path) -> bool {
    cfg!(target_os = "macos")
        && path.extension().is_some_and(|ext| SYSTEM_VIDEOS.contains(&ext.to_string_lossy().to_lowercase().as_str()))
}

/// True when FFmpeg is on this computer, for the videos the system's player can't play.
pub fn ffmpeg_found() -> bool {
    ffmpeg::tool("ffmpeg").is_some()
}

/// A still from a video, no larger than `max_side` on its longer side: from the system's player
/// if it can read the video, otherwise from FFmpeg. Many videos open on black or fade in, so a
/// dark picture half a second in is passed over for one a little later.
pub fn still(path: &Path, max_side: u32) -> Result<Frame, String> {
    let mut last = None;
    for at in [0.5, 2.0, 5.0] {
        match still_at(path, max_side, at) {
            Ok(frame) if !dark(&frame) => return Ok(frame),
            Ok(frame) => last = Some(frame),
            Err(problem) => return last.ok_or(problem),
        }
    }
    last.ok_or_else(|| "The video has no picture.".to_owned())
}

fn still_at(path: &Path, max_side: u32, at: f64) -> Result<Frame, String> {
    #[cfg(target_os = "macos")]
    let system = mac::still(path, max_side, at);
    #[cfg(not(target_os = "macos"))]
    let system: Result<Frame, String> = Err(String::new());
    match system {
        Ok(frame) => Ok(frame),
        Err(system_error) => ffmpeg::still(path, max_side, at).map_err(|ffmpeg_error| {
            // A video the system read fine but FFmpeg can't is still the system's problem to report.
            if system_first(path) && !system_error.is_empty() { system_error } else { ffmpeg_error }
        }),
    }
}

/// True for a picture that is nearly all black: a video's opening before its first scene.
fn dark(frame: &Frame) -> bool {
    let pixels = frame.rgba.as_chunks::<4>().0;
    let step = (pixels.len() / 2000).max(1);
    let (sum, count) = pixels.iter().step_by(step).fold((0u64, 0u64), |(sum, count), [r, g, b, _]| {
        (sum + u64::from(*r) * 3 + u64::from(*g) * 6 + u64::from(*b), count + 10)
    });
    count == 0 || sum / count < 20
}

/// A video playing, with its sound: through the system's player, or through FFmpeg.
pub enum Player {
    #[cfg(target_os = "macos")]
    System(mac::Player),
    Ffmpeg(Box<ffmpeg::Player>),
}

/// Where a playing video's pictures are taken from. It can be sent to another thread.
#[derive(Clone)]
pub enum Frames {
    #[cfg(target_os = "macos")]
    System(mac::Frames),
    Ffmpeg(ffmpeg::Frames),
}

impl Frames {
    /// True when both take pictures from the same video.
    pub fn same(&self, other: &Frames) -> bool {
        match (self, other) {
            #[cfg(target_os = "macos")]
            (Frames::System(a), Frames::System(b)) => a.same(b),
            (Frames::Ffmpeg(a), Frames::Ffmpeg(b)) => a.same(b),
            #[allow(unreachable_patterns)]
            _ => false,
        }
    }

    /// The picture to show now, if there is a new one since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        match self {
            #[cfg(target_os = "macos")]
            Frames::System(frames) => frames.next(),
            Frames::Ffmpeg(frames) => frames.next(),
        }
    }
}

/// Runs the same call on whichever player it is.
macro_rules! either {
    ($self:expr, $player:ident => $call:expr) => {
        match $self {
            #[cfg(target_os = "macos")]
            Player::System($player) => $call,
            Player::Ffmpeg($player) => $call,
        }
    };
}

impl Player {
    /// Opens a video, paused at its start: with the system's player for an MP4 or MOV (on the
    /// main thread), otherwise with FFmpeg. The error is a sentence for the operator.
    pub fn open(path: &Path) -> Result<Player, String> {
        #[cfg(target_os = "macos")]
        if system_first(path) {
            return mac::Player::open(path).map(Player::System);
        }
        ffmpeg::Player::open(path).map(|player| Player::Ffmpeg(Box::new(player)))
    }

    /// Opens a video with FFmpeg, for when the system's player has failed on it (an AV1 video
    /// on a Mac without AV1 hardware, say).
    pub fn open_with_ffmpeg(path: &Path) -> Result<Player, String> {
        ffmpeg::Player::open(path).map(|player| Player::Ffmpeg(Box::new(player)))
    }

    pub fn uses_ffmpeg(&self) -> bool {
        matches!(self, Player::Ffmpeg(_))
    }

    pub fn frames(&self) -> Frames {
        match self {
            #[cfg(target_os = "macos")]
            Player::System(player) => Frames::System(player.frames()),
            Player::Ffmpeg(player) => Frames::Ffmpeg(player.frames()),
        }
    }

    pub fn play(&mut self) {
        either!(self, player => player.play())
    }

    pub fn pause(&mut self) {
        either!(self, player => player.pause())
    }

    pub fn playing(&self) -> bool {
        either!(self, player => player.playing())
    }

    /// Goes to a time, in seconds from the start.
    pub fn seek(&mut self, to: f64) {
        either!(self, player => player.seek(to))
    }

    /// How far in it is, in seconds.
    pub fn position(&self) -> f64 {
        either!(self, player => player.position())
    }

    /// How long it is, once that is known.
    pub fn duration(&self) -> Option<f64> {
        either!(self, player => player.duration())
    }

    pub fn set_looping(&mut self, looping: bool) {
        either!(self, player => player.set_looping(looping))
    }

    /// From 0 (silent) to 1.
    pub fn set_volume(&mut self, volume: f32) {
        either!(self, player => player.set_volume(volume))
    }

    /// True once a video that doesn't loop has played to its end.
    pub fn ended(&self) -> bool {
        either!(self, player => player.ended())
    }

    /// Why the video can't play, once the player has tried and failed.
    pub fn problem(&self) -> Option<String> {
        either!(self, player => player.problem())
    }

    /// Keeps it going (a looping video starts again at its end). Says how soon to call again.
    pub fn tick(&mut self) -> Duration {
        either!(self, player => player.tick())
    }
}

/// A decoded picture: tightly packed RGBA rows, straight (not premultiplied) alpha.
#[derive(Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl std::fmt::Debug for Frame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Frame({}x{})", self.width, self.height)
    }
}

/// Turns premultiplied RGBA into straight RGBA, in place.
fn unpremultiply(rgba: &mut [u8]) {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha == 0 || alpha == 255 {
            continue;
        }
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
        }
    }
}

/// Copies rows of BGRA pixels into RGBA, opaque. Every second pixel and row is taken when the
/// picture is wider than `max_width`, so a 4K video is copied at 1080p.
fn bgra_rows_to_rgba(bytes: &[u8], width: usize, height: usize, bytes_per_row: usize, max_width: usize) -> Frame {
    let step = if width > max_width { 2 } else { 1 };
    let (out_w, out_h) = (width / step, height / step);
    let mut rgba = Vec::with_capacity(out_w * out_h * 4);
    for row in (0..out_h).map(|row| row * step) {
        let start = row * bytes_per_row;
        let line = &bytes[start..start + width * 4];
        for pixel in line.chunks_exact(4 * step) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    // Rows of an odd width taken every second pixel can come out one short or long; trim or pad.
    rgba.resize(out_w * out_h * 4, 255);
    Frame { width: out_w as u32, height: out_h as u32, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_black_opening_counts_as_dark_and_a_scene_does_not() {
        let black = Frame { width: 4, height: 4, rgba: [0, 0, 0, 255].repeat(16) };
        let scene = Frame { width: 4, height: 4, rgba: [90, 120, 60, 255].repeat(16) };
        assert!(dark(&black) && !dark(&scene));
    }

    #[test]
    fn premultiplied_colours_come_back_straight() {
        let mut pixels = [128, 64, 0, 128, 10, 20, 30, 255, 9, 9, 9, 0];
        unpremultiply(&mut pixels);
        assert_eq!(pixels, [255, 128, 0, 128, 10, 20, 30, 255, 9, 9, 9, 0]);
    }

    #[test]
    fn bgra_rows_become_rgba_and_wide_pictures_are_halved() {
        // Two pixels a row, two rows, with two bytes of padding at the end of each row.
        let rows = [1, 2, 3, 0, 4, 5, 6, 0, 99, 99, 7, 8, 9, 0, 10, 11, 12, 0, 99, 99];
        let frame = bgra_rows_to_rgba(&rows, 2, 2, 10, 1920);
        assert_eq!((frame.width, frame.height), (2, 2));
        assert_eq!(frame.rgba, [3, 2, 1, 255, 6, 5, 4, 255, 9, 8, 7, 255, 12, 11, 10, 255]);
        let halved = bgra_rows_to_rgba(&rows, 2, 2, 10, 1);
        assert_eq!((halved.width, halved.height, halved.rgba), (1, 1, vec![3, 2, 1, 255]));
    }
}
