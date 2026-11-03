//! Word-fidelity layout. Rule ids in comments refer to the rulebook. Points, f64 throughout (UNIT-4).
//!
//! The boundary consumers use: [`Snapshot`] and its fragments and traversals, and [`layout`] for a full,
//! uncached layout. Everything else is the engine's.

pub mod autofit;
pub mod cache;
pub mod document;
pub mod dump;
pub mod env;
pub mod float;
pub mod flow;
pub mod fragment;
pub mod ids;
pub mod incremental;
pub mod job;
mod join;
pub mod line;
pub mod node;
pub mod page;
pub mod paragraph;
pub mod record;
pub mod snapshot;
pub mod space;
pub mod story;
pub mod table;
pub mod text;
pub mod token;
pub mod traverse;
pub(crate) mod units;

#[cfg(test)]
pub(crate) mod tests;

pub use document::{layout, layout_with};
pub use ids::{FloatId, FragmentId, LayoutRevision, ParaId, Revision, TableId, Unit};
pub use incremental::{Change, DocumentLayout, Target};
pub use job::{Budget, Step};
pub use snapshot::{LayoutStatus, PageInfo, PageMeta, Snapshot};

use crate::font::Fonts;

use env::LayoutEnv;

/// What every algorithm may read besides its node, space and token: the fonts (a versioned resource), the
/// document-wide layout settings, and the float an object drag previews.
#[derive(Clone, Copy)]
pub struct LayoutCtx<'a> {
    pub fonts: &'a Fonts,
    pub env: &'a LayoutEnv,
    /// Live layout's preview of a dragged float ([`Preview`]).
    pub preview: Option<&'a Preview>,
}

/// Live layout: a dragged float as the drag would leave it, laid out in place of the document's float with its id
/// and anchored in `anchor` (where the release re-anchors it). Not part of the document: the paragraphs it concerns
/// shape with it ([`space::ShapeSpace::float`], [`space::ShapeSpace::hide`]), which keys every cache built from them.
#[derive(Clone, Debug, PartialEq)]
pub struct Preview {
    pub float: crate::docx::Float,
    pub anchor: ParaId,
}

impl<'a> LayoutCtx<'a> {
    /// The floats paragraph `p` anchors as layout places them: its own, the previewed one in place of its document
    /// self or left out when anchored elsewhere, and the previewed one added when `p` is its new anchor.
    pub fn floats_of<'b>(&self, p: &'b crate::docx::Paragraph) -> Vec<&'b crate::docx::Float>
    where
        'a: 'b,
    {
        let preview = self.preview;
        let mut out: Vec<&'b crate::docx::Float> = p
            .inlines
            .iter()
            .filter_map(|i| match i {
                crate::docx::Inline::Float(f, _) => match preview {
                    Some(v) if v.float.id == f.id && v.anchor != ParaId(p.id) => None,
                    Some(v) if v.float.id == f.id => Some(&v.float),
                    _ => Some(&**f),
                },
                _ => None,
            })
            .collect();
        if let Some(v) = preview.filter(|v| v.anchor == ParaId(p.id) && !out.iter().any(|f| f.id == v.float.id)) {
            out.insert(0, &v.float);
        }
        out
    }
}
