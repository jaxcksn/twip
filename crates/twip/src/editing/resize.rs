//! Table border drags

use std::rc::Rc;

use crate::docx::props::Width;
use crate::font::Fonts;
use crate::layout::TableId;

use super::Editor;
use super::commands::Outcome;
use super::index::Container;
use super::input::Request;
use super::selection::Selection;
use super::step::Step;
use super::stories::reachable;
use super::table::Grid;

/// The longest row or column a resize makes, 22 in.
const MAX: f64 = 1584.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Border {
    Column(usize),
    Row(usize),
    Handle,
}

#[derive(Clone, Debug)]
pub(super) struct BorderDrag {
    pub table: TableId,
    pub border: Border,
    pub start: (f64, f64),
    pub now: (f64, f64),
    pub page: usize,
    pub shift: bool,
    pub ctrl: bool,
    pub at: f64,
    pub row_top: f64,
    pub row: usize,
    pub span: [f64; 2],
    pub snapping: super::snap::Snapping,
    pub base: Rc<BorderBase>,
    /// A step changed the document since `base` was measured.
    pub stale: bool,
}

/// The table as drawn when a border drag starts: what the drag resizes from, however the table is laid out as it goes.
#[derive(Clone, Debug, Default)]
pub(super) struct BorderBase {
    /// The table's columns.
    grid: Vec<f64>,
    /// The columns of the table holding it.
    outer: Vec<f64>,
    /// The table's rows on the drag's page, each with its height.
    rows: Vec<(usize, f64)>,
    /// The dragged row's height on the pages before.
    before: f64,
}

/// A table laid out as a live drag would leave it, in place of the model's: see [`Editor::set_table_preview`].
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Provisional {
    pub id: TableId,
    /// The one row that changes, when only one does.
    pub row: Option<usize>,
    pub table: crate::docx::Table,
}

impl BorderDrag {
    pub fn guide(&self) -> [f64; 4] {
        match self.border {
            Border::Column(_) => {
                let x = self.at + self.now.0 - self.start.0;
                [x, self.span[0], x, self.span[1]]
            }
            Border::Row(_) => {
                let y = (self.at + self.now.1 - self.start.1).max(self.row_top + 1.0);
                [self.span[0], y, self.span[1], y]
            }
            Border::Handle => {
                let x = (self.at + self.now.0 - self.start.0).max(self.span[0] + 1.0);
                let y = (self.span[1] + self.now.1 - self.start.1).max(self.row_top + 1.0);
                [x, self.row_top, x, y]
            }
        }
    }
}

/// The caret's table as the rulers show it: see [`Editor::table_edges`].
pub(crate) struct TableEdges {
    pub page: usize,
    pub table: TableId,
    pub row: usize,
    /// The caret's cell's text, left to right.
    pub text: Option<[f64; 2]>,
    pub columns: Vec<f64>,
    /// Top to bottom; a row's bottom names the row and its top on the page.
    pub rows: Vec<(f64, Option<(usize, f64)>)>,
}

impl Editor {
    pub(super) fn border_press(&mut self, x: f64, y: f64, shift: bool, ctrl: bool, fonts: &Fonts) -> bool {
        let Some((page, table, border, at, row_top, span, row)) = self.border_at(x, y, fonts) else { return false };
        if self.table_gesture_kept(table, crate::api::command::CommandId::SetColumnWidth) {
            return false;
        }
        self.drag = None;
        self.border_cancel();
        let snapping = super::snap::Snapping::On;
        let base = Rc::new(self.border_base(
            table,
            match border {
                Border::Row(r) => Some(r),
                _ => None,
            },
            page,
            fonts,
        ));
        self.border_drag =
            Some(BorderDrag { table, border, start: (x, y), now: (x, y), page, shift, ctrl, at, row_top, span, row, snapping, base, stale: false });
        self.requests.push(Request::CapturePointer);
        true
    }

