//! Field codes across paragraphs

use std::collections::HashSet;

use crate::docx::fields::{span_code_text, span_edge, start_in_view};
use crate::docx::{Block, Document, FieldEdge, FieldView, Inline, Paragraph};
use crate::font::Fonts;
use crate::layout::{ParaId, Unit};

use super::commands::Outcome;
use super::index::{Container, DocIndex};
use super::position::Position;
use super::selection::Selection;
use super::step::{Edges, Step, para_len};
use super::toc::{edge_pairs, partner};
use super::{Editor, Mode};

/// A field across paragraphs: its container and the (block, inline) of its start and end edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpanField {
    /// The story or cell its paragraphs are in.
    pub container: Container,
    /// Its start edge.
    pub start: (usize, usize),
    /// Its end edge.
    pub end: (usize, usize),
}

/// A field across paragraphs as edits leave it: the paragraph its start is in, and which start in that paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpanKey {
    para: ParaId,
    nth: u32,
}

fn is_start(i: &Inline) -> bool {
    matches!(span_edge(i), Some(FieldEdge::Start { .. }))
}

/// `s` with its places mapped through `maps`.
fn mapped(s: Selection, maps: &[super::step::Map]) -> Selection {
    let at = |p: Position| maps.iter().fold(p, |p, m| m.apply(p));
    match s {
        Selection::Caret(p) => Selection::Caret(at(p)),
        Selection::Text { anchor, focus, unit } => Selection::range(at(anchor), at(focus), unit),
        s => s,
    }
}

/// Whether a field showing its code can end in paragraph `b`, which holds no closing brace: the block before it is
/// a folded paragraph or a table.
fn may_end_shown(blocks: &[Block], b: usize) -> bool {
    match b.checked_sub(1).and_then(|i| blocks.get(i)) {
        Some(Block::Para(q)) => crate::docx::fields::folded(q),
        Some(Block::Table(_)) => true,
        None => false,
    }
}

fn units_before(inlines: &[Inline], k: usize) -> u32 {
    inlines[..k].iter().map(|i| i.units() as u32).sum()
}

fn para_of(blocks: &[Block], b: usize) -> Option<&Paragraph> {
    match blocks.get(b)? {
        Block::Para(p) => Some(p),
        Block::Table(_) => None,
    }
}

/// Whether the start at `at` of `blocks` shows its code.
fn shows_code(blocks: &[Block], (b, k): (usize, usize)) -> bool {
    let start = para_of(blocks, b).and_then(|p| p.inlines.get(k)).and_then(span_edge);
    matches!(start, Some(FieldEdge::Start { view: FieldView::Code, .. }))
}

/// The closing brace of the code the start at `at` shows: the first edge after it, when that is one.
fn close_after(blocks: &[Block], (b, k): (usize, usize)) -> Option<(usize, usize)> {
    for blk in b..blocks.len() {
        let Some(p) = para_of(blocks, blk) else { continue };
        let from = if blk == b { k + 1 } else { 0 };
        if let Some((j, e)) = p.inlines.iter().enumerate().skip(from).find_map(|(j, i)| Some((j, span_edge(i)?))) {
            return matches!(e, FieldEdge::Code).then_some((blk, j));
        }
    }
    None
}

/// The end edge closing what is open at `at`: the first end at nesting depth 0 from it on.
fn end_from(blocks: &[Block], (b, k): (usize, usize)) -> Option<(usize, usize)> {
    let mut depth = 0usize;
    for blk in b..blocks.len() {
        let Some(p) = para_of(blocks, blk) else { continue };
        let from = if blk == b { k } else { 0 };
        for (j, i) in p.inlines.iter().enumerate().skip(from) {
            match span_edge(i) {
                Some(FieldEdge::Start { .. }) => depth += 1,
                Some(FieldEdge::End) if depth == 0 => return Some((blk, j)),
                Some(FieldEdge::End) => depth -= 1,
                _ => {}
            }
        }
    }
    None
}

/// The place before inline `k` of block `b`.
fn place(blocks: &[Block], story: super::position::StoryId, (b, k): (usize, usize)) -> Option<Position> {
    let p = para_of(blocks, b)?;
    Some(Position::text(story, ParaId(p.id), Unit(units_before(&p.inlines, k))))
}

