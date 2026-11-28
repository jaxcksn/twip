//! Table cell commands

use crate::docx::props::Width;
use crate::docx::{Cell, Row, VMerge};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::table::Grid;
use super::tables::empty_cell;

/// Where Insert Cells moves the selected cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertShift {
    /// New cells go left of the selection in its rows, the rows growing right.
    Right,
    /// New cells go above the selection in its columns, the columns' cells moving down over rows added at the end.
    Down,
}

/// `rows`' grid again from each row's cell widths (`widths`, twips, and its offset before its first cell): every edge
/// any row has becomes a grid edge, so each cell keeps its width and place. Edges a row has at one place (a cell of no
/// width) stay apart, the `j`th at a place in each row on one grid edge. `None` when a row's edges go back.
fn regrid(rows: &mut [Row], widths: &[(i64, Vec<i64>)]) -> Option<Vec<f64>> {
    // Each edge: its place and how many of the row's edges before it are there too.
    fn edges((before, w): &(i64, Vec<i64>)) -> impl Iterator<Item = (i64, u32)> + '_ {
        let (mut x, mut j) = (*before, 0u32);
        std::iter::once((x, 0)).chain(w.iter().map(move |v| {
            let was = x;
            x += v;
            j = if x == was { j + 1 } else { 0 };
            (x, j)
        }))
    }
    if widths.iter().any(|(b, w)| *b < 0 || w.iter().any(|v| *v < 0)) {
        return None;
    }
    let mut all: Vec<(i64, u32)> = Vec::with_capacity(widths.iter().map(|(_, w)| w.len() + 1).sum());
    for w in widths {
        all.extend(edges(w));
    }
    all.sort_unstable();
    all.dedup();
    let at = |e: (i64, u32)| all.binary_search(&e).unwrap_or(0);
    let last = all.len().saturating_sub(1);
    for (row, w) in rows.iter_mut().zip(widths) {
        let mut e = edges(w);
        let mut from = e.next().map_or(0, at);
        row.grid_before = from;
        for (cell, end) in row.cells.iter_mut().zip(e) {
            let to = at(end);
            cell.span = to - from;
            from = to;
        }
        row.grid_after = last - from;
        row.w_before = None;
        row.w_after = None;
    }
    Some(all.windows(2).map(|p| (p[1].0 - p[0].0) as f64 / 20.0).collect())
}

/// Cell `from` moved to `to`'s place: its content and formatting, over `to`'s columns.
fn moved_cell(from: Cell, to: &Cell) -> Cell {
    Cell { width: to.width, span: to.span, ..from }
}

/// Whether cell `k` of row `r` continues a vertical merge from a cell over the same columns.
fn continues_whole(g: &Grid, r: usize, k: usize) -> bool {
    let Some(&(c, s, VMerge::Continue)) = g.rows.get(r).and_then(|row| row.get(k)) else { return false };
    r > 0
        && g.cell_at(r - 1, c).is_some_and(|a| g.rows[r - 1][a].0 == c && g.rows[r - 1][a].1.max(1) == s.max(1) && g.rows[r - 1][a].2 != VMerge::None)
}

/// The cells of row `r` that Shift cells right moves: from the first one in columns `c0..=c1` on.
fn moving(g: &Grid, r: usize, (c0, c1): (usize, usize)) -> Option<usize> {
    g.rows[r].iter().position(|&(c, s, _)| c + s.max(1) > c0 && c <= c1)
}

