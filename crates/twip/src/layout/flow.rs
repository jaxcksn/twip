//! Filling a column: paragraphs (PARA-1 to PARA-5, PAGE-1 to PAGE-6), paragraphs wrapped around floats (FLOAT-2 to
//! FLOAT-9), table rows (TABLE-40 to TABLE-48), footnotes (NOTE-1 to NOTE-8) and line numbers (LNUM-1, LNUM-2). Drawn
//! items are fragments, positions are tokens, and the page attempt's state is explicit ([`PageFill`]).

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::{Jc, TableAnchor};

use super::LayoutCtx;
use super::float::{self, Frames, Placed, Wrap};
use super::fragment::{BoxFragment, BoxKind, Child, Item, ParaFragment, PlacedLine, TableFragment};
use super::ids::ParaId;
use super::job::{Memos, Yield};
use super::join::{Joined, LaidPara, laid_para, piece_para};
use super::line::{Frame, Line, LineOrigin, ParaLines, band, border_key, break_lines, draw_label, draw_line, neighbour_borders};
use super::node::{Notes, SectionNode};
use super::paragraph::{gap, lines_to_take, space_after, space_before};
use super::record::PageEffects;
use super::space::{BorderKey, ColumnSpace, ParaSpace};
use super::story::{StoryEnv, StoryLayout, StoryUnit};
use super::table::{CellPart, DrawnRow, TableLayout, draw_row, draw_whole_row, whole};
use super::text::{FieldValues, Kind, Object, ShapedPara, para_id, shape};
use super::token::{At, FlowState, FlowToken, NotePart};

const EPS: f64 = 1e-6;

/// Why a column fill stopped.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    Done,
    Full,
    Column,
    Page,
}

/// One column's fill: its content, why it stopped, the bottom of its content (with the last space-after when
/// balancing), and where the flow continues.
pub struct ColumnResult {
    pub children: Vec<Child>,
    pub stop: Stop,
    pub bottom: f64,
    pub token: FlowToken,
    pub state: FlowState,
}

/// The footnotes of one column, stacked above its bottom (NOTE-1).
#[derive(Clone, Default, Debug)]
pub struct NoteArea {
    /// Placed note parts.
    pub parts: Vec<NotePart>,
    /// Content height of the parts, without the separator.
    pub height: f64,
    /// The separator's height (0 while the area is empty).
    pub sep: f64,
    /// The first part continues a note from the previous page (continuation separator).
    pub cont: bool,
    /// Compat 15 with several columns (COL-3): `parts` is the page's whole note stream, `height` its balanced height
    /// over `cols` columns, and this is the height reserved by the page's previous fill.
    pub shared: Option<f64>,
    pub cols: usize,
}

impl NoteArea {
    pub fn total(&self) -> f64 {
        self.sep + self.shared.map_or(self.height, |r| self.height.max(r))
    }
}

/// Tables already laid out in this pass, by their identity in the (immutable) document and the width: placing a table
/// on several pages, and checking whether its first row fits, asks for it repeatedly, and a cache probe rebuilds its
/// key from every cell. Exact: tables whose text varies by page (PAGE, SECTION, SECTIONPAGES) aren't memoized, and the
/// other page-dependent values (NUMPAGES, PAGEREF, note numbers) are fixed for a pass.
#[derive(Default)]
pub struct PassMemo {
    tables: HashMap<(usize, u64), Rc<TableLayout>>,
    varies: HashMap<usize, bool>,
    /// Paragraphs joined in this pass (REV-6), by their first paragraph's identity in the document and their count:
    /// pages and balancing attempts lay them out repeatedly.
    joined: HashMap<(usize, usize), Rc<Joined>>,
    /// The lines, tables and rows laid out since the last committed body, for its record to hold.
    pub(crate) held: Held,
}

/// The cached results a page body was laid out from. Its record holds them, so eviction (which drops only what the
/// cache alone holds) keeps them while the page is a baseline's: an incremental layout doesn't look up the pages it
/// copies, and its later pages, moved by an edit, find their paragraphs, tables and rows still laid out.
#[derive(Clone, Debug, Default)]
pub struct Held {
    lines: Vec<Rc<ParaLines>>,
    tables: Vec<Rc<TableLayout>>,
    rows: Vec<Rc<DrawnRow>>,
}

/// Adds `x` to `v` unless `v` holds it already.
fn hold<T>(v: &mut Vec<Rc<T>>, x: &Rc<T>) {
    if !v.iter().any(|y| Rc::ptr_eq(y, x)) {
        v.push(x.clone());
    }
}

impl PassMemo {
    /// The layout this pass made of `t` at `width`, if it was memoized.
    pub fn table(&self, t: &Table, width: f64) -> Option<Rc<TableLayout>> {
        self.tables.get(&(std::ptr::from_ref(t) as usize, width.to_bits())).cloned()
    }

    /// Memoizes `tb` as `t`'s layout at `width` (made for this pass's document before it started).
    pub fn seed(&mut self, t: &Table, width: f64, tb: Rc<TableLayout>) {
        self.tables.insert((std::ptr::from_ref(t) as usize, width.to_bits()), tb);
    }
}

/// Whether blocks show a field whose value differs between the pages of one pass.
fn varies_by_page(blocks: &[Block]) -> bool {
    blocks.iter().any(|b| match b {
        Block::Para(p) => p
            .inlines
            .iter()
            .any(|i| matches!(i, Inline::FieldBegin { kind: Some(FieldKind::Page | FieldKind::Section | FieldKind::SectionPages), .. })),
        Block::Table(t) => t.rows.iter().any(|r| r.cells.iter().any(|c| varies_by_page(&c.blocks))),
    })
}

/// One attempt at a page's body: the inputs it was started with and what it found, owned by the attempt.
#[derive(Clone, Default)]
pub struct AttemptState {
    /// The floats known on the page (text wraps around them).
    pub known: Vec<Placed>,
    /// Floats met by this attempt.
    pub found: Vec<Placed>,
    /// The page's shared note stream while its columns fill (compat 15, COL-3); None = one area per column.
    pub stream: Option<NoteArea>,
    /// A balancing fill had to drop a keep or widow rule in an empty column (COL-5: that height fails).
    pub forced: bool,
    /// The fill's paragraph portions (first line top, starts a paragraph), for vertical alignment (SETUP-1).
    pub starts: Vec<(f64, bool)>,
    /// Whether the attempt drew notes (SETUP-1 isn't applied then).
    pub notes_drawn: bool,
    pub effects: PageEffects,
    /// PAGE-6, compat 15: the keep-with-next paragraph (block) that must end on this page, and the most lines it may
    /// keep here.
    pub keep_cap: Option<(usize, usize)>,
}