/// The first closing brace in `p`: its inline and unit.
fn close_in(p: &Paragraph) -> Option<(usize, u32)> {
    let mut at = 0u32;
    for (j, i) in p.inlines.iter().enumerate() {
        if matches!(span_edge(i), Some(FieldEdge::Code)) {
            return Some((j, at));
        }
        at += i.units() as u32;
    }
    None
}

/// The first end edge in `p` at or after unit `u`: its inline and unit.
fn end_in(p: &Paragraph, u: u32) -> Option<(usize, u32)> {
    let mut at = 0u32;
    for (j, i) in p.inlines.iter().enumerate() {
        if at >= u && matches!(span_edge(i), Some(FieldEdge::End)) {
            return Some((j, at));
        }
        at += i.units() as u32;
    }
    None
}

/// Where a place a field showing its code hides shows: after the field's end (its result is hidden from its code's
/// closing brace on). `None`: the place shows.
pub(super) fn shown_place(doc: &Document, idx: &DocIndex, pos: Position) -> Option<Position> {
    let (para, u) = pos.para_offset()?;
    let slot = idx.para_slot(para)?;
    if let Some(at) = past_folded_table(doc, idx, slot.container) {
        return Some(at);
    }
    let blocks = idx.blocks(doc, slot.container)?;
    let p = para_of(blocks, slot.block)?;
    if !p.inlines.iter().any(|i| matches!(i, Inline::Opaque(o) if o.field.is_some())) {
        return None;
    }
    if let Some((j, c)) = close_in(p) {
        if u.0 <= c {
            return None;
        }
        let end = end_from(blocks, (slot.block, j + 1))?;
        let at = place(blocks, pos.story, end)?;
        return (end.0 != slot.block || at.para_offset().is_some_and(|(_, e)| u < e)).then_some(at);
    }
    if crate::docx::fields::folded(p) {
        return place(blocks, pos.story, end_from(blocks, (slot.block, 0))?);
    }
    if !may_end_shown(blocks, slot.block) {
        return None;
    }
    let (j, e) = end_in(p, u.0 + 1)?;
    shows_code(blocks, partner(blocks, (slot.block, j))?).then(|| Position::text(pos.story, para, Unit(e)))
}

/// Where a place in a cell of a table inside a fold shows: after the end of the field folding the outermost such table.
fn past_folded_table(doc: &Document, idx: &DocIndex, mut c: Container) -> Option<Position> {
    let mut out = None;
    while let Container::Cell { table, .. } = c {
        let Some((slot, blocks)) = idx.table_slot(table).and_then(|s| Some((s, idx.blocks(doc, s.container)?))) else { break };
        if crate::docx::fields::fold_over(blocks, slot.block).is_some() {
            out = end_from(blocks, (slot.block + 1, 0)).and_then(|e| place(blocks, idx.story(slot.container), e)).or(out);
        }
        c = slot.container;
    }
    out
}

/// A caret step over a code's closing brace: right from before it, the place after the field's end; left from after
/// the end, the place before the brace. `None`: an ordinary step.
pub(super) fn fold_step(doc: &Document, idx: &DocIndex, pos: Position, forward: bool) -> Option<Position> {
    let (para, u) = pos.para_offset()?;
    let slot = idx.para_slot(para)?;
    let blocks = idx.blocks(doc, slot.container)?;
    let p = para_of(blocks, slot.block)?;
    if !p.inlines.iter().any(|i| matches!(i, Inline::Opaque(o) if o.field.is_some())) {
        return None;
    }
    if forward {
        let (j, c) = close_in(p)?;
        return if u.0 < c { None } else { place(blocks, pos.story, end_from(blocks, (slot.block, j + 1))?) };
    }
    if close_in(p).is_none() && !may_end_shown(blocks, slot.block) {
        return None;
    }
    let (j, _) = end_in(p, u.0)?;
    let start = partner(blocks, (slot.block, j))?;
    if !shows_code(blocks, start) {
        return None;
    }
    let close = close_after(blocks, start)?;
    let before = place(blocks, pos.story, close)?;
    // Inside the code (a field in one paragraph): an ordinary step.
    let inside = close.0 == slot.block && before.para_offset().is_some_and(|(_, c)| u <= c);
    (!inside).then_some(before)
}

