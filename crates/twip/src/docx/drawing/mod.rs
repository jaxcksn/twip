//! DrawingML and VML reading: shapes, their paint and text bodies, themes, and where each shape's parts are in its
//! source

pub mod colour;
pub mod geom;
pub mod paint;
pub mod read;
pub mod text;
pub mod theme;
pub mod vml;

use std::ops::Range;

use crate::document::graphic;

/// Where a shape's parts are in its source part, as byte ranges: what saving an edited shape replaces.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Spans {
    pub shape: Option<Range<usize>>,
    pub sp_pr: Option<Range<usize>>,
    pub xfrm: Option<Range<usize>>,
    pub geom: Option<Range<usize>>,
    pub fill: Option<Range<usize>>,
    /// A picture's `pic:blipFill`, beside its properties.
    pub blip: Option<Range<usize>>,
    pub line: Option<Range<usize>>,
    pub effects: Option<Range<usize>>,
    pub body: Option<Range<usize>>,
    pub style: Option<Range<usize>>,
}

/// A shape as read, with its parts' [`Spans`].
pub type Shape = graphic::Shape<Spans>;
/// What a shape read is.
pub type ShapeKind = graphic::ShapeKind<Spans>;
/// A group read, its children with their parts' [`Spans`].
pub type GroupShape = graphic::GroupShape<Spans>;