/// A page being filled: the section, the inputs every column shares, and the attempt's state.
pub struct PageFill<'d, 'p> {
    pub ctx: LayoutCtx<'d>,
    pub sec: &'p SectionNode<'d>,
    pub notes: Notes<'d>,
    /// Body field values on this page (FIELD-1) and note numbers (NOTE-4).
    pub fields: &'p FieldValues,
    pub note_numbers: &'p HashMap<(NoteKind, i64), String>,
    /// The page geometry floats position against (FLOAT-6): page size, margins (left, right, top) after gutter and
    /// mirroring, and the section's bottom margin.
    pub page_w: f64,
    pub page_h: f64,
    pub margins: (f64, f64, f64),
    pub pass: &'p mut PassMemo,
    pub at: AttemptState,
}

/// A paragraph's line space at a width (PARA-7: the neighbours only matter to a bordered paragraph; a continuation's
/// "previous" is itself).
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
            (if from > 0 { border_key(style) } else { p }, n)
        }
    };
    ParaSpace { width, start: from, borders, x0, env: ctx.env.text.clone() }
}

/// How many of a note's units fit in `room`, with widow/orphan control (PAGE-3) inside the paragraph the cut falls in;
/// splits between paragraphs are unrestricted (NOTE-7).
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

/// How many of a cell's units stay on this page when its row splits (TABLE-41, TABLE-43, TABLE-44). A cell that keeps
/// nothing fails TABLE-42, so the row moves on whole. `continued`: the units continue a row split on an earlier page.
fn cell_cut(units: &[StoryUnit], room: f64, continued: bool, compat: Compat) -> usize {
    // TABLE-44: a cell line stays only if its full pitch fits (none of PAGE-2's allowance below the text), and its
    // paragraph's space-after when it ends it.
    let fit = units.iter().position(|u| u.top + u.height + u.after > room + EPS).unwrap_or(units.len());
    // TABLE-43: compat ≤ 14 cells have no widow, orphan or keep control at a row split.
    if compat == Compat::Legacy || fit == 0 || fit >= units.len() {
        return fit;
    }
    cell_keeps(units, widow_cut(units, fit, continued))
}

/// TABLE-43: widow and orphan control at a cell cut `fit`, inside the paragraph it falls in.
fn widow_cut(units: &[StoryUnit], fit: usize, continued: bool) -> usize {
    if units[fit - 1].block != units[fit].block || !units[fit].widow {
        return fit;
    }
    let para = units[fit].block;
    let start = units[..fit].iter().rposition(|u| u.block != para).map_or(0, |k| k + 1);
    let end = units[fit..].iter().position(|u| u.block != para).map_or(units.len(), |k| fit + k);
    // shortcut: a continuation's first paragraph is treated as begun on the earlier page (no orphan rule here), even
    // when the earlier split fell between paragraphs; exact only for rows split across more than two pages.
    let first_part = !(continued && start == 0);
    start + lines_to_take(end - start, fit - start, false, first_part, false, true)
}

/// TABLE-43 (compat 15): the body's keep rules inside a cell (PAGE-6). The cut doesn't fall inside a keepLines
/// paragraph, and moves back from a paragraph's start over a keepNext paragraph before it, taking that one's last line
/// (two with widowControl) or, with keepLines, all of it; the step goes on over a whole paragraph moved.
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

/// A merged region that splits with its last row (TABLE-32): its continuation cell `k` in that row, its restart cell
/// `kr` in row `start` (placed whole on this page, `placed[pi]`), how many of its units stay and where they end.
struct MergedSplit {
    k: usize,
    start: usize,
    kr: usize,
    pi: usize,
    cut: usize,
    bottom: f64,
}

/// TABLE-32: the merged regions that end in row `ri` and start in a row placed whole on this page (`placed`), whose
/// content runs past `bottom`: each keeps the lines that fit (TABLE-44) and the rest continues in `ri`'s continuation.
/// shortcut: only a top-aligned region split at its last row; a region split at an earlier row, or that keeps
/// nothing here, still draws whole from its first row.
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

/// The first row of merged region `m` drawn again with only the content that stays on this page, the region ending
/// with row `ri`'s part of height `h` (TABLE-32).
fn draw_merged_part(tb: &TableLayout, m: &MergedSplit, ri: usize, h: f64, left: f64, top: f64) -> super::fragment::RowFragment {
    let mut rows = tb.rows.clone();
    rows[m.start].cells[m.kr].below = tb.rows[m.start + 1..ri].iter().map(|r| r.height).sum::<f64>() + h;
    let t = TableLayout { rows, cols: tb.cols.clone(), outer: tb.outer.clone(), fit: tb.fit.clone(), ..*tb };
    let mut parts = whole(&t.rows[m.start]);
    parts[m.kr] = CellPart { units: &tb.rows[m.start].cells[m.kr].units.units[..m.cut], base: 0.0, ends: false };
    draw_row(&t, m.start, left, top, tb.rows[m.start].height, Some(&parts), false)
}

/// Units moved up so the first starts at 0.
pub fn rebase(units: &[StoryUnit]) -> Vec<StoryUnit> {
    let first = units.first().map_or(0.0, |u| u.top);
    units.iter().map(|x| StoryUnit { top: x.top - first, ..x.clone() }).collect()
}

/// COL-3: a note stream filled column by column at content height `h`, notes split with NOTE-7's widow control. None if
/// it needs more than `ncols` columns; else the parts per column and the tallest column.
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

/// COL-3: the smallest content height at which the stream fits its columns, if it fits within `room`.
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

/// Height of a row drawn with `parts` per cell (a part of a row split across pages, TABLE-41).
fn part_height(row: &super::table::RowBox, parts: &[CellPart]) -> f64 {
    // A part that ends a paragraph (its cell's last, or one a split keeps whole) includes its space-after (TABLE-41).
    let end = |p: &CellPart| p.units.last().map_or(0.0, |u| u.after);
    let content = row.cells.iter().zip(parts).map(|(c, p)| c.margins[0] + p.height() + end(p) + c.margins[2]).fold(0.0, f64::max);
    row.top_border + content
}

/// A story's units placed at (`x`, `y`) as children.
pub fn place_units(units: &[StoryUnit], x: f64, y: f64, out: &mut Vec<Child>) {
    for u in units {
        out.push(Child::Group { offset: (x, y + u.top), children: u.content.clone() });
    }
}

/// FLOAT-14: compat ≤ 14 honours layoutInCell="0" (unless the float is character- or line-relative): a float anchored
/// in one of the table's cells is placed like a body float, not in the cell.
/// shortcut: the table's own cells only; floats in nested tables stay in their cells.
fn page_floats(t: &Table) -> HashMap<u64, &Float> {
    let mut out = HashMap::new();
    for b in t.rows.iter().flat_map(|r| r.cells.iter()).flat_map(|c| c.blocks.iter()) {
        let Block::Para(p) = b else { continue };
        for i in &p.inlines {
            if let Inline::Float(f, _) = i
                && !f.in_cell(Compat::Legacy)
            {
                out.insert(f.id, &**f);
            }
        }
    }
    out
}

