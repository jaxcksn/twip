//! Stories laid out without pagination (cells, headers and footers, notes, text boxes): one unit per line or table
//! row. The paragraph algorithms are the body's (HDR-3, TABLE-29).

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::{Block, Float, Inline, NoteKind, ParaStyle, Paragraph, Table};
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::float::{self, Placed};
use super::fragment::{BoxFragment, BoxKind, Child, ImageKey, Item, PlacedLine};
use super::ids::{FloatId, ParaId};
use super::job::{Memos, Yield};
use super::join::LaidBlock;
use super::line::{Frame, Line, ParaLines, band, break_lines, draw_line, neighbour_borders};
use super::paragraph::{contextual_in, gap, gap_plain, space_after, space_before};
use super::space::ParaSpace;
use super::table::TableLayout;
use super::text::{FieldValues, Kind, Object, ShapedPara, para_id, shape};
use super::units::Lu;

const EPS: f64 = 1e-6;

/// One line or table row of a story. `content` is drawn relative to the unit's top-left (the story's left edge, the
/// unit's top), so a unit can be placed anywhere and rebased when a split moves it.
#[derive(Clone, Debug)]
pub struct StoryUnit {
    pub top: f64,
    pub height: f64,
    /// Space below the text that needn't fit at a page bottom (PAGE-2).
    pub below: f64,
    /// The paragraph's space-after, on its last line (0 on other lines and table rows): a table row's split keeps a
    /// cell line ending a paragraph only if this fits below it too (TABLE-44).
    pub after: f64,
    pub content: Rc<[Child]>,
    /// The block (paragraph or table) this unit belongs to, within its story, and whether that paragraph has
    /// widow/orphan control (NOTE-7: widow control applies inside each note paragraph).
    pub block: usize,
    pub widow: bool,
    /// The paragraph's keepNext and keepLines (TABLE-43: a cell split keeps them in compat 15).
    pub keep_next: bool,
    pub keep_lines: bool,
    /// Notes referenced on this line (NOTE-8: references in table cells place their notes like body lines).
    pub notes: Box<[(NoteKind, i64)]>,
    /// A nested table's row: what a row split redraws it from (TABLE-41).
    pub row: Option<Rc<NestedRow>>,
}

/// A nested table row as a story unit: its table, its row, the table's offset in the story (TABLE-25) and, for the
/// continuation of a row split on an earlier page, what each cell still holds (TABLE-41).
#[derive(Clone, Debug)]
pub struct NestedRow {
    pub table: Rc<TableLayout>,
    pub ri: usize,
    pub shift: f64,
    pub rest: Option<Rc<[Vec<StoryUnit>]>>,
}

#[derive(Clone, Debug, Default)]
pub struct StoryLayout {
    pub units: Vec<StoryUnit>,
    /// The height, including the first space-before and the last space-after (HDR-3, TABLE-29).
    pub height: f64,
    /// In a cell: the lowest bottom of the floats laid out in it (not compat ≤ 14's layoutInCell="0" ones), drawn or
    /// not, from the content top; else 0 (TABLE-30).
    pub float_bottom: f64,
}

impl PartialEq for StoryLayout {
    /// Layouts are equal when their units are in the same places with the same drawings (shared or equal lines).
    fn eq(&self, o: &Self) -> bool {
        self.height == o.height
            && self.float_bottom == o.float_bottom
            && self.units.len() == o.units.len()
            && self
                .units
                .iter()
                .zip(&o.units)
                .all(|(a, b)| a.top == b.top && a.height == b.height && a.below == b.below && a.block == b.block && a.notes == b.notes)
    }
}

/// What a story's content shows that depends on where it is laid out: page-dependent field values and note numbers.
#[derive(Clone, Copy)]
pub struct StoryEnv<'a> {
    pub ctx: LayoutCtx<'a>,
    pub fields: &'a FieldValues,
    pub notes: &'a HashMap<(NoteKind, i64), String>,
}

