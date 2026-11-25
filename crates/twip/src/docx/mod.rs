//! Reading DOCX into the layout engine's model.

pub(crate) mod anchor;
pub mod binding;
pub mod blank;
pub mod char_props;
pub mod comments;
pub mod diagnostics;
pub mod fields;
pub mod font_table;
pub mod forms;
pub mod formula;
pub(crate) mod frames;
pub mod glossary;
pub mod grid;
pub mod image;
pub mod line_style;
pub mod model;
pub mod numbering;
pub mod package;
pub mod paragraph;
pub mod properties;
pub mod props;
mod reader;
pub mod resolve;
pub mod review;
pub mod run_xml;
pub mod section;
pub mod styles;
pub mod watermark;
pub mod xml;

pub use model::*;
#[cfg(test)]
pub(crate) use reader::MAX_CELL_HEADERS;
pub(crate) use reader::header_list;
pub(crate) use reader::line_number_run;
pub(crate) use reader::local_offset;
#[cfg(test)]
pub(crate) use reader::set_option;
pub(crate) use reader::style_cell;
pub use reader::{now_local, now_utc, read, read_at, read_editable};
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod review_read_tests;
#[cfg(test)]
mod styles_tests;
pub mod write;
