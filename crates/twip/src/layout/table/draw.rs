//! Row drawing

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::Border;
use crate::layout::cache::cached;
use crate::layout::fragment::{BoxKind, CellFragment, CellPlace, Child, GlyphRun, Item, LineFragment, PlacedLine, RowFragment, TableFragment};
use crate::layout::ids::Unit;
use crate::layout::job::Memos;
use crate::layout::story::StoryUnit;
use crate::layout::units::EPS;

use super::{
    CellBox, INK_SLACK, RowBox, TableLayout, assemble::gap, borders::bw, borders::diagonal, borders::edge, borders::thick, ink_top, key::RowProbe,
    source_units,
};

// Row drawing
#[derive(Clone, Copy)]
pub struct CellPart<'a> {
    pub units: &'a [StoryUnit],
    pub base: f64,
    pub ends: bool,
    /// The cell's content started in an earlier part: it isn't aligned down.
    pub cont: bool,
}

impl CellPart<'_> {
    pub fn height(&self) -> f64 {
        self.units.last().map_or(0.0, |l| (l.top - self.base) + l.height)
    }
}

pub fn whole(row: &RowBox) -> Vec<CellPart<'_>> {
    row.cells.iter().map(|c| CellPart { units: &c.units.units, base: 0.0, ends: true, cont: false }).collect()
}

pub(super) fn outside_floats(c: &Cell) -> Box<[u64]> {
    let paras = c.blocks.iter().filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None });
    paras
        .flat_map(|p| p.inlines.iter())
        .filter_map(|i| match i {
            Inline::Float(f, _) if !f.in_cell(Compat::Legacy) => Some(f.id),
            _ => None,
        })
        .collect()
}

fn unaligned_floats(children: &Rc<[Child]>, off: f64, fixed: &[u64]) -> Option<Rc<[Child]>> {
    let float = |c: &Child| matches!(c, Child::Box(b) if matches!(b.kind, BoxKind::Float { float, .. } if fixed.contains(&float.0)));
    if off == 0.0 || !children.iter().any(float) {
        return None;
    }
    let up = |c: &Child| match c {
        Child::Box(b) if float(c) => match b.kind {
            BoxKind::Float { anchor, float, rect, layer } => Child::Box(crate::layout::fragment::BoxFragment {
                kind: BoxKind::Float { anchor, float, layer, rect: [rect[0], rect[1] - off, rect[2], rect[3] - off] },
                offset: (b.offset.0, b.offset.1 - off),
                ..b.clone()
            }),
            _ => c.clone(),
        },
        _ => c.clone(),
    };
    Some(children.iter().map(up).collect())
}

/// A dashed rect's phase once cut from `old` to `new`: the pattern stays where it was.
fn cut_phase(old: &[f64; 4], new: &[f64; 4], phase: f32) -> f32 {
    let along_x = old[2] - old[0] >= old[3] - old[1];
    phase + (if along_x { new[0] - old[0] } else { new[1] - old[1] }) as f32
}

pub(in crate::layout) fn clip_x(children: &Rc<[Child]>, lo: f64, hi: f64, bottom: f64) -> Option<Rc<[Child]>> {
    clip_box(children, lo, hi, f64::NEG_INFINITY, bottom)
}

/// `children` without the glyphs whose ink lies wholly outside `lo .. hi` across or `top .. bottom` down, with their
/// nested tables' borders and shading cut to the box once a side of it is open.
pub(in crate::layout) fn clip_box(children: &Rc<[Child]>, lo: f64, hi: f64, top: f64, bottom: f64) -> Option<Rc<[Child]>> {
    clip_cut(children, lo, hi, top, bottom, true)
}

