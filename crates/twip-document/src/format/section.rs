//! Authored section formatting, independent of any file format: what a section sets, with unset kept apart from
//! each value.

use std::rc::Rc;

use super::{ForeignProps, TextBorder};
use crate::section::PageBorderDisplay;

/// A section's page borders as authored: each edge's line, and how the borders are placed. `None` means "not
/// written". Its `==` is authored equality.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PageBorderProps {
    /// The top edge.
    pub top: Option<TextBorder>,
    /// The left edge.
    pub left: Option<TextBorder>,
    /// The bottom edge.
    pub bottom: Option<TextBorder>,
    /// The right edge.
    pub right: Option<TextBorder>,
    /// What the edges' distances are measured from.
    pub offset_from: Option<PageBorderOffset>,
    /// The pages of the section they show on.
    pub display: Option<PageBorderDisplay>,
    /// In front of or behind the text and objects they meet.
    pub z_order: Option<PageBorderZOrder>,
    /// What the source format wrote that this struct does not model (an art border's picture, unknown attributes
    /// and children). Owned by the adapter that read it; never read by formatting or layout.
    pub foreign: Option<Rc<ForeignProps>>,
}

/// What page borders' distances are measured from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PageBorderOffset {
    /// The text.
    #[default]
    Text,
    /// The page's edge.
    Page,
}

/// Where page borders are drawn against the text and objects they meet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PageBorderZOrder {
    /// In front of them.
    #[default]
    Front,
    /// Behind them.
    Back,
}

/// A section's authored page borders: typed (`None`: none written), with where they were read from in their origin.
/// Nothing records what edits set: an edit sets the borders whole.
pub type PageBordersDirect = crate::authored::Authored<Option<Rc<PageBorderProps>>, std::convert::Infallible>;
