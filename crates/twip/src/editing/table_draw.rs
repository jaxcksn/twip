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

/// A table as drawn on a page: its rows there (model row, top, bottom, where its grid starts: a row with an indent of
/// its own starts elsewhere) and each grid edge's place from the grid's start.
struct Drawn {
    rows: Vec<(usize, f64, f64, f64)>,
    offsets: Vec<f64>,
    widths: Vec<f64>,
}

impl Drawn {
    fn of(geo: &PageGeometry, table: TableId, t: &crate::docx::Table, cols: usize) -> Option<Drawn> {
        let mut widths = super::tables::full_grid(t, cols);
        super::tables::drawn_grid(geo, table, t, &mut widths);
        let offsets: Vec<f64> = std::iter::once(0.0)
            .chain(widths.iter().scan(0.0, |x, w| {
                *x += w;
                Some(*x)
            }))
            .collect();
        let origin = |e: &[(f64, usize)]| e.first().map(|&(x, k)| x - offsets[k.min(offsets.len() - 1)]);
        let parts = || geo.rows.iter().filter(|r| r.table == table && !r.repeat);
        let table_origin = parts().find_map(|r| origin(&r.edges))?;
        let mut rows: Vec<(usize, f64, f64, f64)> =
            parts().map(|r| (r.row, r.rect[1], r.rect[3], origin(&r.edges).unwrap_or(table_origin))).collect();
        rows.sort_by_key(|r| r.0);
        Some(Drawn { rows, offsets, widths })
    }

    /// Where model row `r`'s grid starts: the table's where the row isn't on the page.
    fn origin(&self, r: usize) -> f64 {
        self.rows.iter().find(|x| x.0 == r).or(self.rows.first()).map_or(0.0, |x| x.3)
    }

    /// Where the cell over grid columns `c..c + s` starts and ends, in a row whose grid starts at `origin`.
    fn span(&self, origin: f64, c: usize, s: usize) -> (f64, f64) {
        let at = |k: usize| origin + self.offsets.get(k).or(self.offsets.last()).copied().unwrap_or(0.0);
        (at(c), at(c + s.max(1)))
    }
}

/// Whether segment `p`–`q` meets rectangle `r` (Liang–Barsky).
fn segment_meets(r: [f64; 4], p: [f64; 2], q: [f64; 2]) -> bool {
    let (dx, dy) = (q[0] - p[0], q[1] - p[1]);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (den, num) in [(-dx, p[0] - r[0]), (dx, r[2] - p[0]), (-dy, p[1] - r[1]), (dy, r[3] - p[1])] {
        if den == 0.0 {
            if num < 0.0 {
                return false;
            }
        } else if den < 0.0 {
            t0 = t0.max(num / den);
        } else {
            t1 = t1.min(num / den);
        }
    }
    t0 <= t1
}

