//! Reading DOCX into the layout engine's model.

pub mod blank;
pub mod comments;
pub mod diagnostics;
pub mod fields;
pub mod font_table;
pub(crate) mod frames;
pub mod glossary;
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
pub(crate) use reader::line_number_run;
pub use reader::{now_utc, read, read_at, read_editable};
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod review_read_tests;
#[cfg(test)]
mod styles_tests;
pub mod write;
