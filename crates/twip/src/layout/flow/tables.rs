//! Tables in a column

use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::{Jc, TableAnchor};
use crate::layout::LayoutCtx;
use crate::layout::float::{self, Frames, Placed, Wrap};
use crate::layout::fragment::{BoxFragment, BoxKind, Child, RowFragment, TableFragment};
use crate::layout::ids::ParaId;
use crate::layout::job::{Memos, Yield};
use crate::layout::paragraph::lines_to_take;
use crate::layout::space::ColumnSpace;
use crate::layout::story::{NestedRow, StoryEnv, StoryUnit};
use crate::layout::table::{CellPart, RowBox, TableLayout, draw_row, draw_whole_row, whole};
use crate::layout::text::para_id;
use crate::layout::token::{At, FlowState, FlowToken, NoteArea};
use crate::layout::units::{EPS, Lu};

use super::{ColumnCursor, PageFill, Stop, hold, rebase};

// Row splits
fn cell_cut(units: &[StoryUnit], room: f64, continued: bool, compat: Compat) -> usize {
    let fit = fit_count(units, room, compat);
    if compat == Compat::Legacy || fit == 0 || fit >= units.len() {
        return fit;
    }
    cell_keeps(units, widow_cut(units, fit, continued))
}

fn fit_count(units: &[StoryUnit], room: f64, compat: Compat) -> usize {
    let close = |u: &StoryUnit| match &u.row {
        Some(r) if compat == Compat::Generic && !r.table.is_last(r.ri) => r.table.bottom_border,
        _ => 0.0,
    };
    // A line's floats held in the cell fit with it.
    units.iter().position(|u| u.top + (u.height + u.after + close(u)).max(u.hold) > room + EPS).unwrap_or(units.len())
}

#[derive(Clone)]
struct Cut {
    keep: usize,
    head: Option<StoryUnit>,
    tail: Option<StoryUnit>,
    next: usize,
}

impl Cut {
    fn at(n: usize) -> Cut {
        Cut { keep: n, head: None, tail: None, next: n }
    }

    fn any(&self) -> bool {
        self.keep > 0 || self.head.is_some()
    }

    fn kept<'a>(&self, units: &'a [StoryUnit]) -> Cow<'a, [StoryUnit]> {
        match &self.head {
            None => Cow::Borrowed(&units[..self.keep]),
            Some(h) => Cow::Owned(units[..self.keep].iter().chain([h]).cloned().collect()),
        }
    }

    fn rest(self, units: &[StoryUnit]) -> Vec<StoryUnit> {
        let n = self.next;
        let Some(first) = units.get(n) else { return self.tail.into_iter().collect() };
        let shift = match (&self.tail, n.checked_sub(1).map(|k| &units[k])) {
            (Some(t), Some(p)) => t.height - (p.top + p.height),
            (None, Some(p)) if p.block != first.block => (first.top - (p.top + p.height + p.after)).max(0.0) - first.top,
            _ => -first.top,
        };
        self.tail.into_iter().chain(units[n..].iter().map(|u| StoryUnit { top: u.top + shift, ..u.clone() })).collect()
    }
}

fn cut_cell(units: &[StoryUnit], room: f64, continued: bool, compat: Compat) -> Cut {
    let fit = fit_count(units, room, compat);
    if let Some(u) = units.get(fit)
        && let Some((head, tail)) = split_nested(u, room - u.top, compat)
    {
        return Cut { keep: fit, head: Some(head), tail: Some(tail), next: fit + 1 };
    }
    let n = cell_cut(units, room, continued, compat);
    match n.checked_sub(1).map(|k| &units[k]) {
        Some(u) if n < units.len() && u.row.as_ref().is_some_and(|r| !r.table.is_last(r.ri)) => {
            Cut { keep: n - 1, head: Some(nested_unit(u, None, true)), tail: None, next: n }
        }
        _ => Cut::at(n),
    }
}

fn nested_unit(u: &StoryUnit, parts: Option<Vec<Vec<StoryUnit>>>, close: bool) -> StoryUnit {
    let Some(nr) = &u.row else { return u.clone() };
    let (tb, ri) = (&nr.table, nr.ri);
    let last = close || tb.is_last(ri);
    // The kept part of a split is a continuation when the row was split before; the part left over always is.
    let cont = nr.rest.is_some() || parts.is_some() && !close;
    let held: Option<Rc<[Vec<StoryUnit>]>> = parts.map(Rc::from).or_else(|| nr.rest.clone());
    let cells: Option<Vec<CellPart>> = held.as_ref().map(|r| r.iter().map(|v| CellPart { units: v, base: 0.0, ends: !close, cont }).collect());
    let h = match &cells {
        Some(c) => part_height(&tb.rows[ri], c),
        None => tb.rows[ri].height,
    };
    let row = draw_row(tb, ri, nr.shift, 0.0, h, cells.as_deref(), last);
    let notes: Box<[(NoteKind, i64)]> = match &held {
        Some(r) => r.iter().flatten().flat_map(|x| x.notes.iter().copied()).collect(),
        None => u.notes.clone(),
    };
    StoryUnit {
        height: h + if last { tb.bottom_border } else { 0.0 },
        content: Rc::from([Child::Table(TableFragment { table: tb.id, rows: Box::new([row]) })]),
        notes,
        row: if close { None } else { Some(Rc::new(NestedRow { rest: held, ..(**nr).clone() })) },
        ..u.clone()
    }
}

fn split_nested(u: &StoryUnit, room: f64, compat: Compat) -> Option<(StoryUnit, StoryUnit)> {
    let nr = u.row.as_ref()?;
    let row = &nr.table.rows[nr.ri];
    let avail = room - if compat == Compat::Generic { nr.table.bottom_border } else { 0.0 };
    if row.cant_split || row.exact || row.header && compat == Compat::Legacy || row.min_h > avail + EPS {
        return None;
    }
    let held: Vec<&[StoryUnit]> = match &nr.rest {
        Some(r) => r.iter().map(|v| &v[..]).collect(),
        None => row.cells.iter().map(|c| &c.units.units[..]).collect(),
    };
    let (mut kept, mut left) = (Vec::new(), Vec::new());
    for (c, units) in row.cells.iter().zip(&held) {
        let cut = cut_cell(units, avail - row.top_border - c.margins[0] - c.margins[2], nr.rest.is_some(), compat);
        if !units.is_empty() && !cut.any() {
            return None;
        }
        kept.push(cut.kept(units).into_owned());
        left.push(cut.rest(units));
    }
    if left.iter().all(|l| l.is_empty()) {
        return None;
    }
    let tail = StoryUnit { top: 0.0, ..nested_unit(u, Some(left), false) };
    Some((nested_unit(u, Some(kept), true), tail))
}

fn widow_cut(units: &[StoryUnit], fit: usize, continued: bool) -> usize {
    if units[fit - 1].block != units[fit].block || !units[fit].widow {
        return fit;
    }
    let para = units[fit].block;
    let start = units[..fit].iter().rposition(|u| u.block != para).map_or(0, |k| k + 1);
    let end = units[fit..].iter().position(|u| u.block != para).map_or(units.len(), |k| fit + k);
    let first_part = !(continued && start == 0);
    start + lines_to_take(end - start, fit - start, false, first_part, false, true)
}

