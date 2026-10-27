//! Table structure commands: rows and columns inserted
//! and deleted, cells merged and split, tables inserted and deleted. Each command edits the targeted table only, the
//! innermost one at the caret, through one `Rows` step for the whole table (or `Blocks` for a table inserted or
//! removed), so undo restores it exactly and kept paragraphs keep their positions.

use crate::docx::{Block, Cell, Inline, Paragraph, Row, Table, VMerge};
use crate::font::Fonts;
use crate::layout::{ParaId, TableId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::position::Position;
use super::selection::{CellRef, Selection};
use super::step::{Step, para_len};
use super::table::Grid;

/// Where a structure command applies: a table and a rectangle of grid rows and columns.
#[derive(Clone, Copy, Debug)]
pub(super) struct Target {
    pub table: TableId,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

/// An empty cell like `like`: its properties, one empty paragraph in its first paragraph's formatting.
pub(super) fn empty_cell(like: &Cell) -> Cell {
    let mut c = like.clone();
    c.v_merge = VMerge::None;
    // A new cell is in no cell-level control: a control wraps one place.
    c.controls = Vec::new();
    // Every cell ends with a paragraph (invariant 1): its formatting is the new cell's.
    let Some(p) = like.blocks.iter().rev().find_map(|b| if let Block::Para(p) = b { Some(p) } else { None }) else { return c };
    c.blocks = vec![Block::Para(Paragraph {
        style: p.style.clone(),
        mark: p.mark.clone(),
        list: None,
        inlines: vec![Inline::Text(String::new(), p.mark.clone())],
        src: None,
        id: crate::docx::next_para_id(),
        controls: Vec::new(),
    })];
    c
}

fn first_para(blocks: &[Block]) -> Option<ParaId> {
    blocks.iter().find_map(|b| match b {
        Block::Para(p) => Some(ParaId(p.id)),
        Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).find_map(|c| first_para(&c.blocks)),
    })
}

impl Editor {
    /// The innermost table at the selection, and the grid rectangle it covers (a caret: its cell).
    pub(super) fn table_target(&self) -> Option<Target> {
        if let Selection::Cells { table, anchor, focus, .. } = self.selection {
            let t = self.index.table(&self.doc, table)?;
            let g = Grid::of(t);
            let (r0, r1) = (anchor.row.min(focus.row), anchor.row.max(focus.row));
            let (c0, c1) = (anchor.col.min(focus.col), anchor.col.max(focus.col));
            let _ = g;
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
        // A text selection whose other end is in another cell of the same table covers both.
        let other = self.index.chain_of(&anchor).into_iter().find(|l| l.table == level.table);
        let (ar, (ac0, ac1)) = match other {
            Some(l) => (l.row, span(l.row, l.cell).unwrap_or((fc0, fc1))),
            None => (level.row, (fc0, fc1)),
        };
        Some(Target { table: level.table, rows: (ar.min(level.row), ar.max(level.row)), cols: (ac0.min(fc0), ac1.max(fc1)) })
    }

    /// Replaces a table's rows and grid in one step, then puts the caret at the start of cell (`row`, grid `col`).
    pub(super) fn rebuild(
        &mut self,
        label: &'static str,
        table: TableId,
        rows: Vec<Row>,
        grid: Vec<f64>,
        caret: (usize, usize),
        fonts: &Fonts,
    ) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let n = t.rows.len();
        let story = self.index.table_slot(table).map_or(self.editing(), |s| self.index.story(s.container));
        let target = {
            let g = Grid::of(&Table { rows: rows.clone(), grid: grid.clone(), ..t.clone() });
            let r = caret.0.min(rows.len().saturating_sub(1));
            g.cell_at(r, caret.1).or_else(|| rows.get(r).map(|x| x.cells.len().saturating_sub(1))).map(|c| g.merge_start(r, c))
        };
        let caret_para = target.and_then(|(r, c)| rows.get(r)?.cells.get(c).and_then(|c| first_para(&c.blocks)));
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::Rows { table, range: 0..n, with: rows, grid: Some(grid) }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        tx.stale_tables.push(table);
        if let Some(p) = caret_para {
            self.selection = Selection::Caret(Position::text(story, p, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    /// Alt+Shift+Up / Alt+Shift+Down: the target rows change places with the row above or below them,
    /// in one step; the selection moves with them. Rows with vertically merged cells don't move (the merge would split).
    pub(super) fn move_rows(&mut self, up: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let n = t.rows.len();
        let (r0, r1) = (tg.rows.0, tg.rows.1.min(n.saturating_sub(1)));
        if (up && r0 == 0) || (!up && r1 + 1 >= n) {
            return Outcome::Unchanged("at the table's edge");
        }
        let (a, b) = if up { (r0 - 1, r1) } else { (r0, r1 + 1) };
        // shortcut: rows in a vertical merge stay, rather than splitting or carrying the merge.
        if t.rows[a..=b].iter().any(|r| r.cells.iter().any(|c| c.v_merge != VMerge::None)) {
            return Outcome::Unchanged("vertically merged cells");
        }
        let mut with = t.rows[a..=b].to_vec();
        if up {
            with.rotate_left(1);
        } else {
            with.rotate_right(1);
        }
        let table = tg.table;
        let mut tx = self.begin("Move Rows");
        if self.step(&mut tx, Step::Rows { table, range: a..b + 1, with, grid: None }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("table edit failed");
        }
        tx.stale_tables.push(table);
        // The moved rows are selected.
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

    /// A double-click on a column border: the column to its left fits its content (the widest line of
    /// its cells unwrapped, plus the cell margins); the other columns keep their widths, so the table's width changes.
    /// It grows at most until the table reaches the text width plus the outer cell padding.
    pub(super) fn auto_fit_column(&mut self, table: TableId, col: usize, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        // The column's single-span cells: their paragraphs and margins.
        let mut paras: Vec<(crate::layout::ParaId, f64)> = Vec::new();
        for (r, row) in t.rows.iter().enumerate() {
            for (k, cell) in row.cells.iter().enumerate() {
                let Some(&(c, s, _)) = g.rows[r].get(k) else { continue };
                if c == col && s.max(1) == 1 {
                    let m = cell.margins[1] + cell.margins[3];
                    for b in &cell.blocks {
                        if let Block::Para(p) = b {
                            paras.push((crate::layout::ParaId(p.id), m + p.style.ind_left + p.style.ind_right));
                        }
                    }
                }
            }
        }
        if paras.is_empty() {
            return Outcome::Unchanged("no cell of its own in the column");
        }
        let (old_w, total, nrows) = (t.grid.get(col).copied(), t.grid.iter().sum::<f64>(), t.rows.len());
        // Each paragraph unwrapped: the sum of its laid-out lines' widths.
        let mut lines: std::collections::HashMap<crate::layout::ParaId, f64> = std::collections::HashMap::new();
        for (_, geo) in self.page_geometries() {
            for l in geo.lines.iter().filter(|l| !l.repeat) {
                *lines.entry(l.para).or_default() += (l.x1() - l.x0()).max(0.0);
            }
        }
        let margins = paras.iter().map(|(_, m)| *m).fold(0.0, f64::max);
        let content = paras.iter().map(|(p, m)| lines.get(p).copied().unwrap_or(0.0) + m).fold(0.0, f64::max);
        // At least the margins plus 4.5 pt.
        let mut w = content.max(margins + 4.5);
        let old = old_w.unwrap_or(w);
        let avail = self.index.table_slot(table).map_or(468.0, |s| self.text_width(s.container)) + margins + 1.0;
        // shortcut: the cap is one measured case (text width + padding); the exact limit may differ.
        if w > old {
            w = w.min(old + (avail - total).max(0.0));
        }
        let w = (w * 10.0).round() / 10.0;
        if (w - old).abs() < 0.05 {
            return Outcome::Unchanged("already fits");
        }
        let target = Target { table, rows: (0, nrows.saturating_sub(1)), cols: (col, col) };
        self.table_format_on(target, &super::table_format::TableFormat::ColumnWidth(w), fonts)
    }

    /// New rows copied from a row in a repeating section are items of their own: each repeating-item control
    /// they carry becomes a copy with a new id (its `w:id` too); the section itself is shared. An item in `inside`
    /// (the new rows went in between two of its rows) is kept: the rows join it, which stays one run of rows (C2).
    pub(super) fn fresh_items(&mut self, rows: &mut [Row], inside: &[crate::docx::ControlId]) {
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
                // The control's own `w:id` (a signed 32-bit number): unique again.
                if let Some(k) = c.open.find(":id ").or_else(|| c.open.find("<w:id "))
                    && let Some(v) = c.open[k..].find("val=\"").map(|v| k + v + 5)
                    && let Some(e) = c.open[v..].find('"').map(|e| v + e)
                {
                    c.open.replace_range(v..e, &((n.0 % 0x7fff_ffff) as i64 + 1).to_string());
                }
                self.pending_controls.push((n, c));
                *id = n;
            }
        }
    }

    /// Insert Rows Above / Below (n rows like the target's first or last row, empty). A vertical merge the new rows
    /// fall inside extends over them.
    pub(super) fn insert_rows(&mut self, below: bool, n: usize, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let at = if below { tg.rows.1 } else { tg.rows.0 };
        let template = &t.rows[at];
        let g = Grid::of(t);
        let mut row = template.clone();
        row.src = None;
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
        // With several rows selected, as many rows go in.
        let n = n.max(tg.rows.1 - tg.rows.0 + 1);
        for _ in 0..n {
            let mut r = row.clone();
            r.cells.iter_mut().for_each(|c| *c = Cell { blocks: empty_cell(c).blocks, ..c.clone() });
            rows.insert(pos, r);
        }
        let grid = t.grid.clone();
        // Controls the rows on both sides share: the new rows are inside them.
        let inside: Vec<crate::docx::ControlId> = match (pos.checked_sub(1).and_then(|k| rows.get(k)), rows.get(pos + n)) {
            (Some(a), Some(b)) => a.controls.iter().copied().filter(|c| b.controls.contains(c)).collect(),
            _ => Vec::new(),
        };
        self.fresh_items(&mut rows[pos..pos + n], &inside);
        self.rebuild("Insert Rows", tg.table, rows, grid, (pos, tg.cols.0), fonts)
    }

    /// Insert Columns Left / Right of the target: one empty grid column per column it covers, as wide as the column
    /// beside it, in every row (a cell spanning across the insertion point widens).
    pub(super) fn insert_columns(&mut self, right: bool, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        // One new column per selected cell of the target's first row, as wide as that cell (from a cell
        // spanning two columns, one column as wide as both).
        let widths: Vec<f64> = g.rows[tg.rows.0]
            .iter()
            .filter(|&&(c, s, _)| c + s > tg.cols.0 && c <= tg.cols.1)
            .map(|&(c, s, _)| (c..c + s).filter_map(|k| t.grid.get(k)).sum::<f64>())
            .collect();
        let count = widths.len().max(1);
        let at = if right { tg.cols.1 + 1 } else { tg.cols.0 };
        let width = widths.first().copied().unwrap_or(72.0);
        let mut grid = t.grid.clone();
        for (k, w) in widths.iter().enumerate() {
            grid.insert((at + k).min(grid.len()), *w);
        }
        let mut rows = t.rows.clone();
        for (r, row) in rows.iter_mut().enumerate() {
            row.src = None;
            if row.grid_before > 0 && at <= row.grid_before {
                row.grid_before += count;
                continue;
            }
            // The new cells go where the insertion point is; inside a spanning cell, before it: it keeps its span and
            // moves right.
            let cells = &g.rows[r];
            {
                {
                    let k = cells.iter().position(|&(c, s, _)| c >= at || (at > c && at < c + s)).unwrap_or(cells.len());
                    let like = &row.cells[if k == 0 { 0 } else { k - 1 }.min(row.cells.len().saturating_sub(1))];
                    let mut new = empty_cell(like);
                    new.span = 1;
                    new.width = Some(crate::docx::props::Width::Dxa(width));
                    for _ in 0..count {
                        row.cells.insert(k, new.clone());
                    }
                    for c in row.cells.iter_mut().skip(k).take(count) {
                        *c = Cell { blocks: empty_cell(c).blocks, ..c.clone() };
                    }
                }
            }
        }
        self.rebuild("Insert Columns", tg.table, rows, grid, (tg.rows.0, at), fonts)
    }

    /// Delete Rows: the target's rows go; a vertical merge starting in a deleted row starts, empty, in the next kept
    /// row: the merged content goes with the row. All rows: the table goes.
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
        // The caret goes to the first cell of the row that took the deleted rows' place.
        let at = tg.rows.0.min(rows.len().saturating_sub(1));
        let first = rows.get(at).map_or(0, |r| r.grid_before);
        self.rebuild("Delete Rows", tg.table, rows, grid, (at, first), fonts)
    }

    /// Delete Columns: the grid columns the target covers go; a cell spanning more narrows. All columns: the table.
    pub(super) fn delete_columns(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        if tg.cols.0 == 0 && tg.cols.1 + 1 >= g.cols {
            return self.delete_table(fonts);
        }
        let (c0, c1) = tg.cols;
        let mut rows: Vec<Row> = Vec::new();
        for (r, row) in t.rows.iter().enumerate() {
            let mut row = row.clone();
            row.src = None;
            let cut = |a: usize, b: usize| b.min(c1 + 1).saturating_sub(a.max(c0));
            row.grid_before -= cut(0, row.grid_before);
            // The trailing gap loses the deleted columns it covers too.
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
        let mut grid = t.grid.clone();
        grid.drain(c0..=c1.min(grid.len().saturating_sub(1)));
        self.rebuild("Delete Columns", tg.table, rows, grid, (tg.rows.0, c0.saturating_sub(1)), fonts)
    }

    /// Delete Table: the innermost table at the selection goes; the caret goes to what followed it.
    pub(super) fn delete_table(&mut self, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        let Some(slot) = self.index.table_slot(tg.table) else { return Outcome::Unchanged("no table") };
        let story = self.index.story(slot.container);
        let mut tx = self.begin("Delete Table");
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: slot.block..slot.block + 1, with: Vec::new() }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("delete failed");
        }
        // The block that took the table's place (a container always ends with a paragraph).
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

    /// Merge Cells: the target rectangle becomes one cell, its content every merged cell's in reading order.
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
        // The rectangle must not cut a cell: each row's cells in it start and end on its edges.
        for r in tg.rows.0..=tg.rows.1 {
            for &(c, s, _) in &g.rows[r] {
                let inside = c >= c0 && c + s - 1 <= c1;
                let outside = c + s - 1 < c0 || c > c1;
                if !inside && !outside {
                    return Outcome::Unchanged("the selection cuts a merged cell");
                }
            }
        }
        let mut content: Vec<Block> = Vec::new();
        let mut rows = t.rows.clone();
        for r in tg.rows.0..=tg.rows.1 {
            let row = &mut rows[r];
            row.src = None;
            let ks: Vec<usize> = (0..row.cells.len()).filter(|&k| g.rows[r][k].0 >= c0 && g.rows[r][k].0 <= c1).collect();
            let Some(&first) = ks.first() else { continue };
            for &k in &ks {
                let cell = &t.rows[r].cells[k];
                let empty = cell.blocks.len() == 1 && matches!(&cell.blocks[0], Block::Para(p) if para_len(p) == Unit(0));
                if !(empty || g.continues(r, k)) {
                    content.extend(cell.blocks.iter().cloned());
                }
            }
            let width: f64 = (c0..=c1).filter_map(|c| t.grid.get(c)).sum();
            let mut merged = row.cells[first].clone();
            // The merged cell keeps the first cell-level controls among those merged.
            merged.controls = ks.iter().map(|&k| &t.rows[r].cells[k].controls).find(|c| !c.is_empty()).cloned().unwrap_or_default();
            merged.span = c1 - c0 + 1;
            merged.width = Some(crate::docx::props::Width::Dxa(width));
            merged.v_merge = if tg.rows.0 == tg.rows.1 {
                VMerge::None
            } else if r == tg.rows.0 {
                VMerge::Restart
            } else {
                VMerge::Continue
            };
            merged.blocks = empty_cell(&merged).blocks;
            row.cells.splice(first..first + ks.len(), std::iter::once(merged));
        }
        if content.is_empty() {
            content = empty_cell(&t.rows[tg.rows.0].cells[0]).blocks;
        }
        // A merged cell's content ends with a paragraph (invariant 1).
        if matches!(content.last(), Some(Block::Table(_))) {
            content.extend(empty_cell(&t.rows[tg.rows.0].cells[0]).blocks);
        }
        let k =
            (0..rows[tg.rows.0].cells.len()).find(|&k| Grid::of(&Table { rows: rows.clone(), ..t.clone() }).rows[tg.rows.0][k].0 == c0).unwrap_or(0);
        rows[tg.rows.0].cells[k].blocks = content;
        let grid = t.grid.clone();
        self.rebuild("Merge Cells", tg.table, rows, grid, (tg.rows.0, c0), fonts)
    }

    /// Split Cells into `cols` columns (one row): the cell's span is divided, or grid columns are added inside it
    /// (other rows' cells over them widen to keep their shape). The content stays in the first new cell.
    pub(super) fn split_cell(&mut self, cols: usize, fonts: &Fonts) -> Outcome {
        let Some(tg) = self.table_target() else { return Outcome::Unchanged("not in a table") };
        if cols < 2 {
            return Outcome::Unchanged("nothing to split");
        }
        let Some(t) = self.index.table(&self.doc, tg.table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
        let r = tg.rows.0;
        let Some(k) = g.cell_at(r, tg.cols.0) else { return Outcome::Unchanged("no cell") };
        let (c, s, _) = g.rows[r][k];
        let mut grid = t.grid.clone();
        let mut rows = t.rows.clone();
        let added = cols.saturating_sub(s);
        if added > 0 {
            // New grid columns share the cell's last column's width.
            let last = c + s - 1;
            let w = grid.get(last).copied().unwrap_or(72.0) / (added + 1) as f64;
            grid[last] = w;
            for _ in 0..added {
                grid.insert(last + 1, w);
            }
            for (ri, row) in rows.iter_mut().enumerate() {
                if ri == r {
                    continue;
                }
                if let Some(kk) = g.cell_at(ri, last) {
                    row.cells[kk].span += added;
                } else if last < row.grid_before {
                    row.grid_before += added;
                } else {
                    row.grid_after += added;
                }
            }
        }
        let total = s + added;
        let row = &mut rows[r];
        let first = row.cells[k].clone();
        let mut new: Vec<Cell> = Vec::new();
        for i in 0..cols {
            let span = total / cols + usize::from(i < total % cols);
            let mut cell = if i == 0 { first.clone() } else { empty_cell(&first) };
            cell.span = span;
            cell.width = None;
            new.push(cell);
        }
        row.cells.splice(k..k + 1, new);
        rows.iter_mut().for_each(|r| r.src = None);
        self.rebuild("Split Cells", tg.table, rows, grid, (r, c), fonts)
    }

    /// Insert Table (rows × cols) at the caret: before the caret's paragraph when the caret is at its start, else after
    /// splitting it there; in the "Table Grid" style when the document has it, columns sharing the text width.
    pub(super) fn insert_table(&mut self, nrows: usize, ncols: usize, fonts: &Fonts) -> Outcome {
        let Some(caret) = self.caret() else { return Outcome::Unchanged("no caret") };
        let Some((para, at)) = caret.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        let (nrows, ncols) = (nrows.clamp(1, 63), ncols.clamp(1, 63));
        let Some(slot) = self.index.para_slot(para) else { return Outcome::Unchanged("no paragraph") };
        let Some(p) = self.index.para(&self.doc, para).cloned() else { return Outcome::Unchanged("no paragraph") };
        let width = self.text_width(slot.container);
        let cell_para = || Paragraph {
            style: p.style.clone(),
            mark: p.mark.clone(),
            list: None,
            inlines: vec![Inline::Text(String::new(), p.mark.clone())],
            src: None,
            id: crate::docx::next_para_id(),
            controls: Vec::new(),
        };
        let border = Some(crate::docx::props::Border { style: "single".into(), width: 0.5, space: 0.0, color: None });
        let grid_style = self.doc.resolver.as_ref().is_some_and(|r| r.styles.by_id.contains_key("TableGrid"));
        let cell = |w: f64| Cell {
            width: Some(crate::docx::props::Width::Dxa(w)),
            span: 1,
            v_merge: VMerge::None,
            top: None,
            left: None,
            bottom: None,
            right: None,
            shading: None,
            margins: [0.0, 5.4, 0.0, 5.4],
            v_align: "top".into(),
            text_direction: "lrTb".into(),
            no_wrap: false,
            direct: Default::default(),
            blocks: vec![Block::Para(cell_para())],
            controls: Vec::new(),
        };
        // Whole twips, the file's precision.
        let w = (width / ncols as f64 * 20.0).floor() / 20.0;
        let table = Table {
            style_id: grid_style.then(|| "TableGrid".to_string()),
            width: crate::docx::props::Width::Auto,
            jc: crate::docx::props::Jc::Left,
            ind: 0.0,
            borders: if grid_style {
                Default::default()
            } else {
                [border.clone(), border.clone(), border.clone(), border.clone(), border.clone(), border]
            },
            fixed: false,
            cell_spacing: 0.0,
            look: None,
            bands: (0, 0),
            floating: false,
            grid: vec![w; ncols],
            rows: (0..nrows)
                .map(|_| Row {
                    height: None,
                    cant_split: false,
                    header: false,
                    grid_before: 0,
                    grid_after: 0,
                    w_before: 0.0,
                    w_after: 0.0,
                    cells: (0..ncols).map(|_| cell(w)).collect(),
                    src: None,
                    controls: Vec::new(),
                })
                .collect(),
            src: None,
            id: crate::docx::next_para_id(),
            direct: Default::default(),
            // A table put inside a block-level control is in it too.
            controls: p.controls.clone(),
        };
        let first = first_para(&table.rows[0].cells[0].blocks);
        let tid = TableId(table.id);
        let mut tx = self.begin("Insert Table");
        let len = para_len(&p);
        let followed = self.index.blocks(&self.doc, slot.container).is_some_and(|b| slot.block + 1 < b.len());
        let place = if at == Unit(0) {
            slot.block
        } else if at == len && followed {
            // At a paragraph's end with something after it: the table goes right after the paragraph.
            slot.block + 1
        } else {
            // Split the paragraph at the caret; the table goes between the halves.
            let Some(second) = self.split_template(para) else { return Outcome::Unchanged("no paragraph") };
            if self.step(&mut tx, Step::Split { para, at, second }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("split failed");
            }
            slot.block + 1
        };
        if self.step(&mut tx, Step::Blocks { container: slot.container, range: place..place, with: vec![Block::Table(table)] }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("insert failed");
        }
        tx.stale_tables.push(tid);
        if let Some(fp) = first {
            self.selection = Selection::Caret(Position::text(caret.story, fp, Unit(0)));
        }
        self.commit(tx, None, fonts)
    }

    /// The width text has in a container: the section's text column, or a cell's content width.
    pub(super) fn text_width(&self, container: Container) -> f64 {
        match container {
            Container::Cell { table, row, cell } => self
                .index
                .table(&self.doc, table)
                .and_then(|t| {
                    let g = Grid::of(t);
                    let (c, s, _) = *g.rows.get(row)?.get(cell)?;
                    let m = t.rows[row].cells[cell].margins;
                    Some((c..c + s).filter_map(|k| t.grid.get(k)).sum::<f64>() - m[1] - m[3])
                })
                .unwrap_or(144.0),
            _ => self.doc.sections.first().map_or(468.0, |s| s.page_w - s.left - s.right),
        }
    }

    /// Select Cell, Row, Column: a cell block over the target's cells, its rows' whole width, or its columns' height.
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

    /// Ctrl+Shift+Enter in a table: the table splits above the caret's row, an empty paragraph between the
    /// two holding the caret; in the first row, an empty paragraph goes above the table instead.
    pub(super) fn split_table(&mut self, fonts: &Fonts) -> Outcome {
        let Some(focus) = self.caret() else { return Outcome::Unchanged("no caret") };
        let Some(level) = self.index.chain_of(&focus).last().copied() else { return Outcome::Unchanged("not in a table") };
        let Some(t) = self.index.table(&self.doc, level.table).cloned() else { return Outcome::Unchanged("no table") };
        // A region with rows on both sides of the split would be cut in two.
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
        // A merge continuing across the split starts again in the lower table.
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

    /// Delete (`clear`) and Backspace on a cell block: Delete empties the cells; Backspace deletes the whole
    /// table, whole rows or whole columns, and on any other block would open the Delete Cells dialog.
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
        let mut rows = t.rows.clone();
        let mut first = None;
        for (r, row) in rows.iter_mut().enumerate().filter(|(r, _)| (r0..=r1).contains(r)) {
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let (c, s, _) = g.rows[r][k];
                if c + s > c0 && c <= c1 {
                    *cell = Cell { blocks: empty_cell(cell).blocks, ..cell.clone() };
                    first.get_or_insert((r, c));
                }
            }
        }
        let grid = t.grid.clone();
        let (r, c) = first.unwrap_or((r0, c0));
        Some(self.rebuild("Clear", table, rows, grid, (r, c), fonts))
    }

    /// Shift+Alt+Home/End (`row`) and Shift+Alt+Page Up/Down: a cell block from the current cell (or the
    /// block's anchor) to the row's first or last cell, or the column's top or bottom cell.
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

    /// A cell block over the whole innermost table at the selection (Select Table).
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

/// Whether rows `r0..=r1` of `t` cut across a row-level control (a repeating section or item): one holds rows
/// both in the range and outside it, without the range taking all of it. With `within`, any control the range takes
/// rows of and that doesn't hold the whole range counts (a vertical merge stays inside one item).
pub(super) fn cuts_region(t: &Table, r0: usize, r1: usize, within: bool) -> bool {
    let mut ids: Vec<crate::docx::ControlId> = t.rows.iter().flat_map(|r| r.controls.iter().copied()).collect();
    ids.sort_unstable();
    ids.dedup();
    ids.into_iter().any(|c| {
        let rows: Vec<usize> = t.rows.iter().enumerate().filter(|(_, r)| r.controls.contains(&c)).map(|(k, _)| k).collect();
        let inside = rows.iter().any(|&k| r0 <= k && k <= r1);
        let all_in = rows.iter().all(|&k| r0 <= k && k <= r1);
        let covers = (r0..=r1).all(|k| rows.contains(&k));
        inside && !covers && (within || !all_in)
    })
}

impl Editor {
    /// A table edit refused because it cuts a region: answered `"region boundary"`.
    pub(super) fn region_boundary(&mut self) -> Outcome {
        self.answer = Some(serde_json::json!({ "error": "region boundary" }));
        Outcome::Unchanged("region boundary")
    }
}