/// As [`clip_box`], a nested table's borders and shading cut only with `borders`.
fn clip_cut(children: &Rc<[Child]>, lo: f64, hi: f64, top: f64, bottom: f64, borders: bool) -> Option<Rc<[Child]>> {
    let inside = |r: &GlyphRun, g: &(u32, f64), dx: f64, dy: f64| {
        let k = r.size / r.face.upem;
        if bottom.is_finite() && r.face.ink_top(g.0).is_some_and(|t| dy + r.baseline - t * k >= bottom - EPS) {
            return false;
        }
        if top.is_finite() && r.face.ink_bottom(g.0).is_some_and(|b| dy + r.baseline + b * k <= top + EPS) {
            return false;
        }
        match r.face.ink(g.0) {
            Some((bearing, width)) => {
                let x0 = dx + g.1 + bearing * k;
                x0 < hi - EPS && x0 + width * k > lo + EPS
            }
            None => (lo..hi).contains(&(dx + g.1)),
        }
    };
    let spills = |l: &PlacedLine| {
        l.line.items.iter().any(|i| match i {
            Item::Glyphs(r) => r.glyphs.iter().any(|g| !inside(r, g, l.x, l.y)),
            Item::Rect { rect, .. } => l.x + rect[0] < lo - EPS || l.x + rect[2] > hi + EPS,
            Item::Shape(s) => s.ink && (l.x + s.rect[0] < lo - EPS || l.x + s.rect[2] > hi + EPS),
            _ => false,
        })
    };
    let nested: Vec<Option<Child>> = children.iter().map(|c| clip_nested(c, lo, hi, top, bottom, borders)).collect();
    if !children.iter().any(|c| matches!(c, Child::Line { placed, .. } if spills(placed))) && nested.iter().all(Option::is_none) {
        return None;
    }
    let clipped = children.iter().zip(nested).map(|(c, n)| match (c, n) {
        (_, Some(n)) => n,
        (Child::Line { para, placed }, None) if spills(placed) => {
            let (dx, dy) = (placed.x, placed.y);
            let items: Vec<Item> = placed
                .line
                .items
                .iter()
                .filter_map(|i| match i {
                    Item::Glyphs(r) => {
                        let glyphs: Box<[(u32, f64)]> = r.glyphs.iter().copied().filter(|g| inside(r, g, dx, dy)).collect();
                        let (from, to) = glyphs.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), g| (a.min(g.1), b.max(g.1)));
                        let at = |g: &(u32, f64), x: f64| (g.1 - x).abs() < EPS;
                        let dropped = |x: f64| r.glyphs.iter().any(|g| at(g, x) && !inside(r, g, dx, dy)) && !glyphs.iter().any(|g| at(g, x));
                        let chars: Box<[(char, f64, Option<Unit>)]> =
                            r.chars.iter().copied().filter(|ch| ch.1 >= from - EPS && ch.1 <= to + EPS && !dropped(ch.1)).collect();
                        (!glyphs.is_empty() || r.glyphs.is_empty()).then(|| Item::Glyphs(GlyphRun { glyphs, chars, ..r.clone() }))
                    }
                    Item::Rect { rect, colour, snap, dash, phase } => {
                        let (x0, x1) = (rect[0].max(lo - dx), rect[2].min(hi - dx));
                        let cut = [x0, rect[1], x1, rect[3]];
                        (x1 > x0).then_some(Item::Rect { rect: cut, colour: *colour, snap: *snap, dash: *dash, phase: cut_phase(rect, &cut, *phase) })
                    }
                    Item::Shape(s) if s.ink => {
                        super::clip_stroke(s, [lo - dx, f64::NEG_INFINITY, hi - dx, f64::INFINITY]).map(|s| Item::Shape(Rc::new(s)))
                    }
                    other => Some(other.clone()),
                })
                .collect();
            let atoms = placed.line.atoms.iter().copied().filter(|a| (lo..hi).contains(&(dx + a.x0))).collect();
            let line = Rc::new(LineFragment {
                items: items.into_boxed_slice(),
                pitch: placed.line.pitch,
                baseline: placed.line.baseline,
                below: placed.line.below,
                after: placed.line.after,
                atoms,
                changed: placed.line.changed,
                mark: placed.line.mark,
                texts: placed.line.texts.clone(),
                expansions: placed.line.expansions.clone(),
                label: placed.line.label.and_then(|(a, b)| {
                    let (a, b) = (a.max(lo - dx), b.min(hi - dx));
                    (b > a).then_some((a, b))
                }),
            });
            Child::Line { para: *para, placed: PlacedLine { line, ..placed.clone() } }
        }
        (other, None) => other.clone(),
    });
    Some(clipped.collect())
}

