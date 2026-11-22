//! Twip's 2D renderer.
//!
//! Twip records what a page draws into an immutable [`scene::Picture`] once, and a backend replays pictures every
//! frame: [`cpu`] into a pixel buffer, `gpu` (feature `gpu`) into a wgpu texture. The renderer knows nothing about
//! documents: Twip's `render` module translates layout fragments into pictures. Recording takes kurbo's geometry types
//! (re-exported as [`kurbo`]), so Twip and the renderer share one version.
//!
//! Layering, lowest first (a module only uses the ones above it):
//! `geom` → `image`, `font` (ids) → `path`, `paint` → `scene` → `raster` → `cpu`, `gpu`.

#![warn(missing_docs)]

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
    /// The fonts pictures refer to.
    pub fonts: font::FontRegistry,
    /// The images pictures refer to.
    pub images: image::ImageRegistry,
}

impl Resources {
    /// Which registries these are: font and image ids are theirs, so a renderer's caches keyed by them are for one
    /// pair.
    pub(crate) fn registries(&self) -> (u64, u64) {
        (self.fonts.registry, self.images.registry)
    }
}

/// A registry's own number, never 0.
fn next_registry() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
