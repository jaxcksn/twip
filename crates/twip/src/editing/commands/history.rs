//! Clipboard and history

use crate::api::command::{PasteAs, PasteOptions};
use crate::font::Fonts;

use super::{Dir, Outcome};
use crate::editing::selection::Selection;
use crate::editing::{Editor, table};

impl Editor {
    pub(in crate::editing) fn copy_selection(&mut self, fonts: &Fonts) -> Outcome {
        match self.copy_drawn(fonts) {
            Some(_) => Outcome::Moved,
            None => Outcome::Unchanged("nothing selected"),
        }
    }

    pub(in crate::editing) fn cut(&mut self, fonts: &Fonts) -> Outcome {
        if self.copy_drawn(fonts).is_none() {
            return Outcome::Unchanged("nothing selected");
        }
        if !self.is_discontiguous()
            && let Selection::Cells { table, anchor, focus, .. } = self.selection
        {
            let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
            let cols = table::Grid::of(t).cols;
            let rows = t.rows.len();
            let all_rows = anchor.row.min(focus.row) == 0 && anchor.row.max(focus.row) + 1 >= rows;
            let all_cols = anchor.col.min(focus.col) == 0 && anchor.col.max(focus.col) + 1 >= cols;
            return match (all_rows, all_cols) {
                (true, true) => self.delete_table(fonts),
                (false, true) => self.delete_rows(fonts),
                (true, false) => self.delete_columns(fonts),
                (false, false) => self.delete(Dir::Forward, false, fonts),
            };
        }
        let moving = self.tracking() && !self.is_discontiguous() && self.moves_whole();
        if !moving && !self.is_discontiguous() && self.paste_defaults.spaces() {
            self.smart_cut();
        }
        let out = self.delete(Dir::Back, false, fonts);
        self.cut_move = match (&out, &self.clipboard) {
            (Outcome::Changed(_), Some(f)) if moving && !self.recorded.1.is_empty() => Some((f.id, self.recorded.1.clone())),
            _ => None,
        };
        out
    }

    pub(in crate::editing) fn paste_held(&mut self, p: PasteOptions, fonts: &Fonts) -> Outcome {
        let Some(f) = self.clipboard.clone() else { return Outcome::unavailable("nothing to paste") };
        let choice = (p.as_ == Some(PasteAs::Text)).then_some(crate::editing::paste::PasteChoice::Unformatted);
        let clip = crate::editing::paste::Clip { text: f.text(), fragment: Some(f), ..Default::default() };
        self.paste_clip(clip, choice, fonts)
    }

    pub(in crate::editing) fn paste_text(&mut self, t: &str, fonts: &Fonts) -> Outcome {
        self.history.break_typing();
        let (sb, sa) = if self.paste_defaults.spaces() { self.smart_spaces(t) } else { (false, false) };
        let t = format!("{}{t}{}", if sb { " " } else { "" }, if sa { " " } else { "" });
        let r = self.insert_text(&t, fonts);
        self.history.break_typing();
        r
    }

    pub(in crate::editing) fn repeat_last(&mut self, fonts: &Fonts) -> Outcome {
        if self.history.redo_depth() > 0 {
            return self.undo(false, fonts);
        }
        match self.repeat.clone() {
            Some(c) if self.is_discontiguous() && crate::editing::multi::single_only(c.id()) => {
                Outcome::unavailable("not available for multiple selections")
            }
            Some(c) => match self.protection_refusal(&c) {
                Some(e) => Outcome::Refused(e),
                None => self.run(c, fonts),
            },
            None => Outcome::Unchanged("nothing to repeat"),
        }
    }
}
