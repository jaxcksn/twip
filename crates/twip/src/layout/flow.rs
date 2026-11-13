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
use super::space::{BorderKey, ColumnSpace, ParaSpace, ShapeSpace};
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
    /// The area's columns (x, width), those of the section holding the page's first reference; empty for the
    /// column it is filled in.
    pub cols: Vec<(f64, f64)>,
    pub below: bool,
    /// Room kept clear above the notes, down from where a note that goes on on the next page was referenced.
    pub kept: f64,
    /// A note in the area goes on on the next page.
    pub cut: bool,
    /// Room held at the bottom for the continuation notice: compat ≤ 14 on every area, compat 15 once a note is cut.
    pub notice: f64,
    /// The area draws the continuation notice under its cut note.
    pub noticed: bool,
}

/// The space shared between two notes in an area: the gap between them is the later one's space before or the
/// earlier one's space after, whichever is larger.
fn note_join(prev: Option<&NotePart>, next: &NotePart) -> f64 {
    let after = prev.and_then(|p| p.units.last()).map_or(0.0, |u| u.after);
    after.min(next.units.first().map_or(0.0, |u| u.top))
}

/// A note's height in its area, its space after included.
fn note_extent(part: &NotePart) -> f64 {
    part.height() + part.units.last().map_or(0.0, |u| u.after)
}

/// The height of `parts` stacked in one column.
fn stacked(parts: &[NotePart]) -> f64 {
    parts.iter().enumerate().map(|(i, p)| note_extent(p) - note_join(i.checked_sub(1).map(|j| &parts[j]), p)).sum()
}

impl NoteArea {
    pub fn total(&self) -> f64 {
        if self.below {
            return 0.0;
        }
        self.sep + self.shared.map_or(self.height, |r| self.height.max(r)) + self.kept + self.notice
    }
}

