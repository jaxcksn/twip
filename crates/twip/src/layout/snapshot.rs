//! Layout snapshots

use std::ops::Range;
use std::rc::Rc;

use super::fragment::PageFragment;
use super::ids::{LayoutRevision, ParaId, Revision, Unit};

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
    pages: Box<[Rc<PageFragment>]>,
    info: Box<[PageInfo]>,
    #[cfg_attr(not(test), allow(dead_code))]
    excluded: Box<[(usize, ParaId, Range<Unit>)]>,
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
            pages: pages.into_boxed_slice(),
            info: info.into_boxed_slice(),
            excluded: excluded.into_boxed_slice(),
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

    pub fn pages(&self) -> &[Rc<PageFragment>] {
        &self.pages
    }

    pub fn info(&self) -> &[PageInfo] {
        &self.info
    }

    pub fn page(&self, i: usize) -> Option<(&Rc<PageFragment>, &PageInfo)> {
        Some((self.pages.get(i)?, self.info.get(i)?))
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn excluded(&self) -> &[(usize, ParaId, Range<Unit>)] {
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
                let page = PageFragment { id: FragmentId::next(), w: 612.0, h: 792.0, body_area: None, children: Box::new([]) };
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
