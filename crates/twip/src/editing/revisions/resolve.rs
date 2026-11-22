//! Revision accept and reject

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use crate::docx::review::{AuthorId, CellMarkKind, RangeKind, RevisionId, RevisionMark};
use crate::docx::{Block, Document, Inline, Paragraph, Raw, Row, Run, RunStyle, Table, VMerge};
use crate::font::Fonts;
use crate::layout::{ParaId, TableId, Unit};

use super::super::commands::Outcome;
use super::super::index::DocIndex;
use super::super::position::{Position, StoryId};
use super::super::selection::Selection;
use super::super::step::{Edges, Step, StepError, para_len};
use super::super::{Editor, Tx};
use super::index::{Found, Item, Spot, found, revisions};

#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    Selection,
    All,
    Author(String),
    Revision(RevisionId),
}

enum Who {
    All,
    Author(AuthorId),
    Ids(HashSet<RevisionId>),
}

impl Who {
    fn has(&self, m: &RevisionMark) -> bool {
        match self {
            Who::All => true,
            Who::Author(a) => m.author == *a,
            Who::Ids(ids) => ids.contains(&m.id),
        }
    }
}

struct Pick {
    who: Who,
    /// In a selection: the block-level changes it touches, where they are.
    blocks: Option<HashSet<(Spot, RevisionId)>>,
    inside: Option<HashMap<ParaId, Range<u32>>>,
    moves: HashSet<Rc<str>>,
    move_ranges: HashSet<i64>,
}

impl Pick {
    fn moved(&self, m: &RevisionMark) -> bool {
        m.move_name.as_ref().is_some_and(|n| self.moves.contains(n))
    }

    fn block(&self, m: &RevisionMark, spot: Spot) -> bool {
        self.moved(m) || self.who.has(m) && self.blocks.as_ref().is_none_or(|b| b.contains(&(spot, m.id)))
    }

    fn run(&self, m: &RevisionMark, in_range: bool) -> bool {
        self.moved(m) || in_range && self.who.has(m)
    }

    fn with_moves(mut self, found: &Found, touches: impl Fn(&Item) -> bool) -> Pick {
        let all = matches!(self.who, Who::All) && self.blocks.is_none();
        self.moves = found.items.iter().filter(|it| touches(it)).filter_map(|it| it.mark.move_name.clone()).collect();
        if all {
            self.moves.extend(found.move_ranges.iter().map(|(n, _)| n.clone()));
        }
        self.move_ranges = found.move_ranges.iter().filter(|(n, _)| self.moves.contains(n)).map(|(_, id)| *id).collect();
        self
    }
}

fn decide(r: &Run, accept: bool, take: impl Fn(&RevisionMark) -> bool) -> Option<(bool, Run)> {
    let mut rev = r.rev.clone();
    let (mut touched, mut remove, mut base) = (false, false, None);
    if let Some(m) = &r.rev.ins
        && take(m)
    {
        (touched, remove, rev.ins) = (true, !accept, None);
    }
    if let Some(m) = &r.rev.del
        && take(m)
    {
        touched = true;
        remove |= accept;
        rev.del = None;
    }
    if let Some(f) = &r.rev.fmt
        && take(&f.mark)
    {
        (touched, rev.fmt) = (true, None);
        if !accept {
            base = Some(&f.previous);
        }
    }
    if !touched {
        return None;
    }
    let mut s: RunStyle = base.map_or_else(|| (**r).clone(), |b| (**b).clone());
    s.rev = rev;
    Some((remove, Rc::new(s)))
}

