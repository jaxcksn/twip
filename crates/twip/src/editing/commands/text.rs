//! Text commands

use crate::api::command::{DeleteOptions, DeleteUnit, InsertSymbol, SectionBreakKind};
use crate::api::value::Direction;
use crate::docx::{BreakKind, Inline};
use crate::font::Fonts;

use super::{Dir, Outcome};
use crate::editing::Editor;

impl Editor {
    pub(in crate::editing) fn line_break(&mut self, fonts: &Fonts) -> Outcome {
        self.insert_inline(|run| Inline::Break(BreakKind::Line, run), "Line Break", fonts)
    }

    pub(in crate::editing) fn section_break(&mut self, _kind: SectionBreakKind) -> Outcome {
        Outcome::unsupported("section breaks")
    }

    pub(in crate::editing) fn tab(&mut self, fonts: &Fonts) -> Outcome {
        self.insert_inline(Inline::Tab, "Typing", fonts)
    }

    pub(in crate::editing) fn symbol(&mut self, s: InsertSymbol, fonts: &Fonts) -> Outcome {
        if let Some(font) = s.font.as_deref().filter(|f| !f.is_empty()) {
            self.pending_font(font);
        }
        self.insert_text(&s.char.to_string(), fonts)
    }

    pub(in crate::editing) fn delete_by(&mut self, d: DeleteOptions, fonts: &Fonts) -> Outcome {
        let dir = if d.direction == Direction::Backward { Dir::Back } else { Dir::Forward };
        self.delete(dir, d.unit == Some(DeleteUnit::Word), fonts)
    }
}
