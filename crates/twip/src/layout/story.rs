//! Stories laid out without pagination (cells, headers and footers, notes, text boxes): one unit per line or table
//! row. The paragraph algorithms are the body's (HDR-3, TABLE-29).

use std::collections::HashMap;
use std::rc::Rc;

use crate::docx::{Block, Float, Inline, NoteKind, ParaStyle};

use super::LayoutCtx;
use super::cache::{CacheKey, cached};
use super::env::TextEnv;
use super::fragment::{BoxFragment, BoxKind, Child, ImageKey, Item, PlacedLine};
use super::ids::{FloatId, ParaId};
use super::job::{Memos, Yield};
use super::join::LaidBlock;
use super::line::{break_lines, neighbour_borders};
use super::paragraph::{contextual_in, gap, gap_plain, space_after, space_before};
use super::space::ParaSpace;
use super::table::TableLayout;
use super::text::{FieldValues, Kind, Object, ShapedPara, para_id, shape};

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
    /// Notes referenced on this line (NOTE-8: references in table cells place their notes like body lines).
    pub notes: Box<[(NoteKind, i64)]>,
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
pub fn child_ids(blocks: &[LaidBlock], width: Option<f64>, env: &StoryEnv, memos: &mut Memos) -> Result<Vec<ChildId>, Yield> {
    let mut out = Vec::with_capacity(blocks.len());
    for b in blocks {
        out.push(match b {
            LaidBlock::Para(p) => ChildId::Para(shape(p.para(), &p.shape_space(env.fields, env.notes, &env.ctx.env.text), env.ctx.fonts, memos)?),
            LaidBlock::Table(t) => ChildId::Table(match width {
                Some(w) => super::table::layout(t, w, env, None, false, memos)?,
                None => super::table::layout(t, 1e9, env, None, true, memos)?,
            }),
        });
    }
    Ok(out)
}

/// A cell story's ends (PARA-4): whether its first paragraph drops its space-before and its last its space-after,
/// against their neighbours in document order outside the cell.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct CellEnds {
    pub before: bool,
    pub after: bool,
}

/// What a story (a cell, header, footer, note, separator or text box), or a cell's content widths (`width` 0:
/// width-free), depends on: its children, the width, whether it is a cell and its ends (PARA-4), the
/// `TextEnv` and the font revision. Field values and note numbers are in the children's keys.
#[derive(Clone, Debug, PartialEq)]
pub struct StoryKey {
    children: Vec<ChildId>,
    width: u64,
    cell: Option<CellEnds>,
    text: TextEnv,
    fonts: u64,
}

