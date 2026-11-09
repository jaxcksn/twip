//! Table structure commands

use crate::api::command::TablePosition;
use crate::docx::props::{Jc, TableAnchor, TablePos};
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

#[derive(Clone, Copy, Debug)]
pub(super) struct Target {
    pub table: TableId,
    pub rows: (usize, usize),
    pub cols: (usize, usize),
}

pub(super) fn empty_cell(like: &Cell) -> Cell {
    let mut c = like.clone();
    c.v_merge = VMerge::None;
    c.controls = Vec::new();
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
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::Unchanged("no table") };
        let g = Grid::of(t);
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
        let mut lines: std::collections::HashMap<crate::layout::ParaId, f64> = std::collections::HashMap::new();
        for (_, geo) in self.page_geometries() {
            for l in geo.lines.iter().filter(|l| !l.repeat) {
                for &p in l.paras.iter() {
                    let (x0, x1) = l.span_of(p).unwrap_or_default();
                    *lines.entry(p).or_default() += (x1 - x0).max(0.0);
                }
            }
        }
        let margins = paras.iter().map(|(_, m)| *m).fold(0.0, f64::max);
        let content = paras.iter().map(|(p, m)| lines.get(p).copied().unwrap_or(0.0) + m).fold(0.0, f64::max);
        let mut w = content.max(margins + 4.5);
        let old = old_w.unwrap_or(w);
        let avail = self.index.table_slot(table).map_or(468.0, |s| self.text_width(s.container)) + margins + 1.0;
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
        if matches!(content.last(), Some(Block::Table(_))) {
            content.extend(empty_cell(&t.rows[tg.rows.0].cells[0]).blocks);
        }
        let k =
            (0..rows[tg.rows.0].cells.len()).find(|&k| Grid::of(&Table { rows: rows.clone(), ..t.clone() }).rows[tg.rows.0][k].0 == c0).unwrap_or(0);
        rows[tg.rows.0].cells[k].blocks = content;
        let grid = t.grid.clone();
        self.rebuild("Merge Cells", tg.table, rows, grid, (tg.rows.0, c0), fonts)
    }

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
        let border = Some(crate::docx::props::Border { style: "single".into(), width: 0.5, space: 0.0, shadow: false, color: None });
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
            fit_text: false,
            hide_mark: false,
            direct: Default::default(),
            blocks: vec![Block::Para(cell_para())],
            controls: Vec::new(),
            rev: Default::default(),
        };
        let w = (width / ncols as f64 * 20.0).floor() / 20.0;
        let table = Table {
            style_id: grid_style.then(|| "TableGrid".to_string()),
            width: crate::docx::props::Width::Auto,
            jc: crate::docx::props::Jc::Left,
            ind: 0.0,
            ind_set: true,
            borders: if grid_style {
                Default::default()
            } else {
                [border.clone(), border.clone(), border.clone(), border.clone(), border.clone(), border]
            },
            fixed: false,
            cell_spacing: 0.0,
            look: None,
            bands: (0, 0),
            floating: None,
            bidi_visual: false,
            grid: vec![w; ncols],
            rows: (0..nrows)
                .map(|_| Row {
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
                    vanished: false,
                    cell_spacing: None,
                    run: None,
                    cells: (0..ncols).map(|_| cell(w)).collect(),
                    src: None,
                    controls: Vec::new(),
                    rev: Default::default(),
                })
                .collect(),
            src: None,
            id: crate::docx::next_para_id(),
            direct: Default::default(),
            controls: p.controls.clone(),
            change: None,
            grid_change: None,
        };
        let first = first_para(&table.rows[0].cells[0].blocks);
        let tid = TableId(table.id);
        let mut tx = self.begin("Insert Table");
        let len = para_len(&p);
        let followed = self.index.blocks(&self.doc, slot.container).is_some_and(|b| slot.block + 1 < b.len());
        let place = if at == Unit(0) {
            slot.block
        } else if at == len && followed {
            slot.block + 1
        } else {
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

    fn set_floating(&mut self, table: TableId, floating: Option<TablePos>, fonts: &Fonts) -> Outcome {
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
