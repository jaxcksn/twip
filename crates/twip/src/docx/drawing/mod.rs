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
use crate::document::origin::Origin;

/// Where a shape's parts are in its source part, as byte ranges: what saving an edited shape replaces. A shape read
/// holds them in its origin.
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

/// No spans: a shape made rather than read.
static NONE: Spans =
    Spans { shape: None, sp_pr: None, xfrm: None, geom: None, fill: None, blip: None, line: None, effects: None, body: None, style: None };

impl Spans {
    /// The spans shape `s` was read with; none for a shape made rather than read.
    pub fn of(s: &Shape) -> &Spans {
        s.spans.get().unwrap_or(&NONE)
    }
}

/// A shape, its parts' [`Spans`] in its origin when read. The spans are no part of its `==`, as a float's own origin
/// isn't: [`same_shape`] and [`same_kind`] compare them where saving and layout reuse decide.
pub type Shape = graphic::Shape<Origin>;
/// What a shape is.
pub type ShapeKind = graphic::ShapeKind<Origin>;
/// A group, its children with their parts' [`Spans`] in their origins.
pub type GroupShape = graphic::GroupShape<Origin>;

/// Whether shapes `a` and `b` are equal, their parts and a group's children's read from the same places.
pub fn same_shape(a: Option<&Shape>, b: Option<&Shape>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a == b && same_spans(a, b),
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// Whether what shapes are, `a` and `b`, is equal, a group's children's parts read from the same places.
pub fn same_kind(a: &ShapeKind, b: &ShapeKind) -> bool {
    a == b && same_child_spans(a, b)
}

fn same_spans(a: &Shape, b: &Shape) -> bool {
    Spans::of(a) == Spans::of(b) && same_child_spans(&a.kind, &b.kind)
}

fn same_child_spans(a: &ShapeKind, b: &ShapeKind) -> bool {
    match (a, b) {
        (ShapeKind::Group(x), ShapeKind::Group(y)) => x.children.iter().zip(&y.children).all(|(c, d)| same_spans(c, d)),
        _ => true,
    }
}