/// The field across paragraphs showing its code that a Backspace (`back`) or Delete at `pos` meets at a brace or its
/// end: its start and end places.
pub(super) fn brace_field(doc: &Document, idx: &DocIndex, pos: Position, back: bool) -> Option<(Position, Position)> {
    let (para, u) = pos.para_offset()?;
    let slot = idx.para_slot(para)?;
    let blocks = idx.blocks(doc, slot.container)?;
    let p = para_of(blocks, slot.block)?;
    let unit = if back { u.0.checked_sub(1) } else { Some(u.0) };
    let mut at = 0u32;
    let mut met = None;
    for (j, i) in p.inlines.iter().enumerate() {
        let n = i.units() as u32;
        if n > 0 && Some(at) == unit {
            met = match span_edge(i) {
                Some(FieldEdge::Start { view: FieldView::Code, .. }) => Some((slot.block, j)),
                Some(FieldEdge::Code) => partner_start(blocks, (slot.block, j)),
                _ => None,
            };
            break;
        }
        at += n;
    }
    let start = match met {
        Some(s) => s,
        None if back => {
            let after = fold_step(doc, idx, pos, false)?;
            let close = after.para_offset()?;
            let slot = idx.para_slot(close.0)?;
            let p = para_of(blocks, slot.block)?;
            let (j, _) = close_in(p)?;
            partner_start(blocks, (slot.block, j))?
        }
        None => return None,
    };
    let end = partner(blocks, start)?;
    Some((place(blocks, pos.story, start)?, place(blocks, pos.story, end)?))
}

/// The start whose code a closing brace at `at` closes: the nearest start edge before it.
fn partner_start(blocks: &[Block], (b, k): (usize, usize)) -> Option<(usize, usize)> {
    for blk in (0..=b).rev() {
        let Some(p) = para_of(blocks, blk) else { continue };
        let to = if blk == b { k } else { p.inlines.len() };
        if let Some(j) = p.inlines[..to].iter().rposition(|i| span_edge(i).is_some_and(|e| !matches!(e, FieldEdge::Fold))) {
            return matches!(span_edge(&p.inlines[j]), Some(FieldEdge::Start { view: FieldView::Code, .. })).then_some((blk, j));
        }
    }
    None
}

/// Whether unit `u` of `p` is inside a code a field across paragraphs shows (after its opening brace, up to its
/// closing one).
pub(super) fn in_code(p: &Paragraph, u: Unit) -> bool {
    let mut at = 0u32;
    let mut open = false;
    for i in &p.inlines {
        match span_edge(i) {
            Some(FieldEdge::Start { view: FieldView::Code, .. }) => open = at < u.0,
            Some(FieldEdge::Code) if open => return u.0 <= at,
            _ => {}
        }
        at += i.units() as u32;
    }
    false
}

/// The field across paragraphs whose shown code holds `pos` (after its opening brace, up to its closing one): its
/// start and end places.
pub(super) fn code_around(doc: &Document, idx: &DocIndex, pos: Position) -> Option<(Position, Position)> {
    let (para, u) = pos.para_offset()?;
    let slot = idx.para_slot(para)?;
    let blocks = idx.blocks(doc, slot.container)?;
    let p = para_of(blocks, slot.block)?;
    if !in_code(p, u) {
        return None;
    }
    let (j, _) = close_in(p)?;
    let start = partner_start(blocks, (slot.block, j))?;
    Some((place(blocks, pos.story, start)?, place(blocks, pos.story, partner(blocks, start)?)?))
}

/// The steps replacing inline `k` of `p`, a marker without units, with `with`.
fn swap(p: &Paragraph, k: usize, with: Vec<Inline>) -> Step {
    let s = units_before(&p.inlines, k);
    let j = (k + 1..p.inlines.len()).find(|&i| p.inlines[i].units() > 0).unwrap_or(p.inlines.len());
    let under = p.inlines[k + 1..j].iter().filter(|i| !matches!(i, Inline::Text(..))).count() as u32;
    Step::Text { para: ParaId(p.id), range: Unit(s)..Unit(s), with, edges: Edges { lead: 1, trail: 0, under, parts: None } }
}