impl StoryKey {
    pub fn new(children: Vec<ChildId>, width: Option<f64>, cell: Option<CellEnds>, env: &StoryEnv) -> StoryKey {
        StoryKey { children, width: width.map_or(0, f64::to_bits), cell, text: env.ctx.env.text.clone(), fonts: env.ctx.fonts.revision() }
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
    let children = child_ids(&blocks, Some(width), env, memos)?;
    let probe = StoryKey::new(children.clone(), Some(width), cell, env);
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
    for (bi, b) in blocks.iter().enumerate() {
        match b {
            LaidBlock::Para(laid) => {
                let p = laid.para();
                let next = match blocks.get(bi + 1) {
                    Some(LaidBlock::Para(n)) => Some(&**n.para().para_style_in(view)),
                    _ => None,
                };
                let shaped = match resolved.get(bi) {
                    Some(ChildId::Para(s)) => s.clone(),
                    _ => shape(p, &laid.shape_space(env.fields, env.notes, &env.ctx.env.text), env.ctx.fonts, memos)?,
                };
                let style = p.para_style_in(view);
                let ps = ParaSpace { width, start: 0, borders: neighbour_borders(style, prev, next), x0: 0.0, env: text.clone() };
                let lines = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
                let (mut before, mut after) = (space_before(text, style), space_after(text, style));
                // PARA-5: a cell's first paragraph takes no automatic space-before, its last no automatic space-after.
                if in_cell && style.before_auto && bi == 0 {
                    before = 0.0;
                }
                if in_cell && style.after_auto && bi + 1 == blocks.len() {
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
                for (i, line) in lines.lines.iter().enumerate() {
                    let placed = PlacedLine { x: 0.0, y: 0.0, line: lines.fragment(i, p, env.ctx.fonts) };
                    let mut content = vec![Child::Line { para: Some(id), placed }];
                    if in_cell {
                        // A float anchored in a cell, whatever its wrap: drawn from the anchor line, the cell's rows
                        // don't make room for it (FLOAT-14: positioned in the cell, `move_floats` places compat ≤ 14's
                        // layoutInCell="0" ones like the body's).
                        // shortcut: the cell's text doesn't wrap a wrapping float, so one that a line of text would
                        // have to wrap isn't drawn (below); `page` is taken as the cell's content box rather than the
                        // cell inside its borders.
                        let fr = super::float::Frames { page_w: width, page_h: 0.0, margin: [0.0; 4], column: (0.0, width) };
                        for k in line.start..line.end.max(line.start + 1) {
                            if lines.pieces().get(k).is_none_or(|q| q.kind != Kind::Anchor) {
                                continue;
                            }
                            let Some(Object::Float(f)) = lines.shaped.object(lines.from + k) else {
                                continue;
                            };
                            let h = float_height(f, env, memos)?;
                            let (fx, fy) = (super::float::h_pos(f, &fr), super::float::v_pos(f, h, &fr, para_top, y) - y);
                            if text.compat >= 15 || f.layout_in_cell {
                                float_bottom = float_bottom.max(y + fy + h);
                            }
                            if f.wrap != "none" {
                                wrapping.push((units.len(), content.len(), y + fy - f.dist[0], y + fy + h + f.dist[1]));
                            }
                            let anchor = super::join::piece_para(&lines.shaped, lines.from + k, id);
                            content.push(float_box(f, [fx, fy, fx + f.w, fy + h], anchor, env, memos)?);
                        }
                    }
                    units.push(StoryUnit {
                        top: y,
                        height: line.pitch,
                        below: line.below,
                        after: if i + 1 == lines.lines.len() { after } else { 0.0 },
                        content: content.into(),
                        block: bi,
                        widow: style.widow_control,
                        notes: lines.notes(i).into_boxed_slice(),
                    });
                    y += line.pitch;
                }
                prev = Some(style);
                prev_after = after;
            }
            LaidBlock::Table(t) => {
                y += prev_after; // TABLE-48: the space-after before a table applies to its border edge
                let (rows, h) = match resolved.get(bi) {
                    Some(ChildId::Table(tb)) => super::table::table_units(tb),
                    _ => super::table::units(t, width, env, memos)?,
                };
                // TABLE-25: in compat <= 14 a nested table's negative indent counts as 0.
                let shift = if in_cell && text.compat <= 14 { super::table::nested_shift(t) - t.ind.min(0.0) } else { 0.0 };
                for mut u in rows {
                    u.block = bi;
                    u.top += y;
                    if shift != 0.0 {
                        u.content = Rc::from([Child::Group { offset: (shift, 0.0), children: u.content }]);
                    }
                    units.push(u);
                }
                y += h;
                prev = None;
                prev_after = 0.0;
            }
        }
    }
    // PARA-4, TABLE-29: a cell's last paragraph against the paragraph after the cell.
    if !ends.after {
        y += prev_after;
    }
    unwrapped_floats(&mut units, &wrapping);
    Ok(StoryLayout { units, height: y, float_bottom })
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
        let o = tb.outline;
        let w = (f.w - tb.insets[0] - tb.insets[2] - o).max(1.0);
        let blocks: Vec<&Block> = tb.blocks.iter().collect();
        let fields = FieldValues::default();
        let story = layout(&blocks, w, &StoryEnv { fields: &fields, ..*env }, None, memos)?;
        let (cx, cy) = (x0 + tb.insets[0] + o / 2.0, y0 + tb.insets[1] + o / 2.0);
        let content_bottom = y1 - tb.insets[3] - o / 2.0;
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
        for i in &p.inlines {
            let Inline::Float(f, _) = i else { continue };
            let h = float_height(f, env, memos)?;
            let (fx, fy) = (super::float::h_pos(f, fr), super::float::v_pos(f, h, fr, para_top, para_top));
            out.push(float_box(f, [fx, fy, fx + f.w, fy + h], para_id(p), env, memos)?);
        }
    }
    Ok(out)
}

/// The header and footer height cap (HDR-8).
pub fn hf_limit(compat: i64, page_h: f64) -> f64 {
    if compat <= 14 { page_h / 2.0 } else { f64::INFINITY }
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
