//! Paragraphs laid out as one when the markup view hides the paragraph mark between them (REV-6).
//!
//! A chain of such paragraphs is laid out as one synthetic paragraph: their inlines in order, each hidden mark kept as
//! one hidden unit between them, with the last paragraph's properties and mark. Its pieces' sources are units of the
//! joined paragraph; a [`JoinMap`] turns them back into each paragraph's own units, so drawn lines name the paragraphs
//! and offsets of the document.

use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::{Block, Inline, NoteKind, Paragraph, Table};
use crate::docx::review::MarkupView;

use super::env::TextEnv;
use super::ids::{ParaId, Unit};
use super::space::ShapeSpace;
use super::text::{FieldValues, ShapedPara, shape_space};

/// What a hidden mark's unit holds in the joined paragraph: drawn nothing, as the view hides its run.
const MARK: &str = "\u{b6}";

/// Where a joined paragraph's units come from: the first paragraph's start at unit 0, then each later paragraph and
/// the joined unit its own units start at. A paragraph's hidden mark is the joined unit before the next one's start.
#[derive(Clone, Debug, PartialEq)]
pub struct JoinMap {
    first: ParaId,
    rest: Box<[(ParaId, Unit)]>,
}

impl JoinMap {
    /// The paragraph a joined unit belongs to, and its unit there (a hidden mark is its paragraph's end).
    pub fn source(&self, u: Unit) -> (ParaId, Unit) {
        match self.rest[..self.rest.partition_point(|s| s.1 <= u)].last() {
            Some(&(p, start)) => (p, Unit(u.0 - start.0)),
            None => (self.first, u),
        }
    }

    /// The first paragraph.
    pub fn first(&self) -> ParaId {
        self.first
    }

    /// Each paragraph in order, with the joined units it spans: from its first unit up to the next paragraph's first
    /// (`None` for the last).
    pub fn parts(&self) -> impl Iterator<Item = (ParaId, Unit, Option<Unit>)> + '_ {
        let starts = std::iter::once((self.first, Unit(0))).chain(self.rest.iter().copied());
        starts.enumerate().map(|(k, (p, s))| (p, s, self.rest.get(k).map(|r| r.1)))
    }
}

/// Whether `view` hides a block's paragraph mark.
fn hides_mark(b: &Block, view: MarkupView) -> bool {
    matches!(b, Block::Para(p) if view.hides(&p.mark.rev))
}

/// The last block laid out with block `i`: the first paragraph after it whose mark the view shows, over every
/// paragraph between whose mark it hides (REV-6). A chain that meets a table or the end of `blocks` ends at the
/// paragraph before it, whose mark stays. `i` itself when its mark shows or it isn't a paragraph.
pub fn chain_end<B: Borrow<Block>>(blocks: &[B], i: usize, view: MarkupView) -> usize {
    let para = |k: usize| blocks.get(k).is_some_and(|b| matches!(b.borrow(), Block::Para(_)));
    let mut j = i;
    while para(j + 1) && blocks.get(j).is_some_and(|b| hides_mark(b.borrow(), view)) {
        j += 1;
    }
    j
}

/// The first of the paragraphs right before block `i` whose marks the view hides, else `i`: the first block that may
/// lay out with block `i` (whatever block `i` is, so a block that was a paragraph before an edit counts).
pub fn chain_start<B: Borrow<Block>>(blocks: &[B], i: usize, view: MarkupView) -> usize {
    let mut s = i.min(blocks.len());
    while s > 0 && hides_mark(blocks[s - 1].borrow(), view) {
        s -= 1;
    }
    s
}

/// Paragraphs laid out as one: the joined paragraph and where its units come from.
#[derive(Debug)]
pub struct Joined {
    pub para: Paragraph,
    pub map: Rc<JoinMap>,
}

impl Joined {
    /// `paras` (two or more, in order) joined: their inlines with each hidden mark as one hidden unit between them,
    /// and the last one's properties, mark, list label and identity (REV-6).
    fn new(paras: &[&Paragraph]) -> Option<Joined> {
        let (&last, before) = paras.split_last()?;
        let &first = before.first()?;
        let mut inlines = Vec::with_capacity(paras.iter().map(|p| p.inlines.len() + 1).sum());
        let mut rest = Vec::with_capacity(before.len());
        let mut at = 0u32;
        for w in paras.windows(2) {
            let (p, next) = (w[0], w[1]);
            inlines.extend(p.inlines.iter().cloned());
            inlines.push(Inline::Text(MARK.to_string(), p.mark.clone()));
            at += p.inlines.iter().map(Inline::units).sum::<usize>() as u32 + 1;
            rest.push((ParaId(next.id), Unit(at)));
        }
        inlines.extend(last.inlines.iter().cloned());
        let para = Paragraph {
            style: last.style.clone(),
            mark: last.mark.clone(),
            // shortcut: list numbers aren't recounted for the view, so the last paragraph's number still counts the
            // numbered paragraphs joined before it; recount numbering per view to fix.
            list: last.list.clone(),
            inlines,
            src: None,
            id: last.id,
            controls: last.controls.clone(),
        };
        Some(Joined { para, map: Rc::new(JoinMap { first: ParaId(first.id), rest: rest.into() }) })
    }
}

/// A paragraph as layout sees it: a block's own, or several joined (REV-6).
#[derive(Clone, Debug)]
pub enum LaidPara<'a> {
    One(&'a Paragraph),
    Joined(Rc<Joined>),
}

