//! Shapes and text boxes

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::{Block, Float};
use crate::layout::LayoutCtx;
use crate::layout::env::TextEnv;
use crate::layout::fragment::{BoxFragment, BoxKind, Child, ImageKey, Item, Layer, PictureOf, ShapeItem};
use crate::layout::ids::{FloatId, ParaId};
use crate::layout::job::{Memos, Yield};
use crate::layout::join::LaidBlock;
use crate::layout::line::break_lines;
use crate::layout::text::{FieldValues, Kind, shape};
use crate::layout::units::{EPS, Lu};

use super::{StoryEnv, StoryLayout, layout, layout_from};

// Float drawing
pub fn float_box(f: &Float, rect: [f64; 4], anchor: ParaId, env: &StoryEnv, memos: &mut Memos) -> Result<Child, Yield> {
    let [x0, y0, x1, y1] = rect;
    let shape_rect = if f.rot_swap {
        let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
        [cx - hw, cy - hh, cx + hw, cy + hh]
    } else if let Some((w, h)) = f.relative.drawn {
        [x0, y0, x0 + w, y0 + h]
    } else {
        rect
    };
    let mut items = Vec::new();
    let mut groups = Vec::new();
    match f.shape.as_deref() {
        Some(shape) => {
            let t = &env.ctx.env.text;
            let palette = crate::drawing::paint::Palette { theme: &t.theme, map: &t.colour_map };
            items.extend(
                shape_items(shape, shape_rect, f.id, f.behind, palette, None, PictureOf::Float(FloatId(f.id)), env.ctx.fonts)
                    .into_iter()
                    .map(Item::Shape),
            );
        }
        None => {
            if let Some(img) = f.image {
                items.push(Item::Image { rect: shape_rect, key: ImageKey { index: Some(img), generation: 0 }, of: PictureOf::Float(FloatId(f.id)) });
            }
        }
    }
    let chain = env.ctx.env.chain_of(f.id);
    // The text groups each leaf of the shape owns, by its path in the shape.
    let mut owned: Vec<(&crate::drawing::ChildPath, std::ops::Range<usize>)> = Vec::new();
    let no_path = crate::drawing::ChildPath::new();
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
    owned.push((&no_path, 0..groups.len()));
    let leaves = f.shape.as_deref().map(|s| s.leaves(shape_rect)).unwrap_or_default();
    for part in &f.parts {
        let r = [x0 + part.rect[0], y0 + part.rect[1], x0 + part.rect[2], y0 + part.rect[3]];
        if f.shape.is_none()
            && let Some(img) = part.image
        {
            items.push(Item::Image { rect: r, key: ImageKey { index: Some(img), generation: 0 }, of: PictureOf::Float(FloatId(f.id)) });
        }
        if let Some(tb) = &part.text_box {
            let from = groups.len();
            match leaves.iter().find(|l| l.path == part.path) {
                Some(l) => turned_text(tb, Some(l.shape), l.rect, l.rot, l.flip_v, env, &mut groups, memos)?,
                None => text_box(tb, f.shape.as_deref().and_then(|s| s.at(&part.path)), r, env, &mut groups, memos)?,
            }
            owned.push((&part.path, from..groups.len()));
        }
    }
    // A leaf's reflection holds the text of that leaf alone, not its siblings'.
    for item in &mut items {
        let Item::Shape(s) = item else { continue };
        let Some(s) = Rc::get_mut(s).filter(|s| s.effects.reflection.is_some() && !s.picture) else { continue };
        let own: Vec<_> = owned
            .iter()
            .filter(|(path, _)| **path == s.path)
            .flat_map(|(_, range)| crate::layout::traverse::clipped_items_of(&groups[range.clone()]))
            .filter(|(i, _)| matches!(i, Item::Glyphs(_) | Item::Turned(_)))
            .collect();
        s.text = (!own.is_empty()).then(|| crate::layout::fragment::ShapeText(own.into()));
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
fn chain_box(f: &Float, theme: &crate::drawing::theme::Theme) -> crate::docx::TextBox {
    if let Some(tb) = &f.text_box {
        return crate::docx::TextBox { blocks: Vec::new(), ..tb.clone() };
    }
    let sp = f.shape.as_deref().and_then(|s| s.sp());
    let pt = |e: crate::drawing::units::Emu| e.0 as f64 / 12700.0;
    let insets = sp.map_or([7.2, 3.6, 7.2, 3.6], |s| s.body.insets.map(pt));
    // Without its own width the outline takes the width of the theme's line its style refers to.
    let styled =
        sp.and_then(|s| s.style.as_ref()?.line.as_ref()).and_then(|(idx, _)| crate::drawing::paint::style_entry(&theme.lines, None, *idx)?.width);
    let own = sp.and_then(|s| s.line.as_ref());
    let outline = match own.and_then(|l| l.width).or(styled) {
        Some(w) => 2.0 * if own.and_then(|l| l.inset) == Some(true) { w.0 } else { w.0 / 2 } as f64 / 12700.0,
        None => 0.0,
    };
    crate::docx::TextBox { blocks: Vec::new(), insets, outline, auto_fit: false, v_anchor: 0, flow: 0 }
}

pub(in crate::layout) fn text_frame_size(f: &Float, theme: &crate::drawing::theme::Theme) -> [f64; 2] {
    let (w, h) = if f.rot_swap { (f.h, f.w) } else { (f.w, f.h) };
    let fr = layout_frame(&chain_box(f, theme), f.shape.as_deref(), [0.0, 0.0, w, h]);
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
    c: &crate::layout::env::Chain,
    k: usize,
    f: &Float,
    rect: [f64; 4],
    env: &StoryEnv,
    groups: &mut Vec<Child>,
    memos: &mut Memos,
) -> Result<(), Yield> {
    let blocks: Vec<&Block> = c.story.blocks.iter().collect();
    let mut story = layout(&blocks, c.boxes[0].1[0].max(1.0), env, None, memos)?;
    let heights: Vec<f64> = c.boxes.iter().map(|b| b.1[1]).collect();
    let widow = env.ctx.env.text.compat != Compat::Legacy;
    let range = if c.boxes.iter().all(|b| (b.1[0] - c.boxes[0].1[0]).abs() < EPS) {
        let Some(range) = chain_slices(&story, &heights, widow).get(k).cloned() else { return Ok(()) };
        range
    } else {
        // Boxes of different widths: each lays out the rest of the story at its own width, from where the box
        // before it ended.
        for j in 0..k {
            let end = chain_slices(&story, &heights[j..j + 2], widow)[0].end;
            let Some(u) = story.units.get(end) else { return Ok(()) };
            let at = (u.block, u.from);
            story = Rc::new(layout_from(&blocks, c.boxes[j + 1].1[0].max(1.0), env, at, memos)?);
        }
        let last = k + 1 == heights.len();
        if last { 0..story.units.len() } else { chain_slices(&story, &heights[k..k + 2], widow)[0].clone() }
    };
    let fr = layout_frame(&chain_box(f, &env.ctx.env.text.theme), f.shape.as_deref(), rect);
    let base = story.units.get(range.start).map_or(0.0, |u| u.top);
    for u in &story.units[range] {
        let y = fr[1] + u.top - base;
        if y >= fr[3] - EPS {
            continue;
        }
        let bottom = fr[3] - y + crate::layout::table::INK_SLACK;
        let children = crate::layout::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom).unwrap_or_else(|| u.content.clone());
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
pub(in crate::layout) fn shape_items(
    shape: &crate::drawing::Shape,
    rect: [f64; 4],
    float: u64,
    behind: bool,
    palette: crate::drawing::paint::Palette,
    clip_bottom: Option<f64>,
    of: crate::layout::fragment::PictureOf,
    fonts: &crate::font::Fonts,
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
        // A text path draws its glyphs in place of the outline, or nothing; the box stays its geometry, for hits.
        let text_path = match leaf.shape.sp().and_then(|sp| sp.text_path.as_deref()) {
            Some(tp) => match text_path(tp, [lx1 - lx0, ly1 - ly0], fonts) {
                Some(t) => Some(t),
                None => continue,
            },
            None => None,
        };
        let geom = if text_path.is_some() { crate::drawing::geom::Geometry::default().evaluate(w, h) } else { geom };
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
            ink: false,
            decoration: false,
            snap: false,
            clip_bottom,
            text_float: 0,
            text: None,
            text_path,
            path: leaf.path,
            of,
        }));
    }
    out
}

