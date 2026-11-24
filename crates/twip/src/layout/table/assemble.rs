//! Table geometry

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::{Border, Jc};
use crate::docx::review::ReviewView;
use crate::layout::fragment::Child;
use crate::layout::ids::{ParaId, TableId};
use crate::layout::job::{Memos, Yield};
use crate::layout::story::{CellEnds, StoryEnv, StoryLayout, StoryUnit};
use crate::layout::units::EPS;

use super::{
    CellBox, CellGeo, FitRow, RowBox, TableLayout, borders::box_h, borders::bw, borders::conflict, borders::half_border, borders::placed_ind,
    borders::space_h, borders::thick, closing_para, draw::drawn_edges, draw::outside_floats, holds_float, widths::grid_columns, widths::twips,
};

pub fn outer_box(tb: &TableLayout) -> (f64, f64) {
    sides_box(tb, half_border, |w| w / 2.0)
}

/// A floating table's box as the text beside it sees it: past each row's last cell its border's part ([`part`]).
/// Compat ≤ 14 also counts the first cell's left side that way; compat 15 keeps the table's left edge.
pub fn float_box(tb: &TableLayout, t: &Table, legacy: bool) -> (f64, f64) {
    fn width(w: f64) -> f64 {
        if w > 0.0 { w / 2.0 } else { 0.75 }
    }
    if tb.rtl {
        // ponytail: right to left counts a border set nowhere and an undrawn one alike.
        return sides_box(tb, if legacy { width } else { half_border }, width);
    }
    let parts = tb.rows.iter().enumerate().filter_map(|(ri, r)| {
        let (a, z) = (r.cells.first()?, r.cells.last()?);
        let own = |c: &CellBox| t.rows.get(ri).and_then(|m| m.cells.get(c.index));
        let rb = row_borders(t, ri);
        let left = if legacy { part(own(a).map(|c| &c.left), &rb[1]) } else { half_border(bw(drawn_edges(tb, a)[1])) };
        Some((tb.x0 + a.x - left, tb.x0 + z.x + z.w + part(own(z).map(|c| &c.right), &rb[3])))
    });
    parts.fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), (a, z)| (l.min(a), r.max(z)))
}

/// What a floating table's outer border on one side adds to its box: half a drawn line; nothing for a `nil` set on
/// the table, a `none` in a colour or a line of no width; 0.75pt for one set nowhere, a `none` in the automatic
/// colour or a cell's own `nil`.
fn part(own: Option<&Option<Border>>, table: &Option<Border>) -> f64 {
    let (b, cell) = match own {
        Some(Some(b)) => (Some(b), true),
        _ => (table.as_ref(), false),
    };
    match b {
        None => 0.75,
        Some(b) if b.style == "nil" => {
            if cell {
                0.75
            } else {
                0.0
            }
        }
        Some(b) if b.style == "none" => {
            if b.color.is_some() {
                0.0
            } else {
                0.75
            }
        }
        Some(b) if b.visible() => thick(b) / 2.0,
        Some(_) => 0.0,
    }
}

/// The rows' widest extent between the first cell's left and the last cell's right margin.
pub fn cells_box(tb: &TableLayout) -> (f64, f64) {
    let m = |c: &CellBox, i: usize| c.margins[i];
    tb.rows.iter().filter_map(|r| Some((r.cells.first()?, r.cells.last()?))).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), (a, z)| {
        let (a, z) = if tb.rtl { (z, a) } else { (a, z) };
        (l.min(tb.x0 + a.x + m(a, 1)), r.max(tb.x0 + z.x + z.w - m(z, 3)))
    })
}

/// The rows' widest extent: the first cell's grid edge less `left` of its left border, the last cell's plus `right`
/// of its right border.
fn sides_box(tb: &TableLayout, left: fn(f64) -> f64, right: fn(f64) -> f64) -> (f64, f64) {
    let ends = |r: &RowBox| {
        let last = r.cells.len().checked_sub(1)?;
        Some(if tb.rtl { (last, 0) } else { (0, last) })
    };
    tb.rows.iter().filter_map(|r| ends(r).map(|(a, z)| (&r.cells[a], &r.cells[z]))).fold((f64::INFINITY, f64::NEG_INFINITY), |(l, r), (a, z)| {
        let (al, zr) = (drawn_edges(tb, a)[1], drawn_edges(tb, z)[3]);
        (l.min(tb.x0 + a.x - left(bw(al))), r.max(tb.x0 + z.x + z.w + right(bw(zr))))
    })
}

pub fn nested_shift(t: &Table, single: bool) -> f64 {
    if rtl(t) {
        let edges = cell_edges(t, grid_columns(t), single, &|_| None);
        let res = rtl_sides(edges.first().into_iter().flatten().map(|c| &*c.edges));
        return rtl_x0(t, 0.0, 0.0, res, false) - rtl_x0(t, 0.0, 0.0, res, true);
    }
    if matches!(t.jc, Jc::Center | Jc::Right) {
        return 0.0;
    }
    legacy_lead(t)
}

/// How far compat ≤ 14 puts a left-aligned table's box out before its column edge: half its left border and its
/// first cell's left margin.
pub fn legacy_lead(t: &Table) -> f64 {
    let (lw, lmar) = t
        .rows
        .first()
        .and_then(|r| r.cells.first())
        .map_or((space_h(&t.borders[1]), 0.0), |c| (space_h(candidate(Some(&c.left), &t.borders[1])), c.margins[1]));
    lw / 2.0 + edge_gap(t, lw, lmar)
}

/// The indent that places a left-aligned top-level table where it is drawn: its own, else in compat ≤ 14 (where an
/// unset indent isn't pulled left) the gap before its column edge.
pub fn placed_indent(t: &Table, compat: Compat) -> f64 {
    if t.ind_set || compat != Compat::Legacy {
        return placed_ind(t);
    }
    let (lw, lmar) = t
        .rows
        .first()
        .and_then(|r| r.cells.first())
        .map_or((space_h(&t.borders[1]), 0.0), |c| (space_h(candidate(Some(&c.left), &t.borders[1])), c.margins[1]));
    edge_gap(t, lw, lmar)
}