fn resolve_inlines(p: &Paragraph, pick: &Pick, accept: bool) -> Option<(Vec<Inline>, Vec<Range<u32>>)> {
    let inside = pick.inside.as_ref().map(|i| i.get(&ParaId(p.id)).cloned());
    let cuts: Vec<u32> = inside.clone().flatten().map(|r| vec![r.start, r.end]).unwrap_or_default();
    let mut cache: HashMap<(*const RunStyle, bool), Option<(bool, Run)>> = HashMap::new();
    let mut out: Vec<(Inline, bool)> = Vec::with_capacity(p.inlines.len());
    let (mut at, mut changed) = (0u32, false);
    for i in &p.inlines {
        let n = i.units() as u32;
        let pieces: Vec<Inline> = match i {
            Inline::Text(t, r) if cuts.iter().any(|&c| at < c && c < at + n) => {
                let mut v = Vec::new();
                let mut from = 0usize;
                for &c in cuts.iter().filter(|&&c| at < c && c < at + n) {
                    let k = (c - at) as usize;
                    v.push(Inline::Text(t.chars().skip(from).take(k - from).collect(), r.clone()));
                    from = k;
                }
                v.push(Inline::Text(t.chars().skip(from).collect(), r.clone()));
                v
            }
            _ => vec![i.clone()],
        };
        for piece in pieces {
            let n = piece.units() as u32;
            if let Inline::Range(m) = &piece
                && matches!(m.kind, RangeKind::MoveFromStart | RangeKind::MoveFromEnd | RangeKind::MoveToStart | RangeKind::MoveToEnd)
                && pick.move_ranges.contains(&m.id)
            {
                changed = true;
                continue;
            }
            let Some(r) = piece.run() else {
                out.push((piece, false));
                at += n;
                continue;
            };
            let in_range = match &inside {
                None => true,
                Some(None) => false,
                Some(Some(x)) => x.start <= at && at + n <= x.end,
            };
            let d = cache.entry((Rc::as_ptr(r), in_range)).or_insert_with(|| decide(r, accept, |m| pick.run(m, in_range))).clone();
            match d {
                None => out.push((piece, false)),
                Some((remove, run)) => {
                    changed = true;
                    let float = matches!(piece, Inline::Float(..));
                    match (float, remove) {
                        (true, true) => {}
                        (false, true) if n > 0 => out.push((piece, true)),
                        _ => out.push((super::super::format::with_run(&piece, &run), false)),
                    }
                }
            }
            at += n;
        }
    }
    if !changed {
        return None;
    }
    let mut merged: Vec<(Inline, bool)> = Vec::with_capacity(out.len());
    for (i, gone) in out {
        if let (Some((Inline::Text(prev, pr), g)), Inline::Text(t, r)) = (merged.last_mut(), &i)
            && *g == gone
            && Rc::ptr_eq(pr, r)
        {
            prev.push_str(t);
            continue;
        }
        merged.push((i, gone));
    }
    let mut ranges: Vec<Range<u32>> = Vec::new();
    let mut at = 0u32;
    let mut open: Option<u32> = None;
    for (i, gone) in &merged {
        let n = i.units() as u32;
        match (gone, open) {
            (true, None) => open = Some(at),
            (false, Some(s)) => {
                ranges.push(s..at);
                open = None;
            }
            _ => {}
        }
        at += n;
    }
    if let Some(s) = open {
        ranges.push(s..at);
    }
    Some((merged.into_iter().map(|(i, _)| i).collect(), ranges))
}

