//! Column filling

use crate::docx::Compat;
use std::borrow::Cow;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Jc, LineRule, TableAnchor};

use super::LayoutCtx;
use super::float::{self, Frames, Placed, Wrap};
use super::fragment::{BoxFragment, BoxKind, Child, Item, ParaFragment, PlacedLine, RowFragment, TableFragment};
use super::ids::ParaId;
use super::job::{Memos, Yield};
use super::join::{Joined, LaidPara, laid_para, piece_para};
use super::line::{
    Frame, Line, LineOrigin, ParaLines, apply_borders, band, border_key, break_lines, breaks_only, draw_label, draw_line, neighbour_borders,
    top_border_extra,
};
use super::node::{Notes, SectionNode};
use super::paragraph::{gap, lines_to_take, space_after, space_before};
use super::record::PageEffects;
use super::space::{BorderKey, ColumnSpace, ParaSpace};
use super::story::{NestedRow, StoryEnv, StoryLayout, StoryUnit};
use super::table::{CellPart, DrawnRow, TableLayout, draw_row, draw_whole_row, whole};
use super::text::{FieldValues, FontMetrics, Kind, Object, ShapedPara, para_id, shape};

/// Default footnote separator line metrics.
const SEPARATOR_METRICS: FontMetrics = FontMetrics { ascent: 1950.0, descent: 550.0, leading: 0.0, upem: 2048.0 };
use super::token::{At, FlowState, FlowToken, NotePart};
use super::units::Lu;

const EPS: f64 = 1e-6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    Done,
    Full,
    Column,
    Page,
}

pub struct ColumnResult {
    pub children: Vec<Child>,
    pub stop: Stop,
    pub bottom: f64,
    pub token: FlowToken,
    pub state: FlowState,
}

#[derive(Clone, Default, Debug)]
pub struct NoteArea {
    pub parts: Vec<NotePart>,
    /// How many of `parts` continue notes from an earlier column or page.
    pub carried: usize,
    pub height: f64,
    pub sep: f64,
    pub cont: bool,
    pub shared: Option<f64>,
    pub cols: usize,
    pub width: Option<f64>,
    pub below: bool,
}

impl NoteArea {
    pub fn total(&self) -> f64 {
        if self.below {
            return 0.0;
        }
        self.sep + self.shared.map_or(self.height, |r| self.height.max(r))
    }
}

#[derive(Default)]
pub struct PassMemo {
    tables: HashMap<(usize, u64), Rc<TableLayout>>,
    varies: HashMap<usize, bool>,
    joined: HashMap<(usize, usize), Rc<Joined>>,
    mirrored: HashMap<(usize, usize), LaidPara<'static>>,
    pub(crate) held: Held,
}

#[derive(Clone, Debug, Default)]
pub struct Held {
    lines: Vec<Rc<ParaLines>>,
    tables: Vec<Rc<TableLayout>>,
    rows: Vec<Rc<DrawnRow>>,
}

fn hold<T>(v: &mut Vec<Rc<T>>, x: &Rc<T>) {
    if !v.iter().any(|y| Rc::ptr_eq(y, x)) {
        v.push(x.clone());
    }
}

impl PassMemo {
    pub fn table(&self, t: &Table, width: f64) -> Option<Rc<TableLayout>> {
        self.tables.get(&(std::ptr::from_ref(t) as usize, width.to_bits())).cloned()
    }

    pub fn seed(&mut self, t: &Table, width: f64, tb: Rc<TableLayout>) {
        self.tables.insert((std::ptr::from_ref(t) as usize, width.to_bits()), tb);
    }
}

fn varies_by_page(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::FieldBegin { kind: Some(FieldKind::Page | FieldKind::Section | FieldKind::SectionPages), .. })),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| varies_by_page(&c.blocks))),
    })
}

#[derive(Clone, Default)]
pub struct AttemptState {
    pub known: Vec<Placed>,
    pub found: Vec<Placed>,
    pub stream: Option<NoteArea>,
    pub forced: bool,
    pub starts: Vec<(f64, bool)>,
    pub notes_drawn: bool,
    /// The bottom of the column footnote areas drawn under their text (`beneathText`).
    pub notes_bottom: f64,
    pub effects: PageEffects,
    pub keep_cap: Option<(usize, usize)>,
    pub end: Option<FlowToken>,
}

pub struct PageFill<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub sec: &'p SectionNode<'d>,
    pub notes: Notes<'d>,
    pub fields: &'p FieldValues,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    pub page_w: f64,
    pub page_h: f64,
    pub margins: (f64, f64, f64),
    pub pass: &'p mut PassMemo,
    pub at: AttemptState,
    pub fixed: Vec<Placed>,
    pub odd: bool,
    /// Paragraphs with `w:mirrorIndents` mirror on this page.
    pub mirror: bool,
}

pub fn para_space(
    style: &ParaStyle,
    prev: Option<&ParaStyle>,
    next: Option<&ParaStyle>,
    width: f64,
    from: usize,
    x0: f64,
    ctx: &LayoutCtx,
) -> ParaSpace {
    let borders: (BorderKey, BorderKey) = match border_key(style) {
        None => (None, None),
        Some(_) => {
            let (p, n) = neighbour_borders(style, prev, next);
            (if from > 0 { super::line::continued_key(style) } else { p }, n)
        }
    };
    ParaSpace { width, start: from, borders, x0, env: ctx.env.text.clone(), fit: false }
}

pub fn note_cut(units: &[StoryUnit], room: f64) -> usize {
    let mut fit = units.iter().position(|x| x.top + x.height > room + EPS).unwrap_or(units.len());
    if fit == 0 || fit >= units.len() || units[fit - 1].block != units[fit].block || !units[fit].widow {
        return fit;
    }
    let para = units[fit].block;
    let start = units[..fit].iter().rposition(|u| u.block != para).map_or(0, |k| k + 1);
    let end = units[fit..].iter().position(|u| u.block != para).map_or(units.len(), |k| fit + k);
    let (before, after) = (fit - start, end - fit);
    if after < 2 {
        fit = fit.saturating_sub(2 - after);
    }
    if fit - start < 2 && before > 0 {
        fit = start;
    }
    fit
}

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
    units.iter().position(|u| u.top + u.height + u.after + close(u) > room + EPS).unwrap_or(units.len())
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
    let held: Option<Rc<[Vec<StoryUnit>]>> = parts.map(Rc::from).or_else(|| nr.rest.clone());
    let cells: Option<Vec<CellPart>> = held.as_ref().map(|r| r.iter().map(|v| CellPart { units: v, base: 0.0, ends: !close }).collect());
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
        if rc.v_align != "top" || tb.rows[start].exact || !(start + 1..=last).all(|r| continues(r, c.col)) {
            continue;
        }
        let content_top = top + tb.rows[start].top_border + rc.margins[0];
        let units = &rc.units.units;
        let bottom = last_top + tb.rows[last].height;
        let cut = cell_cut(units, bottom - rc.margins[2] - content_top, false, compat);
        if cut == units.len() {
            continue;
        }
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
            if rc.v_align != "top" || tb.rows[start].exact {
                return None;
            }
            let room = tb.rows[start..ri].iter().map(|r| r.height).sum::<f64>() - tb.rows[start].top_border - rc.margins[0] - rc.margins[2];
            let units = &rc.units.units;
            let cut = cell_cut(units, room, false, compat);
            (cut > 0 && cut < units.len()).then(|| rebase(&units[cut..]))
        })
        .collect()
}

fn draw_merged_part(tb: &TableLayout, ms: &[&MergedSplit], ri: usize, h: f64, left: f64, top: f64) -> super::fragment::RowFragment {
    let start = ms[0].start;
    let mut rows = tb.rows.clone();
    for m in ms {
        rows[start].cells[m.kr].below = tb.rows[start + 1..ri].iter().map(|r| r.height).sum::<f64>() + h;
    }
    let t = TableLayout { rows, cols: tb.cols.clone(), outer: tb.outer.clone(), fit: tb.fit.clone(), ..*tb };
    let mut parts = whole(&t.rows[start]);
    for m in ms {
        parts[m.kr] = CellPart { units: &tb.rows[start].cells[m.kr].units.units[..m.cut], base: 0.0, ends: false };
    }
    draw_row(&t, start, left, top, tb.rows[start].height, Some(&parts), false)
}

pub fn rebase(units: &[StoryUnit]) -> Vec<StoryUnit> {
    let first = units.first().map_or(0.0, |u| u.top);
    units.iter().map(|x| StoryUnit { top: x.top - first, ..x.clone() }).collect()
}

// Note streams
pub fn stream_fill(notes: &[NotePart], ncols: usize, h: f64) -> Option<(Vec<Vec<NotePart>>, f64)> {
    let mut cols: Vec<Vec<NotePart>> = vec![Vec::new()];
    let (mut used, mut tallest) = (0.0, 0.0f64);
    for note in notes {
        let mut part = note.clone();
        loop {
            let total = part.height();
            if total <= h - used + EPS {
                used += total;
                cols.last_mut()?.push(part);
                break;
            }
            let fit = note_cut(&part.units, h - used);
            if fit > 0 {
                used += part.units[fit - 1].top + part.units[fit - 1].height;
                cols.last_mut()?.push(NotePart { units: part.units[..fit].to_vec(), ..part.clone() });
                part = NotePart { units: rebase(&part.units[fit..]), ..part };
            }
            tallest = tallest.max(used);
            if cols.len() == ncols {
                return None;
            }
            cols.push(Vec::new());
            used = 0.0;
        }
    }
    Some((cols, tallest.max(used)))
}

