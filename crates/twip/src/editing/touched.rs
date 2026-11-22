//! Touched paragraphs

use std::sync::atomic::{AtomicU64, Ordering};

use crate::layout::ParaId;

/// Entries kept: past them, readers start again from the whole document.
const LIMIT: usize = 1 << 16;

/// A place in an editor's log of touched paragraphs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditMark {
    editor: u64,
    at: u64,
}

/// The paragraphs edits touched, oldest first; `start` counts the entries dropped.
pub(super) struct Touched {
    editor: u64,
    start: u64,
    paras: Vec<ParaId>,
}

impl Touched {
    pub(super) fn new() -> Touched {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Touched { editor: NEXT.fetch_add(1, Ordering::Relaxed), start: 0, paras: Vec::new() }
    }

    pub(super) fn note(&mut self, paras: impl ExactSizeIterator<Item = ParaId>) {
        let n = paras.len();
        if self.paras.len() + n > LIMIT {
            self.everything();
        }
        if n > LIMIT {
            self.start += n as u64;
            return;
        }
        self.paras.extend(paras);
    }

    /// Anything may have changed: every mark before now is stale.
    pub(super) fn everything(&mut self) {
        self.start += self.paras.len() as u64 + 1;
        self.paras.clear();
    }

    pub(super) fn mark(&self) -> EditMark {
        EditMark { editor: self.editor, at: self.start + self.paras.len() as u64 }
    }

    pub(super) fn since(&self, mark: EditMark) -> Option<&[ParaId]> {
        if mark.editor != self.editor || mark.at < self.start {
            return None;
        }
        self.paras.get((mark.at - self.start) as usize..)
    }
}