/// A child of a cached story or table, named by its own cached result: a paragraph by its `ShapedPara` (keyed by the
/// paragraph's content and identity, its field values and its note numbers), a nested table by its `TableLayout`.
/// Compared by identity. As LayoutNG reuses a box's result only while no descendant needs layout, a parent's entry
/// hits only while every child's entry does, so a parent never copies its children's content into its key: an edit
/// in one cell misses that cell, its table and their ancestors, and nothing else.
#[derive(Clone, Debug)]
pub enum ChildId {
    Para(Rc<ShapedPara>),
    Table(Rc<TableLayout>),
}

impl PartialEq for ChildId {
    fn eq(&self, o: &Self) -> bool {
        match (self, o) {
            (ChildId::Para(a), ChildId::Para(b)) => Rc::ptr_eq(a, b),
            (ChildId::Table(a), ChildId::Table(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// The children of `blocks` (as laid out, REV-6), shaped (and nested tables laid out: at `width`, or measured when
/// `None`) through the cache.
pub fn child_ids(blocks: &[LaidBlock], width: Option<f64>, env: &StoryEnv, cell: Option<CellEnds>, memos: &mut Memos) -> Result<Vec<ChildId>, Yield> {
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        out.push(match b {
            LaidBlock::Para(p) => {
                ChildId::Para(shape(p.para(), &p.shape_space_in(env.fields, env.notes, &env.ctx, cell.is_some()), env.ctx.fonts, memos)?)
            }
            LaidBlock::Table(t) => ChildId::Table(match width {
                Some(w) => super::table::layout(&as_nested(t, cell), w, env, None, false, memos)?,
                None => super::table::layout(&as_nested(t, cell), 1e9, env, None, true, memos)?,
            }),
        });
    }
    Ok(out)
}

/// A cell story's ends (PARA-4): whether its first paragraph drops its space-before and its last its space-after,
/// against their neighbours in document order outside the cell; and its table's layout, which its nested tables take.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellEnds {
    pub before: bool,
    pub after: bool,
    /// The cell's table is laid out fixed (TABLE-5).
    pub fixed: bool,
    /// `w:tcFitText`: each paragraph on one line (TABLE-51).
    pub fit: bool,
}

/// TABLE-5: a nested table is laid out with its outermost table's layout, fixed or autofit, whatever its own; a
/// negative indent counts as 0 (TABLE-25).
/// ponytail: copies the nested table when either differs; keep a converted copy if that shows in profiles.
pub fn as_nested(t: &Table, cell: Option<CellEnds>) -> std::borrow::Cow<'_, Table> {
    match cell {
        Some(c) if c.fixed != t.fixed || t.ind < 0.0 => std::borrow::Cow::Owned(Table { fixed: c.fixed, ind: t.ind.max(0.0), ..t.clone() }),
        _ => std::borrow::Cow::Borrowed(t),
    }
}

/// What a story (a cell, header, footer, note, separator or text box), or a cell's content widths (`width` 0:
/// width-free), depends on: its children, the width, whether it is a cell and its ends (PARA-4), the
/// `TextEnv` and the font revision. Field values and note numbers are in the children's keys, except those of its
/// floats' text boxes: a story with one keys the field values too (HDR-3: a footer cell's text box PAGE).
#[derive(Clone, Debug, PartialEq)]
pub struct StoryKey {
    children: Vec<ChildId>,
    width: u64,
    cell: Option<CellEnds>,
    text: TextEnv,
    fonts: u64,
    boxed_fields: Option<FieldValues>,
}

impl StoryKey {
    pub fn new(children: Vec<ChildId>, width: Option<f64>, cell: Option<CellEnds>, env: &StoryEnv, blocks: &[LaidBlock]) -> StoryKey {
        let boxed = blocks.iter().any(|b| match b {
            LaidBlock::Para(l) => l.para().inlines.iter().any(|i| matches!(i, Inline::Float(f, _) if f.text_box.is_some() || !f.parts.is_empty())),
            _ => false,
        });
        StoryKey {
            children,
            width: width.map_or(0, f64::to_bits),
            cell,
            text: env.ctx.env.text.clone(),
            fonts: env.ctx.fonts.revision(),
            boxed_fields: boxed.then(|| env.fields.clone()),
        }
    }
}

impl CacheKey for StoryKey {
    type Probe<'a> = StoryKey;

    fn bucket(p: &StoryKey) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = super::cache::KeyHasher::default();
        (p.width, p.cell).hash(&mut h);
        for c in &p.children {
            match c {
                ChildId::Para(s) => Rc::as_ptr(s).hash(&mut h),
                ChildId::Table(t) => Rc::as_ptr(t).hash(&mut h),
            }
        }
        h.finish()
    }

    fn matches(&self, p: &StoryKey) -> bool {
        self == p
    }

    fn from_probe(p: &StoryKey) -> StoryKey {
        p.clone()
    }
}

// shortcut: entry sizes are rough (the units, not the lines they share); refine with memory budgets.
fn story_bytes(s: &StoryLayout) -> usize {
    std::mem::size_of::<StoryLayout>() + s.units.len() * (std::mem::size_of::<StoryUnit>() + 32)
}

/// Lays out blocks top-down in a box of `width` without pagination: one unit per line or table row, in the box's
/// coordinates. The height includes the first space-before and the last space-after (HDR-3, TABLE-29). Cached by
/// [`StoryKey`].
pub fn layout(blocks: &[&Block], width: f64, env: &StoryEnv, cell: Option<CellEnds>, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
    let blocks = super::join::laid(blocks, env.ctx.env.text.review.view);
    let children = child_ids(&blocks, Some(width), env, cell, memos)?;
    let probe = StoryKey::new(children.clone(), Some(width), cell, env, &blocks);
    cached(memos, |c| &mut c.stories, probe, story_bytes, |m| layout_with(&blocks, &children, width, env, cell, m))
}

/// [`layout`] without the cache, with the blocks' children already resolved (shaped paragraphs, tables laid out at
/// `width`); blocks without one are resolved here.
pub fn layout_with(
    blocks: &[LaidBlock],
    resolved: &[ChildId],
    width: f64,
    env: &StoryEnv,
    cell: Option<CellEnds>,
    memos: &mut Memos,
) -> Result<StoryLayout, Yield> {
    let text = &env.ctx.env.text;
    let view = text.review.view;
    let mut units = Vec::new();
    let mut y = 0.0;
    let mut prev: Option<&ParaStyle> = None;
    let mut prev_after = 0.0;
    let (in_cell, ends) = (cell.is_some(), cell.unwrap_or_default());
    let contextual = contextual_in(text, in_cell);
    // A cell's wrapping floats: (unit, child, top and bottom of the wrap exclusion in the story's coordinates).
    let mut wrapping: Vec<(usize, usize, f64, f64)> = Vec::new();
    let mut float_bottom = 0.0f64;
    // FLOAT-14: the lowest bottom of the wrapping floats the cell's content height holds.
    let mut held = 0.0f64;
    // HDR-11: the table just before (its style, layout and bottom border), while nothing came between.
    let mut prev_table: Option<(&Table, f64)> = None;
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            // STYLE-7: a hidden paragraph isn't laid out, except as a cell's last.
            LaidBlock::Para(laid) if super::paragraph::vanished(laid.para()) && !(cell.is_some() && bi + 1 == blocks.len()) => {}
            LaidBlock::Para(laid) => {
                let p = laid.para();
                let next = match blocks.get(bi + 1) {
                    Some(LaidBlock::Para(n)) => Some(&**n.para().para_style_in(view)),
                    _ => None,
                };
                let shaped = match resolved.get(bi) {
                    Some(ChildId::Para(s)) => s.clone(),
                    _ => shape(p, &laid.shape_space_in(env.fields, env.notes, &env.ctx, cell.is_some()), env.ctx.fonts, memos)?,
                };
                let style = p.para_style_in(view);
                // LINE-16: headers, footers, notes and text boxes don't snap to the grid; cells only with the option.
                let line_grid = text.line_grid.filter(|_| in_cell && text.adjust_line_height_in_table);
                let ps = ParaSpace {
                    width,
                    start: 0,
                    borders: neighbour_borders(style, prev, next),
                    x0: 0.0,
                    env: TextEnv { line_grid, ..text.clone() },
                    fit: ends.fit,
                };
                let lines = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
                let (mut before, mut after) = (space_before(text, style), space_after(text, style));
                // PARA-5: a cell's first paragraph takes no automatic space-before, its last no automatic space-after
                // unless it is a list paragraph.
                if in_cell && style.before_auto && bi == 0 {
                    before = 0.0;
                }
                if in_cell && style.after_auto && bi + 1 == blocks.len() && style.list.is_none() {
                    after = 0.0;
                }
                let g = match prev {
                    // PARA-4: a cell's first paragraph against the paragraph before the cell.
                    None if bi == 0 && ends.before => 0.0,
                    None => before,
                    Some(_) if !contextual => gap_plain(text, prev_after, before),
                    Some(ps) => gap(text, ps, prev_after, style, before),
                };
                y += g;
                let id = para_id(p);
                let para_top = y;
                let fr = float::Frames { page_w: width, page_h: 0.0, margin: [0.0; 4], column: (0.0, width), compat: text.compat };
                // FLOAT-14: the wrapping floats the paragraph lays out in the cell from its column (or character) and
                // its paragraph (or line) without a vertical alignment, by anchor piece, placed against its top.
                let mut own: Vec<(usize, Placed)> = Vec::new();
                if in_cell && !ends.fit {
                    for (k, q) in lines.pieces().iter().enumerate() {
                        let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, lines.shaped.object(lines.from + k)) else { continue };
                        if !f.in_cell(text.compat)
                            || f.wrap == "none"
                            || f.v_align.is_some()
                            || !matches!(f.h_relative.as_str(), "column" | "character")
                            || !matches!(f.v_relative.as_str(), "paragraph" | "line")
                        {
                            continue;
                        }
                        let h = float_height(f, env, memos)?;
                        let (fx, fy) = (float::h_pos(f, &fr), float::v_pos(f, h, &fr, para_top, para_top));
                        own.push((k, Placed::new((0, bi, k), id, f, fx, fy, h)));
                    }
                }
                let floats: Vec<Placed> = own.iter().map(|(_, a)| a.clone()).collect();
                let bands =
                    (!floats.is_empty()).then(|| wrap_bands(&lines, p, style, &floats, &ps, para_top, env.ctx.fonts)).filter(|b| !b.is_empty());
                let rows = bands.as_ref().map_or(lines.lines.len(), Vec::len);
                for i in 0..rows {
                    let ((start, end, pitch, below), mut content) = match &bands {
                        None => {
                            let l = &lines.lines[i];
                            let placed = PlacedLine { x: 0.0, y: 0.0, line: lines.fragment(i, p, env.ctx.fonts) };
                            ((l.start, l.end, l.pitch, l.below), vec![Child::Line { para: Some(id), placed }])
                        }
                        Some(b) => {
                            let (top, segs) = &b[i];
                            y = *top;
                            let o = lines.origin();
                            let drawn = segs.iter().map(|l| {
                                let placed = PlacedLine { x: 0.0, y: 0.0, line: Rc::new(draw_line(&o, l, p, env.ctx.fonts)) };
                                Child::Line { para: Some(id), placed }
                            });
                            let (l, last) = (&segs[0], segs.last().map_or(segs[0].end, |l| l.end));
                            ((l.start, last, l.pitch, l.below), drawn.collect())
                        }
                    };
                    if in_cell {
                        // A float anchored in a cell, whatever its wrap: drawn from the anchor line (FLOAT-14:
                        // positioned in the cell, `move_floats` places compat ≤ 14's layoutInCell="0" ones like the
                        // body's). A wrapping one placed from its paragraph or line without an alignment holds the
                        // cell's content height down to its bottom.
                        // shortcut: only its anchor paragraph wraps a float, and only one placed as `own` says: another
                        // that a line of text would have to wrap isn't drawn (below); `page` is taken as the cell's
                        // content box rather than the cell inside its borders.
                        for k in start..end.max(start + 1) {
                            if lines.pieces().get(k).is_none_or(|q| q.kind != Kind::Anchor) {
                                continue;
                            }
                            let Some(Object::Float(f)) = lines.shaped.object(lines.from + k) else {
                                continue;
                            };
                            let h = float_height(f, env, memos)?;
                            let (fx, fy) = match own.iter().find(|(o, _)| *o == k) {
                                Some((_, a)) => (a.rect[0], a.rect[1] - y),
                                None => (super::float::h_pos(f, &fr), super::float::v_pos(f, h, &fr, para_top, y) - y),
                            };
                            if f.in_cell(text.compat) {
                                float_bottom = float_bottom.max(y + fy + h);
                                if f.wrap != "none" && f.v_align.is_none() && matches!(f.v_relative.as_str(), "paragraph" | "line") {
                                    held = held.max(y + fy + h);
                                }
                            }
                            if f.wrap != "none" && own.iter().all(|(o, _)| *o != k) {
                                wrapping.push((units.len(), content.len(), y + fy - f.dist[0], y + fy + h + f.dist[1]));
                            }
                            let anchor = super::join::piece_para(&lines.shaped, lines.from + k, id);
                            content.push(float_box(f, [fx, fy, fx + f.w, fy + h], anchor, env, memos)?);
                        }
                    }
                    units.push(StoryUnit {
                        top: y,
                        height: pitch,
                        below,
                        after: if i + 1 == rows { after } else { 0.0 },
                        content: content.into(),
                        block: bi,
                        widow: style.widow_control,
                        keep_next: style.keep_next,
                        keep_lines: style.keep_lines,
                        notes: (start..end).filter_map(|k| lines.shaped.note(lines.from + k)).collect(),
                        row: None,
                    });
                    y += pitch;
                }
                prev = Some(style);
                prev_after = after;
                prev_table = None;
            }
            LaidBlock::Table(t) => {
                y += prev_after; // TABLE-48: the space-after before a table applies to its border edge
                let tl = match resolved.get(bi) {
                    Some(ChildId::Table(tb)) => tb.clone(),
                    _ => super::table::layout(&as_nested(t, cell), width, env, None, false, memos)?,
                };
                let (rows, h) = super::table::table_units(&tl);
                // HDR-11: outside cells, a table right after another of the same style and layout continues it: one
                // border between them, not the first one's bottom border and then this one's top.
                // shortcut: each keeps its own borders and cell margins; only the first's bottom border goes.
                if !in_cell
                    && let Some((p, bottom)) = prev_table
                    && p.style_id == t.style_id
                    && p.fixed == t.fixed
                {
                    y -= bottom;
                }
                // TABLE-25.
                let shift = if in_cell && text.compat == Compat::Legacy { super::table::nested_shift(t) } else { 0.0 };
                for mut u in rows {
                    u.block = bi;
                    u.top += y;
                    if shift != 0.0 {
                        u.content = Rc::from([Child::Group { offset: (shift, 0.0), children: u.content }]);
                        u.row = u.row.map(|r| Rc::new(NestedRow { shift, ..(*r).clone() }));
                    }
                    units.push(u);
                }
                y += h;
                prev = None;
                prev_after = 0.0;
                prev_table = Some((*t, tl.bottom_border));
            }
        }
    }
    // PARA-4, TABLE-29: a cell's last paragraph against the paragraph after the cell.
    if !ends.after {
        y += prev_after;
    }
    unwrapped_floats(&mut units, &wrapping);
    Ok(StoryLayout { units, height: y.max(held), float_bottom })
}