fn clip_nested(c: &Child, lo: f64, hi: f64, top: f64, bottom: f64, borders: bool) -> Option<Child> {
    match c {
        Child::Group { offset, children } => clip_cut(children, lo - offset.0, hi - offset.0, top - offset.1, bottom - offset.1, borders)
            .map(|children| Child::Group { offset: *offset, children }),
        Child::Table(t) => {
            let mut any = false;
            // A nested table's borders and shading are cut with its text once the enclosing cell's clip is open.
            let open = borders && (lo.is_finite() || hi.is_finite());
            let rows: Vec<RowFragment> = {
                let mut cut = |items: &[Item]| -> Box<[Item]> {
                    if !open {
                        return items.into();
                    }
                    items
                        .iter()
                        .filter_map(|i| match i {
                            Item::Rect { rect, colour, snap, dash, phase } => {
                                let r = [rect[0].max(lo), rect[1].max(top), rect[2].min(hi), rect[3].min(bottom)];
                                if r != *rect {
                                    any = true;
                                }
                                (r[2] > r[0] && r[3] > r[1]).then_some(Item::Rect {
                                    rect: r,
                                    colour: *colour,
                                    snap: *snap,
                                    dash: *dash,
                                    phase: cut_phase(rect, &r, *phase),
                                })
                            }
                            Item::Shape(s) if s.ink => {
                                let cut = super::clip_stroke(s, [lo, top, hi, bottom]);
                                if cut.as_ref().is_none_or(|c| !Rc::ptr_eq(&c.geom, &s.geom)) {
                                    any = true;
                                }
                                cut.map(|c| Item::Shape(Rc::new(c)))
                            }
                            other => Some(other.clone()),
                        })
                        .collect()
                };
                t.rows.iter().map(|r| RowFragment { background: cut(&r.background), borders: cut(&r.borders), ..r.clone() }).collect()
            };
            let rows = rows.iter().map(|r| {
                let cells = r.cells.iter().map(|cell| {
                    let content = cell
                        .content
                        .iter()
                        .map(|c| clip_nested(c, lo, hi, top, bottom, borders).inspect(|_| any = true).unwrap_or_else(|| c.clone()));
                    CellFragment { content: content.collect(), ..cell.clone() }
                });
                RowFragment { cells: cells.collect(), ..r.clone() }
            });
            let t = TableFragment { table: t.table, rows: rows.collect() };
            any.then_some(Child::Table(t))
        }
        _ => None,
    }
}

pub(super) fn drawn_edges<'a>(tb: &TableLayout, c: &'a CellBox) -> [&'a Option<Border>; 4] {
    let [t, l, b, r] = &*c.edges;
    if tb.rtl { [t, r, b, l] } else { [t, l, b, r] }
}

/// A cell's top edge at `y` as a row draws it.
fn top_edge(out: &mut Vec<Item>, tb: &TableLayout, c: &CellBox, x0: f64, y: f64, t: &Option<Border>) {
    let [_, l, _, r] = drawn_edges(tb, c);
    if let Some(t) = t.as_ref().filter(|b| b.visible()) {
        edge(out, [x0 + c.x - bw(l) / 2.0, y, x0 + c.x + c.w + bw(r) / 2.0, y + thick(t)], true, false, t);
    }
}

/// A cell's bottom edge under `y` as a row draws it.
fn bottom_edge(out: &mut Vec<Item>, tb: &TableLayout, c: &CellBox, x0: f64, y: f64, b: &Option<Border>) {
    let [_, l, _, r] = drawn_edges(tb, c);
    if let Some(b) = b.as_ref().filter(|b| b.visible()) {
        edge(out, [x0 + c.x - bw(l) / 2.0, y, x0 + c.x + c.w + bw(r) / 2.0, y + thick(b)], true, true, b);
    }
}

