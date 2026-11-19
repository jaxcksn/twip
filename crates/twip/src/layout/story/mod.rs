//! Story layout

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::review::MarkupView;
use crate::docx::{Block, Float, Inline, NoteKind, ParaStyle, Paragraph, Table};
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::float::{self, Placed};
use super::fragment::{Child, Item, PlacedLine};
use super::ids::ParaId;
use super::job::{Memos, Yield};
use super::join::{LaidBlock, LaidPara};
use super::line::{Frame, Line, ParaLines, band, break_lines, draw_line, neighbour_borders};
use super::paragraph::{contextual_in, gap, gap_plain, space_after, space_before};
use super::space::ParaSpace;
use super::table::TableLayout;
use super::text::{FieldValues, Kind, Object, ShapedPara, para_id, shape};
use super::units::EPS;

mod boxes;
mod hf;

pub use boxes::{fit_frame, float_box, inline_text_box};
pub(super) use boxes::{shape_items, text_frame_size};
pub use hf::{footer_layout, hf_float_rects, hf_floats, hf_layout, hf_limit};

use boxes::{box_env, inner_frame, text_flow, vertical_box};

#[derive(Clone, Debug)]
pub struct StoryUnit {
    pub top: f64,
    pub height: f64,
    pub below: f64,
    pub after: f64,
    /// How far below its top the floats it anchors hold the cell's content down (0 for none).
    pub hold: f64,
    pub content: Rc<[Child]>,
    pub block: usize,
    /// The line's first piece in its paragraph (0 for a table row).
    pub from: usize,
    pub widow: bool,
    pub keep_next: bool,
    pub keep_lines: bool,
    pub notes: Box<[(NoteKind, i64)]>,
    pub row: Option<Rc<NestedRow>>,
}

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
    pub height: f64,
    pub float_bottom: f64,
    /// A header's or footer's floats that keep where they were first placed on the page, by block and float.
    pub frozen: Vec<((usize, usize), [f64; 4])>,
    /// Each paragraph's top above its space before, by block.
    pub origins: Vec<(usize, f64)>,
    /// A header's or footer's floating tables as obstacles, from the story's top left.
    pub tables: Vec<Placed>,
}

impl PartialEq for StoryLayout {
    fn eq(&self, o: &Self) -> bool {
        let StoryLayout { units, height, float_bottom, frozen, origins: _, tables } = self;
        *height == o.height
            && *float_bottom == o.float_bottom
            && *frozen == o.frozen
            && *tables == o.tables
            && units.len() == o.units.len()
            && units
                .iter()
                .zip(&o.units)
                .all(|(a, b)| a.top == b.top && a.height == b.height && a.below == b.below && a.block == b.block && a.notes == b.notes)
    }
}

#[derive(Clone, Copy)]
pub struct StoryEnv<'a> {
    pub ctx: LayoutCtx<'a>,
    pub fields: &'a FieldValues,
    pub notes: &'a HashMap<(NoteKind, i64), String>,
}

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

pub fn child_ids(blocks: &[LaidBlock], width: Option<f64>, env: &StoryEnv, cell: Option<CellEnds>, memos: &mut Memos) -> Result<Vec<ChildId>, Yield> {
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        out.push(match b {
            LaidBlock::Para(p) => {
                ChildId::Para(shape(p.para(), &p.shape_space_in(env.fields, env.notes, &env.ctx, cell.is_some(), memos)?, env.ctx.fonts, memos)?)
            }
            LaidBlock::Table(t) => ChildId::Table(match width {
                Some(w) => nested_layout(t, cell, w, env, false, memos)?,
                None => nested_layout(t, cell, 1e9, env, true, memos)?,
            }),
        });
    }
    Ok(out)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellEnds {
    pub before: bool,
    pub after: bool,
    pub fixed: bool,
    pub fit: bool,
    /// The cell's box inside its borders, which `page` floats count from, in twips: where the text starts in it and
    /// its width. Zero: the text's own box.
    pub page: (i32, i32),
}

