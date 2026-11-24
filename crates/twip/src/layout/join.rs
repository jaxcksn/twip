//! Joined paragraphs

use std::borrow::Borrow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::{Block, Inline, NoteKind, Paragraph, Table};
use crate::docx::review::ReviewView;

use super::LayoutCtx;
use super::ids::{ParaId, Unit};
use super::job::{Memos, Yield};
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

/// Whether `b`'s mark joins it to the next paragraph: a tracked deletion the markup view hides, or, while hidden text
/// doesn't show, a vanished mark (a hidden paragraph mark, a style separator) made since reading, which joins as
/// reading joins one; or a mark inside the result of a field across paragraphs showing its code (its fold mark).
fn hides_mark(b: &Block, review: &ReviewView) -> bool {
    matches!(b, Block::Para(p) if review.hides(&p.mark.rev) || (p.mark.hidden && !review.show_hidden) || crate::docx::fields::folded(p))
}

/// Whether mark `r` is a style separator (`w:specVanish`).
fn spec_vanish(r: &crate::docx::Run) -> bool {
    r.direct.0.as_deref().is_some_and(|x| crate::docx::write::props::flag_of(x, "specVanish"))
}

/// The last paragraph of the chain from `blocks[i]`; a fold's chain runs on past the tables inside it
/// ([`crate::docx::fields::fold_over`]), which aren't laid out.
pub fn chain_end<B: Borrow<Block>>(blocks: &[B], i: usize, review: &ReviewView) -> usize {
    let table = |k: usize| blocks.get(k).is_some_and(|b| matches!(b.borrow(), Block::Table(_)));
    let mut j = i;
    while let Some(Block::Para(p)) = blocks.get(j).map(Borrow::borrow)
        && hides_mark(blocks[j].borrow(), review)
    {
        let mut next = j + 1;
        if crate::docx::fields::folded(p) {
            while table(next) {
                next += 1;
            }
        }
        if !matches!(blocks.get(next).map(Borrow::borrow), Some(Block::Para(_))) {
            break;
        }
        j = next;
    }
    j
}

/// The first block of the chain holding `blocks[i]` ([`chain_end`]).
pub fn chain_start<B: Borrow<Block>>(blocks: &[B], i: usize, review: &ReviewView) -> usize {
    let mut s = i.min(blocks.len());
    while s > 0 {
        s = match blocks[s - 1].borrow() {
            Block::Para(_) if hides_mark(blocks[s - 1].borrow(), review) => s - 1,
            Block::Table(_) => match crate::docx::fields::fold_over(blocks, s - 1) {
                Some(k) => k,
                None => break,
            },
            Block::Para(_) => break,
        };
    }
    s
}

/// Whether a row is hidden: hidden text (while it doesn't show) or a row change the markup view hides.
fn row_hidden(r: &crate::docx::Row, review: &ReviewView) -> bool {
    (r.vanished && !review.show_hidden) || review.hides_row(&r.rev)
}

/// Whether every row of `t` is hidden.
pub fn table_hidden(t: &Table, review: &ReviewView) -> bool {
    t.rows.iter().all(|r| row_hidden(r, review))
}

/// The first paragraph of a shown table: its first shown row's first cell, into a table that starts it.
fn table_first<'a>(t: &'a Table, review: &ReviewView) -> Option<&'a Paragraph> {
    let row = t.rows.iter().find(|r| !row_hidden(r, review))?;
    match row.cells.first()?.blocks.first()? {
        Block::Para(p) => Some(p),
        Block::Table(t) => table_first(t, review),
    }
}

/// The paragraph whose mark a hidden mark at the end of a chain joins when no paragraph follows it: the next shown mark
/// in text order from `blocks[from]` on, past hidden marks, tables whose rows are all hidden and tables inside a fold; a shown table's first
/// cell paragraph counts. `None` at the end of `blocks`.
pub fn next_shown_mark<'a, B: Borrow<Block>>(blocks: &'a [B], from: usize, review: &ReviewView) -> Option<&'a Paragraph> {
    // Whether the tables met are inside a fold
    let mut fold = crate::docx::fields::fold_over(blocks, from).is_some();
    blocks.get(from..)?.iter().find_map(|b| match b.borrow() {
        Block::Para(p) if !hides_mark(b.borrow(), review) => Some(p),
        Block::Para(p) => {
            fold = crate::docx::fields::folded(p);
            None
        }
        Block::Table(t) if fold || table_hidden(t, review) => None,
        Block::Table(t) => table_first(t, review),
    })
}