/// Moves a placed row's [`page_floats`] from the cell to their page position. Paragraph offsets stay as in the cell,
/// less the row's top border and `up`, how far the table moved below its floats (FLOAT-14).
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
                        BoxKind::Float { anchor, float, rect } if floats.contains_key(&float.0) => {
                            let f = floats[&float.0];
                            let x = float::h_pos(f, fr) - offset.0;
                            let y = match f.v_relative.as_str() {
                                "paragraph" | "line" => rect[1] - top_border - up,
                                _ => float::v_pos(f, rect[3] - rect[1], fr, 0.0, 0.0) - offset.1,
                            };
                            let d = (x - rect[0], y - rect[1]);
                            Child::Box(BoxFragment {
                                kind: BoxKind::Float { anchor, float, rect: [rect[0] + d.0, rect[1] + d.1, rect[2] + d.0, rect[3] + d.1] },
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
    /// The paragraph laid out from block `bi`, and the last block it takes: `bi`, or the last of the paragraphs joined
    /// to it (REV-6). `None` for a table.
    fn laid(&mut self, bi: usize) -> Option<(LaidPara<'d>, usize)> {
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

    /// Paragraph `laid` (from blocks `bi..=end`) laid out at `width` from piece `from` (0 = the whole paragraph), drawn
    /// at `x0`.
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
        let (prev, next) = self.sec.blocks.neighbours(bi, end, self.ctx.env.text.review.view);
        let space = laid.shape_space(self.fields, self.note_numbers, &self.ctx.env.text);
        let shaped = shape(p, &space, self.ctx.fonts, memos)?;
        let ps = para_space(p.para_style_in(self.ctx.env.text.review.view), prev, next, width, from, x0, &self.ctx);
        let lines = break_lines(&shaped, p, &ps, self.ctx.fonts, memos)?;
        hold(&mut self.pass.held.lines, &lines);
        Ok(lines)
    }

    /// A table laid out for `width`.
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

    /// A footnote's units at `width`.
    fn note_units(&mut self, kind: NoteKind, id: i64, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let blocks: Vec<&Block> = self.notes.blocks(kind, id).map(|b| b.iter().collect()).unwrap_or_default();
        let env = StoryEnv { ctx: self.ctx, fields: self.fields, notes: self.note_numbers };
        super::story::layout(&blocks, width, &env, None, memos)
    }

    /// The separator story (NOTE-3) at `width`.
    fn separator(&mut self, cont: bool, width: f64, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
        let name = if cont { "footnote:continuationSeparator" } else { "footnote:separator" };
        let s = match self.notes.separators.get(name) {
            Some(b) => {
                let blocks: Vec<&Block> = b.iter().collect();
                let fields = FieldValues::default();
                let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
                super::story::layout(&blocks, width, &env, None, memos)?
            }
            None => Rc::new({
                // The default separator (NOTE-3): one line of the default paragraph font with a 144pt rule.
                let h = self.ctx.fonts.face("Calibri", false, false).map_or(13.43, |f| {
                    let (a, d, e) = super::text::metrics(&f, 11.0);
                    a + d + e
                });
                let rule = Item::Rect { rect: [0.0, h * 0.78 - 2.7, 144.0, h * 0.78 - 2.1], colour: 0 };
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
                    }],
                    height: h,
                    float_bottom: 0.0,
                }
            }),
        };
        Ok(s)
    }

    /// NOTE-5 to NOTE-7: adds the notes referenced on a line to `area` if the line plus the separator and each note's
    /// minimum first part fit above the column bottom with the line ending at `line_bottom`. Returns false (area
    /// unchanged) if not.
    fn add_notes(
        &mut self,
        refs: &[(NoteKind, i64)],
        area: &mut NoteArea,
        col: &ColumnSpace,
        line_bottom: f64,
        rest: &mut Vec<NotePart>,
        memos: &mut Memos,
    ) -> Result<bool, Yield> {
        // Only footnotes take room on the page: an endnote reference reserves no area (nor its separator).
        if !refs.iter().any(|r| r.0 == NoteKind::Foot) {
            return Ok(line_bottom + area.total() <= col.bottom + EPS);
        }
        let mut a = area.clone();
        if a.sep == 0.0 {
            a.sep = self.separator(false, col.width, memos)?.height;
        }
        if a.shared.is_some() {
            // COL-3: the line fits if the page's stream with its notes balances within the room left below it.
            // shortcut: no NOTE-6 continuation in a shared area; a stream that can't fit moves the referencing line.
            for &(k, id) in refs.iter().filter(|r| r.0 == NoteKind::Foot) {
                let u = self.note_units(k, id, col.width, memos)?;
                a.parts.push(NotePart { kind: k, id, units: u.units.clone() });
            }
            let Some(h) = stream_height(&a.parts, a.cols, col.bottom - line_bottom - a.sep) else { return Ok(false) };
            a.height = h;
            if line_bottom + a.total() > col.bottom + EPS {
                return Ok(false);
            }
            *area = a;
            return Ok(true);
        }
        let mut new_rest = Vec::new();
        for &(k, id) in refs {
            if k != NoteKind::Foot {
                continue;
            }
            let u = self.note_units(k, id, col.width, memos)?;
            let (units, total) = (&u.units, u.height);
            let room = col.bottom - line_bottom - a.total();
            // The note's minimum first part (NOTE-5): its first paragraph's first line, or two with widow control.
            let first_para = units.iter().take_while(|x| x.block == units[0].block).count();
            let min = if units.first().is_some_and(|x| x.widow) { 2.min(first_para) } else { 1.min(first_para) };
            let min_h = units.get(min.saturating_sub(1)).map_or(0.0, |x| x.top + x.height);
            if min_h > room + EPS || !new_rest.is_empty() {
                return Ok(false);
            }
            if total <= room + EPS {
                a.parts.push(NotePart { kind: k, id, units: units.clone() });
                a.height += total;
                continue;
            }
            // Split the note: as many lines as fit; widow control inside the paragraph that straddles the cut (NOTE-7).
            let fit = note_cut(units, room);
            if fit < min {
                return Ok(false);
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

    /// Places continuing notes at the top of a column's note area (NOTE-6).
    fn continue_notes(&mut self, carry: &mut FlowState, col: &ColumnSpace, area: &mut NoteArea, memos: &mut Memos) -> Result<(), Yield> {
        if carry.note_rest.is_empty() {
            return Ok(());
        }
        area.cont = true;
        area.sep = self.separator(true, col.width, memos)?.height;
        let mut rest = Vec::new();
        for part in std::mem::take(&mut carry.note_rest) {
            let room = col.bottom - col.top - area.total();
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
        Ok(())
    }

    /// A note area's separator and parts stacked from `y`, each in its own box (NOTE-1).
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

    /// Draws a column's note area, bottom-up ending at the column bottom (NOTE-1).
    fn draw_notes(&mut self, area: &NoteArea, col: &ColumnSpace, out: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
        if area.parts.is_empty() {
            return Ok(());
        }
        self.at.notes_drawn = true;
        // NOTE-2: drawn 0.15pt low; pagination is unaffected.
        let y = col.bottom - area.total() + 0.15;
        let parts = area.parts.clone();
        out.push(self.notes_box(area, &[(col.x, &parts)], col.width, y, memos)?);
        Ok(())
    }

    /// Fills one column from `token` and `state`. The effects accumulate in the attempt's state, which
    /// the page body owns across its columns.
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
            self.draw_notes(&area, &col, &mut out, memos)?;
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
        let mut y = col.top;
        let mut empty = true;
        // The start of a keep-with-next chain (PAGE-6): where to rewind when the chain can't share the column.
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
        /// PAGE-6, compat 15: going back from the chain's last paragraph, the first that can end on this page without
        /// its last line (two with widowControl): where it started and how many lines it keeps. A keepLines paragraph
        /// moves whole and the step goes on; none when the step reaches the frame's first paragraph.
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
        // The chain's paragraphs, for compat 15's step back (PAGE-6): where each started, its lines here, keepLines and
        // widowControl.
        let mut members: Vec<(Breakpoint, usize, bool, bool)> = Vec::new();
        let generic = self.ctx.env.text.compat == Compat::Generic;
        loop {
            if pos.block >= nblocks {
                let end = y + if col.balance { carry.prev_after } else { 0.0 };
                return Ok((Stop::Done, end));
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
                        // PAGE-6: always a new page; PARA-2: its space-before is dropped in compat 15, kept in 14.
                        carry.keep_top = self.ctx.env.text.compat == Compat::Legacy;
                        return Ok((Stop::Page, y));
                    }
                    let pb = self.para_lines(&laid, bi, end, col.width, pos.at.index(), col.x, memos)?;
                    let placed = self.place_para(p, &pb, pos, carry, &col, &mut y, &mut empty, area, out, memos)?;
                    // A paragraph joined to the ones after it takes their blocks too (REV-6).
                    if pos.block > bi {
                        pos.block = end + 1;
                    }
                    match placed {
                        Stop::Done => {}
                        Stop::Full if pos.block == bi && pos.at.at_start() => {
                            // PAGE-9: a chain member that places nothing here moves the whole chain on, as when
                            // `next_fits` sees it can't follow.
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
                    // PAGE-6: keep with next.
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
                    // PAGE-6: a page break before the first paragraph of a cell in the table's first row starts the
                    // table on a new page; one in a later row breaks the table there (TABLE-45).
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
                            // A keepNext paragraph before a table keeps with its first row (PAGE-6, TABLE-48).
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

    /// Whether the next block's first part fits below `y` (for keep-with-next).
    fn next_fits(&mut self, bi: usize, carry: &FlowState, col: &ColumnSpace, y: f64, notes: f64, memos: &mut Memos) -> Result<bool, Yield> {
        // The page's footnote area takes its share of the column (PAGE-9).
        let bottom = col.bottom - notes;
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

    /// Records a paragraph placed on the page (STYLEREF) and, for its first part, its bookmarks (PAGEREF).
    fn record(&mut self, p: &Paragraph, first_part: bool) {
        // shortcut: paragraphs joined by the view (REV-6) are recorded as their last one, so STYLEREF shows that
        // paragraph's text alone; record the joined text to fix.
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

    /// Line numbers (LNUM-1, LNUM-2) for a placed line.
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

    /// Places a paragraph's lines from `pos` (PARA-1 to PARA-5, PAGE-1 to PAGE-6); `pb` is laid out from that piece.
    /// Done = all lines placed.
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
        // A paragraph that ends above a float's top doesn't wrap it (and keeps its borders and shading: the wrapped
        // path has none): its gap and every line unwrapped.
        let bottom = *y + self.para_gap(p, pos, carry, *empty) + pb.lines.iter().map(|l| l.pitch).sum::<f64>();
        let affected = self.at.known.iter().any(|f| {
            f.wrap != Wrap::None && f.excl[3] > *y + EPS && f.excl[1] < bottom - EPS && f.excl[0] < col.x + col.width - EPS && f.excl[2] > col.x + EPS
        });
        if has_anchor || affected {
            return self.place_wrapped(p, pb, pos, carry, col, y, empty, area, out, memos);
        }
        self.place_plain(p, pb, pos, carry, col, y, empty, area, out, memos)
    }

    /// `place_para` for a paragraph no float affects, and the fallback when no line fits beside the floats.
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
            before = 0.0; // PARA-5
        }
        let after = space_after(&self.ctx.env.text, style);
        let n = pb.lines.len();
        let at_top = *empty;
        let g = if !first_part {
            std::mem::take(&mut carry.break_before)
        } else if at_top {
            // PAGE-7, PARA-2 (compat 11-15): the gap to the previous paragraph (PARA-1) less its space-after, which stayed on
            // the page before: all the space-before with doNotUseHTMLParagraphAutoSpacing, else what the space-after
            // leaves of it.
            if carry.keep_top || carry.first_para {
                (super::paragraph::gap_plain(&self.ctx.env.text, carry.prev_after, before) - carry.prev_after).max(0.0)
            } else {
                0.0
            }
        } else {
            match &carry.prev {
                Some(ps) => gap(&self.ctx.env.text, ps, carry.prev_after, style, before),
                None => self.edge_before(pos.block, style, before),
            }
        };
        let top = *y + g;
        // How many lines fit (PAGE-1, PAGE-2), with the notes they reference (NOTE-5): the note area after each line.
        let mut yy = top;
        let mut fit = 0;
        let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
        for (i, l) in pb.lines.iter().enumerate() {
            let room = if col.balance { l.pitch } else { l.pitch - l.below };
            let refs = pb.notes(i);
            let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
            if !self.add_notes(&refs, &mut a, col, yy + room, &mut rest, memos)? {
                break;
            }
            // PAGE-8: above a footnote area nothing is suppressed.
            let tail = if i + 1 == n { after } else { 0.0 };
            if a.total() > 0.0 && yy + l.pitch + tail + a.total() > col.bottom + EPS {
                break;
            }
            areas.push((a, rest));
            yy += l.pitch;
            fit += 1;
            if matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)) {
                break;
            }
            // NOTE-6: once a note continues, body text after it moves to the next page.
            if areas.last().is_some_and(|x| !x.1.is_empty()) {
                break;
            }
        }
        let hard = pb.lines[..fit.min(n)].iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak)));
        let mut take = lines_to_take(n, fit, hard, first_part, style.keep_lines, style.widow_control);
        // PAGE-6, compat 15: a keep-with-next paragraph stepped back to ends here without its last lines.
        if let Some((b, cap)) = self.at.keep_cap
            && b == pos.block
            && first_part
        {
            self.at.keep_cap = None;
            take = take.min(cap);
        }
        // Balancing: the paragraph's space-after must fit too.
        if col.balance && take == n && yy + after > col.bottom + EPS && !at_top {
            take = 0;
        }
        if take == 0 {
            if at_top {
                // A keep or widow rule that can't be met in an empty column is dropped; PAGE-5: always progress.
                self.at.forced = self.at.forced || col.balance;
                take = fit.max(1).min(n);
            } else {
                return Ok(Stop::Full);
            }
        }
        self.at.starts.push((top, first_part));
        self.record(p, first_part);
        let (a, rest) = areas[take.min(areas.len() - 1)].clone();
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
        // PARA-2, COL-2: whether the break that ended the first line is the paragraph's first character (a break after
        // text drops the space-before).
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
                // PARA-2: a paragraph that starts with a page break keeps on the next page what the previous
                // paragraph's space-after left of its space-before (as PAGE-7).
                let kept = (super::paragraph::gap_plain(&self.ctx.env.text, prev_after, before) - prev_after).max(0.0);
                carry.break_before = if starts_with_break { kept } else { 0.0 };
                Stop::Page
            }
            Some(Kind::ColumnBreak) => {
                carry.keep_top = false;
                carry.prev = None;
                // COL-2: a paragraph that starts with a column break keeps its space-before in the next column.
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

    /// The space above a paragraph (PARA-1 to PARA-5): before its first line, or 0 for a continuation.
    fn para_gap(&self, p: &Paragraph, pos: &FlowToken, carry: &FlowState, empty: bool) -> f64 {
        let style = p.para_style_in(self.ctx.env.text.review.view);
        let mut before = space_before(&self.ctx.env.text, style);
        if style.before_auto && carry.first_para {
            before = 0.0; // PARA-5
        }
        if !pos.at.at_start() {
            0.0
        } else if empty {
            // PAGE-7, PARA-2 (compat 11-15): the gap to the previous paragraph (PARA-1) less its space-after, which stayed on
            // the page before: all the space-before with doNotUseHTMLParagraphAutoSpacing, else what the space-after
            // leaves of it.
            if carry.keep_top || carry.first_para {
                (super::paragraph::gap_plain(&self.ctx.env.text, carry.prev_after, before) - carry.prev_after).max(0.0)
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

    /// PARA-4: block `bi`'s space-before with no paragraph before it in the column. After a table its previous
    /// paragraph is the table's last row end mark, in the default paragraph style; nothing collapses there.
    fn edge_before(&self, bi: usize, style: &ParaStyle, before: f64) -> f64 {
        let after_table = bi > 0 && matches!(self.sec.blocks.get(bi - 1), Some(Block::Table(_)));
        if after_table && super::paragraph::drops_at_edge(style, Some(&self.ctx.env.text.default_style)) { 0.0 } else { before }
    }

    /// TABLE-48, PARA-4: the previous paragraph's space-after above table block `bi`, dropped when that paragraph is
    /// contextual and has the style of the table's first paragraph (the next one in document order).
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

    /// FRAME-1: a frame without `w` is as wide as its content ([`super::story::fit_frame`]).
    pub fn fit_frame(&self, f: &Rc<Float>, width: f64, memos: &mut Memos) -> Result<Rc<Float>, Yield> {
        let fields = FieldValues::default();
        let env = StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers };
        Ok(super::story::fit_frame(f, width, &env, memos)?.map_or_else(|| f.clone(), Rc::new))
    }

    /// A float's height ([`super::story::float_height`]).
    pub fn float_height(&self, f: &Float, memos: &mut Memos) -> Result<f64, Yield> {
        let fields = FieldValues::default();
        super::story::float_height(f, &StoryEnv { ctx: self.ctx, fields: &fields, notes: self.note_numbers }, memos)
    }

    /// A paragraph wrapped around floats (FLOAT-2 to FLOAT-9): broken band by band at its position on the page, then
    /// paginated like ordinary lines (PAGE-1 to PAGE-6). Its own anchors are positioned and recorded for the page's
    /// next fill.
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
        let top = *y + self.para_gap(p, pos, carry, *empty);
        let floats = self.at.known.clone();
        let space = ParaSpace { width: col.width, start: pb.from, borders: (None, None), x0: col.x, env: self.ctx.env.text.clone() };
        let frame = Frame::new(&space);
        let sp: &ShapedPara = &pb.shaped;
        let pieces = pb.pieces();
        let n = pieces.len();
        let est = pb.lines.first().map_or(13.0, |l| l.pitch);
        let (cx0, cx1) = (col.x, col.x + col.width);
        let compat = self.ctx.env.text.compat;
        // FLOAT-4 (compat <= 14): a paragraph holding only one inline picture is placed in the gap even when it's
        // wider, over the float.
        let mut content = pieces.iter().filter(|q| q.kind != Kind::Anchor);
        let lone_picture = compat == Compat::Legacy && content.next().is_some_and(|q| q.kind == Kind::Object) && content.next().is_none();
        // Bands: (top, segments).
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
            // FLOAT-4: a band at the column bottom ignores the floats.
            let here: &[Placed] = if yy >= col.bottom - EPS { &[] } else { &floats };
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
                            Ok((segs, e)) => {
                                let ph = segs[0].pitch;
                                let done = (ph - h).abs() < EPS || attempt == 1;
                                got = Some((segs, e));
                                if done {
                                    break;
                                }
                                h = ph; // re-test the band with the line's real height
                            }
                            Err(t) => {
                                blocked = true;
                                // FLOAT-4: the step includes the paragraph's space-before on its first line.
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
                    bands.push((yy, segs));
                    yy += ph;
                    s = e;
                    if hard {
                        break;
                    }
                }
                _ => {
                    // TABLE-49: a paragraph's first line that meets a floating table goes below what it meets plus
                    // the paragraph's own space before (kept above the line wherever it lands).
                    let meets = |f: &&Placed| f.in_band(yy, yy + h) && f.excl[2] > cx0 + EPS && f.excl[0] < cx1 - EPS;
                    if first_part && bands.is_empty() && here.iter().filter(meets).any(|f| f.key.2 == float::TABLE_PIECE) {
                        let below = here.iter().filter(meets).map(|f| f.excl[3]).fold(yy, f64::max);
                        let before = if style.before_auto && carry.first_para { 0.0 } else { space_before(&self.ctx.env.text, style) };
                        yy = (below + before).min(col.bottom.max(yy + EPS));
                        continue;
                    }
                    // FLOAT-4 (compat <= 14): beside square floats the line retries lower in steps of its trial height.
                    let square = here.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square");
                    if let Some(t) = trial.filter(|&t| compat == Compat::Legacy && square && t > EPS) {
                        yy = (yy + t).min(col.bottom.max(yy + EPS));
                        continue;
                    }
                    // FLOAT-4, FLOAT-8: move down to the next obstacle edge and try again.
                    match float::next_edge(here, cx0, cx1, yy, yy + h) {
                        Some(b) => yy = b.min(col.bottom.max(yy + EPS)),
                        None => yy += h,
                    }
                }
            }
        }
        // shortcut: no line fits beside the floats (a column with no usable width, e.g. a page narrower than its
        // margins): lay the paragraph out as if no float were there so the flow progresses; its floats aren't placed.
        if bands.is_empty() {
            return self.place_plain(p, pb, pos, carry, col, y, empty, area, out, memos);
        }
        // Anchors: position this paragraph's floats against its bands (FLOAT-6, FLOAT-9).
        let frames = Frames {
            page_w: self.page_w,
            page_h: self.page_h,
            margin: [self.margins.0, self.margins.1, self.margins.2, self.sec.section.section_in(self.ctx.env.text.review.view).bottom],
            column: (col.x, col.width),
        };
        let bi = pos.block;
        let si = self.sec.index;
        let anchor = para_id(p);
        let mut anchors: Vec<Placed> = Vec::new();
        // Bands after which the page ends: a FLOAT-9 failure at the top of an empty column, an unresolved FLOAT-12
        // overlap.
        let mut forced: Option<usize> = None;
        let band_of = |k: usize| bands.iter().position(|(_, segs)| segs.iter().any(|l| k >= l.start && k < l.end.max(l.start + 1)));
        for (k, q) in pieces.iter().enumerate() {
            let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, sp.object(pb.from + k)) else { continue };
            let Some(b) = band_of(k) else { continue };
            // REV-6: a float is anchored in its own paragraph, joined or not.
            let owner = piece_para(sp, pb.from + k, anchor);
            let f = &self.fit_frame(f, col.width, memos)?;
            let h = self.float_height(f, memos)?;
            let line_top = bands[b].0;
            let para_top = if first_part { top } else { bands[0].0 };
            let x = float::h_pos(f, &frames);
            let mut fy = float::v_pos(f, h, &frames, para_top, line_top);
            // FLOAT-13: a wrapNone float stays where it is, and so does its paragraph. Small square or tight floats
            // beside a one-line anchor may stay too (moved up only past the page edge); not separated from FLOAT-9 yet.
            if Placed::new((si, bi, pb.from + k), owner, f, x, fy, h).wrap == Wrap::None {
                anchors.push(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h));
                continue;
            }
            // FLOAT-6 (compat 15): a wrapped paragraph- or line-relative float never rises above the top margin.
            if compat == Compat::Generic && matches!(f.v_relative.as_str(), "paragraph" | "line") {
                fy = fy.max(frames.margin[2]);
            }
            // FLOAT-9.
            if matches!(f.v_relative.as_str(), "paragraph" | "line") && fy + h > col.bottom + EPS {
                let up = col.bottom - h;
                let anchor_bottom = line_top + bands[b].1[0].pitch;
                if up >= anchor_bottom - EPS {
                    fy = up;
                } else if !at_top {
                    return Ok(Stop::Full);
                } else {
                    // FLOAT-9: nowhere to move.
                    let mut a = Placed::new((si, bi, pb.from + k), owner, f, x, fy, h);
                    a.wrap = Wrap::None;
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                    anchors.push(a);
                    continue;
                }
            }
            anchors.push(Placed::new((si, bi, pb.from + k), owner, f, x, fy, h));
        }
        // Pagination over bands (PAGE-1 to PAGE-6), with the notes each band references (NOTE-9), as in `place_para`.
        let nb = bands.len();
        let mut fit = 0;
        let mut areas = vec![(area.clone(), Vec::<NotePart>::new())];
        for (bi_, (bt, segs)) in bands.iter().enumerate() {
            let l = &segs[0];
            let room = if col.balance { l.pitch } else { l.pitch - l.below };
            let refs: Vec<(NoteKind, i64)> = segs.iter().flat_map(|l| (l.start..l.end).filter_map(|k| sp.note(pb.from + k))).collect();
            let (mut a, mut rest) = areas.last().cloned().unwrap_or_default();
            if !self.add_notes(&refs, &mut a, col, bt + room, &mut rest, memos)? {
                break;
            }
            // Above a footnote area nothing is suppressed (PAGE-8; see `place_para`).
            let tail = if bi_ + 1 == nb { after } else { 0.0 };
            if a.total() > 0.0 && bt + l.pitch + tail + a.total() > col.bottom + EPS {
                break;
            }
            areas.push((a, rest));
            fit += 1;
            if segs.iter().any(|l| matches!(l.hard, Some(Kind::PageBreak | Kind::ColumnBreak))) {
                break;
            }
            // NOTE-6: once a note continues, body text after it moves to the next page.
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
        // Record the anchors placed on this page.
        let placed_end = bands[take - 1].1.last().map_or(0, |l| l.end);
        for mut a in anchors {
            if a.key.2 - pb.from < placed_end.max(1) {
                // FLOAT-12: avoid earlier floats on the page that may not be overlapped.
                if !float::avoid(&mut a, &self.at.found, frames.page_w, col.bottom) {
                    // Unresolved: the page ends after the anchor's line (FLOAT-12).
                    let b = band_of(a.key.2 - pb.from).unwrap_or(0);
                    forced = Some(forced.map_or(b, |x: usize| x.min(b)));
                }
                self.at.found.push(a);
            }
        }
        if let Some(b) = forced {
            take = take.min(b + 1);
        }
        // Footnotes referenced by the bands placed here (NOTE-9).
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
        carry.prev_after = after;
        let done = take >= nb;
        if done {
            pos.next_block();
        } else {
            pos.at = At::Piece(pb.from + bands[take].1[0].start);
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

    /// Places table rows from `pos` (TABLE-40 to TABLE-48). Done = the table is finished.
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
        let finish = |rows: Vec<_>, out: &mut Vec<Child>| {
            if !rows.is_empty() {
                out.push(Child::Table(TableFragment { table: tb.id, rows: rows.into_boxed_slice() }));
            }
        };
        // TABLE-48: the preceding space-after applies to the table's outer border edge.
        if pos.at.at_start() && !*empty {
            *y += self.after_above_table(pos.block, carry);
        }
        // TABLE-49: a floating table takes no room in the flow. Its rows go from its anchored top, its part on each
        // page is an obstacle the page's text wraps around (`table_obstacle`), and the text after it goes on from
        // where it stood (after a split, from the top of the page holding its last part). A page's later fills keep
        // the top its first fill found: text before the table that re-wraps around it doesn't move it.
        let fp = match self.sec.blocks.get(pos.block) {
            Some(Block::Table(t)) => t.floating,
            _ => None,
        };
        let (flow_y, was_empty) = (*y, *empty);
        let legacy = self.ctx.env.text.compat == Compat::Legacy;
        // doNotBreakWrappedTables (compat ≤ 14): the table isn't split; one that doesn't fit below its anchor moves
        // to the next page with it.
        let unsplit = fp.is_some() && legacy && self.ctx.env.text.do_not_break_wrapped_tables;
        // horzAnchor page: the table's place in the column is moved to the same place in the page.
        let mut at_x = match fp {
            Some(fp) if fp.h_page => match fp.x_spec {
                Some(Jc::Center) => (self.page_w - col.width) / 2.0,
                Some(Jc::Right) => self.page_w - col.width,
                _ => 0.0,
            },
            _ => col.x,
        };
        if let Some(fp) = fp
            && pos.at.at_start()
        {
            let h: f64 = tb.rows.iter().filter(|r| !r.hidden).map(|r| r.height).sum::<f64>() + tb.bottom_border;
            let (t0, t1) = match fp.v_anchor {
                TableAnchor::Page => (0.0, self.page_h),
                _ => (self.margins.2, self.page_h - self.sec.section.section_in(self.ctx.env.text.review.view).bottom),
            };
            // Page and margin frames: the stored tblpY is one twip more than the offset; a frame above the page top is
            // placed at the top.
            let mut top = match (fp.v_anchor, fp.y_spec) {
                (TableAnchor::Text, _) => flow_y + fp.y,
                (_, Some(Jc::Center)) => (t0 + t1 - h) / 2.0,
                (_, Some(Jc::Right)) => t1 - h,
                (_, Some(_)) => t0,
                (_, None) => t0 + fp.y - 0.05,
            }
            .max(0.0);
            // w:tblOverlap: compat 15, a "never" table moves right past an earlier floating table's obstacle; compat
            // ≤ 14, any floating table moves below one it overlaps by 9pt or more (distances included).
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
            if unsplit && !*empty && *y + h > col.bottom + EPS {
                return Ok(Stop::Full);
            }
        }
        let tall;
        let col = if unsplit {
            tall = ColumnSpace { bottom: f64::INFINITY, ..*col };
            &tall
        } else {
            col
        };
        let part_top = *y;
        carry.prev = None;
        carry.prev_after = 0.0;
        let headers: Vec<usize> = (0..tb.rows.len()).take_while(|&r| tb.rows[r].header).collect();
        let n = tb.rows.len();
        // TABLE-46: header rows repeat at the top of a continuation column.
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
            Some(Block::Table(t)) if self.ctx.env.text.compat == Compat::Legacy => page_floats(t),
            _ => HashMap::new(),
        };
        let frames = Frames {
            page_w: self.page_w,
            page_h: self.page_h,
            margin: [self.margins.0, self.margins.1, self.margins.2, self.sec.section.section_in(self.ctx.env.text.review.view).bottom],
            column: (col.x, col.width),
        };
        // FLOAT-14: compat ≤ 14, a square or top-and-bottom body-level float of the first row, placed from the
        // column and the paragraph (the row's top), that overlaps that row moves the whole table below it, its bottom
        // distance included; the floats stay where the row's first place put them.
        // shortcut: a float overlapping only a later row doesn't split the table there; tight and through floats
        // don't move it, as their wrap polygon (often empty) isn't read.
        let lift = match (tb.rows.first(), self.sec.blocks.get(pos.block)) {
            (Some(r0), Some(Block::Table(t))) if fp.is_none() && pos.at.at_start() && !page_floats.is_empty() => {
                let (l, r) = (at_x + tb.x0, at_x + tb.x0 + tb.cols.iter().sum::<f64>());
                t.rows
                    .first()
                    .into_iter()
                    .flat_map(|row| row.cells.iter())
                    .flat_map(|c| c.blocks.iter())
                    .filter_map(|b| if let Block::Para(p) = b { Some(p) } else { None })
                    .flat_map(|p| p.inlines.iter())
                    .filter_map(|i| if let Inline::Float(f, _) = i { page_floats.get(&f.id).copied() } else { None })
                    .filter(|f| matches!(f.wrap.as_str(), "square" | "topAndBottom") && f.h_relative == "column" && f.v_relative == "paragraph" && f.h_align.is_none() && f.v_align.is_none())
                    .filter(|f| {
                        let x = float::h_pos(f, &frames);
                        x < r && x + f.w > l && f.v_offset - f.dist[0] < r0.height
                    })
                    .map(|f| f.v_offset + f.h + f.dist[1])
                    .fold(0.0, f64::max)
            }
            _ => 0.0,
        };
        *y += lift;
        let up = |r: usize| if r == 0 { lift } else { 0.0 };
        let mut placed_here = 0;
        // Rows placed whole on this page: (row, top, index in `rows`), for a merged region that splits (TABLE-32).
        let mut placed: Vec<(usize, f64, usize)> = Vec::new();
        self.at.starts.push((*y, pos.at.at_start()));
        let below_content = pos.at.at_start() && !*empty;
        // TABLE-45: the start of the run of keep-with-next rows the last whole row ends, when something is above the
        // run here: (row, index in `rows`, y, note area, carried note count).
        let mut keep: Option<(usize, usize, f64, NoteArea, usize)> = None;
        while pos.at.index() < n {
            let ri = pos.at.index();
            let row = &tb.rows[ri];
            // REV-1: a row the markup view hides draws nothing.
            if row.hidden {
                pos.at = At::Row(ri + 1);
                continue;
            }
            let last = tb.is_last(ri);
            let rest = pos.at.rest().cloned();
            let at_top = *empty || placed_here == 0 && rest.is_some();
            // TABLE-45: a keepNext row keeps with the next row; in compat 15 so does a header row (TABLE-47).
            let keeps = row.keep_next || self.ctx.env.text.compat == Compat::Generic && row.header && rest.is_none();
            // TABLE-45: a row whose first paragraph has pageBreakBefore, below content in the column, starts the next one
            // (with the keep-with-next rows before it); in compat 15 a keep-with-next row before it cancels that.
            let breaks = row.break_before
                && rest.is_none()
                && !*empty
                && !(self.ctx.env.text.compat == Compat::Generic && ri > 0 && tb.rows[ri - 1].keep_next);
            // The row's cell contents: borrowed (a row is copied only when it splits).
            let parts: Vec<CellPart> = match &rest {
                Some(r) => r.iter().map(|v| CellPart { units: &v[..], base: 0.0, ends: true }).collect(),
                None => whole(row),
            };
            let height = if rest.is_some() { part_height(row, &parts) } else { row.height };
            // NOTE-8: a row's references place their notes like a body line's.
            let refs: Vec<(NoteKind, i64)> = parts.iter().flat_map(|cu| cu.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            let mut a = area.clone();
            let mut note_rest = Vec::new();
            let notes_ok = self.add_notes(&refs, &mut a, col, *y + height + tb.bottom_border, &mut note_rest, memos)?;
            // TABLE-40: the row plus the table's bottom border must fit.
            if !breaks && notes_ok && *y + height + tb.bottom_border + a.total() <= col.bottom + EPS + a.total().min(0.0) {
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
                    // A whole row: through the cache.
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
            // Split the row (TABLE-41) unless it can't be (TABLE-45); a row taller than a page splits anyway.
            let avail = col.bottom - area.total() - *y - row.top_border - tb.bottom_border;
            // An atLeast row starts splitting only where its minimum height fits.
            // TABLE-45: a cantSplit, header or keep-with-next row doesn't split, except in compat 15 at the top of a column
            // (in compat <= 14 it is placed whole there and runs past the column's bottom, PAGE-5).
            let no_split = row.cant_split || row.header || keeps;
            let can_split = !breaks && !row.exact && (!no_split || at_top && self.ctx.env.text.compat == Compat::Generic) && (at_top || rest.is_some() || row.min_h <= avail + row.top_border + EPS);
            let mut fitted: Vec<CellPart> = Vec::new();
            let mut left: Vec<Vec<StoryUnit>> = Vec::new();
            if can_split {
                let compat = self.ctx.env.text.compat;
                // NOTE-8: the references the cells cut so far keep, and the bottom of their lowest kept line.
                let (mut kept, mut low) = (Vec::new(), 0.0f64);
                for (k, cu) in parts.iter().enumerate() {
                    let m = &row.cells[k].margins;
                    let room = avail - m[0] - m[2];
                    let first = cell_cut(cu.units, room, rest.is_some(), compat);
                    let mut cut = first;
                    // NOTE-8: a cell line stays only if its notes and those kept before it can start below it (NOTE-5);
                    // else the cut moves above it, and a cell left with nothing moves the row (TABLE-42).
                    while cut > 0 {
                        let u = &cu.units[cut - 1];
                        let bottom = (*y + row.top_border + m[0] + u.top + u.height + u.after + m[2] + tb.bottom_border).max(low);
                        let mut refs = kept.clone();
                        refs.extend(cu.units[..cut].iter().flat_map(|u| u.notes.iter().copied()));
                        if refs.is_empty() || self.add_notes(&refs, &mut area.clone(), col, bottom, &mut Vec::new(), memos)? {
                            (kept, low) = (refs, bottom);
                            break;
                        }
                        cut = cell_cut(cu.units, u.top, rest.is_some(), compat).min(cut - 1);
                    }
                    // PAGE-5: at the top of a column the cut stands without its notes.
                    if cut == 0 && at_top {
                        cut = first;
                    }
                    fitted.push(CellPart { units: &cu.units[..cut], base: 0.0, ends: cu.ends && cut == cu.units.len() });
                    let mut rest = rebase(&cu.units[cut..]);
                    // TABLE-41: a continuation that starts a new paragraph keeps what its space-before adds to the
                    // space-after left on this page.
                    if cut > 0 && cut < cu.units.len() && cu.units[cut].block != cu.units[cut - 1].block {
                        let p = &cu.units[cut - 1];
                        let keep = (cu.units[cut].top - (p.top + p.height + p.after)).max(0.0);
                        rest.iter_mut().for_each(|u| u.top += keep);
                    }
                    left.push(rest);
                }
            }
            let merged = if can_split && rest.is_none() {
                merged_splits(tb, ri, &placed, col.bottom - area.total() - tb.bottom_border, self.ctx.env.text.compat)
            } else {
                Vec::new()
            };
            for m in &merged {
                left[m.k] = rebase(&tb.rows[m.start].cells[m.kr].units.units[m.cut..]);
            }
            // TABLE-42.
            let any = fitted.iter().any(|f| !f.units.is_empty());
            let every = fitted.iter().zip(&parts).all(|(f, u)| u.units.is_empty() || !f.units.is_empty());
            let h = merged.iter().map(|m| m.bottom - *y).fold(part_height(row, &fitted), f64::max);
            // NOTE-8: the part's references place their notes below it, as a body line's; at the top of a column the
            // part stays even if they can't (PAGE-5).
            let refs: Vec<(NoteKind, i64)> = fitted.iter().flat_map(|f| f.units.iter().flat_map(|u| u.notes.iter().copied())).collect();
            let (mut a, mut note_rest) = (area.clone(), Vec::new());
            if can_split
                && any
                && every
                && (refs.is_empty() || self.add_notes(&refs, &mut a, col, *y + h + tb.bottom_border, &mut note_rest, memos)? || at_top)
            {
                *area = a;
                carry.note_rest.extend(note_rest);
                for m in &merged {
                    let (rs, top, idx) = placed[m.pi];
                    let mut redrawn = draw_merged_part(tb, m, ri, h, at_x, top);
                    move_floats(Some(&mut redrawn), &page_floats, &frames, tb.rows[rs].top_border, up(rs));
                    rows[idx] = redrawn;
                }
                rows.push(draw_row(tb, ri, at_x, *y, h, Some(&fitted), true));
                move_floats(rows.last_mut(), &page_floats, &frames, row.top_border, up(ri));
                *y += h;
                *empty = false;
                pos.at = At::RowPart { row: ri, rest: Rc::new(left) };
                finish(rows, out);
                self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y);
                return Ok(Stop::Full);
            }
            if at_top {
                // PAGE-5: nothing fits in an empty column; place the row anyway.
                rows.push(draw_row(tb, ri, at_x, *y, height, Some(&parts), last));
                move_floats(rows.last_mut(), &page_floats, &frames, row.top_border, up(ri));
                *y += height + if last { tb.bottom_border } else { 0.0 };
                *empty = false;
                pos.at = At::Row(ri + 1);
                placed_here += 1;
                continue;
            }
            // TABLE-45: the run of keep-with-next rows before a row that can't start here moves with it; TABLE-47: so
            // in compat 15 header rows never end a page by themselves.
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
            finish(rows, out);
            self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y);
            return Ok(Stop::Full);
        }
        finish(rows, out);
        if fp.is_some() {
            self.table_obstacle(pos.block, tb, at_x, part_top, *y, flow_y);
            (*y, *empty) = (flow_y, was_empty);
        }
        pos.next_block();
        carry.first_para = false;
        Ok(Stop::Done)
    }

    /// TABLE-49: a floating table's part on this page, `top` to `bottom`, as an obstacle the page's text wraps
    /// around (FLOAT-2, both sides): its outer box grown by the *FromText distances, left and right at least 0.5pt.
    /// It reaches no higher than where the table stood in the flow unless the table itself does (only then does
    /// the text before it re-wrap). Compat ≤ 14: a table in the flow (text-anchored, tblpY 0) has no distance above
    /// and below, and with a negative tblInd the obstacle stays at the frame, which starts at tblpX.
    fn table_obstacle(&mut self, bi: usize, tb: &TableLayout, at_x: f64, top: f64, bottom: f64, flow_y: f64) {
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
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: 0,
            src: None,
            locked: false,
            frame_src: None,
        });
        let mut o = Placed::new((self.sec.index, bi, float::TABLE_PIECE), ParaId(t.id), &f, at_x + bl, top, bottom - top);
        o.excl[1] = o.excl[1].max(flow_y.min(top));
        if legacy {
            o.excl[0] -= t.ind.min(0.0);
        }
        self.at.found.push(o);
    }
}