fn rtl(t: &Table) -> bool {
    t.bidi_visual && t.floating.is_none()
}

fn rtl_sides<'a>(mut row: impl Iterator<Item = &'a [Option<Border>; 4]>) -> (i64, i64) {
    let first = row.next();
    let last = row.last().or(first);
    (first.map_or(0, |e| twips(space_h(&e[3]))), last.map_or(0, |e| twips(space_h(&e[1]))))
}

fn rtl_x0(t: &Table, total: f64, text_w: f64, res: (i64, i64), sloppy: bool) -> f64 {
    let row = t.rows.first();
    let (first, last) = (row.and_then(|r| r.cells.first()), row.and_then(|r| r.cells.last()));
    let side = |own: Option<&Border>, table: &Option<Border>| twips(space_h(&conflict((own, true), (table.as_ref(), false))));
    let b_start = side(first.and_then(|c| c.left.as_ref()), &t.borders[1]);
    let b_end = side(last.and_then(|c| c.right.as_ref()), &t.borders[3]);
    let alone = row.is_some_and(|r| r.grid_before == 0 && r.cells.len() == 1);
    let first_outer = row.is_some_and(|r| r.grid_after == 0 && r.cells.len() == 1);
    let b_last = side(last.and_then(|c| c.left.as_ref()), &t.borders[if alone { 1 } else { 5 }]);
    let hb = |b: i64| (b + 1) / 2;
    let (w, width, ind, s) = (twips(text_w), twips(total), twips(placed_ind(t)), twips(t.cell_spacing.max(0.0)));
    let margin = |c: Option<&Cell>| c.map_or(0, |c| twips(c.margins[1]));
    let gap = |outer: bool, b: i64| if s > 0 { s * if outer { 2 } else { 1 } + b } else { 0 };
    let extra = (b_start + b_last) / 2;
    let left = match t.jc {
        Jc::Right if sloppy => -hb(b_end).max(margin(last) + gap(alone, res.1)),
        Jc::Right => 0,
        Jc::Center if sloppy => (w - width).div_euclid(2),
        Jc::Center => (w - width - extra).div_euclid(2),
        _ if sloppy => w - ind + hb(b_start).max(margin(first) + gap(first_outer, res.0)) - width,
        _ => w - ind - extra - width,
    };
    left as f64 / 20.0
}

fn candidate<'b>(own: Option<&'b Option<Border>>, table: &'b Option<Border>) -> &'b Option<Border> {
    match own {
        Some(o @ Some(_)) => o,
        _ => table,
    }
}

pub(super) fn edge_add(compat: Compat, t: &Table) -> (f64, f64) {
    let first = t.rows.first();
    let (mut lc, mut rc) = (first.and_then(|r| r.cells.first()), first.and_then(|r| r.cells.last()));
    if rtl(t) {
        (lc, rc) = (rc, lc);
    }
    let lw = space_h(candidate(lc.map(|c| &c.left), &t.borders[1]));
    let rw = space_h(candidate(rc.map(|c| &c.right), &t.borders[3]));
    if compat == Compat::Generic {
        (-half_border(lw), -half_border(rw))
    } else {
        (edge_gap(t, lw, lc.map_or(0.0, |c| c.margins[1])), edge_gap(t, rw, rc.map_or(0.0, |c| c.margins[3])))
    }
}

/// Row `ri`'s borders: its own exceptions (`w:tblPrEx`), else the table's.
pub fn row_borders(t: &Table, ri: usize) -> &[Option<Border>; 6] {
    t.rows.get(ri).and_then(|r| r.borders.as_deref()).unwrap_or(&t.borders)
}

fn edge_right(t: &Table) -> f64 {
    let last = t.rows.first().and_then(|r| if rtl(t) { r.cells.first() } else { r.cells.last() });
    space_h(candidate(last.map(|c| &c.right), &row_borders(t, 0)[3]))
}

fn edge_gap(t: &Table, b: f64, mar: f64) -> f64 {
    half_border(b).max(spacing_of(t, 0).map_or(mar, |s| 2.0 * s + b + mar))
}

/// The row's cell spacing, `None` when it has none. A row whose own spacing is 0 still frames its cells.
pub(super) fn spacing_of(t: &Table, ri: usize) -> Option<f64> {
    t.rows.get(ri).and_then(|r| r.cell_spacing).or((t.cell_spacing > 0.0).then_some(t.cell_spacing))
}

fn framed(t: &Table, ri: usize) -> bool {
    spacing_of(t, ri).is_some()
}

pub(super) fn gap(s: f64, outer: bool) -> f64 {
    if outer { 2.0 * s } else { s }
}

pub(super) struct CellEdges {
    pub(super) ci: usize,
    pub(super) col: usize,
    pub(super) span: usize,
    pub(super) edges: Rc<[Option<Border>; 4]>,
}

/// Each cell's resolved edges. A shared edge goes to the conflict winner of the two cells' borders, or with `single`
/// to the right or lower cell's border, the left or upper one's when that is nil.
struct Side<'a> {
    b: Option<&'a Border>,
    cell: bool,
}

fn side<'a>(own: &'a Option<Border>, table: &'a Option<Border>) -> Side<'a> {
    match own {
        Some(b) => Side { b: Some(b), cell: true },
        None => Side { b: table.as_ref(), cell: false },
    }
}

/// A cell's own side against its neighbour's; `later`: the cell is the right or lower one.
fn shared_side(own: &Side, other: &Side, later: bool, single: bool) -> Option<Border> {
    if single {
        let (first, second) = if later { (other, own) } else { (own, other) };
        second.b.filter(|b| b.visible()).or(first.b.filter(|b| b.visible())).cloned()
    } else {
        conflict((own.b, own.cell), (other.b, other.cell))
    }
}

