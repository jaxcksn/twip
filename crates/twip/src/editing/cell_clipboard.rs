//! Cell block clipboard

use crate::docx::{Block, Row, Table};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::{Fragment, fresh};
use super::commands::Outcome;
use super::selection::{CellShape, Selection};
use super::table::Grid;
use super::tables::empty_cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCopy {
    pub whole_rows: bool,
    pub whole_table: bool,
}

impl Editor {
    pub(super) fn copy_cells(&mut self, id: u64) -> Option<Fragment> {
        let tg = self.table_target()?;
        let Selection::Cells { shape, .. } = self.selection else { return None };
        let t = self.index.table(&self.doc, tg.table)?;
        let g = Grid::of(t);
        let (r0, r1) = tg.rows;
        let (c0, c1) = tg.cols;
        let mut rows = Vec::new();
        for r in r0..=r1.min(t.rows.len().saturating_sub(1)) {
            let mut row: Row = t.rows[r].clone();
            row.src = None;
            row.grid_before = 0;
            row.grid_after = 0;
            row.cells = t.rows[r]
                .cells
                .iter()
                .enumerate()
                .filter(|(k, _)| g.rows[r].get(*k).is_some_and(|&(c, s, _)| c + s.max(1) > c0 && c <= c1))
                .map(|(k, cell)| {
                    let (c, s, _) = g.rows[r][k];
                    let mut cell = cell.clone();
                    cell.span = (c + s.max(1)).min(c1 + 1) - c.max(c0);
                    cell
                })
                .collect();
            rows.push(row);
        }
        let whole_rows = shape == CellShape::Rows || (c0 == 0 && c1 + 1 >= g.cols);
        let whole_table = whole_rows && r0 == 0 && r1 + 1 >= t.rows.len();
        let sub =
            Table { grid: super::tables::full_grid(t, g.cols)[c0..=c1].to_vec(), rows, src: None, id: crate::docx::next_para_id(), ..t.clone() };
        Some(Fragment { id, blocks: vec![Block::Table(sub)], cells: Some(CellCopy { whole_rows, whole_table }), ..Default::default() })
    }