/// The step taking away the last fold mark of `p`.
fn unfold_step(p: &Paragraph) -> Option<Step> {
    let k = p.inlines.iter().rposition(|i| matches!(span_edge(i), Some(FieldEdge::Fold)))?;
    Some(swap(p, k, Vec::new()))
}

impl Editor {
    /// Every field across paragraphs, in each story and cell holding one.
    pub fn span_fields(&self) -> Vec<SpanField> {
        let mut containers: HashSet<Container> = HashSet::new();
        for id in self.index.para_ids() {
            if let Some(p) = self.index.para(&self.doc, id)
                && p.inlines.iter().any(|i| matches!(span_edge(i), Some(FieldEdge::Start { .. })))
                && let Some(s) = self.index.para_slot(id)
            {
                containers.insert(s.container);
            }
        }
        let mut out = Vec::new();
        for c in containers {
            let Some(blocks) = self.index.blocks(&self.doc, c) else { continue };
            out.extend(edge_pairs(blocks).into_iter().map(|(start, end)| SpanField { container: c, start, end }));
        }
        out
    }

    /// The innermost field across paragraphs holding the selection's start.
    pub fn span_field_here(&self) -> Option<SpanField> {
        let (a, _) = self.selection.ordered(&self.index)?;
        let (para, u) = a.para_offset()?;
        let slot = self.index.para_slot(para)?;
        let blocks = self.index.blocks(&self.doc, slot.container)?;
        let unit = |(b, k): (usize, usize)| para_of(blocks, b).map_or(0, |p| units_before(&p.inlines, k));
        let here = (slot.block, u.0);
        edge_pairs(blocks)
            .into_iter()
            .filter(|&(s, e)| (s.0, unit(s)) <= here && here <= (e.0, unit(e)))
            .max_by_key(|&(s, _)| s)
            .map(|(start, end)| SpanField { container: slot.container, start, end })
    }

    /// The fields across paragraphs Shift+F9 takes: with the selection in one paragraph, the innermost one holding
    /// it; with a selection across paragraphs, the innermost one holding all of it, else every one it touches.
    pub fn span_fields_selected(&self) -> Vec<SpanField> {
        let Some((a, b)) = self.selection.ordered(&self.index) else { return Vec::new() };
        let (Some((pa, ua)), Some((pb, ub))) = (a.para_offset(), b.para_offset()) else { return Vec::new() };
        let (Some(sa), Some(sb)) = (self.index.para_slot(pa), self.index.para_slot(pb)) else { return Vec::new() };
        if pa == pb || sa.container != sb.container {
            return self.span_field_here().into_iter().collect();
        }
        let Some(blocks) = self.index.blocks(&self.doc, sa.container) else { return Vec::new() };
        let unit = |(b, k): (usize, usize)| para_of(blocks, b).map_or(0, |p| units_before(&p.inlines, k));
        let (from, to) = ((sa.block, ua.0), (sb.block, ub.0));
        let touched: Vec<SpanField> = edge_pairs(blocks)
            .into_iter()
            .filter(|&(s, e)| (s.0, unit(s)) <= to && from <= (e.0, unit(e)))
            .map(|(start, end)| SpanField { container: sa.container, start, end })
            .collect();
        let holding = |f: &&SpanField| (f.start.0, unit(f.start)) <= from && to <= (f.end.0, unit(f.end));
        match touched.iter().filter(holding).max_by_key(|f| f.start) {
            Some(f) => vec![*f],
            None => touched,
        }
    }

    /// Whether field `f` shows its code.
    pub fn span_shows_code(&self, f: &SpanField) -> bool {
        self.index.blocks(&self.doc, f.container).is_some_and(|b| shows_code(b, f.start))
    }