/// The line drawn where two cells meet, as [`cell_edges`] resolves it: the left or upper cell's side (its own border,
/// else its row's or table's line there) against the right or lower one's.
pub fn shared_edge(first: (&Option<Border>, &Option<Border>), second: (&Option<Border>, &Option<Border>), single: bool) -> Option<Border> {
    shared_side(&side(first.0, first.1), &side(second.0, second.1), false, single)
}

pub(super) fn cell_edges<'k>(t: &Table, ncols: usize, single: bool, kept: &dyn Fn(usize) -> Option<&'k RowBox>) -> Vec<Vec<CellEdges>> {
    let nrows = t.rows.len();
    let shared = |own: &Side, other: &Side, later: bool| shared_side(own, other, later, single);
    let mut grid_cells: Vec<Vec<(usize, usize)>> = Vec::new();
    for r in &t.rows {
        let mut col = r.grid_before;
        let mut v = Vec::new();
        for c in &r.cells {
            v.push((col, c.span));
            col += c.span;
        }
        grid_cells.push(v);
    }
    let mut out = Vec::new();
    for (ri, r) in t.rows.iter().enumerate() {
        let alone = framed(t, ri);
        if let Some(k) = kept(ri).filter(|k| k.cells.iter().all(|c| c.index < r.cells.len())) {
            out.push(k.cells.iter().map(|c| CellEdges { ci: c.index, col: c.col, span: c.span, edges: c.edges.clone() }).collect());
            continue;
        }
        let mut cells = Vec::new();
        for (ci, c) in r.cells.iter().enumerate() {
            let (col, span) = grid_cells[ri][ci];
            let span = span.min(ncols.saturating_sub(col)).max(if col < ncols { 1 } else { 0 });
            if span == 0 {
                continue;
            }
            let tb = row_borders(t, ri);
            let top_t = if ri == 0 { &tb[0] } else { &tb[4] };
            let bot_t = if ri + 1 == nrows { &tb[2] } else { &tb[4] };
            let left_t = if col == 0 { &tb[1] } else { &tb[5] };
            let right_t = if col + span >= ncols { &tb[3] } else { &tb[5] };
            let (top, left, bottom, right) = (side(&c.top, top_t), side(&c.left, left_t), side(&c.bottom, bot_t), side(&c.right, right_t));
            let neighbour = |rr: usize, cc: usize| -> Option<&Cell> {
                t.rows.get(rr).and_then(|row| covering(&grid_cells[rr], cc, |&g| g).map(|i| &row.cells[i]))
            };
            let top_e = if ri == 0 || alone || framed(t, ri - 1) {
                conflict((top.b, top.cell), (None, false))
            } else {
                let above = neighbour(ri - 1, col);
                let ab = above.map_or(Side { b: None, cell: false }, |a| side(&a.bottom, &row_borders(t, ri - 1)[4]));
                shared(&top, &ab, true)
            };
            let bot_e = if ri + 1 == nrows || alone || framed(t, ri + 1) {
                conflict((bottom.b, bottom.cell), (None, false))
            } else {
                let below = neighbour(ri + 1, col);
                let bl = below.map_or(Side { b: None, cell: false }, |a| side(&a.top, &row_borders(t, ri + 1)[4]));
                shared(&bottom, &bl, false)
            };
            let left_e = if ci == 0 || alone {
                conflict((left.b, left.cell), (None, false))
            } else {
                let lc = &r.cells[ci - 1];
                let ls = side(&lc.right, &tb[5]);
                shared(&left, &ls, true)
            };
            let right_e = if ci + 1 == r.cells.len() || alone {
                conflict((right.b, right.cell), (None, false))
            } else {
                let rc = &r.cells[ci + 1];
                let rs = side(&rc.left, &tb[5]);
                shared(&right, &rs, false)
            };
            cells.push(CellEdges { ci, col, span, edges: Rc::new([top_e, left_e, bot_e, right_e]) });
        }
        out.push(cells);
    }
    out
}

pub(super) fn place(t: &Table, cols: &[f64], edges: &[Vec<CellEdges>]) -> Vec<Vec<CellGeo>> {
    let col_x: Vec<i64> = std::iter::once(0)
        .chain(cols.iter().scan(0, |acc, w| {
            *acc += twips(*w);
            Some(*acc)
        }))
        .collect();
    let pt = |v: i64| v as f64 / 20.0;
    let ncols = cols.len();
    edges
        .iter()
        .zip(&t.rows)
        .enumerate()
        .map(|(ri, (row, r))| {
            row.iter()
                .map(|e| {
                    let (c, col, span) = (&r.cells[e.ci], e.col, e.span);
                    let (mut left_e, mut right_e) = (&e.edges[1], &e.edges[3]);
                    let (mut left_outer, mut right_outer) = (col == 0, col + span >= ncols);
                    if rtl(t) {
                        (left_e, right_e, left_outer, right_outer) = (right_e, left_e, right_outer, left_outer);
                    }
                    let (mut x, mut w) = (col_x[col], col_x[col + span] - col_x[col]);
                    match r.nudge {
                        Some(true) if e.ci == 0 => w += 5,
                        Some(_) => x += 5,
                        None => {}
                    }
                    let (ml, mr) = (twips(c.margins[1]), twips(c.margins[3]));
                    let inner;
                    let (cx, cw) = if let Some(row_s) = spacing_of(t, ri) {
                        let (gl, gr) = (twips(gap(row_s, left_outer)), twips(gap(row_s, right_outer)));
                        let (bl, br) = (twips(space_h(left_e)), twips(space_h(right_e)));
                        // Without a gap the text starts half the border further left; the width stays.
                        let indent = if gl > 0 { ml } else { (ml - bl / 2).max(0) };
                        inner = (gl + bl / 2, w - gl - gr - (bl + br) / 2);
                        (gl + bl + indent, (w - gl - bl - ml - mr - br - gr).max(0))
                    } else {
                        let (bl, br) = (twips(box_h(left_e)), twips(box_h(right_e)));
                        inner = (bl / 2, w - bl / 2 - br / 2);
                        (ml.max(bl / 2), (w - (ml + mr).max((bl + br) / 2)).max(0))
                    };
                    CellGeo {
                        ci: e.ci,
                        col,
                        span,
                        x: pt(x),
                        w: pt(w),
                        cx: pt(cx),
                        cw: pt(cw),
                        inner: (pt(inner.0), pt(inner.1)),
                        edges: e.edges.clone(),
                    }
                })
                .collect()
        })
        .collect()
}