/// Where a part of a table starts or ends on a page. An unspaced part's first row draws the table's first-row top
/// edge from `open` (each cell's own top, else the table's top; under repeated header rows, `under_header`, the edge
/// resolved with the last header row) and its last row the table's last-row bottom edge. A spaced table's outer
/// frame takes the table's top border from `open` down to the part's first row and its bottom border under the
/// part's last row.
pub fn frame_part(tb: &TableLayout, frag: &mut RowFragment, left: f64, open: Option<(f64, bool)>, close: bool) {
    let row = &tb.rows[frag.row];
    if row.spacing.is_none() {
        let x0 = left + tb.x0;
        let mut old = Vec::new();
        let mut new = Vec::new();
        if let Some((y, under_header)) = open {
            for c in &row.cells {
                if c.v_merge != VMerge::Continue {
                    top_edge(&mut old, tb, c, x0, frag.top, &c.edges[0]);
                }
                top_edge(&mut new, tb, c, x0, y, &c.cut[if under_header { 2 } else { 0 }]);
            }
            for (a, b, s) in &row.seams {
                edge(&mut old, [x0 + a, frag.top, x0 + b, frag.top + thick(s)], true, false, s);
            }
        }
        if close {
            for c in &row.cells {
                if row.closes {
                    bottom_edge(&mut old, tb, c, x0, frag.rect[3], &c.edges[2]);
                }
                bottom_edge(&mut new, tb, c, x0, frag.rect[3], &c.cut[1]);
            }
        }
        let same = |a: &Item, b: &Item| match (a, b) {
            (Item::Rect { rect: r, colour: c, dash: e, .. }, Item::Rect { rect: q, colour: d, dash: f, .. }) => {
                c == d && e == f && r.iter().zip(q).all(|(x, y)| (x - y).abs() < EPS)
            }
            (Item::Shape(a), Item::Shape(b)) => a == b,
            _ => false,
        };
        let mut borders = std::mem::take(&mut frag.borders).into_vec();
        for o in &old {
            if let Some(k) = borders.iter().position(|b| same(b, o)) {
                borders.remove(k);
            }
        }
        borders.extend(new);
        frag.borders = borders.into();
        return;
    }
    if !row.spacing.is_some_and(|s| s > 0.0) {
        return;
    }
    let open = open.map(|o| o.0);
    let [t, l, b, r] = &tb.outer;
    let x0 = left + tb.x0;
    let x1 = x0 + tb.cols.iter().sum::<f64>();
    let (lw, rw) = (bw(l), bw(r));
    let mut borders = std::mem::take(&mut frag.borders).into_vec();
    let sides = |borders: &mut Vec<Item>, y0: f64, y1: f64| {
        if let Some(l) = l.as_ref().filter(|b| b.visible()) {
            edge(borders, [x0 - lw / 2.0, y0, x0 + lw / 2.0, y1], false, false, l);
        }
        if let Some(r) = r.as_ref().filter(|b| b.visible()) {
            edge(borders, [x1 - rw / 2.0, y0, x1 + rw / 2.0, y1], false, true, r);
        }
    };
    if let Some(y) = open {
        if let Some(t) = t.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, y, x1 + rw / 2.0, y + thick(t)], true, false, t);
        }
        sides(&mut borders, y, frag.rect[1]);
    }
    if close {
        let y = frag.rect[3];
        let bottom = y + tb.bottom_border - bw(b);
        sides(&mut borders, y, bottom);
        if let Some(b) = b.as_ref().filter(|b| b.visible()) {
            edge(&mut borders, [x0 - lw / 2.0, bottom, x1 + rw / 2.0, bottom + thick(b)], true, true, b);
        }
    }
    frag.borders = borders.into();
}