/// The most glyphs a text path draws.
const MAX_TEXT_PATH_GLYPHS: usize = 4096;

/// A text path's glyphs, shaped in its face (characters it lacks in their fallback faces), and the box in the
/// shape's own (`size`) their ink fills: the whole box when trimmed, else inset 7.2 points across and 3.6 down, as
/// Word draws it. Word draws no text that isn't fitted to the shape, nor an empty string.
fn text_path(tp: &crate::drawing::TextPath, size: [f64; 2], fonts: &crate::font::Fonts) -> Option<Rc<crate::layout::fragment::TextPathItem>> {
    if !tp.fit_shape || tp.string.trim().is_empty() {
        return None;
    }
    let face = fonts.face(&tp.family, None, tp.bold, tp.italic)?;
    let face_of = |c: char| {
        if face.has_glyph(c) || c.is_whitespace() {
            Some(Rc::clone(&face))
        } else {
            fonts.fallback(c, tp.bold, tp.italic).or_else(|| Some(Rc::clone(&face)))
        }
    };
    let mut runs: Vec<(Rc<crate::font::Face>, Box<[(u32, f64)]>)> = Vec::new();
    let (mut pen, mut count) = (0.0, 0);
    let mut chars = tp.string.chars().map(|c| if c.is_control() { ' ' } else { c }).peekable();
    let mut text = String::new();
    while let Some(c) = chars.next() {
        let Some(run_face) = face_of(c) else { break };
        text.clear();
        text.push(c);
        while let Some(&n) = chars.peek() {
            if !face_of(n).is_some_and(|f| Rc::ptr_eq(&f, &run_face)) {
                break;
            }
            text.push(n);
            chars.next();
        }
        let em = 1.0 / run_face.upem;
        let mut glyphs = Vec::new();
        for g in run_face.shape(&text, true, 0, 0).into_iter().take(MAX_TEXT_PATH_GLYPHS - count) {
            glyphs.push((g.id, pen + g.dx * em));
            pen += g.advance * em;
        }
        count += glyphs.len();
        runs.push((run_face, glyphs.into()));
        if count >= MAX_TEXT_PATH_GLYPHS {
            break;
        }
    }
    let [w, h] = size;
    let fit = if tp.trim { [0.0, 0.0, w, h] } else { [7.2, 3.6, w - 7.2, h - 3.6] };
    Some(Rc::new(crate::layout::fragment::TextPathItem { runs: runs.into(), fit }))
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
    if shape.and_then(crate::drawing::Shape::geometry).is_some_and(|g| !g.holds_text()) {
        return Ok(());
    }
    let (dark_text, dark_env);
    let env = match shape.and_then(|s| solid_fill(s, env)).filter(|c| crate::layout::env::is_dark(*c)) {
        Some(fill) => {
            dark_text = crate::layout::LayoutEnv {
                text: crate::layout::env::TextEnv { backdrop: Some(fill), ..env.ctx.env.text.clone() },
                ..env.ctx.env.clone()
            };
            dark_env = StoryEnv { ctx: LayoutCtx { env: &dark_text, ..env.ctx }, ..*env };
            &dark_env
        }
        None => env,
    };
    let body = shape.and_then(|s| s.sp()).map(|s| &s.body);
    let turn = (deg + body.map_or(0.0, |b| b.rot.degrees()) + if flip_v { 180.0 } else { 0.0 }).rem_euclid(360.0);
    let mut frame = layout_frame(tb, shape, rect);
    // A frame's own text flow, or a text box's: lines along the frame's height, stacked across its width.
    if let Some(v) = text_flow(tb, shape) {
        let blocks: Vec<&Block> = tb.blocks.iter().collect();
        let story = crate::layout::vertical::story(&blocks, frame[3] - frame[1], v, env, None, memos)?;
        let width = frame[2] - frame[0];
        // The anchor moves the block across, towards the side the lines stack to.
        let across = if body.is_some() && !tb.auto_fit && tb.v_anchor > 0 {
            let free = Lu::round_half_up(width - story.height).0.max(0);
            Lu(if tb.v_anchor == 2 { free } else { free / 2 }).to_pt()
        } else {
            0.0
        };
        let clip = body.is_none() || !tb.auto_fit;
        let bottom = if v.flow == crate::layout::vertical::Flow::DownEast || !clip { f64::INFINITY } else { width + crate::layout::table::INK_SLACK };
        let items = crate::layout::vertical::turned(&story.units, v, frame, across, bottom);
        let items = if turn.abs() < 1e-9 {
            items
        } else {
            let centre = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
            vec![Item::Turned(Rc::new(crate::layout::fragment::Turned { centre, deg: turn, items }))]
        };
        groups.push(Child::Items(items.into()));
        return Ok(());
    }
    use crate::drawing::text::TextDirection as D;
    let dir = body.map_or(D::Horizontal, |b| b.direction);
    if dir != D::Horizontal {
        let [x0, y0, x1, y1] = frame;
        let (fx, fy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
        let quarter = [fx - hw, fy - hh, fx + hw, fy + hh];
        let reverse = dir == D::MongolianVert;
        let mut lines = Vec::new();
        lay_lines(tb, quarter, Some((VERTICAL_PITCH, reverse)), env, &mut lines, memos)?;
        let inner =
            Item::Turned(Rc::new(crate::layout::fragment::Turned { centre: [fx, fy], deg: 90.0, items: crate::layout::traverse::items_of(&lines) }));
        let item = if turn.abs() < 1e-9 {
            inner
        } else {
            let centre = [(rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0];
            Item::Turned(Rc::new(crate::layout::fragment::Turned { centre, deg: turn, items: vec![inner] }))
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
    let items = crate::layout::traverse::items_of(&lines);
    groups.push(Child::Items(vec![Item::Turned(Rc::new(crate::layout::fragment::Turned { centre, deg: turn, items }))].into()));
    Ok(())
}

// Text boxes
/// The way a frame's or text box's lines run, `None` horizontal.
pub(super) fn text_flow(tb: &crate::docx::TextBox, shape: Option<&crate::drawing::Shape>) -> Option<crate::layout::vertical::Vertical> {
    match shape.and_then(|s| s.sp()) {
        None => crate::layout::vertical::of_cell(tb.flow),
        Some(s) => crate::layout::vertical::of_text_box(s.body.direction),
    }
}

/// An unturned text box's story and its vertical flow.
pub(super) fn vertical_box(f: &Float) -> Option<(&crate::docx::TextBox, crate::layout::vertical::Vertical)> {
    let tb = f.text_box.as_ref().filter(|_| f.shape.is_some() && !f.rot_swap)?;
    Some((tb, text_flow(tb, f.shape.as_deref())?))
}

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

pub(super) fn inner_frame(tb: &crate::docx::TextBox, shape: Option<&crate::drawing::Shape>, rect: [f64; 4]) -> [f64; 4] {
    let g = text_inset(shape, rect[2] - rect[0], rect[3] - rect[1]);
    let h = tb.outline / 2.0;
    let x0 = rect[0] + g[0] + tb.insets[0] + h;
    let y0 = rect[1] + g[1] + tb.insets[1] + h;
    let x1 = (rect[2] - g[2] - tb.insets[2] - h).max(x0);
    let y1 = (rect[3] - g[3] - tb.insets[3] - h).max(y0);
    [x0, y0, x1, y1]
}

/// The environment of a text box's story: compat ≤ 14 marks it as one (`TextEnv::box_story`).
pub(super) fn box_env<'b>(env: &StoryEnv<'b>, le: &'b mut Option<crate::layout::LayoutEnv>) -> StoryEnv<'b> {
    let text = &env.ctx.env.text;
    if text.compat != Compat::Legacy || text.box_story {
        return *env;
    }
    let le = le.insert(crate::layout::LayoutEnv { text: TextEnv { box_story: true, ..text.clone() }, ..env.ctx.env.clone() });
    StoryEnv { ctx: LayoutCtx { env: le, ..env.ctx }, ..*env }
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
    let mut le = None;
    let story = layout(&blocks, w, &box_env(env, &mut le), None, memos)?;
    let (cx, mut cy) = (fr[0], fr[1]);
    let content_bottom = fr[3];
    if !tb.auto_fit && tb.v_anchor > 0 {
        let free = Lu::round_half_up(content_bottom - cy - story.height).0.max(0);
        cy += Lu(if tb.v_anchor == 2 { free } else { free / 2 }).to_pt();
    }
    for u in &story.units {
        if !tb.auto_fit && cy + u.top >= content_bottom - EPS {
            continue;
        }
        let bottom = if tb.auto_fit { f64::INFINITY } else { content_bottom - (cy + u.top) + crate::layout::table::INK_SLACK };
        let children = crate::layout::table::clip_x(&u.content, f64::NEG_INFINITY, f64::INFINITY, bottom).unwrap_or_else(|| u.content.clone());
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

pub fn fit_frame(f: &Float, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<Option<Float>, Yield> {
    let no_wrap = f.shape.as_deref().and_then(|s| s.sp()).is_some_and(|s| s.body.no_wrap) && !f.rot_swap;
    if let Some(tb) = f.text_box.as_ref().filter(|_| no_wrap) {
        let fr = inner_frame(tb, f.shape.as_deref(), [0.0, 0.0, f.w, f.h]);
        let (longest, _) = longest_line(tb, 1584.0, env, memos)?;
        let tables = tb.blocks.iter().filter_map(|b| if let Block::Table(t) = b { Some(table_extent(t, width, env)) } else { None });
        let longest = tables.fold(longest, f64::max);
        return Ok(Some(Float { w: (f.w - (fr[2] - fr[0]) + longest).min(width.max(f.w)), ..f.clone() }));
    }
    // A text box that fits its vertical text: the lines stack across, so the width follows them.
    if let Some((tb, v)) = vertical_box(f).filter(|(tb, _)| tb.auto_fit) {
        let fr = inner_frame(tb, f.shape.as_deref(), [0.0, 0.0, f.w, f.h]);
        let blocks: Vec<&Block> = tb.blocks.iter().collect();
        let fields = FieldValues::default();
        let story = crate::layout::vertical::story(&blocks, fr[3] - fr[1], v, &StoryEnv { fields: &fields, ..*env }, None, memos)?;
        return Ok(Some(Float { w: f.w - (fr[2] - fr[0]) + story.height.min(1584.0), ..f.clone() }));
    }
    let Some(tb) = f.text_box.as_ref().filter(|_| f.auto_w) else { return Ok(None) };
    // A frame without a width is as wide as its content only when that is one line; two or more lines make it as
    // wide as its column.
    let (w, lines) = longest_line(tb, width, env, memos)?;
    let w = if lines > 1 { width } else { w.min(width) };
    Ok(Some(Float { w: w + tb.insets[0] + tb.insets[2], ..f.clone() }))
}

/// The width a table reports to a box whose text doesn't wrap: from its first cell's text left to its last cell's
/// text right as its grid stores it, less half its outer side borders in compat 15. Compat ≤ 14 resolves a pct width
/// against the section's column.
fn table_extent(t: &crate::docx::Table, column: f64, env: &StoryEnv) -> f64 {
    let legacy = env.ctx.env.text.compat == Compat::Legacy;
    let grid: f64 = t.table_grid_in(env.ctx.env.text.review.view).iter().sum();
    let total = match t.width {
        crate::docx::props::Width::Pct(_) if legacy => match crate::layout::table::in_column(t, column, env).width {
            crate::docx::props::Width::Dxa(w) => w,
            _ => grid,
        },
        _ => grid,
    };
    let row = t.rows.first();
    let lm = row.and_then(|r| r.cells.first()).map_or(0.0, |c| c.margins[1]);
    let rm = row.and_then(|r| r.cells.last()).map_or(0.0, |c| c.margins[3]);
    let side = |b: &Option<crate::docx::props::Border>| b.as_ref().filter(|b| b.visible()).map_or(0.0, |b| b.width);
    let borders = if legacy { 0.0 } else { (side(&t.borders[1]) + side(&t.borders[3])) / 2.0 };
    total - lm - rm - borders
}

/// The right edge of the widest line, trailing spaces excluded, and the number of lines.
fn longest_line(tb: &crate::docx::TextBox, width: f64, env: &StoryEnv, memos: &mut Memos) -> Result<(f64, usize), Yield> {
    let view = env.ctx.env.text.review.view;
    let mut w: f64 = 0.0;
    let mut lines = 0;
    for b in
        crate::layout::join::laid_in(&tb.blocks, view, env.ctx.env.normal.as_ref(), env.ctx.env.text.compat == crate::docx::model::Compat::Legacy)
    {
        let LaidBlock::Para(laid) = b else { continue };
        let p = laid.para();
        let space = laid.shape_space(env.fields, env.notes, &env.ctx, memos)?;
        let shaped = shape(p, &space, env.ctx.fonts, memos)?;
        let ps = crate::layout::line::para_space(p.para_style_in(view), None, None, width, 0, 0.0, &env.ctx);
        let pb = break_lines(&shaped, p, &ps, env.ctx.fonts, memos)?;
        let pieces = pb.pieces();
        lines += pb.lines.len();
        for l in &pb.lines {
            for (k, i) in (l.start..l.end).enumerate() {
                if pieces[i].kind != Kind::Space {
                    w = w.max(l.xs[k] + l.widths[k]);
                }
            }
        }
    }
    Ok((w, lines))
}
