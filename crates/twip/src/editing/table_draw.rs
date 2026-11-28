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

/// What a Draw Table pen stroke would draw, for the host to show while the stroke is dragged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeKind {
    /// A line across or down a table, snapped to the stroke's main direction.
    Line,
    /// A cell's diagonal, corner to corner.
    Diagonal,
    /// A new table's outline: the stroke's rectangle.
    Outline,
}

/// A table tool dragged on a page: where it was pressed and where it is now, in the page's points.
#[derive(Clone, Copy, Debug)]
pub(super) struct ToolDrag {
    page: usize,
    from: [f64; 2],
    now: [f64; 2],
}

/// How near a border a tool has to touch, in points.
const REACH: f64 = 3.0;

/// The shortest stroke the pen draws, in points: a click or a jitter draws nothing.
const MIN_STROKE: f64 = 3.0;

/// The Draw Table pen's line until one is set: single, ½ pt, automatic colour.
pub(super) fn default_pen() -> Border {
    Border { style: crate::docx::props::BorderStyle::Single, width: 0.5, space: 0.0, shadow: false, color: None, color_theme: None }
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

/// The cell of `table` at depth `depth` whose opposite corners the stroke from `a` to `b` (in the cell's own
/// coordinates) joins: a corner to corner stroke draws its diagonal.
fn diagonal_cell(geo: &PageGeometry, table: TableId, depth: usize, a: [f64; 2], b: [f64; 2]) -> Option<&GCell> {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    geo.cells.iter().filter(|c| c.chain.len() == depth && c.chain.last().is_some_and(|k| k.0 == table)).find(|c| {
        let r = c.rect;
        let (tx, ty) = (REACH.max(0.25 * (r[2] - r[0])), REACH.max(0.25 * (r[3] - r[1])));
        let near = |p: [f64; 2], x: f64, y: f64| (p[0] - x).abs() <= tx && (p[1] - y).abs() <= ty;
        let opposite = |p: [f64; 2], q: [f64; 2]| (near(p, r[0], r[1]) && near(q, r[2], r[3])) || (near(p, r[2], r[1]) && near(q, r[0], r[3]));
        // Across and down the cell both, not along a short row or a narrow column
        let slanted = dx.abs() >= 0.5 * (r[2] - r[0]) && dy.abs() >= 0.5 * (r[3] - r[1]);
        slanted && (opposite(a, b) || opposite(b, a))
    })
}

/// The two cells either side of the border at page point `at`, the first left of or above the second, innermost first:
/// their places, whether the border runs down (between columns), and its ends on the page.
type ErasePair = ((TableId, usize, usize), (usize, usize), bool, [[f64; 2]; 2]);

fn erase_pair(geo: &PageGeometry, ctx: StoryId, at: [f64; 2]) -> Option<ErasePair> {
    let near = |v: f64, w: f64| (v - w).abs() <= REACH;
    let within = |v: f64, lo: f64, hi: f64| v > lo + REACH && v < hi - REACH;
    // The cells the point is in or on the edge of: a handful, however big the page's tables
    let touched = || geo.cells.iter().filter(|c| touches(geo, ctx, c, at));
    touched()
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
            let ends = match across {
                true => [[a.rect[2], a.rect[1].max(b.rect[1])], [a.rect[2], a.rect[3].min(b.rect[3])]],
                false => [[a.rect[0].max(b.rect[0]), a.rect[3]], [a.rect[2].min(b.rect[2]), a.rect[3]]],
            };
            let off = if across { (a.rect[2] - p[0]).abs() } else { (a.rect[3] - p[1]).abs() };
            let &(_, rb, kb) = b.chain.last()?;
            Some((a.chain.len(), off, key, (rb, kb), across, ends.map(|e| a.turn.at(e))))
        })
        // The deepest, then the nearest edge
        .max_by(|x, y| x.0.cmp(&y.0).then(y.1.total_cmp(&x.1)))
        .map(|(_, _, key, other, across, ends)| (key, other, across, ends))
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

    /// Sets the pen's line from the API's border (0.25 to 6 pt, a colour value or automatic).
    pub fn set_table_pen_line(&mut self, b: &crate::api::command::Border) -> Outcome {
        let l = super::readback::BorderLine::of_api(b, 0.0, false);
        match super::readback::checked(self, &l) {
            Ok(c) => {
                self.table_pen = Border {
                    style: super::readback::border_style(l.style),
                    width: l.width.0,
                    space: l.space.0,
                    shadow: l.shadow,
                    color: c.map(|c| c.rgb),
                    color_theme: c.and_then(|c| c.theme),
                };
                Outcome::Moved
            }
            Err(e) => Outcome::invalid(&e),
        }
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
        if let Some(e) = self.table_edit_refusal(table, CommandId::DrawTableStroke) {
            return Outcome::Refused(e);
        }
        let (depth, turn) = (cell.chain.len(), cell.turn);
        let (a, b) = (turn.back(s.from), turn.back(s.to));
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        // Corner to corner in one cell: its diagonal
        let diagonal = diagonal_cell(&geo, table, depth, a, b);
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
        let grid = t.grid.clone();
        let Some(cell) = rows.get_mut(row).and_then(|r| r.cells.get_mut(k)) else { return Outcome::Unchanged("no cell there") };
        set_cell_sides(cell, if down { DIAGONAL_DOWN } else { DIAGONAL_UP }, &self.table_pen);
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
                set_cell_sides(cell, TOP, &pen);
            }
            for &k in &above {
                set_cell_sides(&mut rows[b - 1].cells[k], BOTTOM, &pen);
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
        let heights = rows[r].height.map(|(_, rule)| {
            let (before, after) = self.row_beyond(table, r, page, fonts);
            ((before + y - top, rule), (bottom - y + after, rule))
        });
        let mut lower = rows[r].clone();
        lower.rev = Default::default();
        lower.detach();
        rows[r].detach();
        for (k, cell) in lower.cells.iter_mut().enumerate() {
            let upper = &mut rows[r].cells[k];
            let continued = g.cell_at(r + 1, g.rows[r][k].0).is_some_and(|b| g.rows[r + 1][b].0 == g.rows[r][k].0 && g.continues(r + 1, b));
            *cell = empty_cell(upper);
            if crossed.binary_search(&k).is_ok() {
                cell.v_merge = if upper.v_merge != VMerge::None && continued { VMerge::Restart } else { VMerge::None };
                set_cell_sides(upper, BOTTOM, &pen);
                set_cell_sides(cell, TOP, &pen);
            } else {
                cell.v_merge = VMerge::Continue;
                if upper.v_merge == VMerge::None {
                    upper.v_merge = VMerge::Restart;
                }
            }
        }
        if let Some((u, l)) = heights {
            set_height(&mut rows[r], Some(u));
            set_height(&mut lower, Some(l));
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
        let mut rows = t.rows.clone();
        for &(r, k, starts) in &along {
            set_cell_sides(&mut rows[r].cells[k], if starts { LEFT } else { RIGHT }, &pen);
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
            set_cell_sides(cell, RIGHT, &pen);
            set_cell_sides(&mut new, LEFT, &pen);
            rows[r].cells.insert(k + 1, new);
            rows[r].detach();
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
        if let Some(e) = self.edit_refusal_at(CommandId::DrawTableStroke, &start) {
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
            e.table_format(&TableFormat::RowHeight(Some((h.min(1584.0), crate::docx::HeightRule::AtLeast))), fonts);
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
        let Some(((table, ra, ka), (rb, kb), across, _)) = erase_pair(&geo, self.editing(), at) else {
            return Outcome::unavailable("no border between two cells there");
        };
        if let Some(e) = self.table_edit_refusal(table, CommandId::EraseTableBorder) {
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

impl Editor {
    /// What the pen stroke `s` would draw, and where on its page (points): a line snapped to the stroke's main
    /// direction, a cell's diagonal, or a new table's outline; `None` for a stroke too short to draw or off the pages.
    pub fn table_stroke_preview(&mut self, s: Stroke, fonts: &Fonts) -> Option<(StrokeKind, [f64; 2], [f64; 2])> {
        if (s.to[0] - s.from[0]).hypot(s.to[1] - s.from[1]) < MIN_STROKE {
            return None;
        }
        let mid = [(s.from[0] + s.to[0]) / 2.0, (s.from[1] + s.to[1]) / 2.0];
        let geo = self.laid_page(s.page, fonts)?;
        let Some(cell) = cell_near(&geo, self.editing(), mid) else {
            let (a, b) = (s.from, s.to);
            return Some((StrokeKind::Outline, [a[0].min(b[0]), a[1].min(b[1])], [a[0].max(b[0]), a[1].max(b[1])]));
        };
        let &(table, ..) = cell.chain.last()?;
        let (a, b) = (cell.turn.back(s.from), cell.turn.back(s.to));
        if diagonal_cell(&geo, table, cell.chain.len(), a, b).is_some() {
            return Some((StrokeKind::Diagonal, s.from, s.to));
        }
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let (a, b) = match dx.abs() >= dy.abs() {
            true => {
                let y = (a[1] + b[1]) / 2.0;
                ([a[0], y], [b[0], y])
            }
            false => {
                let x = (a[0] + b[0]) / 2.0;
                ([x, a[1]], [x, b[1]])
            }
        };
        Some((StrokeKind::Line, cell.turn.at(a), cell.turn.at(b)))
    }

    /// The border the eraser at page point `at` would remove: its ends on the page (points); `None` where
    /// [`Editor::erase_table_border`] finds no border between two cells.
    pub fn eraser_target(&mut self, page: usize, at: [f64; 2], fonts: &Fonts) -> Option<([f64; 2], [f64; 2])> {
        let geo = self.laid_page(page, fonts)?;
        let (_, _, _, [a, b]) = erase_pair(&geo, self.editing(), at)?;
        Some((a, b))
    }

    /// The pointer's table tool.
    pub fn table_tool(&self) -> crate::api::command::TableTool {
        self.table_tool
    }

    /// Turns a table tool on or off: while one is on, a press and drag on a page draws or erases instead of
    /// selecting.
    pub fn set_table_tool(&mut self, tool: crate::api::command::TableTool) {
        self.table_tool = tool;
        if self.tool_drag.take().is_some() {
            self.requests.push(super::input::Request::ReleasePointer);
        }
    }

    /// The pen stroke being dragged: what it would draw and where (see [`Editor::table_stroke_preview`]).
    pub fn table_tool_stroke(&mut self, fonts: &Fonts) -> Option<(usize, StrokeKind, [f64; 2], [f64; 2])> {
        let d = self.tool_drag.filter(|_| self.table_tool == crate::api::command::TableTool::Pen)?;
        let (kind, a, b) = self.table_stroke_preview(Stroke { page: d.page, from: d.from, to: d.now }, fonts)?;
        Some((d.page, kind, a, b))
    }

    /// The border under the pointer the eraser would remove: its page and ends (see [`Editor::eraser_target`]).
    pub fn table_eraser_hover(&mut self, fonts: &Fonts) -> Option<(usize, [f64; 2], [f64; 2])> {
        if self.table_tool != crate::api::command::TableTool::Eraser {
            return None;
        }
        let (x, y) = self.hover?;
        let (page, px, py) = self.stack().page_at(x, y)?;
        let (a, b) = self.eraser_target(page, [px, py], fonts)?;
        Some((page, a, b))
    }

    /// A press while a table tool is on: the pen starts a stroke, the eraser erases the border there (true: the
    /// press is taken).
    pub(super) fn table_tool_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        use crate::api::command::TableTool;
        if self.table_tool == TableTool::None {
            return false;
        }
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return true };
        self.tool_drag = Some(ToolDrag { page, from: [px, py], now: [px, py] });
        self.requests.push(super::input::Request::CapturePointer);
        if self.table_tool == TableTool::Eraser {
            let _ = self.erase_table_border(page, [px, py], fonts);
        }
        true
    }

    /// The pointer moved while a tool is pressed: the pen's stroke follows it, the eraser erases what it touches.
    pub(super) fn table_tool_move(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(d) = self.tool_drag else { return false };
        let (ox, oy) = self.stack().to_doc(d.page, 0.0, 0.0).unwrap_or((0.0, 0.0));
        let now = [x - ox, y - oy];
        self.tool_drag = Some(ToolDrag { now, ..d });
        if self.table_tool == crate::api::command::TableTool::Eraser {
            let _ = self.erase_table_border(d.page, now, fonts);
        }
        true
    }

    /// The tool let go: the pen draws its stroke.
    pub(super) fn table_tool_release(&mut self, fonts: &Fonts) -> bool {
        let Some(d) = self.tool_drag.take() else { return false };
        self.requests.push(super::input::Request::ReleasePointer);
        if self.table_tool == crate::api::command::TableTool::Pen {
            let _ = self.draw_table_stroke(Stroke { page: d.page, from: d.from, to: d.now }, fonts);
        }
        true
    }

    /// The tool's drag cancelled: nothing drawn.
    pub(super) fn table_tool_cancel(&mut self) -> bool {
        let dragging = self.tool_drag.take().is_some();
        if dragging {
            self.requests.push(super::input::Request::ReleasePointer);
        }
        dragging
    }
}