pub fn stream_height(notes: &[NotePart], ncols: usize, room: f64) -> Option<f64> {
    if notes.is_empty() {
        return Some(0.0);
    }
    stream_fill(notes, ncols, room)?;
    let (mut lo, mut hi) = (0.0, room);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if stream_fill(notes, ncols, mid).is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    stream_fill(notes, ncols, hi).map(|(_, t)| t)
}

fn part_height(row: &super::table::RowBox, parts: &[CellPart]) -> f64 {
    let end = |p: &CellPart| p.units.last().map_or(0.0, |u| u.after);
    let content = row.cells.iter().zip(parts).map(|(c, p)| c.margins[0] + p.height() + end(p) + c.margins[2]).fold(0.0, f64::max);
    row.top_border + content + row.bottom_gap
}

pub fn place_units(units: &[StoryUnit], x: f64, y: f64, out: &mut Vec<Child>) {
    for u in units {
        out.push(Child::Group { offset: (x, y + u.top), children: u.content.clone() });
    }
}

fn page_floats<'b>(t: &'b Table, ctx: &LayoutCtx<'b>) -> HashMap<u64, &'b Float> {
    let mut out = HashMap::new();
    for b in t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()) {
        let Block::Para(p) = b else { continue };
        for f in ctx.floats_of(p) {
            if !f.in_cell(Compat::Legacy) {
                out.insert(f.id, f);
            }
        }
    }
    out
}

