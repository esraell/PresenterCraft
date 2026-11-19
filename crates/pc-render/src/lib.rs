//! Turns a slide and a theme into a picture on the GPU. The same code draws outputs,
//! thumbnails and previews, so the operator sees what the audience sees. Knows nothing about
//! windows.

mod fonts;
mod gpu;
mod picture;
mod pictures;
mod scene;

pub use fonts::Fonts;
pub use gpu::{Painter, read_pixels, target_texture};
pub use picture::Picture;
pub use pictures::{forget_picture, set_picture_loader, show_picture};
pub use scene::{Built, Layers, build_layers, build_scene, cross_fade, fit_rect, object_frame, scrolls, transition};
pub use vello;
pub use vello::wgpu;
