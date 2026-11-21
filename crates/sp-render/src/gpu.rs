use std::sync::{Arc, Weak};

use vello::low_level::vello_encoding::Patch;
use vello::peniko::{Blob, Color, ImageAlphaType, ImageData};
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};

use crate::picture::rows_hold;
use crate::{Picture, PixelOrder};

/// Paints scenes into GPU textures. One per thread that draws.
pub struct Painter {
    renderer: Renderer,
    /// The moving pictures made here that something may still draw (see [`MovingPicture`]).
    moving: Vec<KeptMoving>,
}

/// A picture that changes many times a second (a playing video), kept in a GPU texture of its
/// own: each new picture's pixels are sent straight into it, where a [`Picture`] made new each
/// time would be copied once more and take a new place in the renderer's own texture. The
/// fourth byte of each pixel sent is its straight alpha, as a [`Picture`]'s is.
///
/// Only the [`Painter`] that made it draws its [`picture`](MovingPicture::picture) moving (any
/// other draws it clear, as nothing): it is for the thread that draws the screens, and never
/// goes in the pictures every thread shares. Scenes drawn with it show whatever was sent last,
/// and go on showing that after this is let go, for as long as they are kept.
pub struct MovingPicture {
    texture: wgpu::Texture,
    picture: Picture,
    /// Says, while this is held, that more pixels may come.
    _held: Arc<()>,
}

/// A moving picture's texture kept by the painter, with what says whether it can be let go.
struct KeptMoving {
    image: ImageData,
    held: Weak<()>,
    /// True once it has been painted, from when on the renderer keeps a copy of `image` too.
    painted: bool,
}

impl MovingPicture {
    /// The picture to draw: the same one however many times new pixels are sent.
    pub fn picture(&self) -> Picture {
        self.picture.clone()
    }

    /// True when pixels of this size and order of bytes can be sent into it.
    pub fn fits(&self, width: u32, height: u32, order: PixelOrder) -> bool {
        (self.picture.width(), self.picture.height(), self.picture.image.format) == (width, height, order.format())
    }
}

impl Painter {
    /// Fails if the GPU cannot run the renderer's shaders.
    pub fn new(device: &wgpu::Device) -> Result<Painter, String> {
        let options = RendererOptions {
            use_cpu: false,
            antialiasing_support: AaSupport::area_only(),
            // One thread is the renderer's own advice for macOS.
            num_init_threads: std::num::NonZeroUsize::new(1),
            pipeline_cache: None,
        };
        Renderer::new(device, options)
            .map(|renderer| Painter { renderer, moving: Vec::new() })
            .map_err(|e| e.to_string())
    }

    /// Makes a moving picture of a size, for pixels in an order of bytes. It shows nothing
    /// until pixels are sent with [`Painter::show_rows`]. `None` for a size of zero, or one
    /// larger than the GPU's textures can be.
    pub fn moving_picture(
        &mut self,
        device: &wgpu::Device,
        width: u32,
        height: u32,
        order: PixelOrder,
    ) -> Option<MovingPicture> {
        let most = device.limits().max_texture_dimension_2d;
        if width == 0 || height == 0 || width > most || height > most {
            return None;
        }
        // The renderer copies this texture into its own, which holds RGBA bytes, so it is made
        // the same kind whatever the order of the bytes sent: the order is said by the picture
        // drawn, and put right by the renderer as it draws.
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sp-render moving picture"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        // The renderer takes the pixels from the texture, by the picture's name. The picture's
        // own are clear and never looked at while the texture is kept; they are there because
        // the renderer stops the thread if it is ever asked to draw a picture with neither
        // (and memory nothing reads or writes costs next to nothing).
        let clear = vec![0u8; (width as usize).checked_mul(height as usize)?.checked_mul(4)?];
        let image = ImageData {
            data: Blob::new(Arc::new(clear)),
            format: order.format(),
            alpha_type: ImageAlphaType::Alpha,
            width,
            height,
        };
        let whole = wgpu::TexelCopyTextureInfoBase {
            texture: texture.clone(),
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        };
        self.renderer.override_image(&image, Some(whole));
        let held = Arc::new(());
        self.moving.push(KeptMoving { image: image.clone(), held: Arc::downgrade(&held), painted: false });
        Some(MovingPicture { texture, picture: Picture { image }, _held: held })
    }