/// Drops the `wrapping` floats (unit, child, exclusion top and bottom) that a unit with text overlaps: its text would
/// have to wrap them, which cells don't do (FLOAT-14 shortcut).
fn unwrapped_floats(units: &mut [StoryUnit], wrapping: &[(usize, usize, f64, f64)]) {
    let has_text = |u: &StoryUnit| {
        u.content.iter().any(|c| match c {
            Child::Line { placed, .. } => placed.line.items.iter().any(|i| matches!(i, Item::Glyphs(r) if !r.glyphs.is_empty())),
            Child::Box(_) => false,
            _ => true,
        })
    };
    let covered: Vec<(usize, usize)> = wrapping
        .iter()
        .filter(|&&(_, _, top, bottom)| units.iter().any(|u| has_text(u) && u.top < bottom - EPS && u.top + u.height > top + EPS))
        .map(|&(u, c, _, _)| (u, c))
        .collect();
    for (ui, ci) in covered.into_iter().rev() {
        let content: Vec<Child> = units[ui].content.iter().enumerate().filter(|&(k, _)| k != ci).map(|(_, c)| c.clone()).collect();
        units[ui].content = content.into();
    }
}

/// FLOAT-14: a cell paragraph's lines wrapped around the floats it lays out in the cell (cell coordinates, the
/// paragraph's first line at `top`): bands of segments as in the body (FLOAT-2 to FLOAT-4), with no column bottom.
fn wrap_bands(
    lines: &ParaLines,
    p: &Paragraph,
    style: &ParaStyle,
    floats: &[Placed],
    ps: &ParaSpace,
    top: f64,
    fonts: &Fonts,
) -> Vec<(f64, Vec<Line>)> {
    let frame = Frame::new(ps);
    let pieces = lines.pieces();
    let n = pieces.len();
    let mark = p.mark_in(ps.env.review.view);
    let est = lines.lines.first().map_or(13.0, |l| l.pitch);
    let mut bands: Vec<(f64, Vec<Line>)> = Vec::new();
    let (mut yy, mut s) = (top, 0);
    for _ in 0..4000 {
        if s >= n && !bands.is_empty() {
            break;
        }
        let (mut h, mut got, mut trial) = (est, None, None);
        for attempt in 0..2 {
            got = None;
            let Some(iv) = float::intervals(floats, 0.0, ps.width, yy, yy + h) else { break };
            let open = iv.len() == 1 && iv[0].0.abs() < EPS && (iv[0].1 - ps.width).abs() < EPS;
            match band(&lines.shaped, pieces, s, &iv, bands.is_empty(), style, mark, &frame, fonts, open) {
                Ok((segs, e)) => {
                    let ph = segs[0].pitch;
                    let done = (ph - h).abs() < EPS || attempt == 1;
                    got = Some((segs, e));
                    if done {
                        break;
                    }
                    h = ph;
                }
                Err(t) => {
                    trial = Some(t);
                    break;
                }
            }
        }
        if let Some((segs, e)) = got {
            let ph = segs[0].pitch;
            bands.push((yy, segs));
            yy += ph;
            s = e;
            continue;
        }
        // FLOAT-4 (compat <= 14): beside square floats the line retries lower in steps of its trial height; else it
        // moves down to the next obstacle edge.
        let square = floats.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square");
        yy = match trial.filter(|&t| ps.env.compat == Compat::Legacy && square && t > EPS) {
            Some(t) => yy + t,
            None => float::next_edge(floats, 0.0, ps.width, yy, yy + h).unwrap_or(yy + h),
        };
    }
    bands
}

