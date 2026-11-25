//! Table structure commands

use std::rc::Rc;

use crate::api::command::TablePosition;
use crate::docx::props::{Jc, TableAnchor, TablePos, Width};
use crate::docx::review::RunRevisions;
use crate::docx::{Block, Cell, Inline, ParaStyle, Paragraph, Row, Run, RunStyle, Table, VMerge};
use crate::font::Fonts;
use crate::layout::{ParaId, TableId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::position::Position;
use super::selection::{CellRef, Selection};
use super::step::{Step, para_len};
use super::table::Grid;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Target {
    pub table: TableId,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

/// Most columns a split may grow a table's grid to (Word's limit).
pub(super) const MAX_COLUMNS: usize = 63;

/// Most rows a split may grow a table to (the most rows one command inserts).
pub(super) const MAX_ROWS: usize = 32767;

/// `r` without revision marks.
fn unmarked(r: &Run) -> Run {
    if r.rev == RunRevisions::default() { r.clone() } else { Rc::new(RunStyle { rev: RunRevisions::default(), ..(**r).clone() }) }
}

/// A new empty paragraph formatted like `p`, without its revision marks.
pub(super) fn blank_para(p: &Paragraph) -> Paragraph {
    let mark = unmarked(&p.mark);
    let style = if p.style.change.is_some() { Rc::new(ParaStyle { change: None, ..(*p.style).clone() }) } else { p.style.clone() };
    Paragraph {
        style,
        inlines: vec![Inline::Text(String::new(), mark.clone())],
        mark,
        list: None,
        src: None,
        id: crate::docx::next_para_id(),
        div: None,
        controls: Vec::new(),
    }
}

pub(super) fn empty_cell(like: &Cell) -> Cell {
    let mut c = like.clone();
    c.v_merge = VMerge::None;
    c.controls = Vec::new();
    c.ids = None;
    c.rev = Default::default();
    let Some(p) = like.blocks.iter().rev().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) else { return c };
    c.blocks = vec![Block::Para(blank_para(p))];
    c
}

/// The table's grid with a width for every column its rows reach: columns `w:tblGrid` leaves out are 0 wide, as
/// layout reads them.
pub(super) fn full_grid(t: &Table, cols: usize) -> Vec<f64> {
    let mut g = Vec::with_capacity(cols.max(t.grid.len()) + 1);
    g.extend_from_slice(&t.grid);
    if g.len() < cols {
        g.resize(cols, 0.0);
    }
    g
}

/// Vertical merges made whole: a continuation under no merged cell over the same columns, and a merge start that
/// nothing continues, stop being merged.
pub(super) fn tidy_merges(rows: &mut [Row]) {
    let state = |line: &[(usize, usize, VMerge)], col: usize, span: usize| {
        line.binary_search_by_key(&col, |a| a.0).ok().filter(|&i| line[i].1 == span).map_or(VMerge::None, |i| line[i].2)
    };
    let mut next: Vec<(usize, usize, VMerge)> = Vec::new();
    let mut line: Vec<(usize, usize, VMerge)> = Vec::new();
    for row in rows.iter_mut() {
        let mut col = row.grid_before;
        for c in &mut row.cells {
            let span = c.span.max(1);
            if c.v_merge == VMerge::Continue && state(&line, col, span) == VMerge::None {
                c.v_merge = VMerge::None;
            }
            next.push((col, span, c.v_merge));
            col += span;
        }
        std::mem::swap(&mut line, &mut next);
        next.clear();
    }
    line.clear();
    for row in rows.iter_mut().rev() {
        let mut col = row.grid_before;
        for c in &mut row.cells {
            let span = c.span.max(1);
            if c.v_merge == VMerge::Restart && state(&line, col, span) != VMerge::Continue {
                c.v_merge = VMerge::None;
            }
            next.push((col, span, c.v_merge));
            col += span;
        }
        std::mem::swap(&mut line, &mut next);
        next.clear();
    }
}

/// Removes the cells `gone` names (by row, then cell, in order), as accepting their deletion or rejecting their
/// insertion does. A grid column only removed cells needed goes too, narrowing the cells over it; elsewhere the cell
/// before the gap (else after it) widens over it, as Word does. Rows left without cells go. Whether the grid changed.
pub(super) fn remove_cells(rows: &mut Vec<Row>, gone: &[(usize, usize)], grid: &mut Vec<f64>) -> bool {
    let g = Grid::of_rows(rows, grid.len());
    if grid.len() < g.cols {
        grid.resize(g.cols, 0.0);
    }
    let is_gone = |r: usize, k: usize| gone.binary_search(&(r, k)).is_ok();
    let mut drop = vec![false; g.cols];
    for &(r, k) in gone {
        let (c, s, _) = g.rows[r][k];
        drop[c..c + s].fill(true);
    }
    // Columns a kept cell lies wholly in stay
    let mut keep = vec![false; g.cols];
    for (r, cells) in g.rows.iter().enumerate() {
        for (k, &(c, s, _)) in cells.iter().enumerate() {
            if !is_gone(r, k) && drop[c..c + s].iter().all(|&d| d) {
                keep[c..c + s].fill(true);
            }
        }
    }
    drop.iter_mut().zip(&keep).for_each(|(d, &k)| *d &= !k);
    let dropped = |a: usize, b: usize| drop[a.min(b)..b].iter().filter(|&&d| d).count();
    let kept_width = |a: usize, b: usize| (a..b).filter(|&x| !drop[x]).map(|x| grid[x]).sum::<f64>();
    let widen = |cell: &mut Cell, span: usize, w: Option<f64>| {
        cell.span = cell.span.max(1) + span;
        if let (Some(Width::Dxa(a)), Some(b)) = (&mut cell.width, w) {
            *a += b;
        }
    };
    let mut emptied = vec![false; rows.len()];
    for (r, row) in rows.iter_mut().enumerate() {
        let cells = &g.rows[r];
        let end = cells.last().map_or(row.grid_before, |&(c, s, _)| c + s);
        row.grid_after -= dropped(end, (end + row.grid_after).min(g.cols));
        row.grid_before -= dropped(0, row.grid_before.min(g.cols));
        if gone.get(gone.partition_point(|x| x.0 < r)).is_none_or(|x| x.0 != r) && dropped(row.grid_before, end) == 0 {
            continue;
        }
        let had = !row.cells.is_empty();
        let mut gap: (usize, Option<f64>) = (0, None);
        let mut out: Vec<Cell> = Vec::with_capacity(row.cells.len());
        for (k, mut cell) in std::mem::take(&mut row.cells).into_iter().enumerate() {
            let (c, s, _) = cells[k];
            let d = dropped(c, c + s);
            if is_gone(r, k) {
                if s > d {
                    let w =
                        if d == 0 { cell.width.and_then(|w| if let Width::Dxa(w) = w { Some(w) } else { None }) } else { Some(kept_width(c, c + s)) };
                    match out.last_mut() {
                        Some(n) => widen(n, s - d, w),
                        None => gap = (gap.0 + s - d, Some(gap.1.unwrap_or(0.0) + w.unwrap_or(0.0))),
                    }
                }
                continue;
            }
            if d > 0 {
                let (was, now) = (grid[c..c + s].iter().sum::<f64>(), kept_width(c, c + s));
                cell.span = s - d;
                cell.width = match cell.width {
                    Some(Width::Dxa(_)) => Some(Width::Dxa(now)),
                    Some(Width::Pct(p)) if was > 0.0 => Some(Width::Pct(p * now / was)),
                    w => w,
                };
            }
            if gap.0 > 0 {
                widen(&mut cell, gap.0, gap.1);
                gap = (0, None);
            }
            out.push(cell);
        }
        emptied[r] = had && out.is_empty();
        row.cells = out;
    }
    let mut r = 0;
    rows.retain(|_| {
        r += 1;
        !emptied[r - 1]
    });
    let any = drop.contains(&true);
    if any {
        let mut x = 0;
        grid.retain(|_| {
            x += 1;
            !drop[x - 1]
        });
    }
    any
}

/// `grid` with the widths its columns are drawn at on `geo`.
pub(super) fn drawn_grid(geo: &super::geometry::PageGeometry, table: TableId, grid: &mut [f64]) {
    let mut xs: Vec<Option<f64>> = vec![None; grid.len() + 1];
    for r in geo.rows.iter().filter(|r| r.table == table && !r.repeat) {
        // A row with an indent of its own is drawn shifted: its edges are taken where the rows before put theirs.
        let shift = r.edges.iter().find_map(|&(x, k)| xs.get(k).copied().flatten().map(|known| known - x)).unwrap_or(0.0);
        for &(x, k) in &r.edges {
            if let Some(slot) = xs.get_mut(k)
                && slot.is_none()
            {
                *slot = Some(x + shift);
            }
        }
    }
    for (k, w) in grid.iter_mut().enumerate() {
        if let (Some(a), Some(b)) = (xs[k], xs[k + 1]) {
            *w = ((b - a).max(0.0) * 20.0).round() / 20.0;
        }
    }
}

/// The cells of the vertical merge starting at `(r, k)`: it and the continuations under it.
pub(super) fn merge_chain(g: &Grid, r: usize, k: usize) -> Vec<(usize, usize)> {
    let c = g.rows[r][k].0;
    std::iter::once((r, k))
        .chain((r + 1..g.rows.len()).map_while(|rr| {
            let kk = g.cell_at(rr, c)?;
            (g.rows[rr][kk].0 == c && g.continues(rr, kk)).then_some((rr, kk))
        }))
        .collect()
}

fn first_para(blocks: &[Block]) -> Option<ParaId> {
    blocks.iter().find_map(|b| match b {
        Block::Para(p) => Some(ParaId(p.id)),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| first_para(&c.blocks)),
    })
}

