//! Accepting and rejecting revisions. Resolving is built from ordinary steps (rows, blocks, restyle, text, paragraph
//! and join) with nothing recorded, as one transaction: one undo step, whatever it covers.
//!
//! - A run's insertion: accepting clears it, rejecting removes the text; a deletion the other way round. With both
//!   (an insertion another author deleted) each resolves on its own.
//! - A formatting change: accepting drops it, rejecting restores the formatting before it.
//! - A paragraph mark: accepting its deletion (or rejecting its insertion) joins the paragraph with the next one,
//!   which gives the joined paragraph its properties.
//! - Paragraph, section, table, row, cell and grid property changes: accepting drops them, rejecting restores what
//!   was before.
//! - Rows and cells inserted or deleted are kept or removed whole; a cell removed gives its columns to its neighbour.
//! - A move resolves both ends together, and its range markers go.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;

use crate::docx::props::Width;
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
use super::index::{Found, found, revisions};

/// Which revisions a resolve covers.
#[derive(Clone, Debug, PartialEq)]
pub enum Scope {
    /// Every revision the selection touches (runs split at its edges), or the one at a caret.
    Selection,
    All,
    /// Every revision by the person of this name; others' revisions nested in theirs stay.
    Author(String),
    /// The revision holding this mark.
    Revision(RevisionId),
}

impl Scope {
    /// A host argument: `"selection"` (or none), `"all"`, `"author:<name>"`, or a revision id (number or text).
    pub fn from_host(arg: Option<&serde_json::Value>) -> Option<Scope> {
        let Some(a) = arg.filter(|a| !a.is_null()) else { return Some(Scope::Selection) };
        if let Some(n) = a.as_i64() {
            return Some(Scope::Revision(RevisionId(n)));
        }
        let s = a.as_str()?.trim();
        Some(match s {
            "selection" => Scope::Selection,
            "all" => Scope::All,
            _ => match s.strip_prefix("author:") {
                Some(name) => Scope::Author(name.to_string()),
                None => Scope::Revision(RevisionId(s.parse().ok()?)),
            },
        })
    }
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

/// What a resolve covers, mark by mark.
struct Pick {
    who: Who,
    /// A range: the changes besides runs' (marks, properties, rows, cells) it touches. Runs' changes resolve only in
    /// the units [`Pick::inside`] gives.
    blocks: Option<HashSet<RevisionId>>,
    inside: Option<HashMap<ParaId, Range<u32>>>,
    /// Moves resolved whole, by name, and the ids of their range markers.
    moves: HashSet<Rc<str>>,
    move_ranges: HashSet<i64>,
}

impl Pick {
    fn moved(&self, m: &RevisionMark) -> bool {
        m.move_name.as_ref().is_some_and(|n| self.moves.contains(n))
    }

    /// A change of a paragraph mark, a paragraph, a row, a cell or a table.
    fn block(&self, m: &RevisionMark) -> bool {
        self.moved(m) || self.who.has(m) && self.blocks.as_ref().is_none_or(|b| b.contains(&m.id))
    }

    /// A run's change, the run being in the range (or there being none).
    fn run(&self, m: &RevisionMark, in_range: bool) -> bool {
        self.moved(m) || in_range && self.who.has(m)
    }

