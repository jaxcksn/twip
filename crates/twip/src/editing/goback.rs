//! Go Back (Shift+F5): the last edit locations, mapped through every later edit, undo and redo;
//! Shift+F5 moves to the previous one, and pressed again to the one before (round the last four).

use super::Editor;
use super::commands::Outcome;
use super::position::Position;
use super::selection::Selection;
use super::step::{Map, para_len};

/// How many edit locations are kept.
const KEPT: usize = 4;

impl Editor {
    /// Edit locations moved through a transaction's (or an undo's) position maps.
    pub(super) fn map_marks(&mut self, maps: &[Map]) {
        for m in maps {
            for p in self.edit_marks.iter_mut() {
                *p = m.apply(*p);
            }
        }
    }

    /// After an edit: where it left the caret is the newest edit location.
    pub(super) fn mark_edit(&mut self) {
        let Some(p) = self.selection.focus() else { return };
        // The same paragraph again (typing on): the location moves rather than adding one.
        if let Some(last) = self.edit_marks.last_mut()
            && last.para_offset().map(|(q, _)| q) == p.para_offset().map(|(q, _)| q)
        {
            *last = p;
            return;
        }
        self.edit_marks.push(p);
        if self.edit_marks.len() > KEPT {
            self.edit_marks.remove(0);
        }
    }

    /// Shift+F5: the caret to the previous edit location; repeated, to the ones before.
    pub(super) fn go_back(&mut self, fonts: &crate::font::Fonts) -> Outcome {
        let here = self.selection.focus();
        let valid = |e: &Self, p: &Position| match p.para_offset() {
            Some((para, u)) => e.index.para(&e.doc, para).is_some_and(|q| u <= para_len(q)),
            None => false,
        };
        // Newest first, then where the caret was when Go Back started, then round again.
        if self.back_step.is_none() {
            self.back_from = here.filter(|h| !self.edit_marks.contains(h));
        }
        let mut marks: Vec<Position> = self.edit_marks.iter().rev().filter(|p| valid(self, p)).copied().collect();
        marks.extend(self.back_from.filter(|p| valid(self, p)));
        if marks.is_empty() || (marks.len() == 1 && Some(marks[0]) == here) {
            return Outcome::Unchanged("no earlier edit");
        }
        let k = match self.back_step {
            Some(k) => (k + 1) % marks.len(),
            None if Some(marks[0]) == here => 1,
            None => 0,
        };
        let target = marks[k];
        self.set_selection(Selection::Caret(target));
        self.back_step = Some(k);
        self.ensure(&target, fonts);
        self.scroll_to_caret(fonts);
        Outcome::Moved
    }
}
