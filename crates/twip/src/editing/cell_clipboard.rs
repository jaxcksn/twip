//! Copying and pasting cell blocks. A copied block is a small table of its cells; it
//! remembers whether it was whole rows or the whole table. Pasting it overwrites a selected block (repeated to fill
//! it), nests it over one selected cell, overwrites to the right and down from a caret in a cell (adding rows past the
//! table's end), and puts copied whole rows above the target row. A whole table nests at the caret like any blocks.

use crate::docx::{Block, Row, Table};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::{Fragment, fresh};
use super::commands::Outcome;
use super::selection::{CellShape, Selection};
use super::table::Grid;
use super::tables::empty_cell;

/// What a copied cell block was.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellCopy {
    pub whole_rows: bool,
    pub whole_table: bool,
}

impl Editor {
    /// The fragment of a selected cell block: its rows' cells over the block's columns, as a table.
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
        let sub = Table {
            grid: t.grid[c0..=c1.min(t.grid.len().saturating_sub(1))].to_vec(),
            rows,
            src: None,
            id: crate::docx::next_para_id(),
            ..t.clone()
        };
        Some(Fragment {
            id,
            blocks: vec![Block::Table(sub)],
            open_end: false,
            cells: Some(CellCopy { whole_rows, whole_table }),
            controls: Default::default(),
        })
    }

    /// Pastes a copied cell block over the cells at the target; `None` when the plain path applies (outside a table, or a
    /// whole table at a caret, which nests like any blocks).
    pub(super) fn paste_cells(&mut self, f: &Fragment, fonts: &Fonts) -> Option<Outcome> {
        let copy = f.cells?;
        let Some(Block::Table(src)) = f.blocks.first() else { return None };
        // Pasted cells and rows take no content controls (they are written through a table rebuild, which adds no
        // control); their fields get new ids.
        // shortcut: controls in pasted cells are dropped; carry them as `paste_fragment` does if templates need it.
        let mut stripped = vec![Block::Table(src.clone())];
        super::clipboard::adopt(&mut stripped, &Default::default());
        super::clipboard::strip_controls(&mut stripped);
        let Some(Block::Table(src)) = stripped.first() else { return None };
        // Rows wider than the table they join bring their extra grid columns (at the copy's widths).
        let wide = |grid: &[f64]| -> Vec<f64> { grid.iter().chain(src.grid.iter().skip(grid.len())).copied().collect() };
        // Whole rows pasted in the paragraph right after a table join that table.
        if copy.whole_rows
            && !matches!(self.selection, Selection::Cells { .. })
            && let Some((para, _)) = self.selection.focus().and_then(|p| p.para_offset())
            && let Some(slot) = self.index.para_slot(para)
            && slot.block > 0
            && let Some(Block::Table(t)) = self.index.blocks(&self.doc, slot.container).and_then(|b| b.get(slot.block - 1))
        {
            let (id, mut rows, grid) = (crate::layout::TableId(t.id), t.rows.clone(), t.grid.clone());
            let at = rows.len();
            rows.extend(src.rows.iter().map(|r| {
                let mut r = r.clone();
                r.src = None;
                r.cells.iter_mut().for_each(|c| c.blocks = c.blocks.iter().map(fresh).collect());
                r
            }));
            return Some(self.rebuild("Paste Rows", id, rows, wide(&grid), (at, 0), fonts));
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
        let fresh_cells = |r: usize, j: usize| -> Option<Vec<Block>> {
            let row = &src.rows[r % src.rows.len().max(1)];
            row.cells.get(j % row.cells.len().max(1)).map(|c| c.blocks.iter().map(fresh).collect())
        };
        // Whole rows at a row's start, or over selected rows: inserted above the target row.
        let at_row_start = !block && self.selection.focus().is_some_and(|p| p.para_offset().is_some_and(|(_, o)| o.0 == 0)) && tg.cols.0 == 0;
        let over_rows = matches!(self.selection, Selection::Cells { shape: CellShape::Rows, .. });
        if copy.whole_rows && (at_row_start || over_rows) {
            let new: Vec<Row> = src
                .rows
                .iter()
                .map(|r| {
                    let mut r = r.clone();
                    r.src = None;
                    r.cells.iter_mut().for_each(|c| c.blocks = c.blocks.iter().map(fresh).collect());
                    r
                })
                .collect();
            rows.splice(tg.rows.0..tg.rows.0, new);
            return Some(self.rebuild("Paste Rows", tg.table, rows, wide(&t.grid), (tg.rows.0, 0), fonts));
        }
        if block && single {
            // Over one selected cell: the copy nests there as a table, after the cell's content.
            let k = g.cell_at(tg.rows.0, tg.cols.0)?;
            let mut sub = src.clone();
            sub.id = crate::docx::next_para_id();
            let Block::Table(sub) = fresh(&Block::Table(sub)) else { return None };
            let cell = &mut rows[tg.rows.0].cells[k];
            let closing = empty_cell(cell).blocks;
            cell.blocks.push(Block::Table(sub));
            cell.blocks.extend(closing);
            return Some(self.rebuild("Paste Cells", tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts));
        }
        if block {
            // Over a block: each cell takes the copy's cell at the same place, the copy repeated to fill the block.
            for (i, r) in (tg.rows.0..=tg.rows.1).enumerate() {
                let ks: Vec<usize> = (0..rows[r].cells.len())
                    .filter(|&k| g.rows[r].get(k).is_some_and(|&(c, s, _)| c + s.max(1) > tg.cols.0 && c <= tg.cols.1))
                    .collect();
                for (j, k) in ks.into_iter().enumerate() {
                    if let Some(b) = fresh_cells(i, j) {
                        rows[r].cells[k].blocks = b;
                    }
                }
            }
            return Some(self.rebuild("Paste Cells", tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts));
        }
        // At a caret in a cell: overwrite to the right and down from it, adding rows past the table's end.
        // shortcut: columns past the table's right edge are dropped rather than added.
        let k0 = g.cell_at(tg.rows.0, tg.cols.0)?;
        for i in 0..src.rows.len() {
            let r = tg.rows.0 + i;
            if r >= rows.len() {
                let mut add = rows.last()?.clone();
                add.src = None;
                add.cells.iter_mut().for_each(|c| *c = empty_cell(c));
                rows.push(add);
            }
            for j in 0..src.rows[i].cells.len() {
                let k = if i == 0 { k0 + j } else { g.cell_at(r.min(t.rows.len() - 1), tg.cols.0).unwrap_or(0) + j };
                if k < rows[r].cells.len()
                    && let Some(b) = fresh_cells(i, j)
                {
                    rows[r].cells[k].blocks = b;
                }
            }
        }
        Some(self.rebuild("Paste Cells", tg.table, rows, t.grid.clone(), (tg.rows.0, tg.cols.0), fonts))
    }
}
