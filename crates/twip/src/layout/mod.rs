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

#[cfg(test)]
mod tests;

pub use document::{layout, layout_with};
pub use ids::{FloatId, FragmentId, LayoutRevision, ParaId, Revision, TableId, Unit};
pub use incremental::{Change, DocumentLayout, Target};
pub use job::{Budget, Step};
pub use snapshot::{LayoutStatus, PageInfo, PageMeta, Snapshot};

use crate::font::Fonts;

use env::LayoutEnv;

/// What every algorithm may read besides its node, space and token: the fonts (a versioned resource) and
/// the document-wide layout settings.
#[derive(Clone, Copy)]
pub struct LayoutCtx<'a> {
    pub fonts: &'a Fonts,
    pub env: &'a LayoutEnv,
}
