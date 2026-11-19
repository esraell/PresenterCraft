//! Videos, and pictures in formats beyond the common few, read with the computer's own media
//! system: AVFoundation and ImageIO on macOS, which decode in hardware and read what QuickTime
//! and Preview read (MP4, MOV, HEVC, ProRes; HEIC, TIFF, PSD and others).
//!
//! A [`Player`] belongs to the main thread, which starts, stops and loops it. The pictures it
//! makes are taken from a [`Frames`], which can go to another thread: the one drawing the
//! screens.
//!
//! This is the one crate in PresenterCraft that may use `unsafe` code, for calls into the
//! system's frameworks; each such call says why it is sound.

#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod mac;

#[cfg(target_os = "macos")]
pub use mac::{Frames, Player, picture, still};

#[cfg(not(target_os = "macos"))]
mod other;

#[cfg(not(target_os = "macos"))]
pub use other::{Frames, Player, picture, still};

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

/// True when this build can play videos.
pub const AVAILABLE: bool = cfg!(target_os = "macos");

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
