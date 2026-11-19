//! Small pictures of slides for the home window, drawn by the same renderer as the output.

use std::collections::HashMap;

use pc_model::{Slide, Theme, Uuid};
use pc_render::{Fonts, Painter, build_scene, target_texture, wgpu};

use crate::Gpu;

/// Thumbnails are drawn at this size and scaled by the interface to whatever is displayed.
pub const THUMB_W: u32 = 480;
pub const THUMB_H: u32 = 270;
const PREVIEW_W: u32 = 640;
const PREVIEW_H: u32 = 360;
/// New thumbnails drawn per frame, so a long song fills in without stalling the window.
const PER_FRAME: usize = 12;

struct Thumb {
    id: egui::TextureId,
    overflow: bool,
    // Kept alive for as long as the interface may show it.
    _texture: wgpu::Texture,
}

pub struct Thumbs {
    painter: Painter,
    fonts: Fonts,
    cache: HashMap<Uuid, Thumb>,
    drawn_this_frame: usize,
    preview: Option<(wgpu::Texture, egui::TextureId)>,
}

impl Thumbs {
    pub fn new(device: &wgpu::Device) -> Result<Thumbs, String> {
        Ok(Thumbs {
            painter: Painter::new(device)?,
            fonts: Fonts::new(),
            cache: HashMap::new(),
            drawn_this_frame: 0,
            preview: None,
        })
    }

    /// Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.drawn_this_frame = 0;
    }

    /// The thumbnail for a slide and whether its text overflows. `None` means it will be
    /// ready on a later frame.
    pub fn get(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        slide: &Slide,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        if let Some(thumb) = self.cache.get(&slide.id) {
            return Some((thumb.id, thumb.overflow));
        }
        if self.drawn_this_frame >= PER_FRAME {
            return None;
        }
        self.drawn_this_frame += 1;
        let built = build_scene(&mut self.fonts, Some(slide), theme, THUMB_W, THUMB_H);
        let texture = target_texture(&gpu.device, THUMB_W, THUMB_H);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, THUMB_W, THUMB_H) {
            eprintln!("Could not draw a thumbnail: {e}");
            return None;
        }
        let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
        self.cache.insert(slide.id, Thumb { id, overflow: built.overflow, _texture: texture });
        Some((id, built.overflow))
    }

    /// Forgets every thumbnail, for when another song is selected.
    pub fn clear(&mut self, egui_renderer: &mut egui_wgpu::Renderer) {
        for (_, thumb) in self.cache.drain() {
            egui_renderer.free_texture(&thumb.id);
        }
    }

    /// Redraws the live preview and returns its texture. `None` draws black.
    pub fn preview(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        slide: Option<&Slide>,
        theme: &Theme,
    ) -> Option<egui::TextureId> {
        let built = build_scene(&mut self.fonts, slide, theme, PREVIEW_W, PREVIEW_H);
        if self.preview.is_none() {
            let texture = target_texture(&gpu.device, PREVIEW_W, PREVIEW_H);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
            self.preview = Some((texture, id));
        }
        let (texture, id) = self.preview.as_ref()?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, PREVIEW_W, PREVIEW_H) {
            eprintln!("Could not draw the preview: {e}");
        }
        Some(*id)
    }
}
