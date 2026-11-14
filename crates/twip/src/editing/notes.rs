//! Footnotes and endnotes

use std::rc::Rc;

use crate::docx::write::props::Props;
use crate::docx::{Block, Inline, NoteKind, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::position::{Place, Position, StoryId};
use super::selection::Selection;
use super::step::Step;
use super::{Editor, Tx};

fn story(kind: NoteKind, id: i64) -> StoryId {
    match kind {
        NoteKind::Foot => StoryId::Footnote(id),
        NoteKind::End => StoryId::Endnote(id),
    }
}

fn refs_in(blocks: &[Block], out: &mut Vec<(NoteKind, i64)>) {
    for b in blocks {
        match b {
            Block::Para(p) => refs_inline(&p.inlines, out),
            Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| refs_in(&c.blocks, out)),
        }
    }
}

fn refs_inline(inlines: &[Inline], out: &mut Vec<(NoteKind, i64)>) {
    for i in inlines {
        if let Inline::NoteRef { kind, id, .. } = i {
            out.push((*kind, *id));
        }
    }
}

impl Editor {
    pub(super) fn insert_note(&mut self, kind: NoteKind, fonts: &Fonts) -> Outcome {
        if self.selection.focus().map(|p| p.story) != Some(StoryId::Body) {
            return Outcome::Unchanged("notes are inserted in the body");
        }
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let w = cx.w.clone();
        let (ref_style, text_style) = match kind {
            NoteKind::Foot => ("FootnoteReference", "FootnoteText"),
            NoteKind::End => ("EndnoteReference", "EndnoteText"),
        };
        let has = |s: &str| res.styles.by_id.contains_key(s);
        let mark_prop = |p: &mut Props| {
            if has(ref_style) {
                p.set("rStyle", format!("<{w}rStyle {w}val=\"{ref_style}\"/>"));
            } else {
                p.set("vertAlign", format!("<{w}vertAlign {w}val=\"superscript\"/>"));
            }
        };
        let notes = match kind {
            NoteKind::Foot => &self.doc.footnotes,
            NoteKind::End => &self.doc.endnotes,
        };
        let id = notes.keys().copied().max().unwrap_or(0).max(0) + 1;
        let mut tx = self.begin(if kind == NoteKind::Foot { "Insert Footnote" } else { "Insert Endnote" });
        let caret = match self.delete_range(&mut tx) {
            Ok(Some(p)) => self.past_row_end(p),
            _ => {
                self.abort(tx);
                return Outcome::Unchanged("no caret");
            }
        };
        let Place::Text { para, offset, .. } = caret.place else {
            self.abort(tx);
            return Outcome::Unchanged("caret at a row end");
        };
        let (Some(run), Some(base)) = (self.run_at(&caret), self.run_base(para, &res, &cx)) else {
            self.abort(tx);
            return Outcome::Unchanged("no paragraph");
        };
        let mut p = Props::parse(run.direct.0.as_deref(), "rPr", &cx);
        mark_prop(&mut p);
        let ref_run = res.run_from_xml(&base, p.xml().as_deref(), &cx);
        let reference = Inline::NoteRef { kind, id, custom: false, run: ref_run };
        if self.step(&mut tx, Step::Text { para, range: offset..offset, with: vec![reference], edges: Default::default() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale caret");
        }
        let mut pp = Props::parse(None, "pPr", &cx);
        if has(text_style) {
            pp.set("pStyle", format!("<{w}pStyle {w}val=\"{text_style}\"/>"));
        }
        let r = res.para_from_xml(pp.xml().as_deref(), None, &cx);
        let text_run = res.run_from_xml(&r.base, None, &cx);
        let mut mp = Props::parse(None, "rPr", &cx);
        mark_prop(&mut mp);
        let mark_run = res.run_from_xml(&r.base, mp.xml().as_deref(), &cx);
        let pid = crate::docx::next_para_id();
        let note = Paragraph {
            style: Rc::new(r.style),
            mark: text_run.clone(),
            list: None,
            inlines: vec![Inline::NoteMark(kind, id, mark_run), Inline::Text(" ".into(), text_run)],
            src: None,
            id: pid,
            div: None,
            controls: Vec::new(),
        };
        if self.step(&mut tx, Step::Note { kind, id, blocks: Some(vec![Block::Para(note)]) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("note failed");
        }
        self.selection = Selection::Caret(Position::text(story(kind, id), ParaId(pid), Unit(2)));
        self.commit(tx, None, fonts)
    }

    pub(super) fn drop_orphan_notes(&mut self, tx: &mut Tx) {
        let mut gone = Vec::new();
        for s in &tx.undo {
            match s {
                Step::Text { with, .. } => refs_inline(with, &mut gone),
                Step::Blocks { with, .. } => refs_in(with, &mut gone),
                Step::Rows { with, .. } => with.iter().flat_map(|r| r.cells.iter()).for_each(|c| refs_in(&c.blocks, &mut gone)),
                _ => {}
            }
        }
        if gone.is_empty() {
            return;
        }
        gone.sort_by_key(|&(k, id)| (k == NoteKind::End, id));
        gone.dedup();
        let mut left = Vec::new();
        for pid in self.index.para_ids() {
            if let Some(p) = self.index.para(&self.doc, pid) {
                refs_inline(&p.inlines, &mut left);
            }
        }
        for (kind, id) in gone {
            let exists = match kind {
                NoteKind::Foot => self.doc.footnotes.contains_key(&id),
                NoteKind::End => self.doc.endnotes.contains_key(&id),
            };
            if exists && !left.contains(&(kind, id)) {
                let _ = self.step(tx, Step::Note { kind, id, blocks: None });
            }
        }
    }
}

impl Editor {
    pub(super) fn drop_orphan_controls(&mut self, tx: &mut Tx) {
        use crate::docx::{Block, ControlId, ElementId, Inline};
        fn inlines(v: &[Inline], out: &mut Vec<ControlId>) {
            for i in v {
                match i {
                    Inline::ControlStart(c) | Inline::ControlEnd(c) | Inline::Sym { control: Some(c), .. } => out.push(*c),
                    i if i.story().is_some() => {
                        if let Some((_, tb)) = i.story() {
                            blocks(&tb.blocks, out);
                        }
                    }
                    _ => {}
                }
            }
        }
        fn blocks(v: &[Block], out: &mut Vec<ControlId>) {
            for b in v {
                match b {
                    Block::Para(p) => {
                        out.extend(p.controls.iter().copied());
                        inlines(&p.inlines, out);
                    }
                    Block::Table(t) => {
                        out.extend(t.controls.iter().copied());
                        for r in &t.rows {
                            out.extend(r.controls.iter().copied());
                            for c in &r.cells {
                                out.extend(c.controls.iter().copied());
                                blocks(&c.blocks, out);
                            }
                        }
                    }
                }
            }
        }
        let mut gone = Vec::new();
        for s in &tx.undo {
            match s {
                Step::Text { with, .. } | Step::Restyle { inlines: with, .. } => inlines(with, &mut gone),
                Step::Blocks { with, .. } => blocks(with, &mut gone),
                Step::Rows { with, .. } => {
                    for r in with {
                        gone.extend(r.controls.iter().copied());
                        for c in &r.cells {
                            gone.extend(c.controls.iter().copied());
                            blocks(&c.blocks, &mut gone);
                        }
                    }
                }
                Step::Note { blocks: Some(b), .. } | Step::HeaderPart { blocks: Some(b), .. } => blocks(b, &mut gone),
                Step::Control { id, control: None } => gone.push(*id),
                _ => {}
            }
        }
        gone.sort();
        gone.dedup();
        for c in gone {
            if self.index.element(ElementId::Control(c)).is_none() && self.doc.controls.contains_key(&c) {
                let _ = self.step(tx, Step::Control { id: c, control: None });
            }
        }
    }
}