fn cell_keeps(units: &[StoryUnit], mut keep: usize) -> usize {
    let lines = |k: usize| {
        let b = units[k].block;
        let start = units[..=k].iter().rposition(|u| u.block != b).map_or(0, |i| i + 1);
        let end = units[k..].iter().position(|u| u.block != b).map_or(units.len(), |i| k + i);
        (start, end)
    };
    if keep == 0 || keep >= units.len() {
        return keep;
    }
    let (start, _) = lines(keep);
    if units[keep].keep_lines && start < keep {
        keep = start;
    }
    while keep > 0 && keep < units.len() {
        let (p, i) = (&units[keep - 1], &units[keep]);
        if p.block == i.block || !p.keep_next {
            break;
        }
        let (start, end) = lines(keep - 1);
        let n = keep - start;
        let mut m = n;
        if !p.keep_lines {
            m = n.min(if p.widow && end - start > 1 { 2 } else { 1 });
            if p.widow && n - m == 1 {
                m = n;
            }
        }
        if m > keep {
            break;
        }
        keep -= m;
        if m < n {
            break;
        }
    }
    keep
}

// Merged regions
struct MergedSplit {
    k: usize,
    start: usize,
    kr: usize,
    pi: usize,
    cut: usize,
    bottom: f64,
}

fn merged_splits(tb: &TableLayout, ri: usize, placed: &[(usize, f64, usize)], bottom: f64, compat: Compat) -> Vec<MergedSplit> {
    let continues = |r: usize, col: usize| tb.rows.get(r).is_some_and(|row| row.cells.iter().any(|c| c.col == col && c.v_merge == VMerge::Continue));
    let mut out = Vec::new();
    for (k, c) in tb.rows[ri].cells.iter().enumerate() {
        if c.v_merge != VMerge::Continue || continues(ri + 1, c.col) {
            continue;
        }
        let Some(pi) = placed.iter().rposition(|p| !continues(p.0, c.col)) else { continue };
        let (start, top, _) = placed[pi];
        let Some(kr) = tb.rows[start].cells.iter().position(|x| x.col == c.col && x.v_merge == VMerge::Restart) else { continue };
        let rc = &tb.rows[start].cells[kr];
        if rc.v_align != "top" || tb.rows[start].exact || !(start + 1..ri).all(|r| continues(r, c.col)) {
            continue;
        }
        let content_top = top + tb.rows[start].top_border + rc.margins[0];
        let units = &rc.units.units;
        let cut = cell_cut(units, bottom - rc.margins[2] - content_top, false, compat);
        if cut == 0 || cut == units.len() {
            continue;
        }
        let end = units[cut - 1].top + units[cut - 1].height;
        out.push(MergedSplit { k, start, kr, pi, cut, bottom: content_top + end + rc.margins[2] });
    }
    out
}

fn merged_page_cuts(tb: &TableLayout, next: usize, placed: &[(usize, f64, usize)], compat: Compat) -> Vec<MergedSplit> {
    let Some(&(last, last_top, _)) = placed.last() else { return Vec::new() };
    let continues = |r: usize, col: usize| tb.rows.get(r).is_some_and(|row| row.cells.iter().any(|c| c.col == col && c.v_merge == VMerge::Continue));
    let mut out = Vec::new();
    for (k, c) in tb.rows.get(next).map_or(&[][..], |r| &r.cells[..]).iter().enumerate() {
        if c.v_merge != VMerge::Continue {
            continue;
        }
        let Some(pi) = placed.iter().rposition(|p| !continues(p.0, c.col)) else { continue };
        let (start, top, _) = placed[pi];
        let Some(kr) = tb.rows[start].cells.iter().position(|x| x.col == c.col && x.v_merge == VMerge::Restart) else { continue };
        let rc = &tb.rows[start].cells[kr];
        if tb.rows[start].exact || !(start + 1..=last).all(|r| continues(r, c.col)) {
            continue;
        }
        let content_top = top + tb.rows[start].top_border + rc.margins[0];
        let units = &rc.units.units;
        let bottom = last_top + tb.rows[last].height;
        let cut = cell_cut(units, bottom - rc.margins[2] - content_top, false, compat);
        // The region's part ends at the last whole row; the content kept there is aligned in it.
        out.push(MergedSplit { k, start, kr, pi, cut, bottom });
    }
    out
}

fn merged_rest(tb: &TableLayout, ri: usize, compat: Compat) -> Vec<Option<Vec<StoryUnit>>> {
    tb.rows[ri]
        .cells
        .iter()
        .map(|c| {
            if c.v_merge != VMerge::Continue {
                return None;
            }
            let start = (0..ri).rev().find(|&r| tb.rows[r].cells.iter().any(|x| x.col == c.col && x.v_merge == VMerge::Restart))?;
            let rc = tb.rows[start].cells.iter().find(|x| x.col == c.col && x.v_merge == VMerge::Restart)?;
            // The rest goes on at the top of the next part, however the region is aligned.
            if tb.rows[start].exact {
                return None;
            }
            let room = tb.rows[start..ri].iter().map(|r| r.height).sum::<f64>() - tb.rows[start].top_border - rc.margins[0] - rc.margins[2];
            let units = &rc.units.units;
            let cut = cell_cut(units, room, false, compat);
            (cut > 0 && cut < units.len()).then(|| rebase(&units[cut..]))
        })
        .collect()
}

fn draw_merged_part(tb: &TableLayout, ms: &[&MergedSplit], ri: usize, h: f64, left: f64, top: f64) -> crate::layout::fragment::RowFragment {
    let start = ms[0].start;
    let mut rows = tb.rows.clone();
    for m in ms {
        rows[start].cells[m.kr].below = tb.rows[start + 1..ri].iter().map(|r| r.height).sum::<f64>() + h;
    }
    let t = TableLayout { rows, cols: tb.cols.clone(), outer: tb.outer.clone(), fit: tb.fit.clone(), ..*tb };
    let mut parts = whole(&t.rows[start]);
    for m in ms {
        let units = &tb.rows[start].cells[m.kr].units.units;
        parts[m.kr] = CellPart { units: &units[..m.cut], base: 0.0, ends: m.cut == units.len(), cont: false };
    }
    draw_row(&t, start, left, top, tb.rows[start].height, Some(&parts), false)
}

fn part_height(row: &crate::layout::table::RowBox, parts: &[CellPart]) -> f64 {
    let end = |p: &CellPart| p.units.last().map_or(0.0, |u| u.after);
    let content = row.cells.iter().zip(parts).map(|(c, p)| c.margins[0] + p.height() + end(p) + c.margins[2]).fold(0.0, f64::max);
    // Each part of an atLeast row is at least the row's height.
    (row.top_border + content + row.bottom_gap).max(row.min_h)
}

/// The paragraphs of cells, those of nested tables included.
fn cell_paras<'b>(rows: &'b [crate::docx::model::Row], out: &mut Vec<&'b crate::docx::model::Paragraph>) {
    for b in rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()) {
        match b {
            Block::Para(p) => out.push(p),
            Block::Table(t) => cell_paras(&t.rows, out),
        }
    }
}

