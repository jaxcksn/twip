//! The layout engine's input: the document model (`twip_document::model`), and what the DOCX adapter keeps beside
//! it

use std::collections::HashMap;

pub use crate::document::model::*;

/// A paragraph joined onto the next one's start through its vanished mark, as read ([`Paragraph::joined_parts`]).
#[derive(Clone, Debug)]
pub struct JoinedPart {
    /// The unit its content ends at in the joined paragraph.
    pub end: u32,
    /// Its authored properties.
    pub ppr: super::para_props::ParaDirect,
    /// Its mark is a style separator (`w:specVanish`).
    pub spec_vanish: bool,
}

/// The package an editable document was read from ([`super::read_editable`]).
pub struct Source {
    pub package: Vec<u8>,
    /// The main part's name and XML as read.
    pub main: String,
    pub xml: String,
    /// Each section's blocks as read (what saving compares the edited document with).
    pub blocks: Vec<Vec<Block>>,
    /// Each header's and footer's part read (as [`Document::stories`]): its XML and its blocks as read.
    pub stories: HashMap<HeaderFooterId, (String, Vec<Block>)>,
    /// The part name of each header and footer the sections referred to as read, those that didn't load among them.
    pub story_parts: HashMap<HeaderFooterId, String>,
    /// Each read image's part name, by its id in [`Document::images`] (what a filled placeholder's image replaces).
    pub image_parts: HashMap<ResourceId, String>,
    /// The footnotes and endnotes parts: name, XML, and whether it holds footnotes.
    pub notes: Vec<(String, String, bool)>,
    /// Each note's blocks as read, by id.
    pub footnotes: HashMap<i64, Vec<Block>>,
    pub endnotes: HashMap<i64, Vec<Block>>,
    /// The content controls as read: saving takes the markup of those no longer in the document out.
    pub controls: HashMap<ControlId, Control>,
    /// The glossary part as read, when the package has one.
    pub glossary: Option<GlossarySource>,
    /// The page background's colour and fill as read.
    pub page_background: (Option<u32>, Option<crate::drawing::paint::Fill>),
    /// The page background colour's theme colour as read.
    pub page_theme: Option<ThemeColourRef>,
    /// The note separator stories as read (as [`Document::separators`], Word's built-in ones among them).
    pub separators: HashMap<String, Vec<Block>>,
    /// Word's built-in separator and continuation separator of each kind of note, by [`NoteSeparator::key`].
    pub separator_defaults: HashMap<String, Vec<Block>>,
}

/// The glossary part as read: its name and XML, and each building block's `w:docPart` byte range in that XML with
/// its body as read (what saving compares the edited body with), in glossary order.
#[derive(Debug, PartialEq)]
pub struct GlossarySource {
    pub part: String,
    pub xml: String,
    pub parts: Vec<(std::ops::Range<usize>, Vec<Block>)>,
}

impl std::fmt::Debug for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Source({}, {} bytes)", self.main, self.xml.len())
    }
}

/// A list definition made since reading: its `w:abstractNum` (as written) and the `w:num` that uses it.
#[derive(Clone, Debug, PartialEq)]
pub struct AddedList {
    pub abstract_id: i64,
    pub num_id: i64,
    pub abstract_xml: String,
}