    /// Shows `fields` as their codes (`code`) or their results (Alt+F9, Shift+F9): the start's code becomes runs
    /// between braces on the first paragraph's line, and the result's paragraph marks fold into it up to the end. A
    /// view, not an edit: never tracked, allowed in a read-only document, no undo step and the document isn't modified
    /// (an undo or redo shows the fields as they were for its edit, then as they are again). In a batch, its steps go
    /// with the batch's.
    pub fn set_span_views(&mut self, fields: &[SpanField], code: bool, fonts: &Fonts) -> Outcome {
        let mut fields: Vec<SpanField> = fields.iter().filter(|f| self.span_shows_code(f) != code).copied().collect();
        if fields.is_empty() {
            return Outcome::Unchanged("no change");
        }
        // Last first: a field's steps leave the inlines and blocks before it where they are.
        fields.sort_by_key(|f| std::cmp::Reverse(f.start));
        let here = self.span_field_here().filter(|h| fields.contains(h)).and_then(|h| self.span_key(&h));
        let mode = std::mem::replace(&mut self.mode, Mode::Editing);
        let mut tx = self.begin(if code { "Show Field Codes" } else { "Hide Field Codes" });
        tx.tracked = false;
        let mut taken = None;
        let ok = if self.batch.is_some() {
            fields.iter().all(|f| if code { self.show_code(&mut tx, f) } else { self.show_result(&mut tx, f) })
        } else {
            // The fields shown otherwise since the last edit, undo or redo and these, shown from where it left them
            let mut keys = self.history.pending().and_then(|p| p.view.clone()).unwrap_or_default();
            keys = super::history::merge_views(keys, fields.iter().filter_map(|f| Some((self.span_key(f)?, code))).collect());
            if let Some(v) = self.history.take_pending() {
                self.replay(&mut tx, v.undo.clone());
                taken = Some(v);
            }
            tx.skip = tx.undo.len();
            tx.real = tx.skip;
            let from = tx.undo.len();
            let (kept, ok) = self.show_keys(&mut tx, keys);
            tx.view = Some((from, kept));
            ok
        };
        self.mode = mode;
        if !ok {
            self.abort(tx);
            if taken.is_some() {
                self.history.set_pending(taken);
            }
            return Outcome::Unchanged("the field changed");
        }
        // A caret in the field: at its code's end, or its start.
        match here.and_then(|k| self.span_field_of(k)).and_then(|f| self.span_caret(&f, code)) {
            Some(p) => self.selection = Selection::Caret(p),
            None => self.selection = mapped(self.selection, &tx.maps),
        }
        self.commit(tx, None, fonts)
    }

    /// Shows the fields of `keys` as an undo or redo found them, those still there, after its edit's steps.
    pub(super) fn reshow_views(&mut self, tx: &mut super::Tx, keys: Vec<(SpanKey, bool)>) {
        let (from, maps) = (tx.undo.len(), tx.maps.len());
        let (kept, _) = self.show_keys(tx, keys);
        self.selection = mapped(self.selection, &tx.maps[maps..]);
        tx.view = Some((from, kept));
    }

    /// Shows each field of `keys` still there as its code (`true`) or its result: those, and whether every one did.
    fn show_keys(&mut self, tx: &mut super::Tx, keys: Vec<(SpanKey, bool)>) -> (Vec<(SpanKey, bool)>, bool) {
        let mut kept = Vec::with_capacity(keys.len());
        for (k, code) in keys {
            let Some(f) = self.span_field_of(k) else { continue };
            kept.push((k, code));
            if self.span_shows_code(&f) != code && !(if code { self.show_code(tx, &f) } else { self.show_result(tx, &f) }) {
                return (kept, false);
            }
        }
        (kept, true)
    }

    /// Field `f`'s key.
    fn span_key(&self, f: &SpanField) -> Option<SpanKey> {
        let p = self.span_para(f, f.start.0)?;
        let nth = p.inlines.get(..f.start.1)?.iter().filter(|i| is_start(i)).count() as u32;
        Some(SpanKey { para: ParaId(p.id), nth })
    }

    /// The field across paragraphs of key `k`, if it is still there.
    fn span_field_of(&self, k: SpanKey) -> Option<SpanField> {
        let slot = self.index.para_slot(k.para)?;
        let blocks = self.index.blocks(&self.doc, slot.container)?;
        let p = para_of(blocks, slot.block)?;
        let (j, _) = p.inlines.iter().enumerate().filter(|(_, i)| is_start(i)).nth(k.nth as usize)?;
        let end = partner(blocks, (slot.block, j))?;
        Some(SpanField { container: slot.container, start: (slot.block, j), end })
    }