/// The properties a chain ending at `blocks[end]` takes when its last mark is hidden: the next shown mark's
/// ([`next_shown_mark`]), else at the story's end `last`'s (a story's own final mark). `None`: its own.
pub fn joined_target<'a, B: Borrow<Block>>(
    blocks: &'a [B],
    end: usize,
    review: &ReviewView,
    last: Option<&'a Rc<crate::docx::ParaStyle>>,
) -> Option<&'a Rc<crate::docx::ParaStyle>> {
    let view = review.view;
    let Some(Block::Para(p)) = blocks.get(end).map(Borrow::borrow) else { return None };
    if !review.hides(&p.mark.rev) {
        return None;
    }
    let style = next_shown_mark(blocks, end + 1, review).map_or(last, |q| Some(q.para_style_in(view)))?;
    (!Rc::ptr_eq(style, p.para_style_in(view))).then_some(style)
}

#[derive(Debug)]
pub struct Joined {
    pub para: Paragraph,
    pub map: Rc<JoinMap>,
}

impl Joined {
    /// `paras` as one paragraph. A deleted mark's paragraph takes the next one's properties; a vanished mark's joins
    /// the next as reading joins them ([`crate::docx::resolve::joined_style`]; `legacy`: compat ≤ 14).
    fn new(paras: &[&Paragraph], legacy: bool) -> Option<Joined> {
        let (&last, before) = paras.split_last()?;
        let &first = before.first()?;
        let mut inlines = Vec::with_capacity(paras.iter().map(|p| p.inlines.len() + 1).sum());
        let mut rest = Vec::with_capacity(before.len());
        let mut at = 0u32;
        // The properties and list label so far, the content units so far, and an empty style separator's properties
        let (mut style, mut list) = (first.style.clone(), first.list.clone());
        let (mut shown, mut spec): (u32, Option<Rc<crate::docx::ParaStyle>>) = (0, None);
        for w in paras.windows(2) {
            let (p, next) = (w[0], w[1]);
            inlines.extend(p.inlines.iter().cloned());
            inlines.push(Inline::Text(MARK.to_string(), p.mark.clone()));
            let own = p.inlines.iter().map(Inline::units).sum::<usize>() as u32;
            at += own + 1;
            shown += own;
            rest.push((ParaId(next.id), Unit(at)));
            if p.mark.hidden {
                if !legacy && own == 0 && spec_vanish(&p.mark) {
                    spec = spec.or_else(|| Some(p.style.clone()));
                }
                let from = if shown == 0 { 0 } else { at };
                style = Rc::new(crate::docx::resolve::joined_style(&style, &next.style, from, spec.take().as_deref(), legacy));
                list = list.or_else(|| next.list.clone());
            } else {
                (style, list) = (next.style.clone(), next.list.clone());
            }
        }
        inlines.extend(last.inlines.iter().cloned());
        let para = Paragraph {
            style,
            mark: last.mark.clone(),
            list,
            inlines,
            src: None,
            id: last.id,
            div: last.div.clone(),
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

    pub fn shape_space_in(
        &self,
        fields: &FieldValues,
        notes: &HashMap<(NoteKind, i64), String>,
        ctx: &LayoutCtx,
        in_cell: bool,
        memos: &mut Memos,
    ) -> Result<ShapeSpace, Yield> {
        Ok(ShapeSpace { in_cell, ..self.shape_space(fields, notes, ctx, memos)? })
    }

    pub fn shape_space(
        &self,
        fields: &FieldValues,
        notes: &HashMap<(NoteKind, i64), String>,
        ctx: &LayoutCtx,
        memos: &mut Memos,
    ) -> Result<ShapeSpace, Yield> {
        let mut space = shape_space(self.para(), fields, notes, &ctx.env.text, ctx.preview);
        space.fitted = super::story::fitted_boxes(self.para(), &super::story::StoryEnv { ctx: *ctx, fields, notes }, memos)?;
        space.expansion = ctx.expansions.and_then(|e| e.paras.get(&ParaId(self.para().id))).cloned();
        if let LaidPara::Joined(j) = self {
            space.join = Some(j.map.clone());
        }
        Ok(space)
    }
}

pub fn laid_para<B: Borrow<Block>>(chain: &[B], legacy: bool) -> Option<LaidPara<'_>> {
    let paras: Vec<&Paragraph> = chain.iter().filter_map(|b| if let Block::Para(p) = b.borrow() { Some(p) } else { None }).collect();
    match paras[..] {
        [] => None,
        [p] => Some(LaidPara::One(p)),
        _ => Joined::new(&paras, legacy).map(|j| LaidPara::Joined(Rc::new(j))),
    }
}

#[derive(Clone, Debug)]
pub enum LaidBlock<'a> {
    Para(LaidPara<'a>),
    Table(&'a Table),
}

/// `blocks` as laid out, paragraphs joined by hidden marks as one (`legacy`: compat ≤ 14).
pub fn laid<'a, B: Borrow<Block>>(blocks: &'a [B], review: &ReviewView, legacy: bool) -> Vec<LaidBlock<'a>> {
    laid_in(blocks, review, None, legacy)
}

