//! Turns a slide and a theme into a picture on the GPU. The same code draws outputs,
//! thumbnails and previews, so the operator sees what the audience sees. Knows nothing about
//! windows.

mod fonts;
mod gpu;
mod scene;

pub use fonts::Fonts;
pub use gpu::{Painter, read_pixels, target_texture};
pub use scene::{Built, build_scene, fit_rect};
pub use vello;
pub use vello::wgpu;