    /// Completes a pick: the moves it touches (`touches` says which changes count), whole.
    fn with_moves(mut self, found: &Found, touches: impl Fn(&super::index::Item) -> bool) -> Pick {
        let all = matches!(self.who, Who::All) && self.blocks.is_none();
        self.moves = found.items.iter().filter(|it| touches(it)).filter_map(|it| it.mark.move_name.clone()).collect();
        if all {
            self.moves.extend(found.move_ranges.iter().map(|(n, _)| n.clone()));
        }
        self.move_ranges = found.move_ranges.iter().filter(|(n, _)| self.moves.contains(n)).map(|(_, id)| *id).collect();
        self
    }
}

/// What resolving does to a run: `None` leaves it, else whether its content goes and the run it keeps otherwise.
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

/// An inline carrying `run` instead of its own.
fn with_run(i: &Inline, run: &Run) -> Inline {
    match i {
        Inline::Float(f, _) => Inline::Float(f.clone(), run.clone()),
        Inline::CommentRef { id, .. } => Inline::CommentRef { id: *id, run: run.clone() },
        Inline::Separator { continuation, .. } => Inline::Separator { continuation: *continuation, run: run.clone() },
        other => super::super::format::with_run(other, run),
    }
}

/// A paragraph's inlines resolved: the runs they carry now (units unchanged, removed objects and resolved move
/// markers out), and the unit ranges whose content goes, in order. `None` when nothing changes.
fn resolve_inlines(p: &Paragraph, pick: &Pick, accept: bool) -> Option<(Vec<Inline>, Vec<Range<u32>>)> {
    let inside = pick.inside.as_ref().map(|i| i.get(&ParaId(p.id)).cloned());
    let cuts: Vec<u32> = inside.clone().flatten().map(|r| vec![r.start, r.end]).unwrap_or_default();
    let mut cache: HashMap<(*const RunStyle, bool), Option<(bool, Run)>> = HashMap::new();
    let mut out: Vec<(Inline, bool)> = Vec::with_capacity(p.inlines.len());
    let (mut at, mut changed) = (0u32, false);
    for i in &p.inlines {
        // A text crossing the range's edges is cut there.
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
                        // A floating object goes with its run; a comment's reference stays where its comment is.
                        (true, true) => {}
                        (false, true) if n > 0 => out.push((piece, true)),
                        _ => out.push((with_run(&piece, &run), false)),
                    }
                }
            }
            at += n;
        }
    }
    if !changed {
        return None;
    }
    // Texts now in one run join again (those staying, and those going, each among themselves).
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