impl Editor {
    pub(super) fn table_target(&self) -> Option<Target> {
        if let Selection::Cells { table, anchor, focus, .. } = self.selection {
            let t = self.index.table(&self.doc, table)?;
            let (nr, nc) = (t.rows.len().checked_sub(1)?, Grid::of(t).cols.checked_sub(1)?);
            let (r0, r1) = (anchor.row.min(focus.row).min(nr), anchor.row.max(focus.row).min(nr));
            let (c0, c1) = (anchor.col.min(focus.col).min(nc), anchor.col.max(focus.col).min(nc));
            return Some(Target { table, rows: (r0, r1), cols: (c0, c1) });
        }
        let focus = self.selection.focus()?;
        let anchor = self.selection.anchor().unwrap_or(focus);
        let level = *self.index.chain_of(&focus).last()?;
        let t = self.index.table(&self.doc, level.table)?;
        let g = Grid::of(t);
        let span = |row: usize, cell: usize| -> Option<(usize, usize)> {
            let cell = cell.min(t.rows.get(row)?.cells.len().checked_sub(1)?);
            let (c, s, _) = *g.rows.get(row)?.get(cell)?;
            Some((c, c + s - 1))
        };
        let (fc0, fc1) = span(level.row, level.cell)?;
        let other = self.index.chain_of(&anchor).into_iter().find(|l| l.table == level.table);
        let (ar, (ac0, ac1)) = match other {
            Some(l) => (l.row, span(l.row, l.cell).unwrap_or((fc0, fc1))),
            None => (level.row, (fc0, fc1)),
        };
        Some(Target { table: level.table, rows: (ar.min(level.row), ar.max(level.row)), cols: (ac0.min(fc0), ac1.max(fc1)) })
    }

    pub(super) fn rebuild(
        &mut self,
        label: &'static str,
        table: TableId,
        rows: Vec<Row>,
        grid: Vec<f64>,
        caret: (usize, usize),
        fonts: &Fonts,
    ) -> Outcome {
        let tx = self.begin(label);
        self.rebuild_in(tx, table, rows, grid, caret, fonts)
    }

