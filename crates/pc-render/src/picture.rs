use std::sync::Arc;

use vello::peniko::{Blob, ImageAlphaType, ImageData, ImageFormat};

/// A decoded picture, ready to draw. Cheap to clone: the pixels are shared.
#[derive(Clone)]
pub struct Picture {
    pub(crate) image: ImageData,
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picture({}x{})", self.image.width, self.image.height)
    }
}

impl Picture {
    /// Makes a picture from rows of RGBA pixels with straight (not premultiplied) alpha.
    /// Returns `None` if either side is zero or the pixel count does not match the size.
    pub fn from_rgba(width: u32, height: u32, rgba: Vec<u8>) -> Option<Picture> {
        let expected = (width as usize).checked_mul(height as usize)?.checked_mul(4)?;
        if width == 0 || height == 0 || rgba.len() != expected {
            return None;
        }
        Some(Picture {
            image: ImageData {
                data: Blob::new(Arc::new(rgba)),
                format: ImageFormat::Rgba8,
                alpha_type: ImageAlphaType::Alpha,
                width,
                height,
            },
        })
    }

    /// True when both are the same picture (not merely alike).
    pub fn same(&self, other: &Picture) -> bool {
        self.image.data.id() == other.image.data.id()
    }

    /// A copy no larger than `max_side` on its longer side, each pixel the average of those it
    /// covers. A picture already small enough is handed back as it is (its pixels shared).
    pub fn shrunk(&self, max_side: u32) -> Picture {
        let (w, h) = (self.image.width, self.image.height);
        if w.max(h) <= max_side || max_side == 0 {
            return self.clone();
        }
        let scale = f64::from(max_side) / f64::from(w.max(h));
        let (nw, nh) = (((f64::from(w) * scale).round() as u32).max(1), ((f64::from(h) * scale).round() as u32).max(1));
        let source = self.image.data.data();
        let mut out = Vec::with_capacity((nw * nh * 4) as usize);
        for y in 0..nh {
            let (y0, y1) = (y * h / nh, ((y + 1) * h / nh).max(y * h / nh + 1));
            for x in 0..nw {
                let (x0, x1) = (x * w / nw, ((x + 1) * w / nw).max(x * w / nw + 1));
                let mut sum = [0u32; 4];
                for sy in y0..y1 {
                    let row = (sy * w) as usize * 4;
                    for sx in x0..x1 {
                        let at = row + sx as usize * 4;
                        for (total, value) in sum.iter_mut().zip(&source[at..at + 4]) {
                            *total += u32::from(*value);
                        }
                    }
                }
                let count = (y1 - y0) * (x1 - x0);
                out.extend(sum.map(|total| ((total + count / 2) / count) as u8));
            }
        }
        Picture::from_rgba(nw, nh, out).unwrap_or_else(|| self.clone())
    }

    pub fn width(&self) -> u32 {
        self.image.width
    }

    pub fn height(&self) -> u32 {
        self.image.height
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_and_pixel_count_must_agree() {
        assert!(Picture::from_rgba(2, 2, vec![0; 16]).is_some());
        assert!(Picture::from_rgba(2, 2, vec![0; 15]).is_none());
        assert!(Picture::from_rgba(0, 2, vec![]).is_none());
        assert!(Picture::from_rgba(2, 0, vec![]).is_none());
        let p = Picture::from_rgba(3, 1, vec![0; 12]).unwrap();
        assert_eq!((p.width(), p.height()), (3, 1));
    }

    #[test]
    fn a_shrunk_copy_keeps_the_shape_and_averages_the_pixels() {
        // Four pixels across, two down: black and white columns, which average to grey.
        let rgba: Vec<u8> =
            (0..8).flat_map(|i| if i % 2 == 0 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        let picture = Picture::from_rgba(4, 2, rgba).unwrap();
        let small = picture.shrunk(2);
        assert_eq!((small.width(), small.height()), (2, 1));
        assert_eq!(&small.image.data.data()[..4], &[128, 128, 128, 255]);
        // Small enough already: the same pixels, not a copy.
        assert!(picture.shrunk(10).same(&picture));
    }
}