    pub(super) fn paste_cells(&mut self, f: &Fragment, fonts: &Fonts) -> Option<Outcome> {
        let copy = f.cells?;
        let Some(Block::Table(src)) = f.blocks.first() else { return None };
        let wide = |grid: &[f64]| -> Vec<f64> { grid.iter().chain(src.grid.iter().skip(grid.len())).copied().collect() };
        if copy.whole_rows
            && !matches!(self.selection, Selection::Cells { .. })
            && let Some((para, _)) = self.selection.focus().and_then(|p| p.para_offset())
            && let Some(slot) = self.index.para_slot(para)
            && slot.block > 0
            && let Some(Block::Table(t)) = self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block - 1))
        {
            let (id, mut rows, grid) = (crate::layout::TableId(t.id), t.rows.clone(), t.grid.clone());
            let at = rows.len();
            let mut tx = self.begin("Paste Rows");
            let new = self.take_rows(&mut tx, src, f, &rows, at);
            rows.extend(new);
            return Some(self.rebuild_in(tx, id, rows, wide(&grid), (at, 0), fonts));
        }
        let tg = self.table_target()?;
        let block = matches!(self.selection, Selection::Cells { .. });
        let single = tg.rows.0 == tg.rows.1 && tg.cols.0 == tg.cols.1;
        if copy.whole_table && !block {
            return None;
        }
        let t = self.index.table(&self.doc, tg.table)?.clone();
        let g = Grid::of(&t);
        let mut rows = t.rows.clone();
        // A source cell's content as a fresh copy (the block repeats over a larger target).
        let tile = |r: usize, j: usize| -> Option<Vec<Block>> {
            let row = &src.rows[r % src.rows.len().max(1)];
            row.cells.get(j % row.cells.len().max(1)).map(|c| c.blocks.iter().map(fresh).collect())
        };
        // Whole rows
        let at_row_start = !block && self.selection.focus().is_some_and(|p| p.para_offset().is_some_and(|(_, o)| o.0 == 0)) && tg.cols.0 == 0;
        let over_rows = matches!(self.selection, Selection::Cells { shape: CellShape::Rows, .. });
        if copy.whole_rows && (at_row_start || over_rows) {
            let mut tx = self.begin("Paste Rows");
            let new = self.take_rows(&mut tx, src, f, &rows, tg.rows.0);
            let (at, n) = (tg.rows.0, new.len());
            rows.splice(at..at, new);
            // A merged cell the rows land in splits around them: its part below starts again.
            if at > 0 && n > 0 {
                let g = Grid::of_rows(&rows, t.grid.len());
                for k in 0..rows[at + n].cells.len() {
                    let c = g.rows[at + n][k].0;
                    let above = g.cell_at(at + n - 1, c).filter(|&a| g.rows[at + n - 1][a].0 == c).map(|a| g.rows[at + n - 1][a].2);
                    if g.continues(at + n, k) && above.is_none_or(|v| v == crate::docx::VMerge::None) {
                        rows[at + n].cells[k].v_merge = crate::docx::VMerge::Restart;
                    }
                }
            }
            return Some(self.rebuild_in(tx, tg.table, rows, wide(&t.grid), (tg.rows.0, 0), fonts));
        }
        let mut tx = self.begin("Paste Cells");
        let mut taken = super::clipboard::Taken::default();
        if block && single {
            // One selected cell
            let Some(k) = g.cell_at(tg.rows.0, tg.cols.0) else {
                self.abort(tx);
                return None;
            };
            let mut sub = vec![fresh(&Block::Table(src.clone()))];
            self.take_in(&mut tx, &mut sub, f, &mut taken);
            let cell = &mut rows[tg.rows.0].cells[k];
            let closing = empty_cell(cell).blocks;
            cell.blocks.extend(sub);
            cell.blocks.extend(closing);
            return Some(self.rebuild_in(tx, tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts));
        }
        if block {
            // Cell block
            for (i, r) in (tg.rows.0..=tg.rows.1).enumerate() {
                let ks: Vec<usize> = (0..rows[r].cells.len())
                    .filter(|&k| g.rows[r].get(k).is_some_and(|&(c, s, _)| c + s.max(1) > tg.cols.0 && c <= tg.cols.1))
                    .collect();
                for (j, k) in ks.into_iter().enumerate() {
                    if let Some(mut b) = tile(i, j) {
                        self.take_in(&mut tx, &mut b, f, &mut taken);
                        rows[r].cells[k].blocks = b;
                    }
                }
            }
            return Some(self.rebuild_in(tx, tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts));
        }
        // Caret in a cell
        let Some(k0) = g.cell_at(tg.rows.0, tg.cols.0) else {
            self.abort(tx);
            return None;
        };
        let n = rows.len();
        for i in 0..src.rows.len() {
            let r = tg.rows.0 + i;
            if r >= rows.len() {
                let Some(mut add) = rows.last().cloned() else {
                    self.abort(tx);
                    return None;
                };
                add.src = None;
                add.rev = Default::default();
                add.cells.iter_mut().for_each(|c| *c = empty_cell(c));
                rows.push(add);
            }
            for j in 0..src.rows[i].cells.len() {
                let k = if i == 0 { k0 + j } else { g.cell_at(r.min(t.rows.len() - 1), tg.cols.0).unwrap_or(0) + j };
                if k < rows[r].cells.len()
                    && let Some(mut b) = tile(i, j)
                {
                    self.take_in(&mut tx, &mut b, f, &mut taken);
                    rows[r].cells[k].blocks = b;
                }
            }
        }
        // Rows added below the table are new rows of it, each its own repeating item, as Insert Rows makes them.
        if rows.len() > n {
            self.fresh_items(&mut rows[n..], &[]);
        }
        Some(self.rebuild_in(tx, tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts))
    }

    /// Pasted rows for `rows` (the table's, before the paste) at `at`, in `tx`: fresh copies made this document's, in
    /// the controls the rows on both sides share. Between two rows of a repeating section a row is an item of it: a
    /// copied item keeps its item, not its section, and a row that isn't one becomes a new item.
    fn take_rows(&mut self, tx: &mut super::Tx, src: &Table, f: &Fragment, rows: &[Row], at: usize) -> Vec<Row> {
        use crate::docx::{ControlId, ControlKind};
        let inside: Vec<ControlId> = match (at.checked_sub(1).and_then(|k| rows.get(k)), rows.get(at)) {
            (Some(a), Some(b)) => a.controls.iter().copied().filter(|c| b.controls.contains(c)).collect(),
            _ => Vec::new(),
        };
        let is = |defs: &std::collections::HashMap<ControlId, crate::docx::Control>, c: &ControlId, kind: ControlKind| {
            defs.get(c).is_some_and(|k| k.props.kind == kind)
        };
        let in_section = inside.iter().any(|c| is(&self.doc.controls, c, ControlKind::RepeatingSection));
        let in_item = inside.iter().any(|c| is(&self.doc.controls, c, ControlKind::RepeatingItem));
        let copied: Vec<Row> = src
            .rows
            .iter()
            .map(|r| {
                let mut r = r.clone();
                if in_section && let Some(k) = r.controls.iter().position(|c| is(&f.controls, c, ControlKind::RepeatingItem)) {
                    r.controls.drain(..k + usize::from(in_item));
                }
                r
            })
            .collect();
        let mut blocks = vec![fresh(&Block::Table(Table { rows: copied, controls: Vec::new(), ..src.clone() }))];
        self.take_in(tx, &mut blocks, f, &mut super::clipboard::Taken::default());
        let Some(Block::Table(t)) = blocks.pop() else { return Vec::new() };
        let neighbour = at
            .checked_sub(1)
            .and_then(|k| rows.get(k))
            .and_then(|r| r.controls.iter().copied().find(|c| !inside.contains(c) && is(&self.doc.controls, c, ControlKind::RepeatingItem)));
        let mut new = t.rows;
        for r in &mut new {
            let item = r.controls.iter().any(|c| is(&self.doc.controls, c, ControlKind::RepeatingItem));
            r.controls.splice(0..0, inside.iter().copied());
            if in_section
                && !in_item
                && !item
                && let Some(n) = neighbour
            {
                r.controls.insert(inside.len(), n);
                self.fresh_items(std::slice::from_mut(r), &inside);
            }
        }
        new
    }
}