    /// [`Editor::rebuild`] in `tx`, begun with the steps the rows need first.
    pub(super) fn rebuild_in(
        &mut self,
        mut tx: super::Tx,
        table: TableId,
        rows: Vec<Row>,
        grid: Vec<f64>,
        caret: (usize, usize),
        fonts: &Fonts,
    ) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else {
            self.abort(tx);
            return Outcome::Unchanged("no table");
        };
        let n = t.rows.len();
        let mut rows = rows;
        tidy_merges(&mut rows);
        let story = self.index.table_slot(table).map_or(self.editing(), |s| self.index.story(s.container));
        let target = {
            let g = Grid::of_rows(&rows, grid.len());
            let r = caret.0.min(rows.len().saturating_sub(1));
            g.cell_at(r, caret.1).or_else(|| rows.get(r).map(|x| x.cells.len().saturating_sub(1))).map(|c| g.merge_start(r, c))
        };
        let caret_para = target.and_then(|(r, c)| rows.get(r)?.cells.get(c).and_then(|c| first_para(&c.blocks)));
        if self.step(&mut tx, Step::Rows { table, range: 0..n, with: rows, grid: Some(grid) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        tx.stale_tables.push(table);
        // Tracked: the same cell among the rows that stay
        let caret_para = caret_para.filter(|p| self.index.para(&self.doc, *p).is_some()).or_else(|| {
            let (r, c) = target?;
            let deleted: Vec<_> = tx.rec.as_ref().map(|rec| rec.marks().1.iter().map(|m| m.id).collect()).unwrap_or_default();
            let t = self.index.table(&self.doc, table)?;
            let mut kept = t.rows.iter().filter(|row| !row.rev.del.as_ref().is_some_and(|m| deleted.contains(&m.id)));
            kept.nth(r)?.cells.get(c).and_then(|c| first_para(&c.blocks))
        });
        if let Some(p) = caret_para {
            self.selection = Selection::Caret(Position::text(story, p, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn move_rows(&mut self, up: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let n = t.rows.len();
        let (r0, r1) = (tg.rows.0, tg.rows.1.min(n.saturating_sub(1)));
        if (up && r0 == 0) || (!up && r1 + 1 >= n) {
            return Outcome::Unchanged("at the table's edge");
        }
        let (a, b) = if up { (r0 - 1, r1) } else { (r0, r1 + 1) };
        if t.rows[a..=b].iter().any(|r| r.cells.iter().any(|c| c.v_merge != VMerge::None)) {
            return Outcome::Unchanged("vertically merged cells");
        }
        let mut with = t.rows[a..=b].to_vec();
        if up {
            with.rotate_left(1);
        } else {
            with.rotate_right(1);
        }
        // Row-level controls stay over contiguous rows.
        let order = t.rows[..a].iter().chain(&with).chain(&t.rows[b + 1..]);
        let moved: std::collections::HashSet<crate::docx::ControlId> = with.iter().flat_map(|r| r.controls.iter().copied()).collect();
        let split = control_rows(order).iter().any(|(c, &(first, last, n))| moved.contains(c) && last - first + 1 != n);
        if split {
            return self.region_boundary();
        }
        let table = tg.table;
        let mut tx = self.begin("Move Rows");
        if self.step(&mut tx, Step::Rows { table, range: a..b + 1, with, grid: None }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        tx.stale_tables.push(table);
        let (n0, n1) = if up { (r0 - 1, r1 - 1) } else { (r0 + 1, r1 + 1) };
        let last = self.index.table(&self.doc, table).map_or(0, |t| Grid::of(t).cols.saturating_sub(1));
        self.selection = Selection::Cells {
            table,
            anchor: super::selection::CellRef { row: n0, col: 0 },
            focus: super::selection::CellRef { row: n1, col: last },
            shape: super::selection::CellShape::Rows,
        };
        self.commit(tx, None, fonts)
    }

    pub(super) fn auto_fit_column(&mut self, table: TableId, col: usize, fonts: &Fonts) -> Outcome {
        self.auto_fit_columns_to_contents(table, (col, col), fonts)
    }

    /// AutoFit to contents of columns `c0..=c1`: each as wide as what its own cells hold, within the room the columns
    /// before it left; the table measured once for them all.
    pub(super) fn auto_fit_columns_to_contents(&mut self, table: TableId, (c0, c1): (usize, usize), fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        if c0 > c1 {
            return Outcome::Unchanged("no column");
        }
        let g = Grid::of(t);
        // Each column's paragraphs in cells of its own, with their side margins and indents
        let mut paras: Vec<Vec<(crate::layout::ParaId, f64)>> = vec![Vec::new(); c1 - c0 + 1];
        for (r, row) in t.rows.iter().enumerate() {
            for (k, cell) in row.cells.iter().enumerate() {
                let Some(&(c, s, _)) = g.rows[r].get(k) else { continue };
                if (c0..=c1).contains(&c) && s.max(1) == 1 {
                    let m = cell.margins[1] + cell.margins[3];
                    for b in &cell.blocks {
                        if let Block::Para(p) = b {
                            paras[c - c0].push((crate::layout::ParaId(p.id), m + p.style.ind_left + p.style.ind_right));
                        }
                    }
                }
            }
        }
        if paras.iter().all(Vec::is_empty) {
            return Outcome::Unchanged("no cell of its own in the column");
        }
        let (mut grid, nrows) = (t.grid.clone(), t.rows.len());
        let wanted: std::collections::HashSet<crate::layout::ParaId> = paras.iter().flatten().map(|(p, _)| *p).collect();
        let mut lines: std::collections::HashMap<crate::layout::ParaId, f64> = std::collections::HashMap::with_capacity(wanted.len());
        for geo in self.measured_pages(table) {
            for l in geo.lines.iter().filter(|l| !l.repeat) {
                for &p in l.paras.iter().filter(|p| wanted.contains(p)) {
                    let (x0, x1) = l.span_of(p).unwrap_or_default();
                    *lines.entry(p).or_default() += (x1 - x0).max(0.0);
                }
            }
        }
        let text_width = self.index.table_slot(table).map_or(468.0, |s| self.text_width(s.container));
        let mut widths: Vec<(usize, f64)> = Vec::new();
        for (col, paras) in (c0..).zip(&paras).filter(|(_, p)| !p.is_empty()) {
            let margins = paras.iter().map(|(_, m)| *m).fold(0.0, f64::max);
            let content = paras.iter().map(|(p, m)| lines.get(p).copied().unwrap_or(0.0) + m).fold(0.0, f64::max);
            let mut w = content.max(margins + 4.5);
            let old = grid.get(col).copied().unwrap_or(w);
            let total: f64 = grid.iter().sum();
            if w > old {
                w = w.min(old + (text_width + margins + 1.0 - total).max(0.0));
            }
            let w = (w * 10.0).round() / 10.0;
            if (w - old).abs() < 0.05 {
                continue;
            }
            // As the column width edit stores it
            if let Some(x) = grid.get_mut(col) {
                *x = (w.max(1.0) * 20.0).round() / 20.0;
            }
            widths.push((col, w));
        }
        let mut out = Outcome::Unchanged("already fits");
        for (col, w) in widths {
            let target = Target { table, rows: (0, nrows.saturating_sub(1)), cols: (col, col) };
            if let o @ Outcome::Changed(_) = self.table_format_on(target, &super::table_format::TableFormat::ColumnWidth(w), fonts) {
                out = o;
            }
        }
        out
    }

    /// The pages to measure a table's layout on: one in a header or footer shows on every page, so its first.
    pub(super) fn measured_pages(&mut self, table: TableId) -> Vec<Rc<super::geometry::PageGeometry>> {
        let repeats = self
            .index
            .table_slot(table)
            .is_some_and(|s| matches!(self.index.host(self.index.story(s.container)), super::position::StoryId::HeaderFooter(_)));
        let pages = self.page_geometries().into_iter().map(|(_, g)| g);
        if repeats { pages.filter(|g| g.rows.iter().any(|r| r.table == table)).take(1).collect() } else { pages.collect() }
    }

    pub(super) fn fresh_items(&mut self, rows: &mut [Row], inside: &[crate::docx::ControlId]) {
        let given = |c: &crate::docx::Control| sdt_id_at(&c.open).and_then(|r| c.open[r].parse::<i64>().ok());
        let mut next = self.doc.controls.values().chain(self.pending_controls.iter().map(|(_, c)| c)).filter_map(given).max().unwrap_or(0);
        for row in rows {
            for id in row.controls.iter_mut() {
                if inside.contains(id) {
                    continue;
                }
                let Some(c) = self.doc.controls.get(id).filter(|c| c.props.kind == crate::docx::ControlKind::RepeatingItem).cloned() else {
                    continue;
                };
                let n = crate::docx::ControlId::next();
                let mut c = c;
                if let Some(r) = sdt_id_at(&c.open) {
                    next = if next >= 0x7fff_ffff { 1 } else { next + 1 };
                    c.open.replace_range(r, &next.to_string());
                }
                self.pending_controls.push((n, c));
                *id = n;
            }
        }
    }

    pub(super) fn insert_rows(&mut self, below: bool, n: usize, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let at = if below { tg.rows.1 } else { tg.rows.0 };
        let template = &t.rows[at];
        let g = Grid::of(t);
        let mut row = template.clone();
        row.src = None;
        row.rev = Default::default();
        for (k, cell) in row.cells.iter_mut().enumerate() {
            let col = g.rows[at][k].0;
            let inside = if below {
                cell.v_merge != VMerge::None && g.cell_at(at + 1, col).is_some_and(|c| g.continues(at + 1, c))
            } else {
                cell.v_merge == VMerge::Continue
            };
            *cell = empty_cell(cell);
            if inside {
                cell.v_merge = VMerge::Continue;
            }
        }
        let mut rows = t.rows.clone();
        let pos = if below { at + 1 } else { at };
        let n = n.max(tg.rows.1 - tg.rows.0 + 1);
        for _ in 0..n {
            let mut r = row.clone();
            r.cells.iter_mut().for_each(|c| *c = Cell { blocks: empty_cell(c).blocks, ..c.clone() });
            rows.insert(pos, r);
        }
        let grid = t.grid.clone();
        let inside: Vec<crate::docx::ControlId> = match (pos.checked_sub(1).and_then(|k| rows.get(k)), rows.get(pos + n)) {
            (Some(a), Some(b)) => a.controls.iter().copied().filter(|c| b.controls.contains(c)).collect(),
            _ => Vec::new(),
        };
        self.fresh_items(&mut rows[pos..pos + n], &inside);
        self.rebuild("Insert Rows", tg.table, rows, grid, (pos, tg.cols.0), fonts)
    }

    pub(super) fn insert_columns(&mut self, right: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let mut grid = full_grid(t, g.cols);
        let mut widths: Vec<f64> = g.rows[tg.rows.0]
            .iter()
            .filter(|&&(c, s, _)| c + s > tg.cols.0 && c <= tg.cols.1)
            .map(|&(c, s, _)| grid[c..c + s].iter().sum::<f64>())
            .collect();
        if widths.is_empty() {
            widths.push(72.0);
        }
        let (count, added) = (widths.len(), widths.iter().sum::<f64>());
        let at = if right { tg.cols.1 + 1 } else { tg.cols.0 }.min(grid.len());
        grid.splice(at..at, widths.iter().copied());
        let mut rows = t.rows.clone();
        for (r, row) in rows.iter_mut().enumerate() {
            row.src = None;
            if row.cells.is_empty() {
                continue;
            }
            if row.grid_before > 0 && at <= row.grid_before {
                row.grid_before += count;
                continue;
            }
            let cells = &g.rows[r];
            // Inside a merged cell: it takes the new columns.
            if let Some(k) = cells.iter().position(|&(c, s, _)| c < at && at < c + s) {
                let cell = &mut row.cells[k];
                cell.span = cell.span.max(1) + count;
                if let Some(Width::Dxa(w)) = &mut cell.width {
                    *w += added;
                }
                continue;
            }
            if cells.last().is_some_and(|&(c, s, _)| at > c + s) {
                row.grid_after += count;
                continue;
            }
            let k = cells.iter().position(|&(c, _, _)| c >= at).unwrap_or(cells.len());
            let like = &row.cells[k.saturating_sub(1)];
            let new: Vec<Cell> = widths
                .iter()
                .map(|&w| {
                    let mut c = empty_cell(like);
                    c.span = 1;
                    c.width = Some(Width::Dxa(w));
                    c
                })
                .collect();
            row.cells.splice(k..k, new);
        }
        self.rebuild(super::revisions::record::INSERT_COLUMNS, tg.table, rows, grid, (tg.rows.0, at), fonts)
    }

    pub(super) fn delete_rows(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        if tg.rows.0 == 0 && tg.rows.1 + 1 >= t.rows.len() {
            return self.delete_table(fonts);
        }
        if cuts_region(t, tg.rows.0, tg.rows.1, false) {
            return self.region_boundary();
        }
        let g = Grid::of(t);
        let mut rows = t.rows.clone();
        let next = tg.rows.1 + 1;
        if next < rows.len() {
            for k in 0..rows[next].cells.len() {
                if g.continues(next, k) {
                    let (sr, sc) = g.merge_start(next, k);
                    let _ = sc;
                    if sr >= tg.rows.0 {
                        rows[next].cells[k].v_merge = VMerge::Restart;
                    }
                }
            }
        }
        rows.drain(tg.rows.0..=tg.rows.1);
        rows.iter_mut().for_each(|r| r.src = None);
        let grid = t.grid.clone();
        let at = tg.rows.0.min(rows.len().saturating_sub(1));
        let first = rows.get(at).map_or(0, |r| r.grid_before);
        self.rebuild("Delete Rows", tg.table, rows, grid, (at, first), fonts)
    }

    pub(super) fn delete_columns(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        if tg.cols.0 == 0 && tg.cols.1 + 1 >= g.cols {
            return self.delete_table(fonts);
        }
        let (c0, c1) = tg.cols;
        let mut grid = full_grid(t, g.cols);
        let mut rows: Vec<Row> = Vec::new();
        for (r, row) in t.rows.iter().enumerate() {
            let mut row = row.clone();
            row.src = None;
            let cut = |a: usize, b: usize| b.min(c1 + 1).saturating_sub(a.max(c0));
            row.grid_before -= cut(0, row.grid_before);
            row.grid_after -= cut(g.cols.saturating_sub(row.grid_after), g.cols);
            let mut cells = Vec::new();
            for (k, cell) in row.cells.iter().enumerate() {
                let (c, s, _) = g.rows[r][k];
                let lost = cut(c, c + s);
                if lost >= s {
                    continue;
                }
                let mut cell = cell.clone();
                cell.span = s - lost;
                if lost > 0 {
                    let kept: f64 = (c..c + s).filter(|k| !(c0..=c1).contains(k)).map(|k| grid[k]).sum();
                    let was: f64 = grid[c..c + s].iter().sum();
                    cell.width = match cell.width {
                        Some(Width::Dxa(_)) => Some(Width::Dxa(kept)),
                        Some(Width::Pct(p)) if was > 0.0 => Some(Width::Pct(p * kept / was)),
                        w => w,
                    };
                }
                cells.push(cell);
            }
            if cells.is_empty() {
                continue;
            }
            row.cells = cells;
            rows.push(row);
        }
        if rows.is_empty() {
            return self.delete_table(fonts);
        }
        grid.drain(c0..=c1);
        self.rebuild(super::revisions::record::DELETE_COLUMNS, tg.table, rows, grid, (tg.rows.0, c0.saturating_sub(1)), fonts)
    }

    pub(super) fn delete_table(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(slot) = self.index.table_slot(tg.table) else { return Outcome::Unchanged("no table") };
        let story = self.index.story(slot.container);
        let mut tx = self.begin("Delete Table");
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("delete failed");
        }
        let next = self
            .index
            .blocks(&self.doc, slot.container)
            .and_then(|b| b.get(slot.block).or(b.last()))
            .and_then(|b| first_para(std::slice::from_ref(b)));
        if let Some(p) = next {
            self.selection = Selection::Caret(Position::text(story, p, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn merge_cells(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        if tg.rows.0 == tg.rows.1 && tg.cols.0 == tg.cols.1 {
            return Outcome::Unchanged("one cell");
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        if cuts_region(t, tg.rows.0, tg.rows.1, true) {
            return self.region_boundary();
        }
        let g = Grid::of(t);
        let (c0, c1) = tg.cols;
        // Each row's cells in the columns: rows with none there at the selection's ends stay out of it.
        let mut picked: Vec<(usize, std::ops::Range<usize>)> = Vec::with_capacity(tg.rows.1 - tg.rows.0 + 1);
        for r in tg.rows.0..=tg.rows.1 {
            let (mut ks, mut cover) = (0..0, 0);
            for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                let e = c + s - 1;
                if e < c0 || c > c1 {
                    continue;
                }
                if c < c0 || e > c1 {
                    return Outcome::Unchanged("the selection cuts a merged cell");
                }
                ks = if ks.is_empty() { k..k + 1 } else { ks.start..k + 1 };
                cover += s;
            }
            if cover > 0 && cover != c1 - c0 + 1 {
                return Outcome::Unchanged("rows without cells there");
            }
            if cover > 0 || !picked.is_empty() {
                picked.push((r, ks));
            }
        }
        while picked.last().is_some_and(|(_, ks)| ks.is_empty()) {
            picked.pop();
        }
        if picked.iter().any(|(_, ks)| ks.is_empty()) {
            return Outcome::Unchanged("rows without cells there");
        }
        let (Some(&(top, ref first_ks)), Some(&(bottom, _))) = (picked.first(), picked.last()) else {
            return Outcome::Unchanged("no cells there");
        };
        if top == bottom && first_ks.len() < 2 {
            return Outcome::Unchanged("one cell");
        }
        // A vertical merge the selection cuts
        let below = |k: usize| {
            let c = g.rows[bottom][k].0;
            g.cell_at(bottom + 1, c).is_some_and(|kk| g.rows[bottom + 1][kk].0 == c && g.continues(bottom + 1, kk))
        };
        if first_ks.clone().any(|k| g.continues(top, k)) || picked.last().is_some_and(|(_, ks)| ks.clone().any(below)) {
            return Outcome::Unchanged("the selection cuts a merged cell");
        }
        // Each cell-level content control keeps its cell's boundary.
        let lead = first_ks.start;
        let controls = &t.rows[top].cells[lead].controls;
        if picked.iter().any(|(r, ks)| t.rows[*r].cells[ks.clone()].iter().any(|c| c.controls != *controls)) {
            return self.region_boundary();
        }
        let width: f64 = (c0..=c1).filter_map(|c| t.grid.get(c)).sum();
        let mut content: Vec<Block> = Vec::new();
        let mut rows = t.rows.clone();
        for (r, ks) in &picked {
            let (r, ks) = (*r, ks.clone());
            let row = &mut rows[r];
            row.src = None;
            for k in ks.clone() {
                let cell = &t.rows[r].cells[k];
                let empty = cell.blocks.len() == 1 && matches!(&cell.blocks[0], Block::Para(p) if para_len(p) == Unit(0));
                if !(empty || g.continues(r, k)) {
                    content.extend(cell.blocks.iter().cloned());
                }
            }
            let mut merged = row.cells[ks.start].clone();
            merged.span = c1 - c0 + 1;
            merged.width = Some(Width::Dxa(width));
            merged.v_merge = if top == bottom {
                VMerge::None
            } else if r == top {
                VMerge::Restart
            } else {
                VMerge::Continue
            };
            merged.blocks = empty_cell(&merged).blocks;
            row.cells.splice(ks, std::iter::once(merged));
        }
        if content.is_empty() {
            content = empty_cell(&t.rows[top].cells[lead]).blocks;
        }
        if matches!(content.last(), Some(Block::Table(_))) {
            content.extend(empty_cell(&t.rows[top].cells[lead]).blocks);
        }
        rows[top].cells[lead].blocks = content;
        let grid = t.grid.clone();
        self.rebuild(super::revisions::record::MERGE_CELLS, tg.table, rows, grid, (top, c0), fonts)
    }

    /// Splits every selected cell into `cols` columns and `nrows` rows. A vertically merged cell splits into rows
    /// by unmerging, so `nrows` must divide its rows (as in Word).
    pub(super) fn split_cells_into(&mut self, cols: usize, nrows: usize, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let (cols, nrows) = (cols.max(1), nrows.max(1));
        if cols < 2 && nrows < 2 {
            return Outcome::Unchanged("nothing to split");
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        // The merged cells the selection covers, by their start
        let mut targets: Vec<(usize, usize)> = Vec::new();
        for r in tg.rows.0..=tg.rows.1 {
            for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                if c + s > tg.cols.0 && c <= tg.cols.1 {
                    targets.push(g.merge_start(r, k));
                }
            }
        }
        targets.sort_unstable();
        targets.dedup();
        if targets.is_empty() {
            return Outcome::Unchanged("no cell");
        }
        let mut rows = t.rows.clone();
        rows.iter_mut().for_each(|r| r.src = None);
        let mut grid = full_grid(t, g.cols);
        if nrows > 1 {
            let chains: Vec<Vec<(usize, usize)>> = targets.iter().map(|&(r, k)| merge_chain(&g, r, k)).collect();
            if chains.iter().all(|ch| ch.len() == 1) {
                // New rows under each row with a target, the row's other cells merged down over them
                let mut target_rows: Vec<usize> = targets.iter().map(|t| t.0).collect();
                target_rows.dedup();
                let grown = rows.len() + target_rows.len() * (nrows - 1);
                if grown > MAX_ROWS.max(rows.len()) {
                    return Outcome::Unchanged("too many rows");
                }
                let cx = self.cx();
                let mut out: Vec<Row> = Vec::with_capacity(grown);
                let mut at = vec![0; rows.len()];
                for (r, mut first) in rows.into_iter().enumerate() {
                    at[r] = out.len();
                    if target_rows.binary_search(&r).is_err() {
                        out.push(first);
                        continue;
                    }
                    let split = |k: usize| targets.binary_search(&(r, k)).is_ok();
                    super::table_format::set_height(&mut first, None, &cx);
                    let mut template = first.clone();
                    template.rev = Default::default();
                    for (k, c) in first.cells.iter_mut().enumerate() {
                        if !split(k) && c.v_merge == VMerge::None {
                            c.v_merge = VMerge::Restart;
                        }
                    }
                    out.push(first);
                    for _ in 1..nrows {
                        let mut row = template.clone();
                        for (k, cell) in row.cells.iter_mut().enumerate() {
                            *cell = empty_cell(cell);
                            if !split(k) {
                                cell.v_merge = VMerge::Continue;
                            }
                        }
                        out.push(row);
                    }
                }
                rows = out;
                targets = targets.iter().flat_map(|&(r, k)| (at[r]..at[r] + nrows).map(move |rr| (rr, k))).collect();
            } else if chains.iter().all(|ch| ch.len() > 1 && ch.len() % nrows == 0) {
                // Merged cells unmerged into groups of rows
                let mut starts = Vec::with_capacity(targets.len() * nrows);
                for ch in &chains {
                    let step = ch.len() / nrows;
                    for (i, &(r, k)) in ch.iter().enumerate() {
                        if i % step == 0 {
                            rows[r].cells[k].v_merge = if step == 1 { VMerge::None } else { VMerge::Restart };
                            starts.push((r, k));
                        }
                    }
                }
                starts.sort_unstable();
                targets = starts;
            } else {
                return Outcome::Unchanged("the rows must divide the merged cells' rows");
            }
        }
        if cols > 1 {
            let g = Grid::of_rows(&rows, grid.len());
            // Right to left: a split moves only the columns after it
            let mut work: Vec<(usize, Vec<(usize, usize)>)> = targets.iter().map(|&(r, k)| (g.rows[r][k].0, merge_chain(&g, r, k))).collect();
            work.sort_unstable_by(|a, b| b.cmp(a));
            let limit = MAX_COLUMNS.max(grid.len());
            for (c, ch) in work {
                let (r, k) = ch[0];
                let s = rows[r].cells[k].span.max(1);
                let added = cols.saturating_sub(s);
                if added > 0 {
                    // The grid grows to `limit` columns at most, so this walk over the rows runs that often at most.
                    if grid.len() + added > limit {
                        return Outcome::Unchanged("too many columns");
                    }
                    let last = c + s - 1;
                    let w = grid[last] / (added + 1) as f64;
                    grid[last] = w;
                    grid.splice(last + 1..last + 1, std::iter::repeat_n(w, added));
                    for (ri, row) in rows.iter_mut().enumerate() {
                        if ch.binary_search_by_key(&ri, |x| x.0).is_ok() {
                            continue;
                        }
                        if last < row.grid_before {
                            row.grid_before += added;
                            continue;
                        }
                        let mut col = row.grid_before;
                        let over = row.cells.iter_mut().find(|cell| {
                            col += cell.span.max(1);
                            last < col
                        });
                        match over {
                            Some(cell) => cell.span = cell.span.max(1) + added,
                            None => row.grid_after += added,
                        }
                    }
                }
                let total = s + added;
                let span = |i: usize| total / cols + usize::from(i < total % cols);
                for &(rr, kk) in &ch {
                    let first = &mut rows[rr].cells[kk];
                    first.span = span(0);
                    first.width = None;
                    let v = first.v_merge;
                    let first = &rows[rr].cells[kk];
                    let pieces: Vec<Cell> = (1..cols)
                        .map(|i| {
                            let mut cell = empty_cell(first);
                            (cell.span, cell.v_merge) = (span(i), v);
                            cell
                        })
                        .collect();
                    rows[rr].cells.splice(kk + 1..kk + 1, pieces);
                }
            }
        }
        let (r, k) = targets[0];
        let c = Grid::of_rows(&rows, grid.len()).col_of(r, k).unwrap_or(0);
        self.rebuild(super::revisions::record::SPLIT_CELLS, tg.table, rows, grid, (r, c), fonts)
    }

    /// A new table `width` wide of `content`, each row's cells' blocks (a row with fewer than `ncols` cells filled
    /// with empty ones formatted like `like`), in the document's default table style, else Table Grid.
    pub(super) fn new_table(&self, content: Vec<Vec<Vec<Block>>>, ncols: usize, width: f64, like: &Paragraph) -> Table {
        let border =
            Some(crate::docx::props::Border { style: "single".into(), width: 0.5, space: 0.0, shadow: false, color: None, color_theme: None });
        // The document's default table style, else Table Grid.
        let has = |id: &str| self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key(id));
        let style = self.doc.settings.default_table_style.clone().filter(|s| has(s)).or_else(|| has("TableGrid").then(|| "TableGrid".to_string()));
        let grid_style = style.is_some();
        let cell = |w: f64, blocks: Vec<Block>| Cell {
            width: Some(crate::docx::props::Width::Dxa(w)),
            span: 1,
            v_merge: VMerge::None,
            top: None,
            left: None,
            bottom: None,
            right: None,
            shading: None,
            hatch: None,
            margins: [0.0, 5.4, 0.0, 5.4],
            v_align: "top".into(),
            text_direction: "lrTb".into(),
            no_wrap: false,
            fit_text: false,
            hide_mark: false,
            diagonals: None,
            direct: Default::default(),
            blocks,
            controls: Vec::new(),
            rev: Default::default(),
            ids: None,
        };
        let w = ((width / ncols as f64 * 20.0).floor() / 20.0).max(1.0);
        // Its style, or without one its grid lines and cell margins, as re-resolving and reading find them.
        let direct = {
            let w = self.cx().w;
            let own = match &style {
                Some(id) => format!("<{w}tblStyle {w}val=\"{}\"/>", crate::docx::write::props::esc(id)),
                None => {
                    let line = |side: &str| format!("<{w}{side} {w}val=\"single\" {w}sz=\"4\" {w}space=\"0\" {w}color=\"auto\"/>");
                    let sides: String = ["top", "left", "bottom", "right", "insideH", "insideV"].into_iter().map(line).collect();
                    let mar = |side: &str| format!("<{w}{side} {w}w=\"108\" {w}type=\"dxa\"/>");
                    format!("<{w}tblBorders>{sides}</{w}tblBorders><{w}tblCellMar>{}{}</{w}tblCellMar>", mar("left"), mar("right"))
                }
            };
            crate::docx::Raw(Some(std::rc::Rc::from(format!("<{w}tblPr>{own}</{w}tblPr>").as_str())))
        };
        Table {
            style_id: style,
            width: crate::docx::props::Width::Auto,
            jc: crate::docx::props::Jc::Left,
            own_jc: None,
            ind: 0.0,
            ind_pct: None,
            ind_set: true,
            borders: if grid_style {
                Default::default()
            } else {
                [border.clone(), border.clone(), border.clone(), border.clone(), border.clone(), border]
            },
            fixed: false,
            cell_spacing: 0.0,
            shd: None,
            look: None,
            bands: (0, 0),
            alt: None,
            floating: None,
            bidi_visual: false,
            grid: vec![w; ncols],
            rows: content
                .into_iter()
                .map(|cells| Row {
                    height: None,
                    cant_split: false,
                    header: false,
                    grid_before: 0,
                    grid_after: 0,
                    w_before: None,
                    w_after: None,
                    ind: None,
                    jc: None,
                    div: None,
                    borders: None,
                    shading: None,
                    hatch: None,
                    table_shading: None,
                    hidden: false,
                    vanished: false,
                    cell_spacing: None,
                    run: None,
                    nudge: None,
                    cells: {
                        let n = cells.len();
                        let pad = (n..ncols).map(|_| vec![Block::Para(blank_para(like))]);
                        cells.into_iter().chain(pad).map(|b| cell(w, b)).collect()
                    },
                    src: None,
                    direct: Default::default(),
                    exceptions: Default::default(),
                    controls: Vec::new(),
                    rev: Default::default(),
                })
                .collect(),
            src: None,
            id: crate::docx::next_para_id(),
            direct,
            controls: Vec::new(),
            change: None,
            grid_change: None,
        }
    }

    pub(super) fn insert_table(&mut self, nrows: usize, ncols: usize, fonts: &Fonts) -> Outcome {
        let Some(caret) = self.caret() else { return Outcome::Unchanged("no caret") };
        let Some((para, at)) = caret.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        let (nrows, ncols) = (nrows.clamp(1, MAX_ROWS), ncols.clamp(1, MAX_COLUMNS));
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return Outcome::Unchanged("no paragraph") };
        let width = self.text_width(slot.container);
        let content = (0..nrows).map(|_| (0..ncols).map(|_| vec![Block::Para(blank_para(&p))]).collect()).collect();
        let mut table = self.new_table(content, ncols, width, &p);
        table.controls = p.controls.clone();
        let first = first_para(&table.rows[0].cells[0].blocks);
        let tid = TableId(table.id);
        let mut tx = self.begin("Insert Table");
        let len = para_len(&p);
        let blocks = self.index.blocks(&self.doc, slot.container);
        let followed = blocks.is_some_and(|b| slot.block + 1 < b.len());
        let table_at = |k: Option<usize>| k.and_then(|k| blocks?.get(k)).is_some_and(|b| matches!(b, Block::Table(_)));
        // Tables side by side join into one: a paragraph keeps them apart.
        let before = at == Unit(0) && table_at(slot.block.checked_sub(1));
        let after = at != Unit(0) && at == len && followed && table_at(Some(slot.block + 1));
        let place = if at == Unit(0) {
            slot.block
        } else if at == len && followed {
            slot.block + 1
        } else {
            let Some(second) = self.split_template(para) else { return Outcome::Unchanged("no paragraph") };
            if self.step(&mut tx, Step::Split { para, at, second, kept: Vec::new() }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("split failed");
            }
            slot.block + 1
        };
        let mut with = Vec::with_capacity(3);
        if before {
            with.push(Block::Para(blank_para(&p)));
        }
        with.push(Block::Table(table));
        if after {
            with.push(Block::Para(blank_para(&p)));
        }
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: place..place, with }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("insert failed");
        }
        tx.stale_tables.push(tid);
        if let Some(fp) = first {
            self.selection = Selection::Caret(Position::text(caret.story, fp, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    /// The width text has at `container`: a cell's content, a text box's inside, a column of the section the
    /// container is in. Never less than a point.
    pub(super) fn text_width(&self, container: Container) -> f64 {
        use super::position::StoryId;
        use super::sub_templates::column_width;
        let page = |s: &crate::docx::Section| s.page_w - s.left - s.right - s.gutter;
        let section = |k: usize| self.doc.sections.get(k).or(self.doc.sections.first());
        let w = match container {
            Container::Cell { table, row, cell } => self.index.table(&self.doc, table).and_then(|t| {
                let g = Grid::of(t);
                let (c, s, _) = *g.rows.get(row)?.get(cell)?;
                let m = t.rows[row].cells[cell].margins;
                Some((c..c + s).filter_map(|k| t.grid.get(k)).sum::<f64>() - m[1] - m[3])
            }),
            Container::Section(k) => section(k).map(column_width),
            Container::Story(StoryId::TextBox(f)) => self.index.float_anchor(f).and_then(|p| self.index.para(&self.doc, p)).and_then(|p| {
                p.inlines.iter().find_map(|i| match i {
                    Inline::Float(fl, _) if fl.id == f.0 => fl.text_box.as_ref().map(|tb| fl.w - tb.insets[0] - tb.insets[2]),
                    Inline::Picture(pic, _) => pic.text.as_ref().filter(|t| t.id == f.0).map(|t| pic.w - t.text_box.insets[0] - t.text_box.insets[2]),
                    _ => None,
                })
            }),
            Container::Story(StoryId::HeaderFooter(_)) => section(self.story_section(container)).map(page),
            Container::Story(StoryId::Footnote(_) | StoryId::Endnote(_)) => section(self.story_section(container)).map(column_width),
            Container::Story(StoryId::BuildingBlock(part)) => {
                section(self.surface.as_ref().filter(|s| s.part == part).map_or(0, |s| s.section)).map(column_width)
            }
            Container::Story(StoryId::Body) => section(0).map(column_width),
        };
        w.unwrap_or(468.0).max(1.0)
    }

    /// The section a story's text is laid out in: a header's or footer's first, a note's reference's.
    fn story_section(&self, container: Container) -> usize {
        use super::position::StoryId;
        match container {
            Container::Story(StoryId::HeaderFooter(k)) => {
                let name = self.index.part_name(k);
                self.doc.sections.iter().position(|s| s.headers.iter().chain(s.footers.iter()).flatten().any(|n| Some(n.as_str()) == name))
            }
            Container::Story(StoryId::Footnote(id)) => self.note_reference(crate::docx::NoteKind::Foot, id).and_then(|(p, _)| self.section_at(&p)),
            Container::Story(StoryId::Endnote(id)) => self.note_reference(crate::docx::NoteKind::End, id).and_then(|(p, _)| self.section_at(&p)),
            _ => None,
        }
        .unwrap_or(0)
    }

    pub(super) fn select_cells(&mut self, shape: super::selection::CellShape) -> Outcome {
        use super::selection::CellShape;
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let ((r0, c0), (r1, c1)) = match shape {
            CellShape::Rows => ((tg.rows.0, 0), (tg.rows.1, g.cols.saturating_sub(1))),
            CellShape::Columns => ((0, tg.cols.0), (t.rows.len().saturating_sub(1), tg.cols.1)),
            CellShape::Table => ((0, 0), (t.rows.len().saturating_sub(1), g.cols.saturating_sub(1))),
            CellShape::Rect => ((tg.rows.0, tg.cols.0), (tg.rows.1, tg.cols.1)),
        };
        self.set_selection(Selection::Cells { table: tg.table, anchor: CellRef { row: r0, col: c0 }, focus: CellRef { row: r1, col: c1 }, shape });
        Outcome::Moved
    }

    pub(super) fn split_table(&mut self, fonts: &Fonts) -> Outcome {
        let Some(focus) = self.caret() else { return Outcome::Unchanged("no caret") };
        let Some(level) = self.index.chain_of(&focus).last().copied() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, level.table).cloned() else { return Outcome::Unchanged("no table") };
        if level.row > 0 && t.rows[level.row].controls.iter().any(|c| t.rows[level.row - 1].controls.contains(c)) {
            return self.region_boundary();
        }
        let Some(slot) = self.index.table_slot(level.table) else { return Outcome::Unchanged("no table") };
        let story = self.index.story(slot.container);
        let Some((para, _)) = focus.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        let Some(template) = self.split_template(para) else { return Outcome::Unchanged("no paragraph") };
        let mut p = template;
        p.list = None;
        p.inlines = vec![Inline::Text(String::new(), p.mark.clone())];
        let pid = ParaId(p.id);
        let mut tx = self.begin("Split Table");
        if level.row == 0 {
            if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block, with: vec![Block::Para(p)] }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("insert failed");
            }
            tx.stale_paras.push(pid);
            return self.commit(tx, None, fonts);
        }
        let n = t.rows.len();
        let mut lower = t.clone();
        lower.rows = t.rows[level.row..].to_vec();
        lower.src = None;
        lower.id = crate::docx::next_para_id();
        let g = Grid::of(&t);
        for (k, c) in lower.rows[0].cells.iter_mut().enumerate() {
            if g.continues(level.row, k) {
                c.v_merge = VMerge::Restart;
            }
        }
        let lower_id = TableId(lower.id);
        if self.step(&mut tx, Step::Rows { table: level.table, range: level.row..n, with: Vec::new(), grid: None }).is_err()
            || self
                .step(
                    &mut tx,
                    Step::Blocks {
                        container: slot.container,
                        range: slot.block + 1..slot.block + 1,
                        with: vec![Block::Para(p), Block::Table(lower)],
                    },
                )
                .is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("split failed");
        }
        tx.stale_tables.push(level.table);
        tx.stale_tables.push(lower_id);
        tx.stale_paras.push(pid);
        self.selection = Selection::Caret(Position::text(story, pid, Unit(0)));
        self.commit(tx, None, fonts)
    }

    pub(super) fn delete_cells(&mut self, clear: bool, fonts: &Fonts) -> Option<Outcome> {
        let Selection::Cells { table, anchor, focus, .. } = self.selection else { return None };
        let t = self.index.table(&self.doc, table)?;
        let g = Grid::of(t);
        let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
        let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
        let all_cols = c0 == 0 && c1 + 1 >= g.cols;
        let all_rows = r0 == 0 && r1 + 1 >= t.rows.len();
        if !clear {
            return Some(match (all_rows, all_cols) {
                (true, true) => self.delete_table(fonts),
                (_, true) => self.delete_rows(fonts),
                (true, _) => self.delete_columns(fonts),
                _ => Outcome::Unchanged("Delete Cells dialog"),
            });
        }
        let (_, rows, grid, first) = self.cleared_cells(self.selection)?;
        Some(self.rebuild("Clear", table, rows, grid, first.unwrap_or((r0, c0)), fonts))
    }

    /// Block of cells `s`'s table with the block's cells emptied: the table, its rows and its grid, and the row and
    /// column of the first cell emptied.
    pub(super) fn cleared_cells(&self, s: Selection) -> Option<(TableId, Vec<Row>, Vec<f64>, Option<(usize, usize)>)> {
        let Selection::Cells { table, anchor, focus, .. } = s else { return None };
        let t = self.index.table(&self.doc, table)?;
        let g = Grid::of(t);
        let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
        let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
        let mut rows = t.rows.clone();
        let mut first = None;
        for (r, row) in rows.iter_mut().enumerate().filter(|(r, _)| (r0..=r1).contains(r)) {
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let (c, s, _) = g.rows[r][k];
                if c + s > c0 && c <= c1 {
                    first.get_or_insert((r, c));
                    *cell = Cell { blocks: empty_cell(cell).blocks, ..cell.clone() };
                }
            }
        }
        tidy_merges(&mut rows);
        Some((table, rows, t.grid.clone(), first))
    }

    pub(super) fn select_to_edge(&mut self, row_wise: bool, end: bool) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let (r, c) = match self.selection {
            Selection::Cells { anchor, .. } => (anchor.row, anchor.col),
            _ => (tg.rows.0, tg.cols.0),
        };
        let to = if row_wise {
            CellRef { row: r, col: if end { g.cols.saturating_sub(1) } else { 0 } }
        } else {
            CellRef { row: if end { t.rows.len().saturating_sub(1) } else { 0 }, col: c }
        };
        let shape = if row_wise { super::selection::CellShape::Rows } else { super::selection::CellShape::Columns };
        self.set_selection(Selection::Cells { table: tg.table, anchor: CellRef { row: r, col: c }, focus: to, shape });
        Outcome::Moved
    }

    pub(super) fn select_table(&mut self) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        self.set_selection(Selection::Cells {
            table: tg.table,
            anchor: CellRef { row: 0, col: 0 },
            focus: CellRef { row: t.rows.len().saturating_sub(1), col: g.cols.saturating_sub(1) },
            shape: super::selection::CellShape::Table,
        });
        Outcome::Moved
    }
}

pub(super) fn cuts_region(t: &Table, r0: usize, r1: usize, within: bool) -> bool {
    let all = control_rows(&t.rows);
    let picked = control_rows(t.rows.iter().take(r1.saturating_add(1)).skip(r0));
    picked.iter().any(|(c, &(_, _, n))| {
        let all_in = all.get(c).is_some_and(|&(first, last, _)| r0 <= first && last <= r1);
        let covers = r1.checked_sub(r0).is_some_and(|d| n == d + 1);
        !covers && (within || !all_in)
    })
}

/// Each row-level control over `rows`: the first and last row it is on, and how many rows it is on.
pub(super) fn control_rows<'a>(rows: impl IntoIterator<Item = &'a Row>) -> std::collections::HashMap<crate::docx::ControlId, (usize, usize, usize)> {
    let mut out = std::collections::HashMap::new();
    for (k, r) in rows.into_iter().enumerate() {
        for &c in &r.controls {
            let e = out.entry(c).or_insert((k, k, 0));
            (e.1, e.2) = (k, e.2 + 1);
        }
    }
    out
}

pub(super) const NEW_POSITION: TablePos = TablePos {
    v_anchor: TableAnchor::Text,
    y: 0.0,
    x: 0.0,
    x_spec: None,
    h_page: false,
    h_text: false,
    y_spec: None,
    dist: [0.0, 0.0, 9.0, 9.0],
    never_overlap: false,
};

fn twips(v: f64) -> f64 {
    (v * 20.0).round() / 20.0
}

impl Editor {
    // Insert buttons
    pub(super) fn insert_column_at(&mut self, table: TableId, k: usize, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let (cols, rows) = (Grid::of(t).cols, t.rows.len());
        if cols == 0 || rows == 0 {
            return Outcome::Unchanged("an empty table");
        }
        let (col, right) = if k == 0 { (0, false) } else { ((k - 1).min(cols - 1), true) };
        let column = |c: usize| Selection::Cells {
            table,
            anchor: CellRef { row: 0, col: c },
            focus: CellRef { row: rows - 1, col: c },
            shape: super::selection::CellShape::Columns,
        };
        self.selection = column(col);
        let out = self.insert_columns(right, fonts);
        if matches!(out, Outcome::Changed(_)) {
            self.set_selection(column(if right { col + 1 } else { col }));
        }
        out
    }

