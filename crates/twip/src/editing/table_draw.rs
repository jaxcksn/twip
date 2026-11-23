//! Draw Table and Eraser

use crate::docx::VMerge;
use crate::docx::props::{Border, Width};
use crate::font::Fonts;
use crate::layout::TableId;

use super::Editor;
use super::commands::Outcome;
use super::geometry::{GCell, PageGeometry};
use super::position::Position;
use super::position::StoryId;
use super::revisions::record::DRAW_TABLE;
use super::selection::{CellRef, CellShape, Selection};
use super::stories::reachable;
use super::table::Grid;
use super::table_format::{BOTTOM, DIAGONAL_DOWN, DIAGONAL_UP, LEFT, RIGHT, TOP, set_cell_sides, set_height};
use super::tables::{empty_cell, merge_chain};
use crate::api::command::CommandId;
use crate::layout::Unit;

/// A table tool's stroke on a page, in the page's points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    /// The page.
    pub page: usize,
    /// Where the stroke starts.
    pub from: [f64; 2],
    /// Where it ends.
    pub to: [f64; 2],
}

/// How near a border a tool has to touch, in points.
const REACH: f64 = 3.0;

/// The shortest stroke the pen draws, in points: a click or a jitter draws nothing.
const MIN_STROKE: f64 = 3.0;

/// The Draw Table pen's line until one is set: single, ½ pt, automatic colour.
pub(super) fn default_pen() -> Border {
    Border { style: "single".into(), width: 0.5, space: 0.0, shadow: false, color: None, color_theme: None }
}

/// Whether cell `c` is at page point `p`, or within [`REACH`] of it, in a story editing in `ctx` reaches.
fn touches(geo: &PageGeometry, ctx: StoryId, c: &GCell, p: [f64; 2]) -> bool {
    let [x, y] = c.turn.back(p);
    !c.chain.is_empty()
        && x >= c.rect[0] - REACH
        && x <= c.rect[2] + REACH
        && y >= c.rect[1] - REACH
        && y <= c.rect[3] + REACH
        && geo.stories.get(c.story).is_some_and(|s| reachable(ctx, s.host))
}

/// The innermost table cell at page point `p`, or within [`REACH`] of it, in a story editing in `ctx` reaches.
fn cell_near(geo: &PageGeometry, ctx: StoryId, p: [f64; 2]) -> Option<&GCell> {
    geo.cells.iter().filter(|c| touches(geo, ctx, c, p)).max_by_key(|c| c.chain.len())
}