fn page_floats<'b>(t: &'b Table, ctx: &LayoutCtx<'b>) -> HashMap<u64, &'b Float> {
    let mut out = HashMap::new();
    let mut paras = Vec::new();
    cell_paras(&t.rows, &mut paras);
    for p in paras {
        for f in ctx.floats_of(p) {
            if !f.in_cell(Compat::Legacy) {
                out.insert(f.id, f);
            }
        }
    }
    out
}

/// `pinned`: the page tops of the floats that keep their place whatever their row's.
fn move_floats(
    row: Option<&mut crate::layout::fragment::RowFragment>,
    floats: &HashMap<u64, &Float>,
    pinned: &HashMap<u64, f64>,
    fr: &Frames,
    up: f64,
) {
    let Some(row) = row.filter(|_| !floats.is_empty()) else { return };
    let top = row.top;
    for cell in row.cells.iter_mut() {
        for c in cell.content.iter_mut() {
            let Child::Group { offset, children } = c else { continue };
            if !children.iter().any(|k| matches!(k, Child::Box(b) if matches!(b.kind, BoxKind::Float { float, .. } if floats.contains_key(&float.0))))
            {
                continue;
            }
            let moved: Vec<Child> = children
                .iter()
                .map(|k| match k {
                    Child::Box(b) => match b.kind {
                        BoxKind::Float { anchor, float, rect, layer } if floats.contains_key(&float.0) => {
                            let f = floats[&float.0];
                            let x = float::h_pos(f, fr) - offset.0;
                            // `paragraph` counts from the row's top (its top border included), whichever paragraph of the
                            // cell holds the anchor.
                            let y = match f.v_relative.as_str() {
                                _ if pinned.contains_key(&float.0) => pinned[&float.0] - offset.1,
                                "paragraph" | "line" => (if f.v_align.is_some() { 0.0 } else { f.v_offset }) - (offset.1 - top) - up,
                                _ => float::v_pos(f, rect[3] - rect[1], fr, 0.0, 0.0) - offset.1,
                            };
                            let d = (x - rect[0], y - rect[1]);
                            Child::Box(BoxFragment {
                                kind: BoxKind::Float { anchor, float, layer, rect: [rect[0] + d.0, rect[1] + d.1, rect[2] + d.0, rect[3] + d.1] },
                                offset: (b.offset.0 + d.0, b.offset.1 + d.1),
                                ..b.clone()
                            })
                        }
                        _ => k.clone(),
                    },
                    _ => k.clone(),
                })
                .collect();
            *children = moved.into();
        }
    }
}

// Cell cuts
type CellCuts<'u> = (Vec<(Cow<'u, [StoryUnit]>, bool)>, Vec<Vec<StoryUnit>>);

// A floating table's place
struct Floated {
    at_x: f64,
    settled: bool,
    first: Option<(usize, f64)>,
    wrap_x: Option<f64>,
}

