use vello::peniko::Color;
use vello::{AaConfig, AaSupport, RenderParams, Renderer, RendererOptions, Scene, wgpu};

/// Paints scenes into GPU textures. One per thread that draws.
pub struct Painter {
    renderer: Renderer,
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
        Renderer::new(device, options).map(|renderer| Painter { renderer }).map_err(|e| e.to_string())
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
            return self.renderer.render_to_texture(device, queue, &padded, target, &params).map_err(|e| e.to_string());
        }
        self.renderer.render_to_texture(device, queue, scene, target, &params).map_err(|e| e.to_string())
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
