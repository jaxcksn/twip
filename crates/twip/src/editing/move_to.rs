//! Moves without a left-button drag: F2 and Shift+F2, the right-button drag's menu; the highlighter

use crate::api::command::{Command, CommandId, MoveSelection};
use crate::api::value::HighlightColor;
use crate::docx::Inline;
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::format::Format;
use super::position::Position;
use super::selection::Selection;

/// What the menu a right-button drag of text opens at its drop place does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropChoice {
    /// Move Here.
    Move,
    /// Copy Here.
    Copy,
    /// Link Here: a `LINK` field to the text, bookmarked where it is.
    Link,
    /// Create Hyperlink Here: a link to a hidden bookmark on the text, showing the text.
    Hyperlink,
    /// Cancel.
    Cancel,
}

/// F2 (move) or Shift+F2 (copy) waiting for its place: the next click or arrow keys choose it, Enter moves the text.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MoveTo {
    pub copy: bool,
    pub target: Option<Position>,
}

impl Editor {
    /// F2 (Shift+F2: `copy`): the selected text moves (is copied) to the place the next click or arrow keys choose
    /// when Enter is pressed; Esc or any other command gives up.
    pub fn begin_move_to(&mut self, copy: bool) -> Outcome {
        if !matches!(self.selection, Selection::Text { .. }) || self.selection.is_collapsed() {
            return Outcome::unavailable("no text selected");
        }
        self.move_to = Some(MoveTo { copy, target: None });
        Outcome::Moved
    }

    /// The F2 or Shift+F2 move waiting for its place.
    pub fn move_to_pending(&self) -> Option<MoveTo> {
        self.move_to
    }