/// Whether page point `p` is over a story editing in `ctx` doesn't reach (a header's while in the body, the body's
/// while in a header): the smallest story there.
fn over_other_story(geo: &PageGeometry, ctx: StoryId, p: [f64; 2]) -> bool {
    let area = |r: &[f64; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    geo.stories
        .iter()
        .filter(|s| {
            let [x, y] = s.turn.back(p);
            x >= s.rect[0] && x <= s.rect[2] && y >= s.rect[1] && y <= s.rect[3]
        })
        .min_by(|a, b| area(&a.rect).total_cmp(&area(&b.rect)))
        .is_some_and(|s| !reachable(ctx, s.host))
}

/// A table as drawn on a page: its rows there (model row, top, bottom) and where each grid edge is.
struct Drawn {
    rows: Vec<(usize, f64, f64)>,
    xs: Vec<f64>,
    widths: Vec<f64>,
}

impl Drawn {
    fn of(geo: &PageGeometry, table: TableId, t: &crate::docx::Table, cols: usize) -> Option<Drawn> {
        let mut widths = super::tables::full_grid(t, cols);
        super::tables::drawn_grid(geo, table, &mut widths);
        let mut rows: Vec<(usize, f64, f64)> =
            geo.rows.iter().filter(|r| r.table == table && !r.repeat).map(|r| (r.row, r.rect[1], r.rect[3])).collect();
        rows.sort_by_key(|r| r.0);
        let &(x, k) = geo.rows.iter().filter(|r| r.table == table && !r.repeat).flat_map(|r| r.edges.iter()).next()?;
        let origin = x - widths[..k.min(widths.len())].iter().sum::<f64>();
        let xs = std::iter::once(origin)
            .chain(widths.iter().scan(origin, |x, w| {
                *x += w;
                Some(*x)
            }))
            .collect();
        Some(Drawn { rows, xs, widths })
    }

    /// Where the cell over grid columns `c..c + s` starts and ends.
    fn span(&self, c: usize, s: usize) -> (f64, f64) {
        let at = |k: usize| self.xs.get(k).or(self.xs.last()).copied().unwrap_or(0.0);
        (at(c), at(c + s.max(1)))
    }
}

/// Every cell of the vertical merges `cells` are in, sorted.
fn whole_merges(g: &Grid, cells: &mut Vec<(usize, usize)>) {
    let mut all = Vec::with_capacity(cells.len());
    for &(r, k) in cells.iter() {
        if g.rows[r][k].2 == VMerge::None {
            all.push((r, k));
        } else {
            let (r0, k0) = g.merge_start(r, k);
            all.extend(merge_chain(g, r0, k0));
        }
    }
    all.sort_unstable();
    all.dedup();
    *cells = all;
}

impl Editor {
    /// `page` as laid out.
    fn laid_page(&mut self, page: usize, fonts: &Fonts) -> Option<std::rc::Rc<PageGeometry>> {
        self.ensure_page(page, fonts);
        self.pages.page(page, &self.index)
    }

    /// The line the Draw Table pen draws.
    pub fn table_pen(&self) -> &Border {
        &self.table_pen
    }

    /// Sets the line the Draw Table pen draws (Word's pen style, weight and colour).
    pub fn set_table_pen(&mut self, line: Border) {
        self.table_pen = line;
    }

    /// The Draw Table pen. A line across or down a table splits every cell it crosses there, a cell's corner to
    /// corner stroke draws its diagonal, and a line along a border redraws it: each in the pen's line. A stroke
    /// outside every table draws a new one.
    pub fn draw_table_stroke(&mut self, s: Stroke, fonts: &Fonts) -> Outcome {
        if (s.to[0] - s.from[0]).hypot(s.to[1] - s.from[1]) < MIN_STROKE {
            return Outcome::Unchanged("too short a stroke");
        }
        let mid = [(s.from[0] + s.to[0]) / 2.0, (s.from[1] + s.to[1]) / 2.0];
        let Some(geo) = self.laid_page(s.page, fonts) else { return Outcome::unavailable("no such page") };
        let ctx = self.editing();
        let Some(cell) = cell_near(&geo, ctx, mid) else {
            let corner = [s.from[0].min(s.to[0]), s.from[1].min(s.to[1])];
            if over_other_story(&geo, ctx, mid) || over_other_story(&geo, ctx, corner) {
                return Outcome::unavailable("not in the story being edited");
            }
            return self.draw_new_table(s, fonts);
        };
        let Some(&(table, ..)) = cell.chain.last() else { return Outcome::unavailable("draw in a table's cell") };
        if let Some(e) = self.table_edit_refusal(table, CommandId::SplitCells) {
            return Outcome::Refused(e);
        }
        let (depth, turn) = (cell.chain.len(), cell.turn);
        let (a, b) = (turn.back(s.from), turn.back(s.to));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        // Corner to corner in one cell: its diagonal
        let diagonal = geo.cells.iter().filter(|c| c.chain.len() == depth && c.chain.last().is_some_and(|k| k.0 == table)).find(|c| {
            let r = c.rect;
            let (tx, ty) = (REACH.max(0.25 * (r[2] - r[0])), REACH.max(0.25 * (r[3] - r[1])));
            let near = |p: [f64; 2], x: f64, y: f64| (p[0] - x).abs() <= tx && (p[1] - y).abs() <= ty;
            let opposite = |p: [f64; 2], q: [f64; 2]| (near(p, r[0], r[1]) && near(q, r[2], r[3])) || (near(p, r[2], r[1]) && near(q, r[0], r[3]));
            opposite(a, b) || opposite(b, a)
        });
        if let Some(&(_, row, k)) = diagonal.and_then(|c| c.chain.last()) {
            return self.draw_diagonal(table, (row, k), (dx > 0.0) == (dy > 0.0), fonts);
        }
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let g = Grid::of(t);
        let Some(drawn) = Drawn::of(&geo, table, t, g.cols) else { return Outcome::unavailable("not in a table") };
        if dx.abs() >= dy.abs() {
            self.draw_across(table, &drawn, (a[1] + b[1]) / 2.0, (a[0].min(b[0]), a[0].max(b[0])), s.page, fonts)
        } else {
            self.draw_down(table, &drawn, (a[0] + b[0]) / 2.0, (a[1].min(b[1]), a[1].max(b[1])), fonts)
        }
    }

    /// The pen's corner to corner stroke in cell `(row, k)`: its diagonal down (top left to bottom right) or up.
    fn draw_diagonal(&mut self, table: TableId, (row, k): (usize, usize), down: bool, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let (row, k) = Grid::of(t).merge_start(row, k);
        let mut rows = t.rows.clone();
        let (grid, cx) = (t.grid.clone(), self.cx());
        let Some(cell) = rows.get_mut(row).and_then(|r| r.cells.get_mut(k)) else { return Outcome::Unchanged("no cell there") };
        set_cell_sides(cell, if down { DIAGONAL_DOWN } else { DIAGONAL_UP }, &self.table_pen, &cx);
        let col = Grid::of(t).col_of(row, k).unwrap_or(0);
        self.rebuild(DRAW_TABLE, table, rows, grid, (row, col), fonts)
    }

    /// A pen line across the table at `y` from `xs.0` to `xs.1`: the cells it crosses split into two rows there, or
    /// along a row border, the border redrawn and any vertical merge over it split.
    fn draw_across(&mut self, table: TableId, d: &Drawn, y: f64, xs: (f64, f64), page: usize, fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let (g, mut rows, grid) = (Grid::of(t), t.rows.clone(), t.grid.clone());
        let over = |r: usize| {
            g.rows[r].iter().enumerate().filter(move |&(_, &(c, s, _))| {
                let (l, rr) = d.span(c, s);
                xs.1.min(rr) - xs.0.max(l) > REACH
            })
        };
        let pen = self.table_pen.clone();
        let cx = self.cx();
        let band = d.rows.iter().find(|&&(_, top, bottom)| y > top + REACH && y < bottom - REACH).copied();
        let Some((r, top, bottom)) = band else {
            // Along the border over row `b`
            let at = d.rows.iter().find_map(
                |&(r, top, bottom)| {
                    if (y - top).abs() <= REACH { Some(r) } else { ((y - bottom).abs() <= REACH).then_some(r + 1) }
                },
            );
            let Some(b) = at else { return Outcome::Unchanged("no cell there") };
            let (above, below): (Vec<usize>, Vec<usize>) = (
                b.checked_sub(1).map_or(Vec::new(), |r| over(r).map(|x| x.0).collect()),
                if b < rows.len() { over(b).map(|x| x.0).collect() } else { Vec::new() },
            );
            if above.is_empty() && below.is_empty() {
                return Outcome::Unchanged("no cell there");
            }
            for &k in &below {
                let cell = &mut rows[b].cells[k];
                if cell.v_merge == VMerge::Continue {
                    cell.v_merge = VMerge::Restart;
                }
                set_cell_sides(cell, TOP, &pen, &cx);
            }
            for &k in &above {
                set_cell_sides(&mut rows[b - 1].cells[k], BOTTOM, &pen, &cx);
            }
            let caret = (b.min(rows.len() - 1), g.rows[b.min(rows.len() - 1)].first().map_or(0, |x| x.0));
            return self.rebuild(DRAW_TABLE, table, rows, grid, caret, fonts);
        };
        let crossed: Vec<usize> = over(r).map(|x| x.0).collect();
        let Some(&first) = crossed.first() else { return Outcome::Unchanged("no cell there") };
        if rows.len() >= super::tables::MAX_ROWS {
            return Outcome::unavailable("too many rows");
        }
        // The rows' heights either side of the line, where the row has one
        let heights = rows[r].height.clone().map(|(_, rule)| {
            let (before, after) = self.row_beyond(table, r, page, fonts);
            ((before + y - top, rule.clone()), (bottom - y + after, rule))
        });
        let mut lower = rows[r].clone();
        lower.rev = Default::default();
        lower.src = None;
        rows[r].src = None;
        for (k, cell) in lower.cells.iter_mut().enumerate() {
            let upper = &mut rows[r].cells[k];
            let continued = g.cell_at(r + 1, g.rows[r][k].0).is_some_and(|b| g.rows[r + 1][b].0 == g.rows[r][k].0 && g.continues(r + 1, b));
            *cell = empty_cell(upper);
            if crossed.binary_search(&k).is_ok() {
                cell.v_merge = if upper.v_merge != VMerge::None && continued { VMerge::Restart } else { VMerge::None };
                set_cell_sides(upper, BOTTOM, &pen, &cx);
                set_cell_sides(cell, TOP, &pen, &cx);
            } else {
                cell.v_merge = VMerge::Continue;
                if upper.v_merge == VMerge::None {
                    upper.v_merge = VMerge::Restart;
                }
            }
        }
        if let Some((u, l)) = heights {
            set_height(&mut rows[r], Some(u), &cx);
            set_height(&mut lower, Some(l), &cx);
        }
        rows.insert(r + 1, lower);
        self.rebuild(DRAW_TABLE, table, rows, grid, (r, g.rows[r][first].0), fonts)
    }

    /// How much of row `r` is laid out on the pages before `page` and after it.
    fn row_beyond(&mut self, table: TableId, r: usize, page: usize, fonts: &Fonts) -> (f64, f64) {
        let n = self.pages.len();
        let mut on = |p: usize| {
            self.ensure_page(p, fonts);
            let geo = self.pages.page(p, &self.index)?;
            let mut parts = geo.rows.iter().filter(|x| x.table == table && x.row == r && !x.repeat).peekable();
            parts.peek()?;
            Some(parts.map(|x| x.rect[3] - x.rect[1]).sum::<f64>())
        };
        let before = (0..page).rev().map_while(&mut on).sum();
        let after = (page + 1..n).map_while(&mut on).sum();
        (before, after)
    }

    /// A pen line down the table at `x` from `ys.0` to `ys.1`: the cells it crosses split into two columns there (a
    /// vertically merged cell over all its rows), or along a column border, the border redrawn.
    fn draw_down(&mut self, table: TableId, d: &Drawn, x: f64, ys: (f64, f64), fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let g = Grid::of(t);
        let edge = (0..d.xs.len()).filter(|&e| (d.xs[e] - x).abs() <= REACH).min_by(|&a, &b| (d.xs[a] - x).abs().total_cmp(&(d.xs[b] - x).abs()));
        let (mut crossed, mut along) = (Vec::new(), Vec::new());
        for &(r, ..) in d.rows.iter().filter(|&&(_, top, bottom)| ys.1.min(bottom) - ys.0.max(top) > REACH) {
            for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                let (l, rr) = d.span(c, s);
                if x > l + REACH && x < rr - REACH {
                    crossed.push((r, k));
                } else if edge.is_some_and(|e| c == e || c + s.max(1) == e) {
                    along.push((r, k, c == edge.unwrap_or(0)));
                }
            }
        }
        if crossed.is_empty() && along.is_empty() {
            return Outcome::Unchanged("no cell there");
        }
        whole_merges(&g, &mut crossed);
        let pen = self.table_pen.clone();
        let cx = self.cx();
        let mut rows = t.rows.clone();
        let mut grid = d.widths.clone();
        for &(r, k, starts) in &along {
            set_cell_sides(&mut rows[r].cells[k], if starts { LEFT } else { RIGHT }, &pen, &cx);
        }
        let Some(&(r, k)) = crossed.first() else {
            let (r, k, _) = along[0];
            let col = g.rows[r][k].0;
            return self.rebuild(DRAW_TABLE, table, rows, t.grid.clone(), (r, col), fonts);
        };
        let caret = (r, g.rows[r][k].0);
        // The grid edge the split cells split at: the one drawn over, else a new one at the line
        let at = match edge {
            Some(e) => e,
            None => {
                let Some(j) = (0..grid.len()).find(|&j| d.xs[j] < x && x < d.xs[j + 1]) else { return Outcome::Unchanged("no cell there") };
                if grid.len() + 1 > super::tables::MAX_COLUMNS.max(grid.len()) {
                    return Outcome::unavailable("too many columns");
                }
                grid.splice(j..=j, [x - d.xs[j], d.xs[j + 1] - x]);
                for (r, row) in rows.iter_mut().enumerate() {
                    if j < row.grid_before {
                        row.grid_before += 1;
                        continue;
                    }
                    match g.cell_at(r, j) {
                        Some(k) if crossed.binary_search(&(r, k)).is_err() => row.cells[k].span = row.cells[k].span.max(1) + 1,
                        Some(_) => {}
                        None => row.grid_after += 1,
                    }
                }
                j + 1
            }
        };
        // Right to left in each row: a split moves the cells after it on
        for &(r, k) in crossed.iter().rev() {
            let (c, s, _) = g.rows[r][k];
            let (left, right) = (at - c, c + s.max(1) + usize::from(edge.is_none()) - at);
            let cell = &mut rows[r].cells[k];
            let mut new = empty_cell(cell);
            new.v_merge = cell.v_merge;
            (cell.span, new.span) = (left.max(1), right.max(1));
            set_cell_sides(cell, RIGHT, &pen, &cx);
            set_cell_sides(&mut new, LEFT, &pen, &cx);
            rows[r].cells.insert(k + 1, new);
            rows[r].src = None;
        }
        let ng = Grid::of_rows(&rows, grid.len());
        for (r, row) in rows.iter_mut().enumerate() {
            for (k, cell) in row.cells.iter_mut().enumerate() {
                let (c, s, _) = ng.rows[r][k];
                cell.width = Some(Width::Dxa(grid[c.min(grid.len())..(c + s.max(1)).min(grid.len())].iter().sum()));
            }
        }
        self.rebuild(DRAW_TABLE, table, rows, grid, caret, fonts)
    }

    /// The pen's first stroke outside a table: the outline of a new one-cell table, positioned where it was drawn,
    /// its cell that wide and at least that high, anchored before the paragraph there.
    fn draw_new_table(&mut self, s: Stroke, fonts: &Fonts) -> Outcome {
        let (left, top) = (s.from[0].min(s.to[0]), s.from[1].min(s.to[1]));
        let (w, h) = ((s.to[0] - s.from[0]).abs(), (s.to[1] - s.from[1]).abs());
        // Word's smallest cell: a column a quarter inch wide, a row a point high.
        if w < 18.0 || h < 1.0 {
            return Outcome::unavailable("draw a table's outline");
        }
        let Some((x, y)) = self.stack().to_doc(s.page, left, top) else { return Outcome::unavailable("no such page") };
        let Some(at) = self.hit(x, y, fonts) else { return Outcome::unavailable("no text there") };
        let Some((para, _)) = at.para_offset() else { return Outcome::unavailable("no text there") };
        let start = Position::text(at.story, para, Unit(0));
        if let Some(e) = self.edit_refusal_at(CommandId::InsertTable, &start) {
            return Outcome::Refused(e);
        }
        self.selection = Selection::Caret(start);
        let pen = self.table_pen.clone();
        self.joined(DRAW_TABLE, fonts, |e| {
            let out = e.insert_table(1, 1, fonts);
            if !matches!(out, Outcome::Changed(_)) {
                return out;
            }
            let Some(tg) = e.table_target() else { return out };
            use super::table_format::{Edges, TableFormat};
            e.table_format(&TableFormat::ColumnWidth(w.min(1584.0)), fonts);
            e.table_format(&TableFormat::RowHeight(Some((h.min(1584.0), "atLeast".into()))), fonts);
            e.table_format(&TableFormat::CellBorders(Edges::Outside, pen.clone()), fonts);
            let pos = crate::docx::props::TablePos {
                v_anchor: crate::docx::props::TableAnchor::Page,
                y: (top * 20.0).round() / 20.0,
                x: (left * 20.0).round() / 20.0,
                h_page: true,
                ..super::tables::NEW_POSITION
            };
            e.set_floating(tg.table, Some(pos), fonts);
            out
        })
    }

    /// The Eraser: touching the border between two cells merges them.
    pub fn erase_table_border(&mut self, page: usize, at: [f64; 2], fonts: &Fonts) -> Outcome {
        let Some(geo) = self.laid_page(page, fonts) else { return Outcome::unavailable("no such page") };
        let ctx = self.editing();
        let near = |v: f64, w: f64| (v - w).abs() <= REACH;
        let within = |v: f64, lo: f64, hi: f64| v > lo + REACH && v < hi - REACH;
        // The cells the point is in or on the edge of: a handful, however big the page's tables
        let touched = || geo.cells.iter().filter(|c| touches(&geo, ctx, c, at));
        // The two cells either side of the touched border, the first left of or above the second, innermost first
        let pair = touched()
            .filter_map(|a| {
                let p = a.turn.back(at);
                let key = *a.chain.last()?;
                let b = touched().find(|b| {
                    let Some(other) = b.chain.last() else { return false };
                    other.0 == key.0
                        && *other != key
                        && b.chain.len() == a.chain.len()
                        && ((near(a.rect[2], p[0]) && near(b.rect[0], p[0]) && within(p[1], a.rect[1].max(b.rect[1]), a.rect[3].min(b.rect[3])))
                            || (near(a.rect[3], p[1]) && near(b.rect[1], p[1]) && within(p[0], a.rect[0].max(b.rect[0]), a.rect[2].min(b.rect[2]))))
                })?;
                let across = near(a.rect[2], p[0]) && near(b.rect[0], p[0]);
                Some((a.chain.len(), key, *b.chain.last()?, across))
            })
            .max_by_key(|x| x.0);
        let Some((_, (table, ra, ka), (_, rb, kb), across)) = pair else { return Outcome::unavailable("no border between two cells there") };
        if let Some(e) = self.table_edit_refusal(table, CommandId::MergeCells) {
            return Outcome::Refused(e);
        }
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let g = Grid::of(t);
        // Each cell's rows and columns, whole merges included
        let extent = |r: usize, k: usize| {
            let (r0, k0) = g.merge_start(r, k);
            let last = merge_chain(&g, r0, k0).last().map_or(r0, |x| x.0);
            let (c, s, _) = g.rows[r0][k0];
            ((r0, last), (c, c + s.max(1) - 1))
        };
        let (Some(_), Some(_)) = (g.rows.get(ra).and_then(|x| x.get(ka)), g.rows.get(rb).and_then(|x| x.get(kb))) else {
            return Outcome::unavailable("no border between two cells there");
        };
        let ((a_rows, a_cols), (b_rows, b_cols)) = (extent(ra, ka), extent(rb, kb));
        // The two cells must make a rectangle
        if (across && a_rows != b_rows) || (!across && a_cols != b_cols) {
            return Outcome::unavailable("the cells either side don't line up");
        }
        let (r0, r1) = (a_rows.0.min(b_rows.0), a_rows.1.max(b_rows.1));
        let (c0, c1) = (a_cols.0.min(b_cols.0), a_cols.1.max(b_cols.1));
        let cells = Selection::Cells { table, anchor: CellRef { row: r0, col: c0 }, focus: CellRef { row: r1, col: c1 }, shape: CellShape::Rect };
        // The merge moves the cells' content together: a locked control in either keeps them apart
        if self.pieces_lock(cells, false) {
            return Outcome::unavailable("locked");
        }
        self.set_selection(cells);
        self.merge_cells(fonts)
    }
}