/// [`laid`] for a story whose own final mark has the properties `last` (a header, footer, text box or notes story).
pub fn laid_in<'a, B: Borrow<Block>>(
    blocks: &'a [B],
    review: &ReviewView,
    last: Option<&'a Rc<crate::docx::ParaStyle>>,
    legacy: bool,
) -> Vec<LaidBlock<'a>> {
    let mut out = Vec::with_capacity(blocks.len());
    let mut i = 0;
    while let Some(b) = blocks.get(i) {
        match b.borrow() {
            Block::Table(t) => {
                out.push(LaidBlock::Table(t));
                i += 1;
            }
            Block::Para(p) => {
                let end = chain_end(blocks, i, review);
                let chain = laid_para(&blocks[i..=end], legacy).unwrap_or(LaidPara::One(p));
                out.push(LaidBlock::Para(match joined_target(blocks, end, review, last) {
                    Some(style) => chain.restyled(style.clone()),
                    None => chain,
                }));
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
    use crate::docx::review::{AuthorId, MarkKind, MarkupView, RevisionId, RevisionMark};

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
        let review = ReviewView { view, ..ReviewView::default() };
        assert_eq!(chain_end(&blocks, 0, &review), 2);
        assert_eq!(chain_start(&blocks, 2, &review), 0);
        assert_eq!(chain_start(&blocks, 3, &review), 3);
        assert_eq!(chain_end(&blocks, 3, &review), 3);
        assert_eq!(chain_end(&blocks, 0, &ReviewView { view: MarkupView::All, ..ReviewView::default() }), 0);
        let laid = laid(&blocks, &review, false);
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
        let review = ReviewView { view: MarkupView::NoMarkup, ..ReviewView::default() };
        assert_eq!(chain_end(&blocks, 0, &review), 1);
        assert_eq!(chain_end(&blocks, 1, &review), 1);
        assert_eq!(chain_start(&blocks, 2, &review), 0);
    }

    #[test]
    fn a_deleted_mark_before_a_hidden_table_joins_into_it_while_hidden_text_shows() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-TABLES.docx");
        let d = super::super::tests::doc(&path).expect("the tables fixture");
        let Some(Block::Table(mut table)) = d.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else { panic!("a table") };
        for r in &mut table.rows {
            r.vanished = true;
            r.rev = Default::default();
        }
        let mut blocks = blocks(&[("a", true)]);
        blocks.push(Block::Table(table));
        blocks.extend(super::super::tests::plain_doc(&["after".to_string()]).expect("plain").sections.remove(0).blocks);
        let Block::Table(t) = &blocks[1] else { unreachable!() };
        let first = table_first(t, &ReviewView { show_hidden: true, ..ReviewView::default() }).map(|p| p.id);
        let hidden = ReviewView { view: MarkupView::NoMarkup, ..ReviewView::default() };
        let shown = ReviewView { show_hidden: true, ..hidden.clone() };
        assert!(table_hidden(t, &hidden) && !table_hidden(t, &shown));
        assert_eq!(next_shown_mark(&blocks, 1, &hidden).map(|p| p.id), Some(id(&blocks[2]).0), "past the hidden table");
        assert_eq!(next_shown_mark(&blocks, 1, &shown).map(|p| p.id), first, "into the shown table's first cell");
        assert!(first.is_some());
    }

    #[test]
    fn the_next_shown_mark_is_past_a_vanished_one_while_hidden_text_doesnt_show() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/editor/review/FX-REVIEW-TABLES.docx");
        let d = super::super::tests::doc(&path).expect("the tables fixture");
        let Some(Block::Table(mut table)) = d.sections[0].blocks.iter().find(|b| matches!(b, Block::Table(_))).cloned() else { panic!("a table") };
        for r in &mut table.rows {
            r.vanished = true;
            r.rev = Default::default();
        }
        let mut blocks = blocks(&[("a", true)]);
        blocks.push(Block::Table(table));
        blocks.extend(blocks_after(&["vanished", "shown"]));
        let Block::Para(p) = &mut blocks[2] else { unreachable!() };
        Rc::make_mut(&mut p.mark).hidden = true;
        let hidden = ReviewView { view: MarkupView::NoMarkup, ..ReviewView::default() };
        assert_eq!(next_shown_mark(&blocks, 1, &hidden).map(|p| p.id), Some(id(&blocks[3]).0), "past the vanished mark");
        let shown = ReviewView { show_hidden: true, ..hidden };
        let Block::Table(t) = &blocks[1] else { unreachable!() };
        assert_eq!(next_shown_mark(&blocks, 1, &shown).map(|p| p.id), table_first(t, &shown).map(|p| p.id), "the table shows");
    }

    fn blocks_after(texts: &[&str]) -> Vec<Block> {
        let texts: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        super::super::tests::plain_doc(&texts).expect("plain").sections.remove(0).blocks
    }
}
