//! Table border drags

use crate::docx::props::Width;
use crate::font::Fonts;
use crate::layout::TableId;

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::selection::Selection;
use super::step::Step;
use super::stories::reachable;
use super::table::Grid;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Border {
    Column(usize),
    Row(usize),
    Handle,
}

#[derive(Clone, Copy, Debug)]
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

impl Editor {
    pub(super) fn border_press(&mut self, x: f64, y: f64, shift: bool, ctrl: bool, fonts: &Fonts) -> bool {
        let Some((page, table, border, at, row_top, span, row)) = self.border_at(x, y, fonts) else { return false };
        self.drag = None;
        let snapping = super::snap::Snapping::On;
        self.border_drag = Some(BorderDrag { table, border, start: (x, y), now: (x, y), page, shift, ctrl, at, row_top, span, row, snapping });
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

    pub(super) fn border_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.border_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        if d.now == d.start {
            return Some(Outcome::Unchanged("no drag"));
        }
        let t = self.index.table(&self.doc, d.table)?;
        let old_sum: f64 = t.grid.iter().sum();
        let (n, sized) = (t.rows.len(), !matches!(t.width, Width::Auto | Width::Nil));
        let mut rows = t.rows.clone();
        // Drawn column widths
        let mut grid = t.grid.clone();
        if let Some(geo) = self.pages.page(d.page, &self.index) {
            let mut xs: Vec<Option<f64>> = vec![None; grid.len() + 1];
            for r in geo.rows.iter().filter(|r| r.table == d.table && !r.repeat) {
                for &(x, k) in &r.edges {
                    if let Some(slot) = xs.get_mut(k)
                        && slot.is_none()
                    {
                        *slot = Some(x);
                    }
                }
            }
            for k in 0..grid.len() {
                if let (Some(a), Some(b)) = (xs[k], xs[k + 1]) {
                    grid[k] = ((b - a).max(0.0) * 20.0).round() / 20.0;
                }
            }
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
                let g = Grid::of(t);
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
                    return Some(Outcome::Unchanged("not a column border"));
                }
                let grid_of = Grid::of(t);
                let g = &grid_of;
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
                    grid[left] += dx.max(min(left) - grid[left]);
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
                let grid_of = Grid::of(t);
                let g = &grid_of;
                let floor = grid
                    .iter()
                    .enumerate()
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
                    let measured: Vec<(usize, f64)> = self.pages.page(d.page, &self.index).map_or_else(Vec::new, |geo| {
                        geo.rows.iter().filter(|r| r.table == d.table && !r.repeat).map(|r| (r.row, r.rect[3] - r.rect[1])).collect()
                    });
                    for (r, h) in measured {
                        let Some(row) = rows.get_mut(r) else { continue };
                        let rule = match &row.height {
                            Some((_, rule)) if rule == "exact" => "exact",
                            _ => "atLeast",
                        };
                        row.height = Some(((h * sy).max(1.0), rule.to_string()));
                    }
                }
            }
            Border::Row(r) => {
                let mut before = 0.0;
                for p in 0..d.page {
                    self.ensure_page(p, fonts);
                    if let Some(geo) = self.pages.page(p, &self.index) {
                        before +=
                            geo.rows.iter().filter(|x| x.table == d.table && x.row == r && !x.repeat).map(|x| x.rect[3] - x.rect[1]).sum::<f64>();
                    }
                }
                let h = (before + d.guide()[1] - d.row_top).max(1.0);
                let row = rows.get_mut(r)?;
                let rule = match &row.height {
                    Some((_, rule)) if rule == "exact" => "exact",
                    _ => "atLeast",
                };
                row.height = Some((h, rule.to_string()));
            }
        }
        let new_sum: f64 = grid.iter().sum();
        let mut tx = self.begin(match d.border {
            Border::Row(_) => "Row Height",
            Border::Handle => "Resize Table",
            Border::Column(_) => "Column Width",
        });
        let stepped =
            crate::prof::time("edit:rows-step", || self.step(&mut tx, Step::Rows { table: d.table, range: 0..n, with: rows, grid: Some(grid) }));
        if stepped.is_err() {
            self.abort(tx);
            return Some(Outcome::Unchanged("stale table"));
        }
        if (new_sum - old_sum).abs() > 1e-9 && sized {
            let _ = self.step(&mut tx, Step::TableWidth { table: d.table, width: Width::Dxa(new_sum) });
        }
        if new_sum > old_sum + 1e-9
            && let Some(slot) = self.index.table_slot(d.table)
            && let super::index::Container::Cell { table: outer, row, cell } = slot.container
            && let Some(o) = self.index.table(&self.doc, outer).cloned()
            && !o.fixed
        {
            let g = Grid::of(&o);
            if let Some(&(col, span, _)) = g.rows.get(row).and_then(|r| r.get(cell)) {
                let c = &o.rows[row].cells[cell];
                let last = (col + span.max(1) - 1).min(o.grid.len().saturating_sub(1));
                let have: f64 = o.grid[col..=last].iter().sum();
                let need = new_sum + c.margins[1] + c.margins[3] + 0.5;
                if need > have + 1e-9 {
                    let mut og = o.grid.clone();
                    let more = need - have;
                    og[last] += more;
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
                    let n = orows.len();
                    let _ = self.step(&mut tx, Step::Rows { table: outer, range: 0..n, with: orows, grid: Some(og) });
                }
            }
        }
        Some(self.commit(tx, None, fonts))
    }
}

fn within(dx: f64, lo: f64, hi: f64) -> f64 {
    if lo <= hi { dx.clamp(lo, hi) } else { 0.0 }
}