pub(super) fn vertical_cell(c: &Cell) -> Option<crate::layout::vertical::Vertical> {
    crate::layout::vertical::of_cell(text_flow(&c.text_direction))
}

/// A vertical cell's content laid out with lines `len` long.
fn vertical_story(
    c: &Cell,
    v: crate::layout::vertical::Vertical,
    len: f64,
    fixed: bool,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Rc<StoryLayout>, Yield> {
    let n = c.blocks.len() - usize::from(closing_para(c).is_some());
    let blocks: Vec<&Block> = c.blocks[..n].iter().collect();
    crate::layout::vertical::story(&blocks, len, v, env, Some(CellEnds { fixed, fit: c.fit_text, ..CellEnds::default() }), memos)
}

// Row assembly
#[allow(clippy::too_many_arguments)]
pub(super) fn assemble(
    t: &Table,
    text_w: f64,
    env: &StoryEnv,
    cols: Vec<f64>,
    geo: &[Vec<CellGeo>],
    stories: &[Option<Rc<StoryLayout>>],
    closings: &[Option<(ParaId, f64)>],
    fit: Rc<[FitRow]>,
    nested: bool,
    memos: &mut Memos,
) -> Result<TableLayout, Yield> {
    let total: f64 = cols.iter().sum();
    let compat = env.ctx.env.text.compat;
    let first_left = t.rows.first().and_then(|r| r.cells.first()).map(|c| (space_h(candidate(Some(&c.left), &row_borders(t, 0)[1])), c.margins[1]));
    let (lw, lmar) = first_left.unwrap_or((space_h(&row_borders(t, 0)[1]), 0.0));
    let rtl = rtl(t);
    let x0 = table_x0(t, text_w, &cols, geo, total, (lw, lmar), nested, rtl, compat);
    let asm = Assembly { t, text_w, env, cols: &cols, compat, nested, rtl, lw, lmar, x0 };
    let mirror = |x: f64, w: f64| if rtl { total - x - w } else { x };
    let mut stories = stories.iter();
    let mut closings = closings.iter();
    let mut rows: Vec<RowBox> = Vec::new();
    let review = &env.ctx.env.text.review;
    let single = env.ctx.env.text.single_cell_borders;
    // Compat ≤ 14: the edge under a repeated header row is resolved between the last header row and the page's first
    // row, as an inside edge.
    let header = t.rows.iter().take_while(|r| r.header).count().checked_sub(1).filter(|_| compat == Compat::Legacy);
    let table_shade = crate::layout::shade::Shade::of(t.shd.as_ref());
    for ((ri, r), cells_geo) in t.rows.iter().enumerate().zip(geo) {
        let row_s = spacing_of(t, ri);
        let own_or = |own: &Option<Border>, table: &Option<Border>| conflict((own.as_ref().or(table.as_ref()), own.is_some()), (None, false));
        let under_header = |c: &Cell, col: usize| -> Option<Border> {
            let h = header.filter(|&h| ri > h + 1)?;
            let above = geo[h].iter().find(|g| col >= g.col && col < g.col + g.span).map(|g| &t.rows[h].cells[g.ci].bottom);
            let (a, a_cell) = match above {
                Some(Some(b)) => (Some(b), true),
                _ => (row_borders(t, h)[4].as_ref(), false),
            };
            let (b, b_cell) = (c.top.as_ref().or(row_borders(t, ri)[4].as_ref()), c.top.is_some());
            if single { b.filter(|b| b.visible()).or(a.filter(|a| a.visible())).cloned() } else { conflict((b, b_cell), (a, a_cell)) }
        };
        let shift = row_shift(&asm, r, ri, cells_geo, row_s);
        let mut cells = Vec::new();
        for g in cells_geo {
            let c = &r.cells[g.ci];
            let shade = tint(review, r, c);
            let units = stories.next().cloned().flatten().unwrap_or_default();
            let content_h = units.height;
            let unaligned = compat == Compat::Legacy && env.ctx.env.text.do_not_vert_align_cell_with_sp && holds_float(c);
            let align_h = content_h.max(units.float_bottom);
            let fixed_floats = if compat == Compat::Legacy { outside_floats(c) } else { Box::default() };
            cells.push(CellBox {
                x: mirror(g.x, g.w) + shift,
                w: g.w,
                cx: g.cx,
                cw: g.cw,
                margins: c.margins,
                units,
                content_h,
                align_h,
                fixed_floats,
                v_align: if unaligned { "top".into() } else { c.v_align.clone() },
                v_merge: c.v_merge,
                col: g.col,
                span: g.span,
                edges: g.edges.clone(),
                cut: Rc::new([own_or(&c.top, &row_borders(t, ri)[0]), own_or(&c.bottom, &row_borders(t, ri)[2]), under_header(c, g.col)]),
                diagonals: c.diagonals.clone().filter(|d| d.iter().flatten().any(Border::visible)),
                shade,
                below: 0.0,
                index: g.ci,
                closing: closings.next().copied().flatten(),
            });
        }
        let bars = |m: Option<&crate::docx::review::RevisionMark>| m.is_some_and(|m| review.bars(m));
        let changed = review.change_bars()
            && (bars(r.rev.ins.as_deref())
                || bars(r.rev.del.as_deref())
                || bars(r.rev.change.as_deref().map(|c| &*c.mark))
                || cells_geo.iter().any(|g| {
                    let c = &r.cells[g.ci].rev;
                    bars(c.structural.as_ref().map(|(m, _)| &**m)) || bars(c.change.as_deref().map(|c| &*c.mark))
                }));
        let mut top_border = cells.iter().map(|c| space_h(&c.edges[0])).fold(0.0, f64::max);
        let seams: Vec<(f64, f64, Border)> = match ri.checked_sub(1).and_then(|p| geo.get(p)) {
            Some(above) if row_s.is_none() && !framed(t, ri - 1) => {
                let mut seams = Vec::new();
                for a in above {
                    let mut seam = |x0: f64, x1: f64, b: Option<Border>| {
                        if let Some(b) = b.filter(|b| b.visible()) {
                            seams.push(if rtl { (total - x1, total - x0, b) } else { (x0, x1, b) });
                        }
                    };
                    let (start, end) = (a.x - bw(&a.edges[1]) / 2.0, a.x + a.w + bw(&a.edges[3]) / 2.0);
                    let under = covering(cells_geo, a.col, |b| (b.col, b.span)).map(|i| &cells_geo[i]);
                    if under.is_some_and(|b| b.col != a.col && r.cells[b.ci].v_merge != VMerge::Continue) {
                        seam(start, end, a.edges[2].clone());
                        continue;
                    }
                    // Over the columns the row leaves out
                    let own = || own_or(&t.rows[ri - 1].cells[a.ci].bottom, &row_borders(t, ri - 1)[4]);
                    let (mut col, mut x) = (a.col, start);
                    let first = cells_geo.partition_point(|b| b.col + b.span <= a.col);
                    for b in cells_geo[first..].iter().take_while(|b| b.col < a.col + a.span) {
                        if b.col > col {
                            seam(x, b.x, own());
                        }
                        (col, x) = (b.col + b.span, b.x + b.w);
                    }
                    if col < a.col + a.span {
                        seam(x, end, own());
                    }
                }
                seams
            }
            _ => Vec::new(),
        };
        top_border = seams.iter().map(|s| thick(&s.2)).fold(top_border, f64::max);
        let (mut part_top, header_top) = if row_s.is_none() { row_top(t, ri, r, cells_geo, &mut cells, top_border, header) } else { (0.0, 0.0) };
        let mut gap_top = 0.0;
        let mut cell_bottom = 0.0;
        if let Some(s) = row_s {
            gap_top = if ri == 0 { 2.0 * s + space_h(&row_borders(t, 0)[0]) } else { s };
            // An unframed row above leaves its bottom border to this row's top.
            let under = ri.checked_sub(1).filter(|&p| !framed(t, p)).map_or(0.0, |p| {
                let w = rows[p].cells.iter().map(|c: &CellBox| space_h(&c.edges[2])).fold(0.0, f64::max);
                let own = top_border + s + cells.iter().map(|c| c.margins[0]).fold(0.0, f64::max);
                (w - own).max(0.0)
            });
            gap_top += under;
            // At the top of a later part the table's top border takes the place of the row above.
            if ri > 0 {
                part_top = space_h(&row_borders(t, ri)[0]) - under;
            }
            top_border += gap_top;
            cell_bottom = cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max) + gap(s, ri + 1 == t.rows.len());
        }
        // A row of vertical cells only is one vertical line high.
        let vertical: Vec<Option<crate::layout::vertical::Vertical>> = cells_geo.iter().map(|g| vertical_cell(&r.cells[g.ci])).collect();
        if vertical.iter().all(Option::is_some) {
            for ((c, g), v) in cells.iter_mut().zip(cells_geo).zip(&vertical) {
                if c.v_merge == VMerge::None {
                    let story = vertical_story(&r.cells[g.ci], v.unwrap(), g.cw, t.fixed, env, memos)?;
                    c.content_h = story.units.first().map_or(0.0, |u| u.height);
                }
            }
        }
        let below = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let content = cells.iter().filter(|c| c.v_merge == VMerge::None).map(|c| c.margins[0] + c.content_h).fold(f64::NEG_INFINITY, f64::max);
        let content = if content.is_finite() { content + below + cell_bottom } else { 0.0 };
        let min_margins = cells.iter().map(|c| c.margins[0] + c.margins[2]).fold(0.0, f64::max);
        let content = content.max(min_margins);
        let bottom = cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
        let (height, exact, min_h) = match &r.height {
            Some((h, rule)) if rule == "exact" && *h > 0.0 => (*h + bottom, true, 0.0),
            Some((h, rule)) if rule != "auto" => (top_border + content.max(*h + min_margins), false, top_border + *h + min_margins),
            _ => (top_border + content, false, 0.0),
        };
        // Vertical cells: lines as long as the row's content is high, stacked across the cell. A merged region's first
        // cell is laid out once its rows have their heights, below.
        for ((c, g), v) in cells.iter_mut().zip(cells_geo).zip(vertical) {
            let Some(v) = v.filter(|_| c.v_merge == VMerge::None) else { continue };
            let len = height - top_border - c.margins[0] - c.margins[2];
            turn_cell(c, &r.cells[g.ci], g, v, len, t.fixed, env, memos)?;
        }
        let first = match r.cells.first().and_then(|c| c.blocks.first()) {
            Some(Block::Para(p)) => Some(p.para_style_in(env.ctx.env.text.review.view)),
            _ => None,
        };
        let closes = row_s.is_none() && ri + 1 < t.rows.len() && framed(t, ri + 1);
        let fit_border = cells_geo
            .iter()
            .map(|g| {
                let own = &r.cells[g.ci].bottom;
                space_h(if own.is_some() { own } else { &row_borders(t, ri)[2] })
            })
            .fold(0.0, f64::max);
        let (lead, trail) = r.div.as_ref().map_or((0.0, 0.0), |d| (d.lead, d.trail));
        rows.push(RowBox {
            cells,
            top_border,
            height: height + lead + trail,
            cant_split: r.cant_split,
            keep_next: first.is_some_and(|s| s.keep_next),
            break_before: first.is_some_and(|s| s.page_break_before),
            header: r.header,
            exact,
            min_h,
            gap_top,
            part_top,
            header_top,
            bottom_gap: cell_bottom,
            seams,
            hidden: false,
            changed,
            lead,
            trail,
            div_boxes: r.div.as_ref().map_or_else(|| Rc::from([]), |d| d.boxes.clone()),
            ground: r.table_shading.map_or(table_shade, |(fill, hatch)| crate::layout::shade::Shade { fill, hatch }),
            shade: crate::layout::shade::Shade { fill: r.shading, hatch: r.hatch },
            spacing: row_s,
            closes,
            fit_border,
        });
    }
    fit_merged(&mut rows, t, geo, env, memos)?;
    let mut bottom_border = rows.last().map(|r| r.cells.iter().map(|c| space_h(&c.edges[2])).fold(0.0, f64::max)).unwrap_or(0.0);
    if framed(t, t.rows.len().saturating_sub(1)) {
        bottom_border = space_h(&row_borders(t, t.rows.len() - 1)[2]);
    }
    let (l, r) = if rtl { (3, 1) } else { (1, 3) };
    let (tb0, tbn) = (row_borders(t, 0), row_borders(t, t.rows.len().saturating_sub(1)));
    let outer = [tb0[0].clone(), tb0[l].clone(), tbn[2].clone(), tb0[r].clone()];
    Ok(TableLayout { x0, cols, rows, bottom_border, spacing: t.cell_spacing, outer, id: TableId(t.id), fit, rtl })
}