    pub(super) fn insert_row_at(&mut self, table: TableId, k: usize, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let (cols, rows) = (Grid::of(t).cols, t.rows.len());
        if cols == 0 || k == 0 || k > rows {
            return Outcome::Unchanged("no row there");
        }
        let row = |r: usize| Selection::Cells {
            table,
            anchor: CellRef { row: r, col: 0 },
            focus: CellRef { row: r, col: cols - 1 },
            shape: super::selection::CellShape::Rows,
        };
        self.selection = row(k - 1);
        let out = self.insert_rows(true, 1, fonts);
        if matches!(out, Outcome::Changed(_)) {
            self.set_selection(row(k));
        }
        out
    }

    // Table position
    pub(super) fn table_position(&self) -> Option<TablePosition> {
        use crate::api::value::{
            HorizontalPosition, HorizontalReference as H, ObjectHorizontalAlignment as HA, ObjectVerticalAlignment as VA, Points, VerticalPosition,
            VerticalReference as V,
        };
        let p = self.index.table(&self.doc, self.table_target()?.table)?.floating?;
        let horizontal = HorizontalPosition {
            relative_to: match (p.h_page, p.h_text) {
                (true, _) => H::Page,
                (false, true) => H::Column,
                (false, false) => H::Margin,
            },
            align: p.x_spec.map(|j| match j {
                Jc::Center => HA::Center,
                Jc::Right => HA::Right,
                _ => HA::Left,
            }),
            offset: p.x_spec.is_none().then_some(Points(p.x)),
            percent: None,
        };
        let y_spec = p.y_spec.filter(|_| p.v_anchor != TableAnchor::Text);
        let vertical = VerticalPosition {
            relative_to: match p.v_anchor {
                TableAnchor::Text => V::Paragraph,
                TableAnchor::Margin => V::Margin,
                TableAnchor::Page => V::Page,
            },
            align: y_spec.map(|j| match j {
                Jc::Center => VA::Center,
                Jc::Right => VA::Bottom,
                _ => VA::Top,
            }),
            offset: y_spec.is_none().then_some(Points(p.y)),
            percent: None,
        };
        let [top, bottom, left, right] = p.dist.map(Points);
        Some(TablePosition {
            horizontal: Some(horizontal),
            vertical: Some(vertical),
            wrap_distance: Some(crate::api::state::TextDistance { top, bottom, left, right }),
        })
    }

