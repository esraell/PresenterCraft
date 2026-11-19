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
}