/// A story's table: nested when the story is a cell's.
fn nested_layout(t: &Table, cell: Option<CellEnds>, width: f64, env: &StoryEnv, measure: bool, memos: &mut Memos) -> Result<Rc<TableLayout>, Yield> {
    match cell {
        Some(_) => super::table::layout_nested(&as_nested(t, cell), width, env, measure, memos),
        None => super::table::layout(t, width, env, None, measure, memos),
    }
}

pub fn as_nested(t: &Table, cell: Option<CellEnds>) -> std::borrow::Cow<'_, Table> {
    match cell {
        Some(c) if c.fixed != t.fixed || t.ind < 0.0 => std::borrow::Cow::Owned(Table { fixed: c.fixed, ind: t.ind.max(0.0), ..t.clone() }),
        _ => std::borrow::Cow::Borrowed(t),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StoryKey {
    children: Vec<ChildId>,
    width: u64,
    cell: Option<CellEnds>,
    text: TextEnv,
    fonts: u64,
    boxed_fields: Option<FieldValues>,
    hf: Option<(f64, f64)>,
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
            hf: None,
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

fn story_bytes(s: &StoryLayout) -> usize {
    std::mem::size_of::<StoryLayout>() + s.units.len() * (std::mem::size_of::<StoryUnit>() + 32)
}

pub fn layout(blocks: &[&Block], width: f64, env: &StoryEnv, cell: Option<CellEnds>, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
    layout_in(blocks, width, env, cell, None, memos)
}

/// `hf`: a header or footer story, whose floating tables float: its top on the page and the page's top margin.
fn layout_in(
    blocks: &[&Block],
    width: f64,
    env: &StoryEnv,
    cell: Option<CellEnds>,
    hf: Option<(f64, f64)>,
    memos: &mut Memos,
) -> Result<Rc<StoryLayout>, Yield> {
    let last = if cell.is_some() { None } else { env.ctx.env.normal.as_ref() };
    let blocks = super::join::laid_in(blocks, env.ctx.env.text.review.view, last);
    let children = child_ids(&blocks, Some(width), env, cell, memos)?;
    let probe = StoryKey { hf, ..StoryKey::new(children.clone(), Some(width), cell, env, &blocks) };
    cached(memos, |c| &mut c.stories, probe, story_bytes, |m| layout_with(&blocks, &children, width, env, cell, (0, 0), &[], hf, m))
}

/// The story's text from piece `at.1` of block `at.0` on, laid out at `width` (not cached).
pub fn layout_from(blocks: &[&Block], width: f64, env: &StoryEnv, at: (usize, usize), memos: &mut Memos) -> Result<StoryLayout, Yield> {
    let blocks = super::join::laid_in(blocks, env.ctx.env.text.review.view, env.ctx.env.normal.as_ref());
    let children = child_ids(&blocks, Some(width), env, None, memos)?;
    layout_with(&blocks, &children, width, env, None, at, &[], None, memos)
}

#[allow(clippy::too_many_arguments, reason = "the story, its width and cell ends, where it starts, the floats placed around it and its kind")]
pub fn layout_with(
    blocks: &[LaidBlock],
    resolved: &[ChildId],
    width: f64,
    env: &StoryEnv,
    cell: Option<CellEnds>,
    start: (usize, usize),
    around: &[Placed],
    hf: Option<(f64, f64)>,
    memos: &mut Memos,
) -> Result<StoryLayout, Yield> {
    let text = &env.ctx.env.text;
    let view = text.review.view;
    let (in_cell, ends) = (cell.is_some(), cell.unwrap_or_default());
    let contextual = contextual_in(text, in_cell);
    let w = StoryIn { blocks, resolved, width, env, cell, around, hf, text, view, in_cell, ends, contextual };
    let mut st = Stack {
        units: Vec::new(),
        y: 0.0,
        prev: None,
        prev_after: 0.0,
        wrapping: Vec::new(),
        float_bottom: 0.0,
        held: 0.0,
        origins: Vec::new(),
        tables: Vec::new(),
        hf_tables: Vec::new(),
        aligned: Vec::new(),
    };
    for (bi, b) in blocks.iter().enumerate() {
        // The part of a paragraph starting at a later line goes on without its space before.
        let from = if bi == start.0 { start.1 } else { 0 };
        match b {
            _ if bi < start.0 => {}
            LaidBlock::Para(laid) if super::paragraph::vanished(laid.para()) && !(cell.is_some() && bi + 1 == blocks.len()) => {}
            // Paragraphs joined by hidden marks at the story's end that show nothing aren't laid out (a cell's last
            // paragraph keeps its line).
            LaidBlock::Para(laid)
                if cell.is_none()
                    && bi + 1 == blocks.len()
                    && view.hides(&laid.para().mark.rev)
                    && super::paragraph::shows_nothing(laid.para(), view) => {}
            LaidBlock::Para(laid) => stack_para(&w, &mut st, bi, laid, from, memos)?,
            LaidBlock::Table(t) => stack_table(&w, &mut st, bi, t, memos)?,
        }
    }
    let Stack { mut units, mut y, prev_after, wrapping, float_bottom, held, origins, hf_tables, aligned, .. } = st;
    if !ends.after {
        y += prev_after;
    }
    // A cell's vertically aligned floats are wrapped by every line of the cell that meets them: laid out again with
    // them placed.
    // ponytail: one more pass, with the places the first gave them.
    if !aligned.is_empty() && around.is_empty() {
        return layout_with(blocks, resolved, width, env, cell, start, &aligned, hf, memos);
    }
    unwrapped_floats(&mut units, &wrapping);
    Ok(StoryLayout { units, height: y.max(held), float_bottom, frozen: Vec::new(), origins, tables: hf_tables })
}

// Paragraph block
fn stack_para<'b>(w: &StoryIn<'b, '_>, st: &mut Stack<'b>, bi: usize, laid: &'b LaidPara<'b>, from: usize, memos: &mut Memos) -> Result<(), Yield> {
    let &StoryIn { blocks, resolved, width, env, cell, around, text, view, in_cell, ends, contextual, .. } = w;
    let Stack { ref mut units, ref mut wrapping, ref mut origins, ref tables, ref mut aligned, .. } = *st;
    let (prev, prev_after) = (st.prev, st.prev_after);
    let (mut y, mut float_bottom, mut held) = (st.y, st.float_bottom, st.held);
    let p = laid.para();
    let next = match blocks.get(bi + 1) {
        Some(LaidBlock::Para(n)) => Some(&**n.para().para_style_in(view)),
        _ => None,
    };
    let shaped = match resolved.get(bi) {
        Some(ChildId::Para(s)) => s.clone(),
        _ => shape(p, &laid.shape_space_in(env.fields, env.notes, &env.ctx, cell.is_some(), memos)?, env.ctx.fonts, memos)?,
    };
    let style = p.para_style_in(view);
    let line_grid = text.line_grid.filter(|_| in_cell && text.adjust_line_height_in_table);
    let ps = ParaSpace {
        width,
        start: from,
        borders: neighbour_borders(style, prev, next),
        x0: 0.0,
        env: TextEnv { line_grid, ..text.clone() },
        fit: ends.fit,
        letter_break: false,
    };
    let lines = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
    let (mut before, mut after) = (space_before(text, style), space_after(text, style));
    if in_cell && style.before_auto && bi == 0 {
        before = 0.0;
    }
    if in_cell && style.after_auto && bi + 1 == blocks.len() && style.list.is_none() {
        after = 0.0;
    }
    let g = match prev {
        _ if from > 0 => 0.0,
        None if bi == 0 && ends.before => 0.0,
        None => before,
        Some(_) if !contextual => gap_plain(text, prev_after, before),
        Some(ps) => gap(text, ps, prev_after, style, before),
    };
    // The paragraph's top, which `paragraph` floats count from, is above its space before.
    let kept = match prev {
        Some(ps) if contextual && from == 0 => gap(text, ps, prev_after, style, 0.0),
        Some(_) if from == 0 => prev_after,
        _ => 0.0,
    };
    let origin = y + kept;
    origins.push((bi, origin));
    y += g;
    let id = para_id(p);
    let para_top = y;
    // In a cell `page` is the cell's box inside its borders and `margin` and `column` its text: positions
    // are found in the cell's box, from `inset` left of the text.
    let (inset, page_w) = match ends.page {
        (_, 0) => (0.0, width),
        (l, w) => (l as f64 / 20.0, w as f64 / 20.0),
    };
    let fr = float::Frames {
        page_w,
        page_h: 0.0,
        margin: [inset, page_w - inset - width, 0.0, 0.0],
        column: (inset, width),
        compat: text.compat,
        cell: true,
        odd: true,
    };
    let h_pos = |f: &Float, h: f64| float::page_clamp(f, (float::h_pos(f, &fr), 0.0), h, &fr).0 - inset;
    // A wrapping float laid out in the cell doesn't start above the cell's content top.
    let v_pos = |f: &Float, h: f64, line_top: f64| {
        let y = float::v_pos(f, h, &fr, origin, line_top);
        if f.wrap != "none" && f.in_cell(text.compat) { y.max(0.0) } else { y }
    };
    // Wrapping floats
    let mut own: Vec<(usize, Placed)> = Vec::new();
    if in_cell && !ends.fit {
        for (k, q) in lines.pieces().iter().enumerate() {
            let (Kind::Anchor, Some(Object::Float(f))) = (q.kind, lines.shaped.object(lines.from + k)) else { continue };
            if !f.in_cell(text.compat)
                || f.wrap == "none"
                || !matches!(f.v_relative.as_str(), "paragraph" | "line" | "margin" | "page") && f.v_align.is_none()
            {
                continue;
            }
            let h = float_height(f, env, memos)?;
            let (fx, fy) = (h_pos(f, h), v_pos(f, h, para_top));
            own.push((k, Placed::new((0, bi, k), id, f, fx, fy, h)));
        }
    }
    let mut floats: Vec<Placed> = own.iter().map(|(_, a)| a.clone()).collect();
    // The story's own floats placed from outside it: only those beside the paragraph's unwrapped lines.
    let bottom = para_top + lines.lines.iter().map(|l| l.pitch).sum::<f64>();
    // In a cell, its other paragraphs' aligned floats (the anchor paragraph's are its own).
    floats.extend(around.iter().filter(|f| !(in_cell && f.key.1 == bi)).chain(tables).filter(|f| f.in_band(para_top, bottom)).cloned());
    if in_cell {
        aligned.extend(own.iter().filter(|(_, a)| a.float.v_align.is_some()).map(|(_, a)| a.clone()));
    }
    let bands = (!floats.is_empty()).then(|| wrap_bands(&lines, p, style, &floats, &ps, (para_top, before), env.ctx.fonts)).filter(|b| !b.is_empty());
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
        let mut hold = 0.0f64;
        if in_cell {
            // Anchored floats
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
                    None => (h_pos(f, h), v_pos(f, h, y) - y),
                };
                // Compat ≤ 14: a float that doesn't wrap, placed from the column, counts from the first
                // piece its line leaves beside the line's own wrapping floats.
                let fx = match own.iter().filter(|(o, _)| (start..end).contains(o)).map(|(_, a)| a.clone()).collect::<Vec<_>>() {
                    line_own
                        if f.wrap == "none"
                            && text.compat == Compat::Legacy
                            && f.h_relative == "column"
                            && f.h_align.is_none()
                            && !line_own.is_empty() =>
                    {
                        let piece = float::intervals(&line_own, 0.0, width, y, y + pitch, true).and_then(|iv| iv.first().map(|p| p.0));
                        fx + piece.unwrap_or(0.0)
                    }
                    _ => fx,
                };
                if f.in_cell(text.compat) {
                    // Compat 15: a tight or through float ends at its polygon's bottom (one along its top
                    // adds nothing). A float that doesn't wrap counts for the alignment only when it
                    // reaches across the cell's text, and never holds the row.
                    let shape = match &f.poly {
                        Some(p) if text.compat == Compat::Generic && matches!(f.wrap.as_str(), "tight" | "through") => {
                            p.iter().map(|q| q.1).fold(0.0, f64::max).clamp(0.0, 1.0) * h
                        }
                        _ => h,
                    };
                    if f.wrap != "none" || fx < width && fx + f.w > 0.0 {
                        float_bottom = float_bottom.max(y + fy + shape);
                    }
                    if f.wrap != "none" {
                        held = held.max(y + fy + shape);
                        hold = hold.max(fy + shape);
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
            hold,
            content: content.into(),
            block: bi,
            from: lines.from + start,
            widow: style.widow_control,
            keep_next: style.keep_next,
            keep_lines: style.keep_lines,
            notes: (start..end).filter_map(|k| lines.shaped.note(lines.from + k)).collect(),
            row: None,
        });
        y += pitch;
    }
    (st.y, st.float_bottom, st.held) = (y, float_bottom, held);
    st.prev = Some(style);
    st.prev_after = after;
    Ok(())
}

