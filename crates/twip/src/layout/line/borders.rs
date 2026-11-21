//! Paragraph borders

use std::rc::Rc;

use crate::docx::model::*;
use crate::docx::props::Border;
use crate::layout::fragment::Item;
use crate::layout::space::{BorderGroup, BorderKey};
use crate::layout::text::Kind;

use super::Line;

// Paragraph borders
pub fn border_space(b: &Border) -> f64 {
    f64::from((b.space.max(0.0) as u32) & 31)
}

fn masked(b: &Option<Border>) -> Option<Border> {
    b.as_ref().map(|b| Border { space: border_space(b), ..b.clone() })
}

fn group(s: &ParaStyle) -> BorderGroup {
    BorderGroup {
        sides: [masked(&s.border_top), masked(&s.border_left), masked(&s.border_bottom), masked(&s.border_right)],
        between: masked(&s.border_between),
        edges: (s.ind_left + s.ind_first.min(0.0), s.ind_right),
        frame: s.frame.clone(),
        div: s.div,
        continued: false,
    }
}

pub fn border_key(s: &ParaStyle) -> BorderKey {
    let any = [&s.border_top, &s.border_left, &s.border_bottom, &s.border_right, &s.border_between].iter().any(|b| b.is_some());
    any.then(|| Rc::new(group(s)))
}

fn shaded(s: &ParaStyle) -> bool {
    s.shading.is_some() || s.shd.as_ref().is_some_and(|d| d.hatch().is_some())
}

/// As Word draws it: a shaded paragraph's shading runs on over the space to the next paragraph (its space after and
/// the next one's before) when the next is shaded too, with the same indents and borders, whatever its fill; also at
/// the foot of a page. `contextual`: the space as [`crate::layout::paragraph::gap`] finds it, else the plain sum.
pub fn shade_gap(env: &crate::layout::env::TextEnv, s: &ParaStyle, next: Option<&ParaStyle>, contextual: bool) -> f64 {
    use crate::layout::paragraph::{gap, gap_plain, space_after, space_before};
    match next.filter(|n| !n.page_break_before && shaded(s) && shaded(n) && group(s).joins(&group(n))) {
        Some(n) if contextual => gap(env, s, space_after(env, s), n, space_before(env, n)).max(0.0),
        Some(n) => gap_plain(env, space_after(env, s), space_before(env, n)).max(0.0),
        None => 0.0,
    }
}

/// The key of a paragraph's part continued from the page or column before.
pub fn continued_key(s: &ParaStyle) -> BorderKey {
    border_key(s).map(|k| Rc::new(BorderGroup { continued: true, ..(*k).clone() }))
}

pub fn neighbour_borders(style: &ParaStyle, prev: Option<&ParaStyle>, next: Option<&ParaStyle>) -> (BorderKey, BorderKey) {
    match border_key(style) {
        None => (None, None),
        Some(_) => (prev.filter(|_| !style.page_break_before).and_then(border_key), next.filter(|n| !n.page_break_before).and_then(border_key)),
    }
}

fn joined(key: &BorderKey, other: Option<&BorderGroup>) -> bool {
    matches!((key, other), (Some(k), Some(o)) if k.joins(o))
}

fn shadowed(b: &Border) -> f64 {
    if b.shadow { crate::layout::table::thick(b) } else { 0.0 }
}

fn between_gap(prev: &BorderGroup) -> f64 {
    if prev.continued {
        return 0.0;
    }
    prev.between.as_ref().map_or(0.0, |b| crate::layout::table::thick(b) + b.space + shadowed(b))
}

pub fn top_border_extra(s: &ParaStyle, prev: Option<&BorderGroup>, first_part: bool) -> f64 {
    let key = border_key(s);
    let own = match (prev, &s.border_top) {
        (Some(p), _) if joined(&key, Some(p)) => between_gap(p),
        (_, Some(t)) => crate::layout::table::thick(t) + border_space(t) + shadowed(t),
        _ => 0.0,
    };
    own + if first_part { div_tops(s).1 } else { 0.0 }
}

fn div_tops(s: &ParaStyle) -> (Vec<f64>, f64) {
    let mut y = 0.0;
    let mut at = Vec::new();
    for d in s.div_boxes.iter() {
        at.push(y);
        if let (true, Some(t)) = (d.starts, &d.borders[0]) {
            y += crate::layout::table::thick(t) + border_space(t) + shadowed(t);
        }
    }
    (at, y)
}

