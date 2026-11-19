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
/// Thumbnails kept at once. Each is about half a megabyte of GPU memory, so this is about
/// 80 MB; scrolling a long song drops the ones not looked at for longest.
const MAX_CACHED: usize = 160;

struct Thumb {
    id: egui::TextureId,
    overflow: bool,
    /// The frame this thumbnail was last shown in.
    last_used: u64,
    // Kept alive for as long as the interface may show it.
    _texture: wgpu::Texture,
}

pub struct Thumbs {
    painter: Painter,
    fonts: Fonts,
    cache: HashMap<Uuid, Thumb>,
    drawn_this_frame: usize,
    frame: u64,
    preview: Option<(wgpu::Texture, egui::TextureId)>,
}

impl Thumbs {
    pub fn new(device: &wgpu::Device) -> Result<Thumbs, String> {
        Ok(Thumbs {
            painter: Painter::new(device)?,
            fonts: Fonts::new(),
            cache: HashMap::new(),
            drawn_this_frame: 0,
            frame: 0,
            preview: None,
        })
    }

    /// Call once at the start of each frame.
    pub fn begin_frame(&mut self) {
        self.drawn_this_frame = 0;
        self.frame += 1;
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
        if let Some(thumb) = self.cache.get_mut(&slide.id) {
            thumb.last_used = self.frame;
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
        self.cache.insert(slide.id, Thumb { id, overflow: built.overflow, last_used: self.frame, _texture: texture });
        let ages: Vec<(Uuid, u64)> = self.cache.iter().map(|(id, thumb)| (*id, thumb.last_used)).collect();
        for old in to_evict(&ages, MAX_CACHED, self.frame) {
            if let Some(thumb) = self.cache.remove(&old) {
                egui_renderer.free_texture(&thumb.id);
            }
        }
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

/// Which thumbnails to drop so that at most `max` remain: the ones shown longest ago, and
/// never one shown in the current frame (it is on screen right now).
fn to_evict(ages: &[(Uuid, u64)], max: usize, current_frame: u64) -> Vec<Uuid> {
    if ages.len() <= max {
        return Vec::new();
    }
    let mut old: Vec<(Uuid, u64)> = ages.iter().copied().filter(|(_, used)| *used < current_frame).collect();
    old.sort_by_key(|(_, used)| *used);
    old.into_iter().take(ages.len() - max).map(|(id, _)| id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ages(used: &[u64]) -> Vec<(Uuid, u64)> {
        used.iter().map(|u| (Uuid::new_v4(), *u)).collect()
    }

    #[test]
    fn nothing_is_dropped_under_the_cap() {
        assert!(to_evict(&ages(&[1, 2, 3]), 3, 9).is_empty());
        assert!(to_evict(&[], 3, 9).is_empty());
    }

    #[test]
    fn the_oldest_are_dropped_first() {
        let all = ages(&[5, 1, 9, 3]);
        let dropped = to_evict(&all, 2, 10);
        assert_eq!(dropped, vec![all[1].0, all[3].0]);
    }

    #[test]
    fn thumbnails_on_screen_now_are_kept_even_over_the_cap() {
        let all = ages(&[7, 7, 7, 2]);
        assert_eq!(to_evict(&all, 1, 7), vec![all[3].0]);
    }
}