impl LaidPara<'_> {
    pub fn para(&self) -> &Paragraph {
        match self {
            LaidPara::One(p) => p,
            LaidPara::Joined(j) => &j.para,
        }
    }

    /// Its [`ShapeSpace`]: the values of its fields and note numbers, and where its units come from.
    pub fn shape_space(&self, fields: &FieldValues, notes: &HashMap<(NoteKind, i64), String>, text: &TextEnv) -> ShapeSpace {
        let mut space = shape_space(self.para(), fields, notes, text);
        if let LaidPara::Joined(j) = self {
            space.join = Some(j.map.clone());
        }
        space
    }
}

/// The paragraph `chain` (consecutive paragraph blocks, as [`chain_end`] finds them) lays out as.
pub fn laid_para<B: Borrow<Block>>(chain: &[B]) -> Option<LaidPara<'_>> {
    let paras: Vec<&Paragraph> = chain.iter().filter_map(|b| if let Block::Para(p) = b.borrow() { Some(p) } else { None }).collect();
    match paras[..] {
        [] => None,
        [p] => Some(LaidPara::One(p)),
        _ => Joined::new(&paras).map(|j| LaidPara::Joined(Rc::new(j))),
    }
}

/// A block as layout sees it.
#[derive(Clone, Debug)]
pub enum LaidBlock<'a> {
    Para(LaidPara<'a>),
    Table(&'a Table),
}

/// A story's blocks as laid out: each table, and each paragraph with the ones joined to it (REV-6).
pub fn laid<B: Borrow<Block>>(blocks: &[B], view: MarkupView) -> Vec<LaidBlock<'_>> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while let Some(b) = blocks.get(i) {
        match b.borrow() {
            Block::Table(t) => {
                out.push(LaidBlock::Table(t));
                i += 1;
            }
            Block::Para(p) => {
                let end = chain_end(blocks, i, view);
                out.push(LaidBlock::Para(laid_para(&blocks[i..=end]).unwrap_or(LaidPara::One(p))));
                i = end + 1;
            }
        }
    }
    out
}

/// The paragraph piece `i` of `shaped` comes from: its own for a joined paragraph's piece, else `para`.
pub fn piece_para(shaped: &ShapedPara, i: usize, para: ParaId) -> ParaId {
    match (&shaped.join, shaped.pieces.get(i).and_then(|p| p.src)) {
        (Some(j), Some(u)) => j.source(u).0,
        _ => para,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docx::review::{AuthorId, MarkKind, RevisionId, RevisionMark};

    /// Plain paragraphs with these texts, those marked `true` with a deleted mark.
    fn blocks(paras: &[(&str, bool)]) -> Vec<Block> {
        let texts: Vec<String> = paras.iter().map(|p| p.0.to_string()).collect();
        let mut d = super::super::tests::plain_doc(&texts).expect("the plain fixture");
        let mut out = std::mem::take(&mut d.sections[0].blocks);
        for (b, &(_, deleted)) in out.iter_mut().zip(paras) {
            if let (Block::Para(p), true) = (b, deleted) {
                let m = RevisionMark { id: RevisionId(1), author: AuthorId(0), date: None, kind: MarkKind::Delete, move_name: None };
                Rc::make_mut(&mut p.mark).rev.del = Some(Rc::new(m));
            }
        }
        out
    }

    fn id(b: &Block) -> ParaId {
        match b {
            Block::Para(p) => ParaId(p.id),
            Block::Table(_) => ParaId(0),
        }
    }

    #[test]
    fn hidden_marks_join_their_paragraphs_with_the_next() {
        let blocks = blocks(&[("ab", true), ("", true), ("cde", false), ("f", true)]);
        let view = MarkupView::NoMarkup;
        assert_eq!(chain_end(&blocks, 0, view), 2);
        assert_eq!(chain_start(&blocks, 2, view), 0);
        assert_eq!(chain_start(&blocks, 3, view), 3);
        // The last mark has no paragraph after it: it stays.
        assert_eq!(chain_end(&blocks, 3, view), 3);
        assert_eq!(chain_end(&blocks, 0, MarkupView::All), 0);
        let laid = laid(&blocks, view);
        assert_eq!(laid.len(), 2);
        let LaidBlock::Para(LaidPara::Joined(j)) = &laid[0] else { panic!("a joined paragraph") };
        assert_eq!(ParaId(j.para.id), id(&blocks[2]));
        // "ab", its mark, the empty paragraph's mark, "cde".
        assert_eq!(j.para.inlines.iter().map(Inline::units).sum::<usize>(), 7);
        let (a, b, c) = (id(&blocks[0]), id(&blocks[1]), id(&blocks[2]));
        let m = &j.map;
        assert_eq!(m.source(Unit(1)), (a, Unit(1)));
        assert_eq!(m.source(Unit(2)), (a, Unit(2)));
        assert_eq!(m.source(Unit(3)), (b, Unit(0)));
        assert_eq!(m.source(Unit(4)), (c, Unit(0)));
        assert_eq!(m.source(Unit(6)), (c, Unit(2)));
        let parts: Vec<_> = m.parts().collect();
        assert_eq!(parts, vec![(a, Unit(0), Some(Unit(3))), (b, Unit(3), Some(Unit(4))), (c, Unit(4), None)]);
    }

    #[test]
    fn a_mark_before_a_table_stays() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-TABLES.docx");
        let d = super::super::tests::doc(&path).expect("the tables fixture");
        let table = d.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned().expect("a table");
        let mut blocks = blocks(&[("a", true), ("b", true)]);
        blocks.push(table);
        assert_eq!(chain_end(&blocks, 0, MarkupView::NoMarkup), 1);
        assert_eq!(chain_end(&blocks, 1, MarkupView::NoMarkup), 1);
        assert_eq!(chain_start(&blocks, 2, MarkupView::NoMarkup), 0);
    }
}