pub fn draw_row(tb: &TableLayout, ri: usize, left: f64, top: f64, height: f64, parts: Option<&[CellPart]>, last_of_table: bool) -> RowFragment {
    let row = &tb.rows[ri];
    let whole = (height - row.height).abs() < EPS;
    let (lead, trail) = if whole { (row.lead, row.trail) } else { (0.0, 0.0) };
    let (outer_top, outer_bottom) = (top, top + height);
    let (top, height) = (top + lead, height - lead - trail);
    let x0 = left + tb.x0;
    let (mut background, mut cells, mut borders, mut diagonals) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut edges: Vec<(f64, usize)> = Vec::new();
    let mut rect = [x0, top, x0, top + height];
    if let (Some(mut first), Some(mut end)) = (row.cells.first(), row.cells.last()) {
        if tb.rtl {
            (first, end) = (end, first);
        }
        let mut order: Vec<&CellBox> = row.cells.iter().collect();
        if tb.rtl {
            order.reverse();
        }
        for c in order {
            let (l, r) = if tb.rtl { (c.col + c.span, c.col) } else { (c.col, c.col + c.span) };
            for e in [(x0 + c.x, l), (x0 + c.x + c.w, r)] {
                // In column order: a shared edge is the one just pushed.
                if edges.last().is_none_or(|x| x.1 != e.1) {
                    edges.push(e);
                }
            }
        }
        rect = [x0 + first.x, top, x0 + end.x + end.w, top + height];
    }
    // The table's shading, then the row's exceptions', show around spaced cells only.
    if let Some(s) = row.spacing.filter(|s| *s > 0.0)
        && !(row.ground.is_none() && row.shade.is_none())
    {
        let boxes: Vec<[f64; 4]> = row.cells.iter().map(|c| spaced_box(tb, row, ri, c, s, x0, (top, height))).collect();
        row.ground.draw_around(&mut background, rect, &boxes);
        row.shade.draw_around(&mut background, rect, &boxes);
    }
    let below_margin = row.cells.iter().map(|c| c.margins[2]).fold(0.0, f64::max);
    for (k, c) in row.cells.iter().enumerate() {
        let part = parts.map(|p| p[k]).unwrap_or(CellPart { units: &c.units.units, base: 0.0, ends: true, cont: false });
        let ch = if part.ends && part.base == 0.0 && part.units.len() == c.units.units.len() { c.align_h } else { part.height() };
        let height = height + c.below;
        // Every cell aligns in the row's box: below the row's widest top border and margin, above its largest bottom
        // margin.
        let inner = height - row.top_border - c.margins[0] - below_margin;
        let off = match c.v_align.as_str() {
            _ if part.cont => 0.0,
            "center" => ((inner - ch) / 2.0).max(0.0),
            "bottom" => (inner - ch).max(0.0),
            _ => 0.0,
        };
        let base_y = top + row.top_border + c.margins[0] + off;
        let base_x = x0 + c.x + c.cx;
        let clip = if row.exact { top + height - below_margin } else { f64::INFINITY };
        let mut content = Vec::new();
        let mut hidden = Vec::new();
        // Content clips to the box between the borders' inner edges: across, the left and right borders'; down, the
        // row's top border and its bottom edge (an exact row's bottom margin). A spaced cell clips inside its own
        // border box. A nested table also clips to it once a paragraph of the cell has been drawn in this part;
        // before that it keeps only its own cells' clips.
        let [et, el, eb, er] = drawn_edges(tb, c);
        let (lo, hi, y0, y1) = match row.spacing {
            Some(s) => {
                let (mut lo, mut ro) = (c.col == 0, c.col + c.span >= tb.cols.len());
                if tb.rtl {
                    (lo, ro) = (ro, lo);
                }
                let (by0, by1) = (top + row.gap_top, top + height - gap(s, tb.is_last(ri)));
                let y1 = if c.v_merge == VMerge::Continue { f64::INFINITY } else { by1 - bw(eb) };
                (gap(s, lo) + bw(el) - c.cx, c.w - gap(s, ro) - bw(er) - c.cx, by0 + bw(et), y1)
            }
            None => {
                let t = if c.v_merge == VMerge::Continue { 0.0 } else { bw(et) };
                // A continuation cell's carried content runs on through the region's rows.
                let y1 = if c.v_merge == VMerge::Continue { f64::INFINITY } else { top + height };
                (-c.cx + bw(el) / 2.0, c.w - c.cx - bw(er) / 2.0, top + t, y1)
            }
        };
        let y1 = if row.exact { clip } else { y1 };
        let mut open = false;
        for u in part.units {
            let ut = u.top - part.base;
            let bottom = clip - (base_y + ut) + INK_SLACK;
            let hide = match ink_top(&u.content) {
                Some(top) => top >= bottom - EPS,
                None => base_y + ut >= clip - EPS,
            };
            if hide {
                source_units(&u.content, &mut hidden);
                continue;
            }
            // A floating table's text clips from the start, its borders not.
            let floating = u.row.as_ref().is_some_and(|r| r.floating);
            let shown = u.row.is_none() || floating || open;
            let (lo, hi, top, bottom) = if shown {
                (lo, hi, y0 - (base_y + ut), y1 - (base_y + ut) + INK_SLACK)
            } else {
                (f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY, bottom)
            };
            open |= u.row.is_none();
            let children = clip_cut(&u.content, lo, hi, top, bottom, !floating).unwrap_or_else(|| u.content.clone());
            let children = unaligned_floats(&children, off, &c.fixed_floats).unwrap_or(children);
            content.push(Child::Group { offset: (base_x, base_y + ut), children });
        }
        let closing = c.closing.filter(|_| part.ends).map(|(para, size)| (para, [base_x, base_y + ch, size * 1.2]));
        let place = CellPlace {
            index: c.index,
            col: c.col,
            span: c.span,
            rect: [x0 + c.x, top, x0 + c.x + c.w, top + height],
            clip: row.exact.then_some(clip),
            v_merge: c.v_merge,
            hidden: hidden.into_boxed_slice(),
            closing,
        };
        cells.push(CellFragment { cell: k, content: content.into_boxed_slice(), place });
        // A cell's diagonals: in a spaced cell between its frame's inner edges; else across the grid lines, from under
        // its top border to the row's (a merged region's) bottom.
        let diagonal_box = if let Some(spacing) = row.spacing {
            let [bx0, by0, bx1, by1] = spaced_cell_frame(tb, row, ri, c, spacing, x0, (top, height), &mut background, &mut borders);
            [bx0 + bw(el), by0 + bw(et), bx1 - bw(er), by1 - bw(eb)]
        } else {
            let r = [x0 + c.x, top, x0 + c.x + c.w, top + height];
            c.shade.draw(&mut background, r, r);
            cell_borders(tb, row, c, x0, (top, height), last_of_table, &mut borders);
            [r[0], top + bw(et), r[2], r[3]]
        };
        // Diagonals, unmirrored right to left
        if let Some(d) = c.diagonals.as_deref().filter(|_| c.v_merge != VMerge::Continue) {
            for (b, up) in d.iter().zip([false, true]) {
                if let Some(b) = b.as_ref().filter(|b| b.visible()) {
                    diagonal(&mut diagonals, diagonal_box, up, b);
                }
            }
        }
    }
    for (a, b, s) in &row.seams {
        edge(&mut borders, [x0 + a, top, x0 + b, top + thick(s)], true, false, s);
    }
    if row.spacing.is_some_and(|s| s > 0.0) {
        spacing_frame(tb, ri, x0, (top, height), last_of_table, &mut borders);
    }
    // Division boxes
    if whole && !row.div_boxes.is_empty() {
        division_boxes(row, rect, (top, height), (outer_top, outer_bottom), &mut borders);
    }
    // Over the borders, as Word draws them.
    borders.append(&mut diagonals);
    RowFragment {
        row: ri,
        rect,
        top,
        edges: edges.into_boxed_slice(),
        background: background.into_boxed_slice(),
        cells: cells.into_boxed_slice(),
        borders: borders.into_boxed_slice(),
        repeat: false,
        changed: row.changed,
    }
}

