//! Layout snapshots

use std::ops::Range;
use std::rc::Rc;

use super::fragment::PageFragment;
use super::ids::{LayoutRevision, ParaId, Revision, Unit};
use super::pvec::Pvec;

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutStatus {
    Pending { stale_from: usize },
    Converged,
    Stabilized { frozen_hf: Box<[usize]>, unresolved_fields: Box<[usize]> },
    Inconsistent { pages: Box<[usize]> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageMeta {
    pub section: usize,
    pub number: i64,
    pub section_page: usize,
    pub blank: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PageInfo {
    pub section: usize,
    pub number: i64,
    pub section_page: usize,
    pub blank: bool,
    pub provisional: bool,
}

#[derive(Debug)]
pub struct Snapshot {
    revision: Revision,
    layout_revision: LayoutRevision,
    status: LayoutStatus,
    pages: Pvec<Rc<PageFragment>>,
    info: Pvec<PageInfo>,
    excluded: Pvec<(usize, ParaId, Range<Unit>)>,
}

impl Snapshot {
    pub(crate) fn new(
        revision: Revision,
        layout_revision: LayoutRevision,
        status: LayoutStatus,
        pages: Vec<(Rc<PageFragment>, PageMeta)>,
        excluded: Vec<(usize, ParaId, Range<Unit>)>,
    ) -> Snapshot {
        let stale_from = match status {
            LayoutStatus::Pending { stale_from } => stale_from,
            _ => usize::MAX,
        };
        let (pages, info): (Vec<_>, Vec<_>) = pages
            .into_iter()
            .enumerate()
            .map(|(i, (page, m))| {
                (page, PageInfo { section: m.section, number: m.number, section_page: m.section_page, blank: m.blank, provisional: i >= stale_from })
            })
            .unzip();
        Snapshot {
            revision,
            layout_revision,
            status,
            pages: pages.into_iter().collect(),
            info: info.into_iter().collect(),
            excluded: excluded.into_iter().collect(),
        }
    }

    /// A pending layout's snapshot: `prev`'s, its pages from `from` replaced by `fresh`, the pages laid out, and its
    /// excluded text from `from` by `excluded`. It shares the rest with `prev`.
    pub(crate) fn pending_after(
        prev: &Snapshot,
        revision: Revision,
        layout_revision: LayoutRevision,
        from: usize,
        fresh: Vec<(Rc<PageFragment>, PageMeta)>,
        excluded: Vec<(usize, ParaId, Range<Unit>)>,
    ) -> Snapshot {
        let n = from + fresh.len();
        let (mut pages, mut info) = (prev.pages.clone(), prev.info.clone());
        for (k, (page, m)) in fresh.into_iter().enumerate() {
            pages.set(from + k, page);
            info.set(from + k, PageInfo { section: m.section, number: m.number, section_page: m.section_page, blank: m.blank, provisional: false });
        }
        // The pages after them are what is left of the layout before.
        let was = match prev.status {
            LayoutStatus::Pending { stale_from } => stale_from,
            _ => usize::MAX,
        };
        for i in n..was.min(info.len()) {
            let mut old = info[i].clone();
            old.provisional = true;
            info.set(i, old);
        }
        let mut kept = prev.excluded.clone();
        let (mut lo, mut hi) = (0, kept.len());
        while lo < hi {
            let mid = (lo + hi) / 2;
            if kept[mid].0 < from { lo = mid + 1 } else { hi = mid }
        }
        kept.truncate(lo);
        for e in excluded {
            kept.push(e);
        }
        Snapshot { revision, layout_revision, status: LayoutStatus::Pending { stale_from: n }, pages, info, excluded: kept }
    }

    /// The pages that may differ from `old`'s, ascending.
    pub fn changed_from(&self, old: &Snapshot) -> Vec<Range<usize>> {
        let mut out = self.pages.changed_from(&old.pages);
        out.extend(self.info.changed_from(&old.info));
        out.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for r in out {
            match merged.last_mut() {
                Some(l) if l.end >= r.start => l.end = l.end.max(r.end),
                _ => merged.push(r),
            }
        }
        merged
    }

    /// The first provisional page: pages from it on are what is left of the layout before.
    pub fn stale_from(&self) -> Option<usize> {
        match self.status {
            LayoutStatus::Pending { stale_from } if stale_from < self.pages.len() => Some(stale_from),
            _ => None,
        }
    }

    pub fn revision(&self) -> Revision {
        self.revision
    }

    pub fn layout_revision(&self) -> LayoutRevision {
        self.layout_revision
    }

    pub fn status(&self) -> &LayoutStatus {
        &self.status
    }

    pub fn pages(&self) -> &Pvec<Rc<PageFragment>> {
        &self.pages
    }

    pub fn info(&self) -> &Pvec<PageInfo> {
        &self.info
    }

    pub fn page(&self, i: usize) -> Option<(&Rc<PageFragment>, &PageInfo)> {
        Some((self.pages.get(i)?, self.info.get(i)?))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn excluded(&self) -> &Pvec<(usize, ParaId, Range<Unit>)> {
        &self.excluded
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::super::fragment::PageFragment;
    use super::super::ids::{FragmentId, LayoutRevision, Revision};
    use super::*;

    fn pages(n: usize) -> Vec<(Rc<PageFragment>, PageMeta)> {
        (0..n)
            .map(|i| {
                let page = PageFragment {
                    id: FragmentId::next(),
                    w: 612.0,
                    h: 792.0,
                    body_area: None,
                    children: Box::new([]),
                    borders: Box::new([]),
                    borders_behind: false,
                };
                (Rc::new(page), PageMeta { section: 0, number: i as i64 + 1, section_page: i, blank: false })
            })
            .collect()
    }

    fn provisional(status: LayoutStatus) -> Vec<bool> {
        let s = Snapshot::new(Revision(1), LayoutRevision(1), status, pages(4), Vec::new());
        assert_eq!(s.info().len(), s.pages().len());
        s.info().iter().map(|i| i.provisional).collect()
    }

    #[test]
    fn provisional_flags_follow_the_status() {
        assert_eq!(provisional(LayoutStatus::Pending { stale_from: 0 }), [true, true, true, true]);
        assert_eq!(provisional(LayoutStatus::Pending { stale_from: 2 }), [false, false, true, true]);
        assert_eq!(provisional(LayoutStatus::Pending { stale_from: 9 }), [false, false, false, false]);
        assert_eq!(provisional(LayoutStatus::Converged), [false, false, false, false]);
    }
}
