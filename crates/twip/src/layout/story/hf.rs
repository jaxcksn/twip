//! Headers and footers

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::{Block, Float};
use crate::layout::float::Placed;
use crate::layout::fragment::Child;
use crate::layout::ids::ParaId;
use crate::layout::job::{Memos, Yield};
use crate::layout::text::para_id;
use crate::layout::units::EPS;

use super::{StoryEnv, StoryLayout, StoryUnit, boxes::fit_frame, boxes::float_box, child_ids, float_height, layout_in, layout_with, stack_limited};

// Header and footer floats
pub fn hf_floats(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &crate::layout::float::Frames,
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

/// A header or footer story laid out around its own wrapping floats. `top` gives the story's page y from its
/// layout (a footer grows upward), `fr` the page the floats are placed on. Compat ≤ 14: the text runs over them.
pub fn hf_layout(
    blocks: &[&Block],
    width: f64,
    fr: &crate::layout::float::Frames,
    top: impl Fn(&StoryLayout) -> f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Rc<StoryLayout>, Yield> {
    let margin = fr.margin[2];
    let mut l = layout_in(blocks, width, env, None, Some((0.0, margin)), memos)?;
    // A table placed from the page is placed again once the story's top is known.
    let placed = |b: &&Block| matches!(b, Block::Table(t) if t.floating.is_some_and(|f| f.v_anchor != crate::docx::props::TableAnchor::Text));
    if blocks.iter().any(placed) && top(&l) != 0.0 {
        l = layout_in(blocks, width, env, None, Some((top(&l), margin)), memos)?;
    }
    // ponytail: compat ≤ 14 seen only with empty paragraphs under a picture as wide as the page; text beside a
    // smaller float is unverified.
    if env.ctx.env.text.compat == Compat::Legacy || !blocks.iter().any(|b| matches!(b, Block::Para(p) if !env.ctx.floats_of(p).is_empty())) {
        return Ok(l);
    }
    let laid = crate::layout::join::laid_in(blocks, env.ctx.env.text.review.view, env.ctx.env.normal.as_ref());
    let children = child_ids(&laid, Some(width), env, None, memos)?;
    let mut seen: Option<Vec<[f64; 4]>> = None;
    let mut frozen = Vec::new();
    // ponytail: a few passes for floats placed from the story's own text, no convergence test beyond them.
    for _ in 0..3 {
        let at = top(&l);
        let mut around = Vec::new();
        for (i, (key, f, r, para)) in float_rects(blocks, &l, fr, at, env, memos)?.into_iter().enumerate() {
            let p = Placed::new((usize::MAX, 0, i), para, &f, r[0] - fr.column.0, r[1] - at, r[3] - r[1]);
            if p.wrap == crate::layout::float::Wrap::None {
                continue;
            }
            // A float that meets the story's lines where it is placed keeps that place on the page in the passes
            // after, even when its anchor moves.
            // ponytail: a line meets it when their bands overlap and it reaches into the story's width.
            let line = |u: &StoryUnit| u.content.iter().any(|c| matches!(c, Child::Line { .. })) && p.in_band(u.top, u.top + u.height);
            let meets = p.excl[2] > EPS && p.excl[0] < width - EPS && l.units.iter().any(line);
            if meets && frozen.iter().all(|(k, _)| *k != key) {
                frozen.push((key, r));
            }
            around.push(p);
        }
        let rects: Vec<[f64; 4]> = around.iter().map(|p| p.rect).collect();
        if around.is_empty() || seen.as_ref() == Some(&rects) {
            break;
        }
        l = Rc::new(StoryLayout {
            frozen: frozen.clone(),
            ..layout_with(&laid, &children, width, env, None, (0, 0), &around, Some((at, margin)), memos)?
        });
        seen = Some(rects);
    }
    Ok(l)
}

/// A footer laid out for the page: the layout drawn, the page y it is drawn from and the height the body gives way
/// to, the story growing up from `foot` (the page height less the footer distance), each limited to `limit`. Compat 15
/// with its text wrapping its own floats: laid out first from the bottom margin line, which height sets the body's
/// bottom, then again from where that height puts it, and drawn laid out from where that second height puts it.
pub fn footer_layout(
    blocks: &[&Block],
    width: f64,
    fr: &crate::layout::float::Frames,
    foot: f64,
    limit: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<(Rc<StoryLayout>, f64, f64), Yield> {
    let height = |l: &StoryLayout| stack_limited(l, 0.0, 0.0, limit).1;
    if env.ctx.env.text.compat == Compat::Legacy || !blocks.iter().any(|b| matches!(b, Block::Para(p) if !env.ctx.floats_of(p).is_empty())) {
        let l = hf_layout(blocks, width, fr, |l| foot - height(l), env, memos)?;
        let h = height(&l);
        return Ok((l, foot - h, h));
    }
    let margin_line = fr.page_h - fr.margin[3].abs();
    let first = height(&*hf_layout(blocks, width, fr, |_| margin_line, env, memos)?);
    let second = height(&*hf_layout(blocks, width, fr, |_| foot - first, env, memos)?);
    let l = hf_layout(blocks, width, fr, |_| foot - second, env, memos)?;
    Ok((l, foot - second, first))
}

pub fn hf_float_rects(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &crate::layout::float::Frames,
    top: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Vec<(Rc<Float>, [f64; 4], ParaId)>, Yield> {
    Ok(float_rects(blocks, story, fr, top, env, memos)?.into_iter().map(|(_, f, r, p)| (f, r, p)).collect())
}

/// The story's floats on the page, each with its block and its place among the paragraph's floats.
#[allow(clippy::type_complexity, reason = "the float's key, the float, its rect and anchor")]
fn float_rects(
    blocks: &[&Block],
    story: &StoryLayout,
    fr: &crate::layout::float::Frames,
    top: f64,
    env: &StoryEnv,
    memos: &mut Memos,
) -> Result<Vec<((usize, usize), Rc<Float>, [f64; 4], ParaId)>, Yield> {
    let mut out = Vec::new();
    for (bi, b) in blocks.iter().enumerate() {
        let Block::Para(p) = b else { continue };
        let Some(u) = story.units.iter().find(|u| u.block == bi) else { continue };
        let para_top = top + story.origins.iter().find(|o| o.0 == bi).map_or(u.top, |o| o.1);
        for (k, f) in env.ctx.floats_of(p).iter().enumerate() {
            let sized = crate::layout::float::sized(f, fr);
            let f = sized.as_ref().unwrap_or(f);
            let fitted = fit_frame(f, fr.column.1, env, memos)?;
            let f = fitted.as_ref().unwrap_or(f);
            let h = float_height(f, env, memos)?;
            let (fx, fy) = (crate::layout::float::h_pos(f, fr), crate::layout::float::v_pos(f, h, fr, para_top, para_top));
            let (fx, fy) = crate::layout::float::page_clamp(f, (fx, fy), h, fr);
            let rect = match story.frozen.iter().find(|(key, _)| *key == (bi, k)) {
                Some((_, r)) => *r,
                None => [fx, fy, fx + f.w, fy + h],
            };
            out.push(((bi, k), Rc::new(f.clone()), rect, para_id(p)));
        }
    }
    Ok(out)
}

// Header and footer height
pub fn hf_limit(compat: Compat, page_h: f64) -> f64 {
    if compat == Compat::Legacy { page_h / 2.0 } else { f64::INFINITY }
}