// A spaced cell's own frame
#[allow(clippy::too_many_arguments, reason = "the cell, the row's place and the two lists it draws into")]
fn spaced_cell_frame(
    tb: &TableLayout,
    row: &RowBox,
    ri: usize,
    c: &CellBox,
    spacing: f64,
    x0: f64,
    (top, height): (f64, f64),
    background: &mut Vec<Item>,
    borders: &mut Vec<Item>,
) -> [f64; 4] {
    let frame = spaced_box(tb, row, ri, c, spacing, x0, (top, height));
    let [bx0, by0, bx1, by1] = frame;
    c.shade.draw(background, frame, frame);
    let [t, l, b, r] = drawn_edges(tb, c);
    let vis = |e: &'_ Option<Border>| e.as_ref().filter(|b| b.visible()).map(|b| (thick(b), b.clone()));
    if let Some((w, e)) = vis(t) {
        edge(borders, [bx0, by0, bx1, by0 + w], true, false, &e);
    }
    if let Some((w, e)) = vis(b) {
        edge(borders, [bx0, by1 - w, bx1, by1], true, true, &e);
    }
    if let Some((w, e)) = vis(l) {
        edge(borders, [bx0, by0, bx0 + w, by1], false, false, &e);
    }
    if let Some((w, e)) = vis(r) {
        edge(borders, [bx1 - w, by0, bx1, by1], false, true, &e);
    }
    frame
}

