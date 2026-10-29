//! Clipboard and history: cut, copy, paste, undo, redo and repeat.

use crate::api::command::{PasteAs, PasteOptions};
use crate::font::Fonts;

use super::{Dir, Outcome};
use crate::editing::selection::Selection;
use crate::editing::{Editor, table};

impl Editor {
    pub(in crate::editing) fn copy_selection(&mut self) -> Outcome {
        match self.copy() {
            Some(_) => Outcome::Moved,
            None => Outcome::Unchanged("nothing selected"),
        }
    }

    pub(in crate::editing) fn cut(&mut self, fonts: &Fonts) -> Outcome {
        if self.copy().is_none() {
            return Outcome::Unchanged("nothing selected");
        }
        // A cell block: whole rows, columns or the table go; other blocks are emptied.
        if let Selection::Cells { table, anchor, focus, .. } = self.selection {
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
        // With tracking, whole paragraphs or a sentence cut are a move once pasted: what was selected goes.
        let moving = self.tracking() && self.moves_whole();
        // Smart cut: the space the cut would leave doubled goes too.
        if !moving {
            self.smart_cut();
        }
        let out = self.delete(Dir::Back, false, fonts);
        self.cut_move = match (&out, &self.clipboard, &self.recorded.1) {
            (Outcome::Changed(_), Some(f), Some(m)) if moving => Some((f.id, m.clone())),
            _ => None,
        };
        out
    }

    /// Pastes what was last copied or cut here, with its formatting or as text.
    // shortcut: the system clipboard is the host's; a host pastes what it read there through `Session::paste`.
    pub(in crate::editing) fn paste_held(&mut self, p: PasteOptions, fonts: &Fonts) -> Outcome {
        let Some(f) = self.clipboard.clone() else { return Outcome::unavailable("nothing to paste") };
        if p.as_ == Some(PasteAs::Text) {
            return self.paste_text(&f.text(), fonts);
        }
        let out = self.paste_fragment(&f, fonts);
        self.paste_moved(&f, &out, fonts);
        out
    }

    /// Plain text, with the spaces a pasted word needs beside its neighbours.
    pub(in crate::editing) fn paste_text(&mut self, t: &str, fonts: &Fonts) -> Outcome {
        self.history.break_typing();
        let (sb, sa) = self.smart_spaces(t);
        let t = format!("{}{t}{}", if sb { " " } else { "" }, if sa { " " } else { "" });
        let r = self.insert_text(&t, fonts);
        self.history.break_typing();
        r
    }

    /// F4: redo, or with nothing to redo, the last insertion or break again.
    pub(in crate::editing) fn repeat_last(&mut self, fonts: &Fonts) -> Outcome {
        if self.history.redo_depth() > 0 {
            return self.undo(false, fonts);
        }
        match self.repeat.clone() {
            Some(c) => self.run(c, fonts),
            None => Outcome::Unchanged("nothing to repeat"),
        }
    }
}
