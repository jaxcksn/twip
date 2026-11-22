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

    /// A section break at the selection: its paragraph splits there, the text before it ends a new section with the
    /// page setup the section had (and how it started), and the section after starts as `kind` says.
    pub(in crate::editing) fn section_break(&mut self, kind: SectionBreakKind, fonts: &Fonts) -> Outcome {
        use crate::docx::SectionBreak as B;
        use crate::editing::position::{Place, Position};
        use crate::editing::step::{SectionBack, Step};
        if self.style_target.is_some() {
            return Outcome::unavailable("not in a style");
        }
        if self.refuse_expanded() || self.refuse_locked() {
            return Outcome::Unchanged("locked");
        }
        let mut tx = self.begin("Section Break");
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => p,
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let Place::Text { para, offset, .. } = caret.place else {
            self.abort(tx);
            return Outcome::unavailable("not in text");
        };
        let slot = self.index.para_slot(para);
        let Some((section, block)) = slot.and_then(|s| match s.container {
            crate::editing::index::Container::Section(k) => Some((k, s.block)),
            _ => None,
        }) else {
            self.abort(tx);
            return Outcome::unavailable("a section break goes in the body's text, not a table, box, note or header");
        };
        let (Some(second), Some(s)) = (self.split_template(para), self.doc.sections.get(section)) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let new = s.without_blocks();
        let mut after = s.page();
        after.pg_start = None;
        after.break_kind = match kind {
            SectionBreakKind::NextPage => B::NextPage,
            SectionBreakKind::Continuous => B::Continuous,
            SectionBreakKind::EvenPage => B::EvenPage,
            SectionBreakKind::OddPage => B::OddPage,
            SectionBreakKind::NextColumn => B::NextColumn,
        };
        let change = s.change.clone();
        let next = crate::layout::ParaId(second.id);
        let back = Box::new(SectionBack { blocks: block + 1, section: new, sources: 0, edited: true });
        let ok = self.step(&mut tx, Step::Split { para, at: offset, second, kept: Vec::new() }).is_ok()
            && self.step(&mut tx, Step::SectionBreak { section, back: Some(back) }).is_ok()
            && self.step(&mut tx, Step::SectionPage { section: section + 1, page: Box::new(after), change }).is_ok();
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("stale selection");
        }
        self.selection = crate::editing::selection::Selection::Caret(Position::text(caret.story, next, crate::layout::Unit(0)));
        self.commit(tx, None, fonts)
    }

    pub(in crate::editing) fn tab(&mut self, fonts: &Fonts) -> Outcome {
        self.insert_inlines_typed(|run| vec![Inline::Tab(run)], "Typing", true, fonts)
    }

    pub(in crate::editing) fn symbol(&mut self, s: InsertSymbol, fonts: &Fonts) -> Outcome {
        // The font is the symbol's alone: typing after it goes on as before.
        let was = self.pending.clone();
        if let Some(font) = s.font.as_deref().filter(|f| !f.is_empty()) {
            self.pending_font(font);
        }
        let out = self.insert_text(&s.char.to_string(), fonts);
        self.history.break_typing();
        self.pending = was;
        out
    }

    pub(in crate::editing) fn delete_by(&mut self, d: DeleteOptions, fonts: &Fonts) -> Outcome {
        let dir = if d.direction == Direction::Backward { Dir::Back } else { Dir::Forward };
        self.delete(dir, d.unit == Some(DeleteUnit::Word), fonts)
    }
}
