//! Identities and units the layout engine hands out or reads.

use std::sync::atomic::{AtomicU64, Ordering};

/// A paragraph's identity: [`crate::docx::Paragraph::id`], stable across edits. The only way effects, records and
/// fragments refer to a paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParaId(pub u64);

/// A table's identity: [`crate::docx::Table::id`]. Tables re-read by an edit get a new one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u64);

/// A floating object's identity: [`crate::docx::Float::id`], stable across edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FloatId(pub u64);

/// The identity of a page or body fragment: process-unique, never reused within a process; a reused body keeps its
/// id. The caret map and paint caches key per-page data by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FragmentId(u64);

impl FragmentId {
    /// A fresh id. A process-wide counter, so ids stay unique however many documents and layouts a process holds.
    pub(crate) fn next() -> FragmentId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        FragmentId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

/// A document revision: the editor's edit counter, strictly increasing per edit, undo and redo.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(pub u64);

/// A layout revision: strictly increasing per `DocumentLayout`, one per published snapshot.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayoutRevision(pub u64);

/// A caret-unit offset in a paragraph: one per text character or atom. Distinct from `char`
/// indices and byte offsets, which it is easy to confuse with.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unit(pub u32);

#[cfg(test)]
mod tests {
    use super::FragmentId;

    #[test]
    fn fragment_ids_are_never_reused() {
        let (a, b) = (FragmentId::next(), FragmentId::next());
        assert_ne!(a, b);
        assert!(b > a);
    }
}