impl<'d> PageFill<'d, '_> {
    // Tables
    pub(super) fn place_table(&mut self, tb: &TableLayout, cur: &mut ColumnCursor, col: &ColumnSpace, memos: &mut Memos) -> Result<Stop, Yield> {
        let (pos, carry, y, empty, area, out) = cur.parts();
        if tb.rows.is_empty() {
            pos.next_block();
            return Ok(Stop::Done);
        }
        if pos.at.at_start() && !*empty {
            *y += self.after_above_table(pos.block, carry);
        }
        // A vertical section's upright tables have their own placement.
        let floating = matches!(self.sec.blocks.get(pos.block), Some(Block::Table(t)) if t.floating.is_some());
        let page_floats = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) if self.ctx.env.text.compat == Compat::Legacy => page_floats(t, &self.ctx),
            _ => HashMap::new(),
        };
        let frames = Frames {
            page_w: self.page_w,
            page_h: self.page_h,
            margin: [self.margins.0, self.margins.1, self.margins.2, self.sec.section.section_in(self.ctx.env.text.review.view).bottom],
            column: (col.x, col.width),
            compat: self.ctx.env.text.compat,
            cell: false,
            odd: self.odd,
        };
        let first_h = tb.rows.first().map_or(0.0, |r| r.height);
        let y0 = *y;
        let (row_floats, held) = self.held_row_floats(tb, pos.block, pos.at.at_start(), floating, &page_floats, &frames, y0);
        let pinned: HashMap<u64, f64> = held.iter().map(|(_, a)| (a.float.id, a.rect[1])).collect();
        // A tight or through polygon of a single point is drawn but leaves the rows where they are.
        let moves = |a: &Placed| !a.float.poly.as_ref().is_some_and(|p| p.iter().all(|q| *q == p[0]));
        // The first row goes beside those over it (a later row's too) when the table fits the first piece they leave,
        // else below them.
        let mut over: Vec<Placed> = held.iter().map(|(_, a)| a.clone()).collect();
        let spot = if pos.at.at_start() && !floating && self.ctx.env.text.vertical.is_none() {
            let hits: Vec<Placed> =
                held.iter().filter(|(r, a)| moves(a) && (*r > 0 || a.rect[1] - a.dist[0] - y0 < first_h)).map(|(_, a)| a.clone()).collect();
            let ri = tb.rows.iter().position(|r| !r.hidden).unwrap_or(0);
            self.beside_floats(pos.block, tb, col, *y, &hits, ri, true, memos)?
        } else {
            None
        };
        let (beside, relaid) = match spot {
            Some((x, top, relaid)) => {
                *y = top;
                (Some(x), relaid)
            }
            None => (None, None),
        };
        let lift = if over.is_empty() { 0.0 } else { *y - y0 };
        let tb: &TableLayout = relaid.as_deref().unwrap_or(tb);
        let mut rows = Vec::new();
        // A spaced table's outer frame is closed at the top and bottom of each part.
        let finish = |mut rows: Vec<RowFragment>, out: &mut Vec<Child>, left: f64, opened: Option<(f64, bool)>, closed: bool| {
            if let Some(o) = opened
                && let Some(first) = rows.iter_mut().find(|r| !r.repeat)
            {
                crate::layout::table::frame_part(tb, first, left, Some(o), false);
            }
            if closed && let Some(last) = rows.last_mut() {
                crate::layout::table::frame_part(tb, last, left, None, true);
            }
            if !rows.is_empty() {
                out.push(Child::Table(TableFragment { table: tb.id, rows: rows.into_boxed_slice() }));
            }
        };
        // A vertical section's tables are placed upright by their caller, floating or not.
        let fp = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) if self.ctx.env.text.vertical.is_none() => t.floating,
            _ => None,
        };
        let (flow_y, was_empty) = (*y, *empty);
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let unsplit = fp.is_some() && legacy && self.ctx.env.text.do_not_break_wrapped_tables;
        let mut at_x = match fp {
            Some(fp) if fp.h_page => match fp.x_align() {
                Some(Jc::Center) => (self.page_w - col.width) / 2.0,
                Some(Jc::Right) => self.page_w - col.width,
                _ => 0.0,
            },
            // A continuation keeps the place its first part took beside floats.
            // ponytail: the place only; a compat ≤ 14 autofit table relaid at the piece's width goes on at the column
            // layout.
            _ => beside.or_else(|| carry.beside_at.filter(|_| !pos.at.at_start()).map(|d| col.x + d)).unwrap_or(col.x),
        };
        let mut settled = true;
        let mut first: Option<(usize, f64)> = None;
        // Where the text wraps a floating table, when it is drawn elsewhere.
        let mut wrap_x = None;
        if let Some(fp) = fp
            && pos.at.at_start()
        {
            match self.place_floating(fp, tb, col, pos, carry, y, empty, at_x, (flow_y, was_empty), unsplit)? {
                Ok(f) => (at_x, settled, first, wrap_x) = (f.at_x, f.settled, f.first, f.wrap_x),
                Err(stop) => return Ok(stop),
            }
        }
        // An exact row is cut to the column's own height, whatever bottom the table's first part has.
        let column_h = if unsplit { f64::INFINITY } else { col.bottom.to_pt() - col.top.to_pt() };
        let tall;
        let col = match first {
            _ if unsplit => {
                tall = ColumnSpace { bottom: Lu::MAX, ..*col };
                &tall
            }
            // Compat ≤ 14 draws the first part whole, past the column bottom; compat 15 cuts it there.
            Some((_, hn)) => {
                let end = Lu::round_half_up(*y + hn + 0.01);
                tall = ColumnSpace { bottom: if legacy { end } else { end.min(col.bottom) }, ..*col };
                &tall
            }
            None => col,
        };
        let part_top = *y;
        carry.prev = None;
        carry.prev_after = 0.0;
        carry.balanced_after = 0.0;
        let headers: Vec<usize> =
            (0..tb.rows.len()).take_while(|&r| tb.rows[r].header || tb.rows[r].hidden).filter(|&r| !tb.rows[r].hidden).collect();
        let n = tb.rows.len();
        // A continuation goes on from its first part's place under the page's floats: compat ≤ 14 goes beside or
        // below those its first row's band meets there, as a first part does; compat 15 keeps its place and moves
        // down below every float the band of its rows on the page meets there.
        let part_x = at_x;
        let repeats = *empty && !pos.at.at_start() && !headers.contains(&pos.at.index());
        if fp.is_none() && !pos.at.at_start() && self.ctx.env.text.vertical.is_none() && !tb.rtl {
            let ri = if repeats { headers.first().copied().unwrap_or(pos.at.index()) } else { pos.at.index() };
            let (c0, c1) = crate::layout::table::cells_box(tb);
            let floats = self.page_floats_for(pos.block, true);
            if legacy {
                let band = tb.rows[ri].height + if tb.is_last(ri) { tb.bottom_border } else { 0.0 };
                if floats
                    .iter()
                    .any(|f| f.in_band(*y, *y + band) && (f.wrap == Wrap::TopAndBottom || f.excl[2] > at_x + c0 + EPS && f.excl[0] < at_x + c1 - EPS))
                    && let Some((x, top, _)) = self.beside_floats(pos.block, tb, col, *y, &[], ri, false, memos)?
                {
                    (at_x, *y) = (x, top);
                }
            } else {
                while let Some(e) =
                    float::next_edge(&floats, at_x + c0, at_x + c1, *y, *y + Self::part_band(tb, ri, *y, col)).filter(|&e| e > *y + EPS)
                {
                    *y = e;
                }
            }
        }
        // Header rows
        if repeats {
            for &h in &headers {
                let hh = tb.rows[h].height;
                let mut row = draw_row(tb, h, at_x, *y, hh, None, false);
                row.repeat = true;
                rows.push(row);
                *y += hh;
            }
        }
        let later: Vec<(f64, f64)> = {
            let (l, r) = (at_x + tb.x0, at_x + tb.x0 + tb.cols.iter().sum::<f64>());
            row_floats
                .iter()
                .filter(|(f, _, row, top, _)| {
                    *row == 0 && f.v_relative == "paragraph" && *top >= first_h && {
                        let x = float::h_pos(f, &frames);
                        x < r && x + f.w > l
                    }
                })
                .map(|(_, _, _, top, bottom)| (y0 + top, y0 + bottom))
                .collect()
        };
        let up = |r: usize| if r == 0 { lift } else { 0.0 };
        let mut placed_here = 0;
        let mut placed: Vec<(usize, f64, usize)> = Vec::new();
        self.at.starts.push((*y, pos.at.at_start()));
        let below_content = pos.at.at_start() && !*empty;
        let mut keep: Option<(usize, usize, f64, NoteArea, usize)> = None;
        let mut opened = None;
        // The note references of the rows placed, by their index in `rows`.
        let mut row_refs: Vec<(usize, (NoteKind, i64))> = Vec::new();
        let cell_refs = |parts: &[CellPart], k: usize| -> Vec<(usize, (NoteKind, i64))> {
            parts.iter().flat_map(|cu| cu.units.iter().flat_map(|u| u.notes.iter().map(move |&r| (k, r)))).collect()
        };
        while pos.at.index() < n {
            let ri = pos.at.index();
            let row = &tb.rows[ri];
            if row.hidden {
                pos.at = At::Row(ri + 1);
                continue;
            }
            if rows.is_empty() && ri > 0 && (row.part_top != 0.0 || row.spacing.is_none()) {
                opened = Some((*y, false));
                *y += row.part_top;
            } else if legacy && opened.is_none() && !rows.is_empty() && rows.iter().all(|r| r.repeat) && row.spacing.is_none() {
                opened = Some((*y, true));
                *y += row.header_top;
            }
            let last = tb.is_last(ri);
            let rest = pos.at.rest().cloned();
            let at_top = *empty || placed_here == 0 && rest.is_some();
            let keeps = row.keep_next || self.ctx.env.text.compat == Compat::Generic && row.header && rest.is_none();
            let breaks = row.break_before
                && rest.is_none()
                && !*empty
                && !(self.ctx.env.text.compat == Compat::Generic && ri > 0 && tb.rows[ri - 1].keep_next);
            let carried =
                if rest.is_none() && placed_here == 0 && !pos.at.at_start() { merged_rest(tb, ri, self.ctx.env.text.compat) } else { Vec::new() };
            let mut parts: Vec<CellPart> = match &rest {
                Some(r) => r.iter().map(|v| CellPart { units: &v[..], base: 0.0, ends: true, cont: true }).collect(),
                None => whole(row),
            };
            for (k, c) in carried.iter().enumerate() {
                if let Some(u) = c {
                    parts[k] = CellPart { units: &u[..], base: 0.0, ends: true, cont: true };
                }
            }
            let carries = carried.iter().any(Option::is_some);
            let height = if rest.is_some() { part_height(row, &parts) } else { row.height };
            let height = if row.exact && rest.is_none() && self.ctx.env.text.compat == Compat::Legacy {
                height.min((column_h - row.top_border - tb.bottom_border).max(0.0))
            } else {
                height
            };
            // Compat ≤ 14: a later row whose band meets one of the page's floats where the part stands goes beside or
            // below the floats there as a first row does, and the rows after it on the page keep that place; the rows
            // before it close their part.
            if legacy && fp.is_none() && placed_here > 0 && self.ctx.env.text.vertical.is_none() && !tb.rtl {
                let band = height + if last { tb.bottom_border } else { 0.0 };
                let (c0, c1) = crate::layout::table::cells_box(tb);
                let own: Vec<Placed> = held.iter().filter(|(r, a)| *r != ri && *r > 0 && moves(a)).map(|(_, a)| a.clone()).collect();
                let floats: Vec<Placed> = self.page_floats_for(pos.block, true).into_iter().chain(own.iter().cloned()).collect();
                // ponytail: an autofit table keeps its layout at the new place.
                if floats
                    .iter()
                    .any(|f| f.in_band(*y, *y + band) && (f.wrap == Wrap::TopAndBottom || f.excl[2] > at_x + c0 + EPS && f.excl[0] < at_x + c1 - EPS))
                    && let Some((x, top, _)) = self.beside_floats(pos.block, tb, col, *y, &own, ri, false, memos)?
                {
                    if let Some(&(rs, rtop, idx)) = placed.last().filter(|p| p.2 + 1 == rows.len()) {
                        let (mut drawn, _) = draw_whole_row(tb, rs, at_x, rtop, tb.rows[rs].height, true, memos);
                        move_floats(Some(&mut drawn), &page_floats, &pinned, &frames, up(rs));
                        rows[idx] = drawn;
                    }
                    (at_x, *y) = (x, top + row.part_top);
                }
            }
            if rest.is_none()
                && !rows.is_empty()
                && let Some(&(_, bottom)) = later.iter().find(|&&(t, b)| *y < b - EPS && t < *y + height - EPS)
            {
                if let Some(&(rs, top, idx)) = placed.last().filter(|p| p.2 + 1 == rows.len()) {
                    let (mut drawn, _) = draw_whole_row(tb, rs, at_x, top, tb.rows[rs].height, true, memos);
                    move_floats(Some(&mut drawn), &page_floats, &pinned, &frames, up(rs));
                    rows[idx] = drawn;
                }
                *y = bottom;
            }
            let refs: Vec<(NoteKind, i64)> = parts.iter().flat_map(|cu| cu.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            let mut a = area.clone();
            let mut note_rest = Vec::new();
            // A framed row fits with the table's bottom frame, any other with its own cells' bottom borders.
            let allowance = if row.spacing.is_some() { tb.bottom_border } else { row.fit_border };
            let notes_ok = self.add_notes(&refs, &mut a, col, *y + height + allowance, &mut note_rest, false, memos)?;
            if !breaks && notes_ok && col.bottom.holds(*y + height + allowance + a.total() - a.total().min(0.0)) {
                if !keeps {
                    keep = None;
                } else if keep.is_none() && (!*empty || !rows.is_empty()) {
                    keep = Some((ri, rows.len(), *y, area.clone(), carry.note_rest.len()));
                }
                *area = a;
                carry.note_rest.extend(note_rest);
                if rest.is_none() {
                    placed.push((ri, *y, rows.len()));
                }
                row_refs.extend(cell_refs(&parts, rows.len()));
                rows.push(match &rest {
                    None if carries => draw_row(tb, ri, at_x, *y, height, Some(&parts), last),
                    None => {
                        let (drawn, cached) = draw_whole_row(tb, ri, at_x, *y, height, last, memos);
                        if let Some(d) = &cached {
                            hold(&mut self.pass.held.rows, d);
                        }
                        drawn
                    }
                    Some(_) => draw_row(tb, ri, at_x, *y, height, Some(&parts), last),
                });
                move_floats(rows.last_mut(), &page_floats, &pinned, &frames, up(ri));
                *y += height + if last { tb.bottom_border } else { 0.0 };
                *empty = false;
                pos.at = At::Row(ri + 1);
                placed_here += 1;
                continue;
            }
            // Row split
            let avail = col.bottom.to_pt() - area.total() - *y - row.top_border - tb.bottom_border;
            let no_split = row.cant_split || row.header || keeps;
            let can_split = !breaks
                && !row.exact
                && (!no_split || at_top && self.ctx.env.text.compat == Compat::Generic)
                && (at_top || rest.is_some() || row.min_h <= avail + row.top_border + EPS);
            let (fitted, mut left) = if can_split {
                self.cut_cells(tb, row, &parts, avail, rest.is_some(), at_top, *y, area, col, memos)?
            } else {
                (Vec::new(), Vec::new())
            };
            let fitted: Vec<CellPart> =
                fitted.iter().zip(&parts).map(|((units, ends), p)| CellPart { units, base: 0.0, ends: *ends, cont: p.cont }).collect();
            let merged = if can_split && rest.is_none() {
                merged_splits(tb, ri, &placed, col.bottom.to_pt() - area.total() - tb.bottom_border, self.ctx.env.text.compat)
            } else {
                Vec::new()
            };
            for m in &merged {
                left[m.k] = rebase(&tb.rows[m.start].cells[m.kr].units.units[m.cut..]);
            }
            let any = fitted.iter().any(|f| !f.units.is_empty());
            let every = fitted.iter().zip(&parts).all(|(f, u)| u.units.is_empty() || !f.units.is_empty());
            let h = merged.iter().map(|m| m.bottom - *y).fold(part_height(row, &fitted), f64::max);
            let refs: Vec<(NoteKind, i64)> = fitted.iter().flat_map(|f| f.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            let (mut a, mut note_rest) = (area.clone(), Vec::new());
            if can_split
                && any
                && every
                && (refs.is_empty() || self.add_notes(&refs, &mut a, col, *y + h + tb.bottom_border, &mut note_rest, false, memos)? || at_top)
            {
                *area = a;
                carry.note_rest.extend(note_rest);
                for m in merged.iter().filter(|m| merged.iter().find(|o| o.start == m.start).is_some_and(|o| std::ptr::eq(o, *m))) {
                    let (rs, top, idx) = placed[m.pi];
                    let same: Vec<&MergedSplit> = merged.iter().filter(|o| o.start == m.start).collect();
                    let mut redrawn = draw_merged_part(tb, &same, ri, h, at_x, top);
                    move_floats(Some(&mut redrawn), &page_floats, &pinned, &frames, up(rs));
                    rows[idx] = redrawn;
                }
                row_refs.extend(cell_refs(&fitted, rows.len()));
                rows.push(draw_row(tb, ri, at_x, *y, h, Some(&fitted), true));
                move_floats(rows.last_mut(), &page_floats, &pinned, &frames, up(ri));
                *y += h + tb.bottom_border;
                *empty = false;
                pos.at = At::RowPart { row: ri, rest: Rc::new(left) };
                self.show_endnote_refs(row_refs.into_iter().map(|r| r.1));
                self.at.found.append(&mut over);
                finish(rows, out, at_x, opened, false);
                self.table_obstacle(pos.block, tb, wrap_x.unwrap_or(at_x), part_top, *y, flow_y, settled);
                carry.beside_at = (fp.is_none() && (part_x - col.x).abs() > EPS).then_some(part_x - col.x);
                return Ok(self.text_goes_on(fp.is_some(), pos, carry, y, empty, (flow_y, was_empty)));
            }
            if at_top {
                row_refs.extend(cell_refs(&parts, rows.len()));
                rows.push(draw_row(tb, ri, at_x, *y, height, Some(&parts), last));
                move_floats(rows.last_mut(), &page_floats, &pinned, &frames, up(ri));
                *y += height + if last { tb.bottom_border } else { 0.0 };
                *empty = false;
                pos.at = At::Row(ri + 1);
                placed_here += 1;
                continue;
            }
            if let Some((kri, kidx, y0, area0, notes0)) = keep {
                *y = y0;
                *area = area0;
                carry.note_rest.truncate(notes0);
                rows.truncate(kidx);
                row_refs.retain(|r| r.0 < kidx);
                if rows.is_empty() && below_content {
                    pos.at = At::Start;
                    return Ok(Stop::Full);
                }
                pos.at = At::Row(kri);
            }
            if let At::Row(next) = pos.at {
                let kept: Vec<(usize, f64, usize)> = placed.iter().copied().filter(|p| p.2 < rows.len()).collect();
                let cuts = merged_page_cuts(tb, next, &kept, self.ctx.env.text.compat);
                for m in cuts.iter().filter(|m| cuts.iter().find(|o| o.start == m.start).is_some_and(|o| std::ptr::eq(o, *m))) {
                    let (rs, top, idx) = kept[m.pi];
                    let last = kept.last().map_or(rs, |p| p.0);
                    let same: Vec<&MergedSplit> = cuts.iter().filter(|o| o.start == m.start).collect();
                    let mut redrawn = draw_merged_part(tb, &same, last + 1, 0.0, at_x, top);
                    move_floats(Some(&mut redrawn), &page_floats, &pinned, &frames, up(rs));
                    rows[idx] = redrawn;
                }
            }
            let split = !rows.is_empty();
            if split {
                *y += tb.bottom_border;
            }
            self.show_endnote_refs(row_refs.into_iter().map(|r| r.1));
            if !rows.is_empty() {
                self.at.found.append(&mut over);
            }
            finish(rows, out, at_x, opened, true);
            self.table_obstacle(pos.block, tb, wrap_x.unwrap_or(at_x), part_top, *y, flow_y, settled);
            carry.beside_at = (fp.is_none() && split && (part_x - col.x).abs() > EPS).then_some(part_x - col.x);
            return Ok(self.text_goes_on(fp.is_some() && split, pos, carry, y, empty, (flow_y, was_empty)));
        }
        self.show_endnote_refs(row_refs.into_iter().map(|r| r.1));
        if !rows.is_empty() {
            self.at.found.append(&mut over);
        }
        finish(rows, out, at_x, opened, false);
        if fp.is_some() {
            self.table_obstacle(pos.block, tb, wrap_x.unwrap_or(at_x), part_top, *y, flow_y, settled);
            (*y, *empty) = (flow_y, was_empty);
        }
        pos.next_block();
        carry.first_para = false;
        carry.beside_at = None;
        Ok(Stop::Done)
    }

    #[allow(clippy::too_many_arguments, reason = "the table, where it starts, its page-placed floats and the page")]
    #[allow(clippy::type_complexity, reason = "the rows' floats as the row loop reads them")]
    fn held_row_floats<'f>(
        &self,
        tb: &TableLayout,
        bi: usize,
        at_start: bool,
        floating: bool,
        page_floats: &HashMap<u64, &'f Float>,
        frames: &Frames,
        y0: f64,
    ) -> (Vec<(&'f Float, ParaId, usize, f64, f64)>, Vec<(usize, Placed)>) {
        // Compat ≤ 14: the wrapping floats of the rows placed from the page, each from its row's top where the rows'
        // first placement puts it. The first row's over it and the later rows' below it.
        let row_floats: Vec<(&Float, ParaId, usize, f64, f64)> = match self.sec.blocks.get(bi) {
            Some(Block::Table(t)) if !floating && at_start && !page_floats.is_empty() => (0..t.rows.len().min(tb.rows.len()))
                .flat_map(|r| {
                    let mut paras = Vec::new();
                    cell_paras(&t.rows[r..r + 1], &mut paras);
                    paras.into_iter().map(move |p| (r, p))
                })
                .flat_map(|(r, p)| p.inlines.iter().map(move |i| (r, para_id(p), i)))
                .filter_map(|(r, id, i)| if let Inline::Float(f, _) = i { page_floats.get(&f.id).map(|f| (*f, id, r)) } else { None })
                .filter(|(f, ..)| {
                    f.wrap != "none" && matches!(f.v_relative.as_str(), "paragraph" | "margin" | "page") && f.h_align.is_none() && f.v_align.is_none()
                })
                .map(|(f, id, r)| (f, id, r, f.v_offset - f.dist[0], f.v_offset + f.h + f.dist[1]))
                .collect(),
            _ => Vec::new(),
        };
        // They are in the page's geometry: the text before the table wraps those reaching above it when the page is
        // filled again, and they keep the place their rows' first placement gives them when the rows then move.
        let row_top = |r: usize| y0 + tb.rows[..r].iter().filter(|r| !r.hidden).map(|r| r.height).sum::<f64>();
        let held: Vec<(usize, Placed)> = row_floats
            .iter()
            .enumerate()
            .map(|(k, (f, id, r, _, _))| {
                let key = (self.sec.index, bi, k);
                let y = if f.v_relative == "paragraph" { row_top(*r) + f.v_offset } else { float::v_pos(f, f.h, frames, 0.0, 0.0) };
                let at = (float::h_pos(f, frames), y);
                let mut a = Placed::new(key, *id, &Rc::new((*f).clone()), at.0, at.1, f.h);
                a.collided = a.excl[1] < y0 - EPS;
                (*r, a)
            })
            .collect();
        (row_floats, held)
    }

    // Cell cuts of a split row
    #[allow(clippy::too_many_arguments, reason = "the row, its cells' parts, and the room and note area they are cut for")]
    fn cut_cells<'u>(
        &mut self,
        tb: &TableLayout,
        row: &RowBox,
        parts: &[CellPart<'u>],
        avail: f64,
        continued: bool,
        at_top: bool,
        y: f64,
        area: &NoteArea,
        col: &ColumnSpace,
        memos: &mut Memos,
    ) -> Result<CellCuts<'u>, Yield> {
        let mut fitted: Vec<(Cow<[StoryUnit]>, bool)> = Vec::new();
        let mut left: Vec<Vec<StoryUnit>> = Vec::new();
        let compat = self.ctx.env.text.compat;
        let (mut kept, mut low) = (Vec::new(), 0.0f64);
        for (k, cu) in parts.iter().enumerate() {
            let m = &row.cells[k].margins;
            let room = avail - m[0] - m[2];
            let first = cut_cell(cu.units, room, continued, compat);
            let mut cut = first.clone();
            while cut.any() {
                let held = cut.kept(cu.units);
                let u = &held[held.len() - 1];
                let bottom = (y + row.top_border + m[0] + u.top + u.height + u.after + m[2] + tb.bottom_border).max(low);
                let mut refs = kept.clone();
                refs.extend(held.iter().flat_map(|u| u.notes.iter().copied()));
                if refs.is_empty() || self.add_notes(&refs, &mut area.clone(), col, bottom, &mut Vec::new(), false, memos)? {
                    (kept, low) = (refs, bottom);
                    break;
                }
                let next = cut_cell(cu.units, u.top, continued, compat);
                let (a, b) = (next.keep + next.head.is_some() as usize, cut.keep + cut.head.is_some() as usize);
                cut = if a < b { next } else { Cut::at(b - 1) };
            }
            if !cut.any() && at_top {
                cut = first;
            }
            let held = cut.kept(cu.units);
            let ends = cu.ends && cut.head.is_none() && held.len() == cu.units.len();
            fitted.push((held, ends));
            left.push(cut.rest(cu.units));
        }
        Ok((fitted, left))
    }

    // Floating table position
    #[allow(clippy::too_many_arguments, reason = "the table, its column place, and where the flow stood before it")]
    fn place_floating(
        &mut self,
        fp: crate::docx::props::TablePos,
        tb: &TableLayout,
        col: &ColumnSpace,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        y: &mut f64,
        empty: &mut bool,
        mut at_x: f64,
        (flow_y, was_empty): (f64, bool),
        unsplit: bool,
    ) -> Result<Result<Floated, Stop>, Yield> {
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let mut first: Option<(usize, f64)> = None;
        let mut wrap_x = None;
        let mut settled = self.at.found.iter().all(|f| self.at.known.iter().any(|k| k.same(f)));
        let h: f64 = tb.rows.iter().filter(|r| !r.hidden).map(|r| r.height).sum::<f64>() + tb.bottom_border;
        let (t0, t1) = match fp.v_anchor {
            TableAnchor::Page => (0.0, self.page_h),
            _ => (self.margins.2, self.page_h - self.sec.section.section_in(self.ctx.env.text.review.view).bottom),
        };
        let mut top = match (fp.v_anchor, fp.y_spec) {
            (TableAnchor::Text, _) => flow_y + fp.y,
            (_, Some(Jc::Center)) => (t0 + t1 - h) / 2.0,
            (_, Some(Jc::Right)) => t1 - h,
            (_, Some(_)) => t0,
            (_, None) => t0 + fp.y - 0.05,
        }
        .max(0.0);
        // A page-anchored table's first part holds the rows that fit from where it stands in the flow to the
        // column bottom, and it is moved up so they end on the paper.
        // ponytail: whole rows only, and only `tblpY` against the page.
        if fp.v_anchor == TableAnchor::Page && fp.y_spec.is_none() {
            let mut part = (0, tb.bottom_border);
            for r in tb.rows.iter().filter(|r| !r.hidden) {
                if !col.bottom.holds(flow_y + part.1 + r.height) {
                    break;
                }
                part = (part.0 + 1, part.1 + r.height);
            }
            if part.0 > 0 {
                top = top.min(self.page_h - part.1).max(0.0);
                first = Some(part);
            }
        }
        let (bl, br) = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) => crate::layout::table::float_box(tb, t, legacy),
            _ => (0.0, 0.0),
        };
        let [dt, db, ..] = fp.dist;
        // Compat ≤ 14 puts a table placed in the flow, from the nearest twip, below an earlier table whose obstacle
        // its text box (a twip left of `tblpX`, the indent added to its width) reaches.
        let in_flow = legacy && fp.v_anchor == TableAnchor::Text && fp.y == 0.0;
        let tables: Vec<Placed> = self.at.found.iter().filter(|o| o.key.2 == float::TABLE_PIECE).cloned().collect();
        let (c0, c1) = crate::layout::table::cells_box(tb);
        let shift = if fp.x_align().is_none() && !fp.h_text { 0.05 } else { 0.0 };
        let ind = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) => t.ind,
            _ => 0.0,
        };
        let (f0, f1) = (at_x + c0 - shift - ind, at_x + c1 - shift);
        // A table below which it went is not avoided again for what the rounding left of their overlap.
        let mut under = Vec::new();
        for o in tables.iter().filter(|_| in_flow) {
            if f0 < o.excl[2] - EPS && o.excl[0] < f1 - EPS && o.excl[3].min(top + h + db) - o.excl[1].max(top - dt) >= 9.0 {
                top = (o.excl[3] * 20.0).round() / 20.0;
                under.push(o.key);
            }
        }
        if let Some(Block::Table(t)) = self.sec.blocks.get(pos.block) {
            let f = float::table_float(t.id, br - bl, h, float::table_dist(&fp, in_flow), !fp.never_overlap);
            let mut probe = Placed::new((self.sec.index, pos.block, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, h);
            let body_bottom = self.page_h - self.sec.section.section_in(self.ctx.env.text.review.view).bottom;
            // It moves off the tables and floats placed before it that it may not overlap, or that may not overlap it.
            let frames = self.at.found.iter().filter(|o| o.key.2 != float::TABLE_PIECE);
            let others: Vec<Placed> = tables.iter().filter(|o| !under.contains(&o.key)).chain(frames).cloned().collect();
            // Its area: the box its horizontal and vertical anchors name.
            let across = match () {
                _ if fp.h_page => (0.0, self.page_w),
                _ if fp.h_text => (col.x, col.x + col.width),
                _ => (self.margins.0, self.page_w - self.margins.1),
            };
            let down = if fp.v_anchor == TableAnchor::Page { (0.0, self.page_h) } else { (self.margins.2, body_bottom) };
            let asked = at_x;
            float::avoid(&mut probe, &others, across, down, self.ctx.env.text.compat);
            (at_x, top) = (probe.rect[0] - bl, probe.rect[1]);
            // Compat 15: a table moved clear of others whose box meets the text laid out before it on the page (the
            // page is filled again) is drawn at the x it asked for and the y it was moved to; the text wraps the
            // place it was moved to.
            // ponytail: "meets" is its box reaching into the band from the column top to where the flow reached it
            // on a page with text before it, not a test against those lines.
            let meets = !was_empty && probe.excl[1] < flow_y - EPS && probe.excl[3] > col.top.to_pt() + EPS;
            if !legacy && meets && (asked - at_x).abs() > EPS {
                wrap_x = Some(at_x);
                at_x = asked;
            }
        }
        // A table in the flow is placed again from the flow on every fill. Another keeps the place of the last
        // fill, in compat 15 only when it met the text laid out before it there.
        let flowing = fp.v_anchor == TableAnchor::Text && fp.y == 0.0;
        // A kept place stays for the rest of the page, however the floats before it moved.
        *y = match self.at.known.iter().find(|k| k.key == (self.sec.index, pos.block, float::TABLE_PIECE)) {
            Some(k) if !flowing && (legacy || k.restarts) => {
                settled = true;
                k.rect[1]
            }
            _ => top,
        };
        if unsplit && !*empty && !col.bottom.holds(*y + h) {
            return Ok(Err(Stop::Full));
        }
        // Compat 15: one placed above where it stands that doesn't hold every row in its first part goes to the
        // next page (where the text after it goes on is as for a split table).
        if let Some((n, _)) = first
            && !legacy
            && !*empty
            && *y + 0.05 < flow_y - EPS
            && n < tb.rows.iter().filter(|r| !r.hidden).count()
        {
            if self.ctx.env.text.text_after_table_break {
                carry.prev = None;
                carry.prev_after = 0.0;
                carry.balanced_after = 0.0;
            }
            return Ok(Err(self.text_goes_on(true, pos, carry, y, empty, (flow_y, was_empty))));
        }
        Ok(Ok(Floated { at_x, settled, first, wrap_x }))
    }

    /// The page's floats a table part placed in the flow goes beside or below: all but those of its own block; with
    /// `before`, only the header's and footer's and those anchored before it.
    fn page_floats_for(&self, bi: usize, before: bool) -> Vec<Placed> {
        let si = self.sec.index;
        let at = |f: &&Placed| if before { (f.key.0, f.key.1) < (si, bi) } else { (f.key.0, f.key.1) != (si, bi) };
        self.fixed.iter().chain(self.known_at(bi).filter(at)).cloned().collect()
    }

    /// The height of the rows from `ri` on that fit in the column from `top`: at least the first.
    fn part_band(tb: &TableLayout, ri: usize, top: f64, col: &ColumnSpace) -> f64 {
        let mut h = 0.0;
        for (k, r) in tb.rows.iter().enumerate().skip(ri).filter(|(_, r)| !r.hidden) {
            let next = h + r.height + if tb.is_last(k) { tb.bottom_border } else { 0.0 };
            if h > 0.0 && !col.bottom.holds(top + next) {
                break;
            }
            h = next;
        }
        h
    }

    /// Where an in-flow table's part starting at row `ri` at `y` goes beside the floats over its band: in the first
    /// free piece of the column there, its indent from the piece's start and its `jc` within the piece, when the row
    /// fits the piece; else lower, at the first float bottom where it does. Compat ≤ 14 tests the row's own band
    /// and only the first piece, and lays an autofit table out again at the piece's width when `relay`; compat 15
    /// tests the band of the rows the column holds from there and takes the first piece the table fits. `None` when
    /// no float meets the band.
    #[allow(clippy::too_many_arguments, reason = "as place_table")]
    fn beside_floats(
        &mut self,
        bi: usize,
        tb: &TableLayout,
        col: &ColumnSpace,
        y: f64,
        own: &[Placed],
        ri: usize,
        relay: bool,
        memos: &mut Memos,
    ) -> Result<Option<(f64, f64, Option<Rc<TableLayout>>)>, Yield> {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { return Ok(None) };
        // A part placed again on a later page or from a later row meets only the floats already on the page.
        let floats: Vec<Placed> = self.page_floats_for(bi, !relay).into_iter().chain(own.iter().cloned()).collect();
        if tb.rtl || floats.is_empty() {
            return Ok(None);
        }
        let text = &self.ctx.env.text;
        let legacy = text.compat == Compat::Legacy;
        let (cx0, cx1) = (col.x, col.x + col.width);
        let band = |top: f64| {
            if legacy { tb.rows[ri].height + if tb.is_last(ri) { tb.bottom_border } else { 0.0 } } else { Self::part_band(tb, ri, top, col) }
        };
        let mut top = y;
        let mut moved = false;
        while top < col.bottom.to_pt() - EPS {
            let h = band(top);
            let pieces = float::intervals(&floats, cx0, cx1, top, top + h, legacy).unwrap_or_default();
            if pieces.first().is_some_and(|&(u, r)| u <= cx0 + EPS && r >= cx1 - EPS) {
                break;
            }
            for &(u, r) in pieces.iter().take(if legacy { 1 } else { usize::MAX }) {
                let w = r - u;
                let relaid = if relay && legacy && !t.fixed && !text.no_constrained_autofit {
                    let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
                    let prev = self.sec.blocks.neighbours(bi, bi, text.review.view).0;
                    let laid = crate::layout::table::layout(&crate::layout::table::in_column(t, col.width, &env), w, &env, prev, false, memos)?;
                    hold(&mut self.pass.held.tables, &laid);
                    Some(laid)
                } else {
                    None
                };
                let shift = match t.jc {
                    _ if relaid.is_some() => 0.0,
                    Jc::Center => (w - col.width) / 2.0,
                    Jc::Right => w - col.width,
                    _ => 0.0,
                };
                let l = relaid.as_deref().unwrap_or(tb);
                if let Some(row) = l.rows.get(ri)
                    && let (Some(a), Some(z)) = (row.cells.first(), row.cells.last())
                {
                    // The width the row reports: from the piece's start to its last cell's content right edge, or with
                    // `layoutRawTableWidth` from its first cell's left edge to its last cell's right edge.
                    // ponytail: a table relaid at the piece's width counts as pulled left by its margin, unset indent
                    // or not.
                    let x0 = if relaid.is_some() { l.x0.min(-a.margins[1]) } else { l.x0 };
                    let reported = if text.raw_table_width { z.x + z.w - a.x } else { x0 + shift + z.x + z.w - z.margins[3] };
                    if reported <= w + EPS {
                        return Ok(Some((u + shift, top, relaid)));
                    }
                }
            }
            match float::next_edge(&floats, cx0, cx1, top, top + band(top)) {
                Some(e) if e > top + EPS => (top, moved) = (e, true),
                _ => break,
            }
        }
        Ok(moved.then_some((cx0, top, None)))
    }

    /// Compat 15 with `allowTextAfterFloatingTableBreak`: after a floating table's part that doesn't end it, the text
    /// goes on beside the part, from where the table started; the rest of the table opens the next column.
    fn text_goes_on(&self, floating: bool, pos: &mut FlowToken, carry: &mut FlowState, y: &mut f64, empty: &mut bool, from: (f64, bool)) -> Stop {
        if !floating || !self.ctx.env.text.text_after_table_break {
            return Stop::Full;
        }
        carry.table_rest = Some(pos.clone());
        pos.next_block();
        (*y, *empty) = from;
        Stop::Done
    }

    #[allow(clippy::too_many_arguments, reason = "the part's place and the attempt state it was placed in")]
    fn table_obstacle(&mut self, bi: usize, tb: &TableLayout, at_x: f64, top: f64, bottom: f64, flow_y: f64, settled: bool) {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { return };
        let Some(fp) = t.floating.filter(|_| self.ctx.env.text.vertical.is_none()) else { return };
        if bottom <= top + EPS {
            return;
        }
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let in_flow = legacy && fp.v_anchor == TableAnchor::Text && fp.y == 0.0;
        let (bl, br) = crate::layout::table::float_box(tb, t, legacy);
        let f = float::table_float(t.id, br - bl, bottom - top, float::table_dist(&fp, in_flow), !fp.never_overlap);
        let mut o = Placed::new((self.sec.index, bi, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, bottom - top);
        // Compat ≤ 14 keeps the obstacle below where the table stands; compat 15's top distance reaches the text above.
        if legacy {
            o.excl[1] = o.excl[1].max(flow_y.min(top));
        }
        // One twip left of `tblpX`, in either mode.
        if fp.x_align().is_none() && !fp.h_text {
            o.excl[0] -= 0.05;
            o.excl[2] -= 0.05;
        }
        // Compat ≤ 14 draws the cells the indent further along but keeps the obstacle's left edge.
        if legacy {
            o.excl[0] -= t.ind;
        }
        o.settled = settled;
        let known = self.at.known.iter().find(|k| k.key == o.key);
        // It met the text before it when it stands above where the flow reached; a kept one keeps that.
        // ponytail: above the flow, not tested against that text's lines.
        o.restarts = known.map_or(top < flow_y - EPS, |k| k.restarts);
        if fp.v_anchor == TableAnchor::Text && fp.y == 0.0 {
            // Compat ≤ 14: the text before a table in the flow wraps the box it first had on the page; compat 15:
            // only the strip its top distance reaches above it.
            o.early_box = match known {
                Some(k) if legacy => k.early_box.or(Some((k.rect, k.excl))),
                _ if legacy => None,
                _ => Some((o.rect, [o.excl[0], o.excl[1], o.excl[2], o.excl[1].max(o.rect[1])])),
            };
        }
        self.at.found.push(o);
    }
}