    /// A press while F2 waits: the place under it is the target, the selection kept.
    pub(super) fn move_to_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(m) = self.move_to else { return false };
        let target = self.hit(x, y, fonts);
        self.move_to = Some(MoveTo { target: target.or(m.target), ..m });
        true
    }

    /// A command while F2 waits: Enter moves the text, the arrow keys move the target, Esc gives up; anything else
    /// gives up and runs (`None`).
    pub(super) fn move_to_command(&mut self, cmd: &Command, fonts: &Fonts) -> Option<Outcome> {
        let m = self.move_to?;
        match cmd {
            Command::InsertParagraph => {
                self.move_to = None;
                let Some(target) = m.target else { return Some(Outcome::Unchanged("no place chosen")) };
                Some(self.drop_at(target, m.copy, fonts))
            }
            Command::MoveSelection(mv) => {
                let keep = self.selection;
                let from = m.target.or_else(|| self.selection.focus());
                if let Some(from) = from {
                    self.selection = Selection::Caret(from);
                }
                let modes = std::mem::take(&mut self.modes);
                let _ = self.move_selection(MoveSelection { extend: None, ..mv.clone() }, fonts);
                self.modes = modes;
                let target = self.caret();
                self.selection = keep;
                self.move_to = Some(MoveTo { target: target.or(m.target), ..m });
                Some(Outcome::Moved)
            }
            Command::Escape => {
                self.move_to = None;
                Some(Outcome::Moved)
            }
            _ => {
                self.move_to = None;
                None
            }
        }
    }

    /// The selected text dropped at `target`, as a drag ends: refused where the text is kept or can't go.
    fn drop_at(&mut self, target: Position, copy: bool, fonts: &Fonts) -> Outcome {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        if (!copy && self.kept(CommandId::Delete, &a, &b)) || !self.editable(&target) || self.kept(CommandId::Paste, &target, &target) {
            return Outcome::Unchanged("kept");
        }
        self.drop_selection(target, copy, None, fonts)
    }

    /// A right-button press inside the selected text starts a drag that ends in the drop menu, while text drag and
    /// drop is on, the document editable and no extend mode on.
    pub fn right_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        self.drop_menu = None;
        if !self.text_drags() {
            return false;
        }
        let Some(p) = self.hit(x, y, fonts).filter(|p| self.inside_selection(p)) else { return false };
        self.text_drag = Some(super::interaction::TextDrag { start: (x, y), press: p, moved: false, copy: false, drop: None, menu: true });
        self.requests.push(super::input::Request::CapturePointer);
        true
    }

    /// The right button's release: where its drag dropped the text, for the drop menu to open at; `None` when it
    /// didn't drag (the context menu's click) or dropped on the selection.
    pub fn right_release(&mut self) -> Option<Position> {
        let d = self.text_drag.take_if(|d| d.menu)?;
        self.requests.push(super::input::Request::ReleasePointer);
        let drop = d.drop.filter(|p| d.moved && !self.inside_selection(p))?;
        self.drop_menu = Some((drop, self.revision));
        Some(drop)
    }

    /// Where the open drop menu drops the text; `None` once the document changed after it opened.
    pub fn drop_menu(&self) -> Option<Position> {
        self.drop_menu.filter(|&(_, at)| at == self.revision).map(|(p, _)| p)
    }

    /// The drop menu's choice, as one undo step. Link Here needs the document's file name.
    pub fn choose_drop(&mut self, choice: DropChoice, fonts: &Fonts) -> Outcome {
        let Some((drop, at)) = self.drop_menu.take() else { return Outcome::unavailable("no drop menu open") };
        if at != self.revision {
            return Outcome::Unchanged("the document changed");
        }
        match choice {
            DropChoice::Move | DropChoice::Copy => self.drop_at(drop, choice == DropChoice::Copy, fonts),
            DropChoice::Link => self.link_drop(drop, fonts),
            DropChoice::Hyperlink => self.hyperlink_here(drop, fonts),
            DropChoice::Cancel => Outcome::Moved,
        }
    }

    /// Create Hyperlink Here: a hidden bookmark (`_Hlk…`) on the selected text, and at `drop` a link to it showing
    /// the text. One undo step.
    fn hyperlink_here(&mut self, drop: Position, fonts: &Fonts) -> Outcome {
        use crate::api::command::Hyperlink;
        let Some(source) = self.selection.ordered(&self.index) else { return Outcome::Unchanged("no selection") };
        let text = self.selected_text();
        if text.is_empty() || !self.editable(&drop) || self.kept(CommandId::InsertHyperlink, &drop, &drop) {
            return Outcome::Unchanged("kept");
        }
        let names: std::collections::HashSet<String> = self.bookmarks().into_iter().map(|b| b.name.to_ascii_lowercase()).collect();
        let first = 100_000_000 + self.generation % 800_000_000;
        let Some(name) = (first..first + 10_000).map(|n| format!("_Hlk{n}")).find(|n| !names.contains(&n.to_ascii_lowercase())) else {
            return Outcome::Unchanged("no bookmark name");
        };
        self.begin_batch(Some("Create Hyperlink".into()));
        self.set_selection(Selection::range(source.0, source.1, super::selection::Granularity::Char));
        let mut ok = matches!(self.execute(super::commands::Command::InsertBookmark(name.clone()), fonts), Outcome::Changed(_));
        self.set_selection(Selection::Caret(drop));
        let link = Hyperlink { address: format!("#{name}"), text: Some(text) };
        ok &= matches!(self.execute(super::commands::Command::InsertHyperlink(link), fonts), Outcome::Changed(_));
        self.end_batch(ok, fonts)
    }

    /// Link Here: the selection bookmarked (`OLE_LINKn`), and at `drop` a `LINK` field to it showing its text. One
    /// undo step.
    fn link_drop(&mut self, drop: Position, fonts: &Fonts) -> Outcome {
        let Some(file) = self.doc.file_name.clone() else { return Outcome::unavailable("the document has no file name to link to") };
        if !self.editable(&drop) || self.kept(CommandId::InsertField, &drop, &drop) {
            return Outcome::Unchanged("kept");
        }
        let text = self.selected_text();
        if text.is_empty() || text.contains('\n') {
            return Outcome::unavailable("a link is to text in one paragraph");
        }
        let used = self
            .index
            .para_ids()
            .filter_map(|p| self.index.para(&self.doc, p))
            .flat_map(|p| p.inlines.iter())
            .filter_map(|i| match i {
                Inline::Bookmark(n) => n.strip_prefix("OLE_LINK").and_then(|k| k.parse::<u32>().ok()),
                _ => None,
            })
            .max()
            .unwrap_or(0);
        let name = format!("OLE_LINK{}", used + 1);
        let quote = crate::docx::fields::quote;
        let code = format!("LINK Word.Document.12 {} {} \\a \\r", quote(&file), quote(&name));
        self.joined("Link", fonts, |e| {
            let marked = e.insert_bookmark(&name, fonts);
            if !matches!(marked, Outcome::Changed(_)) {
                return marked;
            }
            e.selection = Selection::Caret(drop);
            e.insert_field(crate::api::command::InsertField { code, display: Some(text), show_code: None }, fonts)
        })
    }

    /// The highlighter: with no text selected, Highlight turns the pointer into a pen that highlights the text it
    /// drags over in `color`; again (or Esc) turns it off.
    pub fn toggle_highlighter(&mut self, color: Option<HighlightColor>) -> Outcome {
        self.highlighter = match (self.highlighter.take(), color) {
            (None, Some(c)) => Some(super::commands::highlight_value(c).to_string()),
            _ => None,
        };
        Outcome::Moved
    }

    /// The highlighter's colour, while it is on.
    pub fn highlighter(&self) -> Option<&str> {
        self.highlighter.as_deref()
    }

    /// The highlighter's drag ends: the text it went over is highlighted, the caret at its end.
    pub(super) fn highlighter_release(&mut self, fonts: &Fonts) {
        let Some(name) = self.highlighter.clone() else { return };
        if self.selection.is_collapsed() || !matches!(self.selection, Selection::Text { .. }) {
            return;
        }
        let end = self.selection.focus();
        let _ = self.format(&Format::Highlight(Some(name)), fonts);
        if let Some(end) = end {
            self.set_selection(Selection::Caret(end));
        }
    }
}
