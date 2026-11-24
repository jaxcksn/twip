//! Cell block clipboard

use crate::docx::{Block, Row, Table};
use crate::font::Fonts;

use super::Editor;
use super::clipboard::{Fragment, fresh};
use super::commands::Outcome;
use super::paste::PasteChoice;
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

    /// The table directly before the caret's paragraph in its story (none for a block of cells): whole rows pasted
    /// there go at its end.
    pub(super) fn table_before_caret(&self) -> Option<&Table> {
        if matches!(self.selection, Selection::Cells { .. }) {
            return None;
        }
        let (para, _) = self.selection.focus()?.para_offset()?;
        let slot = self.index.para_slot(para)?;
        match self.index.blocks(&self.doc, slot.container)?.get(slot.block.checked_sub(1)?) {
            Some(Block::Table(t)) => Some(t),
            _ => None,
        }
    }

    /// The table whole rows of `copy` go into as rows, and the row they go above: the end of a table the caret's
    /// paragraph follows, else the caret's row.
    pub(super) fn rows_target(&self, copy: CellCopy) -> Option<(crate::layout::TableId, usize)> {
        if copy.whole_rows
            && let Some(t) = self.table_before_caret()
        {
            return Some((crate::layout::TableId(t.id), t.rows.len()));
        }
        let tg = self.table_target()?;
        Some((tg.table, tg.rows.0))
    }

    /// The table choice [`Editor::paste_cells`] takes for copied cells (or a table) `f` at the selection in a table:
    /// rows at the end of a table the caret's paragraph follows, rows at a row's start or over selected rows, the cells' content over the cells from the caret's (or the
    /// selected block), a table in a selected cell or at a caret for a whole table. `None` outside a table.
    pub(super) fn suggested_cells(&self, f: &Fragment) -> Option<PasteChoice> {
        if f.cells.is_some_and(|c| c.whole_rows) && self.table_before_caret().is_some() {
            return Some(PasteChoice::InsertRows);
        }
        let tg = self.table_target()?;
        let Some(copy) = f.cells else { return Some(PasteChoice::NestTable) };
        let block = matches!(self.selection, Selection::Cells { .. });
        let single = tg.rows.0 == tg.rows.1 && tg.cols.0 == tg.cols.1;
        let at_row_start = !block && self.selection.focus().is_some_and(|p| p.para_offset().is_some_and(|(_, o)| o.0 == 0)) && tg.cols.0 == 0;
        let over_rows = matches!(self.selection, Selection::Cells { shape: CellShape::Rows, .. });
        Some(if copy.whole_table && !block {
            PasteChoice::NestTable
        } else if copy.whole_rows && (at_row_start || over_rows) {
            PasteChoice::InsertRows
        } else if block && single {
            PasteChoice::NestTable
        } else {
            PasteChoice::OverwriteCells
        })
    }

    pub(super) fn paste_cells(&mut self, f: &Fragment, fonts: &Fonts) -> Option<Outcome> {
        self.paste_cells_as(f, None, fonts)
    }

    /// Copied cells pasted as `choice` (a table choice) takes them; `None`: as the copy and the selection suggest.
    pub(super) fn paste_cells_as(&mut self, f: &Fragment, choice: Option<PasteChoice>, fonts: &Fonts) -> Option<Outcome> {
        let copy = f.cells?;
        let Some(Block::Table(src)) = f.blocks.first() else { return None };
        let wide = |grid: &[f64]| -> Vec<f64> { grid.iter().chain(src.grid.iter().skip(grid.len())).copied().collect() };
        let after = copy.whole_rows && self.table_before_caret().is_some();
        if matches!(choice, Some(PasteChoice::InsertRows | PasteChoice::MergeTable)) || (choice.is_none() && after) {
            let (id, at) = self.rows_target(copy)?;
            let t = self.index.table(&self.doc, id)?.clone();
            let mut rows = t.rows.clone();
            let mut tx = self.begin("Paste Rows");
            let mut new = self.take_rows(&mut tx, src, f, &rows, at);
            let grid = if choice == Some(PasteChoice::MergeTable) {
                if let Some(like) = rows.get(at.min(rows.len().saturating_sub(1))) {
                    new = merged_rows(&new, like);
                }
                t.grid.clone()
            } else {
                wide(&t.grid)
            };
            let n = new.len();
            rows.splice(at..at, new);
            if n > 0
                && let Some(below) = rows.get_mut(at + n)
            {
                for c in &mut below.cells {
                    if c.v_merge == crate::docx::VMerge::Continue {
                        c.v_merge = crate::docx::VMerge::Restart;
                    }
                }
            }
            return Some(self.rebuild_in(tx, id, rows, grid, (at, 0), fonts));
        }
        let overwrite = choice == Some(PasteChoice::OverwriteCells);
        let tg = self.table_target()?;
        let block = matches!(self.selection, Selection::Cells { .. });
        let single = tg.rows.0 == tg.rows.1 && tg.cols.0 == tg.cols.1;
        if copy.whole_table && !block && !overwrite {
            return None;
        }
        let t = self.index.table(&self.doc, tg.table)?.clone();
        let g = Grid::of(&t);
        let mut rows = t.rows.clone();
        // A source cell's content as a fresh copy (the block repeats over a larger target).
        let source = |r: usize, j: usize| {
            let row = src.rows.get(r % src.rows.len().max(1))?;
            row.cells.get(j % row.cells.len().max(1))
        };
        let tile = |r: usize, j: usize| -> Option<Vec<Block>> { source(r, j).map(|c| c.blocks.iter().map(fresh).collect()) };
        // Whole rows
        let at_row_start = !block && self.selection.focus().is_some_and(|p| p.para_offset().is_some_and(|(_, o)| o.0 == 0)) && tg.cols.0 == 0;
        let over_rows = matches!(self.selection, Selection::Cells { shape: CellShape::Rows, .. });
        if !overwrite && copy.whole_rows && (at_row_start || over_rows) {
            let mut tx = self.begin("Paste Rows");
            let new = self.take_rows(&mut tx, src, f, &rows, tg.rows.0);
            let (at, n) = (tg.rows.0, new.len());
            rows.splice(at..at, new);
            // A merged cell the rows land in splits around them: its part below starts again.
            if at > 0 && n > 0 {
                for c in &mut rows[at + n].cells {
                    if c.v_merge == crate::docx::VMerge::Continue {
                        c.v_merge = crate::docx::VMerge::Restart;
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
            let at: Vec<(usize, Vec<usize>)> = (tg.rows.0..=tg.rows.1)
                .map(|r| {
                    let ks = (0..rows[r].cells.len())
                        .filter(|&k| g.rows[r].get(k).is_some_and(|&(c, s, _)| c + s.max(1) > tg.cols.0 && c <= tg.cols.1))
                        .collect();
                    (r, ks)
                })
                .collect();
            for (i, (r, ks)) in at.iter().enumerate() {
                for (j, &k) in ks.iter().enumerate() {
                    if source(i, j).is_some() {
                        taken.replacing(&rows[*r].cells[k].blocks);
                    }
                }
            }
            for (i, (r, ks)) in at.into_iter().enumerate() {
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
        let place = |i: usize, j: usize| {
            let r = tg.rows.0 + i;
            if i == 0 { k0 + j } else { g.cell_at(r.min(t.rows.len() - 1), tg.cols.0).unwrap_or(0) + j }
        };
        for (i, row) in src.rows.iter().enumerate().take(n.saturating_sub(tg.rows.0)) {
            for j in 0..row.cells.len() {
                if let Some(c) = rows[tg.rows.0 + i].cells.get(place(i, j)) {
                    taken.replacing(&c.blocks);
                }
            }
        }
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
                let k = place(i, j);
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

/// Pasted rows made like `like`, a row of the table they land in: its cells (their widths and formatting), each
/// holding the content of the pasted cell in its place, the pasted row's controls and revision kept.
fn merged_rows(new: &[Row], like: &Row) -> Vec<Row> {
    new.iter()
        .map(|r| {
            let mut row = like.clone();
            row.src = None;
            (row.controls, row.rev) = (r.controls.clone(), r.rev.clone());
            for (k, c) in row.cells.iter_mut().enumerate() {
                c.v_merge = crate::docx::VMerge::None;
                *c = match r.cells.get(k) {
                    Some(from) => crate::docx::Cell { blocks: from.blocks.clone(), ..empty_cell(c) },
                    None => empty_cell(c),
                };
            }
            row
        })
        .collect()
}