// Table block
fn stack_table(w: &StoryIn, st: &mut Stack, bi: usize, t: &Table, memos: &mut Memos) -> Result<(), Yield> {
    let &StoryIn { resolved, width, env, cell, hf, text, in_cell, .. } = w;
    let Stack { ref mut units, ref mut tables, ref mut hf_tables, .. } = *st;
    let prev_after = st.prev_after;
    let (mut y, mut held) = (st.y, st.held);
    y += prev_after;
    let tl = match resolved.get(bi) {
        Some(ChildId::Table(tb)) => tb.clone(),
        _ => nested_layout(t, cell, width, env, false, memos)?,
    };
    let (rows, h) = super::table::table_units(&tl);
    let shift = if in_cell && text.compat == Compat::Legacy { super::table::nested_shift(t, text.single_cell_borders) } else { 0.0 };
    // A floating table in a header, a footer or a cell takes no room in the story: it stands `tblpY` below
    // where it starts (from the text) and the text after it wraps around it. Compat ≤ 14: a header's or
    // footer's text runs over it. A cell holds it.
    // A header's or footer's table against the page or the margin stands `tblpY` (less a twip) below
    // its top, or with `tblpYSpec` top at it.
    // ponytail: a cell's against the page or the margin, and `tblpYSpec` center or bottom, start where the
    // story is.
    let fp = t.floating.filter(|_| hf.is_some() || in_cell);
    let top = match (fp, hf) {
        (Some(fp), _) if fp.v_anchor == crate::docx::props::TableAnchor::Text => y + fp.y,
        (Some(fp), Some((at, margin))) if fp.y_spec.is_none_or(|j| j == crate::docx::props::Jc::Left) => {
            let t0 = if fp.v_anchor == crate::docx::props::TableAnchor::Page { 0.0 } else { margin };
            let dy = if fp.y_spec.is_some() { 0.0 } else { fp.y - 0.05 };
            (t0 + dy).max(0.0) - at
        }
        _ => y,
    };
    for mut u in rows {
        u.block = bi;
        u.top += top;
        if shift != 0.0 {
            u.content = Rc::from([Child::Group { offset: (shift, 0.0), children: u.content }]);
            u.row = u.row.map(|r| Rc::new(NestedRow { shift, ..(*r).clone() }));
        }
        units.push(u);
    }
    match fp {
        Some(fp) => {
            let legacy = in_cell && text.compat == Compat::Legacy;
            let (bl, br) = super::table::float_box(&tl, t, legacy);
            // In a cell the right distance has no minimum.
            let mut dist = float::table_dist(&fp, false);
            if in_cell {
                dist[3] = fp.dist[3];
            }
            let f = float::table_float(t.id, br - bl, h, dist, !fp.never_overlap);
            let placed = Placed::new((0, bi, float::TABLE_PIECE), ParaId(t.id), &f, bl + shift, top, h);
            if in_cell {
                held = held.max(top + h);
            } else {
                hf_tables.push(placed.clone());
            }
            // Compat ≤ 14: a header's or footer's text runs over it.
            if in_cell || text.compat != Compat::Legacy {
                tables.push(placed);
            }
        }
        None => y += h,
    }
    (st.y, st.held) = (y, held);
    st.prev = None;
    st.prev_after = 0.0;
    Ok(())
}

