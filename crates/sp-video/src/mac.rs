//! AVFoundation and ImageIO.

use std::path::Path;
use std::time::Duration;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{AllocAnyThread, MainThreadMarker};
use objc2_av_foundation::{
    AVAssetImageGenerator, AVPlayer, AVPlayerActionAtItemEnd, AVPlayerItem, AVPlayerItemStatus,
    AVPlayerItemVideoOutput, AVURLAsset,
};
use objc2_core_foundation::{CFDictionary, CFString, CFURL, CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGColorSpace, CGContext, CGImage, CGImageAlphaInfo, CGImageByteOrderInfo, kCGColorSpaceSRGB,
};
use objc2_core_media::{CMTime, kCMTimeZero};
use objc2_core_video::{
    CVGetCurrentHostTime, CVPixelBuffer, CVPixelBufferGetBaseAddress, CVPixelBufferGetBytesPerRow,
    CVPixelBufferGetHeight, CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress, kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString, NSURL};
use objc2_image_io::{
    CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways, kCGImageSourceCreateThumbnailWithTransform,
    kCGImageSourceThumbnailMaxPixelSize,
};

use crate::Frame;

/// Video pictures wider than this are copied at half size.
const MAX_VIDEO_WIDTH: usize = 2048;

fn shown_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

fn file_url(path: &Path) -> Result<Retained<NSURL>, String> {
    if !path.exists() {
        return Err(format!("{} could not be found.", shown_name(path)));
    }
    let text = path.to_str().ok_or_else(|| format!("{}: the file's name can't be read.", shown_name(path)))?;
    Ok(NSURL::fileURLWithPath(&NSString::from_str(text)))
}

/// A Core Foundation string as the Foundation string it is the same object as.
fn ns_key(key: &CFString) -> &NSString {
    // SAFETY: CFString and NSString are toll-free bridged: the same object, usable as either.
    unsafe { &*std::ptr::from_ref(key).cast::<NSString>() }
}

fn seconds(time: CMTime) -> Option<f64> {
    // SAFETY: CMTimeGetSeconds only reads the value it is given.
    let seconds = unsafe { time.seconds() };
    seconds.is_finite().then_some(seconds)
}

fn at_seconds(seconds: f64) -> CMTime {
    // SAFETY: CMTimeMakeWithSeconds only builds a value.
    unsafe { CMTime::with_seconds(seconds, 600) }
}

/// A video playing, with its sound. It lives on the main thread.
pub struct Player {
    player: Retained<AVPlayer>,
    item: Retained<AVPlayerItem>,
    output: Retained<AVPlayerItemVideoOutput>,
    looping: bool,
    ended: bool,
}

impl Player {
    /// Opens a video, paused at its start. Must be called on the main thread. The error is a
    /// sentence for the operator.
    pub fn open(path: &Path) -> Result<Player, String> {
        let mtm = MainThreadMarker::new().ok_or("Videos can only be opened on the main thread.")?;
        let url = file_url(path)?;
        // SAFETY: the URL is a valid file URL; the item is made on the main thread, as it must be.
        let item = unsafe { AVPlayerItem::playerItemWithURL(&url, mtm) };
        // Pictures come out as rows of BGRA pixels, the order the frames are copied in.
        let format = NSNumber::new_u32(kCVPixelFormatType_32BGRA);
        // SAFETY: the key is a static string the framework defines.
        let key = ns_key(unsafe { kCVPixelBufferPixelFormatTypeKey });
        let attributes: Retained<NSDictionary<NSString, AnyObject>> =
            NSDictionary::from_slices(&[key], &[format.as_ref() as &AnyObject]);
        // SAFETY: the attributes are a dictionary of the keys and value types the output expects.
        let output = unsafe {
            AVPlayerItemVideoOutput::initWithPixelBufferAttributes(AVPlayerItemVideoOutput::alloc(), Some(&attributes))
        };
        // SAFETY: both objects are alive; an item takes any number of outputs.
        unsafe { item.addOutput(&output) };
        // SAFETY: made on the main thread with a live item.
        let player = unsafe { AVPlayer::playerWithPlayerItem(Some(&item), mtm) };
        // At the end the player stays where it is, so it can be looped or left on its last picture.
        // SAFETY: a plain setter on a live object.
        unsafe { player.setActionAtItemEnd(AVPlayerActionAtItemEnd::None) };
        Ok(Player { player, item, output, looping: false, ended: false })
    }

    /// Where its pictures are taken from, on any thread.
    pub fn frames(&self) -> Frames {
        Frames { output: self.output.clone() }
    }

    pub fn play(&mut self) {
        if self.ended {
            self.seek(0.0);
        }
        // SAFETY: a plain call on a live object, on its thread.
        unsafe { self.player.play() };
    }