// Table-wide inputs of row assembly
struct Assembly<'a> {
    t: &'a Table,
    text_w: f64,
    env: &'a StoryEnv<'a>,
    cols: &'a [f64],
    compat: Compat,
    nested: bool,
    rtl: bool,
    lw: f64,
    lmar: f64,
    x0: f64,
}

// Table origin
#[allow(clippy::too_many_arguments, reason = "the table, its grid and the text it stands in")]
fn table_x0(
    t: &Table,
    text_w: f64,
    cols: &[f64],
    geo: &[Vec<CellGeo>],
    total: f64,
    (lw, lmar): (f64, f64),
    nested: bool,
    rtl: bool,
    compat: Compat,
) -> f64 {
    let mut x0 = match compat {
        Compat::Generic => placed_ind(t) + half_border(lw),
        // An unset indent isn't pulled left, but a nested table's is placed as indent 0.
        Compat::Legacy if !t.ind_set && !nested => 0.0,
        Compat::Legacy => placed_ind(t) - edge_gap(t, lw, lmar),
    };
    match t.jc {
        // The whole grid is centred, its `gridBefore` and `gridAfter` columns too.
        Jc::Center => {
            let (lh, rh) = (twips(half_border(lw)), twips(edge_right(t)) / 2);
            let free = twips(text_w) - (lh + twips(total) + rh);
            x0 = (lh + (free + 1).div_euclid(2)) as f64 / 20.0;
        }
        Jc::Right => x0 = (twips(text_w) - twips(total) - twips(edge_right(t)) / 2) as f64 / 20.0,
        _ => {}
    }
    // Floating tables
    if let Some(fp) = t.floating {
        // A cell's floating table is placed as in compat 15 in every mode.
        let compat = if nested { Compat::Generic } else { compat };
        let (e0, ind) = match compat {
            Compat::Generic => (half_border(lw), 0.0),
            Compat::Legacy => (-edge_gap(t, lw, lmar), t.ind),
        };
        let frame = || {
            let row_w = |(r, g): (&Row, &CellGeo)| match compat {
                Compat::Generic => {
                    let table = if g.col + g.span >= cols.len() { &t.borders[3] } else { &t.borders[5] };
                    let b = match (&g.edges[3], candidate(r.cells.get(g.ci).map(|c| &c.right), table)) {
                        (Some(b), _) => thick(b) - ((thick(b) * 20.0).round() as i64 / 2) as f64 / 20.0,
                        (None, None) => 0.75,
                        (None, Some(_)) => 0.0,
                    };
                    g.x + g.w + e0 + b
                }
                Compat::Legacy => g.x + g.cx + g.cw + e0,
            };
            t.rows.iter().zip(geo).filter_map(|(r, gs)| gs.last().map(|g| (r, g))).map(row_w).fold(0.0, f64::max) + ind
        };
        x0 = match fp.x_align() {
            Some(Jc::Center) => (text_w - frame()) / 2.0 + e0 + ind,
            Some(Jc::Right) => text_w - frame() + e0 + ind,
            Some(_) => e0 + ind,
            // A frame left of the page starts at its edge.
            None if fp.h_page => fp.x.max(0.05) + e0 + ind,
            None => fp.x + e0 + ind,
        };
    }
    if rtl {
        x0 = rtl_x0(t, total, text_w, rtl_sides(geo.first().into_iter().flatten().map(|g| &*g.edges)), compat == Compat::Legacy);
    }
    x0
}