    /// Where the caret goes in field `f` once it shows its code (before its closing brace) or its result (its start).
    fn span_caret(&self, f: &SpanField, code: bool) -> Option<Position> {
        let blocks = self.index.blocks(&self.doc, f.container)?;
        let story = self.index.story(f.container);
        if code { place(blocks, story, close_after(blocks, f.start)?) } else { place(blocks, story, f.start) }
    }

    /// The paragraph block `b` of `f`'s container is.
    fn span_para(&self, f: &SpanField, b: usize) -> Option<&Paragraph> {
        para_of(self.index.blocks(&self.doc, f.container)?, b)
    }

    /// Shows `f`'s code: its start a brace with the code's runs and the closing brace after it, a fold mark on each
    /// paragraph before its end's.
    fn show_code(&mut self, tx: &mut super::Tx, f: &SpanField) -> bool {
        let Some(p) = self.span_para(f, f.start.0) else { return false };
        let Some(Inline::Opaque(o)) = p.inlines.get(f.start.1) else { return false };
        let Some(FieldEdge::Start { code, .. }) = &o.field else { return false };
        let run = o.run.clone().unwrap_or_else(|| p.mark.clone());
        let mut with = vec![start_in_view(o, code.clone(), FieldView::Code)];
        if !code.is_empty() {
            with.push(Inline::Text(code.clone(), run.clone()));
        }
        with.push(FieldEdge::Code.inline("", run));
        let step = swap(p, f.start.1, with);
        if self.apply_step(tx, step).is_err() {
            return false;
        }
        for b in f.start.0..f.end.0 {
            let Some(q) = self.span_para(f, b) else { continue };
            let len = para_len(q);
            let mark = q.mark.clone();
            let step = Step::Text { para: ParaId(q.id), range: len..len, with: vec![FieldEdge::Fold.inline("", mark)], edges: Edges::default() };
            if self.apply_step(tx, step).is_err() {
                return false;
            }
        }
        true
    }

    /// Shows `f`'s result: its start takes the code its runs read, the runs and closing brace go, and so do the fold
    /// marks.
    fn show_result(&mut self, tx: &mut super::Tx, f: &SpanField) -> bool {
        let Some(blocks) = self.index.blocks(&self.doc, f.container) else { return false };
        let Some(p) = para_of(blocks, f.start.0) else { return false };
        let Some(Inline::Opaque(o)) = p.inlines.get(f.start.1) else { return false };
        let k = f.start.1;
        let close = close_after(blocks, f.start);
        let s = units_before(&p.inlines, k);
        // The code's runs in the start's paragraph: up to its closing brace, else to its last unit.
        let (to, upto) = match close {
            Some((cb, ck)) if cb == f.start.0 => (Unit(units_before(&p.inlines, ck) + 1), ck + 1),
            _ => (para_len(p), p.inlines.iter().rposition(|i| i.units() > 0).map_or(k + 1, |j| j + 1)),
        };
        let mut with = vec![start_in_view(o, span_code_text(&p.inlines, k), FieldView::Result)];
        with.extend(p.inlines[k + 1..upto.max(k + 1)].iter().filter(|i| i.units() == 0 && !matches!(i, Inline::Text(..))).cloned());
        let split = close.filter(|c| c.0 != f.start.0);
        let step = Step::Text { para: ParaId(p.id), range: Unit(s)..to, with, edges: Edges::default() };
        if self.apply_step(tx, step).is_err() {
            return false;
        }
        // A closing brace a split took to a later paragraph
        if let Some((cb, ck)) = split
            && let Some(q) = self.span_para(f, cb)
        {
            let c = Unit(units_before(&q.inlines, ck));
            let step = Step::Text { para: ParaId(q.id), range: c..Unit(c.0 + 1), with: Vec::new(), edges: Edges::default() };
            if self.apply_step(tx, step).is_err() {
                return false;
            }
        }
        for b in f.start.0..=f.end.0 {
            if let Some(step) = self.span_para(f, b).and_then(unfold_step)
                && self.apply_step(tx, step).is_err()
            {
                return false;
            }
        }
        true
    }
}
