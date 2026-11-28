//! Identities

use std::sync::atomic::{AtomicU64, Ordering};

/// A paragraph's identity: process-local, kept through edits, undo and redo; new on every read and paste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParaId(pub u64);

/// A table's identity, as a paragraph's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u64);

/// A floating object's identity, as a paragraph's.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FloatId(pub u64);

/// An offset in a paragraph: one unit per character and per inline object; markers take none.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Unit(pub u32);

/// A field's identity: its begin, separator and end markers share it. Process-local, kept
/// through edits, undo and redo; new on every read and paste.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FieldId(pub u64);

impl FieldId {
    /// A new id (unique in the process).
    pub fn next() -> FieldId {
        FieldId(next_para_id())
    }
}

/// A content control's or custom XML wrapper's identity: process-local, kept through edits,
/// undo and redo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ControlId(pub u64);

impl ControlId {
    /// A new id (unique in the process).
    pub fn next() -> ControlId {
        ControlId(next_para_id())
    }
}

/// A header's or footer's identity: process-local, kept through edits, undo and redo; new on every read. A read
/// numbers its headers and footers in the order its format keeps them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct HeaderFooterId(pub u64);

impl HeaderFooterId {
    /// A new id (unique in the process, after every id before it).
    pub fn next() -> HeaderFooterId {
        HeaderFooterId(next_para_id())
    }
}

/// A resource's identity (an image a picture, fill, bullet or background shows): process-local, kept through edits,
/// undo and redo; new on every read, and on every paste from another document. Later ones are greater.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResourceId(pub u64);

impl ResourceId {
    /// A new id (unique in the process, greater than every earlier one).
    pub fn next() -> ResourceId {
        ResourceId(next_para_id())
    }
}

/// A new paragraph id (unique in the process). Fields, controls, headers and footers, tables, floats and resources
/// draw from the same counter.
pub fn next_para_id() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::{ControlId, FieldId, HeaderFooterId, ResourceId, next_para_id};

    #[test]
    fn ids_come_from_one_counter_and_are_never_reused() {
        let (a, f, c, h, r, b) = (next_para_id(), FieldId::next(), ControlId::next(), HeaderFooterId::next(), ResourceId::next(), next_para_id());
        assert!(a < f.0 && f.0 < c.0 && c.0 < h.0 && h.0 < r.0 && r.0 < b);
    }
}
