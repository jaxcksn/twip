//! Layout identities

use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ParaId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TableId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FloatId(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FragmentId(u64);

impl FragmentId {
    pub(crate) fn next() -> FragmentId {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        FragmentId(NEXT.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Revision(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LayoutRevision(pub u64);

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