/// A float's height: the picture's, or a text box's (spAutoFit: content + insets + outline, FLOAT-10).
pub fn float_height(f: &Float, env: &StoryEnv, memos: &mut Memos) -> Result<f64, Yield> {
    Ok(match &f.text_box {
        Some(tb) if tb.auto_fit => {
            let w = (f.w - tb.insets[0] - tb.insets[2] - tb.outline).max(1.0);
            let blocks: Vec<&Block> = tb.blocks.iter().collect();
            let fields = FieldValues::default();
            let h = layout(&blocks, w, &StoryEnv { fields: &fields, ..*env }, None, memos)?.height;
            let h = h + tb.insets[1] + tb.insets[3] + tb.outline;
            // FRAME-1: a frame's `h` without hRule="exact" is a minimum.
            if f.auto_w || tb.insets[1] == 0.0 && tb.insets[3] == 0.0 && tb.outline == 0.0 { h.max(f.h) } else { h }
        }
        _ => f.h,
    })
}

/// A float drawn at `rect`: its picture, or its text box with content and outline (FLOAT-10).
pub fn float_box(f: &Float, rect: [f64; 4], anchor: ParaId, env: &StoryEnv, memos: &mut Memos) -> Result<Child, Yield> {
    let [x0, y0, x1, y1] = rect;
    let mut items = Vec::new();
    let mut groups = Vec::new();
    if let Some(img) = f.image {
        items.push(Item::Image { rect, key: ImageKey { index: Some(img), generation: 0 } });
    }
    if let Some(tb) = &f.text_box {
        text_box(tb, rect, env, &mut groups, memos)?;
    }
    // FLOAT-15: a group's pictures and text boxes, each in its own box.
    for part in &f.parts {
        let r = [x0 + part.rect[0], y0 + part.rect[1], x0 + part.rect[2], y0 + part.rect[3]];
        if let Some(img) = part.image {
            items.push(Item::Image { rect: r, key: ImageKey { index: Some(img), generation: 0 } });
        }
        if let Some(tb) = &part.text_box {
            text_box(tb, r, env, &mut groups, memos)?;
        }
    }
    let mut children = Vec::new();
    if !items.is_empty() {
        children.push(Child::Items(items.into()));
    }
    children.extend(groups);
    Ok(Child::Box(BoxFragment {
        kind: BoxKind::Float { anchor, float: FloatId(f.id), rect },
        id: None,
        offset: (0.0, 0.0),
        size: (x1 - x0, y1 - y0),
        children: children.into(),
    }))
}

