//! Reading DOCX into the layout engine's model.

pub(crate) mod anchor;
#[cfg(test)]
pub mod baseline;
pub mod binding;
pub mod blank;
pub mod char_props;
pub mod comments;
pub mod diagnostics;
pub mod drawing;
pub mod fields;
pub mod font_table;
pub mod forms;
pub mod formula;
pub(crate) mod frames;
pub mod glossary;
pub mod grid;
pub mod image;
pub mod line_style;
pub mod list_xml;
pub mod model;
pub mod numbering;
pub mod package;
pub mod page_borders;
pub mod para_props;
pub mod para_xml;
pub mod paragraph;
mod prop_xml;
pub mod properties;
pub mod props;
mod reader;
#[cfg(test)]
pub mod resolve;
pub mod resolvers;
pub mod review;
pub mod run_xml;
pub mod section;
pub mod style_xml;
pub mod styles;
pub mod table_props;
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
mod cascade_tests;
#[cfg(test)]
mod merge_tests;
#[cfg(test)]
mod review_read_tests;
#[cfg(test)]
pub(crate) mod style_xml_tests;
#[cfg(test)]
mod styles_tests;
pub mod write;
