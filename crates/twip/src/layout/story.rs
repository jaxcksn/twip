//! Story layout

use crate::docx::Compat;
use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::{Block, Float, Inline, NoteKind, ParaStyle, Paragraph, Table};
use crate::font::Fonts;

use super::LayoutCtx;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::float::{self, Placed};
use super::fragment::{BoxFragment, BoxKind, Child, ImageKey, Item, Layer, PlacedLine, ShapeItem};
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

#[derive(Clone, Debug)]
pub struct StoryUnit {
    pub top: f64,
    pub height: f64,
    pub below: f64,
    pub after: f64,
    pub content: Rc<[Child]>,
    pub block: usize,
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
}

impl PartialEq for StoryLayout {
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellEnds {
    pub before: bool,
    pub after: bool,
    pub fixed: bool,
    pub fit: bool,
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

fn story_bytes(s: &StoryLayout) -> usize {
    std::mem::size_of::<StoryLayout>() + s.units.len() * (std::mem::size_of::<StoryUnit>() + 32)
}

pub fn layout(blocks: &[&Block], width: f64, env: &StoryEnv, cell: Option<CellEnds>, memos: &mut Memos) -> Result<Rc<StoryLayout>, Yield> {
    let blocks = super::join::laid(blocks, env.ctx.env.text.review.view);
    let children = child_ids(&blocks, Some(width), env, cell, memos)?;
    let probe = StoryKey::new(children.clone(), Some(width), cell, env, &blocks);
    cached(memos, |c| &mut c.stories, probe, story_bytes, |m| layout_with(&blocks, &children, width, env, cell, m))
}

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
    let mut wrapping: Vec<(usize, usize, f64, f64)> = Vec::new();
    let mut float_bottom = 0.0f64;
    let mut held = 0.0f64;
    let mut prev_table: Option<(&Table, f64)> = None;
    for (bi, b) in blocks.iter().enumerate() {
        match b {
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
                if in_cell && style.before_auto && bi == 0 {
                    before = 0.0;
                }
                if in_cell && style.after_auto && bi + 1 == blocks.len() && style.list.is_none() {
                    after = 0.0;
                }
                let g = match prev {
                    None if bi == 0 && ends.before => 0.0,
                    None => before,
                    Some(_) if !contextual => gap_plain(text, prev_after, before),
                    Some(ps) => gap(text, ps, prev_after, style, before),
                };
                y += g;
                let id = para_id(p);
                let para_top = y;
                let fr =
                    float::Frames { page_w: width, page_h: 0.0, margin: [0.0; 4], column: (0.0, width), compat: text.compat, cell: true, odd: true };
                // Wrapping floats
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
                y += prev_after;
                let tl = match resolved.get(bi) {
                    Some(ChildId::Table(tb)) => tb.clone(),
                    _ => super::table::layout(&as_nested(t, cell), width, env, None, false, memos)?,
                };
                let (rows, h) = super::table::table_units(&tl);
                if !in_cell
                    && let Some((p, bottom)) = prev_table
                    && p.style_id == t.style_id
                    && p.fixed == t.fixed
                {
                    y -= bottom;
                }
                let shift = if in_cell && text.compat == Compat::Legacy { super::table::nested_shift(t, text.single_cell_borders) } else { 0.0 };
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
    if !ends.after {
        y += prev_after;
    }
    unwrapped_floats(&mut units, &wrapping);
    Ok(StoryLayout { units, height: y.max(held), float_bottom })
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
        let square = floats.iter().filter(|f| f.in_band(yy, yy + h)).all(|f| f.float.wrap == "square");
        yy = match trial.filter(|&t| ps.env.compat == Compat::Legacy && square && t > EPS) {
            Some(t) => yy + t,
            None => float::next_edge(floats, 0.0, ps.width, yy, yy + h).unwrap_or(yy + h),
        };
    }
    bands
}

pub fn float_height(f: &Float, env: &StoryEnv, memos: &mut Memos) -> Result<f64, Yield> {
    Ok(match &f.text_box {
        Some(tb) if tb.auto_fit => {
            let (w0, h0) = if f.rot_swap { (f.h, f.w) } else { (f.w, f.h) };
            let fr = inner_frame(tb, f.shape.as_deref(), [0.0, 0.0, w0, h0]);
            let w = (fr[2] - fr[0]).max(1.0);
            let blocks: Vec<&Block> = tb.blocks.iter().collect();
            let fields = FieldValues::default();
            let content = layout(&blocks, w, &StoryEnv { fields: &fields, ..*env }, None, memos)?.height.min(1584.0);
            let h = h0 - (fr[3] - fr[1]) + content;
            if f.auto_w || f.shape.is_none() && tb.insets[1] == 0.0 && tb.insets[3] == 0.0 && tb.outline == 0.0 { h.max(f.h) } else { h }
        }
        _ => f.h,
    })
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
    let story = layout(&blocks, du, &StoryEnv { fields: &fields, ..*env }, None, memos)?;
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

// Float drawing
pub fn float_box(f: &Float, rect: [f64; 4], anchor: ParaId, env: &StoryEnv, memos: &mut Memos) -> Result<Child, Yield> {
    let [x0, y0, x1, y1] = rect;
    let shape_rect = if f.rot_swap {
        let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
        [cx - hw, cy - hh, cx + hw, cy + hh]
    } else {
        rect
    };
    let mut items = Vec::new();
    let mut groups = Vec::new();
    match f.shape.as_deref() {
        Some(shape) => {
            let t = &env.ctx.env.text;
            let palette = crate::drawing::paint::Palette { theme: &t.theme, map: &t.colour_map };
            items.extend(shape_items(shape, shape_rect, f.id, f.behind, palette).into_iter().map(Item::Shape));
        }
        None => {
            if let Some(img) = f.image {
                items.push(Item::Image { rect, key: ImageKey { index: Some(img), generation: 0 } });
            }
        }
    }
    let chain = env.ctx.env.chain_of(f.id);
    if let Some((c, k)) = chain {
        linked_text(c, k, f, shape_rect, env, &mut groups, memos)?;
    }
    if let Some(tb) = f.text_box.as_ref().filter(|_| chain.is_none()) {
        let shape = f.shape.as_deref();
        turned_text(
            tb,
            shape,
            shape_rect,
            shape.map_or(0.0, |s| s.xfrm.rot.normalized().degrees()),
            shape.is_some_and(|s| s.xfrm.flip_v),
            env,
            &mut groups,
            memos,
        )?;
        if f.shape.is_none() {
            outline(tb.outline, rect, &mut groups);
        }
    }
    let leaves = f.shape.as_deref().map(|s| s.leaves(shape_rect)).unwrap_or_default();
    for part in &f.parts {
        let r = [x0 + part.rect[0], y0 + part.rect[1], x0 + part.rect[2], y0 + part.rect[3]];
        if f.shape.is_none()
            && let Some(img) = part.image
        {
            items.push(Item::Image { rect: r, key: ImageKey { index: Some(img), generation: 0 } });
        }
        if let Some(tb) = &part.text_box {
            match leaves.iter().find(|l| l.path == part.path) {
                Some(l) => turned_text(tb, Some(l.shape), l.rect, l.rot, l.flip_v, env, &mut groups, memos)?,
                None => text_box(tb, f.shape.as_deref().and_then(|s| s.at(&part.path)), r, env, &mut groups, memos)?,
            }
        }
    }
    let mut children = Vec::new();
    if !items.is_empty() {
        children.push(Child::Items(items.into()));
    }
    children.extend(groups);
    Ok(Child::Box(BoxFragment {
        kind: BoxKind::Float { anchor, float: FloatId(f.id), rect, layer: Layer { behind: f.behind, z: f.z, order: f.order } },
        id: None,
        offset: (0.0, 0.0),
        size: (x1 - x0, y1 - y0),
        children: children.into(),
    }))
}

// Linked text boxes
fn chain_box(f: &Float) -> crate::docx::TextBox {
    if let Some(tb) = &f.text_box {
        return crate::docx::TextBox { blocks: Vec::new(), ..tb.clone() };
    }
    let sp = f.shape.as_deref().and_then(|s| s.sp());
    let pt = |e: crate::drawing::units::Emu| e.0 as f64 / 12700.0;
    let insets = sp.map_or([7.2, 3.6, 7.2, 3.6], |s| s.body.insets.map(pt));
    let outline = sp.and_then(|s| s.line.as_ref()).map_or(0.0, |l| {
        let w = l.width.map_or(0, |w| w.0);
        2.0 * if l.inset == Some(true) { w } else { w / 2 } as f64 / 12700.0
    });
    crate::docx::TextBox { blocks: Vec::new(), insets, outline, auto_fit: false, v_anchor: 0, flow: 0 }
}

pub(super) fn text_frame_size(f: &Float) -> [f64; 2] {
    let (w, h) = if f.rot_swap { (f.h, f.w) } else { (f.w, f.h) };
    let fr = layout_frame(&chain_box(f), f.shape.as_deref(), [0.0, 0.0, w, h]);
    [fr[2] - fr[0], fr[3] - fr[1]]
}

fn chain_slices(story: &StoryLayout, heights: &[f64], widow: bool) -> Vec<std::ops::Range<usize>> {
    let units = &story.units;
    let n = units.len();
    let (mut out, mut at) = (Vec::new(), 0);
    for (k, &h) in heights.iter().enumerate() {
        if k + 1 == heights.len() {
            out.push(at..n);
            break;
        }
        let base = units.get(at).map_or(0.0, |u| u.top);
        let mut end = at;
        while end < n && units[end].top - base + units[end].height <= h + EPS {
            end += 1;
        }
        if widow && at < end && end < n && units[end].widow && units[end - 1].block == units[end].block {
            let b = units[end].block;
            let before = (at..end).rev().take_while(|&i| units[i].block == b).count();
            let after = (end..n).take_while(|&i| units[i].block == b).count();
            let back = match (before, after) {
                (1, _) => 1,
                (2, 1) => 2,
                (_, 1) => 1,
                _ => 0,
            };
            if end - back > at {
                end -= back;
            }
        }
        out.push(at..end);
        at = end;
    }
    out
}

fn linked_text(
    c: &super::env::Chain,
    k: usize,
    f: &Float,
    rect: [f64; 4],
    env: &StoryEnv,
    groups: &mut Vec<Child>,
    memos: &mut Memos,
) -> Result<(), Yield> {
    let blocks: Vec<&Block> = c.story.blocks.iter().collect();
    let story = layout(&blocks, c.boxes[0].1[0].max(1.0), env, None, memos)?;
    let heights: Vec<f64> = c.boxes.iter().map(|b| b.1[1]).collect();
    let slices = chain_slices(&story, &heights, env.ctx.env.text.compat != Compat::Legacy);
    let Some(range) = slices.get(k).cloned() else { return Ok(()) };
    let fr = layout_frame(&chain_box(f), f.shape.as_deref(), rect);
    let base = story.units.get(range.start).map_or(0.0, |u| u.top);
    for u in &story.units[range] {
        let y = fr[1] + u.top - base;
        if y >= fr[3] - EPS {
            continue;
        }
        let bottom = fr[3] - y + super::table::INK_SLACK;
        let children = super::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom).unwrap_or_else(|| u.content.clone());
        groups.push(Child::Group { offset: (fr[0], y), children });
    }
    Ok(())
}

pub fn inline_text_box(
    pic: &crate::docx::Picture,
    rect: [f64; 4],
    anchor: ParaId,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Option<Child>, Yield> {
    let Some(t) = &pic.text else { return Ok(None) };
    let shape = pic.shape.as_deref();
    let (rot, flip_v) = shape.map_or((0.0, false), |s| (s.xfrm.rot.normalized().degrees(), s.xfrm.flip_v));
    let mut groups = Vec::new();
    turned_text(&t.text_box, shape, rect, rot, flip_v, env, &mut groups, memos)?;
    let [x0, y0, x1, y1] = rect;
    Ok(Some(Child::Box(BoxFragment {
        kind: BoxKind::Float { anchor, float: FloatId(t.id), rect, layer: Layer { behind: false, z: u32::MAX, order: usize::MAX } },
        id: None,
        offset: (0.0, 0.0),
        size: (x1 - x0, y1 - y0),
        children: groups.into(),
    })))
}

// Shapes
pub(super) fn shape_items(
    shape: &crate::drawing::Shape,
    rect: [f64; 4],
    float: u64,
    behind: bool,
    palette: crate::drawing::paint::Palette,
) -> Vec<Rc<ShapeItem>> {
    use crate::drawing::ShapeKind;
    use crate::drawing::paint::{self, Fill, Paint};
    use crate::drawing::units::Emu;
    if shape.nv.hidden {
        return Vec::new();
    }
    let mut out = Vec::new();
    for leaf in shape.leaves(rect) {
        let group = leaf.group_fill.map_or(Paint::None, |g| paint::paint(g, palette, None, &Paint::None));
        let [lx0, ly0, lx1, ly1] = leaf.rect;
        let (w, h) = (Emu::from_pt(lx1 - lx0), Emu::from_pt(ly1 - ly0));
        let (geom, fill, line, effects) = match &leaf.shape.kind {
            ShapeKind::Sp(sp) => (
                sp.geom.evaluate(w, h),
                paint::resolve_fill(sp.fill.as_ref(), sp.style.as_ref(), palette, &group),
                paint::resolve_line(sp.line.as_ref(), sp.style.as_ref(), palette, &group),
                paint::resolve_effects(sp.effects.as_ref(), sp.style.as_ref(), palette),
            ),
            ShapeKind::Pic(p) => (
                p.geom.evaluate(w, h),
                paint::paint(&Fill::Blip(p.blip.clone()), palette, None, &group),
                paint::resolve_line(p.line.as_ref(), None, palette, &group),
                paint::resolve_effects(p.effects.as_ref(), None, palette),
            ),
            ShapeKind::Group(_) | ShapeKind::Raw(_) => continue,
        };
        if leaf.shape.nv.hidden {
            continue;
        }
        let mut line = line;
        if line.join == paint::Join::Round && leaf.shape.geometry().and_then(|g| g.preset()).is_some_and(|p| MITER_PRESETS.contains(&p.name())) {
            line.join = paint::Join::Miter(crate::drawing::units::Pct(800_000));
        }
        out.push(Rc::new(ShapeItem {
            rect: leaf.rect,
            rot: leaf.rot,
            flip_h: leaf.flip_h,
            flip_v: leaf.flip_v,
            geom,
            fill,
            line,
            effects,
            behind,
            float,
            picture: leaf.shape.pic().is_some(),
            path: leaf.path,
        }));
    }
    out
}

const MITER_PRESETS: &[&str] = &[
    "bentUpArrow",
    "chevron",
    "cube",
    "downArrow",
    "ellipseRibbon",
    "ellipseRibbon2",
    "leftArrowCallout",
    "leftRightArrowCallout",
    "leftUpArrow",
    "mathEqual",
    "mathMinus",
    "mathMultiply",
    "mathNotEqual",
    "mathPlus",
    "moon",
    "notchedRightArrow",
    "plus",
    "quadArrowCallout",
    "rightArrow",
    "star10",
    "star12",
    "star16",
    "star24",
    "star32",
    "star4",
    "star5",
    "star8",
    "stripedRightArrow",
    "upDownArrow",
    "upDownArrowCallout",
];

fn solid_fill(shape: &crate::drawing::Shape, env: &StoryEnv) -> Option<u32> {
    use crate::drawing::paint::{self, Paint};
    let sp = shape.sp()?;
    let palette = paint::Palette { theme: &env.ctx.env.text.theme, map: &env.ctx.env.text.colour_map };
    match paint::resolve_fill(sp.fill.as_ref(), sp.style.as_ref(), palette, &Paint::None) {
        Paint::Solid(c) if c.a == 255 => Some((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b)),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn turned_text(
    tb: &crate::docx::TextBox,
    shape: Option<&crate::drawing::Shape>,
    rect: [f64; 4],
    deg: f64,
    flip_v: bool,
    env: &StoryEnv,
    groups: &mut Vec<Child>,
    memos: &mut Memos,
) -> Result<(), Yield> {
    let (dark_text, dark_env);
    let env = match shape.and_then(|s| solid_fill(s, env)).filter(|c| super::env::is_dark(*c)) {
        Some(fill) => {
            dark_text = super::LayoutEnv { text: super::env::TextEnv { backdrop: Some(fill), ..env.ctx.env.text.clone() }, ..env.ctx.env.clone() };
            dark_env = StoryEnv { ctx: LayoutCtx { env: &dark_text, ..env.ctx }, ..*env };
            &dark_env
        }
        None => env,
    };
    let body = shape.and_then(|s| s.sp()).map(|s| &s.body);
    let turn = (deg + body.map_or(0.0, |b| b.rot.degrees()) + if flip_v { 180.0 } else { 0.0 }).rem_euclid(360.0);
    let mut frame = layout_frame(tb, shape, rect);
    // A frame's own text flow.
    if body.is_none()
        && let Some(v) = super::vertical::of_cell(tb.flow)
    {
        let blocks: Vec<&Block> = tb.blocks.iter().collect();
        let story = super::vertical::story(&blocks, frame[3] - frame[1], v, env, None, memos)?;
        let bottom = if v.flow == super::vertical::Flow::DownEast { f64::INFINITY } else { frame[2] - frame[0] + super::table::INK_SLACK };
        groups.push(Child::Items(super::vertical::turned(&story.units, v, frame, 0.0, bottom).into()));
        return Ok(());
    }
    use crate::drawing::text::TextDirection as D;
    let dir = body.map_or(D::Horizontal, |b| b.direction);
    if dir != D::Horizontal {
        let [x0, y0, x1, y1] = frame;
        let (fx, fy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
        let quarter = [fx - hw, fy - hh, fx + hw, fy + hh];
        let reverse = matches!(dir, D::MongolianVert | D::WordArtVert | D::WordArtVertRtl);
        let mut lines = Vec::new();
        lay_lines(tb, quarter, Some((VERTICAL_PITCH, reverse)), env, &mut lines, memos)?;
        let quarter_turn = if dir == D::Vert270 { 270.0 } else { 90.0 };
        let inner = Item::Turned(Rc::new(super::fragment::Turned { centre: [fx, fy], deg: quarter_turn, items: super::traverse::items_of(&lines) }));
        let item = if turn.abs() < 1e-9 {
            inner
        } else {
            let centre = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
            Item::Turned(Rc::new(super::fragment::Turned { centre, deg: turn, items: vec![inner] }))
        };
        groups.push(Child::Items(vec![item].into()));
        return Ok(());
    }
    if body.is_some_and(|b| b.upright) {
        if (45.0..135.0).contains(&(deg % 180.0)) {
            let [x0, y0, x1, y1] = frame;
            let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
            frame = [cx - hw, cy - hh, cx + hw, cy + hh];
        }
        return lay_text(tb, frame, env, groups, memos);
    }
    if turn.abs() < 1e-9 {
        return lay_text(tb, frame, env, groups, memos);
    }
    let mut lines = Vec::new();
    lay_text(tb, frame, env, &mut lines, memos)?;
    let centre = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
    let items = super::traverse::items_of(&lines);
    groups.push(Child::Items(vec![Item::Turned(Rc::new(super::fragment::Turned { centre, deg: turn, items }))].into()));
    Ok(())
}

// Text boxes
fn outline(o: f64, rect: [f64; 4], groups: &mut Vec<Child>) {
    if o <= 0.0 {
        return;
    }
    let [x0, y0, x1, y1] = rect;
    let h = o / 2.0;
    let outline = [[x0 - h, y0 - h, x1 + h, y0 + h], [x0 - h, y1 - h, x1 + h, y1 + h], [x0 - h, y0, x0 + h, y1], [x1 - h, y0, x1 + h, y1]];
    groups.push(Child::Items(outline.iter().map(|r| Item::rect(*r, 0)).collect()));
}

fn text_inset(shape: Option<&crate::drawing::Shape>, w: f64, h: f64) -> [f64; 4] {
    use crate::drawing::units::Emu;
    let Some(g) = shape.and_then(crate::drawing::Shape::geometry) else { return [0.0; 4] };
    let r = g.evaluate(Emu::from_pt(w), Emu::from_pt(h)).text_rect;
    [r[0], r[1], w - r[2], h - r[3]]
}

fn inner_frame(tb: &crate::docx::TextBox, shape: Option<&crate::drawing::Shape>, rect: [f64; 4]) -> [f64; 4] {
    let g = text_inset(shape, rect[2] - rect[0], rect[3] - rect[1]);
    let h = tb.outline / 2.0;
    let x0 = rect[0] + g[0] + tb.insets[0] + h;
    let y0 = rect[1] + g[1] + tb.insets[1] + h;
    let x1 = (rect[2] - g[2] - tb.insets[2] - h).max(x0);
    let y1 = (rect[3] - g[3] - tb.insets[3] - h).max(y0);
    [x0, y0, x1, y1]
}

fn text_box(
    tb: &crate::docx::TextBox,
    shape: Option<&crate::drawing::Shape>,
    rect: [f64; 4],
    env: &StoryEnv,
    groups: &mut Vec<Child>,
    memos: &mut Memos,
) -> Result<(), Yield> {
    lay_text(tb, layout_frame(tb, shape, rect), env, groups, memos)
}

fn layout_frame(tb: &crate::docx::TextBox, shape: Option<&crate::drawing::Shape>, rect: [f64; 4]) -> [f64; 4] {
    let mut fr = inner_frame(tb, shape, rect);
    if shape.and_then(|s| s.sp()).is_some_and(|s| s.body.no_wrap) {
        fr[2] += 0.05;
    }
    fr
}

fn lay_text(tb: &crate::docx::TextBox, fr: [f64; 4], env: &StoryEnv, groups: &mut Vec<Child>, memos: &mut Memos) -> Result<(), Yield> {
    lay_lines(tb, fr, None, env, groups, memos)
}

const VERTICAL_PITCH: f64 = 1.032;

fn first_baseline(children: &[Child]) -> Option<f64> {
    children.iter().find_map(|c| match c {
        Child::Line { placed, .. } => Some(placed.y + placed.line.baseline),
        Child::Para(p) => p.lines.first().map(|l| l.y + l.line.baseline),
        Child::Group { offset, children } => first_baseline(children).map(|b| b + offset.1),
        _ => None,
    })
}

fn lay_lines(
    tb: &crate::docx::TextBox,
    fr: [f64; 4],
    vertical: Option<(f64, bool)>,
    env: &StoryEnv,
    groups: &mut Vec<Child>,
    memos: &mut Memos,
) -> Result<(), Yield> {
    let w = (fr[2] - fr[0]).max(1.0);
    let blocks: Vec<&Block> = tb.blocks.iter().collect();
    let story = layout(&blocks, w, env, None, memos)?;
    let (cx, mut cy) = (fr[0], fr[1]);
    let content_bottom = fr[3];
    let text = &env.ctx.env.text;
    if !tb.auto_fit && tb.v_anchor > 0 && !(text.compat == Compat::Legacy && text.do_not_vert_align_in_txbx) {
        let free = Lu::round_half_up(content_bottom - cy - story.height).0.max(0);
        cy += Lu(if tb.v_anchor == 2 { free } else { free / 2 }).to_pt();
    }
    for u in &story.units {
        if !tb.auto_fit && cy + u.top >= content_bottom - EPS {
            continue;
        }
        let bottom = if tb.auto_fit { f64::INFINITY } else { content_bottom - (cy + u.top) + super::table::INK_SLACK };
        let children = super::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom).unwrap_or_else(|| u.content.clone());
        let y = match vertical {
            None => cy + u.top,
            Some((k, reverse)) => {
                let b = first_baseline(&u.content).unwrap_or(0.8 * u.height);
                let top = u.top * k + (k - 1.0) * b;
                if reverse { content_bottom - (top + u.height * k) + (cy - fr[1]) } else { cy + top }
            }
        };
        groups.push(Child::Group { offset: (cx, y), children });
    }
    Ok(())
}

// Header and footer floats
pub fn hf_floats(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &super::float::Frames,
    top: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Vec<Child>, Yield> {
    let mut out = Vec::new();
    for (f, rect, para) in hf_float_rects(blocks, story, fr, top, env, memos)? {
        out.push(float_box(&f, rect, para, env, memos)?);
    }
    Ok(out)
}

pub fn hf_float_rects(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &super::float::Frames,
    top: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Vec<(Rc<Float>, [f64; 4], ParaId)>, Yield> {
    let mut out = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let Block::Para(p) = b else { continue };
        let Some(u) = story.units.iter().find(|u| u.block == bi) else { continue };
        let para_top = top + u.top - if bi == 0 { space_before(&env.ctx.env.text, p.para_style_in(env.ctx.env.text.review.view)) } else { 0.0 };
        for f in env.ctx.floats_of(p) {
            let sized = super::float::sized(f, fr);
            let f = sized.as_ref().unwrap_or(f);
            let fitted = fit_frame(f, fr.column.1, env, memos)?;
            let f = fitted.as_ref().unwrap_or(f);
            let h = float_height(f, env, memos)?;
            let (fx, fy) = (super::float::h_pos(f, fr), super::float::v_pos(f, h, fr, para_top, para_top));
            out.push((Rc::new(f.clone()), [fx, fy, fx + f.w, fy + h], para_id(p)));
        }
    }
    Ok(out)
}

pub fn fit_frame(f: &Float, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<Option<Float>, Yield> {
    let no_wrap = f.shape.as_deref().and_then(|s| s.sp()).is_some_and(|s| s.body.no_wrap) && !f.rot_swap;
    if let Some(tb) = f.text_box.as_ref().filter(|_| no_wrap) {
        let fr = inner_frame(tb, f.shape.as_deref(), [0.0, 0.0, f.w, f.h]);
        let longest = longest_line(tb, 1584.0, env, memos)?;
        return Ok(Some(Float { w: (f.w - (fr[2] - fr[0]) + longest).min(width.max(f.w)), ..f.clone() }));
    }
    let Some(tb) = f.text_box.as_ref().filter(|_| f.auto_w) else { return Ok(None) };
    let w = longest_line(tb, width, env, memos)?;
    Ok(Some(Float { w: w.min(width) + tb.insets[0] + tb.insets[2], ..f.clone() }))
}

fn longest_line(tb: &crate::docx::TextBox, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<f64, Yield> {
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
    Ok(w)
}

// Header and footer height
pub fn hf_limit(compat: Compat, page_h: f64) -> f64 {
    if compat == Compat::Legacy { page_h / 2.0 } else { f64::INFINITY }
}

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
