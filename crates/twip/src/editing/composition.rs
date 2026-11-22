//! IME composition: the text being composed shows at the caret as an edit the history and records never keep

use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::Editor;
use super::commands::{Command, Outcome};
use super::position::Position;
use super::selection::{Granularity, Selection};

/// The composition showing: its text's units in a paragraph, and the transaction that put it there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Composition {
    para: ParaId,
    start: Unit,
    end: Unit,
    tx: u64,
}

impl Editor {
    /// Shows `text` as the composition at the selection (replacing it, the first time), with the caret `cursor`
    /// characters into it. Empty text shows nothing.
    pub fn compose(&mut self, text: &str, cursor: u32, fonts: &Fonts) -> Outcome {
        self.end_composition(fonts);
        if text.is_empty() {
            return Outcome::Moved;
        }
        self.history.break_typing();
        let out = self.execute(Command::InsertText(text.to_string()), fonts);
        self.history.break_typing();
        let Outcome::Changed(_) = out else { return out };
        let Some(caret) = self.selection.focus() else { return out };
        let Some((para, end)) = caret.para_offset() else { return out };
        let len = text.chars().count() as u32;
        let start = Unit(end.0.saturating_sub(len));
        self.composition = Some(Composition { para, start, end, tx: self.history.position() });
        let at = Position::text(caret.story, para, Unit(start.0 + cursor.min(len)));
        self.selection = Selection::range(at, at, Granularity::Char);
        out
    }

    /// Takes the composition showing out again, as if it had never been typed.
    pub fn end_composition(&mut self, fonts: &Fonts) {
        let Some(c) = self.composition.take() else { return };
        if self.history.position() == c.tx && matches!(self.execute(Command::Undo, fonts), Outcome::Changed(_)) {
            self.history.pop_redo();
        }
    }

    /// Whether a composition is showing.
    pub fn composing(&self) -> bool {
        self.composition.is_some()
    }

    /// The composition's rects on `pages`, to underline.
    pub(super) fn composition_rects(&mut self, pages: std::ops::Range<usize>) -> Vec<(usize, [f64; 4])> {
        let Some(c) = self.composition else { return Vec::new() };
        let Some(story) = self.index.para_slot(c.para).map(|s| self.index.story(s.container)) else { return Vec::new() };
        self.range_rects(Position::text(story, c.para, c.start), Position::text(story, c.para, c.end), pages)
    }
}
