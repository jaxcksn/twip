//! Reading DOCX into the layout engine's model.

pub mod blank;
pub mod fields;
mod frames;
pub mod image;
pub mod model;
pub mod numbering;
pub mod props;
mod reader;
pub mod resolve;
pub mod styles;
pub mod xml;

pub use model::*;
pub use reader::{read, read_editable};
#[cfg(test)]
mod merge_tests;
pub mod write;