    pub(super) fn set_table_position(&mut self, p: Option<TablePosition>, fonts: &Fonts) -> Outcome {
        use crate::api::value::{HorizontalReference as H, ObjectHorizontalAlignment as HA, ObjectVerticalAlignment as VA, VerticalReference as V};
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        let Some(p) = p else { return self.set_floating(tg.table, None, fonts) };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let mut n = t.floating.unwrap_or(NEW_POSITION);
        let mut stay = [false; 2];
        if let Some(h) = p.horizontal {
            (n.h_page, n.h_text) = match h.relative_to {
                H::Column => (false, true),
                H::Margin => (false, false),
                H::Page => (true, false),
                H::Character | H::LeftMargin | H::RightMargin | H::InsideMargin | H::OutsideMargin => {
                    return Outcome::invalid("a table is placed across from the column, the margin or the page");
                }
            };
            match (h.align, h.offset) {
                (Some(a), _) => {
                    n.x_spec = Some(match a {
                        HA::Left | HA::Inside => Jc::Left,
                        HA::Center => Jc::Center,
                        HA::Right | HA::Outside => Jc::Right,
                    })
                }
                (None, Some(o)) => (n.x_spec, n.x) = (None, twips(o.0)),
                (None, None) => stay[0] = true,
            }
        }
        if let Some(v) = p.vertical {
            n.v_anchor = match v.relative_to {
                V::Paragraph => TableAnchor::Text,
                V::Margin => TableAnchor::Margin,
                V::Page => TableAnchor::Page,
                V::Line | V::TopMargin | V::BottomMargin | V::InsideMargin | V::OutsideMargin => {
                    return Outcome::invalid("a table is placed down from the paragraph, the margin or the page");
                }
            };
            match (v.align, v.offset) {
                (Some(a), _) => {
                    let (jc, code) = match a {
                        VA::Top => (Jc::Left, -4.0),
                        VA::Center => (Jc::Center, -8.0),
                        VA::Bottom => (Jc::Right, -12.0),
                        VA::Inside => (Jc::Left, -16.0),
                        VA::Outside => (Jc::Right, -20.0),
                    };
                    n.y_spec = Some(jc);
                    if n.v_anchor == TableAnchor::Text {
                        n.y = code / 20.0;
                    }
                }
                (None, Some(o)) => (n.y_spec, n.y) = (None, twips(o.0)),
                (None, None) => stay[1] = true,
            }
        }
        if let Some(d) = p.wrap_distance {
            n.dist = [d.top, d.bottom, d.left, d.right].map(|v| twips(v.0.clamp(0.0, 1584.0)));
        }
        let table = tg.table;
        match self.table_box(table, fonts) {
            Some((page, r)) if stay.contains(&true) => {
                let keep = [stay[0].then_some(r[0]), stay[1].then_some(r[1])];
                self.position_table(table, n, keep, page, "Table Position", fonts)
            }
            _ => self.set_floating(table, Some(n), fonts),
        }
    }