// Row alignment shift
fn row_shift(asm: &Assembly, r: &Row, ri: usize, cells_geo: &[CellGeo], row_s: Option<f64>) -> f64 {
    let &Assembly { t, text_w, env, cols, compat, nested, rtl, lw, lmar, x0, .. } = asm;
    let shift = match r.ind {
        Some(i) if !t.fixed && t.jc == Jc::Left && t.floating.is_none() && !rtl => i - placed_ind(t),
        _ => 0.0,
    } + r.div.as_ref().filter(|_| t.jc == Jc::Left && t.floating.is_none() && !rtl).map_or(0.0, |d| d.shift);
    // Row alignment
    shift
        + match r.jc.unwrap_or(t.jc) {
            // Compat ≤ 14: with `alignTablesRowByRow` a row is aligned by its own cells' extent, else by the
            // whole grid. A nested table's rows always take the whole grid, and a right-aligned one no margin.
            jc @ (Jc::Center | Jc::Right) if compat == Compat::Legacy && t.floating.is_none() && !rtl => {
                let own = env.ctx.env.text.align_tables_row_by_row && !nested;
                let (start, end) = match (cells_geo.first(), cells_geo.last()) {
                    (Some(a), Some(z)) if own => (a.col.min(cols.len()), (z.col + z.span).min(cols.len())),
                    _ => (0, cols.len()),
                };
                let before: f64 = cols[..start].iter().sum();
                let width: f64 = cols[start..end.max(start)].iter().sum();
                let gap = if nested {
                    0.0
                } else {
                    r.cells.first().map_or(0.0, |c| c.margins[1]) + row_s.map_or(0.0, |s| 2.0 * s + space_h(&row_borders(t, ri)[1]))
                };
                let left = if jc == Jc::Center { (twips(text_w) - twips(width)).div_euclid(2) as f64 / 20.0 } else { text_w - width + gap };
                // A nested left-aligned table is moved by its lead as a whole (`nested_shift`).
                let lead = if nested && !matches!(t.jc, Jc::Center | Jc::Right) { legacy_lead(t) } else { 0.0 };
                left - (x0 + before) - lead
            }
            // A left row of a centred or right-aligned table: placed as a left-aligned table would be, a nested
            // one by its indent and lead too.
            Jc::Left if compat == Compat::Legacy && t.floating.is_none() && !rtl && t.jc != Jc::Left => {
                let (ind, lead) = if nested { (placed_ind(t).max(0.0), legacy_lead(t)) } else { (0.0, 0.0) };
                ind - edge_gap(t, lw, lmar) + lead - x0
            }
            _ => 0.0,
        }
}