/// A spaced cell's border box.
fn spaced_box(tb: &TableLayout, row: &RowBox, ri: usize, c: &CellBox, spacing: f64, x0: f64, (top, height): (f64, f64)) -> [f64; 4] {
    let (mut lo, mut ro) = (c.col == 0, c.col + c.span >= tb.cols.len());
    if tb.rtl {
        (lo, ro) = (ro, lo);
    }
    [x0 + c.x + gap(spacing, lo), top + row.gap_top, x0 + c.x + c.w - gap(spacing, ro), top + height - gap(spacing, tb.is_last(ri))]
}

// A cell's borders and gridlines
fn cell_borders(tb: &TableLayout, row: &RowBox, c: &CellBox, x0: f64, (top, height): (f64, f64), last_of_table: bool, borders: &mut Vec<Item>) {
    // Borders
    let [t, l, b, r] = drawn_edges(tb, c);
    let (cx0, cx1) = (x0 + c.x, x0 + c.x + c.w);
    let shown = |e: &Option<Border>| e.as_ref().is_some_and(|b| b.visible());
    if !shown(t) && c.v_merge != VMerge::Continue {
        borders.push(Item::Gridline { line: [cx0, top, cx1, top] });
    }
    if !shown(l) {
        borders.push(Item::Gridline { line: [cx0, top, cx0, top + height] });
    }
    if !shown(r) {
        borders.push(Item::Gridline { line: [cx1, top, cx1, top + height] });
    }
    if last_of_table && !shown(b) {
        borders.push(Item::Gridline { line: [cx0, top + height, cx1, top + height] });
    }
    if let Some(t) = t.as_ref().filter(|b| b.visible() && c.v_merge != VMerge::Continue) {
        edge(borders, [cx0 - bw(l) / 2.0, top, cx1 + bw(r) / 2.0, top + thick(t)], true, false, t);
    }
    if let Some(l) = l.as_ref().filter(|b| b.visible()) {
        edge(borders, [cx0 - thick(l) / 2.0, top, cx0 + thick(l) / 2.0, top + height], false, false, l);
    }
    if let Some(r) = r.as_ref().filter(|b| b.visible()) {
        edge(borders, [cx1 - thick(r) / 2.0, top, cx1 + thick(r) / 2.0, top + height], false, true, r);
    }
    // A part's last row before a page break draws the table's last-row bottom edge.
    let b = if last_of_table { &c.cut[1] } else { b };
    if (last_of_table || row.closes)
        && let Some(b) = b.as_ref().filter(|b| b.visible())
    {
        edge(borders, [cx0 - bw(l) / 2.0, top + height, cx1 + bw(r) / 2.0, top + height + thick(b)], true, true, b);
    }
}

