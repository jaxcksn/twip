//! Reading DOCX into the layout engine's model.

pub mod blank;
pub mod comments;
pub mod diagnostics;
pub mod fields;
mod frames;
pub mod image;
pub mod model;
pub mod numbering;
pub mod package;
pub mod props;
mod reader;
pub mod resolve;
pub mod review;
pub mod styles;
pub mod xml;

pub use model::*;
pub use reader::{read, read_editable};
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod review_read_tests;
pub mod write;
