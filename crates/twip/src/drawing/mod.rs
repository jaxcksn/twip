//! DrawingML objects: the shape model, its shapes keeping where the DOCX adapter read them from

pub use crate::document::graphic::{
    ChildPath, Frame, NewShape, NonVisual, PicShape, RawGraphic, ShapeText, SpShape, TextPath, Xfrm, colour, geom, paint, patterns, text, theme,
    units,
};
pub use crate::docx::drawing::{GroupShape, Shape, ShapeKind, Spans};