    pub fn pause(&self) {
        // SAFETY: a plain call on a live object, on its thread.
        unsafe { self.player.pause() };
    }

    pub fn playing(&self) -> bool {
        // SAFETY: a plain getter on a live object.
        unsafe { self.player.rate() != 0.0 && !self.ended }
    }

    /// Goes to a time, in seconds from the start, exactly.
    pub fn seek(&mut self, to: f64) {
        self.ended = false;
        // SAFETY: plain values; zero tolerance asks for the exact time.
        unsafe {
            self.player.seekToTime_toleranceBefore_toleranceAfter(at_seconds(to.max(0.0)), kCMTimeZero, kCMTimeZero);
        }
    }

    /// How far in it is, in seconds.
    pub fn position(&self) -> f64 {
        // SAFETY: a plain getter on a live object.
        seconds(unsafe { self.player.currentTime() }).unwrap_or(0.0)
    }

    /// How long it is, once that is known.
    pub fn duration(&self) -> Option<f64> {
        // SAFETY: a plain getter on a live object.
        seconds(unsafe { self.item.duration() }).filter(|seconds| *seconds > 0.0)
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// From 0 (silent) to 1.
    pub fn set_volume(&self, volume: f32) {
        // SAFETY: a plain setter on a live object.
        unsafe { self.player.setVolume(volume.clamp(0.0, 1.0)) };
    }

    /// True once a video that doesn't loop has played to its end.
    pub fn ended(&self) -> bool {
        self.ended
    }

    /// Why the video can't play, once the system has tried and failed.
    pub fn problem(&self) -> Option<String> {
        // SAFETY: plain getters on a live object.
        unsafe {
            (self.item.status() == AVPlayerItemStatus::Failed).then(|| {
                self.item.error().map_or_else(
                    || "The video could not be played.".to_owned(),
                    |error| format!("The video could not be played: {}", error.localizedDescription()),
                )
            })
        }
    }

    /// Keeps it going: a looping video starts again at its end, and one that doesn't stops
    /// there. Says how soon to call again.
    pub fn tick(&mut self) -> Duration {
        let Some(duration) = self.duration() else {
            return Duration::from_millis(100);
        };
        let left = duration - self.position();
        if left <= 0.02 && self.playing() {
            if self.looping {
                self.seek(0.0);
            } else {
                self.pause();
                self.ended = true;
            }
            return Duration::from_millis(50);
        }
        Duration::from_secs_f64(left.clamp(0.01, 0.5))
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // SAFETY: a plain call on a live object, on its thread.
        unsafe { self.player.pause() };
    }
}

/// Where a playing video's pictures are taken from. It can be sent to another thread.
#[derive(Clone)]
pub struct Frames {
    output: Retained<AVPlayerItemVideoOutput>,
}

impl Frames {
    /// True when both take pictures from the same video.
    pub fn same(&self, other: &Frames) -> bool {
        Retained::as_ptr(&self.output) == Retained::as_ptr(&other.output)
    }

    /// The picture to show now, if there is a new one since the last was taken.
    pub fn next(&self) -> Option<Frame> {
        let host = i64::try_from(CVGetCurrentHostTime()).unwrap_or(i64::MAX);
        // SAFETY: a video output may be asked for pictures from any thread; these are plain values.
        let time = unsafe { self.output.itemTimeForMachAbsoluteTime(host) };
        // SAFETY: as above.
        if !unsafe { self.output.hasNewPixelBufferForItemTime(time) } {
            return None;
        }
        // SAFETY: as above; a null pointer says the display time isn't wanted.
        let buffer = unsafe { self.output.copyPixelBufferForItemTime_itemTimeForDisplay(time, std::ptr::null_mut()) }?;
        copy_pixels(&buffer)
    }
}

/// Copies a BGRA pixel buffer's picture out as RGBA.
fn copy_pixels(buffer: &CVPixelBuffer) -> Option<Frame> {
    // SAFETY: the buffer is alive; it is locked for reading and unlocked below.
    if unsafe { CVPixelBufferLockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) } != 0 {
        return None;
    }
    let (width, height) = (CVPixelBufferGetWidth(buffer), CVPixelBufferGetHeight(buffer));
    let bytes_per_row = CVPixelBufferGetBytesPerRow(buffer);
    let base = CVPixelBufferGetBaseAddress(buffer);
    let frame = (!base.is_null() && width > 0 && height > 0 && bytes_per_row >= width * 4).then(|| {
        // SAFETY: while locked, the base address points at `height` rows of `bytes_per_row`
        // bytes, which stay valid until the unlock below.
        let bytes = unsafe { std::slice::from_raw_parts(base.cast::<u8>(), bytes_per_row * height) };
        crate::bgra_rows_to_rgba(bytes, width, height, bytes_per_row, MAX_VIDEO_WIDTH)
    });
    // SAFETY: unlocks the lock taken above.
    unsafe { CVPixelBufferUnlockBaseAddress(buffer, CVPixelBufferLockFlags::ReadOnly) };
    frame
}