#[derive(Default)]
pub struct PassMemo {
    tables: HashMap<(usize, u64), Rc<TableLayout>>,
    varies: HashMap<usize, bool>,
    joined: HashMap<(usize, usize), Rc<Joined>>,
    mirrored: HashMap<(usize, usize), LaidPara<'static>>,
    rewrapped: HashMap<(NoteKind, i64, usize, usize, u64), Vec<StoryUnit>>,
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
    /// A float placed in this fill met text laid out before it where the last fill didn't have it: the floats after
    /// it were placed beside text that moves.
    pub hit: bool,
    pub starts: Vec<(f64, bool)>,
    pub notes_drawn: bool,
    /// The bottom of the column footnote areas drawn under their text (`beneathText`).
    pub notes_bottom: f64,
    /// The footnote position of the page's area when an earlier section on the page started it.
    pub beneath: Option<bool>,
    pub effects: PageEffects,
    pub keep_cap: Option<(usize, usize)>,
    pub end: Option<FlowToken>,
    /// Tables of a vertical section, drawn upright on the page as printed.
    pub upright: Vec<Child>,
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
    /// The section's footnote area passes to the next section on the page when the section ends.
    pub hold_notes: bool,
    /// The section starts below the page top.
    pub mid_page: bool,
    /// The page's footnote area is closed: a line with a footnote reference goes to the next page.
    pub notes_closed: bool,
    /// The column being filled.
    pub column: usize,
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
    ParaSpace { width, start: from, borders, x0, env: ctx.env.text.clone(), fit: false, letter_break: false }
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
    if fit - start < 2 && before > 0 && units[start].from == 0 {
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
/// Lays a note part out again at a column's width: the note's text from the part's first line on.
pub type Rewrap<'w> = dyn FnMut(&NotePart, f64) -> Result<NotePart, Yield> + 'w;

/// `notes` flowed through columns of `widths`, `h` high: the columns, the tallest one's height, and the parts that don't
/// fit. A note (or its rest) is wrapped at the width of the column it starts in.
pub fn stream_flow(notes: &[NotePart], widths: &[f64], h: f64, wrap: &mut Rewrap) -> Result<(Vec<Vec<NotePart>>, f64, Vec<NotePart>), Yield> {
    let ncols = widths.len().max(1);
    let uneven = widths.iter().any(|w| (w - widths[0]).abs() > EPS);
    let mut cols: Vec<Vec<NotePart>> = vec![Vec::new()];
    let (mut used, mut tallest) = (0.0, 0.0f64);
    for (i, note) in notes.iter().enumerate() {
        let mut part = note.clone();
        loop {
            if uneven {
                part = wrap(&part, widths[cols.len() - 1])?;
            }
            let total = part.height();
            if total <= h - used + EPS {
                used += total;
                cols.last_mut().expect("a column").push(part);
                break;
            }
            let fit = note_cut(&part.units, h - used);
            if fit > 0 {
                used += part.units[fit - 1].top + part.units[fit - 1].height;
                cols.last_mut().expect("a column").push(NotePart { units: part.units[..fit].to_vec(), ..part.clone() });
                part = NotePart { units: rebase(&part.units[fit..]), ..part };
            }
            tallest = tallest.max(used);
            if cols.len() >= ncols {
                let rest = std::iter::once(part).chain(notes[i + 1..].iter().cloned()).collect();
                return Ok((cols, tallest, rest));
            }
            cols.push(Vec::new());
            used = 0.0;
        }
    }
    Ok((cols, tallest.max(used), Vec::new()))
}

pub fn stream_fill(notes: &[NotePart], widths: &[f64], h: f64, wrap: &mut Rewrap) -> Result<Option<(Vec<Vec<NotePart>>, f64)>, Yield> {
    let (cols, tallest, rest) = stream_flow(notes, widths, h, wrap)?;
    Ok(rest.is_empty().then_some((cols, tallest)))
}

/// The smallest height `notes` balance to in columns of `widths` within `room`.
pub fn stream_height(notes: &[NotePart], widths: &[f64], room: f64, wrap: &mut Rewrap) -> Result<Option<f64>, Yield> {
    if notes.is_empty() {
        return Ok(Some(0.0));
    }
    if stream_fill(notes, widths, room, wrap)?.is_none() {
        return Ok(None);
    }
    let (mut lo, mut hi) = (0.0, room);
    for _ in 0..40 {
        let mid = (lo + hi) / 2.0;
        if stream_fill(notes, widths, mid, wrap)?.is_some() {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    Ok(stream_fill(notes, widths, hi, wrap)?.map(|(_, t)| t))
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

fn move_floats(row: Option<&mut super::fragment::RowFragment>, floats: &HashMap<u64, &Float>, fr: &Frames, up: f64) {
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
        (from, letter_break): (usize, bool),
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
        let mut space = laid.shape_space(self.fields, self.note_numbers, &self.ctx);
        if space.stale_fit && self.sec.index == 0 && bi == 0 && p.inlines.iter().any(|i| i.run().is_some_and(|r| r.fit_text.is_some())) {
            // The document's first line as broken with its regions unadjusted: real tab stops and list number.
            let probe = ShapeSpace { unfitted: true, ..space.clone() };
            let shaped = shape(p, &probe, self.ctx.fonts, memos)?;
            let lines = break_lines(&shaped, p, &para_space(style, prev, next, width, 0, x0, &self.ctx), self.ctx.fonts, memos)?;
            space.first_line = Some(lines.lines.first().map_or(0, |l| l.end));
        }
        let shaped = shape(p, &space, self.ctx.fonts, memos)?;
        let mut ps = para_space(style, prev, next, width, from, x0, &self.ctx);
        ps.letter_break = from > 0 && letter_break;
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

    /// Whether only the section's empty closing paragraph, or nothing, is left from `pos`.
    fn at_section_end(&self, pos: &FlowToken) -> bool {
        let n = self.sec.blocks.len();
        pos.block >= n || pos.at.at_start() && pos.block + 1 == n && n > 1 && matches!(self.sec.blocks.get(pos.block), Some(Block::Para(p)) if p.inlines.is_empty())
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
                        hold: 0.0,
                        content,
                        block: 0,
                        from: 0,
                        widow: false,
                        keep_next: false,
                        keep_lines: false,
                        notes: Box::new([]),
                        row: None,
                    }],
                    height: h,
                    float_bottom: 0.0,
                    frozen: Vec::new(),
                    origins: Vec::new(),
                    tables: Vec::new(),
                }
            }),
        };
        Ok(s)
    }

    /// The height of the footnote continuation notice, 0 without one.
    fn notice_height(&mut self, width: f64, memos: &mut Memos) -> Result<f64, Yield> {
        Ok(self.separator_story("footnote:continuationNotice", width, memos)?.map_or(0.0, |s| s.height))
    }

    /// Records the endnote references among `refs` as shown on this page (numbers restarting each page count from there).
    fn show_endnote_refs(&mut self, refs: impl IntoIterator<Item = (NoteKind, i64)>) {
        for (kind, id) in refs.into_iter().filter(|r| r.0 == NoteKind::End) {
            self.at.effects.notes_shown.push((kind, id, self.note_numbers.get(&(kind, id)).cloned().unwrap_or_default()));
        }
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
        if self.notes_closed {
            return Ok(false);
        }
        let mut a = area.clone();
        if a.sep == 0.0 {
            a.sep = self.separator(false, col.width, memos)?.height;
            if self.ctx.env.text.compat == Compat::Legacy && a.shared.is_none() {
                a.notice = self.notice_height(col.width, memos)?;
            }
        }
        if a.shared.is_some() {
            // ponytail: after a note is cut, a later line's notes move it to the next page; where the body after the
            // cut goes on in a later column, its notes might share the next page's stream instead.
            if a.cut && !a.below {
                return Ok(false);
            }
            for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                let u = self.note_units(k, id, a.cols.first().map_or(col.width, |c| c.1), memos)?;
                a.parts.push(NotePart { kind: k, id, units: u.units.clone() });
            }
            if a.below {
                *area = a;
                return Ok(col.bottom.holds(line_bottom));
            }
            let room = col.bottom.to_pt() - line_bottom - a.sep;
            let widths: Vec<f64> = if a.cols.is_empty() { vec![col.width] } else { a.cols.iter().map(|c| c.1).collect() };
            // With columns the page's text up to the line can be spread over them, which leaves the area the room under
            // it: before the last column, notes that need more than the room under the line make it wait for a later
            // column; a cut note fills the room the spread text leaves.
            let cs = &self.sec.section.section_in(self.ctx.env.text.review.view).columns;
            let n = if cs.equal || cs.cols.is_empty() { cs.num } else { cs.cols.len() }.max(1);
            let full = col.bottom.to_pt() - col.top.to_pt();
            let used = self.column as f64 * (full - area.total()) + line_bottom - col.top.to_pt();
            let spread = (widths.len() > 1 && n > 1).then(|| full - used / n as f64 - a.sep);
            let waits = self.column + 1 < n && spread.is_some_and(|s| s > room + EPS);
            let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
            let mut cut = Vec::new();
            let h = if widths.len() > 1 { stream_height(&a.parts, &widths, room, &mut wrap)? } else { Some(stacked(&a.parts)).filter(|&h| h <= room + EPS) };
            if h.is_none() && waits {
                return Ok(false);
            }
            match h {
                Some(h) => a.height = h,
                // Compat 15: the line's last note goes on on the next page when its first part fits.
                None => {
                    let room = spread.map_or(room, |s| s.min(room));
                    let (cols, tallest, over) = stream_flow(&a.parts, &widths, room, &mut wrap)?;
                    let last = a.parts.last().expect("the line's notes");
                    let Some(first) = over.first().filter(|o| (o.kind, o.id) == (last.kind, last.id)) else { return Ok(false) };
                    let placed: usize = cols.iter().flatten().filter(|p| (p.kind, p.id) == (first.kind, first.id)).map(|p| p.units.len()).sum();
                    let units = &last.units;
                    let first_para = units.iter().take_while(|x| x.block == units[0].block).count();
                    if placed < if units.first().is_some_and(|u| u.widow) { 2.min(first_para) } else { 1.min(first_para) } {
                        return Ok(false);
                    }
                    a.height = tallest;
                    a.cut = true;
                    cut = over;
                }
            }
            // A cut area is as high as it is, whatever an earlier fill of the page reserved.
            if !col.bottom.holds(line_bottom + if a.cut { a.sep + a.height + a.kept } else { a.total() }) {
                return Ok(false);
            }
            *area = a;
            rest.extend(cut);
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
            let mut room = col.bottom.to_pt() - line_bottom - a.total();
            // A note cut here is cut in the room the continuation notice leaves.
            let notice = if total > room + EPS && !a.noticed { self.notice_height(col.width, memos)? } else { a.notice };
            if total > room + EPS {
                room -= notice - a.notice;
            }
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
                let part = NotePart { kind: k, id, units: units.clone() };
                a.height += note_extent(&part) - note_join(a.parts.last(), &part);
                a.parts.push(part);
                continue;
            }
            let part = NotePart { kind: k, id, units: units[..fit].to_vec() };
            a.height += note_extent(&part) - note_join(a.parts.last(), &part);
            a.parts.push(part);
            (a.notice, a.noticed) = (notice, notice > 0.0);
            new_rest.push(NotePart { kind: k, id, units: rebase(&units[fit..]) });
        }
        *area = a;
        rest.extend(new_rest);
        Ok(true)
    }

    fn continue_notes(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        // A cut area's rest goes on on the next page.
        if carry.note_rest.is_empty() || area.cut {
            return Ok(());
        }
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        if self.ctx.env.text.compat == Compat::Legacy {
            area.notice = self.notice_height(col.width, memos)?;
        }
        let mut rest = Vec::new();
        for part in std::mem::take(&mut carry.note_rest) {
            // In a later column of the page the note goes on at that column's width.
            let part = if self.column > 0 { self.rewrap(&part, col.width, memos)? } else { part };
            let room = col.bottom.to_pt() - col.top.to_pt() - area.total();
            // Cut as a new note is, the orphan step only for a part holding its paragraph's first line.
            // ponytail: measured between a balanced section's columns; a continuation going on from page to page is
            // assumed to cut the same way.
            let fit = note_cut(&part.units, room);
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

    /// `part` laid out at `width`: its note's text from the part's first line on. A part starting at a table row keeps
    /// its layout.
    fn rewrap(&mut self, part: &NotePart, width: f64, memos: &mut Memos) -> Result<NotePart, Yield> {
        let Some(u) = part.units.first().filter(|u| u.row.is_none()) else { return Ok(part.clone()) };
        let key = (part.kind, part.id, u.block, u.from, width.to_bits());
        if let Some(units) = self.pass.rewrapped.get(&key) {
            return Ok(NotePart { units: units.clone(), ..part.clone() });
        }
        let blocks: Vec<&Block> = self.notes.blocks(part.kind, part.id).map(|b| b.iter().collect()).unwrap_or_default();
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        let units = rebase(&super::story::layout_from(&blocks, width, &env, (u.block, u.from), memos)?.units);
        self.pass.rewrapped.insert(key, units.clone());
        Ok(NotePart { units, ..part.clone() })
    }

    /// Starts the page's note stream with the notes going on from the last page, flowed through its columns.
    fn continue_stream(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        let room = col.bottom.to_pt() - col.top.to_pt() - area.sep;
        let widths: Vec<f64> = if area.cols.is_empty() { vec![col.width] } else { area.cols.iter().map(|c| c.1).collect() };
        let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
        let (_, tallest, over) = stream_flow(&carry.note_rest, &widths, room, &mut wrap)?;
        area.parts = std::mem::take(&mut carry.note_rest);
        area.height = if over.is_empty() { stream_height(&area.parts, &widths, room, &mut wrap)?.unwrap_or(tallest) } else { tallest };
        area.carried = area.parts.len();
        area.cut = !over.is_empty();
        carry.note_rest = over;
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
        let mut end = (x0, top + sep.height);
        for (x, parts) in cols {
            let mut y = top + sep.height;
            for (i, part) in parts.iter().enumerate() {
                y -= note_join(i.checked_sub(1).map(|j| &parts[j]), part);
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
                y += note_extent(part);
                end = (*x, y);
            }
        }
        // The continuation notice, an ordinary paragraph under the cut note's last line.
        if area.noticed
            && let Some(n) = self.separator_story("footnote:continuationNotice", width, memos)?
        {
            place_units(&n.units, end.0, end.1, &mut children);
        }
        Ok(Child::Box(BoxFragment { kind: BoxKind::Notes, id: None, offset: (0.0, 0.0), size: (width, area.total()), children: children.into() }))
    }

    /// Whether the page's footnote area goes beneath the text.
    fn beneath(&self) -> bool {
        self.at.beneath.unwrap_or(self.sec.section.notes[0].beneath)
    }

    /// The top of a footnote area of height `h` in `col`: at the column bottom; with `beneathText`, at `text` (the
    /// bottom of the column's text), below the floats it would overlap in compat 15, and at the bottom when it
    /// doesn't fit there.
    pub fn note_top(&self, h: f64, col: &ColumnSpace, text: f64) -> f64 {
        let bottom = col.bottom.to_pt() - h;
        if !self.beneath() {
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

    /// Draws `area` in its columns, or in `col` when it has none of its own, below `text` or at the bottom of `col`.
    pub fn draw_notes(&mut self, area: &NoteArea, col: &ColumnSpace, text: f64, out: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
        if area.parts.is_empty() {
            return Ok(());
        }
        self.at.notes_drawn = true;
        let cols = if area.cols.is_empty() { vec![(col.x, col.width)] } else { area.cols.clone() };
        let (first, last) = (cols[0], cols[cols.len() - 1]);
        let span = ColumnSpace { x: first.0, width: last.0 + last.1 - first.0, ..*col };
        let h = area.sep + area.height + area.notice;
        let y = self.note_top(h, &span, text) + 0.15;
        if self.beneath() {
            self.at.notes_bottom = self.at.notes_bottom.max(y - 0.15 + h);
        } else {
            self.at.effects.notes_top = self.at.effects.notes_top.into_iter().chain([y - 0.15]).reduce(f64::min);
        }
        let widths: Vec<f64> = cols.iter().map(|c| c.1).collect();
        let parts = if cols.len() > 1 || area.cut {
            let mut wrap = |p: &NotePart, w: f64| self.rewrap(p, w, memos);
            stream_flow(&area.parts, &widths, area.height + EPS, &mut wrap)?.0
        } else {
            vec![area.parts.clone()]
        };
        let per_col: Vec<(f64, &[NotePart])> = cols.iter().zip(&parts).map(|(c, p)| (c.0, &p[..])).collect();
        out.push(self.notes_box(area, &per_col, first.1, y, memos)?);
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
            if self.column == 0 && area.parts.is_empty() && !carry.note_rest.is_empty() {
                self.continue_stream(&mut carry, &col, &mut area, memos)?;
            }
            let r = self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos);
            self.at.stream = Some(area);
            r?
        } else {
            let mut area = NoteArea::default();
            let received = carry.page_notes.is_some();
            if let Some((held, beneath)) = carry.page_notes.take() {
                area = held;
                self.at.beneath = Some(beneath);
            }
            self.continue_notes(&mut carry, &col, &mut area, memos)?;
            // Compat ≤ 14, a balanced column: under a continuation that goes on past it the column holds only the note.
            let r = if col.balance && self.ctx.env.text.compat == Compat::Legacy && !carry.note_rest.is_empty() {
                (Stop::Full, col.top.to_pt())
            } else {
                self.fill_body(&mut pos, &mut carry, col, page_empty, &mut area, &mut out, memos)?
            };
            let text = r.1 + if pos.at.at_start() { carry.prev_after } else { 0.0 };
            // Compat 15: the first section on the page with a footnote reference (on this page or an earlier one)
            // gives the page's footnote area its columns, notes on this page or not.
            let starts_area = || !area.parts.is_empty() || received || self.sec.blocks.blocks.iter().any(|b| super::document::has_footnote_refs(b));
            if r.0 == Stop::Done && self.hold_notes && starts_area() {
                if area.cols.is_empty() {
                    area.cols = vec![(col.x, col.width)];
                }
                carry.page_notes = Some((area, self.beneath()));
            } else {
                self.draw_notes(&area, &col, text, &mut out, memos)?;
            }
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
        if let Some(mut t) = carry.table_rest.take() {
            let tb = self.table_layout(t.block, col.width, memos)?;
            let (prev, prev_after) = (carry.prev.clone(), carry.prev_after);
            let (mut ty, mut te) = (y, empty);
            if self.place_table(&tb, &mut t, carry, &col, &mut ty, &mut te, area, out, memos)? != Stop::Done {
                carry.table_rest.get_or_insert(t);
            }
            (carry.prev, carry.prev_after) = (prev, prev_after);
        }
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
            // Compat 15: a note cut at the end of a section that the next one follows on the page goes on in the next
            // section's layout.
            let passes = self.hold_notes && area.cut && self.at_section_end(pos);
            if pos.block >= nblocks || passes {
                let end = y + if col.balance { carry.prev_after } else { 0.0 };
                // A note cut on this page goes on on the next one, even past the end of the text.
                let more = !carry.note_rest.is_empty() && !area.parts.is_empty() && !passes || carry.table_rest.is_some();
                return Ok((if more { Stop::Full } else { Stop::Done }, end));
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
            // Compat ≤ 14: a beneath-text footnote area goes right under the section's own text, and the endnotes
            // that follow the section go on below it.
            if !generic
                && self.beneath()
                && pos.block == self.sec.section.blocks.len()
                && pos.at.at_start()
                && !area.parts.is_empty()
                && !area.cut
                && carry.note_rest.is_empty()
            {
                self.draw_notes(area, &col, y + carry.prev_after, out, memos)?;
                *area = NoteArea::default();
                (y, carry.prev_after) = (self.at.notes_bottom, 0.0);
            }
            // Compat 15: a continuous section's text waits for the notes going on from an earlier section, on pages
            // of their own.
            if generic
                && pos.block == 0
                && pos.at.at_start()
                && self.sec.index > 0
                && self.sec.section.break_kind == crate::docx::SectionBreak::Continuous
                && (area.carried > 0 || !carry.note_rest.is_empty())
            {
                return Ok((Stop::Full, y));
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
                    // Compat ≤ 14: a page-break-before paragraph keeps its space-before on a page that is already
                    // empty too (after a page-break character).
                    if pos.at.at_start() && style.page_break_before && self.ctx.env.text.compat == Compat::Legacy {
                        carry.keep_top = true;
                    }
                    let pb = self.para_lines(&laid, bi, end, col.width, (pos.at.index(), carry.letter_break), col.x, memos)?;
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
                Some(Block::Table(_)) if self.ctx.env.text.vertical.is_some() => {
                    match self.place_upright_table(bi, pos, carry, &col, &mut y, empty, area, memos)? {
                        Stop::Done => (chain, members) = (None, Vec::new()),
                        s => return Ok((s, y)),
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
                let pb = self.para_lines(&laid, bi, end, col.width, (0, false), col.x, memos)?;
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

    /// The space after the paragraph at `block` that its next paragraph keeps: none between paragraphs of one style
    /// with contextual spacing.
    fn after_kept(&self, block: usize, style: &ParaStyle) -> f64 {
        let text = &self.ctx.env.text;
        let after = space_after(text, style);
        match self.sec.blocks.neighbours(block, block, text.review.view).1 {
            Some(next) => gap(text, style, after, next, 0.0),
            None => after,
        }
    }

    /// The floats of the last fill that wrap the text at `block`: one anchored further on only if it met the text laid
    /// out before its paragraph; one that didn't is placed below that text and wraps only what follows it.
    fn known_at(&self, block: usize) -> impl Iterator<Item = &Placed> {
        let si = self.sec.index;
        self.at.known.iter().filter(move |f| f.collided || (f.key.0, f.key.1) <= (si, block))
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
            Compat::Legacy => bottom + self.after_kept(pos.block, p.para_style_in(self.ctx.env.text.review.view)),
        };
        let affected = self.fixed.iter().chain(self.known_at(pos.block)).any(|f| {
            let reach = if f.key.2 == float::TABLE_PIECE || self.ctx.env.text.compat == Compat::Legacy { text_bottom } else { bottom };
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
            if carry.keep_top || carry.first_para || self.keeps_top_space() {
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
        let top = *y + g - if at_top { self.top_lift(style, pb) } else { 0.0 };
        let wc = style.widow_control;
        // Compat ≤ 14: when the lines can't stay on the page with a line's notes after a note placed whole, those
        // notes wait for the next page and the lines stay.
        let mut force: Option<usize> = None;
        let (fit, areas, mut take) = loop {
            let mut yy = top;
            let mut fit = 0;
            let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
            // After a note waits for the next page, the lines its paragraph can't be parted from there.
            let mut more: Option<usize> = None;
            let mut first_ref = None;
            for (i, l) in pb.lines.iter().enumerate() {
                let room = l.pitch - l.below;
                let refs = pb.notes(i);
                if more.is_some() && refs.iter().any(|r| r.0 == NoteKind::Foot) {
                    break;
                }
                let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
                let foot = refs.iter().any(|r| r.0 == NoteKind::Foot);
                if first_ref.is_none() && foot && a.parts.len() > a.carried {
                    first_ref = Some(i);
                }
                let defer = i == 0 || first_part && wc && (i == 1 || n <= 3);
                // The last line's space after comes off the room for its notes (a line without notes fits without it).
                let tail = if i + 1 == n { after } else { 0.0 };
                let note_tail = if foot { tail } else { 0.0 };
                if force == Some(i) {
                    for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                        rest.push(NotePart { kind: k, id, units: self.note_units(k, id, col.width, memos)?.units.clone() });
                    }
                } else if !self.add_notes(&refs, &mut a, col, yy + room + note_tail, &mut rest, defer, memos)? && !pb.leading_page_break(i) {
                    break;
                }
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
            let take_of = |fit| lines_to_take(n, fit, hard, first_part, style.keep_lines, style.widow_control);
            let take = take_of(fit);
            // Compat 15: when the line that ends the page or column after widow and orphan control ends at a
            // hyphenation point, it goes on to the next one, and widow and orphan control apply once more.
            let (fit, take) = if self.ctx.env.text.bottom_hyphen_moves && take > 0 && take < n && !hard && pb.lines[take - 1].hyphen {
                (take - 1, take_of(take - 1))
            } else {
                (fit, take)
            };
            if take == 0 && force.is_none() && wc && !at_top && self.ctx.env.text.compat == Compat::Legacy && first_ref.is_some() {
                force = first_ref;
                continue;
            }
            break (fit, areas, take);
        };
        if let Some((b, cap)) = self.at.keep_cap
            && b == pos.block
            && first_part
        {
            self.at.keep_cap = None;
            take = take.min(cap);
        }
        if take == 0 {
            // Under notes going on from an earlier column or page that leave no room for it, the line waits (in
            // compat 15 a balancing trial fails instead). So does a line with a footnote reference above an earlier
            // section's closed footnote area, and a section's first line below an earlier section that leaves no
            // room for it on the page.
            let under = area.carried > 0 && (!col.balance || self.ctx.env.text.compat == Compat::Legacy);
            let waits = fit == 0 && (under || self.notes_closed || self.mid_page);
            if at_top && !waits {
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
        // Compat 15 with automatic hyphenation: the line that ends the page or column is broken again without it.
        let text = &self.ctx.env.text;
        let last = match &pb.lines[take - 1] {
            l if text.bottom_unhyphenated && text.hyphenation.is_some() && take < n && l.hyphen && !pb.pieces()[l.end - 1].soft_hyphen => {
                let env = super::env::TextEnv { hyphenation: None, ..text.clone() };
                let space = ParaSpace { width: pb.width, start: pb.from + l.start, borders: pb.borders.clone(), x0: pb.x0, env, fit: false, letter_break: false };
                Some(break_lines(&pb.shaped, p, &space, self.ctx.fonts, memos)?)
            }
            _ => None,
        };
        let mut yy = top;
        let mut stop_kind = None;
        let mut lines = Vec::with_capacity(take);
        let mut numbers = Vec::new();
        for (i, l) in pb.lines[..take].iter().enumerate() {
            let (l, line) = match &last {
                Some(a) if i + 1 == take => (&a.lines[0], a.fragment(0, p, self.ctx.fonts)),
                _ => (l, pb.fragment(i, p, self.ctx.fonts)),
            };
            lines.push(PlacedLine { x: 0.0, y: yy, line });
            self.show_endnote_refs(pb.notes(i));
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
            pos.at = At::Piece(last.as_ref().map_or(pb.from + pb.lines[take].start, |a| a.from + a.lines[0].end));
            carry.letter_break = !last.as_ref().map_or(&pb.lines[take - 1], |a| &a.lines[0]).hyphen;
        }
        if notes_continue && stop_kind.is_none() && !(self.hold_notes && self.at_section_end(pos)) {
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
                carry.break_before = if starts_with_break && !self.ctx.env.text.suppress_before_after_page_break { kept } else { 0.0 };
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
            if carry.keep_top || carry.first_para || self.keeps_top_space() {
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

    /// `suppressSpacingAtTopOfPage` below the first column: the space before stays and no line rises.
    fn keeps_top_space(&self) -> bool {
        self.ctx.env.text.suppress_spacing_at_top && self.column > 0
    }

    /// How far a line at the top of a page (or of a later column of a section that starts at the page top) rises:
    /// with `suppressSpacingAtTopOfPage` (compat ≤ 14) an at-least line by its at-least height beyond its text; with
    /// `suppressTopSpacing`, an exact or at-least line of an unbordered, unshaded paragraph by its line height beyond
    /// 9.6pt.
    fn top_lift(&self, style: &ParaStyle, pb: &ParaLines) -> f64 {
        let env = &self.ctx.env.text;
        let Some(l) = pb.lines.first().filter(|_| !self.mid_page) else { return 0.0 };
        if env.suppress_spacing_at_top {
            return if style.line_rule == LineRule::AtLeast && !self.keeps_top_space() { (style.line - l.ascent - l.descent).max(0.0) } else { 0.0 };
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
            // An exact line counts four fifths of its height less its first character's raise (`w:position`).
            LineRule::Exact => {
                let raise = pb.pieces()[l.start..l.end]
                    .iter()
                    .find(|q| q.kind == Kind::Text)
                    .map_or(0.0, |q| pb.shaped.styles[q.style as usize].run.position);
                let twips = (style.line.abs() * 20.0).round() as i64;
                (twips * 4 / 5 - 10 * (raise * 2.0).round() as i64) as f64 / 20.0
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
        let p = if self.ctx.env.text.compat == Compat::Legacy { p.legacy_dist() } else { p };
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
        let kept_after = self.after_kept(pos.block, style);
        let at_top = *empty;
        let top = *y + self.para_gap(p, pos, carry, *empty) - if at_top { self.top_lift(style, pb) } else { 0.0 };
        let mut floats: Vec<Placed> = self.fixed.iter().chain(self.known_at(pos.block)).cloned().collect();
        for f in floats.iter_mut().filter(|f| f.key.0 == self.sec.index && f.key.1 > pos.block) {
            if let Some(r) = f.early_right {
                (f.rect[2], f.excl[2]) = (r, r);
            }
        }
        let letter_break = !first_part && carry.letter_break;
        let env = self.ctx.env.text.clone();
        let space = ParaSpace { width: col.width, start: pb.from, borders: (None, None), x0: col.x, env, fit: false, letter_break };
        let mut frame = Frame::new(&space);
        let sp: &ShapedPara = &pb.shaped;
        let pieces = pb.pieces();
        let n = pieces.len();
        let est = pb.lines.first().map_or(13.0, |l| l.pitch);
        let est_below = pb.lines.first().map_or(0.0, |l| l.below);
        let top_extra = top_border_extra(style, pb.borders.0.as_deref(), first_part);
        let (cx0, cx1) = (col.x, col.x + col.width);
        let compat = self.ctx.env.text.compat;
        // After a leading page or column break the paragraph starts again with its first line.
        let restarts = pb.from > 0 && sp.pieces[..pb.from.min(sp.pieces.len())].iter().all(|q| matches!(q.kind, Kind::PageBreak | Kind::ColumnBreak));
        let mut content = pieces.iter().filter(|q| q.kind != Kind::Anchor);
        let lone_picture = compat == Compat::Legacy && content.next().is_some_and(|q| q.kind == Kind::Object) && content.next().is_none();
        // Text laid out before the paragraph in this column ends at `*y`, then the space after it keeps.
        let kept = carry.prev.as_ref().map_or(carry.prev_after, |ps| gap(&self.ctx.env.text, ps, carry.prev_after, style, 0.0));
        // The paragraph's top is above its space before: where that text and its kept space after end.
        let origin = if at_top { *y } else { *y + kept };
        // The space before is part of the first line's box: a float beside it wraps the line.
        let lead = if first_part || restarts { (top - origin).max(0.0) } else { 0.0 };
        // Bands
        let mut bands: Vec<(f64, Vec<Line>)> = Vec::new();
        let mut yy = top;
        let mut s = 0;
        let mut guard = 0;
        // Compat ≤ 14 takes the floats in the order they were placed. The strip a line was fitted into with a slack
        // (its pieces and where they end) serves the next lines too while it ends below their top, by at least that
        // slack when it was fitted in one piece. Once a line went down in its own query, the paragraph's later queries
        // take its height plus that drop as the slack.
        // ponytail: a strip found with no slack isn't kept (the next line's own query gives the same pieces where
        // measured), one found with the paragraph's slack is kept to its end, both reset with each paragraph, and a
        // second drop keeps the first slack.
        let mut ordered: Vec<&Placed> = floats.iter().collect();
        ordered.sort_by_key(|f| f.key);
        let mut strip_at: Option<(Vec<(f64, f64)>, f64, f64)> = None;
        let mut slack_on = 0.0;
        // Compat 15: the page's first line, holding the anchor of a float that found no place, goes below the page's
        // floats, across the whole column.
        let unplaced = compat == Compat::Generic
            && at_top
            && self.column == 0
            && !self.mid_page
            && self.at.known.iter().any(|k| k.unplaced && (k.key.0, k.key.1) == (self.sec.index, pos.block));
        if unplaced {
            let low = floats.iter().filter(|f| f.wrap != Wrap::None).map(|f| f.excl[3]).fold(yy, f64::max);
            yy = low.min(col.bottom.to_pt());
        }
        if compat == Compat::Legacy && style.keep_lines && first_part && !at_top {
            let own = floats.iter().filter(|f| {
                f.frozen && (f.key.0, f.key.1) == (self.sec.index, pos.block) && f.wrap != Wrap::None && f.excl[3] > yy + EPS && f.excl[0] < cx1 - EPS && f.excl[2] > cx0 + EPS
            });
            yy = own.map(|f| f.excl[3]).fold(yy, f64::max);
        }
        while (s < n || bands.is_empty()) && guard < 4000 {
            guard += 1;
            let mut h = est;
            let mut below = est_below;
            let mut got = None;
            let mut blocked = false;
            let mut trial = None;
            // Every piece of the line was cut away or too narrow.
            let mut no_room = false;
            let here: &[Placed] = if yy >= col.bottom.to_pt() - EPS || unplaced && bands.is_empty() { &[] } else { &floats };
            let mut uncut: Option<Vec<Placed>> = None;
            let mut forced: Option<Vec<(f64, f64)>> = None;
            let mut fitted: Option<(Vec<(f64, f64)>, f64, f64, f64)> = None;
            let mut moved = None;
            let order: &[&Placed] = if here.is_empty() { &[] } else { &ordered };
            let end_of = |ok: &[(f64, f64, f64)]| ok.iter().map(|p| p.2).fold(f64::INFINITY, f64::min);
            // The pieces from `v0` with that slack: lower, at where the strip ends, while none reaches further than
            // the slack below `v0`. With the top they were found at.
            let query = |v0: f64, slack: f64, h: f64| -> Option<(f64, Vec<(f64, f64, f64)>)> {
                let mut t = v0;
                for _ in 0..64 {
                    if t >= col.bottom.to_pt() - EPS {
                        return None;
                    }
                    let found = float::strip(order, cx0, cx1, t, t + h.max(slack), slack)?;
                    let ok: Vec<(f64, f64, f64)> = found.iter().copied().filter(|p| slack < p.2 - v0 - EPS).collect();
                    if !ok.is_empty() && t > v0 + EPS {
                        // Kept lower: fitted again there with a slack down to where its strip ends.
                        let slack = end_of(&ok) - v0;
                        let again = float::strip(order, cx0, cx1, t, t + h.max(slack), slack).unwrap_or_default();
                        let again: Vec<(f64, f64, f64)> = again.into_iter().filter(|p| slack < p.2 - t - EPS).collect();
                        return Some((t, if again.is_empty() { ok } else { again }));
                    }
                    if !ok.is_empty() {
                        return Some((t, ok));
                    }
                    let covering = order
                        .iter()
                        .filter(|f| f.wrap != Wrap::None && f.excl[1] <= t + slack + EPS && f.excl[3] > t + EPS && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS);
                    let end = found.iter().map(|p| p.2).chain(covering.map(|f| f.excl[3])).fold(f64::INFINITY, f64::min);
                    if !end.is_finite() || end <= t + EPS {
                        return None;
                    }
                    t = end;
                }
                None
            };
            let pieces_of = |ok: &[(f64, f64, f64)]| ok.iter().map(|p| (p.0, p.1)).collect::<Vec<(f64, f64)>>();
            let reuse = compat == Compat::Legacy && strip_at.as_ref().is_some_and(|(_, end, least)| end - yy > EPS && end - yy >= least - EPS);
            if reuse {
                forced = strip_at.as_ref().map(|x| x.0.clone());
            } else if compat == Compat::Legacy && slack_on > EPS {
                if let Some((t, ok)) = query(yy, slack_on, h) {
                    fitted = Some((pieces_of(&ok), end_of(&ok), 0.0, 0.0));
                    forced = Some(pieces_of(&ok));
                    yy = t;
                }
            } else if compat == Compat::Legacy {
                let near = || here.iter().filter(|f| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS);
                let over: Vec<Placed> = near().filter(|f| f.excl[1] <= yy + EPS).cloned().collect();
                let e = over.iter().map(|f| f.excl[3]).fold(f64::INFINITY, f64::min);
                // A float starting inside the band counts only where it meets what the floats over the band's top
                // leave of the line: else it neither moves nor cuts the line.
                let left = float::intervals(&over, cx0, cx1, yy, yy + h, true).unwrap_or_default();
                let meets = |f: &Placed| left.iter().any(|&(a, b)| f.excl[0] < b - EPS && f.excl[2] > a + EPS);
                let hidden = |f: &Placed| f.in_band(yy, yy + h) && f.excl[1] > yy + EPS && !meets(f);
                // The line is fitted again with a slack: where they leave it one piece, down to where that piece ends,
                // else its height. A piece is kept only if it reaches further than the slack below the line's top;
                // with none the line goes down to where the strip ends and is fitted there.
                if e < yy + h - EPS && near().any(|f| f.excl[1] > yy + EPS && meets(f)) {
                    let one = float::strip(order, cx0, cx1, yy, yy + h, 0.0).filter(|p| p.len() == 1).map(|p| p[0].2 - yy);
                    let slack = one.unwrap_or(h);
                    match query(yy, slack, h) {
                        Some((t, ok)) => {
                            fitted = Some((pieces_of(&ok), end_of(&ok), one.map_or(0.0, |_| slack), t - yy));
                            forced = Some(pieces_of(&ok));
                            moved = Some(t);
                        }
                        None => {
                            yy = e;
                            continue;
                        }
                    }
                }
                if here.iter().any(hidden) {
                    uncut = Some(here.iter().filter(|f| !hidden(f)).cloned().collect());
                }
                if style.keep_lines && forced.is_none() {
                    let top = near().filter(|f| f.wrap != Wrap::None && f.excl[1] > yy + EPS).map(|f| f.excl[1]).fold(f64::INFINITY, f64::min);
                    if top < yy + h - EPS {
                        yy = top;
                        continue;
                    }
                }
            }
            if let Some(t) = moved {
                yy = t;
            }
            let cutting = uncut.as_deref().unwrap_or(here);
            for attempt in 0..2 {
                let free = match &forced {
                    Some(iv) => Some(iv.clone()),
                    None => {
                        // A float over the whole line that reaches only into the space before doesn't move it.
                        let at = |t: f64| float::text_intervals(cutting, cx0, cx1, t, yy + h, yy + h - below, compat == Compat::Legacy);
                        let lead = if bands.is_empty() { lead } else { 0.0 };
                        at(yy - lead).or_else(|| at(yy).filter(|_| lead > EPS))
                    }
                };
                match free {
                    None => {
                        blocked = true;
                        break;
                    }
                    Some(iv) => {
                        no_room = iv.is_empty();
                        let ivc: Vec<(f64, f64)> = iv.iter().map(|(a, b)| (a - cx0, b - cx0)).collect();
                        // A word too long for the line breaks after the last character that fits: in an uncut line,
                        // and in compat ≤ 14 in a line's only piece beside a float.
                        let open = iv.len() == 1
                            && (compat == Compat::Legacy || (iv[0].0 - cx0).abs() < EPS && (iv[0].1 - cx1).abs() < EPS);
                        let first = (first_part || restarts) && bands.is_empty();
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
                                // Compat ≤ 14 tests the paragraph's last line with its space after.
                                let tail = if compat == Compat::Legacy && e >= n { kept_after } else { 0.0 };
                                let (ph, segs_below) = (segs[0].pitch + tail, segs[0].below + tail);
                                let done = (ph - h).abs() < EPS && (segs_below - below).abs() < EPS || attempt == 1;
                                got = Some((segs, e));
                                if done {
                                    break;
                                }
                                h = ph;
                                below = segs_below;
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
                    if segs.last().is_some_and(|l| !l.hyphen) {
                        frame.word_from = Some(e);
                    }
                    if let Some((iv, end, stay, drop)) = fitted {
                        strip_at = Some((iv, end, stay));
                        if drop > EPS && slack_on <= EPS {
                            slack_on = ph + drop;
                        }
                    } else if !reuse {
                        strip_at = None;
                    }
                    bands.push((yy, segs));
                    let next = yy + ph;
                    yy = match clear {
                        Some(((l, r), x)) if compat == Compat::Generic => {
                            let iv = float::intervals(here, cx0, cx1, next - ph, next, false).unwrap_or_default();
                            let (a, b) = iv.iter().copied().find(|&(a, b)| cx0 + x >= a - EPS && cx0 + x <= b + EPS).unwrap_or((cx0, cx1));
                            here.iter()
                                .filter(|f| f.in_band(next - ph, next) && ((l && f.excl[2] <= a + EPS) || (r && f.excl[0] >= b - EPS)))
                                .map(|f| f.excl[3])
                                .fold(next, f64::max)
                        }
                        Some(((l, r), _)) => {
                            let mut at = next;
                            while at < col.bottom.to_pt() - EPS {
                                let first = float::intervals(here, cx0, cx1, at, at + ph, true).and_then(|iv| iv.first().copied());
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
                    // A vertical section's table moves the line to its far side whole.
                    let square = here.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square" && f.key.2 != float::UPRIGHT_TABLE);
                    // Compat ≤ 14: a line whose content fits no piece retries lower in steps of its height. One left no
                    // piece at all first goes to the top of a float starting inside it, then down by its height or to
                    // the bottom of the floats over its top, whichever is nearer.
                    if let Some(t) = trial.filter(|&t| compat == Compat::Legacy && square && t > EPS) {
                        let starts = here.iter().filter(meets).filter(|f| f.excl[1] > yy + EPS).map(|f| f.excl[1]).fold(f64::INFINITY, f64::min);
                        let covers = here.iter().filter(meets).filter(|f| f.excl[1] <= yy + EPS).map(|f| f.excl[3]).fold(f64::INFINITY, f64::min);
                        let next = match no_room {
                            true if starts < yy + h - EPS => starts,
                            true => (yy + t).min(covers.max(yy + EPS)),
                            false => yy + t,
                        };
                        yy = next.min(col.bottom.to_pt().max(yy + EPS));
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
        let mut kept_at = Vec::new();
        let mut forced: Option<usize> = None;
        // Compat 15, a float that can't be placed: the line holding its anchor goes to the next column (the next
        // page from a column's top) when it doesn't start the page; when it does, the page ends after it.
        let page_first = at_top && self.column == 0 && !self.mid_page;
        let mut push: Option<usize> = None;
        let mut page_end = false;
        let band_of = |k: usize| bands.iter().position(|(_, segs)| segs.iter().any(|l| k >= l.start && k < l.end.max(l.start + 1)));
        for (k, q) in pieces.iter().enumerate() {
            let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, sp.object(pb.from + k)) else { continue };
            let Some(b) = band_of(k) else { continue };
            let owner = piece_para(sp, pb.from + k, anchor);
            let f = &float::sized(f, &frames).map_or_else(|| f.clone(), Rc::new);
            let f = &self.fit_frame(f, col.width, memos)?;
            let h = self.float_height(f, memos)?;
            let line_top = bands[b].0;
            let para_top = if first_part { origin } else { bands[0].0 };
            let mut x = float::h_pos(f, &frames);
            if compat == Compat::Legacy && f.h_relative == "column" {
                match f.h_align.as_deref() {
                    Some("left") => x += style.ind_left,
                    Some("right") => x -= style.ind_right,
                    // From a line that starts beside another paragraph's float: from where its piece starts.
                    None => {
                        let others: Vec<Placed> = floats.iter().chain(&self.at.found).filter(|o| (o.key.0, o.key.1) != (si, bi)).cloned().collect();
                        let piece = float::intervals(&others, cx0, cx1, line_top, line_top + bands[b].1[0].pitch, true);
                        x += piece.and_then(|iv| iv.first().map(|p| p.0)).unwrap_or(cx0) - cx0;
                    }
                    _ => {}
                }
            }
            let mut fy = float::v_pos(f, h, &frames, para_top, line_top);
            // Compat ≤ 14: a drawing's page position in whole twips, for the collision with the lines above and the
            // drawing. Frames keep theirs.
            if compat == Compat::Legacy && (f.text_box.is_none() || f.shape.is_some()) {
                (x, fy) = ((x * 20.0).round() / 20.0, (fy * 20.0).round() / 20.0);
            }
            if Placed::new((si, bi, pb.from + k), owner, f, x, fy, h).wrap == Wrap::None {
                anchors.push(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h));
                continue;
            }
            // One that met text laid out before it keeps the place it met it at while the page is filled again.
            let frozen = self.at.known.iter().find(|o| o.key == (si, bi, pb.from + k) && o.frozen).cloned();
            if let Some(k) = &frozen {
                let mut a = self.wrapped(Placed::new(k.key, owner, f, k.rect[0], k.rect[1], h), memos)?;
                (a.collided, a.restarts, a.frozen) = (true, true, true);
                kept_at.push(a.key);
                anchors.push(a);
                continue;
            }
            (x, fy) = float::page_clamp(f, (x, fy), h, &frames);
            // Compat 15 keeps the box with its effect extents (not its distances) on the page.
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") {
                fy = fy.max(frames.margin[2]);
                let (l, r) = (x - f.effect[0], x + f.w + f.effect[2]);
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
                } else if !(page_first && b == 0) {
                    push = Some(push.map_or(b, |x: usize| x.min(b)));
                    continue;
                } else {
                    let mut a = Placed::new((si, bi, pb.from + k), owner, f, x, fy, h);
                    a.wrap = Wrap::None;
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                    page_end = true;
                    anchors.push(a);
                    continue;
                }
            }
            let mut a = self.wrapped(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h), memos)?;
            // That text (with its space after in compat ≤ 14): a float wholly below it, or beside the column, doesn't
            // meet it. Earlier columns count as met.
            let reach = *y + if compat == Compat::Legacy { kept } else { 0.0 };
            let met = a.excl[1] < reach - EPS && a.excl[0] < cx1 - EPS && a.excl[2] > cx0 + EPS;
            a.restarts = self.column > 0 || met || a.excl[1] < line_top + bands[b].1[0].pitch - EPS && a.excl[0] < cx1 - EPS && a.excl[2] > cx0 + EPS;
            // Only the first float of the fill that meets earlier text of its column met it where it will stay: that
            // text wraps it in the next fill. A later one was placed beside text that moves, and is placed again.
            // Compat ≤ 14: one that meets the lines of its own keepLines paragraph below a column's top stays too, and
            // the paragraph starts below it.
            let own = compat == Compat::Legacy && style.keep_lines && !at_top && a.restarts && a.excl[3] > bands[0].0 + EPS;
            a.frozen = (met || own) && !self.at.hit;
            a.collided = a.frozen || self.column > 0 && !met;
            anchors.push(a);
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
            let tail = if bi_ + 1 == nb { after } else { 0.0 };
            let note_tail = if refs.iter().any(|r| r.0 == NoteKind::Foot) { tail } else { 0.0 };
            if !self.add_notes(&refs, &mut a, col, bt + room + note_tail, &mut rest, false, memos)? && !lead {
                break;
            }
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
                // A frame placed in the flow: compat ≤ 14 lays it out below an earlier frame whose box with its
                // distances it meets, from the nearest twip; compat 15 moves one that may not overlap only down.
                let frame = |o: &Placed| o.float.text_box.is_some() && o.float.shape.is_none();
                let in_flow = frame(&a) && a.float.v_relative == "paragraph" && a.float.v_offset == 0.0;
                let mut under = Vec::new();
                // Compat ≤ 14 places a frame that may not overlap, or any frame once the page holds one, at the nearest
                // twip, its height kept.
                let apart = !a.allow_overlap || self.at.found.iter().any(|o| frame(o) && !o.allow_overlap);
                if frame(&a) && apart && compat == Compat::Legacy {
                    a.moved_to(a.rect[0], (a.rect[1] * 20.0).round() / 20.0);
                }
                if in_flow && compat == Compat::Legacy {
                    for o in self.at.found.iter().filter(|o| frame(o)) {
                        if o.excl[0] < a.rect[2] - EPS && a.rect[0] < o.excl[2] - EPS && o.rect[1] < a.rect[3] - EPS && a.rect[1] < o.rect[3] - EPS {
                            // Over a frame placed from the page or margin it stays, unless one of the two may not
                            // overlap: then it goes a twip above that frame's bottom, in whole twips.
                            let y = match matches!(o.float.v_relative.as_str(), "page" | "margin") {
                                false => (o.rect[3] * 20.0).round() / 20.0,
                                true if a.allow_overlap && o.allow_overlap => continue,
                                true => ((o.rect[1] * 20.0 + ((o.rect[3] - o.rect[1]) * 20.0).round() - 1.0) / 20.0).max(a.rect[1]),
                            };
                            a.moved_to(a.rect[0], y);
                            under.push(o.key);
                        }
                    }
                }
                let across = if in_flow && compat == Compat::Generic { (f64::INFINITY, f64::NEG_INFINITY) } else { (0.0, frames.page_w) };
                let others: Vec<Placed> = self.at.found.iter().filter(|o| !under.contains(&o.key)).cloned().collect();
                // Compat ≤ 14: once a float (not a frame) later on the page met the text laid out before it, the page
                // filled again keeps a frame where it was placed, unmoved.
                let restarted = compat == Compat::Legacy
                    && frame(&a)
                    && self.at.known.iter().any(|k| k.restarts && !frame(k) && k.wrap != Wrap::None && k.key.0 == a.key.0 && (k.key.1, k.key.2) > (a.key.1, a.key.2));
                if !restarted && !kept_at.contains(&a.key) && !float::avoid(&mut a, &others, across, (col.top.to_pt(), col.bottom.to_pt()), compat) {
                    let b = band_of(a.key.2 - pb.from).unwrap_or(0);
                    if compat == Compat::Generic && !(page_first && b == 0) {
                        push = Some(push.map_or(b, |x: usize| x.min(b)));
                        continue;
                    }
                    // Starting the page, it stays where it was asked for and the line goes below the page's floats.
                    a.unplaced = compat == Compat::Generic;
                    page_end |= compat == Compat::Generic;
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                }
                if a.restarts && !self.at.known.iter().any(|k| k.same(&a)) {
                    self.at.hit = true;
                }
                self.at.found.push(a);
            }
        }
        if let Some(b) = forced {
            take = take.min(b + 1);
        }
        // The split before a pushed line keeps the paragraph's own rules; the page's first paragraph keeps the lines before it.
        if let Some(b) = push {
            let keep = lines_to_take(nb, b, false, first_part, style.keep_lines, style.widow_control);
            take = take.min(if keep == 0 && page_first { b } else { keep });
            let end = if take == 0 { 0 } else { bands[take - 1].1.last().map_or(0, |l| l.end) };
            self.at.found.retain(|f| (f.key.0, f.key.1) != (si, bi) || f.key.2 == float::TABLE_PIECE || f.key.2 < pb.from + end);
            if take == 0 {
                carry.keep_top = false;
                return Ok(if at_top { Stop::Page } else { Stop::Full });
            }
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
            carry.letter_break = !take.checked_sub(1).and_then(|k| bands[k].1.last()).is_some_and(|l| l.hyphen);
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
                if page_end { Stop::Page } else { Stop::Full }
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
        // Compat ≤ 14: the floats of the first row placed from the page, each from the row top, the first row's
        // over it and the later rows' below it.
        let row_floats: Vec<(&Float, ParaId, f64, f64)> = match (tb.rows.first(), self.sec.blocks.get(pos.block)) {
            (Some(_), Some(Block::Table(t))) if !floating && pos.at.at_start() && !page_floats.is_empty() => t
                .rows
                .first()
                .into_iter()
                .flat_map(|row| row.cells.iter())
                .flat_map(|c| c.blocks.iter())
                .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                .flat_map(|p| p.inlines.iter().map(move |i| (para_id(p), i)))
                .filter_map(|(id, i)| if let Inline::Float(f, _) = i { page_floats.get(&f.id).map(|f| (*f, id)) } else { None })
                .filter(|(f, _)| {
                    matches!(f.wrap.as_str(), "square" | "topAndBottom")
                        && f.h_relative == "column"
                        && f.v_relative == "paragraph"
                        && f.h_align.is_none()
                        && f.v_align.is_none()
                })
                .map(|(f, id)| (f, id, f.v_offset - f.dist[0], f.v_offset + f.h + f.dist[1]))
                .collect(),
            _ => Vec::new(),
        };
        let first_h = tb.rows.first().map_or(0.0, |r| r.height);
        // The row goes beside those over it when the table fits the first piece they leave, else below them; they
        // keep their place, and wrap the text after the table.
        let y0 = *y;
        let mut over: Vec<Placed> = row_floats
            .iter()
            .filter(|(_, _, top, _)| *top < first_h)
            .enumerate()
            .map(|(k, (f, id, _, _))| {
                let mut a = Placed::new((self.sec.index, pos.block, k), *id, &Rc::new((*f).clone()), float::h_pos(f, &frames), y0 + f.v_offset, f.h);
                a.collided = false;
                a
            })
            .collect();
        let spot = if pos.at.at_start() && !floating && self.ctx.env.text.vertical.is_none() {
            self.beside_floats(pos.block, tb, col, *y, &over, memos)?
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
            _ => beside.unwrap_or(col.x),
        };
        let mut settled = true;
        let mut first: Option<(usize, f64)> = None;
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
                Some(Block::Table(t)) => super::table::float_box(tb, t, legacy),
                _ => (0.0, 0.0),
            };
            let [dt, db, ..] = fp.dist;
            // Compat ≤ 14 puts a table placed in the flow, from the nearest twip, below an earlier table whose obstacle
            // its text box (a twip left of `tblpX`, the indent added to its width) reaches.
            let in_flow = legacy && fp.v_anchor == TableAnchor::Text && fp.y == 0.0;
            let tables: Vec<Placed> = self.at.found.iter().filter(|o| o.key.2 == float::TABLE_PIECE).cloned().collect();
            let (c0, c1) = super::table::cells_box(tb);
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
                let f = table_float(t.id, br - bl, h, table_dist(&fp, in_flow), !fp.never_overlap);
                let mut probe = Placed::new((self.sec.index, pos.block, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, h);
                let body_bottom = self.page_h - self.sec.section.section_in(self.ctx.env.text.review.view).bottom;
                let others: Vec<Placed> = tables.iter().filter(|o| !under.contains(&o.key)).cloned().collect();
                float::avoid(&mut probe, &others, (self.margins.0, self.page_w), (self.margins.2, body_bottom), self.ctx.env.text.compat);
                (at_x, top) = (probe.rect[0] - bl, probe.rect[1]);
            }
            *y = self.at.known.iter().find(|k| k.key == (self.sec.index, pos.block, float::TABLE_PIECE)).map_or(top, |k| k.rect[1]);
            if unsplit && !*empty && !col.bottom.holds(*y + h) {
                return Ok(Stop::Full);
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
                return Ok(self.text_goes_on(true, pos, carry, y, empty, (flow_y, was_empty)));
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
        let later: Vec<(f64, f64)> = {
            let (l, r) = (at_x + tb.x0, at_x + tb.x0 + tb.cols.iter().sum::<f64>());
            row_floats
                .iter()
                .filter(|(f, _, top, _)| *top >= first_h && {
                    let x = float::h_pos(f, &frames);
                    x < r && x + f.w > l
                })
                .map(|(_, _, top, bottom)| (y0 + top, y0 + bottom))
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
                height.min((column_h - row.top_border - tb.bottom_border).max(0.0))
            } else {
                height
            };
            if rest.is_none()
                && !rows.is_empty()
                && let Some(&(_, bottom)) = later.iter().find(|&&(t, b)| *y < b - EPS && t < *y + height - EPS)
            {
                if let Some(&(rs, top, idx)) = placed.last().filter(|p| p.2 + 1 == rows.len()) {
                    let (mut drawn, _) = draw_whole_row(tb, rs, at_x, top, tb.rows[rs].height, true, memos);
                    move_floats(Some(&mut drawn), &page_floats, &frames, up(rs));
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
                move_floats(rows.last_mut(), &page_floats, &frames, up(ri));
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
                    move_floats(Some(&mut redrawn), &page_floats, &frames, up(rs));
                    rows[idx] = redrawn;
                }
                row_refs.extend(cell_refs(&fitted, rows.len()));
                rows.push(draw_row(tb, ri, at_x, *y, h, Some(&fitted), true));
                move_floats(rows.last_mut(), &page_floats, &frames, up(ri));
                *y += h + tb.bottom_border;
                *empty = false;
                pos.at = At::RowPart { row: ri, rest: Rc::new(left) };
                self.show_endnote_refs(row_refs.into_iter().map(|r| r.1));
                self.at.found.append(&mut over);
                finish(rows, out, at_x, opened, false);
                self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y, settled);
                return Ok(self.text_goes_on(fp.is_some(), pos, carry, y, empty, (flow_y, was_empty)));
            }
            if at_top {
                row_refs.extend(cell_refs(&parts, rows.len()));
                rows.push(draw_row(tb, ri, at_x, *y, height, Some(&parts), last));
                move_floats(rows.last_mut(), &page_floats, &frames, up(ri));
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
                    move_floats(Some(&mut redrawn), &page_floats, &frames, up(rs));
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
            self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y, settled);
            return Ok(self.text_goes_on(fp.is_some() && split, pos, carry, y, empty, (flow_y, was_empty)));
        }
        self.show_endnote_refs(row_refs.into_iter().map(|r| r.1));
        if !rows.is_empty() {
            self.at.found.append(&mut over);
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

    /// Where an in-flow table starting at `y` goes beside the floats over its first row: in the first free piece of
    /// the column there, its indent from the piece's start and its `jc` within the piece, when its first row fits
    /// the piece; else lower, at the first float bottom where it does. Compat ≤ 14 lays an autofit table out again
    /// at the piece's width. All its rows keep that place. `None` when no float meets the row.
    fn beside_floats(
        &mut self,
        bi: usize,
        tb: &TableLayout,
        col: &ColumnSpace,
        y: f64,
        own: &[Placed],
        memos: &mut Memos,
    ) -> Result<Option<(f64, f64, Option<Rc<TableLayout>>)>, Yield> {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { return Ok(None) };
        let Some(ri) = tb.rows.iter().position(|r| !r.hidden) else { return Ok(None) };
        let si = self.sec.index;
        let floats: Vec<Placed> =
            self.fixed.iter().chain(self.known_at(bi)).filter(|f| (f.key.0, f.key.1) != (si, bi)).chain(own).cloned().collect();
        if tb.rtl || floats.is_empty() {
            return Ok(None);
        }
        let text = &self.ctx.env.text;
        let legacy = text.compat == Compat::Legacy;
        let (cx0, cx1) = (col.x, col.x + col.width);
        let (mut top, h) = (y, tb.rows[ri].height + if tb.is_last(ri) { tb.bottom_border } else { 0.0 });
        let mut moved = false;
        while top < col.bottom.to_pt() - EPS {
            if let Some((u, r)) = float::intervals(&floats, cx0, cx1, top, top + h, legacy).and_then(|iv| iv.first().copied()) {
                if u <= cx0 + EPS && r >= cx1 - EPS {
                    break;
                }
                let w = r - u;
                let relaid = if legacy && !t.fixed && !text.no_constrained_autofit {
                    let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
                    let prev = self.sec.blocks.neighbours(bi, bi, text.review.view).0;
                    let laid = super::table::layout(&super::table::in_column(t, col.width, &env), w, &env, prev, false, memos)?;
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
            match float::next_edge(&floats, cx0, cx1, top, top + h) {
                Some(e) if e > top + EPS => (top, moved) = (e, true),
                _ => break,
            }
        }
        Ok(moved.then_some((cx0, top, None)))
    }

    /// A table in a vertical section, laid out horizontally and drawn upright: at the stack position the next line
    /// would take (compat ≤ 14 moves it back towards the earlier lines by its legacy lead), from the line start or
    /// below an earlier such table it meets, its own `jc` and indent ignored. It doesn't advance the stack: the lines
    /// it meets, 9pt either side across the page, wrap past it along the line. In compat 15 a table that doesn't fit
    /// across the rest of the page goes to the next one, unless the page has nothing else. A floating table keeps its
    /// left and right distances from the text, on those sides as printed, and starts `tblpY` (or where `tblpYSpec`
    /// puts it) along the line, a `tblpX` from the text further along the stack; one placed from the page on both
    /// axes sits at `tblpX`, `tblpY` as printed.
    #[allow(clippy::too_many_arguments, reason = "as place_table")]
    fn place_upright_table(
        &mut self,
        bi: usize,
        pos: &mut FlowToken,
        carry: &mut FlowState,
        col: &ColumnSpace,
        y: &mut f64,
        empty: bool,
        notes: &mut NoteArea,
        memos: &mut Memos,
    ) -> Result<Stop, Yield> {
        let Some(Block::Table(t)) = self.sec.blocks.get(bi) else { unreachable!("an upright table") };
        let Some(v) = self.ctx.env.text.vertical else { unreachable!("a vertical section") };
        if !empty {
            *y += self.after_above_table(bi, carry);
        }
        let text = super::env::TextEnv { vertical: None, ..self.ctx.env.text.clone() };
        let le = super::LayoutEnv { text, ..self.ctx.env.clone() };
        let env = StoryEnv { ctx: LayoutCtx { env: &le, ..self.ctx }, fields: self.fields, notes: self.note_numbers };
        let prev = self.sec.blocks.neighbours(bi, bi, self.ctx.env.text.review.view).0;
        let tb = super::table::layout(t, col.bottom.to_pt() - col.top.to_pt(), &env, prev, false, memos)?;
        let (bl, br) = super::table::outer_box(&tb);
        let h: f64 = tb.rows.iter().filter(|r| !r.hidden).map(|r| r.height).sum::<f64>() + tb.bottom_border;
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        let back = if legacy { super::table::legacy_lead(t) } else { 0.0 };
        let down = v.flow != super::vertical::Flow::DownEast;
        let fp = t.floating;
        let paged = fp.is_some_and(|fp| fp.v_anchor == TableAnchor::Page && fp.h_page);
        let y0 = match fp {
            Some(fp) if paged && down => self.page_h - (fp.x - back) - (br - bl),
            Some(fp) if paged => fp.x - back,
            // `tblpX` from the text runs along the stack.
            Some(fp) if fp.h_text => *y - back + fp.x,
            _ => *y - back,
        };
        let end = col.x + col.width;
        let meeting = |y0: f64| {
            let y1 = y0 + br - bl;
            self.at.found.iter().filter(move |o| o.key.2 == float::UPRIGHT_TABLE && o.excl[1] < y1 - EPS && o.excl[3] > y0 + EPS)
        };
        let below_tables = |y0: f64| meeting(y0).map(|o| o.excl[2]).fold(col.x, f64::max);
        // The distances across, on the turned page's top and bottom: the sides as printed swap when the lines stack
        // from the right.
        let (l, r) = fp.map_or((9.0, 9.0), |fp| (fp.dist[2], fp.dist[3]));
        let (above, below) = if down { (r, l) } else { (l, r) };
        // Lines pushed past it don't move it: it stays where an earlier attempt at the page put it. In compat 15 the
        // lines before it that went past an earlier table take it along when it was there moved past that table or
        // ran past the line end below it.
        let passed = |k: f64| {
            let (moved, short) = (|o: &Placed| (o.excl[3] + above - k).abs() < EPS, below_tables(k) + h > end + EPS);
            self.at.found.iter().filter(|o| o.key.2 == float::UPRIGHT_TABLE && o.excl[3] <= y0 + EPS).any(|o| moved(o) || (short && meeting(k).any(|m| m.same(o))))
        };
        let known_rect = self.at.known.iter().find(|k| k.key == (self.sec.index, bi, float::UPRIGHT_TABLE)).map(|k| k.rect);
        let known = known_rect.map(|r| r[1]).filter(|&k| legacy || !passed(k));
        // Compat ≤ 14 after lines that went across past an earlier table (a line starts at its obstacle's far edge
        // `e`): it takes the stack position after them, and keeps it when the lines then went past it too. It stays
        // there with the lines that meet it below it when its obstacle clears the first of those lines or spans the
        // earlier table's on the page; else, when the earlier table's far edge (less its legacy lead) is at or past
        // the middle of the text across and the lines still fit past it before the far margin, it runs the line for
        // them and they go across past it again; else it goes to the next page.
        let mut full_line = false;
        let mut jumped_at = None;
        if legacy && fp.is_none() && carry.upright_at.is_none() {
            let upright = || self.at.found.iter().filter(|o| o.key.2 == float::UPRIGHT_TABLE && o.excl[3] <= *y + EPS);
            let after = known_rect.filter(|r| *y >= r[3] + below - EPS && upright().any(|o| o.excl[3] <= r[1] + EPS));
            if let Some(r) = after {
                (full_line, jumped_at) = (true, Some(r[1]));
            } else if let Some(o) = upright().filter(|o| self.at.starts.iter().any(|s| (s.0 - o.excl[3]).abs() < EPS)).max_by(|a, b| a.excl[3].total_cmp(&b.excl[3])) {
                let e = o.excl[3];
                let lead = match self.sec.blocks.get(o.key.1) {
                    Some(Block::Table(t1)) => super::table::legacy_lead(t1),
                    _ => back,
                };
                let second = self.at.starts.iter().map(|s| s.0).filter(|&s| s > e + EPS && s < *y - EPS).fold(*y, f64::min);
                let (top, bottom) = (col.top.to_pt(), col.bottom.to_pt());
                // ponytail: an earlier table at the stack's start keeps it there too; Word then draws the line between
                // them over it and the earlier table further out, which isn't modelled.
                let clears = y0 - above >= second - EPS || br - bl + above + below >= e - top - EPS || o.rect[1] <= top + EPS;
                if !clears {
                    let fits = y0 + br - bl + below + (*y - e) <= bottom + EPS;
                    if e + lead > (top + bottom) / 2.0 + EPS || !fits {
                        return Ok(Stop::Full);
                    }
                    full_line = true;
                }
                jumped_at = Some(y0);
            }
        }
        let stack = y0;
        // Its later parts go on where the first one was, across the page.
        let y0 = carry.upright_at.filter(|_| !pos.at.at_start()).or(jumped_at).or(known).unwrap_or(y0);
        // Compat 15 moves a table that runs past the line end below the tables it meets across the stack, past the
        // one that holds it back (their distances touching). Where that would put its far distance past the page's
        // edge it stays, drawn from the line start over them.
        let (own, mut y0) = (y0, y0);
        while !legacy && fp.is_none() && below_tables(y0) > col.x + EPS && below_tables(y0) + h > end + EPS {
            let Some(b) = meeting(y0).max_by(|a, b| a.excl[2].total_cmp(&b.excl[2])) else { break };
            y0 = b.excl[3] + above;
        }
        let over = y0 > own + EPS && y0 + br - bl + below > self.page_h + EPS;
        if over {
            y0 = own;
        }
        let y1 = y0 + br - bl;
        // Compat ≤ 14 has no fit test: the table stays at its stack position, over the margin if need be.
        if y1 > col.bottom.to_pt() + EPS && !empty && !paged && !legacy {
            return Ok(Stop::Full);
        }
        let along = match fp {
            _ if over => col.x,
            Some(fp) if paged => fp.y,
            // `tblpYSpec` from the margin or the page: along the line as printed.
            Some(fp) if fp.v_anchor != TableAnchor::Text && fp.y_spec.is_some() => {
                let (a0, a1) = if fp.v_anchor == TableAnchor::Page { (0.0, self.page_w) } else { (col.x, col.x + col.width) };
                match fp.y_spec {
                    Some(Jc::Center) => (a0 + a1 - h) / 2.0,
                    Some(Jc::Right) => a1 - h,
                    _ => a0,
                }
            }
            Some(fp) => col.x + fp.y,
            None => below_tables(y0),
        };
        // Compat 15 also moves a table longer than the rest of the line to the next page; one that starts a page or
        // was moved across splits by rows at the line end, in either mode, its header rows repeated.
        if along + h > end + EPS && !empty && !legacy && fp.is_none() && y0 <= stack + EPS {
            return Ok(Stop::Full);
        }
        // On the page as printed: the turned page's y runs leftwards from its right edge, or rightwards when the
        // lines stack from the left.
        let left = if v.flow == super::vertical::Flow::DownEast { y0 } else { self.page_h - y1 };
        let bottom = if fp.is_some() { Lu::MAX } else { Lu::round_half_up(end) };
        // Compat ≤ 14: one that runs past the line end below an earlier table is drawn from the line start, over
        // it. It keeps the more of the rows that fit below that table (all that fit the line, when those reach past
        // its end) and the rows that fit once the table is stacked below its own image until it no longer fits
        // whole; at least one. When the rows that fit below that table reach past its end, the lines before meet
        // those rows only.
        let mut reach = None;
        let (along, bottom) = if legacy && along > col.x + EPS && along + h > end + EPS && fp.is_none() {
            let hs: Vec<f64> = tb.rows.iter().skip(pos.at.index()).filter(|r| !r.hidden).map(|r| r.height).collect();
            let bb = tb.bottom_border;
            let fit = |room: f64| {
                let mut s = bb;
                hs.iter().take_while(|&&r| {
                    s += r;
                    s <= room + EPS
                })
                .count()
            };
            let mut under = fit(end - along);
            let piece = hs[..under].iter().sum::<f64>() + bb;
            if under > 0 && col.x + piece - bb > along + EPS {
                reach = Some(piece);
                under = fit(col.width);
            }
            let whole = hs.iter().sum::<f64>() + bb;
            let mut room = col.width - whole;
            while whole > EPS && room >= whole {
                room -= whole;
            }
            let kept = under.max(fit(room)).max(1).min(hs.len());
            (col.x, Lu::round_half_up(col.x + hs[..kept].iter().sum::<f64>() + bb))
        } else {
            (along, bottom)
        };
        // Compat 15: a table whose notes can't start past it goes to the next page with them.
        if !legacy && !empty && !paged && pos.at.at_start() {
            let refs: Vec<(NoteKind, i64)> =
                tb.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.units.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            if refs.iter().any(|r| r.0 == NoteKind::Foot) && !self.add_notes(&refs, &mut notes.clone(), col, y1, &mut Vec::new(), false, memos)? {
                return Ok(Stop::Full);
            }
        }
        let page_col = ColumnSpace { x: left - bl, width: br - bl, top: Lu::round_half_up(along), bottom, balance: false };
        // Its notes don't count against its fit: they are collected aside and go to the page's note area.
        let mut area = NoteArea { shared: Some(0.0), below: true, ..NoteArea::default() };
        let (mut top, mut fresh, mut rows) = (along, true, Vec::new());
        let saved = std::mem::take(&mut self.at.starts);
        let stop = self.place_table(&tb, pos, carry, &page_col, &mut top, &mut fresh, &mut area, &mut rows, memos)?;
        carry.upright_at = (stop == Stop::Full).then_some(y0);
        self.at.starts = saved;
        self.at.upright.extend(rows);
        let refs: Vec<(NoteKind, i64)> = area.parts.iter().map(|p| (p.kind, p.id)).collect();
        if !refs.is_empty() {
            // From past the table across the page; compat ≤ 14 from where it starts. What doesn't fit goes on on
            // the next page.
            let from = if legacy { y0 } else { y1 };
            let (mut a, mut rest) = (notes.clone(), Vec::new());
            if self.add_notes(&refs, &mut a, col, from, &mut rest, false, memos)? {
                // A note cut there leaves no room for the text after the table.
                if !rest.is_empty() {
                    a.kept = (col.bottom.to_pt() - from - a.total()).max(0.0);
                }
                *notes = a;
                carry.note_rest.extend(rest);
            } else {
                // ponytail: a note that waits whole for the next page is drawn there as a continuation, under the
                // continuation separator; give it a fresh start if Word draws the plain separator there.
                for &(kind, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                    let units = self.note_units(kind, id, col.width, memos)?.units.clone();
                    self.at.effects.notes_shown.push((kind, id, self.note_numbers.get(&(kind, id)).cloned().unwrap_or_default()));
                    carry.note_rest.push(NotePart { kind, id, units });
                }
            }
        }
        let (from, to) = if full_line { (col.x, end) } else { (along, top) };
        let f = table_float(t.id, to - from, y1 - y0, [above, below, 0.0, 0.0], true);
        let mut o = Placed::new((self.sec.index, bi, float::UPRIGHT_TABLE), ParaId(t.id), &f, from, y0, y1 - y0);
        o.early_right = reach.map(|r| along + r);
        self.at.found.push(o);
        Ok(stop)
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
        let (bl, br) = super::table::float_box(tb, t, legacy);
        let f = table_float(t.id, br - bl, bottom - top, table_dist(&fp, in_flow), !fp.never_overlap);
        let mut o = Placed::new((self.sec.index, bi, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, bottom - top);
        o.excl[1] = o.excl[1].max(flow_y.min(top));
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
        self.at.found.push(o);
    }
}

/// A floating table's distances from the text: none above or below one placed in the flow.
pub(super) fn table_dist(fp: &crate::docx::props::TablePos, in_flow: bool) -> [f64; 4] {
    let [dt, db, dl, dr] = fp.dist;
    if in_flow { [0.0, 0.0, dl.max(0.5), dr.max(0.5)] } else { [dt, db, dl.max(0.5), dr.max(0.5)] }
}

/// A table's box as a square, both-sides float `dist` apart from the text.
pub(super) fn table_float(id: u64, w: f64, h: f64, dist: [f64; 4], allow_overlap: bool) -> Rc<Float> {
    Rc::new(Float {
        id,
        w,
        auto_w: false,
        h,
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
        dist,
        effect: [0.0; 4],
        poly: None,
        behind: false,
        allow_overlap,
        layout_in_cell: true,
        order: 0,
        z: 0,
        src: None,
        locked: false,
        frame_src: None,
        in_note: false,
        parts: Vec::new(),
        shape: None,
        rot_swap: false,
        relative: Default::default(),
    })
}