/// Every table of the document in document order (an outer table before those in its cells), and every paragraph.
fn tables_and_paras(doc: &Document) -> (Vec<TableId>, Vec<ParaId>) {
    fn walk<'a>(blocks: &'a [Block], t: &mut Vec<TableId>, p: &mut Vec<ParaId>, boxes: &mut Vec<&'a [Block]>) {
        for b in blocks {
            match b {
                Block::Para(q) => {
                    p.push(ParaId(q.id));
                    for i in &q.inlines {
                        if let Inline::Float(f, _) = i
                            && let Some(tb) = &f.text_box
                        {
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
    let stories = doc.sections.iter().map(|s| &s.blocks).chain(doc.stories.values()).chain(doc.footnotes.values()).chain(doc.endnotes.values());
    for blocks in stories {
        walk(blocks, &mut t, &mut p, &mut boxes);
    }
    while let Some(b) = boxes.pop() {
        walk(b, &mut t, &mut p, &mut boxes);
    }
    (t, p)
}

/// The grid column each cell of a row starts at.
fn columns(r: &Row) -> Vec<usize> {
    let mut at = r.grid_before;
    r.cells
        .iter()
        .map(|c| {
            let col = at;
            at += c.span.max(1);
            col
        })
        .collect()
}

/// Vertical merges that no longer hold: a start with nothing continuing it below, a continuation with nothing above.
fn tidy_merges(rows: &mut [Row]) {
    let cols: Vec<Vec<usize>> = rows.iter().map(columns).collect();
    let state = |rows: &[Row], ri: usize, col: usize| {
        cols[ri].iter().position(|&c| c == col).and_then(|k| rows.get(ri).and_then(|r| r.cells.get(k))).map_or(VMerge::None, |c| c.v_merge)
    };
    for ri in 0..rows.len() {
        for k in 0..rows[ri].cells.len() {
            let col = cols[ri][k];
            let above = if ri == 0 { VMerge::None } else { state(rows, ri - 1, col) };
            let below = if ri + 1 < rows.len() { state(rows, ri + 1, col) } else { VMerge::None };
            let c = &mut rows[ri].cells[k];
            match c.v_merge {
                VMerge::Restart if below != VMerge::Continue => c.v_merge = VMerge::None,
                VMerge::Continue if above == VMerge::None => c.v_merge = VMerge::None,
                _ => {}
            }
        }
    }
}

/// A table resolved: its rows (none: the table goes), its grid when that changed, and the table itself when its own
/// properties changed.
struct TablePlan {
    rows: Option<Vec<Row>>,
    grid: Option<Vec<f64>>,
    table: Option<Table>,
    /// Cells got their earlier properties: their formatting resolves again from them.
    restyle: bool,
}

impl Editor {
    /// Accepts or rejects the revisions `scope` covers, as one transaction.
    pub(in crate::editing) fn resolve_revisions(&mut self, accept: bool, scope: &Scope, fonts: &Fonts) -> Outcome {
        if self.doc.settings.protection.as_ref().is_some_and(|p| p.forces_tracking()) {
            self.answer = Some(serde_json::json!({ "error": "unavailable" }));
            return Outcome::Unchanged("unavailable");
        }
        let Some(pick) = self.pick(scope) else { return Outcome::Unchanged("no revision") };
        let mut tx = self.begin(if accept { "Accept" } else { "Reject" });
        // Resolving is never recorded, whether tracking is on or not.
        tx.tracked = false;
        if self.apply_pick(&mut tx, &pick, accept).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("revisions don't resolve");
        }
        self.follow_steps(&tx);
        self.commit(tx, None, fonts)
    }

    /// The marks `scope` covers, `None` when it covers no revision.
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
            Scope::Selection => {
                let (s, e) = match self.selection {
                    Selection::Caret(c) => {
                        // The innermost revision holding the caret.
                        let list = revisions(&self.doc, &self.index);
                        let idx = &self.index;
                        let r = list.iter().rfind(|r| r.story == c.story && idx.cmp(&r.start, &c).is_le() && idx.cmp(&c, &r.end).is_le())?;
                        let picked: HashSet<RevisionId> = r.ids.iter().copied().collect();
                        return Some(base(ids(&r.ids)).with_moves(&f, |it| picked.contains(&it.mark.id)));
                    }
                    Selection::Text { .. } => self.selection.ordered(&self.index)?,
                    Selection::Cells { table, anchor, focus, .. } => {
                        // shortcut: a cell block resolves from its first cell to its last in document order, the
                        // cells between them in the rows between included.
                        let t = self.index.table(&self.doc, table)?;
                        let story = self.index.story(self.index.table_slot(table)?.container);
                        let cell = |row: usize, col: usize| t.rows.get(row).and_then(|r| r.cells.get(col.min(r.cells.len().saturating_sub(1))));
                        let first = cell(anchor.row.min(focus.row), anchor.col.min(focus.col))?.blocks.iter().find_map(first_para)?;
                        let last = cell(anchor.row.max(focus.row), anchor.col.max(focus.col))?.blocks.iter().rev().find_map(last_para)?;
                        (Position::text(story, ParaId(first.id), Unit(0)), Position::text(story, ParaId(last.id), para_len(last)))
                    }
                    Selection::Object(_) | Selection::Block { .. } => return None,
                };
                self.range_pick(&f, s, e)
            }
        };
        let touched = |m: &RevisionMark| pick.block(m) || pick.run(m, true);
        f.items.iter().any(|it| touched(&it.mark)).then_some(pick)
    }

    /// The changes from `s` to `e`: runs' inside it, everything else it touches, and the moves it touches whole.
    fn range_pick(&self, f: &Found, s: Position, e: Position) -> Pick {
        let idx = &self.index;
        let touches = |it: &super::index::Item| it.story == s.story && idx.cmp(&it.start, &e).is_lt() && idx.cmp(&s, &it.end).is_lt();
        let blocks: HashSet<RevisionId> = f.items.iter().filter(|it| it.run.is_none() && touches(it)).map(|it| it.mark.id).collect();
        let paras: HashSet<ParaId> = f.items.iter().filter_map(|it| it.run.as_ref().map(|r| r.0)).collect();
        let inside: HashMap<ParaId, Range<u32>> = paras.into_iter().filter_map(|p| Some((p, covered(&self.doc, idx, s, e, p)?))).collect();
        let p = Pick { who: Who::All, blocks: Some(blocks), inside: Some(inside), moves: HashSet::new(), move_ranges: HashSet::new() };
        p.with_moves(f, |it| touches(it))
    }

    /// Applies a pick: sections' page changes, tables (rows, cells and their properties), then runs, then paragraph
    /// marks and properties, then the joins deleted marks make, last first.
    fn apply_pick(&mut self, tx: &mut Tx, pick: &Pick, accept: bool) -> Result<(), StepError> {
        for section in 0..self.doc.sections.len() {
            let s = &self.doc.sections[section];
            let Some(c) = s.change.as_ref().filter(|c| pick.block(&c.mark)) else { continue };
            let page = if accept { s.page() } else { c.previous.clone() };
            self.step(tx, Step::SectionPage { section, page, change: None })?;
        }
        let (tables, _) = tables_and_paras(&self.doc);
        for t in tables {
            self.resolve_table(tx, t, pick, accept)?;
        }
        let (_, paras) = tables_and_paras(&self.doc);
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
            let mark = decide(&p.mark, accept, |m| pick.block(m));
            let style = p.style.change.as_ref().filter(|c| pick.block(&c.mark)).map(|c| {
                let mut s = if accept { (*p.style).clone() } else { (*c.previous).clone() };
                s.change = None;
                Rc::new(s)
            });
            if mark.as_ref().is_some_and(|m| m.0) {
                joins.push(*para);
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

    /// A paragraph whose mark goes joins the next one, taking its properties. With no paragraph after it in its
    /// container its mark stays (cleared already); an empty one before a table goes.
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
            // The table as the rows step left it, with its own properties resolved.
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

    /// A table's revisions resolved; `None` when it has none the pick covers.
    fn table_plan(&self, t: &Table, pick: &Pick, accept: bool) -> Option<TablePlan> {
        let cx = self.cx();
        let picked = |m: &Rc<RevisionMark>| pick.block(m);
        let row_picked = |r: &Row| {
            [&r.rev.ins, &r.rev.del].into_iter().flatten().any(picked)
                || [&r.rev.change, &r.rev.ex_change].into_iter().flatten().any(|c| picked(&c.mark))
                || r.cells
                    .iter()
                    .any(|c| c.rev.structural.as_ref().is_some_and(|s| picked(&s.0)) || c.rev.change.as_ref().is_some_and(|x| picked(&x.mark)))
        };
        let own = t.change.as_ref().is_some_and(|c| picked(&c.mark));
        let grid = t.grid_change.as_ref().filter(|g| picked(&g.mark));
        if !own && grid.is_none() && !t.rows.iter().any(row_picked) {
            return None;
        }
        let (mut restyle, mut merges) = (false, false);
        let mut rows = Vec::with_capacity(t.rows.len());
        for r in &t.rows {
            if !row_picked(r) {
                rows.push(r.clone());
                continue;
            }
            let mut r = r.clone();
            let mut keep = true;
            if r.rev.ins.as_ref().is_some_and(picked) {
                r.rev.ins = None;
                keep &= accept;
            }
            if r.rev.del.as_ref().is_some_and(picked) {
                r.rev.del = None;
                keep &= !accept;
            }
            if let Some(c) = r.rev.change.clone().filter(|c| picked(&c.mark)) {
                r.rev.change = None;
                if !accept && let Some(p) = &c.previous {
                    p.restore_row(&mut r);
                }
            }
            // shortcut: rejecting a change of a row's table exceptions drops it without restoring them (the model
            // keeps no row exceptions).
            if r.rev.ex_change.as_ref().is_some_and(|c| picked(&c.mark)) {
                r.rev.ex_change = None;
            }
            let mut k = 0;
            while k < r.cells.len() {
                let c = &mut r.cells[k];
                if let Some(ch) = c.rev.change.clone().filter(|x| picked(&x.mark)) {
                    c.rev.change = None;
                    if !accept {
                        if let Some(p) = &ch.previous {
                            p.restore_cell(c);
                        }
                        // Its formatting resolves again from the earlier properties at its place now.
                        let raw = ch.previous_raw.0.clone().unwrap_or_else(|| Rc::from(format!("<{}tcPr/>", cx.w).as_str()));
                        c.direct = Raw(Some(raw));
                        restyle = true;
                    }
                }
                if let Some((m, kind)) = c.rev.structural.clone()
                    && picked(&m)
                {
                    c.rev.structural = None;
                    match (kind, accept) {
                        (CellMarkKind::Insert, false) | (CellMarkKind::Delete, true) => {
                            let gone = r.cells.remove(k);
                            let to = if k > 0 {
                                Some(k - 1)
                            } else if r.cells.is_empty() {
                                None
                            } else {
                                Some(0)
                            };
                            if let Some(n) = to.and_then(|j| r.cells.get_mut(j)) {
                                n.span = n.span.max(1) + gone.span.max(1);
                                if let (Some(Width::Dxa(a)), Some(Width::Dxa(b))) = (&mut n.width, gone.width) {
                                    *a += b;
                                }
                            }
                            continue;
                        }
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
        if merges {
            tidy_merges(&mut rows);
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
        let new_grid = grid.filter(|_| !accept).map(|g| g.previous.clone());
        let changed = rows != t.rows;
        Some(TablePlan { rows: changed.then_some(rows), grid: new_grid, table, restyle })
    }

    /// The selection carried through the transaction's steps; a place they removed becomes the start of the story.
    fn follow_steps(&mut self, tx: &Tx) {
        let map = |p: Position| tx.maps.iter().fold(p, |p, m| m.apply(p));
        let s = match self.selection {
            Selection::Caret(p) => Selection::Caret(map(p)),
            Selection::Text { anchor, focus, unit } => Selection::range(map(anchor), map(focus), unit),
            Selection::Cells { table, .. } if self.index.table(&self.doc, table).is_some() => self.selection,
            other => match other.focus() {
                Some(p) => Selection::Caret(map(p)),
                None => Selection::Caret(Position::text(StoryId::Body, ParaId(0), Unit(0))),
            },
        };
        let valid = |p: &Position| p.para_offset().is_none_or(|(para, u)| self.index.para(&self.doc, para).is_some_and(|q| u <= para_len(q)));
        let ok = matches!(s, Selection::Cells { .. }) || s.focus().is_some_and(|p| valid(&p)) && s.anchor().is_none_or(|p| valid(&p));
        self.selection = if ok {
            s
        } else {
            let story = s.focus().map_or(StoryId::Body, |p| p.story);
            let edge = self.nav().story_edge(story, false).or_else(|| self.nav().story_edge(StoryId::Body, false));
            Selection::Caret(edge.unwrap_or(Position::text(StoryId::Body, ParaId(0), Unit(0))))
        };
    }

    /// Selects and reveals the next (or, `back`, the previous) revision after the selection's start, across stories.
    pub(in crate::editing) fn go_to_revision(&mut self, back: bool, fonts: &Fonts) -> Outcome {
        let here = self.selection.ordered(&self.index).map(|(a, _)| a).or_else(|| self.selection.focus());
        // A caret at a revision's start goes to that revision; a selection moves past it.
        let caret = self.selection.is_collapsed();
        let (list, stories) = self.review_list.get(self.revision, &self.doc, &self.index);
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

fn first_para(b: &Block) -> Option<&Paragraph> {
    match b {
        Block::Para(p) => Some(p),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()).find_map(first_para),
    }
}

fn last_para(b: &Block) -> Option<&Paragraph> {
    match b {
        Block::Para(p) => Some(p),
        Block::Table(t) => t.rows.iter().rev().flat_map(|r| r.cells.iter().rev()).flat_map(|c| c.blocks.iter().rev()).find_map(last_para),
    }
}

/// The units of `para` from `s` to `e`, when they hold any.
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