/// A text box's content and outline in `rect` (FLOAT-10), appended to `groups`.
fn text_box(tb: &crate::docx::TextBox, rect: [f64; 4], env: &StoryEnv, groups: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
    let [x0, y0, x1, y1] = rect;
    {
        let o = tb.outline;
        let w = (x1 - x0 - tb.insets[0] - tb.insets[2] - o).max(1.0);
        let blocks: Vec<&Block> = tb.blocks.iter().collect();
        // A text box or frame shows its story's fields: a header's PAGE counts in its frames (the page number
        // gallery's framed PAGE); body text boxes get none (page::draw_floats).
        let story = layout(&blocks, w, env, None, memos)?;
        let (cx, mut cy) = (x0 + tb.insets[0] + o / 2.0, y0 + tb.insets[1] + o / 2.0);
        let content_bottom = y1 - tb.insets[3] - o / 2.0;
        // FLOAT-10: centred or bottom text takes all or half (in whole layout units, down) of the room below it, never
        // less than none; compat ≤ 14 with doNotVertAlignInTxbx keeps it at the top.
        // shortcut: floats inside the text box don't count as its content.
        let text = &env.ctx.env.text;
        if !tb.auto_fit && tb.v_anchor > 0 && !(text.compat == Compat::Legacy && text.do_not_vert_align_in_txbx) {
            let free = Lu::round_half_up(content_bottom - cy - story.height).0.max(0);
            cy += Lu(if tb.v_anchor == 2 { free } else { free / 2 }).to_pt();
        }
        for u in &story.units {
            // FLOAT-10: a line is drawn if its box starts above the content bottom.
            if !tb.auto_fit && cy + u.top >= content_bottom - EPS {
                continue;
            }
            // FLOAT-10: the content bottom clips per glyph, as an exact row's bottom does (TABLE-34).
            let bottom = if tb.auto_fit { f64::INFINITY } else { content_bottom - (cy + u.top) + super::table::INK_SLACK };
            let children = super::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom).unwrap_or_else(|| u.content.clone());
            groups.push(Child::Group { offset: (cx, cy + u.top), children });
        }
        if o > 0.0 {
            let h = o / 2.0;
            let outline = [[x0 - h, y0 - h, x1 + h, y0 + h], [x0 - h, y1 - h, x1 + h, y1 + h], [x0 - h, y0, x0 + h, y1], [x1 - h, y0, x1 + h, y1]];
            groups.push(Child::Items(outline.iter().map(|r| Item::Rect { rect: *r, colour: 0 }).collect()));
        }
    }
    Ok(())
}