// Top border and margins of a row without cell spacing
fn row_top(t: &Table, ri: usize, r: &Row, cells_geo: &[CellGeo], cells: &mut [CellBox], top_border: f64, header: Option<usize>) -> (f64, f64) {
    let mut part_top = 0.0;
    let mut header_top = 0.0;
    let cand = |own: &Option<Border>, table: &Option<Border>| space_h(if own.is_some() { own } else { table });
    let top_t = if ri == 0 { &row_borders(t, ri)[0] } else { &row_borders(t, ri)[4] };
    let w = ri
        .checked_sub(1)
        .filter(|&p| !framed(t, p))
        .map_or(0.0, |p| t.rows[p].cells.iter().map(|c| cand(&c.bottom, &row_borders(t, p)[4])).fold(0.0, f64::max));
    let above = cells_geo.iter().map(|g| cand(&r.cells[g.ci].top, top_t) + r.cells[g.ci].margins[0]).fold(w, f64::max);
    for c in cells.iter_mut() {
        c.margins[0] = (above - top_border).max(0.0);
    }
    // At the top of a later part the table's top border takes the place of the inside one and the row above.
    if ri > 0 {
        let top_t = &row_borders(t, ri)[0];
        let first = cells_geo.iter().map(|g| cand(&r.cells[g.ci].top, top_t) + r.cells[g.ci].margins[0]).fold(0.0, f64::max);
        part_top = first - above.max(top_border);
    }
    if let Some(h) = header.filter(|&h| ri > h + 1) {
        let w = t.rows[h].cells.iter().map(|c| cand(&c.bottom, &row_borders(t, h)[4])).fold(0.0, f64::max);
        let first = cells_geo.iter().map(|g| cand(&r.cells[g.ci].top, top_t) + r.cells[g.ci].margins[0]).fold(w, f64::max);
        let edge = cells.iter().map(|c| space_h(&c.cut[2])).fold(0.0, f64::max);
        header_top = first.max(edge) - above.max(top_border);
    }
    (part_top, header_top)
}

/// The cell covering grid column `col`, of cells in column order with their first column and span.
fn covering<T>(cells: &[T], col: usize, at: impl Fn(&T) -> (usize, usize)) -> Option<usize> {
    let i = cells.partition_point(|c| at(c).0 <= col).checked_sub(1)?;
    let (first, span) = at(&cells[i]);
    (col < first + span).then_some(i)
}

// Merged regions
fn fit_merged(rows: &mut [RowBox], t: &Table, geo: &[Vec<CellGeo>], env: &StoryEnv, memos: &mut Memos) -> Result<(), Yield> {
    let mut regions = Vec::new();
    for ri in 0..rows.len() {
        for ci in 0..rows[ri].cells.len() {
            if rows[ri].cells[ci].v_merge != VMerge::Restart {
                continue;
            }
            let col = rows[ri].cells[ci].col;
            let mut last = ri;
            let continues = |r: &RowBox| r.cells.binary_search_by_key(&col, |c| c.col).is_ok_and(|i| r.cells[i].v_merge == VMerge::Continue);
            while last + 1 < rows.len() && continues(&rows[last + 1]) {
                last += 1;
            }
            let c = &rows[ri].cells[ci];
            let need = rows[ri].top_border + c.margins[0] + c.content_h + c.margins[2] + rows[last].bottom_gap;
            let have: f64 = rows[ri..=last].iter().map(|r| r.height).sum();
            if need > have + EPS && !rows[last].exact {
                rows[last].height += need - have;
            }
            regions.push((ri, ci, last));
        }
    }
    // Once every region has grown its last row (a later region can grow a row an earlier one spans), each region's
    // rows below its first and, for vertical text, its lines as long as its content is high.
    for (ri, ci, last) in regions {
        rows[ri].cells[ci].below = rows[ri + 1..=last].iter().map(|r| r.height).sum();
        let g = &geo[ri][ci];
        if let Some(v) = vertical_cell(&t.rows[ri].cells[g.ci]) {
            let c = &rows[ri].cells[ci];
            let high: f64 = rows[ri..=last].iter().map(|r| r.height).sum();
            let len = high - rows[ri].top_border - c.margins[0] - c.margins[2];
            turn_cell(&mut rows[ri].cells[ci], &t.rows[ri].cells[g.ci], g, v, len, t.fixed, env, memos)?;
        }
    }
    Ok(())
}

