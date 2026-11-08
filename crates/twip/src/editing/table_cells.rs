//! Table cell commands

use crate::docx::props::Width;
use crate::docx::{Row, VMerge};
use crate::font::Fonts;

use super::Editor;
use super::commands::Outcome;
use super::table::Grid;
use super::tables::empty_cell;

impl Editor {
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
            let simple = (0..rows.len())
                .all(|r| (c0..=c1).all(|c| g.cell_at(r, c).is_some_and(|k| g.rows[r][k].1.max(1) == 1 && rows[r].cells[k].v_merge == VMerge::None)));
            if !simple {
                return Outcome::Unchanged("merged cells in the columns");
            }
            let n = r1 - r0 + 1;
            for c in c0..=c1 {
                let ks: Vec<usize> = (0..rows.len()).filter_map(|r| g.cell_at(r, c)).collect();
                for r in r0..rows.len() {
                    let from = r + n;
                    let blocks =
                        if from < rows.len() { t.rows[from].cells[ks[from]].blocks.clone() } else { empty_cell(&t.rows[r].cells[ks[r]]).blocks };
                    rows[r].cells[ks[r]].blocks = blocks;
                }
            }
        } else {
            for (r, row) in rows.iter_mut().enumerate().filter(|(r, _)| (r0..=r1).contains(r)) {
                let mut kept = Vec::new();
                let mut gone = 0;
                for (k, cell) in row.cells.iter().enumerate() {
                    let (c, s, _) = g.rows[r][k];
                    if c >= c0 && c + s.max(1) - 1 <= c1 {
                        gone += s.max(1);
                    } else {
                        kept.push(cell.clone());
                    }
                }
                if kept.is_empty() {
                    continue;
                }
                row.cells = kept;
                row.grid_after += gone;
            }
            if rows.iter().zip(&t.rows).all(|(a, b)| a.cells.len() == b.cells.len()) {
                return Outcome::Unchanged("no whole cells in the target");
            }
            let mut shifted = t.clone();
            shifted.rows = rows.clone();
            let g = Grid::of(&shifted);
            for r in 1..rows.len() {
                for (k, &(c, _, v)) in g.rows[r].iter().enumerate() {
                    if v == VMerge::Continue && g.cell_at(r - 1, c).is_none_or(|a| g.rows[r - 1][a].0 != c) {
                        rows[r].cells[k].v_merge = VMerge::None;
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
        let (c0, c1) = if tg.cols.0 == tg.cols.1 && tg.rows.0 == tg.rows.1 { (0, t.grid.len().saturating_sub(1)) } else { tg.cols };
        if c1 <= c0 {
            return Outcome::Unchanged("one column");
        }
        let mut grid = t.grid.clone();
        let total: f64 = grid[c0..=c1].iter().sum();
        let each = total / (c1 - c0 + 1) as f64;
        grid[c0..=c1].iter_mut().for_each(|w| *w = each);
        let mut rows = t.rows.clone();
        for (r, row) in rows.iter_mut().enumerate() {
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let (c, s, _) = g.rows[r][k];
                cell.width = Some(Width::Dxa(grid[c..(c + s.max(1)).min(grid.len())].iter().sum()));
            }
        }
        self.rebuild("Distribute Columns", tg.table, rows, grid, (tg.rows.0, tg.cols.0), fonts)
    }

    pub(super) fn distribute_rows(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let (r0, r1) = if tg.cols.0 == tg.cols.1 && tg.rows.0 == tg.rows.1 { (0, t.rows.len().saturating_sub(1)) } else { tg.rows };
        let mut heights = vec![0.0; t.rows.len()];
        for (_, geo) in self.page_geometries() {
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
            row.height = Some((avg, "atLeast".into()));
        }
        let grid = t.grid.clone();
        self.rebuild("Distribute Rows", tg.table, rows, grid, (tg.rows.0, tg.cols.0), fonts)
    }

    pub(super) fn split_cell_rows(&mut self, n: usize, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        if n < 2 {
            return Outcome::Unchanged("nothing to split");
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let r = tg.rows.0;
        let target: Vec<usize> = (0..t.rows[r].cells.len())
            .filter(|&k| {
                let (c, s, _) = g.rows[r][k];
                c + s.max(1) > tg.cols.0 && c <= tg.cols.1
            })
            .collect();
        let mut rows = t.rows.clone();
        let first = &mut rows[r];
        first.height = None;
        let template = first.clone();
        for k in 0..first.cells.len() {
            if !target.contains(&k) && first.cells[k].v_merge == VMerge::None {
                first.cells[k].v_merge = VMerge::Restart;
            }
        }
        let mut added = Vec::new();
        for _ in 1..n {
            let mut row = template.clone();
            row.src = None;
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let keep = cell.v_merge;
                *cell = empty_cell(cell);
                if !target.contains(&k) {
                    cell.v_merge = if keep == VMerge::None || keep == VMerge::Restart { VMerge::Continue } else { keep };
                }
            }
            added.push(row);
        }
        rows.splice(r + 1..r + 1, added);
        let grid = t.grid.clone();
        self.rebuild("Split Cells", tg.table, rows, grid, (r, tg.cols.0), fonts)
    }
}
