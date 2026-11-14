//! Twip's 2D renderer.
//!
//! Twip records what a page draws into an immutable [`scene::Picture`] once, and a backend replays pictures every
//! frame: [`cpu`] into a pixel buffer, `gpu` (feature `gpu`) into a wgpu texture. The renderer knows nothing about
//! documents: Twip's `render` module translates layout fragments into pictures. Recording takes kurbo's geometry types
//! (re-exported as [`kurbo`]), so Twip and the renderer share one version.
//!
//! Layering, lowest first (a module only uses the ones above it):
//! `geom` → `image`, `font` (ids) → `path`, `paint` → `scene` → `raster` → `cpu`, `gpu`.

pub use kurbo;

pub mod font;
mod fx;
mod geom;
pub mod image;
pub mod paint;
mod path;
pub mod scene;

mod raster;

mod colour;
mod cover;
pub mod cpu;
#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "pdf")]
pub mod pdf;

/// What pictures refer to by id, owned by the caller and lent to every frame.
#[derive(Debug, Default)]
pub struct Resources {
    pub fonts: font::FontRegistry,
    pub images: image::ImageRegistry,
}