    /// Sends a moving picture its new pixels: rows of four-byte pixels in the order of bytes it
    /// was made for, each row starting `bytes_per_row` after the last (which may be more than
    /// its pixels fill). Everything drawn with its picture from now on shows them. False, with
    /// nothing changed, when there are too few bytes for its size.
    pub fn show_rows(
        &mut self,
        queue: &wgpu::Queue,
        moving: &MovingPicture,
        rows: &[u8],
        bytes_per_row: usize,
    ) -> bool {
        let (width, height) = (moving.picture.width(), moving.picture.height());
        let Ok(stride) = u32::try_from(bytes_per_row) else { return false };
        if !rows_hold(width, height, rows.len(), bytes_per_row) {
            return false;
        }
        queue.write_texture(
            moving.texture.as_image_copy(),
            rows,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(stride), rows_per_image: None },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.renderer.mark_override_image_dirty(&moving.picture.image);
        true
    }

    /// How many moving pictures' textures are kept: those something may still show, as of the
    /// last painting.
    pub fn moving_pictures(&self) -> usize {
        self.moving.len()
    }

    /// After a scene is painted, lets go of the textures of moving pictures nothing shows any
    /// more. One is kept while more pixels may come for it, and then while any scene drawn
    /// with it is kept (the picture a fade starts from, say), so that scene goes on showing
    /// the pixels sent last.
    fn forget_unused_moving_pictures(&mut self, painted: &Scene) {
        if self.moving.is_empty() {
            return;
        }
        let in_scene =
            |image: &ImageData| {
                painted.encoding().resources.patches.iter().any(
                    |patch| matches!(patch, Patch::Image { image: drawn, .. } if drawn.data.id() == image.data.id()),
                )
            };
        let renderer = &mut self.renderer;
        self.moving.retain_mut(|kept| {
            if in_scene(&kept.image) {
                kept.painted = true;
                return true;
            }
            // Who holds the picture, now that the scene just painted isn't drawn with it: the
            // list here, the renderer once it has painted it (it remembers every picture it
            // has a place for), and each scene kept that is drawn with it. Should the
            // renderer have given the place away, a last scene loses its picture early and
            // shows it clear; nothing worse.
            let unused =
                kept.held.strong_count() == 0 && kept.image.data.strong_count() <= 1 + usize::from(kept.painted);
            if unused {
                renderer.override_image(&kept.image, None);
            }
            !unused
        });
    }

    /// Paints into a view of a texture made by [`target_texture`] with the same size.
    pub fn paint(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        scene: &Scene,
        target: &wgpu::TextureView,
        w: u32,
        h: u32,
    ) -> Result<(), String> {
        let params =
            RenderParams { base_color: Color::BLACK, width: w, height: h, antialiasing_method: AaConfig::Area };
        // A scene of plain colour fills only (no text, gradient or picture) makes vello 0.12 take
        // a shortcut that throws its picture atlas away while still counting the pictures in it
        // as uploaded, so the next picture drawn from it comes out black. Such a scene is given a
        // gradient no one can see, so it takes the usual way.
        if scene.encoding().resources.patches.is_empty() {
            let mut padded = scene.clone();
            let unseen = vello::peniko::Gradient::new_linear((0.0, 0.0), (1.0, 0.0))
                .with_stops([Color::TRANSPARENT, Color::TRANSPARENT]);
            padded.fill(
                vello::peniko::Fill::NonZero,
                vello::kurbo::Affine::IDENTITY,
                &unseen,
                None,
                &vello::kurbo::Rect::new(0.0, 0.0, 1.0, 1.0),
            );
            let done = self.renderer.render_to_texture(device, queue, &padded, target, &params);
            self.forget_unused_moving_pictures(scene);
            return done.map_err(|e| e.to_string());
        }
        let done = self.renderer.render_to_texture(device, queue, scene, target, &params);
        self.forget_unused_moving_pictures(scene);
        done.map_err(|e| e.to_string())
    }
}

/// A texture the painter can draw into, that can be shown, copied to a window or read back.
/// Sizes of zero are raised to one pixel.
pub fn target_texture(device: &wgpu::Device, w: u32, h: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("sp-render target"),
        size: wgpu::Extent3d { width: w.max(1), height: h.max(1), depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Reads a texture back from the GPU as tightly packed RGBA rows. Blocks until done.
pub fn read_pixels(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Result<Vec<u8>, String> {
    let (w, h) = (texture.width(), texture.height());
    let row = w as usize * 4;
    // The GPU requires each row of a copy to start on a 256-byte boundary.
    let align = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT as usize;
    let padded_row = row.div_ceil(align) * align;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("sp-render readback"),
        size: (padded_row * h as usize) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row as u32),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
    );
    queue.submit([encoder.finish()]);

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
    let mapped = slice.get_mapped_range().map_err(|e| e.to_string())?;
    let mut pixels = Vec::with_capacity(row * h as usize);
    for padded in mapped.chunks(padded_row) {
        pixels.extend_from_slice(&padded[..row]);
    }
    Ok(pixels)
}