// Story stacking inputs
#[derive(Clone, Copy)]
struct StoryIn<'b, 'e> {
    blocks: &'b [LaidBlock<'b>],
    resolved: &'b [ChildId],
    width: f64,
    env: &'b StoryEnv<'e>,
    cell: Option<CellEnds>,
    around: &'b [Placed],
    hf: Option<(f64, f64)>,
    text: &'b TextEnv,
    view: MarkupView,
    in_cell: bool,
    ends: CellEnds,
    contextual: bool,
}

// Story stacking state
struct Stack<'b> {
    units: Vec<StoryUnit>,
    y: f64,
    prev: Option<&'b ParaStyle>,
    prev_after: f64,
    wrapping: Vec<(usize, usize, f64, f64)>,
    float_bottom: f64,
    held: f64,
    origins: Vec<(usize, f64)>,
    tables: Vec<Placed>,
    hf_tables: Vec<Placed>,
    aligned: Vec<Placed>,
}

// Cell floats
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

fn wrap_bands(
    lines: &ParaLines,
    p: &Paragraph,
    style: &ParaStyle,
    floats: &[Placed],
    ps: &ParaSpace,
    (top, before): (f64, f64),
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
            let Some(iv) = float::intervals(floats, 0.0, ps.width, yy, yy + h, ps.env.compat == crate::docx::Compat::Legacy) else { break };
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
        // Compat ≤ 14 steps down by the text's height past square-wrapped shapes; past a floating table the line goes
        // to its bottom.
        let square = floats.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square" && f.key.2 != float::TABLE_PIECE);
        // A first line that goes below a floating table keeps its space before below it.
        let table = bands.is_empty() && floats.iter().any(|f| f.key.2 == float::TABLE_PIECE && f.in_band(yy, yy + h));
        yy = match trial.filter(|&t| ps.env.compat == Compat::Legacy && square && t > EPS) {
            Some(t) => yy + t,
            None => float::next_edge(floats, 0.0, ps.width, yy, yy + h).unwrap_or(yy + h) + if table { before } else { 0.0 },
        };
    }
    bands
}