fn tables_and_paras(doc: &Document, flow: Option<u32>) -> (Vec<TableId>, Vec<ParaId>) {
    fn walk<'a>(blocks: &'a [Block], t: &mut Vec<TableId>, p: &mut Vec<ParaId>, boxes: &mut Vec<&'a [Block]>) {
        for b in blocks {
            match b {
                Block::Para(q) => {
                    p.push(ParaId(q.id));
                    for i in &q.inlines {
                        if let Some(tb) = i.story().map(|(_, tb)| tb) {
                            boxes.push(&tb.blocks);
                        }
                    }
                }
                Block::Table(x) => {
                    t.push(TableId(x.id));
                    for c in x.rows.iter().flat_map(|r| r.cells.iter()) {
                        walk(&c.blocks, t, p, boxes);
                    }
                }
            }
        }
    }
    let (mut t, mut p, mut boxes) = (Vec::new(), Vec::new(), Vec::new());
    let flow = flow.and_then(|k| doc.glossary.as_ref()?.parts.get(k as usize)).map(|b| &b.body);
    let stories =
        doc.sections.iter().map(|s| &s.blocks).chain(doc.stories.values()).chain(doc.footnotes.values()).chain(doc.endnotes.values()).chain(flow);
    for blocks in stories {
        walk(blocks, &mut t, &mut p, &mut boxes);
    }
    while let Some(b) = boxes.pop() {
        walk(b, &mut t, &mut p, &mut boxes);
    }
    (t, p)
}

struct TablePlan {
    rows: Option<Vec<Row>>,
    grid: Option<Vec<f64>>,
    table: Option<Table>,
    restyle: bool,
}