/// Draws a picture the system decoded into RGBA rows.
fn draw(image: &CGImage) -> Option<Frame> {
    let (width, height) = (CGImage::width(Some(image)), CGImage::height(Some(image)));
    if width == 0 || height == 0 {
        return None;
    }
    let mut rgba = vec![0u8; width.checked_mul(height)?.checked_mul(4)?];
    // SAFETY: the name is a static string the framework defines.
    let space = CGColorSpace::with_name(Some(unsafe { kCGColorSpaceSRGB }))?;
    let info = CGImageAlphaInfo::PremultipliedLast.0 | CGImageByteOrderInfo::Order32Big.0;
    // SAFETY: the buffer holds `height` rows of `width * 4` bytes and outlives the context,
    // which is dropped before the buffer is read.
    let context =
        unsafe { CGBitmapContextCreate(rgba.as_mut_ptr().cast(), width, height, 8, width * 4, Some(&space), info) }?;
    let all = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width as f64, height as f64));
    CGContext::draw_image(Some(&context), all, Some(image));
    drop(context);
    crate::unpremultiply(&mut rgba);
    Some(Frame { width: width as u32, height: height as u32, rgba })
}

/// A still from a video, `at` seconds in, no larger than `max_side` on its longer side.
pub fn still(path: &Path, max_side: u32, at: f64) -> Result<Frame, String> {
    let url = file_url(path)?;
    // SAFETY: a valid file URL and no options.
    let asset = unsafe { AVURLAsset::URLAssetWithURL_options(&url, None) };
    // SAFETY: a live asset; the generator may be used on any thread.
    let generator = unsafe { AVAssetImageGenerator::assetImageGeneratorWithAsset(&asset) };
    let side = f64::from(max_side);
    // SAFETY: plain setters on a live object.
    unsafe {
        generator.setAppliesPreferredTrackTransform(true);
        generator.setMaximumSize(CGSize::new(side, side));
    }
    // The newer call that replaces this one hands the picture back later, on another thread;
    // this one waits for it, which a still for the media bin can do.
    #[allow(deprecated)]
    // SAFETY: plain values; a null pointer says the actual time isn't wanted.
    let image = unsafe { generator.copyCGImageAtTime_actualTime_error(at_seconds(at), std::ptr::null_mut()) }.map_err(
        |error| format!("{}: not a video SoftPresenter can open ({}).", shown_name(path), error.localizedDescription()),
    )?;
    draw(&image).ok_or_else(|| format!("{}: the video's picture is empty.", shown_name(path)))
}

/// A picture file in any format the system reads, no larger than `max_side` on its longer
/// side, turned the way the camera held it.
pub fn picture(path: &Path, max_side: u32) -> Result<Frame, String> {
    let url = file_url(path)?;
    // SAFETY: NSURL and CFURL are toll-free bridged: the same object, usable as either.
    let url: &CFURL = unsafe { &*Retained::as_ptr(&url).cast::<CFURL>() };
    // SAFETY: a valid file URL and no options.
    let source = unsafe { CGImageSource::with_url(url, None) }
        .ok_or_else(|| format!("{}: not a picture SoftPresenter can open.", shown_name(path)))?;
    let yes = NSNumber::new_bool(true);
    let size = NSNumber::new_u32(max_side);
    // SAFETY: the keys are static strings the framework defines.
    let keys = unsafe {
        [
            ns_key(kCGImageSourceCreateThumbnailFromImageAlways),
            ns_key(kCGImageSourceCreateThumbnailWithTransform),
            ns_key(kCGImageSourceThumbnailMaxPixelSize),
        ]
    };
    let options: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&keys, &[yes.as_ref() as &AnyObject, yes.as_ref(), size.as_ref()]);
    // SAFETY: NSDictionary and CFDictionary are toll-free bridged; the keys and values are the
    // types these options take.
    let options: &CFDictionary = unsafe { &*Retained::as_ptr(&options).cast::<CFDictionary>() };
    // SAFETY: a live source and a dictionary of options it understands.
    let image = unsafe { source.thumbnail_at_index(0, Some(options)) }
        .ok_or_else(|| format!("{}: the picture could not be read.", shown_name(path)))?;
    draw(&image).ok_or_else(|| format!("{}: the picture is empty.", shown_name(path)))
}