/// A header or footer story's anchored floats, drawn in page coordinates (FLOAT-6): `top` is where the story's first
/// unit starts on the page. They don't change the story's height. Floats in its table cells are drawn by the cells.
/// shortcut: a line-relative float is placed against its paragraph's first line; header floats don't wrap the body.
pub fn hf_floats(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &super::float::Frames,
    top: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Vec<Child>, Yield> {
    let mut out = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let Block::Para(p) = b else { continue };
        let Some(u) = story.units.iter().find(|u| u.block == bi) else { continue };
        // The story's first paragraph is measured from above its space-before (as Word places footer stamps).
        // shortcut: later paragraphs from their first line; not checked against Word.
        let para_top = top + u.top - if bi == 0 { space_before(&env.ctx.env.text, p.para_style_in(env.ctx.env.text.review.view)) } else { 0.0 };
        for f in env.ctx.floats_of(p) {
            let fitted = fit_frame(f, fr.column.1, env, memos)?;
            let f = fitted.as_ref().unwrap_or(f);
            let h = float_height(f, env, memos)?;
            let (fx, fy) = (super::float::h_pos(f, fr), super::float::v_pos(f, h, fr, para_top, para_top));
            out.push(float_box(f, [fx, fy, fx + f.w, fy + h], para_id(p), env, memos)?);
        }
    }
    Ok(out)
}