/// A vertical cell's lines `len` long, turned and stacked across the cell, as its one unit.
#[allow(clippy::too_many_arguments)]
fn turn_cell(
    c: &mut CellBox,
    cell: &Cell,
    g: &CellGeo,
    v: crate::layout::vertical::Vertical,
    len: f64,
    fixed: bool,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<(), Yield> {
    let story = vertical_story(cell, v, len, fixed, env, memos)?;
    use crate::layout::vertical::Flow;
    // Compat ≤ 14: centred and bottom blocks reach into the margin on the far side (the left one when the
    // lines stack from the right), up to half its border.
    let far = if env.ctx.env.text.compat == Compat::Legacy {
        let side = if v.flow == Flow::Down { 1 } else { 3 };
        (c.margins[side] - bw(&c.edges[side]) / 2.0).max(0.0)
    } else {
        0.0
    };
    // Compat ≤ 14 doesn't clamp: a block thicker than the cell starts before its first side.
    let floor = if env.ctx.env.text.compat == Compat::Legacy { f64::NEG_INFINITY } else { 0.0 };
    let across = match c.v_align.as_str() {
        "center" => ((g.cw + far - story.height) / 2.0).max(floor),
        "bottom" => (g.cw + far - story.height).max(floor),
        _ => 0.0,
    };
    // Glyphs whose ink lies wholly outside the cell's clip, between its side borders' inner edges, are not drawn,
    // before the first side as past the far one.
    let (l, r) = (bw(&c.edges[1]) / 2.0, bw(&c.edges[3]) / 2.0);
    let (first, bottom) = match v.flow {
        Flow::Down => (g.cx + g.cw - g.w + r, g.cw + g.cx - l),
        Flow::Up => (l - g.cx, g.w - g.cx - r),
        Flow::DownEast => (f64::NEG_INFINITY, f64::INFINITY),
    };
    let rect = [0.0, 0.0, g.cw, len];
    let items = crate::layout::vertical::turned_in(&story.units, v, rect, across, first, bottom);
    let text = crate::layout::vertical::unturned(&story.units, v, rect, across, crate::layout::vertical::Turn::NONE);
    let unit = StoryUnit {
        top: 0.0,
        height: 0.0,
        below: 0.0,
        after: 0.0,
        hold: 0.0,
        content: Rc::from([Child::Turned { items: items.into(), text }]),
        block: 0,
        from: 0,
        widow: false,
        keep_next: false,
        keep_lines: false,
        notes: story.units.iter().flat_map(|u| u.notes.iter().copied()).collect(),
        row: None,
    };
    c.units = Rc::new(StoryLayout { units: vec![unit], ..StoryLayout::default() });
    c.v_align = "top".into();
    Ok(())
}

/// A cell's shading, or a tracked inserted, deleted, merged or split cell's (or inserted or deleted row's) highlight over its
/// fill: the author's colour mixed in, or the markup style's fixed colour.
fn tint(review: &ReviewView, r: &Row, c: &Cell) -> crate::layout::shade::Shade {
    use crate::docx::VMerge;
    use crate::docx::review::{CellMarkKind, MarkColor};
    let own = crate::layout::shade::Shade { fill: c.shading, hatch: c.hatch };
    let style = &review.markup.style;
    let shown = |m: &&std::rc::Rc<crate::docx::review::RevisionMark>| review.marks(m);
    let cell = c.rev.structural.as_ref().filter(|(m, _)| shown(&m)).map(|(m, k)| match k {
        CellMarkKind::Insert => (m, style.inserted_cells),
        CellMarkKind::Delete => (m, style.deleted_cells),
        CellMarkKind::Merge { v_merge, v_merge_orig } => {
            let split = *v_merge_orig == Some(VMerge::Continue) && *v_merge != Some(VMerge::Continue);
            (m, if split { style.split_cells } else { style.merged_cells })
        }
    });
    let row = || {
        r.rev
            .del
            .as_ref()
            .filter(shown)
            .map(|m| (m, style.deleted_cells))
            .or_else(|| r.rev.ins.as_ref().filter(shown).map(|m| (m, style.inserted_cells)))
    };
    let Some((m, color)) = cell.or_else(row) else { return own };
    match color {
        MarkColor::Auto => own,
        MarkColor::Fixed(f) => crate::layout::shade::Shade { fill: Some(f), hatch: None },
        MarkColor::ByAuthor => {
            let (a, b) = (review.palette.color(m.author), c.shading.unwrap_or(0xFFFFFF));
            let mix = |s: u32| ((((a >> s) & 0xFF) + 3 * ((b >> s) & 0xFF)) / 4) << s;
            crate::layout::shade::Shade { fill: Some(mix(16) | mix(8) | mix(0)), hatch: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::covering;

    #[test]
    fn the_cell_covering_a_column_is_found_by_its_first_column() {
        let cells = [(0, 2), (2, 0), (2, 1), (3, 3)];
        let at = |col| covering(&cells, col, |&c| c);
        assert_eq!([0, 1, 2, 3, 5, 6].map(at), [Some(0), Some(0), Some(2), Some(3), Some(3), None]);
        assert_eq!(covering(&[(1, 1)], 0, |&c: &(usize, usize)| c), None);
    }
}