/// The innermost table cell stroke `s` passes over, or within [`REACH`] of, in a story editing in `ctx` reaches.
fn cell_along<'g>(geo: &'g PageGeometry, ctx: StoryId, s: &Stroke) -> Option<&'g GCell> {
    let near = |c: &&GCell| {
        let r = [c.rect[0] - REACH, c.rect[1] - REACH, c.rect[2] + REACH, c.rect[3] + REACH];
        !c.chain.is_empty()
            && geo.stories.get(c.story).is_some_and(|x| reachable(ctx, x.host))
            && segment_meets(r, c.turn.back(s.from), c.turn.back(s.to))
    };
    geo.cells.iter().filter(near).max_by_key(|c| c.chain.len())
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
        // A stroke from a table out over the text edits the table it crosses.
        let Some(cell) = cell_near(&geo, ctx, mid).or_else(|| cell_along(&geo, ctx, &s)) else {
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
            // Across and down the cell both, not along a short row or a narrow column
            let slanted = dx.abs() >= 0.5 * (r[2] - r[0]) && dy.abs() >= 0.5 * (r[3] - r[1]);
            slanted && (opposite(a, b) || opposite(b, a))
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
            let origin = d.origin(r);
            g.rows[r].iter().enumerate().filter(move |&(_, &(c, s, _))| {
                let (l, rr) = d.span(origin, c, s);
                xs.1.min(rr) - xs.0.max(l) > REACH
            })
        };
        let pen = self.table_pen.clone();
        let cx = self.cx();
        let band = d.rows.iter().find(|&&(_, top, bottom, _)| y > top + REACH && y < bottom - REACH).map(|&(r, top, bottom, _)| (r, top, bottom));
        let Some((r, top, bottom)) = band else {
            // Along the border over row `b`: a row's top or bottom, not where a page cuts it
            let mut at = None;
            for &(r, top, bottom, _) in &d.rows {
                if (y - top).abs() <= REACH && self.row_beyond(table, r, page, fonts).0 == 0.0 {
                    at = Some(r);
                } else if (y - bottom).abs() <= REACH && self.row_beyond(table, r, page, fonts).1 == 0.0 {
                    at = Some(r + 1);
                }
                if at.is_some() {
                    break;
                }
            }
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
    /// vertically merged cell over all its rows), or along a column border, the border redrawn. Each row is matched
    /// where it is drawn: one with an indent of its own splits at its own place in the grid.
    fn draw_down(&mut self, table: TableId, d: &Drawn, x: f64, ys: (f64, f64), fonts: &Fonts) -> Outcome {
        let Some(t) = self.index.table(&self.doc, table) else { return Outcome::unavailable("not in a table") };
        let g = Grid::of(t);
        let offs = &d.offsets;
        // Where a crossed cell splits: at a grid edge, or at a new one this far from the grid's start
        #[derive(Clone, Copy)]
        enum At {
            Edge(usize),
            New(f64),
        }
        let (mut crossed, mut along, mut at_row) = (Vec::new(), Vec::new(), Vec::new());
        for &(r, _, _, origin) in d.rows.iter().filter(|&&(_, top, bottom, _)| ys.1.min(bottom) - ys.0.max(top) > REACH) {
            let dist = |e: usize| (origin + offs[e] - x).abs();
            let edge = (0..offs.len()).filter(|&e| dist(e) <= REACH).min_by(|&a, &b| dist(a).total_cmp(&dist(b)));
            for (k, &(c, s, _)) in g.rows[r].iter().enumerate() {
                let (l, rr) = d.span(origin, c, s);
                if x > l + REACH && x < rr - REACH {
                    crossed.push((r, k));
                } else if let Some(e) = edge.filter(|&e| c == e || c + s.max(1) == e) {
                    along.push((r, k, c == e));
                }
            }
            // To the 20th of a point a grid holds
            at_row.push((r, edge.map_or(At::New(((x - origin) * 20.0).round() / 20.0), At::Edge)));
        }
        if crossed.is_empty() && along.is_empty() {
            return Outcome::Unchanged("no cell there");
        }
        // A merge splits where the stroke crosses it, in all its rows
        let mut place: Vec<((usize, usize), At)> = Vec::with_capacity(crossed.len());
        for &(r, k) in &crossed {
            let start = g.merge_start(r, k);
            if !place.iter().any(|p| p.0 == start)
                && let Some(&(_, a)) = at_row.iter().find(|p| p.0 == r)
            {
                place.push((start, a));
            }
        }
        whole_merges(&g, &mut crossed);
        let pen = self.table_pen.clone();
        let cx = self.cx();
        let mut rows = t.rows.clone();
        for &(r, k, starts) in &along {
            set_cell_sides(&mut rows[r].cells[k], if starts { LEFT } else { RIGHT }, &pen, &cx);
        }
        let Some(&(r, k)) = crossed.first() else {
            let (r, k, _) = along[0];
            let col = g.rows[r][k].0;
            return self.rebuild(DRAW_TABLE, table, rows, t.grid.clone(), (r, col), fonts);
        };
        let caret = (r, g.rows[r][k].0);
        let split_of = |r: usize, k: usize| place.iter().find(|p| p.0 == g.merge_start(r, k)).map(|p| p.1);
        // The new grid edges, inside a column each: the cells across them span one more column
        let mut fresh: Vec<f64> = crossed
            .iter()
            .filter_map(|&(r, k)| match split_of(r, k) {
                Some(At::New(u)) => Some(u),
                _ => None,
            })
            .collect();
        fresh.sort_by(f64::total_cmp);
        fresh.dedup();
        if fresh.iter().any(|&u| !(0..d.widths.len()).any(|j| offs[j] < u && u < offs[j + 1])) {
            return Outcome::Unchanged("no cell there");
        }
        if d.widths.len() + fresh.len() > super::tables::MAX_COLUMNS.max(d.widths.len()) {
            return Outcome::unavailable("too many columns");
        }
        let before = |u: f64| fresh.partition_point(|&v| v < u);
        // Grid edge `e` and new edge `u` in the new grid
        let edge_now = |e: usize| e + before(offs[e.min(offs.len() - 1)]);
        let new_now = |u: f64| offs.partition_point(|&o| o < u) + before(u);
        let mut grid = Vec::with_capacity(d.widths.len() + fresh.len());
        for (j, &w) in d.widths.iter().enumerate() {
            let inside = &fresh[before(offs[j])..before(offs[j + 1])];
            if inside.is_empty() {
                grid.push(w);
                continue;
            }
            let cuts: Vec<f64> = std::iter::once(offs[j]).chain(inside.iter().copied()).chain([offs[j + 1]]).collect();
            grid.extend(cuts.windows(2).map(|p| p[1] - p[0]));
        }
        if !fresh.is_empty() {
            for (r, row) in rows.iter_mut().enumerate() {
                let line = &g.rows[r];
                let end = line.last().map_or(row.grid_before, |&(c, s, _)| c + s.max(1));
                row.grid_before = edge_now(row.grid_before);
                for (cell, &(c, s, _)) in row.cells.iter_mut().zip(line) {
                    cell.span = edge_now(c + s.max(1)) - edge_now(c);
                }
                row.grid_after = edge_now(end + row.grid_after) - edge_now(end);
            }
        }
        // Right to left in each row: a split moves the cells after it on
        for &(r, k) in crossed.iter().rev() {
            let Some(a) = split_of(r, k) else { continue };
            let (c, s, _) = g.rows[r][k];
            let (from, to) = (edge_now(c), edge_now(c + s.max(1)));
            let at = match a {
                At::Edge(e) => edge_now(e),
                At::New(u) => new_now(u),
            };
            if at <= from || at >= to {
                continue;
            }
            let cell = &mut rows[r].cells[k];
            let mut new = empty_cell(cell);
            new.v_merge = cell.v_merge;
            (cell.span, new.span) = (at - from, to - at);
            // The two halves' widths; the cells not split keep theirs
            let width = |a: usize, b: usize| Some(Width::Dxa(grid[a.min(grid.len())..b.min(grid.len())].iter().sum()));
            (cell.width, new.width) = (width(from, at), width(at, to));
            set_cell_sides(cell, RIGHT, &pen, &cx);
            set_cell_sides(&mut new, LEFT, &pen, &cx);
            rows[r].cells.insert(k + 1, new);
            rows[r].src = None;
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
                let off = if across { (a.rect[2] - p[0]).abs() } else { (a.rect[3] - p[1]).abs() };
                Some((a.chain.len(), off, key, *b.chain.last()?, across))
            })
            // The deepest, then the nearest edge
            .max_by(|x, y| x.0.cmp(&y.0).then(y.1.total_cmp(&x.1)));
        let Some((_, _, (table, ra, ka), (_, rb, kb), across)) = pair else { return Outcome::unavailable("no border between two cells there") };
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