#[allow(clippy::too_many_arguments, reason = "the paragraph part's place in its border group and its division boxes")]
pub fn apply_borders(
    lines: &mut [Line],
    s: &ParaStyle,
    prev: Option<&BorderGroup>,
    next: Option<&BorderGroup>,
    width: f64,
    top_pitched: bool,
    first_part: bool,
    last_part: bool,
    shade_gap: f64,
    tops: &[f64],
) {
    let key = border_key(s);
    let n = lines.len();
    let hatch = s.shd.as_ref().and_then(crate::docx::props::Shd::hatch);
    if n == 0 || (key.is_none() && s.shading.is_none() && hatch.is_none() && s.div_boxes.is_empty()) {
        return;
    }
    let (div_at, div_top) = if first_part { div_tops(s) } else { (vec![0.0; s.div_boxes.len()], 0.0) };
    let lead =
        usize::from(first_part && n > 1 && matches!(lines[0].hard, Some(Kind::PageBreak | Kind::ColumnBreak)) && lines[0].end <= lines[0].start + 1);
    let lines = &mut lines[lead..];
    let tops = tops.get(lead..).unwrap_or(&[]);
    let n = lines.len();
    let same_prev = joined(&key, prev);
    let same_next = joined(&key, next);
    let thick = crate::layout::table::thick;
    let lx = s.border_left.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
    let rx = s.border_right.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
    let left = s.ind_left + s.ind_first.min(0.0);
    let (x0, x1) = (left - lx, width - s.ind_right + rx);
    // Each edge draws as a table's does: every border style, its stripes and tones.
    let edge = |line: &mut Line, b: &Border, horizontal: bool, far: bool, a: f64, from: f64, to: f64, phase: f64| {
        let w = thick(b);
        let r = if horizontal { [from, a, to, a + w] } else { [a, from, a + w, to] };
        let mut items = Vec::new();
        crate::layout::table::edge_from(&mut items, r, horizontal, far, b, phase);
        for i in items {
            match i {
                Item::Rect { rect, colour, snap, dash } => line.rects.push((rect, colour, snap, dash)),
                Item::Shape(s) => line.strokes.push(s),
                _ => {}
            }
        }
    };
    // A shadowed far edge's shadow: black rects beside it.
    let cast = |line: &mut Line, b: &Border, horizontal: bool, r: [f64; 4], lead: bool, trail: bool| {
        let mut items = Vec::new();
        crate::layout::table::shadow(&mut items, r, horizontal, true, b, lead, trail);
        for i in items {
            if let Item::Rect { rect, colour, snap, dash } = i {
                line.rects.push((rect, colour, snap, dash));
            }
        }
    };
    let (mut shade_top, mut shade_bottom) = (0.0, None);
    let top_extra = top_border_extra(s, prev, first_part);
    let div_x = |d: &crate::docx::model::DivBox| {
        let side = |b: &Option<Border>| b.as_ref().map_or(0.0, |b| border_space(b) + thick(b));
        (d.left - side(&d.borders[1]), width - d.right + side(&d.borders[3]))
    };
    for (d, &y) in s.div_boxes.iter().zip(&div_at) {
        if let (true, true, Some(t)) = (first_part, d.starts, &d.borders[0]) {
            let (a, b) = div_x(d);
            edge(&mut lines[0], t, true, false, y, a, b, 0.0);
        }
    }
    match prev {
        Some(p) if same_prev => {
            if let Some(b) = p.between.as_ref().filter(|_| !p.continued) {
                let y = div_top + shadowed(b);
                edge(&mut lines[0], b, true, false, y, x0, x1, 0.0);
            }
        }
        _ => {
            if let Some(t) = &s.border_top {
                edge(&mut lines[0], t, true, false, div_top + shadowed(t), x0, x1, 0.0);
                shade_top = div_top + shadowed(t) + thick(t);
            }
        }
    }
    if !top_pitched && top_extra > 0.0 {
        let l = &mut lines[0];
        l.pitch += top_extra;
        l.baseline += top_extra;
    }
    if let (Some(b), false) = (&s.border_bottom, same_next) {
        let l = &mut lines[n - 1];
        let y = l.pitch + border_space(b);
        edge(&mut *l, b, true, true, y, x0, x1, 0.0);
        cast(&mut *l, b, true, [x0, y, x1, y + thick(b)], s.border_left.is_some(), s.border_right.is_some());
        shade_bottom = Some(y);
        l.pitch += border_space(b) + thick(b) + shadowed(b);
    }
    if let Some(b) = next.filter(|_| same_next && last_part).and_then(|k| k.between.as_ref()) {
        lines[n - 1].pitch += border_space(b);
    }
    let mut div_bottom = vec![None; s.div_boxes.len()];
    if last_part {
        let l = &mut lines[n - 1];
        for (k, d) in s.div_boxes.iter().enumerate().rev() {
            if let (true, Some(b)) = (d.ends, &d.borders[2]) {
                let y = l.pitch + border_space(b);
                let (a, c) = div_x(d);
                edge(&mut *l, b, true, true, y, a, c, 0.0);
                cast(&mut *l, b, true, [a, y, c, y + thick(b)], d.borders[1].is_some(), d.borders[3].is_some());
                l.pitch = y + thick(b) + shadowed(b);
                div_bottom[k] = Some(l.pitch);
            }
        }
    }
    // How far down the part each line starts (the lines' own tops when they aren't one under the other): a wave or hatch on
    // a side goes on from one line to the next, and a hatch's tiles start at the part's top.
    let mut stacked = 0.0;
    for (i, l) in lines.iter_mut().enumerate() {
        let h = l.pitch;
        let along = tops.get(i).map_or(stacked, |t| t - tops[0]);
        let shade_from = shade_top - along;
        for (k, d) in s.div_boxes.iter().enumerate() {
            let (a, c) = div_x(d);
            let from = if i == 0 && first_part && d.starts { div_at[k] } else { 0.0 };
            let to = if i + 1 == n { div_bottom[k].unwrap_or(h) } else { h };
            let origin = if first_part && d.starts { div_at[k] } else { 0.0 };
            if let Some(b) = &d.borders[1] {
                edge(&mut *l, b, false, false, a, from, to, along + from - origin);
            }
            if let Some(b) = &d.borders[3] {
                edge(&mut *l, b, false, true, c - thick(b), from, to, along + from - origin);
                let top = i == 0 && first_part && d.starts && d.borders[0].is_some();
                cast(&mut *l, b, false, [c - thick(b), from, c, to], top, false);
            }
        }
        // The sides stop at the bottom border; its shadow is drawn clear of them.
        let sides_to = if i + 1 == n && !same_next { h - s.border_bottom.as_ref().map_or(0.0, shadowed) } else { h };
        if let Some(b) = &s.border_left {
            edge(&mut *l, b, false, false, x0, 0.0, sides_to, along);
        }
        if let Some(b) = &s.border_right {
            edge(&mut *l, b, false, true, x1 - thick(b), 0.0, sides_to, along);
            let top = i == 0 && first_part && s.border_top.is_some() && !same_prev;
            cast(&mut *l, b, false, [x1 - thick(b), 0.0, x1, sides_to], top, false);
        }
        if s.shading.is_some() || hatch.is_some() {
            // As Word draws it (every combination of sides checked, compat 15 and earlier): the shading fills to a
            // right border, and to a left one only with a right one too; a left border alone (a callout's bar)
            // stands apart from the shaded text, top and bottom borders or not.
            let sx0 = s.border_left.as_ref().filter(|_| s.border_right.is_some()).map_or(left, |b| x0 + thick(b));
            let sx1 = s.border_right.as_ref().map_or(width - s.ind_right, |b| x1 - thick(b));
            let top = if i == 0 { shade_top } else { 0.0 };
            let bottom = if i + 1 == n { shade_bottom.unwrap_or(h + if last_part { shade_gap } else { 0.0 }) } else { h };
            let r = [sx0, top, sx1, bottom];
            match (hatch, s.shading) {
                (Some(hatch), fill) => l.shade = Some(crate::layout::shade::hatched([sx0, shade_from, sx1, bottom], r, fill, hatch)),
                (None, Some(fill)) => l.rects.insert(0, (r, fill, true, crate::layout::fragment::Dash::Solid)),
                (None, None) => {}
            }
        }
        stacked += h;
    }
}
