//! Small pictures of slides for the home window, drawn by the same renderer as the output.

use std::collections::HashMap;

use pc_model::{Slide, Theme, Uuid};
use pc_render::{Fonts, Layers, Painter, Picture, build_layers, target_texture, wgpu};

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
    /// The two pictures the preview alternates between, and which one is in front.
    previews: [Option<(wgpu::Texture, egui::TextureId)>; 2],
    preview_front: usize,
    canvas: Option<(wgpu::Texture, egui::TextureId)>,
}

impl Thumbs {
    pub fn new(device: &wgpu::Device) -> Result<Thumbs, String> {
        Ok(Thumbs {
            painter: Painter::new(device)?,
            fonts: Fonts::new(),
            cache: HashMap::new(),
            drawn_this_frame: 0,
            frame: 0,
            previews: [None, None],
            preview_front: 0,
            canvas: None,
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
        media: Option<(&Picture, pc_model::ImageFit)>,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        self.get_layers(
            gpu,
            egui_renderer,
            slide.id,
            &Layers {
                media_fit: media.map_or(pc_model::ImageFit::Fill, |(_, fit)| fit),
                media: media.map(|(picture, _)| picture),
                slide: Some(slide),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
            },
            theme,
        )
    }

    /// The thumbnail for a picture in the media bin. `None` means it will be ready on a later
    /// frame.
    pub fn get_media(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        id: Uuid,
        picture: &Picture,
        theme: &Theme,
    ) -> Option<egui::TextureId> {
        self.get_layers(
            gpu,
            egui_renderer,
            id,
            &Layers {
                media_fit: pc_model::ImageFit::Fill,
                media: Some(picture),
                slide: None,
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
            },
            theme,
        )
        .map(|(id, _)| id)
    }

    /// Drops one thumbnail so it is drawn again, for when a slide's words change.
    pub fn forget(&mut self, key: Uuid, egui_renderer: &mut egui_wgpu::Renderer) {
        if let Some(thumb) = self.cache.remove(&key) {
            egui_renderer.free_texture(&thumb.id);
        }
    }

    fn get_layers(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        key: Uuid,
        layers: &Layers,
        theme: &Theme,
    ) -> Option<(egui::TextureId, bool)> {
        if let Some(thumb) = self.cache.get_mut(&key) {
            thumb.last_used = self.frame;
            return Some((thumb.id, thumb.overflow));
        }
        if self.drawn_this_frame >= PER_FRAME {
            return None;
        }
        self.drawn_this_frame += 1;
        let built = build_layers(&mut self.fonts, layers, theme, THUMB_W, THUMB_H);
        let texture = target_texture(&gpu.device, THUMB_W, THUMB_H);
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, THUMB_W, THUMB_H) {
            eprintln!("Could not draw a thumbnail: {e}");
            return None;
        }
        let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
        self.cache.insert(key, Thumb { id, overflow: built.overflow, last_used: self.frame, _texture: texture });
        let ages: Vec<(Uuid, u64)> = self.cache.iter().map(|(id, thumb)| (*id, thumb.last_used)).collect();
        for old in to_evict(&ages, MAX_CACHED, self.frame) {
            if let Some(thumb) = self.cache.remove(&old) {
                egui_renderer.free_texture(&thumb.id);
            }
        }
        Some((id, built.overflow))
    }

    /// Forgets the thumbnails of slides that are no longer shown, for when another song is
    /// selected. `keep` says which ones to hold on to (the media bin's).
    pub fn clear_except(&mut self, egui_renderer: &mut egui_wgpu::Renderer, keep: impl Fn(&Uuid) -> bool) {
        let gone: Vec<Uuid> = self.cache.keys().filter(|id| !keep(id)).copied().collect();
        for id in gone {
            self.forget(id, egui_renderer);
        }
    }

    /// Redraws the Edit view's large picture of the slide being edited and returns its texture.
    pub fn canvas(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        slide: &Slide,
        theme: &Theme,
    ) -> Option<egui::TextureId> {
        // Large enough to stay sharp when the slide is zoomed in on a Retina screen.
        const W: u32 = 2560;
        const H: u32 = 1440;
        let built = build_layers(
            &mut self.fonts,
            &Layers {
                media_fit: pc_model::ImageFit::Fill,
                media: None,
                slide: Some(slide),
                overlay: None,
                time: 0.0,
                backdrop: None,
                mask: [0.0; 4],
            },
            theme,
            W,
            H,
        );
        if self.canvas.is_none() {
            let texture = target_texture(&gpu.device, W, H);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
            self.canvas = Some((texture, id));
        }
        let (texture, id) = self.canvas.as_ref()?;
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, W, H) {
            eprintln!("Could not draw the slide being edited: {e}");
        }
        Some(*id)
    }

    /// Draws the audience screen's picture for the preview. Returns it, and the picture that
    /// was there before, which is kept so the new one can fade in over it.
    pub fn preview(
        &mut self,
        gpu: &Gpu,
        egui_renderer: &mut egui_wgpu::Renderer,
        layers: &Layers,
        theme: &Theme,
    ) -> (Option<egui::TextureId>, Option<egui::TextureId>) {
        let built = build_layers(&mut self.fonts, layers, theme, PREVIEW_W, PREVIEW_H);
        let before = self.previews[self.preview_front].as_ref().map(|(_, id)| *id);
        // Draw into the picture that is not in front, then bring it to the front.
        let back = 1 - self.preview_front;
        if self.previews[back].is_none() {
            let texture = target_texture(&gpu.device, PREVIEW_W, PREVIEW_H);
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            let id = egui_renderer.register_native_texture(&gpu.device, &view, wgpu::FilterMode::Linear);
            self.previews[back] = Some((texture, id));
        }
        let Some((texture, id)) = self.previews[back].as_ref() else { return (before, None) };
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        if let Err(e) = self.painter.paint(&gpu.device, &gpu.queue, &built.scene, &view, PREVIEW_W, PREVIEW_H) {
            eprintln!("Could not draw the preview: {e}");
        }
        self.preview_front = back;
        (Some(*id), before)
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
