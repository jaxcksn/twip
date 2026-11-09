//! Joined paragraphs

use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::{Block, Inline, NoteKind, Paragraph, Table};
use crate::docx::review::MarkupView;

use super::LayoutCtx;
use super::ids::{ParaId, Unit};
use super::space::ShapeSpace;
use super::text::{FieldValues, ShapedPara, shape_space};

const MARK: &str = "\u{b6}";

#[derive(Clone, Debug, PartialEq)]
pub struct JoinMap {
    first: ParaId,
    rest: Box<[(ParaId, Unit)]>,
}

impl JoinMap {
    pub fn source(&self, u: Unit) -> (ParaId, Unit) {
        match self.rest[..self.rest.partition_point(|s| s.1 <= u)].last() {
            Some(&(p, start)) => (p, Unit(u.0 - start.0)),
            None => (self.first, u),
        }
    }

    pub fn first(&self) -> ParaId {
        self.first
    }

    pub fn parts(&self) -> impl Iterator<Item = (ParaId, Unit, Option<Unit>)> + '_ {
        let starts = std::iter::once((self.first, Unit(0))).chain(self.rest.iter().copied());
        starts.enumerate().map(|(k, (p, s))| (p, s, self.rest.get(k).map(|r| r.1)))
    }
}

fn hides_mark(b: &Block, view: MarkupView) -> bool {
    matches!(b, Block::Para(p) if view.hides(&p.mark.rev))
}

pub fn chain_end<B: Borrow<Block>>(blocks: &[B], i: usize, view: MarkupView) -> usize {
    let para = |k: usize| blocks.get(k).is_some_and(|b| matches!(b.borrow(), Block::Para(_)));
    let mut j = i;
    while para(j + 1) && blocks.get(j).is_some_and(|b| hides_mark(b.borrow(), view)) {
        j += 1;
    }
    j
}

pub fn chain_start<B: Borrow<Block>>(blocks: &[B], i: usize, view: MarkupView) -> usize {
    let mut s = i.min(blocks.len());
    while s > 0 && hides_mark(blocks[s - 1].borrow(), view) {
        s -= 1;
    }
    s
}

#[derive(Debug)]
pub struct Joined {
    pub para: Paragraph,
    pub map: Rc<JoinMap>,
}

impl Joined {
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
            list: last.list.clone(),
            inlines,
            src: None,
            id: last.id,
            controls: last.controls.clone(),
        };
        Some(Joined { para, map: Rc::new(JoinMap { first: ParaId(first.id), rest: rest.into() }) })
    }
}

#[derive(Clone, Debug)]
pub enum LaidPara<'a> {
    One(&'a Paragraph),
    Joined(Rc<Joined>),
    /// A paragraph laid out with other paragraph properties than its own.
    Restyled(Rc<Paragraph>),
}

impl LaidPara<'_> {
    pub fn para(&self) -> &Paragraph {
        match self {
            LaidPara::One(p) => p,
            LaidPara::Joined(j) => &j.para,
            LaidPara::Restyled(p) => p,
        }
    }

    /// The same paragraph (or chain) with `style` as its properties.
    pub fn restyled(&self, style: Rc<crate::docx::ParaStyle>) -> LaidPara<'static> {
        match self {
            LaidPara::Joined(j) => LaidPara::Joined(Rc::new(Joined { para: Paragraph { style, ..j.para.clone() }, map: j.map.clone() })),
            _ => LaidPara::Restyled(Rc::new(Paragraph { style, ..self.para().clone() })),
        }
    }

    pub fn shape_space_in(&self, fields: &FieldValues, notes: &HashMap<(NoteKind, i64), String>, ctx: &LayoutCtx, in_cell: bool) -> ShapeSpace {
        ShapeSpace { in_cell, ..self.shape_space(fields, notes, ctx) }
    }

    pub fn shape_space(&self, fields: &FieldValues, notes: &HashMap<(NoteKind, i64), String>, ctx: &LayoutCtx) -> ShapeSpace {
        let mut space = shape_space(self.para(), fields, notes, &ctx.env.text, ctx.preview);
        space.expansion = ctx.expansions.and_then(|e| e.paras.get(&ParaId(self.para().id))).cloned();
        if let LaidPara::Joined(j) = self {
            space.join = Some(j.map.clone());
        }
        space
    }
}

pub fn laid_para<B: Borrow<Block>>(chain: &[B]) -> Option<LaidPara<'_>> {
    let paras: Vec<&Paragraph> = chain.iter().filter_map(|b| if let Block::Para(p) = b.borrow() { Some(p) } else { None }).collect();
    match paras[..] {
        [] => None,
        [p] => Some(LaidPara::One(p)),
        _ => Joined::new(&paras).map(|j| LaidPara::Joined(Rc::new(j))),
    }
}

#[derive(Clone, Debug)]
pub enum LaidBlock<'a> {
    Para(LaidPara<'a>),
    Table(&'a Table),
}

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
        assert_eq!(chain_end(&blocks, 3, view), 3);
        assert_eq!(chain_end(&blocks, 0, MarkupView::All), 0);
        let laid = laid(&blocks, view);
        assert_eq!(laid.len(), 2);
        let LaidBlock::Para(LaidPara::Joined(j)) = &laid[0] else { panic!("a joined paragraph") };
        assert_eq!(ParaId(j.para.id), id(&blocks[2]));
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
