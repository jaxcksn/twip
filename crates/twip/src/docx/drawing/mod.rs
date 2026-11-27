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