fn move_floats(row: Option<&mut super::fragment::RowFragment>, floats: &HashMap<u64, &Float>, fr: &Frames, top_border: f64, up: f64) {
    let Some(row) = row.filter(|_| !floats.is_empty()) else { return };
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
                            let y = match f.v_relative.as_str() {
                                "paragraph" | "line" => rect[1] - top_border - up,
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

impl<'d> PageFill<'d, '_> {
    fn laid(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
        let (laid, end) = self.laid_own(bi)?;
        let view = self.ctx.env.text.review.view;
        let style = laid.para().para_style_in(view);
        if !self.mirror || !style.mirror_indents || style.frame.is_some() {
            return Some((laid, end));
        }
        let key = (std::ptr::from_ref(laid.para()) as usize, end - bi);
        if let Some(m) = self.pass.mirrored.get(&key) {
            return Some((m.clone(), end));
        }
        // A mirrored page: the right indent takes the first line's start, the left the right indent (plus a hanging
        // indent); the first-line indent stays.
        let mut s = (**style).clone();
        (s.ind_left, s.ind_right) = (style.ind_right + (-style.ind_first).max(0.0), style.ind_left + style.ind_first);
        s.change = None;
        let m = laid.restyled(Rc::new(s));
        self.pass.mirrored.insert(key, m.clone());
        Some((m, end))
    }

    fn laid_own(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
        let Some(Block::Para(p)) = self.sec.blocks.get(bi) else { return None };
        let end = self.sec.blocks.chain_end(bi, self.ctx.env.text.review.view);
        if end == bi {
            return Some((LaidPara::One(p), bi));
        }
        let key = (std::ptr::from_ref(p) as usize, end - bi);
        let j = match self.pass.joined.get(&key) {
            Some(j) => j.clone(),
            None => {
                let Some(LaidPara::Joined(j)) = laid_para(&self.sec.blocks.blocks[bi..=end]) else { return Some((LaidPara::One(p), bi)) };
                self.pass.joined.insert(key, j.clone());
                j
            }
        };
        Some((LaidPara::Joined(j), end))
    }

    #[allow(clippy::too_many_arguments, reason = "the paragraph, its blocks, and the line space it is laid out in")]
    fn para_lines(
        &mut self,
        laid: &LaidPara,
        bi: usize,
        end: usize,
        width: f64,
        from: usize,
        x0: f64,
        memos: &mut Memos,
    ) -> Result<Rc<ParaLines>, Yield> {
        let p = laid.para();
        let view = self.ctx.env.text.review.view;
        let (prev, next) = self.sec.blocks.neighbours(bi, end, view);
        let style = p.para_style_in(view);
        let breaks_first = |b: Option<&Block>| {
            let Some(Block::Para(q)) = b else { return false };
            let mut shown = q.inlines.iter().filter(|i| !matches!(i, Inline::Bookmark(..) | Inline::Range(..)));
            matches!(shown.next(), Some(Inline::Break(crate::docx::model::BreakKind::Page | crate::docx::model::BreakKind::Column, _)))
        };
        let prev = prev.filter(|_| !breaks_first(self.sec.blocks.get(bi)));
        let next = next.filter(|_| !breaks_first(self.sec.blocks.get(end + 1)));
        let next = next.or_else(|| self.sec.next.filter(|_| end + 1 >= self.sec.blocks.len()));
        let space = laid.shape_space(self.fields, self.note_numbers, &self.ctx);
        let shaped = shape(p, &space, self.ctx.fonts, memos)?;
        let mut ps = para_space(style, prev, next, width, from, x0, &self.ctx);
        if from > 0 && shaped.pieces[..from.min(shaped.pieces.len())].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak)) {
            ps.borders.0 = super::line::neighbour_borders(style, prev, None).0;
        }

        let lines = break_lines(&shaped, p, &ps, self.ctx.fonts, memos)?;
        hold(&mut self.pass.held.lines, &lines);
        Ok(lines)
    }

    fn table_layout(&mut self, bi: usize, width: f64, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { unreachable!("table_layout on a paragraph block") };
        let id = std::ptr::from_ref(t) as usize;
        let varies = *self.pass.varies.entry(id).or_insert_with(|| t.rows.iter().any(|r| r.cells.iter().any(|c| varies_by_page(&c.blocks))));
        if !varies && let Some(tb) = self.pass.tables.get(&(id, width.to_bits())) {
            hold(&mut self.pass.held.tables, tb);
            return Ok(tb.clone());
        }
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        let prev = self.sec.blocks.neighbours(bi, bi, self.ctx.env.text.review.view).0;
        let tb = crate::prof::time("table", || super::table::layout(t, width, &env, prev, false, memos))?;
        hold(&mut self.pass.held.tables, &tb);
        if !varies {
            self.pass.tables.insert((id, width.to_bits()), tb.clone());
        }
        Ok(tb)
    }

    // Footnotes
    fn note_units(&mut self, kind: NoteKind, id: i64, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let blocks: Vec<&Block> = self.notes.blocks(kind, id).map(|b| b.iter().collect()).unwrap_or_default();
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        super::story::layout(&blocks, width, &env, None, memos)
    }

    /// The separator story `name` laid out at `width`, when the document has it.
    fn separator_story(&mut self, name: &str, width: f64, memos: &mut Memos) -> Result<Option<Rc<StoryLayout>>, Yield> {
        let Some(b) = self.notes.separators.get(name) else { return Ok(None) };
        let blocks: Vec<&Block> = b.iter().collect();
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        super::story::layout(&blocks, width, &env, None, memos).map(Some)
    }

    /// Whether `pos` is inside an endnote that began on an earlier page.
    fn continues_endnote(&self, pos: &FlowToken) -> bool {
        let s = &self.sec.blocks.stories;
        s.len() > 1 && pos.block >= s[1] && !(pos.at.at_start() && s[1..].contains(&pos.block))
    }

    fn separator(&mut self, cont: bool, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let name = if cont { "footnote:continuationSeparator" } else { "footnote:separator" };
        let s = match self.separator_story(name, width, memos)? {
            Some(s) => s,
            None => Rc::new({
                let (a, d, e) = super::text::metrics_from(SEPARATOR_METRICS, 11.0);
                let h = a + d + e;
                let rule = Item::rect([0.0, h * 0.78 - 2.7, 144.0, h * 0.78 - 2.1], 0);
                let content: Rc<[Child]> = Rc::from([Child::Items(Box::new([rule]))]);
                StoryLayout {
                    units: vec![StoryUnit {
                        top: 0.0,
                        height: h,
                        below: 0.0,
                        after: 0.0,
                        content,
                        block: 0,
                        widow: false,
                        keep_next: false,
                        keep_lines: false,
                        notes: Box::new([]),
                        row: None,
                    }],
                    height: h,
                    float_bottom: 0.0,
                }
            }),
        };
        Ok(s)
    }

    /// Adds the footnotes `refs` of a line ending at `line_bottom` to `area`, the parts that don't fit to `rest`; whether
    /// the line fits. `defer`: the line starts its paragraph's part in the column (or may not be parted from it), so in
    /// compat ≤ 14 a note that can't start here waits whole for the next page when a new note before it on the page
    /// was placed whole.
    #[allow(clippy::too_many_arguments, reason = "the line's notes, the area and column they go in, and where the line ends")]
    fn add_notes(
        &mut self,
        refs: &[(NoteKind, i64)],
        area: &mut NoteArea,
        col: &ColumnSpace,
        line_bottom: f64,
        rest: &mut Vec<NotePart>,
        defer: bool,
        memos: &mut Memos,
    ) -> Result<bool, Yield> {
        if !refs.iter().any(|r| r.0 == NoteKind::Foot) {
            return Ok(col.bottom.holds(line_bottom + area.total()));
        }
        let mut a = area.clone();
        if a.sep == 0.0 {
            a.sep = self.separator(false, col.width, memos)?.height;
        }
        if a.shared.is_some() {
            for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                let u = self.note_units(k, id, a.width.unwrap_or(col.width), memos)?;
                a.parts.push(NotePart { kind: k, id, units: u.units.clone() });
            }
            if a.below {
                *area = a;
                return Ok(col.bottom.holds(line_bottom));
            }
            let Some(h) = stream_height(&a.parts, a.cols, col.bottom.to_pt() - line_bottom - a.sep) else { return Ok(false) };
            a.height = h;
            if !col.bottom.holds(line_bottom + a.total()) {
                return Ok(false);
            }
            *area = a;
            return Ok(true);
        }
        let mut new_rest = Vec::new();
        let mut deferred = false;
        for &(k, id) in refs {
            if k != NoteKind::Foot {
                continue;
            }
            let u = self.note_units(k, id, col.width, memos)?;
            let (units, total) = (&u.units, u.height);
            if deferred {
                new_rest.push(NotePart { kind: k, id, units: units.clone() });
                continue;
            }
            let room = col.bottom.to_pt() - line_bottom - a.total();
            let first_para = units.iter().take_while(|x| x.block == units[0].block).count();
            let min = if units.first().is_some_and(|x| x.widow) { 2.min(first_para) } else { 1.min(first_para) };
            let min_h = units.get(min.saturating_sub(1)).map_or(0.0, |x| x.top + x.height);
            let fit = if total <= room + EPS { units.len() } else { note_cut(units, room) };
            if min_h > room + EPS || !new_rest.is_empty() || fit < min {
                let waits = defer
                    && new_rest.is_empty()
                    && a.parts.len() > a.carried
                    && self.ctx.env.text.compat == Compat::Legacy
                    && col.bottom.holds(line_bottom + a.total());
                if !waits {
                    return Ok(false);
                }
                deferred = true;
                new_rest.push(NotePart { kind: k, id, units: units.clone() });
                continue;
            }
            if fit == units.len() {
                a.parts.push(NotePart { kind: k, id, units: units.clone() });
                a.height += total;
                continue;
            }
            let h = units[fit - 1].top + units[fit - 1].height;
            a.parts.push(NotePart { kind: k, id, units: units[..fit].to_vec() });
            a.height += h;
            new_rest.push(NotePart { kind: k, id, units: rebase(&units[fit..]) });
        }
        *area = a;
        rest.extend(new_rest);
        Ok(true)
    }

    fn continue_notes(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        if carry.note_rest.is_empty() {
            return Ok(());
        }
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        let mut rest = Vec::new();
        for part in std::mem::take(&mut carry.note_rest) {
            let room = col.bottom.to_pt() - col.top.to_pt() - area.total();
            let fit = part.units.iter().position(|x| x.top + x.height > room + EPS).unwrap_or(part.units.len());
            if fit == 0 || !rest.is_empty() {
                rest.push(part);
                continue;
            }
            area.height += part.units[fit - 1].top + part.units[fit - 1].height;
            if fit < part.units.len() {
                rest.push(NotePart { units: rebase(&part.units[fit..]), ..part.clone() });
            }
            area.parts.push(NotePart { units: part.units[..fit].to_vec(), ..part });
        }
        carry.note_rest = rest;
        area.carried = area.parts.len();
        Ok(())
    }

    pub fn notes_box(&mut self, area: &NoteArea, cols: &[(f64, &[NotePart])], width: f64, top: f64, memos: &mut Memos) -> Result<Child, Yield> {
        let sep = self.separator(area.cont, width, memos)?;
        let mut children = Vec::new();
        let mut sep_children = Vec::new();
        let x0 = cols.first().map_or(0.0, |c| c.0);
        place_units(&sep.units, x0, top, &mut sep_children);
        children.push(Child::Box(BoxFragment {
            kind: BoxKind::Separator,
            id: None,
            offset: (0.0, 0.0),
            size: (width, sep.height),
            children: sep_children.into(),
        }));
        for (x, parts) in cols {
            let mut y = top + sep.height;
            for part in parts.iter() {
                let mut nc = Vec::new();
                place_units(&part.units, *x, y, &mut nc);
                self.at.effects.notes_shown.push((part.kind, part.id, self.note_numbers.get(&(part.kind, part.id)).cloned().unwrap_or_default()));
                children.push(Child::Box(BoxFragment {
                    kind: BoxKind::Note { kind: part.kind, id: part.id },
                    id: None,
                    offset: (0.0, 0.0),
                    size: (width, part.height()),
                    children: nc.into(),
                }));
                y += part.height();
            }
        }
        Ok(Child::Box(BoxFragment { kind: BoxKind::Notes, id: None, offset: (0.0, 0.0), size: (width, area.total()), children: children.into() }))
    }

    /// The top of a footnote area of height `h` in `col`: at the column bottom; with `beneathText`, at `text` (the
    /// bottom of the column's text), below the floats it would overlap in compat 15, and at the bottom when it
    /// doesn't fit there.
    pub fn note_top(&self, h: f64, col: &ColumnSpace, text: f64) -> f64 {
        let bottom = col.bottom.to_pt() - h;
        if !self.sec.section.notes[0].beneath {
            return bottom;
        }
        let mut top = text.max(col.top.to_pt());
        if self.ctx.env.text.compat == Compat::Generic {
            let (x0, x1) = (col.x, col.x + col.width);
            while let Some(f) = self
                .at
                .found
                .iter()
                .chain(&self.fixed)
                .find(|f| f.wrap != Wrap::None && f.rect[1] < top + h - EPS && f.rect[3] > top + EPS && f.rect[0] < x1 && f.rect[2] > x0)
            {
                top = f.rect[3];
            }
        }
        if top > bottom + EPS { bottom } else { top }
    }

    fn draw_notes(&mut self, area: &NoteArea, col: &ColumnSpace, text: f64, out: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
        if area.parts.is_empty() {
            return Ok(());
        }
        self.at.notes_drawn = true;
        let y = self.note_top(area.total(), col, text) + 0.15;
        if self.sec.section.notes[0].beneath {
            self.at.notes_bottom = self.at.notes_bottom.max(y - 0.15 + area.total());
        }
        let parts = area.parts.clone();
        out.push(self.notes_box(area, &[(col.x, &parts)], col.width, y, memos)?);
        Ok(())
    }

    // Column fill
    pub fn fill(
        &mut self,
        token: &FlowToken,
        state: &FlowState,
        col: ColumnSpace,
        page_empty: bool,
        memos: &mut Memos,
    ) -> Result<ColumnResult, Yield> {
        let (mut pos, mut carry, mut out) = (token.clone(), state.clone(), Vec::new());
        let (stop, bottom) = if let Some(mut area) = self.at.stream.take() {
            if area.sep == 0.0 && area.shared.is_some_and(|r| r > 0.0) {
                area.sep = self.separator(false, col.width, memos)?.height;
            }
            let r = self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos);
            self.at.stream = Some(area);
            r?
        } else {
            let mut area = NoteArea::default();
            self.continue_notes(&mut carry, &col, &mut area, memos)?;
            let r = self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos)?;
            let text = r.1 + if pos.at.at_start() { carry.prev_after } else { 0.0 };
            self.draw_notes(&area, &col, text, &mut out, memos)?;
            r
        };
        Ok(ColumnResult { children: out, stop, bottom, token: pos, state: carry })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the column fill passes its state (flow position, carried state, column, y, emptiness, note area, output) explicitly, so what each step reads and changes stays visible"
    )]
    fn fill_body(
        &mut self,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: ColumnSpace,
        page_empty: bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<(Stop, f64), Yield> {
        let nblocks = self.sec.blocks.len();
        let mut y = col.top.to_pt();
        let mut empty = true;
        if page_empty
            && self.continues_endnote(pos)
            && let Some(sep) = self.separator_story("endnote:continuationSeparator", col.width, memos)?
        {
            place_units(&sep.units, col.x, y, out);
            y += sep.height;
        }
        // Keep-with-next chain
        #[derive(Clone)]
        struct Breakpoint {
            pos: FlowToken,
            carry: FlowState,
            out: usize,
            y: f64,
            empty: bool,
            area: NoteArea,
            starts: usize,
            effects: PageEffects,
        }
        fn step_back(members: &[(Breakpoint, usize, bool, bool)]) -> Option<(Breakpoint, usize)> {
            for (bp, here, keep_lines, widow) in members.iter().rev() {
                if bp.empty {
                    return None;
                }
                if *keep_lines {
                    continue;
                }
                let keep = here.saturating_sub(if *widow { 2 } else { 1 });
                if keep >= if *widow { 2 } else { 1 } {
                    return Some((bp.clone(), keep));
                }
            }
            None
        }
        let mut chain: Option<Breakpoint> = None;
        let mut members: Vec<(Breakpoint, usize, bool, bool)> = Vec::new();
        let generic = self.ctx.env.text.compat == Compat::Generic;
        loop {
            if pos.block >= nblocks {
                let end = y + if col.balance { carry.prev_after } else { 0.0 };
                return Ok((Stop::Done, end));
            }
            if self.at.end.as_ref() == Some(&*pos) {
                return Ok((Stop::Page, y));
            }
            let vanished = matches!(self.sec.blocks.get(pos.block), Some(Block::Para(p)) if super::paragraph::vanished(p));
            if pos.at.at_start()
                && (self.break_para_after_table(pos.block) && self.ctx.env.text.compat == Compat::Legacy
                    || self.break_para_after_para(pos.block)
                    || vanished)
            {
                pos.next_block();
                continue;
            }
            let bi = pos.block;
            let start_state = Breakpoint {
                pos: pos.clone(),
                carry: carry.clone(),
                out: out.len(),
                y,
                empty,
                area: area.clone(),
                starts: self.at.starts.len(),
                effects: self.at.effects.clone(),
            };
            let found_len = self.at.found.len();
            match self.sec.blocks.get(bi) {
                Some(Block::Para(_)) => {
                    let Some((laid, end)) = self.laid(bi) else { unreachable!("a paragraph block") };
                    let p = laid.para();
                    let style = p.para_style_in(self.ctx.env.text.review.view);
                    if pos.at.at_start() && style.page_break_before && !(empty && page_empty) {
                        carry.keep_top = self.ctx.env.text.compat == Compat::Legacy
                            || bi == 0 && self.sec.section.break_kind == crate::docx::SectionBreak::Continuous;
                        return Ok((Stop::Page, y));
                    }
                    let pb = self.para_lines(&laid, bi, end, col.width, pos.at.index(), col.x, memos)?;
                    let placed = self.place_para(p, &pb, pos, carry, &col, &mut y, &mut empty, area, out, memos)?;
                    if pos.block > bi {
                        pos.block = end + 1;
                    }
                    match placed {
                        Stop::Done => {}
                        Stop::Full if pos.block == bi && pos.at.at_start() => {
                            if let Some(c) = chain.take()
                                && !c.empty
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.found.truncate(found_len);
                                self.at.effects = c.effects;
                                carry.keep_top = false;
                                return Ok((Stop::Full, c.y));
                            }
                            return Ok((Stop::Full, y));
                        }
                        s => return Ok((s, y)),
                    }
                    if style.keep_next {
                        let here = out[start_state.out..].iter().map(|c| if let Child::Para(f) = c { f.lines.len() } else { 0 }).sum();
                        members.push((start_state.clone(), here, style.keep_lines, style.widow_control));
                        if chain.is_none() {
                            chain = Some(start_state);
                        }
                        let fits = self.next_fits(pos.block, carry, &col, y, area.total(), memos)?;
                        if !fits && let Some((c, cap)) = generic.then(|| step_back(&members)).flatten() {
                            *pos = c.pos;
                            *carry = c.carry;
                            *area = c.area;
                            out.truncate(c.out);
                            self.at.starts.truncate(c.starts);
                            self.at.found.truncate(found_len);
                            self.at.effects = c.effects;
                            self.at.keep_cap = Some((pos.block, cap));
                            (chain, members) = (None, Vec::new());
                            y = c.y;
                            empty = c.empty;
                            continue;
                        }
                        if !fits
                            && let Some(c) = chain.take()
                            && !c.empty
                        {
                            *pos = c.pos;
                            *carry = c.carry;
                            *area = c.area;
                            out.truncate(c.out);
                            self.at.starts.truncate(c.starts);
                            self.at.found.truncate(found_len);
                            self.at.effects = c.effects;
                            carry.keep_top = false;
                            return Ok((Stop::Full, c.y));
                        }
                    } else {
                        (chain, members) = (None, Vec::new());
                    }
                }
                Some(Block::Table(t)) => {
                    let breaks = t.rows.first().is_some_and(|r| {
                        r.cells.iter().any(
                            |c| matches!(c.blocks.first(), Some(Block::Para(p)) if p.para_style_in(self.ctx.env.text.review.view).page_break_before),
                        )
                    });
                    if pos.at.at_start() && breaks && !(empty && page_empty) {
                        return Ok((Stop::Page, y));
                    }
                    let tb = self.table_layout(bi, col.width, memos)?;
                    match self.place_table(&tb, pos, carry, &col, &mut y, &mut empty, area, out, memos)? {
                        Stop::Done => (chain, members) = (None, Vec::new()),
                        s => {
                            if pos.block == bi
                                && pos.at.at_start()
                                && let Some((c, cap)) = generic.then(|| step_back(&members)).flatten()
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.effects = c.effects;
                                self.at.keep_cap = Some((pos.block, cap));
                                (chain, members) = (None, Vec::new());
                                y = c.y;
                                empty = c.empty;
                                continue;
                            }
                            if let Some(c) = chain.take()
                                && !c.empty
                                && pos.block == bi
                                && pos.at.at_start()
                            {
                                *pos = c.pos;
                                *carry = c.carry;
                                *area = c.area;
                                out.truncate(c.out);
                                self.at.starts.truncate(c.starts);
                                self.at.effects = c.effects;
                                return Ok((Stop::Full, c.y));
                            }
                            return Ok((s, y));
                        }
                    }
                }
                None => unreachable!("block index checked above"),
            }
        }
    }

    fn next_fits(&mut self, bi: usize, carry: &FlowState, col: &ColumnSpace, y: f64, notes: f64, memos: &mut Memos) -> Result<bool, Yield> {
        let bottom = col.bottom.to_pt() - notes;
        match self.sec.blocks.get(bi) {
            None => Ok(true),
            Some(Block::Para(_)) => {
                let Some((laid, end)) = self.laid(bi) else { unreachable!("a paragraph block") };
                let pb = self.para_lines(&laid, bi, end, col.width, 0, col.x, memos)?;
                let s = laid.para().para_style_in(self.ctx.env.text.review.view);
                let g = match &carry.prev {
                    Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, s, space_before(&self.ctx.env.text, s)),
                    None => self.edge_before(bi, s, space_before(&self.ctx.env.text, s)),
                };
                let n = pb.lines.len();
                let need = if s.keep_lines {
                    n
                } else if s.widow_control {
                    2.min(n)
                } else {
                    1.min(n)
                };
                let h: f64 = pb.lines[..need].iter().map(|l| l.pitch).sum::<f64>() - pb.lines.get(need.saturating_sub(1)).map_or(0.0, |l| l.below);
                Ok(y + g + h <= bottom + EPS)
            }
            Some(Block::Table(_)) => {
                let tb = self.table_layout(bi, col.width, memos)?;
                let after = self.after_above_table(bi, carry);
                Ok(tb.rows.first().is_none_or(|r| y + after + r.height + tb.bottom_border <= bottom + EPS))
            }
        }
    }

    fn record(&mut self, p: &Paragraph, first_part: bool) {
        let id = para_id(p);
        if !self.at.effects.paras.contains(&id) {
            self.at.effects.paras.push(id);
        }
        if first_part {
            for i in &p.inlines {
                if let Inline::Bookmark(name) = i {
                    self.at.effects.bookmarks.retain(|b| b != name);
                    self.at.effects.bookmarks.push(name.clone());
                }
            }
        }
    }

    fn line_number(&self, carry: &mut FlowState, style: &ParaStyle, col: &ColumnSpace, baseline: f64, out: &mut Vec<Box<[Item]>>) {
        let Some(ln) = self.sec.section.line_numbers.as_ref().filter(|_| !style.suppress_line_numbers) else { return };
        carry.line_no += 1;
        let n = ln.start + carry.line_no;
        if n % ln.count_by != 0 {
            return;
        }
        let ncols = self.sec.section.section_in(self.ctx.env.text.review.view).columns.num.max(1);
        let dist = if ln.distance > 0.0 {
            ln.distance
        } else if ncols > 1 {
            9.0
        } else {
            18.0
        };
        if let Some(run) = &self.ctx.env.line_number_run {
            let label = draw_label(&n.to_string(), run, col.x - dist, baseline, self.ctx.fonts);
            if !label.is_empty() {
                out.push(label.into_boxed_slice());
            }
        }
    }

    // Paragraphs
    #[allow(
        clippy::too_many_arguments,
        reason = "the column fill passes its state (flow position, carried state, column, y, emptiness, note area, output) explicitly, so what each step reads and changes stays visible"
    )]
    fn place_para(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: &ColumnSpace,
        y: &mut f64,
        empty: &mut bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let has_anchor = pb.pieces().iter().any(|q| q.kind == Kind::Anchor);
        let bottom = *y + self.para_gap(p, pos, carry, *empty) + pb.lines.iter().map(|l| l.pitch).sum::<f64>();
        let text_bottom = match self.ctx.env.text.compat {
            Compat::Generic => bottom - pb.lines.last().map_or(0.0, |l| l.below),
            Compat::Legacy => bottom + space_after(&self.ctx.env.text, p.para_style_in(self.ctx.env.text.review.view)),
        };
        let affected = self.fixed.iter().chain(&self.at.known).any(|f| {
            let reach = if f.key.2 == float::TABLE_PIECE { text_bottom } else { bottom };
            f.wrap != Wrap::None && f.excl[3] > *y + EPS && f.excl[1] < reach - EPS && f.excl[0] < col.x + col.width - EPS && f.excl[2] > col.x + EPS
        });
        if has_anchor || affected {
            return self.place_wrapped(p, pb, pos, carry, col, y, empty, area, out, memos);
        }
        self.place_plain(p, pb, pos, carry, col, y, empty, area, out, memos)
    }

    #[allow(clippy::too_many_arguments, reason = "as place_para")]
    fn place_plain(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: &ColumnSpace,
        y: &mut f64,
        empty: &mut bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let first_part = pos.at.at_start();
        let mut before = space_before(&self.ctx.env.text, style);
        if style.before_auto && carry.first_para {
            before = 0.0;
        }
        let after = space_after(&self.ctx.env.text, style);
        let n = pb.lines.len();
        let at_top = *empty;
        let g = if !first_part {
            std::mem::take(&mut carry.break_before)
        } else if at_top {
            if carry.keep_top || carry.first_para {
                let after = if pos.block == 0 { carry.prev_after.max(carry.balanced_after) } else { carry.prev_after };
                (super::paragraph::gap_plain(&self.ctx.env.text, after, before) - after).max(0.0)
            } else {
                0.0
            }
        } else {
            match &carry.prev {
                Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, style, before),
                None => self.edge_before(pos.block, style, before),
            }
        };
        let lifted = at_top && first_part && !(carry.keep_top || carry.first_para);
        let top = *y + g - if lifted { self.top_lift(style, pb) } else { 0.0 };
        let mut yy = top;
        let mut fit = 0;
        let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
        // After a note waits for the next page, the lines its paragraph can't be parted from there.
        let mut more: Option<usize> = None;
        let wc = style.widow_control;
        for (i, l) in pb.lines.iter().enumerate() {
            let room = l.pitch - l.below;
            let refs = pb.notes(i);
            if more.is_some() && refs.iter().any(|r| r.0 == NoteKind::Foot) {
                break;
            }
            let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
            let defer = i == 0 || first_part && wc && (i == 1 || n <= 3);
            if !self.add_notes(&refs, &mut a, col, yy + room, &mut rest, defer, memos)? && !pb.leading_page_break(i) {
                break;
            }
            let tail = if i + 1 == n { after } else { 0.0 };
            let over = |a: &NoteArea| a.total() > 0.0 && !col.bottom.holds(yy + l.pitch + tail + a.total()) && !pb.leading_page_break(i);
            let waits = |a: &NoteArea, rest: &[NotePart]| rest.iter().any(|r| !a.parts.iter().any(|p| (p.kind, p.id) == (r.kind, r.id)));
            if over(&a) && defer {
                // The notes fit only without the line's full height: in compat ≤ 14 they may wait instead.
                let (mut a2, mut rest2) = areas.last().cloned().unwrap_or_default();
                if self.add_notes(&refs, &mut a2, col, yy + l.pitch + tail, &mut rest2, defer, memos)? && waits(&a2, &rest2) {
                    (a, rest) = (a2, rest2);
                }
            }
            if over(&a) {
                break;
            }
            areas.push((a, rest));
            yy += l.pitch;
            fit += 1;
            if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                break;
            }
            if let Some((a, rest)) = areas.last().filter(|x| !x.1.is_empty()) {
                if more.is_none() && waits(a, rest) {
                    more = Some(match (first_part && wc, n <= 3) {
                        (true, true) => n - 1 - i,
                        (true, false) if i == 0 || i + 2 == n => 1,
                        _ => 0,
                    });
                }
                match more {
                    Some(m) if m > 0 => more = Some(m - 1),
                    _ => break,
                }
            }
        }
        let hard = pb.lines[..fit.min(n)].iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)));
        let mut take = lines_to_take(n, fit, hard, first_part, style.keep_lines, style.widow_control);
        if let Some((b, cap)) = self.at.keep_cap
            && b == pos.block
            && first_part
        {
            self.at.keep_cap = None;
            take = take.min(cap);
        }
        if take == 0 {
            if at_top {
                self.at.forced = self.at.forced || col.balance;
                take = fit.max(1).min(n);
            } else {
                return Ok(Stop::Full);
            }
        }
        self.at.starts.push((top, first_part));
        self.record(p, first_part);
        let (a, rest) = areas[take.min(areas.len() - 1)].clone();
        // A note waiting whole for the next page is numbered on this one.
        for r in rest.iter().filter(|r| !a.parts.iter().any(|p| (p.kind, p.id) == (r.kind, r.id))) {
            self.at.effects.notes_shown.push((r.kind, r.id, self.note_numbers.get(&(r.kind, r.id)).cloned().unwrap_or_default()));
        }
        *area = a;
        let notes_continue = !rest.is_empty();
        carry.note_rest.extend(rest);
        let mut yy = top;
        let mut stop_kind = None;
        let mut lines = Vec::with_capacity(take);
        let mut numbers = Vec::new();
        for (i, l) in pb.lines[..take].iter().enumerate() {
            lines.push(PlacedLine { x: 0.0, y: yy, line: pb.fragment(i, p, self.ctx.fonts) });
            self.line_number(carry, style, col, yy + l.baseline, &mut numbers);
            yy += l.pitch;
            if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                stop_kind = l.hard;
            }
        }
        out.push(Child::Para(ParaFragment { para: para_id(p), lines: lines.into_boxed_slice(), line_numbers: numbers.into_boxed_slice() }));
        *y = yy;
        *empty = false;
        carry.first_para = false;
        carry.prev = Some(p.para_style_in(self.ctx.env.text.review.view).clone());
        carry.balanced_after = 0.0;
        let prev_after = std::mem::replace(&mut carry.prev_after, after);
        let done = take >= n;
        if done {
            pos.next_block();
        } else {
            pos.at = At::Piece(pb.from + pb.lines[take].start);
        }
        if notes_continue && stop_kind.is_none() {
            carry.keep_top = false;
            return Ok(Stop::Full);
        }
        let starts_with_break = first_part
            && take == 1
            && !done
            && pb.pieces()[pb.lines[0].start..pb.lines[0].end]
                .iter()
                .all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak | Kind::Anchor | Kind::ListNumber));
        Ok(match stop_kind {
            Some(Kind::PageBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                let kept = (super::paragraph::gap_plain(&self.ctx.env.text, prev_after, before) - prev_after).max(0.0);
                carry.break_before = if starts_with_break { kept } else { 0.0 };
                Stop::Page
            }
            Some(Kind::ColumnBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                carry.break_before = if starts_with_break { before } else { 0.0 };
                Stop::Column
            }
            _ if !done => {
                carry.keep_top = false;
                Stop::Full
            }
            _ => Stop::Done,
        })
    }

    fn para_gap(&self, p: &Paragraph, pos: &FlowToken, carry: &FlowState, empty: bool) -> f64 {
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let mut before = space_before(&self.ctx.env.text, style);
        if style.before_auto && carry.first_para {
            before = 0.0;
        }
        if !pos.at.at_start() {
            0.0
        } else if empty {
            if carry.keep_top || carry.first_para {
                let after = if pos.block == 0 { carry.prev_after.max(carry.balanced_after) } else { carry.prev_after };
                (super::paragraph::gap_plain(&self.ctx.env.text, after, before) - after).max(0.0)
            } else {
                0.0
            }
        } else {
            match &carry.prev {
                Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, style, before),
                None => self.edge_before(pos.block, style, before),
            }
        }
    }

    pub fn break_para_after_table(&self, bi: usize) -> bool {
        self.sec.breaks
            && bi + 1 == self.sec.section.blocks.len()
            && bi > 0
            && matches!(self.sec.blocks.get(bi - 1), Some(Block::Table(_)))
            && matches!(self.sec.blocks.get(bi), Some(Block::Para(p)) if p.inlines.is_empty())
    }

    pub fn break_para_after_para(&self, bi: usize) -> bool {
        let bare = |i: &Inline| match i {
            Inline::Text(t, _) => t.is_empty(),
            Inline::Bookmark(_) | Inline::Range(_) | Inline::ControlStart(_) | Inline::ControlEnd(_) | Inline::CommentRef { .. } => true,
            _ => false,
        };
        self.sec.breaks
            && bi + 1 == self.sec.section.blocks.len()
            && bi > 0
            && matches!(self.sec.blocks.get(bi - 1), Some(Block::Para(_)))
            && matches!(self.sec.blocks.get(bi), Some(Block::Para(p)) if p.inlines.iter().all(bare))
    }

    /// How far a paragraph's first line rises at a column's top that dropped its space-before: with `suppressSpacingAtTopOfPage` (compat ≤ 14) by the
    /// line's height beyond its text; with `suppressTopSpacing`, an exact or at-least line of an unbordered,
    /// unshaded paragraph by its line height beyond 9.6pt.
    fn top_lift(&self, style: &ParaStyle, pb: &ParaLines) -> f64 {
        let env = &self.ctx.env.text;
        let Some(l) = pb.lines.first() else { return 0.0 };
        if env.suppress_spacing_at_top {
            return if style.line != 0.0 { (l.pitch - l.ascent - l.descent).max(0.0) } else { 0.0 };
        }
        let framed = [&style.border_top, &style.border_bottom, &style.border_left, &style.border_right]
            .iter()
            .any(|b| b.as_ref().is_some_and(|b| b.visible()))
            || style.shading.is_some();
        if !env.suppress_top_spacing || framed {
            return 0.0;
        }
        let line = match style.line_rule {
            LineRule::AtLeast => style.line,
            // An exact line counts four fifths of its height less its first character's size.
            LineRule::Exact => {
                let size = pb.pieces()[l.start..l.end].iter().find(|q| q.kind == Kind::Text).map_or(0.0, |q| pb.shaped.styles[q.style as usize].size);
                let twips = (style.line.abs() * 20.0).round() as i64;
                (twips * 4 / 5 - 10 * (size * 2.0).round() as i64) as f64 / 20.0
            }
            LineRule::Auto => return 0.0,
        };
        (line - 9.6).max(0.0)
    }

    fn edge_before(&self, bi: usize, style: &ParaStyle, before: f64) -> f64 {
        let after_table = bi > 0 && matches!(self.sec.blocks.get(bi - 1), Some(Block::Table(_)));
        if after_table && super::paragraph::drops_at_edge(style, Some(&self.ctx.env.text.default_style)) { 0.0 } else { before }
    }

    fn after_above_table(&self, bi: usize, carry: &FlowState) -> f64 {
        let first = match self.sec.blocks.get(bi) {
            Some(Block::Table(t)) => t.rows.first().and_then(|r| r.cells.first()).and_then(|c| match c.blocks.first() {
                Some(Block::Para(p)) => Some(&p.para_style_in(self.ctx.env.text.review.view).style_id),
                _ => None,
            }),
            _ => None,
        };
        if carry.prev.as_ref().is_some_and(|ps| super::paragraph::drops_at_edge(ps, first)) { 0.0 } else { carry.prev_after }
    }

    pub fn fit_frame(&self, f: &Rc<Float>, width: f64, memos: &mut Memos) -> Result<Rc<Float>, Yield> {
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        Ok(super::story::fit_frame(f, width, &env, memos)?.map_or_else(|| f.clone(), Rc::new))
    }

    pub fn float_height(&self, f: &Float, memos: &mut Memos) -> Result<f64, Yield> {
        let fields = FieldValues::default();
        super::story::float_height(f, &StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers }, memos)
    }

    fn wrapped(&self, p: Placed, memos: &mut Memos) -> Result<Placed, Yield> {
        let fields = FieldValues::default();
        let lines = super::story::tight_lines(&p.float, &StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers }, memos)?;
        Ok(match lines {
            Some(l) => p.around_lines(&l),
            None => p,
        })
    }

    // Wrapped paragraphs
    #[allow(
        clippy::too_many_arguments,
        reason = "the column fill passes its state (flow position, carried state, column, y, emptiness, note area, output) explicitly, so what each step reads and changes stays visible"
    )]
    fn place_wrapped(
        &mut self,
        p: &Paragraph,
        pb: &ParaLines,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: &ColumnSpace,
        y: &mut f64,
        empty: &mut bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let first_part = pos.at.at_start();
        let after = space_after(&self.ctx.env.text, style);
        let at_top = *empty;
        let lifted = at_top && first_part && !(carry.keep_top || carry.first_para);
        let top = *y + self.para_gap(p, pos, carry, *empty) - if lifted { self.top_lift(style, pb) } else { 0.0 };
        let floats: Vec<Placed> = self.fixed.iter().chain(&self.at.known).cloned().collect();
        let space = ParaSpace { width: col.width, start: pb.from, borders: (None, None), x0: col.x, env: self.ctx.env.text.clone(), fit: false };
        let frame = Frame::new(&space);
        let sp: &ShapedPara = &pb.shaped;
        let pieces = pb.pieces();
        let n = pieces.len();
        let est = pb.lines.first().map_or(13.0, |l| l.pitch);
        let top_extra = top_border_extra(style, pb.borders.0.as_deref(), first_part);
        let (cx0, cx1) = (col.x, col.x + col.width);
        let compat = self.ctx.env.text.compat;
        let mut content = pieces.iter().filter(|q| q.kind != Kind::Anchor);
        let lone_picture = compat == Compat::Legacy && content.next().is_some_and(|q| q.kind == Kind::Object) && content.next().is_none();
        // Bands
        let mut bands: Vec<(f64, Vec<Line>)> = Vec::new();
        let mut yy = top;
        let mut s = 0;
        let mut guard = 0;
        while (s < n || bands.is_empty()) && guard < 4000 {
            guard += 1;
            let mut h = est;
            let mut got = None;
            let mut blocked = false;
            let mut trial = None;
            let here: &[Placed] = if yy >= col.bottom.to_pt() - EPS { &[] } else { &floats };
            if compat == Compat::Legacy {
                let near = || here.iter().filter(|f| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS);
                let e = near().filter(|f| f.excl[1] <= yy + EPS).map(|f| f.excl[3]).fold(f64::INFINITY, f64::min);
                if e < yy + h - EPS && near().any(|f| f.excl[1] > yy + EPS) {
                    yy = e;
                    continue;
                }
                if style.keep_lines {
                    let top = near().filter(|f| f.wrap != Wrap::None && f.excl[1] > yy + EPS).map(|f| f.excl[1]).fold(f64::INFINITY, f64::min);
                    if top < yy + h - EPS {
                        yy = top;
                        continue;
                    }
                }
            }
            for attempt in 0..2 {
                match float::intervals(here, cx0, cx1, yy, yy + h) {
                    None => {
                        blocked = true;
                        break;
                    }
                    Some(iv) => {
                        let ivc: Vec<(f64, f64)> = iv.iter().map(|(a, b)| (a - cx0, b - cx0)).collect();
                        let open = iv.len() == 1 && (iv[0].0 - cx0).abs() < EPS && (iv[0].1 - cx1).abs() < EPS;
                        let first = first_part && bands.is_empty();
                        let r = band(
                            sp,
                            pieces,
                            s,
                            &ivc,
                            first,
                            style,
                            p.mark_in(self.ctx.env.text.review.view),
                            &frame,
                            self.ctx.fonts,
                            open || lone_picture,
                        );
                        match r {
                            Ok((mut segs, e)) => {
                                if first {
                                    for l in &mut segs {
                                        l.pitch += top_extra;
                                        l.baseline += top_extra;
                                    }
                                }
                                let ph = segs[0].pitch;
                                let done = (ph - h).abs() < EPS || attempt == 1;
                                got = Some((segs, e));
                                if done {
                                    break;
                                }
                                h = ph;
                            }
                            Err(t) => {
                                blocked = true;
                                trial = Some(t + if first { top - *y } else { 0.0 });
                                break;
                            }
                        }
                    }
                }
            }
            match (got, blocked) {
                (Some((segs, e)), false) => {
                    let ph = segs[0].pitch;
                    let hard = segs.last().is_some_and(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)));
                    let clear = segs.last().and_then(|l| Some((crate::layout::line::clear_of(sp, pieces, l)?, l.xs.first().copied().unwrap_or(0.0))));
                    bands.push((yy, segs));
                    let next = yy + ph;
                    yy = match clear {
                        Some(((l, r), x)) if compat == Compat::Generic => {
                            let iv = float::intervals(here, cx0, cx1, next - ph, next).unwrap_or_default();
                            let (a, b) = iv.iter().copied().find(|&(a, b)| cx0 + x >= a - EPS && cx0 + x <= b + EPS).unwrap_or((cx0, cx1));
                            here.iter()
                                .filter(|f| f.in_band(next - ph, next) && ((l && f.excl[2] <= a + EPS) || (r && f.excl[0] >= b - EPS)))
                                .map(|f| f.excl[3])
                                .fold(next, f64::max)
                        }
                        Some(((l, r), _)) => {
                            let mut at = next;
                            while at < col.bottom.to_pt() - EPS {
                                let first = float::intervals(here, cx0, cx1, at, at + ph).and_then(|iv| iv.first().copied());
                                if first.is_some_and(|(a, b)| (!l || a <= cx0 + EPS) && (!r || b >= cx1 - EPS)) {
                                    break;
                                }
                                match float::next_edge(here, cx0, cx1, at, at + ph) {
                                    Some(e) => at = e,
                                    None => break,
                                }
                            }
                            at
                        }
                        None => next,
                    };
                    s = e;
                    if hard {
                        break;
                    }
                }
                _ => {
                    let meets = |f: &&Placed| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS;
                    if first_part && bands.is_empty() && here.iter().filter(meets).any(|f| f.key.2 == float::TABLE_PIECE) {
                        let below = here.iter().filter(meets).map(|f| f.excl[3]).fold(yy, f64::max);
                        let before = if style.before_auto && carry.first_para { 0.0 } else { space_before(&self.ctx.env.text, style) };
                        yy = (below + before).min(col.bottom.to_pt().max(yy + EPS));
                        continue;
                    }
                    let square = here.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square");
                    if let Some(t) = trial.filter(|&t| compat == Compat::Legacy && square && t > EPS) {
                        yy = (yy + t).min(col.bottom.to_pt().max(yy + EPS));
                        continue;
                    }
                    match float::next_edge(here, cx0, cx1, yy, yy + h).filter(|_| !float::steps(here, yy, yy + h)) {
                        Some(b) => yy = b.min(col.bottom.to_pt().max(yy + EPS)),
                        None => yy += h,
                    }
                }
            }
        }
        if bands.is_empty() {
            return self.place_plain(p, pb, pos, carry, col, y, empty, area, out, memos);
        }
        if border_key(style).is_some() || style.shading.is_some() || !style.div_boxes.is_empty() {
            let mut firsts: Vec<Line> = bands.iter().map(|(_, segs)| segs[0].clone()).collect();
            let next = if s >= n { pb.borders.1.clone() } else { border_key(style) };
            apply_borders(&mut firsts, style, pb.borders.0.as_deref(), next.as_deref(), col.width, first_part, first_part, s >= n);
            for ((_, segs), l) in bands.iter_mut().zip(firsts) {
                segs[0] = l;
            }
        }
        // Anchors
        let frames = Frames {
            page_w: self.page_w,
            page_h: self.page_h,
            margin: [self.margins.0, self.margins.1, self.margins.2, self.sec.section.section_in(self.ctx.env.text.review.view).bottom],
            column: (col.x, col.width),
            compat: self.ctx.env.text.compat,
            cell: false,
            odd: self.odd,
        };
        let bi = pos.block;
        let si = self.sec.index;
        let anchor = para_id(p);
        let mut anchors: Vec<Placed> = Vec::new();
        let mut forced: Option<usize> = None;
        let band_of = |k: usize| bands.iter().position(|(_, segs)| segs.iter().any(|l| k >= l.start && k < l.end.max(l.start + 1)));
        for (k, q) in pieces.iter().enumerate() {
            let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, sp.object(pb.from + k)) else { continue };
            let Some(b) = band_of(k) else { continue };
            let owner = piece_para(sp, pb.from + k, anchor);
            let f = &float::sized(f, &frames).map_or_else(|| f.clone(), Rc::new);
            let f = &self.fit_frame(f, col.width, memos)?;
            let h = self.float_height(f, memos)?;
            let line_top = bands[b].0;
            let para_top = if first_part { top } else { bands[0].0 };
            let mut x = float::h_pos(f, &frames);
            if compat == Compat::Legacy && f.h_relative == "column" {
                match f.h_align.as_deref() {
                    Some("left") => x += style.ind_left,
                    Some("right") => x -= style.ind_right,
                    _ => {}
                }
            }
            let mut fy = float::v_pos(f, h, &frames, para_top, line_top);
            if Placed::new((si, bi, pb.from + k), owner, f, x, fy, h).wrap == Wrap::None {
                anchors.push(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h));
                continue;
            }
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") {
                fy = fy.max(frames.margin[2]);
                let (l, r) = (x - f.dist[2], x + f.w + f.dist[3]);
                if l < 0.0 {
                    x -= l;
                } else if r > frames.page_w {
                    x -= (r - frames.page_w).min(l);
                }
            }
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") && !col.bottom.holds(fy + h) {
                let up = col.bottom.to_pt() - h;
                let anchor_bottom = line_top + bands[b].1[0].pitch;
                if up >= anchor_bottom - EPS {
                    fy = up;
                } else if !at_top {
                    return Ok(Stop::Full);
                } else {
                    let mut a = Placed::new((si, bi, pb.from + k), owner, f, x, fy, h);
                    a.wrap = Wrap::None;
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                    anchors.push(a);
                    continue;
                }
            }
            anchors.push(self.wrapped(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h), memos)?);
        }
        // Pagination
        let nb = bands.len();
        let mut fit = 0;
        let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
        for (bi_, (bt, segs)) in bands.iter().enumerate() {
            let l = &segs[0];
            let room = l.pitch - l.below;
            let refs: Vec<(NoteKind, i64)> = segs.iter().flat_map(|l| (l.start..l.end).filter_map(|k| sp.note(pb.from + k))).collect();
            let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
            let lead = segs.len() == 1 && breaks_only(pieces, l) && l.end < n;
            if !self.add_notes(&refs, &mut a, col, bt + room, &mut rest, false, memos)? && !lead {
                break;
            }
            let tail = if bi_ + 1 == nb { after } else { 0.0 };
            if a.total() > 0.0 && !col.bottom.holds(bt + l.pitch + tail + a.total()) && !lead {
                break;
            }
            areas.push((a, rest));
            fit += 1;
            if segs.iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak))) {
                break;
            }
            if areas.last().is_some_and(|x| !x.1.is_empty()) {
                break;
            }
        }
        let hard = bands[..fit].iter().any(|(_, segs)| segs.iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak))));
        let mut take = lines_to_take(nb, fit, hard, first_part, style.keep_lines, style.widow_control);
        if take == 0 {
            if at_top {
                take = fit.max(1).min(nb);
            } else {
                return Ok(Stop::Full);
            }
        }
        let placed_end = bands[take - 1].1.last().map_or(0, |l| l.end);
        for mut a in anchors {
            if a.key.2 - pb.from < placed_end.max(1) {
                if !float::avoid(&mut a, &self.at.found, (frames.margin[0], frames.page_w), col.bottom.to_pt(), compat) {
                    let b = band_of(a.key.2 - pb.from).unwrap_or(0);
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                }
                self.at.found.push(a);
            }
        }
        if let Some(b) = forced {
            take = take.min(b + 1);
        }
        let (a, rest) = areas[take.min(areas.len() - 1)].clone();
        *area = a;
        let notes_continue = !rest.is_empty();
        carry.note_rest.extend(rest);
        self.at.starts.push((bands[0].0, first_part));
        self.record(p, first_part);
        let mut stop_kind = None;
        let mut end_y = top;
        let origin = LineOrigin {
            shaped: sp,
            from: pb.from,
            width: col.width,
            x0: col.x,
            compat: self.ctx.env.text.compat,
            para: anchor,
            review: &self.ctx.env.text.review,
            ul_trail_space: self.ctx.env.text.ul_trail_space,
            palette: crate::drawing::paint::Palette { theme: &self.ctx.env.text.theme, map: &self.ctx.env.text.colour_map },
            backdrop: self.ctx.env.text.backdrop,
        };
        let mut lines = Vec::new();
        let mut numbers = Vec::new();
        for (bt, segs) in &bands[..take] {
            for l in segs {
                lines.push(PlacedLine { x: 0.0, y: *bt, line: Rc::new(draw_line(&origin, l, p, self.ctx.fonts)) });
                if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                    stop_kind = l.hard;
                }
            }
            self.line_number(carry, style, col, bt + segs[0].baseline, &mut numbers);
            end_y = bt + segs[0].pitch;
        }
        out.push(Child::Para(ParaFragment { para: anchor, lines: lines.into_boxed_slice(), line_numbers: numbers.into_boxed_slice() }));
        *y = end_y;
        *empty = false;
        carry.first_para = false;
        carry.prev = Some(p.para_style_in(self.ctx.env.text.review.view).clone());
        carry.balanced_after = 0.0;
        carry.prev_after = after;
        let after_break = (take == nb && s < n).then_some(s);
        let done = take >= nb && after_break.is_none();
        if done {
            pos.next_block();
        } else {
            pos.at = At::Piece(pb.from + after_break.unwrap_or_else(|| bands[take].1[0].start));
        }
        if notes_continue && stop_kind.is_none() {
            carry.keep_top = false;
            return Ok(Stop::Full);
        }
        Ok(match stop_kind {
            Some(Kind::PageBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                Stop::Page
            }
            Some(Kind::ColumnBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                Stop::Column
            }
            _ if !done => {
                carry.keep_top = false;
                Stop::Full
            }
            _ => Stop::Done,
        })
    }

    // Tables
    #[allow(
        clippy::too_many_arguments,
        reason = "the column fill passes its state (flow position, carried state, column, y, emptiness, note area, output) explicitly, so what each step reads and changes stays visible"
    )]
    fn place_table(
        &mut self,
        tb: &TableLayout,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: &ColumnSpace,
        y: &mut f64,
        empty: &mut bool,
        area: &mut NoteArea,
        out: &mut Vec<Child>,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        if tb.rows.is_empty() {
            pos.next_block();
            return Ok(Stop::Done);
        }
        let mut rows = Vec::new();
        // A spaced table's outer frame is closed at the top and bottom of each part.
        let finish = |mut rows: Vec<RowFragment>, out: &mut Vec<Child>, left: f64, opened: Option<f64>, closed: bool| {
            if let Some(y) = opened
                && let Some(first) = rows.iter_mut().find(|r| !r.repeat)
            {
                super::table::frame_part(tb, first, left, Some(y), false);
            }
            if closed && let Some(last) = rows.last_mut() {
                super::table::frame_part(tb, last, left, None, true);
            }
            if !rows.is_empty() {
                out.push(Child::Table(TableFragment { table: tb.id, rows: rows.into_boxed_slice() }));
            }
        };
        if pos.at.at_start() && !*empty {
            *y += self.after_above_table(pos.block, carry);
        }
        let fp = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) => t.floating,
            _ => None,
        };
        let (flow_y, was_empty) = (*y, *empty);
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let unsplit = fp.is_some() && legacy && self.ctx.env.text.do_not_break_wrapped_tables;
        let mut at_x = match fp {
            Some(fp) if fp.h_page => match fp.x_spec {
                Some(Jc::Center) => (self.page_w - col.width) / 2.0,
                Some(Jc::Right) => self.page_w - col.width,
                _ => 0.0,
            },
            _ => col.x,
        };
        let mut settled = true;
        if let Some(fp) = fp
            && pos.at.at_start()
        {
            settled = self.at.found.iter().all(|f| self.at.known.iter().any(|k| k.same(f)));
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
            let (bl, br) = super::table::outer_box(tb);
            let [dt, db, dl, dr] = fp.dist;
            for o in self.at.found.iter().filter(|o| o.key.2 == float::TABLE_PIECE) {
                let (x0, x1) = (at_x + bl - dl.max(0.5), at_x + br + dr.max(0.5));
                if !legacy && fp.never_overlap && o.excl[0] < x1 - dr.max(0.5) && o.excl[2] > x0 {
                    at_x += o.excl[2] - x0;
                } else if legacy && o.excl[0] < x1 && o.excl[2] > x0 && o.excl[3].min(top + h + db) - o.excl[1].max(top - dt) >= 9.0 {
                    top = o.excl[3] + dt;
                }
            }
            *y = self.at.known.iter().find(|k| k.key == (self.sec.index, pos.block, float::TABLE_PIECE)).map_or(top, |k| k.rect[1]);
            if unsplit && !*empty && !col.bottom.holds(*y + h) {
                return Ok(Stop::Full);
            }
        }
        let tall;
        let col = if unsplit {
            tall = ColumnSpace { bottom: Lu::MAX, ..*col };
            &tall
        } else {
            col
        };
        let part_top = *y;
        carry.prev = None;
        carry.prev_after = 0.0;
        carry.balanced_after = 0.0;
        let headers: Vec<usize> =
            (0..tb.rows.len()).take_while(|&r| tb.rows[r].header || tb.rows[r].hidden).filter(|&r| !tb.rows[r].hidden).collect();
        let n = tb.rows.len();
        // Header rows
        if *empty && !pos.at.at_start() && !headers.contains(&pos.at.index()) {
            for &h in &headers {
                let hh = tb.rows[h].height;
                let mut row = draw_row(tb, h, at_x, *y, hh, None, false);
                row.repeat = true;
                rows.push(row);
                *y += hh;
            }
        }
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
        let mut later: Vec<(f64, f64)> = Vec::new();
        let lift = match (tb.rows.first(), self.sec.blocks.get(pos.block)) {
            (Some(r0), Some(Block::Table(t))) if fp.is_none() && pos.at.at_start() && !page_floats.is_empty() => {
                let (l, r) = (at_x + tb.x0, at_x + tb.x0 + tb.cols.iter().sum::<f64>());
                let mut lift = 0.0f64;
                for f in t
                    .rows
                    .first()
                    .into_iter()
                    .flat_map(|row| row.cells.iter())
                    .flat_map(|c| c.blocks.iter())
                    .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                    .flat_map(|p| p.inlines.iter())
                    .filter_map(|i| if let Inline::Float(f, _) = i { page_floats.get(&f.id).copied() } else { None })
                    .filter(|f| {
                        matches!(f.wrap.as_str(), "square" | "topAndBottom")
                            && f.h_relative == "column"
                            && f.v_relative == "paragraph"
                            && f.h_align.is_none()
                            && f.v_align.is_none()
                    })
                    .filter(|f| {
                        let x = float::h_pos(f, &frames);
                        x < r && x + f.w > l
                    })
                {
                    let (top, bottom) = (f.v_offset - f.dist[0], f.v_offset + f.h + f.dist[1]);
                    if top < r0.height {
                        lift = lift.max(bottom);
                    } else {
                        later.push((*y + top, *y + bottom));
                    }
                }
                lift
            }
            _ => 0.0,
        };
        *y += lift;
        let up = |r: usize| if r == 0 { lift } else { 0.0 };
        let mut placed_here = 0;
        let mut placed: Vec<(usize, f64, usize)> = Vec::new();
        self.at.starts.push((*y, pos.at.at_start()));
        let below_content = pos.at.at_start() && !*empty;
        let mut keep: Option<(usize, usize, f64, NoteArea, usize)> = None;
        let mut opened = None;
        while pos.at.index() < n {
            let ri = pos.at.index();
            let row = &tb.rows[ri];
            if row.hidden {
                pos.at = At::Row(ri + 1);
                continue;
            }
            if rows.is_empty() && ri > 0 && row.part_top != 0.0 {
                opened = Some(*y);
                *y += row.part_top;
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
                Some(r) => r.iter().map(|v| CellPart { units: &v[..], base: 0.0, ends: true }).collect(),
                None => whole(row),
            };
            for (k, c) in carried.iter().enumerate() {
                if let Some(u) = c {
                    parts[k] = CellPart { units: &u[..], base: 0.0, ends: true };
                }
            }
            let carries = carried.iter().any(Option::is_some);
            let height = if rest.is_some() { part_height(row, &parts) } else { row.height };
            let height = if row.exact && rest.is_none() && self.ctx.env.text.compat == Compat::Legacy {
                height.min((col.bottom.to_pt() - col.top.to_pt() - row.top_border - tb.bottom_border).max(0.0))
            } else {
                height
            };
            if rest.is_none()
                && !rows.is_empty()
                && let Some(&(_, bottom)) = later.iter().find(|&&(t, b)| *y < b - EPS && t < *y + height - EPS)
            {
                if let Some(&(rs, top, idx)) = placed.last().filter(|p| p.2 + 1 == rows.len()) {
                    let (mut drawn, _) = draw_whole_row(tb, rs, at_x, top, tb.rows[rs].height, true, memos);
                    move_floats(Some(&mut drawn), &page_floats, &frames, tb.rows[rs].top_border, up(rs));
                    rows[idx] = drawn;
                }
                *y = bottom;
            }
            let refs: Vec<(NoteKind, i64)> = parts.iter().flat_map(|cu| cu.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            let mut a = area.clone();
            let mut note_rest = Vec::new();
            let notes_ok = self.add_notes(&refs, &mut a, col, *y + height + tb.bottom_border, &mut note_rest, false, memos)?;
            if !breaks && notes_ok && col.bottom.holds(*y + height + tb.bottom_border + a.total() - a.total().min(0.0)) {
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
                move_floats(rows.last_mut(), &page_floats, &frames, row.top_border, up(ri));
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
            let mut fitted: Vec<(Cow<[StoryUnit]>, bool)> = Vec::new();
            let mut left: Vec<Vec<StoryUnit>> = Vec::new();
            if can_split {
                let compat = self.ctx.env.text.compat;
                let (mut kept, mut low) = (Vec::new(), 0.0f64);
                for (k, cu) in parts.iter().enumerate() {
                    let m = &row.cells[k].margins;
                    let room = avail - m[0] - m[2];
                    let first = cut_cell(cu.units, room, rest.is_some(), compat);
                    let mut cut = first.clone();
                    while cut.any() {
                        let held = cut.kept(cu.units);
                        let u = &held[held.len() - 1];
                        let bottom = (*y + row.top_border + m[0] + u.top + u.height + u.after + m[2] + tb.bottom_border).max(low);
                        let mut refs = kept.clone();
                        refs.extend(held.iter().flat_map(|u| u.notes.iter().copied()));
                        if refs.is_empty() || self.add_notes(&refs, &mut area.clone(), col, bottom, &mut Vec::new(), false, memos)? {
                            (kept, low) = (refs, bottom);
                            break;
                        }
                        let next = cut_cell(cu.units, u.top, rest.is_some(), compat);
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
            }
            let fitted: Vec<CellPart> = fitted.iter().map(|(units, ends)| CellPart { units, base: 0.0, ends: *ends }).collect();
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
                    move_floats(Some(&mut redrawn), &page_floats, &frames, tb.rows[rs].top_border, up(rs));
                    rows[idx] = redrawn;
                }
                rows.push(draw_row(tb, ri, at_x, *y, h, Some(&fitted), true));
                move_floats(rows.last_mut(), &page_floats, &frames, row.top_border, up(ri));
                *y += h + tb.bottom_border;
                *empty = false;
                pos.at = At::RowPart { row: ri, rest: Rc::new(left) };
                finish(rows, out, at_x, opened, false);
                self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y, settled);
                return Ok(Stop::Full);
            }
            if at_top {
                rows.push(draw_row(tb, ri, at_x, *y, height, Some(&parts), last));
                move_floats(rows.last_mut(), &page_floats, &frames, row.top_border, up(ri));
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
                    move_floats(Some(&mut redrawn), &page_floats, &frames, tb.rows[rs].top_border, up(rs));
                    rows[idx] = redrawn;
                }
            }
            if !rows.is_empty() {
                *y += tb.bottom_border;
            }
            finish(rows, out, at_x, opened, true);
            self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y, settled);
            return Ok(Stop::Full);
        }
        finish(rows, out, at_x, opened, false);
        if fp.is_some() {
            self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y, settled);
            (*y, *empty) = (flow_y, was_empty);
        }
        pos.next_block();
        carry.first_para = false;
        Ok(Stop::Done)
    }

    #[allow(clippy::too_many_arguments, reason = "the part's place and the attempt state it was placed in")]
    fn table_obstacle(&mut self, bi: usize, tb: &TableLayout, at_x: f64, top: f64, bottom: f64, flow_y: f64, settled: bool) {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { return };
        let Some(fp) = t.floating else { return };
        if bottom <= top + EPS {
            return;
        }
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let in_flow = legacy && fp.v_anchor == TableAnchor::Text && fp.y == 0.0;
        let [dt, db, dl, dr] = fp.dist;
        let (bl, br) = super::table::outer_box(tb);
        let f = Rc::new(Float {
            id: t.id,
            w: br - bl,
            auto_w: false,
            h: bottom - top,
            image: None,
            alt: "".into(),
            lock_aspect: false,
            text_box: None,
            h_relative: "column".into(),
            h_align: None,
            h_offset: 0.0,
            v_relative: "paragraph".into(),
            v_align: None,
            v_offset: 0.0,
            wrap: "square".into(),
            wrap_text: "bothSides".into(),
            dist: if in_flow { [0.0, 0.0, dl.max(0.5), dr.max(0.5)] } else { [dt, db, dl.max(0.5), dr.max(0.5)] },
            effect: [0.0; 4],
            poly: None,
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: 0,
            z: 0,
            src: None,
            locked: false,
            frame_src: None,
            parts: Vec::new(),
            shape: None,
            rot_swap: false,
            relative: Default::default(),
        });
        let mut o = Placed::new((self.sec.index, bi, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, bottom - top);
        o.excl[1] = o.excl[1].max(flow_y.min(top));
        let l = at_x + bl - if fp.x_spec.is_none() && !fp.h_text { 0.05 } else { 0.0 };
        let shift = ((l / 0.12 + 0.5).floor() * 0.12) - (at_x + bl);
        o.excl[0] += shift;
        o.excl[2] += shift;
        if legacy {
            o.excl[0] -= t.ind.min(0.0);
        }
        o.settled = settled;
        self.at.found.push(o);
    }
}
