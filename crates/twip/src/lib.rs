//! Twip: DOCX layout, editing and rendering. `docx` reads and writes packages, `layout` paginates, `editing` is the
//! editor, `render` draws with Skia, `pdf` exports, and `host` is what the SDKs (`twip-ffi`) drive.

pub mod docx;
pub mod editing;
#[cfg(not(target_os = "emscripten"))]
pub mod features;
pub mod font;
#[cfg(not(target_os = "emscripten"))]
pub mod golden;
pub mod host;
pub mod layout;
pub mod pdf;
pub mod prof;
pub mod render;
#[cfg(test)]
mod tests;