    pub(super) fn position_table(
        &mut self,
        table: TableId,
        mut pos: TablePos,
        keep: [Option<f64>; 2],
        page: usize,
        label: &'static str,
        fonts: &Fonts,
    ) -> Outcome {
        if keep[0].is_some() {
            pos.x_spec = None;
        }
        if keep[1].is_some() {
            pos.y_spec = None;
        }
        self.begin_batch(Some(label.into()));
        self.set_floating(table, Some(pos), fonts);
        self.ensure_page(page, fonts);
        if let Some(now) = self.pages.page(page, &self.index).and_then(|g| table_on(&g, table)) {
            if let Some(x) = keep[0] {
                pos.x = twips(pos.x + x - now[0]);
            }
            if let Some(y) = keep[1] {
                pos.y = twips(pos.y + y - now[1]);
            }
            self.set_floating(table, Some(pos), fonts);
        }
        self.end_batch(true, fonts)
    }

    pub(super) fn set_floating(&mut self, table: TableId, floating: Option<TablePos>, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let mut props = super::step::TableProps::of(t);
        if props.floating == floating {
            return Outcome::Unchanged("already there");
        }
        props.floating = floating;
        let mut tx = self.begin("Table Position");
        if self.step(&mut tx, Step::TableProps { table, props }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale table");
        }
        self.commit(tx, None, fonts)
    }