impl Editor {
    /// Insert Cells: blank cells the size of the selected ones, the selected cells shifting right in their rows or
    /// down in their columns.
    pub fn insert_cells(&mut self, shift: InsertShift, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::unavailable("not in a table") };
        // No command of its own: protection takes it as inserting the columns or rows it shifts the cells by
        let id = match shift {
            InsertShift::Right => crate::api::command::CommandId::InsertColumns,
            InsertShift::Down => crate::api::command::CommandId::InsertRows,
        };
        if let Some(e) = self.table_edit_refusal(tg.table, id) {
            return Outcome::Refused(e);
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::unavailable("not in a table") };
        if super::tables::cuts_region(t, tg.rows.0, t.rows.len() - 1, false) {
            return self.region_boundary();
        }
        let g = Grid::of(t);
        let (c0, c1) = tg.cols;
        let last = t.rows.len() - 1;
        let (mut r0, mut r1) = (tg.rows.0.min(last), tg.rows.1.min(last));
        let mut rows: Vec<Row> = t.rows.clone();
        let (grid, own_grid) = (super::tables::full_grid(t, g.cols), t.grid.len());
        let (grid, caret) = match shift {
            InsertShift::Right => {
                // The rows widened over every vertical merge among the cells that move
                let merged = |r: usize, below: bool| {
                    let Some(k) = moving(&g, r, (c0, c1)) else { return false };
                    (k..g.rows[r].len()).any(|j| {
                        if below {
                            let c = g.rows[r][j].0;
                            g.cell_at(r + 1, c).is_some_and(|b| continues_whole(&g, r + 1, b))
                        } else {
                            continues_whole(&g, r, j)
                        }
                    })
                };
                while r0 > 0 && merged(r0, false) {
                    r0 -= 1;
                }
                while r1 < last && merged(r1, true) {
                    r1 += 1;
                }
                let tw = |a: usize, b: usize| grid[a.min(grid.len())..b.min(grid.len())].iter().map(|v| (v * 20.0).round() as i64).sum::<i64>();
                let mut widths = Vec::with_capacity(rows.len());
                for (r, row) in rows.iter_mut().enumerate() {
                    let line = &g.rows[r];
                    let mut w: Vec<i64> = line.iter().map(|&(c, s, _)| tw(c, c + s.max(1))).collect();
                    if (r0..=r1).contains(&r)
                        && let Some(k) = moving(&g, r, (c0, c1))
                    {
                        let picked = (k..line.len()).take_while(|&j| line[j].0 <= c1);
                        let new: Vec<_> = picked
                            .clone()
                            .map(|j| {
                                let mut c = empty_cell(&row.cells[j]);
                                c.v_merge = row.cells[j].v_merge;
                                c
                            })
                            .collect();
                        let nw: Vec<i64> = picked.map(|j| w[j]).collect();
                        row.cells.splice(k..k, new);
                        w.splice(k..k, nw);
                        row.detach();
                    }
                    widths.push((tw(0, row.grid_before), w));
                }
                let Some(grid) = regrid(&mut rows, &widths) else { return Outcome::unavailable("the columns can't be kept") };
                if grid.len() > super::tables::MAX_COLUMNS.max(g.cols) {
                    return Outcome::unavailable("too many columns");
                }
                // Every merge whole before stays whole, on the cell it continued; each new cell's merge is whole
                let ng = Grid::of_rows(&rows, grid.len());
                let added = |r: usize| rows[r].cells.len() - t.rows[r].cells.len();
                let at = |r: usize, k: usize| match moving(&g, r, (c0, c1)) {
                    Some(m) if k >= m => k + added(r),
                    _ => k,
                };
                let kept = (1..g.rows.len()).all(|r| {
                    (0..g.rows[r].len()).filter(|&k| continues_whole(&g, r, k)).all(|k| {
                        let (above, now) = (g.cell_at(r - 1, g.rows[r][k].0), at(r, k));
                        continues_whole(&ng, r, now) && ng.cell_at(r - 1, ng.rows[r][now].0) == above.map(|a| at(r - 1, a))
                    })
                });
                let new_whole = (r0..=r1).all(|r| {
                    let m = moving(&g, r, (c0, c1)).unwrap_or(0);
                    (m..m + added(r)).all(|j| rows[r].cells[j].v_merge != VMerge::Continue || continues_whole(&ng, r, j))
                });
                if !kept || !new_whole {
                    return Outcome::unavailable("merged cells in the rows");
                }
                // The caret in the first new cell, at its column in the grid as rebuilt
                let r = tg.rows.0.min(last);
                let col = moving(&g, r, (c0, c1)).and_then(|k| ng.rows[r].get(k)).map_or(c0, |x| x.0);
                (grid, (r, col))
            }
            InsertShift::Down => {
                // From the selection down, every row's cells in the columns: within them, unmerged, and with the same
                // boundaries. The rows above stay where they are.
                let gr = &g;
                let starts = |r: usize| (c0..=c1).filter(move |&c| gr.rows[r].iter().any(|x| x.0 == c));
                let simple = (r0..rows.len()).all(|r| {
                    starts(r).eq(starts(r0))
                        && (c0..=c1).all(|c| {
                            g.cell_at(r, c).is_some_and(|k| {
                                g.rows[r][k].0 >= c0 && g.rows[r][k].0 + g.rows[r][k].1.max(1) - 1 <= c1 && rows[r].cells[k].v_merge == VMerge::None
                            })
                        })
                });
                if !simple {
                    return Outcome::unavailable("merged cells in the columns");
                }
                let n = r1 - r0 + 1;
                if rows.len() + n > super::tables::MAX_ROWS {
                    return Outcome::unavailable("too many rows");
                }
                // Rows at the end take what moves past the last: in the last one's controls, each its own repeating
                // item, as rows inserted below it are.
                let mut added: Vec<Row> = Vec::with_capacity(n);
                for _ in 0..n {
                    let mut r = rows[last].clone();
                    r.detach();
                    r.rev = Default::default();
                    r.cells.iter_mut().for_each(|c| *c = empty_cell(c));
                    added.push(r);
                }
                self.fresh_items(&mut added, &[]);
                rows.extend(added);
                let all = Grid::of_rows(&rows, own_grid);
                for c in starts(r0) {
                    let ks: Vec<usize> = (0..rows.len()).map(|r| all.cell_at(r, c).unwrap_or(0)).collect();
                    for r in (r0..rows.len()).rev() {
                        let cell = if r >= r0 + n { rows[r - n].cells[ks[r - n]].clone() } else { empty_cell(&rows[r].cells[ks[r]]) };
                        let to = &mut rows[r].cells[ks[r]];
                        *to = moved_cell(cell, to);
                    }
                }
                (grid, (r0, c0))
            }
        };
        self.rebuild(super::revisions::record::INSERT_CELLS, tg.table, rows, grid, caret, fonts)
    }

    pub(super) fn delete_cells_cmd(&mut self, up: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        if super::tables::cuts_region(t, tg.rows.0, t.rows.len() - 1, false) {
            return self.region_boundary();
        }
        let g = Grid::of(t);
        let (c0, c1) = tg.cols;
        let (r0, r1) = tg.rows;
        let mut rows: Vec<Row> = t.rows.clone();
        if up {
            // From the selection down; the rows above stay where they are
            let simple = (r0..rows.len())
                .all(|r| (c0..=c1).all(|c| g.cell_at(r, c).is_some_and(|k| g.rows[r][k].1.max(1) == 1 && rows[r].cells[k].v_merge == VMerge::None)));
            if !simple {
                return Outcome::Unchanged("merged cells in the columns");
            }
            let n = r1 - r0 + 1;
            for c in c0..=c1 {
                let ks: Vec<usize> = (0..rows.len()).map(|r| g.cell_at(r, c).unwrap_or(0)).collect();
                for r in r0..rows.len() {
                    let from = r + n;
                    let cell = if from < rows.len() { t.rows[from].cells[ks[from]].clone() } else { empty_cell(&t.rows[r].cells[ks[r]]) };
                    let to = &mut rows[r].cells[ks[r]];
                    *to = moved_cell(cell, to);
                }
            }
        } else {
            // The old index of each cell left
            let mut origin: Vec<Vec<usize>> = rows.iter().map(|r| (0..r.cells.len()).collect()).collect();
            for (r, row) in rows.iter_mut().enumerate().filter(|(r, _)| (r0..=r1).contains(r)) {
                let mut kept = Vec::new();
                let mut from = Vec::new();
                let mut gone = 0;
                for (k, cell) in row.cells.iter().enumerate() {
                    let (c, s, _) = g.rows[r][k];
                    if c >= c0 && c + s.max(1) - 1 <= c1 {
                        gone += s.max(1);
                    } else {
                        kept.push(cell.clone());
                        from.push(k);
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                row.cells = kept;
                row.grid_after += gone;
                origin[r] = from;
            }
            if rows.iter().zip(&t.rows).all(|(a, b)| a.cells.len() == b.cells.len()) {
                return Outcome::Unchanged("no whole cells in the target");
            }
            // A continuation now under a cell other than the one it continued starts a merge of its own
            let ng = Grid::of_rows(&rows, t.grid.len());
            for r in 1..rows.len() {
                for i in 0..rows[r].cells.len() {
                    if rows[r].cells[i].v_merge != VMerge::Continue {
                        continue;
                    }
                    let old = g.rows[r][origin[r][i]].0;
                    let was = g.cell_at(r - 1, old).filter(|&k| g.rows[r - 1][k].0 == old);
                    let col = ng.rows[r][i].0;
                    let now = ng.cell_at(r - 1, col).filter(|&k| ng.rows[r - 1][k].0 == col).map(|k| origin[r - 1][k]);
                    if was.is_none() || now != was {
                        rows[r].cells[i].v_merge = VMerge::Restart;
                    }
                }
            }
        }
        let grid = t.grid.clone();
        self.rebuild("Delete Cells", tg.table, rows, grid, (r0, c0.saturating_sub(1)), fonts)
    }

    pub(super) fn distribute_columns(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let (c0, c1) = if tg.cols.0 == tg.cols.1 && tg.rows.0 == tg.rows.1 { (0, g.cols.saturating_sub(1)) } else { tg.cols };
        if c1 <= c0 {
            return Outcome::Unchanged("one column");
        }
        let mut grid = super::tables::full_grid(t, g.cols);
        let total: f64 = grid[c0..=c1].iter().sum();
        let each = total / (c1 - c0 + 1) as f64;
        grid[c0..=c1].iter_mut().for_each(|w| *w = each);
        let mut rows = t.rows.clone();
        for (r, row) in rows.iter_mut().enumerate() {
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let (c, s, _) = g.rows[r][k];
                cell.width = Some(Width::Dxa(grid[c..c + s].iter().sum()));
            }
        }
        self.rebuild("Distribute Columns", tg.table, rows, grid, (tg.rows.0, tg.cols.0), fonts)
    }

    pub(super) fn distribute_rows(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let (r0, r1) = if tg.cols.0 == tg.cols.1 && tg.rows.0 == tg.rows.1 { (0, t.rows.len().saturating_sub(1)) } else { tg.rows };
        let mut heights = vec![0.0; t.rows.len()];
        for geo in self.measured_pages(tg.table) {
            for gr in geo.rows.iter().filter(|r| r.table == tg.table && !r.repeat) {
                if let Some(h) = heights.get_mut(gr.row) {
                    *h += gr.rect[3] - gr.rect[1];
                }
            }
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let avg = heights[r0..=r1].iter().sum::<f64>() / (r1 - r0 + 1) as f64;
        let mut rows = t.rows.clone();
        for row in &mut rows[r0..=r1] {
            super::table_format::set_height(row, Some((avg, crate::docx::HeightRule::AtLeast)));
        }
        let grid = t.grid.clone();
        self.rebuild("Distribute Rows", tg.table, rows, grid, (tg.rows.0, tg.cols.0), fonts)
    }
}
