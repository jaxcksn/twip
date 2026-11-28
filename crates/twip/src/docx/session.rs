//! DOCX session

use std::collections::HashSet;
use std::rc::Rc;

use super::diagnostics::Diagnostics;
use super::font_table::FontRename;
use super::model::{AddedList, Source};

/// What the DOCX adapter keeps beside a document it read: the package as read, what reading reported, and what the
/// edits since then leave the save to write. The reader returns it with the document and the writer takes it with
/// the document; an editor holds both.
#[derive(Clone, Debug, Default)]
pub struct Session {
    /// What the document was read from, when read for editing.
    pub source: Option<Rc<Source>>,
    /// Sections whose page ([`super::SectionPage`]) was edited: saving writes their `w:sectPr` page properties.
    pub edited_sections: HashSet<usize>,
    /// How many sections as read each section holds: more than one once section breaks were deleted. Empty: one
    /// each (nothing deleted).
    pub section_sources: Vec<usize>,
    /// The item parts of data parts removed since reading (taken out of the package on save).
    pub removed_data: Vec<String>,
    /// Families the style, numbering, theme and font table parts were rewritten from and to, in order: the save
    /// rewrites those parts.
    pub font_renames: Vec<FontRename>,
    /// Lists made by the Numbering and Bullets commands since reading: written into the numbering part.
    pub added_lists: Vec<AddedList>,
    /// What reading couldn't keep or make sense of.
    pub diagnostics: Diagnostics,
}