    pub(super) fn table_box(&mut self, table: TableId, fonts: &Fonts) -> Option<(usize, [f64; 4])> {
        let n = self.pages.len();
        let near = self.near;
        (0..n).map(|d| if d % 2 == 0 { near + d / 2 } else { near.wrapping_sub(d / 2 + 1) }).filter(|&p| p < n).find_map(|p| {
            self.ensure_page(p, fonts);
            let geo = self.pages.page(p, &self.index)?;
            geo.rows.iter().any(|r| r.table == table && r.row == 0 && !r.repeat).then(|| table_on(&geo, table)).flatten().map(|r| (p, r))
        })
    }

    pub(super) fn region_boundary(&mut self) -> Outcome {
        self.refused = Some("region boundary");
        Outcome::Unchanged("region boundary")
    }
}

pub(super) fn table_on(geo: &super::geometry::PageGeometry, table: TableId) -> Option<[f64; 4]> {
    geo.rows
        .iter()
        .filter(|r| r.table == table && !r.repeat)
        .map(|r| r.rect)
        .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
}

fn sdt_id_at(open: &str) -> Option<std::ops::Range<usize>> {
    let k = open.find(":id ").or_else(|| open.find("<w:id "))?;
    let v = k + open[k..].find("val=\"")? + 5;
    Some(v..v + open[v..].find('"')?)
}