pub fn float_height(f: &Float, env: &StoryEnv, memos: &mut Memos) -> Result<f64, Yield> {
    Ok(match &f.text_box {
        // Vertical text grows the box across instead (`fit_frame`).
        Some(tb) if tb.auto_fit && vertical_box(f).is_none() => {
            let (w0, h0) = if f.rot_swap { (f.h, f.w) } else { (f.w, f.h) };
            let h = fitted_height(tb, f.shape.as_deref(), w0, h0, env, memos)?;
            if f.auto_w || f.shape.is_none() && tb.insets[1] == 0.0 && tb.insets[3] == 0.0 && tb.outline == 0.0 { h.max(f.h) } else { h }
        }
        _ => f.h,
    })
}

/// The height of a `w0` by `h0` frame grown or shrunk to text box `tb`'s text.
fn fitted_height(
    tb: &crate::docx::TextBox,
    shape: Option<&crate::drawing::Shape>,
    w0: f64,
    h0: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<f64, Yield> {
    let fr = inner_frame(tb, shape, [0.0, 0.0, w0, h0]);
    let w = (fr[2] - fr[0]).max(1.0);
    let blocks: Vec<&Block> = tb.blocks.iter().collect();
    let fields = FieldValues::default();
    let mut le = None;
    let content = layout(&blocks, w, &StoryEnv { fields: &fields, ..box_env(env, &mut le) }, None, memos)?.height.min(1584.0);
    Ok(h0 - (fr[3] - fr[1]) + content)
}

/// The heights of paragraph `p`'s inline text boxes that fit their text, by their text's id. Vertical text keeps
/// its box.
pub fn fitted_boxes(p: &crate::docx::Paragraph, env: &StoryEnv, memos: &mut Memos) -> Result<Vec<(u64, f64)>, Yield> {
    let mut out = Vec::new();
    for i in &p.inlines {
        let Inline::Picture(pic, _) = i else { continue };
        let Some(t) = pic.text.as_ref().filter(|t| t.text_box.auto_fit && text_flow(&t.text_box, pic.shape.as_deref()).is_none()) else { continue };
        out.push((t.id, fitted_height(&t.text_box, pic.shape.as_deref(), pic.w, pic.h, env, memos)?));
    }
    Ok(out)
}

/// The lines of a tight or through text box with neither fill nor outline, which text wraps around instead of the
/// box: one rectangle per line from the box's top left, laid out from the top without the top inset, its own
/// paragraph's setting choosing which lines take their ink extent rather than the full width.
pub fn tight_lines(f: &Float, env: &StoryEnv, memos: &mut Memos) -> Result<Option<Vec<[f64; 4]>>, Yield> {
    use crate::drawing::paint::Fill;
    let (Some(tb), Some(shape)) = (&f.text_box, f.shape.as_deref()) else { return Ok(None) };
    let bare = shape.sp().is_some_and(|sp| sp.fill == Some(Fill::None) && sp.line.as_ref().and_then(|l| l.fill.as_ref()) == Some(&Fill::None));
    if !bare || !matches!(f.wrap.as_str(), "tight" | "through") || shape.xfrm.rot.0 != 0 {
        return Ok(None);
    }
    let fr = inner_frame(tb, Some(shape), [0.0, 0.0, f.w, f.h]);
    let (left, du, dv) = (fr[0], (fr[2] - fr[0]).max(1.0), fr[3] - fr[1]);
    let blocks: Vec<&Block> = tb.blocks.iter().collect();
    let fields = FieldValues::default();
    let mut le = None;
    let story = layout(&blocks, du, &StoryEnv { fields: &fields, ..box_env(env, &mut le) }, None, memos)?;
    let units = &story.units;
    let mut out = Vec::new();
    for (k, u) in units.iter().enumerate() {
        if u.top >= dv - EPS {
            break;
        }
        let first = k == 0 || units[k - 1].block != u.block;
        let last = units.get(k + 1).is_none_or(|n| n.block != u.block);
        let mode = match tb.blocks.get(u.block) {
            Some(Block::Para(p)) if u.row.is_none() => p.para_style_in(env.ctx.env.text.review.view).tight_wrap,
            _ => 0,
        };
        let selected = match mode {
            1 => true,
            2 => first || last,
            3 => first,
            4 => last,
            _ => false,
        };
        // Shifted right by the left inset, then clipped to the content width from the box's left.
        let (l, r) = ink(&u.content).filter(|_| selected).map_or((0.0, du), |(a, b)| (a.max(0.0), b));
        out.push([left + l, u.top, (left + r).min(du), (u.top + u.height).min(dv)]);
    }
    Ok(Some(out))
}

/// The first character's x and the start of the trailing white space of a line.
fn ink(children: &[Child]) -> Option<(f64, f64)> {
    let mut span: Option<(f64, f64)> = None;
    let mut line = |placed: &PlacedLine| {
        let runs: Vec<_> = placed.line.items.iter().filter_map(|i| if let Item::Glyphs(g) = i { Some(g) } else { None }).collect();
        let chars: Vec<(char, f64)> = runs.iter().flat_map(|g| g.chars.iter().map(|c| (c.0, c.1))).collect();
        let (Some(a), Some(z)) = (chars.iter().position(|c| !c.0.is_whitespace()), chars.iter().rposition(|c| !c.0.is_whitespace())) else { return };
        let end = match chars.get(z + 1) {
            Some(next) => next.1,
            None => runs.last().and_then(|g| g.glyphs.last().map(|&(id, x)| x + g.face.advance(id) * g.face.scale(g.size))).unwrap_or(chars[z].1),
        };
        let (l, r) = (placed.x + chars[a].1, placed.x + end);
        span = Some(span.map_or((l, r), |(sl, sr): (f64, f64)| (sl.min(l), sr.max(r))));
    };
    for c in children {
        match c {
            Child::Line { placed, .. } => line(placed),
            Child::Para(p) => p.lines.iter().for_each(&mut line),
            _ => {}
        }
    }
    span
}

pub fn stack_limited(story: &StoryLayout, x0: f64, top: f64, limit: f64) -> (Vec<Child>, f64) {
    let mut out = Vec::new();
    let mut kept = 0.0;
    let mut cut = false;
    for u in &story.units {
        // A floating table's rows take no room in the story.
        if story.tables.iter().any(|t| t.key.1 == u.block) {
            out.push(Child::Group { offset: (x0, top + u.top), children: u.content.clone() });
            continue;
        }
        if !cut && u.top + u.height <= limit + EPS {
            out.push(Child::Group { offset: (x0, top + u.top), children: u.content.clone() });
            kept = u.top + u.height;
        } else {
            cut = true;
        }
    }
    (out, if cut { kept } else { story.height })
}