/// FRAME-1: a frame without `w` is as wide as its content (trailing spaces excluded), at most `width`, with the
/// fields of the story it is in. `None` for any other float.
pub fn fit_frame(f: &Float, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<Option<Float>, Yield> {
    let Some(tb) = f.text_box.as_ref().filter(|_| f.auto_w) else { return Ok(None) };
    let view = env.ctx.env.text.review.view;
    let mut w: f64 = 0.0;
    for b in super::join::laid(&tb.blocks, view) {
        let LaidBlock::Para(laid) = b else { continue };
        let p = laid.para();
        let space = laid.shape_space(env.fields, env.notes, &env.ctx);
        let shaped = shape(p, &space, env.ctx.fonts, memos)?;
        let ps = super::flow::para_space(p.para_style_in(view), None, None, width, 0, 0.0, &env.ctx);
        let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
        let pieces = pb.pieces();
        for l in &pb.lines {
            for (k, i) in (l.start..l.end).enumerate() {
                if pieces[i].kind != Kind::Space {
                    w = w.max(l.xs[k] + l.widths[k]);
                }
            }
        }
    }
    Ok(Some(Float { w: w.min(width) + tb.insets[0] + tb.insets[2], ..f.clone() }))
}

/// The header and footer height cap (HDR-8).
pub fn hf_limit(compat: Compat, page_h: f64) -> f64 {
    if compat == Compat::Legacy { page_h / 2.0 } else { f64::INFINITY }
}

/// The units of a story that end within `limit` of its top (HDR-8 keeps whole lines), placed at (`x0`, `top`), and the
/// height they take: the story's height when nothing was cut.
pub fn stack_limited(story: &StoryLayout, x0: f64, top: f64, limit: f64) -> (Vec<Child>, f64) {
    let mut out = Vec::new();
    let mut kept = 0.0;
    let mut cut = false;
    for u in &story.units {
        if !cut && u.top + u.height <= limit + EPS {
            out.push(Child::Group { offset: (x0, top + u.top), children: u.content.clone() });
            kept = u.top + u.height;
        } else {
            cut = true;
        }
    }
    (out, if cut { kept } else { story.height })
}