// A spaced table's outer frame
fn spacing_frame(tb: &TableLayout, ri: usize, x0: f64, (top, height): (f64, f64), last_of_table: bool, borders: &mut Vec<Item>) {
    // Outer border
    let [t, l, b, r] = &tb.outer;
    let x1 = x0 + tb.cols.iter().sum::<f64>();
    let (lw, rw) = (bw(l), bw(r));
    let bottom = top + height + if last_of_table { tb.bottom_border - bw(b) } else { 0.0 };
    if ri == 0
        && let Some(t) = t.as_ref().filter(|b| b.visible())
    {
        edge(borders, [x0 - lw / 2.0, top, x1 + rw / 2.0, top + thick(t)], true, false, t);
    }
    if let Some(l) = l.as_ref().filter(|b| b.visible()) {
        edge(borders, [x0 - lw / 2.0, top, x0 + lw / 2.0, bottom], false, false, l);
    }
    if let Some(r) = r.as_ref().filter(|b| b.visible()) {
        edge(borders, [x1 - rw / 2.0, top, x1 + rw / 2.0, bottom], false, true, r);
    }
    if last_of_table && let Some(b) = b.as_ref().filter(|b| b.visible()) {
        edge(borders, [x0 - lw / 2.0, bottom, x1 + rw / 2.0, bottom + thick(b)], true, true, b);
    }
}

fn division_boxes(row: &RowBox, rect: [f64; 4], (top, height): (f64, f64), (outer_top, outer_bottom): (f64, f64), borders: &mut Vec<Item>) {
    let side = |b: &Option<Border>| b.as_ref().map_or(0.0, |b| b.space + thick(b));
    let n = row.div_boxes.len();
    let (mut l, mut r) = (rect[0], rect[2]);
    let mut extents = vec![(0.0, 0.0); n];
    for (k, d) in row.div_boxes.iter().enumerate().rev() {
        l -= side(&d.borders[1]);
        r += side(&d.borders[3]);
        extents[k] = (l, r);
    }
    let opening: f64 = row.div_boxes.iter().filter(|d| d.starts).map(|d| side(&d.borders[0])).sum();
    let mut y = top - opening;
    let mut tops = vec![outer_top; n];
    for (k, d) in row.div_boxes.iter().enumerate().filter(|(_, d)| d.starts) {
        tops[k] = y;
        if let Some(b) = &d.borders[0] {
            edge(borders, [extents[k].0, y, extents[k].1, y + thick(b)], true, false, b);
        }
        y += side(&d.borders[0]);
    }
    let mut y = top + height;
    let mut bottoms = vec![outer_bottom; n];
    for (k, d) in row.div_boxes.iter().enumerate().rev().filter(|(_, d)| d.ends) {
        if let Some(b) = &d.borders[2] {
            y += b.space;
            edge(borders, [extents[k].0, y, extents[k].1, y + thick(b)], true, true, b);
            y += thick(b);
        }
        bottoms[k] = y;
    }
    for (k, d) in row.div_boxes.iter().enumerate() {
        if let Some(b) = &d.borders[1] {
            edge(borders, [extents[k].0, tops[k], extents[k].0 + thick(b), bottoms[k]], false, false, b);
        }
        if let Some(b) = &d.borders[3] {
            edge(borders, [extents[k].1 - thick(b), tops[k], extents[k].1, bottoms[k]], false, true, b);
        }
    }
}

// Row cache
#[derive(Debug)]
pub struct DrawnRow(RowFragment);

impl PartialEq for DrawnRow {
    fn eq(&self, o: &Self) -> bool {
        format!("{:?}", self.0) == format!("{:?}", o.0)
    }
}

pub fn draw_whole_row(
    tb: &TableLayout,
    ri: usize,
    left: f64,
    top: f64,
    height: f64,
    last_of_table: bool,
    memos: &mut Memos,
) -> (RowFragment, Option<Rc<DrawnRow>>) {
    let probe = RowProbe { tb, ri, at: [left.to_bits(), height.to_bits()], last: last_of_table };
    let bytes = |d: &DrawnRow| {
        std::mem::size_of::<RowFragment>()
            + d.0.cells.len() * std::mem::size_of::<CellFragment>()
            + (d.0.borders.len() + d.0.background.len()) * std::mem::size_of::<Item>()
    };
    match cached(memos, |c| &mut c.rows, probe, bytes, |_| Ok(DrawnRow(draw_row(tb, ri, left, 0.0, height, None, last_of_table)))) {
        Ok(d) => (crate::layout::fragment::row_down(&d.0, top), Some(d)),
        Err(_) => (crate::layout::fragment::row_down(&draw_row(tb, ri, left, 0.0, height, None, last_of_table), top), None),
    }
}