impl Editor {
    pub(in crate::editing) fn resolve_revisions(&mut self, accept: bool, scope: &Scope, fonts: &Fonts) -> Outcome {
        if self.doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking()) {
            self.refused = Some("unavailable");
            return Outcome::Unchanged("unavailable");
        }
        let Some(pick) = self.pick(scope) else { return Outcome::Unchanged("no revision") };
        let mut tx = self.begin(if accept { "Accept" } else { "Reject" });
        tx.tracked = false;
        if self.apply_pick(&mut tx, &pick, accept).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("revisions don't resolve");
        }
        self.follow_steps(&tx);
        self.commit(tx, None, fonts)
    }

    fn pick(&mut self, scope: &Scope) -> Option<Pick> {
        let f = found(&self.doc, &self.index);
        let base = |who: Who| Pick { who, blocks: None, inside: None, moves: HashSet::new(), move_ranges: HashSet::new() };
        let ids = |ids: &[RevisionId]| Who::Ids(ids.iter().copied().collect());
        let pick = match scope {
            Scope::All => {
                let p = base(Who::All);
                p.with_moves(&f, |_| true)
            }
            Scope::Author(name) => {
                let a = AuthorId(self.doc.people.persons.iter().position(|p| &*p.name == name.as_str())? as u32);
                base(Who::Author(a)).with_moves(&f, |it| it.mark.author == a)
            }
            Scope::Revision(id) => {
                let list = revisions(&self.doc, &self.index);
                let who = match list.iter().find(|r| r.ids.contains(id)) {
                    Some(r) => ids(&r.ids),
                    None if f.items.iter().any(|it| it.mark.id == *id) => ids(&[*id]),
                    None => return None,
                };
                let p = base(who);
                let picked: HashSet<RevisionId> = match &p.who {
                    Who::Ids(s) => s.clone(),
                    _ => HashSet::new(),
                };
                p.with_moves(&f, |it| picked.contains(&it.mark.id))
            }
            Scope::Selection => match self.selection {
                Selection::Caret(c) => {
                    let list = revisions(&self.doc, &self.index);
                    let idx = &self.index;
                    let r = list.iter().rfind(|r| r.story == c.story && idx.cmp(&r.start, &c).is_le() && idx.cmp(&c, &r.end).is_le())?;
                    let picked: HashSet<RevisionId> = r.ids.iter().copied().collect();
                    return Some(base(ids(&r.ids)).with_moves(&f, |it| picked.contains(&it.mark.id)));
                }
                Selection::Text { .. } => {
                    let (s, e) = self.selection.ordered(&self.index)?;
                    let idx = &self.index;
                    let touches = |it: &Item| it.story == s.story && idx.cmp(&it.start, &e).is_lt() && idx.cmp(&s, &it.end).is_lt();
                    self.place_pick(&f, touches, |p| covered(&self.doc, idx, s, e, p))
                }
                Selection::Cells { table, anchor, focus, .. } => {
                    let t = self.index.table(&self.doc, table)?;
                    let g = crate::docx::grid::Grid::of(t);
                    let (rows, cols) = (anchor.row.min(focus.row)..=anchor.row.max(focus.row), anchor.col.min(focus.col)..=anchor.col.max(focus.col));
                    let chosen = |row: usize, cell: usize| {
                        rows.contains(&row)
                            && g.rows.get(row).and_then(|r| r.get(cell)).is_some_and(|&(c, n, _)| c <= *cols.end() && c + n.max(1) > *cols.start())
                    };
                    let idx = &self.index;
                    let inside = |pos: &Position| idx.chain_of(pos).iter().any(|l| l.table == table && chosen(l.row, l.cell));
                    let touches = |it: &Item| match it.spot {
                        Spot::Row(x, row) if x == table => rows.contains(&row),
                        Spot::Table(x) if x == table => true,
                        Spot::Cell(x, row, cell) if x == table => chosen(row, cell),
                        _ => inside(&it.start),
                    };
                    let whole = |p: ParaId| {
                        let q = idx.para(&self.doc, p)?;
                        let story = idx.story(idx.para_slot(p)?.container);
                        inside(&Position::text(story, p, Unit(0))).then(|| 0..para_len(q).0)
                    };
                    self.place_pick(&f, touches, whole)
                }
                Selection::Object(_) | Selection::Block { .. } => return None,
            },
        };
        f.items.iter().any(|it| pick.block(&it.mark, it.spot) || pick.run(&it.mark, true)).then_some(pick)
    }

    /// The pick of the changes `touches` takes, the text of each paragraph `covered` gives.
    fn place_pick(&self, f: &Found, touches: impl Fn(&Item) -> bool, covered: impl Fn(ParaId) -> Option<Range<u32>>) -> Pick {
        let blocks: HashSet<(Spot, RevisionId)> =
            f.items.iter().filter(|it| it.run.is_none() && touches(it)).map(|it| (it.spot, it.mark.id)).collect();
        let paras: HashSet<ParaId> = f.items.iter().filter_map(|it| it.run.as_ref().map(|r| r.0)).collect();
        let inside: HashMap<ParaId, Range<u32>> = paras.into_iter().filter_map(|p| Some((p, covered(p)?))).collect();
        let p = Pick { who: Who::All, blocks: Some(blocks), inside: Some(inside), moves: HashSet::new(), move_ranges: HashSet::new() };
        p.with_moves(f, touches)
    }

    fn apply_pick(&mut self, tx: &mut Tx, pick: &Pick, accept: bool) -> Result<(), StepError> {
        for section in 0..self.doc.sections.len() {
            let s = &self.doc.sections[section];
            let Some(c) = s.change.as_ref().filter(|c| pick.block(&c.mark, Spot::Section(section))) else { continue };
            let page = if accept { s.page() } else { c.previous.clone() };
            self.step(tx, Step::SectionPage { section, page: Box::new(page), change: None })?;
        }
        let (tables, _) = tables_and_paras(&self.doc, self.index.flow);
        for t in tables {
            self.resolve_table(tx, t, pick, accept)?;
        }
        let (_, paras) = tables_and_paras(&self.doc, self.index.flow);
        for para in &paras {
            let Some(p) = self.index.para(&self.doc, *para) else { continue };
            let Some((inlines, gone)) = resolve_inlines(p, pick, accept) else { continue };
            if inlines != p.inlines {
                self.step(tx, Step::Restyle { para: *para, inlines })?;
            }
            for r in gone.into_iter().rev() {
                self.step(tx, Step::Text { para: *para, range: Unit(r.start)..Unit(r.end), with: Vec::new(), edges: Edges::default() })?;
            }
        }
        let mut joins = Vec::new();
        for para in &paras {
            let Some(p) = self.index.para(&self.doc, *para) else { continue };
            let mark = decide(&p.mark, accept, |m| pick.block(m, Spot::Mark(*para)));
            let style = p.style.change.as_ref().filter(|c| pick.block(&c.mark, Spot::Para(*para))).map(|c| {
                let mut s = if accept { (*p.style).clone() } else { (*c.previous).clone() };
                s.change = None;
                Rc::new(s)
            });
            if mark.as_ref().is_some_and(|m| m.0) {
                joins.push(*para);
            }
            if style.is_some() && !accept {
                tx.stale_paras.push(*para);
            }
            if mark.is_some() || style.is_some() {
                let (style, mark, list) = (style.unwrap_or_else(|| p.style.clone()), mark.map_or_else(|| p.mark.clone(), |m| m.1), p.list.clone());
                self.step(tx, Step::Para { para: *para, style, mark, list })?;
            }
        }
        for para in joins.into_iter().rev() {
            self.join_next(tx, para)?;
        }
        Ok(())
    }

    fn join_next(&mut self, tx: &mut Tx, para: ParaId) -> Result<(), StepError> {
        let Some(slot) = self.index.para_slot(para) else { return Ok(()) };
        let Some(blocks) = self.index.blocks(&self.doc, slot.container) else { return Ok(()) };
        match (blocks.get(slot.block), blocks.get(slot.block + 1)) {
            (_, Some(Block::Para(q))) => {
                let (second, style, mark, list) = (ParaId(q.id), q.style.clone(), q.mark.clone(), q.list.clone());
                self.step(tx, Step::Para { para, style, mark, list })?;
                self.step(tx, Step::Join { first: para, second })?;
            }
            (Some(Block::Para(p)), Some(Block::Table(_))) if p.inlines.iter().all(|i| matches!(i, Inline::Text(t, _) if t.is_empty())) => {
                let range = slot.block..slot.block + 1;
                self.step(tx, Step::Blocks { container: slot.container, range, with: Vec::new() })?;
            }
            _ => {}
        }
        Ok(())
    }

    fn resolve_table(&mut self, tx: &mut Tx, table: TableId, pick: &Pick, accept: bool) -> Result<(), StepError> {
        let Some(t) = self.index.table(&self.doc, table) else { return Ok(()) };
        let n = t.rows.len();
        let Some(plan) = self.table_plan(t, pick, accept) else { return Ok(()) };
        let slot = self.index.table_slot(table).ok_or(StepError::Missing)?;
        match plan.rows {
            Some(rows) if rows.is_empty() => {
                self.step(tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() })?;
                return Ok(());
            }
            Some(rows) => {
                self.step(tx, Step::Rows { table, range: 0..n, with: rows, grid: plan.grid })?;
            }
            None if plan.grid.is_some() => {
                let rows = self.index.table(&self.doc, table).ok_or(StepError::Missing)?.rows.clone();
                self.step(tx, Step::Rows { table, range: 0..n, with: rows, grid: plan.grid })?;
            }
            None => {}
        }
        if let Some(mut own) = plan.table {
            let now = self.index.table(&self.doc, table).ok_or(StepError::Missing)?;
            own.rows = now.rows.clone();
            own.grid = now.grid.clone();
            self.step(tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: vec![Block::Table(own)] })?;
        }
        if plan.restyle {
            tx.stale_tables.push(table);
        }
        Ok(())
    }

    fn table_plan(&self, t: &Table, pick: &Pick, accept: bool) -> Option<TablePlan> {
        let cx = self.cx();
        let id = TableId(t.id);
        let picked = |m: &Rc<RevisionMark>, spot: Spot| pick.block(m, spot);
        let row_picked = |ri: usize, r: &Row| {
            [&r.rev.ins, &r.rev.del].into_iter().flatten().any(|m| picked(m, Spot::Row(id, ri)))
                || [&r.rev.change, &r.rev.ex_change].into_iter().flatten().any(|c| picked(&c.mark, Spot::Row(id, ri)))
                || r.cells.iter().enumerate().any(|(ci, c)| {
                    let at = Spot::Cell(id, ri, ci);
                    c.rev.structural.as_ref().is_some_and(|s| picked(&s.0, at)) || c.rev.change.as_ref().is_some_and(|x| picked(&x.mark, at))
                })
        };
        let own = t.change.as_ref().is_some_and(|c| picked(&c.mark, Spot::Table(id)));
        let grid = t.grid_change.as_ref().filter(|g| picked(&g.mark, Spot::Table(id)));
        if !own && grid.is_none() && !t.rows.iter().enumerate().any(|(ri, r)| row_picked(ri, r)) {
            return None;
        }
        let (mut restyle, mut merges) = (false, false);
        let mut rows = Vec::with_capacity(t.rows.len());
        // Cells the resolution removes, by row and cell
        let mut gone: Vec<(usize, usize)> = Vec::new();
        for (ri, r) in t.rows.iter().enumerate() {
            if !row_picked(ri, r) {
                rows.push(r.clone());
                continue;
            }
            let at_row = Spot::Row(id, ri);
            let mut r = r.clone();
            let mut keep = true;
            if r.rev.ins.as_ref().is_some_and(|m| picked(m, at_row)) {
                r.rev.ins = None;
                keep &= accept;
            }
            if r.rev.del.as_ref().is_some_and(|m| picked(m, at_row)) {
                r.rev.del = None;
                keep &= !accept;
            }
            if let Some(c) = r.rev.change.clone().filter(|c| picked(&c.mark, at_row)) {
                r.rev.change = None;
                if !accept {
                    if let Some(p) = &c.previous {
                        p.restore_row(&mut r);
                    }
                    // Its properties as written before, its own revisions kept
                    let now = crate::docx::write::props::Props::parse(r.direct.0.as_deref(), "trPr", &cx);
                    let mut was = crate::docx::write::props::Props::parse(c.previous_raw.0.as_deref(), "trPr", &cx);
                    for (name, xml) in now.kids.iter().filter(|(n, _)| matches!(n.as_str(), "ins" | "del")) {
                        was.set(name, xml.clone());
                    }
                    r.direct = Raw(was.xml().map(|x| Rc::from(x.as_str())));
                    restyle = true;
                }
            }
            if r.rev.ex_change.as_ref().is_some_and(|c| picked(&c.mark, at_row)) {
                r.rev.ex_change = None;
            }
            let (mut k, mut ci) = (0, 0);
            while k < r.cells.len() {
                let at = Spot::Cell(id, ri, ci);
                ci += 1;
                let c = &mut r.cells[k];
                if let Some(ch) = c.rev.change.clone().filter(|x| picked(&x.mark, at)) {
                    c.rev.change = None;
                    if !accept {
                        if let Some(p) = &ch.previous {
                            p.restore_cell(c);
                        }
                        let raw = ch.previous_raw.0.clone().unwrap_or_else(|| Rc::from(format!("<{}tcPr/>", cx.w).as_str()));
                        c.direct = Raw(Some(raw));
                        restyle = true;
                    }
                }
                if let Some((m, kind)) = c.rev.structural.clone()
                    && picked(&m, at)
                {
                    c.rev.structural = None;
                    match (kind, accept) {
                        (CellMarkKind::Insert, false) | (CellMarkKind::Delete, true) if keep => gone.push((rows.len(), k)),
                        (CellMarkKind::Merge { v_merge_orig, .. }, false) => {
                            c.v_merge = v_merge_orig.unwrap_or(VMerge::None);
                            merges = true;
                        }
                        _ => {}
                    }
                }
                k += 1;
            }
            if keep && !r.cells.is_empty() {
                rows.push(r);
            }
        }
        let mut new_grid = grid.filter(|_| !accept).map(|g| g.previous.clone());
        if !gone.is_empty() {
            let mut g = new_grid.clone().unwrap_or_else(|| t.grid.clone());
            if crate::editing::tables::remove_cells(&mut rows, &gone, &mut g) {
                new_grid = Some(g);
            }
            merges = true;
        }
        if merges {
            crate::editing::tables::tidy_merges(&mut rows);
        }
        let table = own.then(|| {
            let mut x = t.clone();
            x.change = None;
            if !accept && let Some(c) = &t.change {
                if let Some(p) = &c.previous {
                    p.restore_table(&mut x);
                }
                x.direct = c.previous_raw.clone();
                restyle = true;
            }
            x
        });
        let table = match (table, grid) {
            (Some(mut x), Some(_)) => {
                x.grid_change = None;
                Some(x)
            }
            (None, Some(_)) => {
                let mut x = t.clone();
                x.grid_change = None;
                Some(x)
            }
            (x, None) => x,
        };
        let changed = rows != t.rows;
        Some(TablePlan { rows: changed.then_some(rows), grid: new_grid, table, restyle })
    }

    fn follow_steps(&mut self, tx: &Tx) {
        let map = |p: Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        let s = match self.selection {
            Selection::Caret(p) => Selection::Caret(map(p)),
            Selection::Text { anchor, focus, unit } => Selection::range(map(anchor), map(focus), unit),
            Selection::Cells { table, .. } if self.index.table(&self.doc, table).is_some_and(|t| !t.rows.is_empty()) => {
                self.normalize(self.selection)
            }
            other => match other.focus() {
                Some(p) => Selection::Caret(map(p)),
                None => Selection::Caret(Position::text(StoryId::Body, ParaId(0), Unit(0))),
            },
        };
        self.selection = s;
        self.repair_selection();
    }

    pub(in crate::editing) fn go_to_revision(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        let here = self.selection.ordered(&self.index).map(|(a, _)| a).or_else(|| self.selection.focus());
        let caret = self.selection.is_collapsed();
        let (list, stories) = self.review_list.get(self.generation, &self.doc, &self.index);
        let rank = |s: StoryId| stories.iter().position(|x| *x == s).unwrap_or(0);
        let idx = &self.index;
        let found = match here {
            None => {
                if back {
                    list.last()
                } else {
                    list.first()
                }
            }
            Some(h) => {
                let order = |r: &super::index::Revision| {
                    rank(r.story).cmp(&rank(h.story)).then_with(|| if r.story == h.story { idx.cmp(&r.start, &h) } else { std::cmp::Ordering::Equal })
                };
                if back { list.iter().rfind(|r| order(r).is_lt()) } else { list.iter().find(|r| order(r).is_gt() || caret && order(r).is_eq()) }
            }
        };
        let Some(r) = found.cloned() else { return Outcome::Unchanged("no more revisions") };
        self.set_selection(Selection::range(r.start, r.end, super::super::selection::Granularity::Char));
        if let Some(p) = self.selection.focus() {
            self.ensure(&p, fonts);
            self.scroll_to_caret(fonts);
        }
        Outcome::Moved
    }
}

fn covered(doc: &Document, idx: &DocIndex, s: Position, e: Position, para: ParaId) -> Option<Range<u32>> {
    let len = para_len(idx.para(doc, para)?).0;
    let story = idx.story(idx.para_slot(para)?.container);
    let (p0, p1) = (Position::text(story, para, Unit(0)), Position::text(story, para, Unit(len)));
    if story != s.story || idx.cmp(&e, &p0).is_le() || idx.cmp(&s, &p1).is_ge() {
        return None;
    }
    let a = s.para_offset().filter(|(q, _)| *q == para).map_or(0, |(_, u)| u.0);
    let b = e.para_offset().filter(|(q, _)| *q == para).map_or(len, |(_, u)| u.0);
    (a < b).then_some(a..b)
}