    #[allow(clippy::type_complexity)]
    pub(super) fn border_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(usize, TableId, Border, f64, f64, [f64; 2], usize)> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let tol = 2.0 * 0.75 / self.viewport.zoom.max(0.1);
        let mut rows: Vec<&super::geometry::GRow> = geo
            .rows
            .iter()
            .filter(|r| !r.repeat && reachable(ctx, geo.stories[r.story].host) && r.rect[0] - tol <= px && px <= r.rect[2] + tol)
            .collect();
        rows.sort_by_key(|r| std::cmp::Reverse(r.outer.len()));
        let table_span = |t: TableId| {
            let (top, bottom) = geo.rows.iter().filter(|r| r.table == t).fold((f64::MAX, f64::MIN), |(a, b), r| (a.min(r.rect[1]), b.max(r.rect[3])));
            [top, bottom]
        };
        let found = rows.iter().find_map(|r| {
            if py < r.rect[1] || py > r.rect[3] {
                return None;
            }
            let &(ex, k) = r.edges.iter().filter(|(_, k)| *k > 0).find(|(ex, _)| (px - ex).abs() <= tol)?;
            Some((r.table, Border::Column(k), ex, r.rect[1], table_span(r.table), r.row))
        });
        // Row borders
        let px1 = tol / 2.0;
        let last_row = |r: &super::geometry::GRow| !geo.rows.iter().any(|o| o.table == r.table && o.row > r.row);
        let row_hit = |r: &super::geometry::GRow| {
            let b = r.rect[3];
            py >= b - 0.5 * px1 && py <= b + 3.0 * px1
        };
        let above = rows.iter().find(|r| {
            let b = r.rect[3];
            !r.outer.is_empty() && last_row(r) && py < b - 0.5 * px1 && py >= b - 3.0 * px1
        });
        let found = found.or_else(|| {
            above
                .or_else(|| rows.iter().rev().find(|r| row_hit(r)))
                .map(|r| (r.table, Border::Row(r.row), r.rect[3], r.rect[1], [r.rect[0], r.rect[2]], r.row))
        });
        // Resize handle
        let held = self.held_tables();
        let found = found.or_else(|| {
            geo.rows
                .iter()
                .filter(|r| !r.repeat && reachable(ctx, geo.stories[r.story].host))
                .filter(|r| r.outer.is_empty() || held.contains(&r.table))
                .filter(|r| !geo.rows.iter().any(|o| o.table == r.table && o.row > r.row))
                .find(|r| px >= r.rect[2] && px <= r.rect[2] + 4.0 * tol && py >= r.rect[3] && py <= r.rect[3] + 4.0 * tol)
                .map(|r| {
                    let (left, top) =
                        geo.rows.iter().filter(|o| o.table == r.table).fold((f64::MAX, f64::MAX), |(a, b), o| (a.min(o.rect[0]), b.min(o.rect[1])));
                    (r.table, Border::Handle, r.rect[2], top, [left, r.rect[3]], r.row)
                })
        });
        found.map(|(table, border, at, row_top, span, row)| (page, table, border, at, row_top, span, row))
    }

    pub(super) fn held_tables(&self) -> Vec<TableId> {
        match self.selection {
            Selection::Cells { table, .. } => vec![table],
            s => s.focus().map(|p| self.index.chain_of(&p).iter().map(|l| l.table).collect()).unwrap_or_default(),
        }
    }

    /// The caret's table as the rulers show it on the caret's page: its row's cell boundaries (the grid's, from the
    /// outer border's edge), the caret's cell's text span, and the rows' boundaries, each with the row it ends.
    pub(crate) fn table_edges(&mut self) -> Option<TableEdges> {
        let caret = self.caret_view()?;
        let level = *self.index.chain_of(&self.selection.focus()?).last()?;
        let geo = self.page_geometry(caret.page)?;
        let t = self.index.table(&self.doc, level.table)?;
        let margins = t.rows.get(level.row)?.cells.get(level.cell).map(|c| c.margins);
        let text = geo
            .cells
            .iter()
            .find(|g| g.chain.last() == Some(&(level.table, level.row, level.cell)))
            .zip(margins)
            .map(|(cell, m)| [cell.rect[0] + m[1], cell.rect[2] - m[3]]);
        let mut cells: Vec<[f64; 4]> =
            geo.cells.iter().filter(|g| g.chain.last().is_some_and(|k| (k.0, k.1) == (level.table, level.row))).map(|g| g.rect).collect();
        cells.sort_by(|a, b| a[0].total_cmp(&b[0]));
        // Cells are laid out from the left border's middle; the boundaries are the grid's, from its outer edge.
        let border = t.rows.get(level.row).and_then(|r| r.cells.first()?.left.as_ref()).or(t.borders[1].as_ref());
        let shift = -border.filter(|b| b.visible()).map_or(0.0, |b| b.width / 2.0);
        let columns = cells.iter().map(|r| r[0]).chain(cells.last().map(|r| r[2])).map(|x| x + shift).collect();
        let shown: Vec<&super::geometry::GRow> = geo.rows.iter().filter(|r| r.table == level.table).collect();
        let mut rows: Vec<(f64, Option<(usize, f64)>)> =
            shown.iter().flat_map(|r| [(r.rect[1], None), (r.rect[3], (!r.repeat).then_some((r.row, r.rect[1])))]).collect();
        rows.sort_by(|a, b| a.0.total_cmp(&b.0).then(b.1.is_some().cmp(&a.1.is_some())));
        rows.dedup_by(|a, b| (a.0 - b.0).abs() < 0.01);
        Some(TableEdges { page: caret.page, table: level.table, row: level.row, text, columns, rows })
    }

    /// Moves boundary `index` of the caret's row (as [`TableEdges::columns`] counts them) to `to`: the columns either
    /// side of it share the change, the outer edges widen or narrow the table, the left one moving its indent.
    pub(super) fn move_table_column_boundary(&mut self, index: usize, to: f64, fonts: &Fonts) -> Outcome {
        let Some(edges) = self.table_edges() else { return Outcome::unavailable("not in a table") };
        let Some(&at) = edges.columns.get(index) else { return Outcome::invalid("no such column boundary") };
        let Some(t) = self.index.table(&self.doc, edges.table) else { return Outcome::unavailable("not in a table") };
        let g = Grid::of(t);
        let Some(cells) = g.rows.get(edges.row) else { return Outcome::unavailable("not in a table") };
        let k = match cells.get(index) {
            Some(&(col, _, _)) => col,
            None => cells.last().map_or(0, |&(col, span, _)| col + span.max(1)),
        };
        // The first column as drawn: the drag below resizes from it.
        let geo = self.pages.page(edges.page, &self.index);
        let first = drawn_grid(t, edges.table, geo.as_deref(), g.cols).first().copied().unwrap_or(0.0);
        let (dx, indent) = (to - at, t.ind);
        let base = Rc::new(self.border_base(edges.table, None, edges.page, fonts));
        let drag = |border, dx: f64| BorderDrag {
            table: edges.table,
            border,
            start: (0.0, 0.0),
            now: (dx, 0.0),
            page: edges.page,
            shift: true,
            ctrl: false,
            at,
            row_top: 0.0,
            row: edges.row,
            span: [0.0, 0.0],
            snapping: super::snap::Snapping::default(),
            base: base.clone(),
            stale: false,
        };
        if dx.abs() < 1e-9 {
            return Outcome::Unchanged("already there");
        }
        if k > 0 {
            let shift = k == g.cols;
            return self.apply_border_drag(&BorderDrag { shift, ..drag(Border::Column(k), dx) }, fonts);
        }
        // The left edge: the first column takes the change, the table's indent the move.
        self.joined("Column Width", fonts, |e| {
            let out = e.apply_border_drag(&drag(Border::Column(1), -dx), fonts);
            let now = e.index.table(&e.doc, edges.table).and_then(|t| t.grid.first().copied()).unwrap_or(first);
            if (now - first).abs() < 1e-9 {
                return out;
            }
            e.execute(super::commands::Command::SetTableIndent(crate::api::value::Points(indent + first - now)), fonts)
        })
    }

    /// Moves row boundary `index` of the caret's table on its page (as [`TableEdges::rows`] counts them) to `to`: the
    /// row above it takes the height.
    pub(super) fn move_table_row_boundary(&mut self, index: usize, to: f64, fonts: &Fonts) -> Outcome {
        let Some(edges) = self.table_edges() else { return Outcome::unavailable("not in a table") };
        let Some(&(at, ends)) = edges.rows.get(index) else { return Outcome::invalid("no such row boundary") };
        let Some((row, top)) = ends else { return Outcome::invalid("the table's top edge doesn't move") };
        if (to - at).abs() < 1e-9 {
            return Outcome::Unchanged("already there");
        }
        let base = Rc::new(self.border_base(edges.table, Some(row), edges.page, fonts));
        let d = BorderDrag {
            table: edges.table,
            border: Border::Row(row),
            start: (0.0, 0.0),
            now: (0.0, to - at),
            page: edges.page,
            shift: false,
            ctrl: false,
            at,
            row_top: top,
            row,
            span: [0.0, 0.0],
            snapping: super::snap::Snapping::default(),
            base,
            stale: false,
        };
        self.apply_border_drag(&d, fonts)
    }

    pub(super) fn border_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        self.border_remeasure(fonts);
        let d = self.border_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        self.set_table_preview(Vec::new());
        if d.now == d.start {
            return Some(Outcome::Unchanged("no drag"));
        }
        Some(self.apply_border_drag(&d, fonts))
    }

    /// A step changed the document during a border drag: one that changed the dragged table's rows or properties, or
    /// those of the table holding it, ends the drag; after any other, the drag resizes from the table as drawn next.
    pub(super) fn border_stepped(&mut self, s: &Step) {
        let Some(d) = self.border_drag.as_mut() else { return };
        d.stale = true;
        let table = d.table;
        let (Step::Rows { table: changed, .. } | Step::TableProps { table: changed, .. }) = s else { return };
        let holder = self.index.table_slot(table).and_then(|slot| match slot.container {
            Container::Cell { table, .. } => Some(table),
            _ => None,
        });
        if *changed == table || Some(*changed) == holder {
            self.border_cancel();
            self.requests.push(Request::ReleasePointer);
        }
    }

    /// Measures a border drag's table again after a step changed the document.
    fn border_remeasure(&mut self, fonts: &Fonts) {
        let Some((table, border, page)) = self.border_drag.as_ref().filter(|d| d.stale).map(|d| (d.table, d.border, d.page)) else { return };
        self.ensure_page(page, fonts);
        let base = Rc::new(self.border_base(
            table,
            match border {
                Border::Row(r) => Some(r),
                _ => None,
            },
            page,
            fonts,
        ));
        if let Some(d) = self.border_drag.as_mut() {
            (d.base, d.stale) = (base, false);
        }
    }

    /// Ends a border drag where it started: its live layout goes.
    pub(super) fn border_cancel(&mut self) -> bool {
        let Some(_) = self.border_drag.take() else { return false };
        self.set_table_preview(Vec::new());
        true
    }

    /// A border drag moved: with live layout, the tables are laid out as the drag would leave them.
    pub(super) fn border_moved(&mut self, fonts: &Fonts) {
        if !self.live_layout {
            self.set_table_preview(Vec::new());
            return;
        }
        self.border_remeasure(fonts);
        let Some(d) = self.border_drag.as_ref().filter(|d| d.border != Border::Handle) else { return };
        let tables = if d.now == d.start { Ok(Vec::new()) } else { self.border_steps(d).map(|(_, steps)| self.provisional(steps)) };
        self.set_table_preview(tables.unwrap_or_default());
    }

    /// What a drag of `table`'s borders resizes from on `page`: its columns as drawn and those of the table holding
    /// it, its rows' heights there, and for row `row`'s border, that row's height on the pages before.
    fn border_base(&mut self, table: TableId, row: Option<usize>, page: usize, fonts: &Fonts) -> BorderBase {
        let mut base = BorderBase::default();
        if let Some(r) = row {
            for p in 0..page {
                self.ensure_page(p, fonts);
                if let Some(geo) = self.pages.page(p, &self.index) {
                    base.before +=
                        geo.rows.iter().filter(|x| x.table == table && x.row == r && !x.repeat).map(|x| x.rect[3] - x.rect[1]).sum::<f64>();
                }
            }
            return base;
        }
        let geo = self.pages.page(page, &self.index);
        let Some(t) = self.index.table(&self.doc, table) else { return base };
        base.grid = drawn_grid(t, table, geo.as_deref(), Grid::of(t).cols);
        if let Some(geo) = &geo {
            base.rows = geo.rows.iter().filter(|r| r.table == table && !r.repeat).map(|r| (r.row, r.rect[3] - r.rect[1])).collect();
        }
        if let Some(slot) = self.index.table_slot(table)
            && let Container::Cell { table: outer, .. } = slot.container
            && let Some(o) = self.index.table(&self.doc, outer)
        {
            base.outer = drawn_grid(o, outer, geo.as_deref(), Grid::of(o).cols);
        }
        base
    }

    /// Resizes as a border drag ending where `d` is.
    fn apply_border_drag(&mut self, d: &BorderDrag, fonts: &Fonts) -> Outcome {
        let (label, steps) = match self.border_steps(d) {
            Ok(edit) => edit,
            Err(why) => return why,
        };
        let mut tx = self.begin(label);
        for (i, mut s) in steps.into_iter().enumerate() {
            // Table holding the resized one
            if let Step::Rows { table, with, .. } = &mut s
                && *table != d.table
            {
                self.put_held(d.table, with);
            }
            if self.step(&mut tx, s).is_err() && i == 0 {
                self.abort(tx);
                return STALE;
            }
        }
        self.commit(tx, None, fonts)
    }

    /// Puts the model's table `inner` into the cell of `rows` (a copy of the rows holding it) that holds it, so a step
    /// with `rows` keeps what the steps before did to `inner`, tracked changes included.
    fn put_held(&self, inner: TableId, rows: &mut [crate::docx::Row]) {
        let Some(slot) = self.index.table_slot(inner) else { return };
        let Container::Cell { row, cell, .. } = slot.container else { return };
        let Some(t) = self.index.table(&self.doc, inner) else { return };
        if let Some(b) = rows.get_mut(row).and_then(|r| r.cells.get_mut(cell)).and_then(|c| c.blocks.get_mut(slot.block)) {
            *b = crate::docx::Block::Table(t.clone());
        }
    }

    /// The steps a border drag ending where `d` is makes, and their transaction's label.
    fn border_steps(&self, d: &BorderDrag) -> Result<(&'static str, Vec<Step>), Outcome> {
        if let Border::Row(r) = d.border {
            return self.row_drag_steps(d, r);
        }
        let t = self.index.table(&self.doc, d.table).ok_or(STALE)?;
        let old_sum: f64 = t.grid.iter().sum();
        let (n, sized) = (t.rows.len(), !matches!(t.width, Width::Auto | Width::Nil));
        let mut rows = t.rows.clone();
        let cx = self.cx();
        let grid_of = Grid::of(t);
        let g = &grid_of;
        let mut grid = d.base.grid.clone();
        if grid.is_empty() {
            return Err(STALE);
        }
        let selected = match self.selection {
            super::selection::Selection::Cells { table, anchor, focus, .. } if table == d.table => {
                Some(anchor.row.min(focus.row)..=anchor.row.max(focus.row))
            }
            _ => None,
        };
        match d.border {
            Border::Column(k) if k < grid.len() && selected.as_ref().is_some_and(|s| s.contains(&d.row)) => {
                let sel = selected.unwrap_or(0..=0);
                let x: f64 = grid[..k].iter().sum();
                let total: f64 = grid.iter().sum();
                let dx = d.now.0 - d.start.0;
                let mut bounds: Vec<Vec<f64>> = Vec::new();
                for (r, row) in t.rows.iter().enumerate() {
                    let mut b = vec![grid[..row.grid_before.min(grid.len())].iter().sum::<f64>()];
                    for i in 0..row.cells.len() {
                        let (col, span, _) = g.rows[r][i];
                        b.push(grid[..(col + span).min(grid.len())].iter().sum());
                    }
                    if sel.contains(&r)
                        && let Some(j) = (1..b.len().saturating_sub(1)).find(|&j| (b[j] - x).abs() < 0.01)
                    {
                        let m = |i: usize| row.cells.get(i).map_or(4.5, |c| c.margins[1] + c.margins[3] + 4.5);
                        let (lo, hi) = (b[j - 1] + m(j - 1), b[j + 1] - m(j));
                        if lo <= hi {
                            b[j] = (b[j] + dx).clamp(lo, hi);
                        }
                    }
                    bounds.push(b);
                }
                let mut all: Vec<f64> = bounds.iter().flatten().copied().chain([0.0, total]).collect();
                all.sort_by(f64::total_cmp);
                all.dedup_by(|a, b| (*a - *b).abs() < 0.05);
                let at = |v: f64| all.iter().position(|a| (a - v).abs() < 0.05).unwrap_or(0);
                grid = all.windows(2).map(|w| w[1] - w[0]).collect();
                for (row, b) in rows.iter_mut().zip(&bounds) {
                    row.grid_before = at(b[0]);
                    for (i, cell) in row.cells.iter_mut().enumerate() {
                        cell.span = at(b[i + 1]) - at(b[i]);
                        cell.width = Some(Width::Dxa(b[i + 1] - b[i]));
                    }
                    row.grid_after = grid.len().saturating_sub(at(*b.last().unwrap_or(&total)));
                }
            }
            Border::Column(k) => {
                let n = grid.len();
                if k == 0 || k > n {
                    return Err(Outcome::Unchanged("not a column border"));
                }
                let min = |c: usize| {
                    t.rows
                        .iter()
                        .enumerate()
                        .flat_map(|(r, row)| {
                            row.cells
                                .iter()
                                .enumerate()
                                .filter(move |(i, _)| g.rows[r].get(*i).is_some_and(|&(col, _, _)| col == c))
                                .map(|(_, cell)| cell)
                        })
                        .map(|cell| cell.margins[1] + cell.margins[3])
                        .fold(0.0, f64::max)
                        + 4.5
                };
                let dx = d.now.0 - d.start.0;
                let left = k - 1;
                if d.shift && !d.ctrl || k == n {
                    let grown = grid[left] + dx.max(min(left) - grid[left]);
                    grid[left] = grown.min(MAX.max(grid[left]));
                } else if d.ctrl {
                    let right: f64 = grid[k..].iter().sum();
                    let floor: f64 = (k..n).map(min).sum();
                    let dx = within(dx, min(left) - grid[left], right - floor);
                    grid[left] += dx;
                    let scale = (right - dx) / right;
                    grid[k..].iter_mut().for_each(|w| *w *= scale);
                } else {
                    let dx = within(dx, min(left) - grid[left], grid[k] - min(k));
                    grid[left] += dx;
                    grid[k] -= dx;
                }
                for (r, row) in rows.iter_mut().enumerate() {
                    for (i, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(&(col, span, _)) = g.rows[r].get(i) {
                            cell.width = Some(Width::Dxa(grid[col..(col + span).min(n)].iter().sum()));
                        }
                    }
                }
            }
            Border::Handle => {
                let total: f64 = grid.iter().sum();
                // A column with no width stays at none, whatever the scale.
                let floor = grid
                    .iter()
                    .enumerate()
                    .filter(|(_, w)| **w > 0.0)
                    .map(|(c, w)| {
                        let m = t
                            .rows
                            .iter()
                            .enumerate()
                            .flat_map(|(r, row)| {
                                row.cells
                                    .iter()
                                    .enumerate()
                                    .filter(move |(i, _)| g.rows[r].get(*i).is_some_and(|x| x.0 == c))
                                    .map(|(_, cell)| cell.margins[1] + cell.margins[3])
                            })
                            .fold(0.0, f64::max);
                        (m + 4.5) / w.max(0.01)
                    })
                    .fold(0.0, f64::max);
                let sx = ((total + d.now.0 - d.start.0) / total.max(0.01)).max(floor);
                grid.iter_mut().for_each(|w| *w *= sx);
                for (r, row) in rows.iter_mut().enumerate() {
                    for (i, cell) in row.cells.iter_mut().enumerate() {
                        if let Some(&(col, span, _)) = g.rows[r].get(i) {
                            cell.width = Some(Width::Dxa(grid[col..(col + span).min(grid.len())].iter().sum()));
                        }
                    }
                }
                let height = d.span[1] - d.row_top;
                let dy = d.now.1 - d.start.1;
                if dy.abs() > 0.5 && height > 1.0 {
                    let sy = ((height + dy) / height).max(0.05);
                    for &(r, h) in &d.base.rows {
                        let Some(row) = rows.get_mut(r) else { continue };
                        let rule = match &row.height {
                            Some((_, rule)) if rule == "exact" => "exact",
                            _ => "atLeast",
                        };
                        super::table_format::set_height(row, Some(((h * sy).min(MAX.max(h)).max(1.0), rule.to_string())), &cx);
                    }
                }
            }
            Border::Row(_) => return Err(STALE),
        }
        let new_sum: f64 = grid.iter().sum();
        let label = if d.border == Border::Handle { "Resize Table" } else { "Column Width" };
        let mut steps = vec![Step::Rows { table: d.table, range: 0..n, with: rows, grid: Some(grid) }];
        if (new_sum - old_sum).abs() > 1e-9 && sized {
            let mut props = super::step::TableProps::of(t);
            super::table_format::set_width(&mut props, Width::Dxa(new_sum), &cx);
            steps.push(Step::TableProps { table: d.table, props });
        }
        if new_sum > old_sum + 1e-9
            && let Some(slot) = self.index.table_slot(d.table)
            && let super::index::Container::Cell { table: outer, row, cell } = slot.container
            && let Some(o) = self.index.table(&self.doc, outer)
            && !o.fixed
            && !d.base.outer.is_empty()
        {
            let g = Grid::of(o);
            if let Some(&(col, span, _)) = g.rows.get(row).and_then(|r| r.get(cell)) {
                let c = &o.rows[row].cells[cell];
                let mut og = d.base.outer.clone();
                let last = (col + span.max(1) - 1).min(og.len().saturating_sub(1));
                let have: f64 = og.get(col..=last).map_or(0.0, |w| w.iter().sum());
                let need = new_sum + c.margins[1] + c.margins[3] + 0.5;
                if need > have + 1e-9
                    && let Some(w) = og.get_mut(last)
                {
                    let more = need - have;
                    *w += more;
                    if let Some(next) = og.get_mut(last + 1) {
                        *next = (*next - more).max(c.margins[1] + c.margins[3] + 4.5);
                    }
                    let mut orows = o.rows.clone();
                    for (r, orow) in orows.iter_mut().enumerate() {
                        for (i, oc) in orow.cells.iter_mut().enumerate() {
                            if let Some(&(c0, s0, _)) = g.rows[r].get(i) {
                                oc.width = Some(Width::Dxa(og[c0..(c0 + s0.max(1)).min(og.len())].iter().sum()));
                            }
                        }
                    }
                    // The cell holds the table as the steps before leave it.
                    let mut inner = t.clone();
                    for s in &steps {
                        match s.clone() {
                            Step::Rows { range, with, grid, .. } => _ = super::step::put_rows(&mut inner, range, with, grid),
                            Step::TableProps { props, .. } => _ = props.put(&mut inner),
                            _ => {}
                        }
                    }
                    if let Some(b) = orows.get_mut(row).and_then(|r| r.cells.get_mut(cell)).and_then(|c| c.blocks.get_mut(slot.block)) {
                        *b = crate::docx::Block::Table(inner);
                    }
                    let n = orows.len();
                    steps.push(Step::Rows { table: outer, range: 0..n, with: orows, grid: Some(og) });
                }
            }
        }
        Ok((label, steps))
    }

    /// A row border drag: row `r` alone takes the height, the table's columns and width as they are.
    fn row_drag_steps(&self, d: &BorderDrag, r: usize) -> Result<(&'static str, Vec<Step>), Outcome> {
        let h = (d.base.before + d.guide()[1] - d.row_top).clamp(1.0, MAX);
        let mut row = self.index.table(&self.doc, d.table).and_then(|t| t.rows.get(r)).ok_or(STALE)?.clone();
        let rule = match &row.height {
            Some((_, rule)) if rule == "exact" => "exact",
            _ => "atLeast",
        };
        super::table_format::set_height(&mut row, Some((h, rule.to_string())), &self.cx());
        Ok(("Row Height", vec![Step::Rows { table: d.table, range: r..r + 1, with: vec![row], grid: None }]))
    }

    /// The tables `steps` leave, outermost first, each a copy of the model's with the steps on it.
    fn provisional(&self, steps: Vec<Step>) -> Vec<Provisional> {
        let mut out: Vec<Provisional> = Vec::new();
        for s in steps {
            let (Step::Rows { table, .. } | Step::TableProps { table, .. }) = s else { continue };
            let k = match out.iter().position(|p| p.id == table) {
                Some(k) => k,
                None => {
                    let Some(t) = self.index.table(&self.doc, table) else { continue };
                    out.push(Provisional { id: table, row: None, table: t.clone() });
                    out.len() - 1
                }
            };
            let p = &mut out[k];
            match s {
                Step::Rows { range, with, grid, .. } if range.end <= p.table.rows.len() => {
                    p.row = (range.len() == 1 && with.len() == 1 && grid.is_none()).then_some(range.start);
                    super::step::put_rows(&mut p.table, range, with, grid);
                }
                Step::TableProps { props, .. } => {
                    p.row = None;
                    props.put(&mut p.table);
                }
                _ => {}
            }
        }
        // A nested table goes into the copy of the one holding it.
        out.reverse();
        out
    }
}

const STALE: Outcome = Outcome::Unchanged("stale table");

/// `t`'s grid widened to the `cols` columns its rows reach, each column as wide as `geo` draws it.
fn drawn_grid(t: &crate::docx::Table, id: TableId, geo: Option<&super::geometry::PageGeometry>, cols: usize) -> Vec<f64> {
    let mut grid = super::tables::full_grid(t, cols);
    if let Some(geo) = geo {
        super::tables::drawn_grid(geo, id, &mut grid);
    }
    grid
}

fn within(dx: f64, lo: f64, hi: f64) -> f64 {
    if lo <= hi { dx.clamp(lo, hi) } else { 0.0 }
}
