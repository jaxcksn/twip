//! Line heights

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::LineRule;
use crate::font::Fonts;
use crate::layout::text::{Kind, Object, Piece, ShapedPara};
use crate::layout::units::Lu;

use super::{Align, Frame, Line, align_drop, object_drop};

// Line height
fn picture_line_height(sp: &ShapedPara, line: &[Piece], mark: &Run, fonts: &Fonts, last: bool, compat: Compat, run_h: f64) -> f64 {
    let height = |run: &Run| {
        fonts.face(&run.ascii, None, run.bold, run.italic).map_or(0.0, |f| {
            let (a, d, e) = sp.metrics(&f, run, run.size);
            a + e + d
        })
    };
    let objects = line.iter().enumerate().filter(|(_, p)| p.kind == Kind::Object);
    match compat {
        Compat::Generic => objects.map(|(_, p)| height(&sp.styles[p.style as usize].run)).fold(0.0, f64::max),
        Compat::Legacy if run_h > 0.0 => 0.0,
        Compat::Legacy => {
            let before = objects
                .filter_map(|(k, _)| line[..k].iter().rev().find(|q| matches!(q.kind, Kind::Text | Kind::Space)))
                .map(|q| height(&sp.styles[q.style as usize].run))
                .fold(0.0, f64::max);
            if before > 0.0 {
                before
            } else if last {
                height(mark)
            } else {
                line.last().map_or(0.0, |p| height(&sp.styles[p.style as usize].run))
            }
        }
    }
}

fn times_line(h: f64, m: f64) -> f64 {
    let (h, line) = (Lu::round_half_up(h).0 as i64, (m * 240.0).round() as i64);
    Lu(((h * line + 120).div_euclid(240)) as i32).to_pt()
}

pub(super) fn size(line: &mut Line, sp: &ShapedPara, pieces: &[Piece], style: &ParaStyle, mark: &Run, fonts: &Fonts, last: bool, frame: &Frame) {
    let compat = frame.compat;
    let (mut asc, mut desc) = (0.0f64, 0.0f64);
    let mark_size = style.mark_size.unwrap_or(mark.size);
    let mut object = false;
    let (mut un_asc, mut un_desc, mut un_text_asc, mut un_text_desc) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut run_h = 0.0f64;
    let (mut em_above, mut em_below) = (false, false);
    let (mut boxed_asc, mut boxed_desc) = (0.0f64, 0.0f64);
    let mut num_asc = 0.0f64;
    let mut has_content = false;
    let mut has_text = false;
    let mut separator = false;
    let mut brk: Option<(f64, f64)> = None;
    let mut blank: Option<(f64, f64)> = None;
    // The deepest descent an at-least line keeps below its baseline (a turned ruby annotation's lower half).
    let mut at_least_desc = 0.0f64;
    // `auto` centres the runs in flows that keep East Asian text upright, else aligns them to the baseline.
    line.align = match style.text_alignment.as_str() {
        "top" => Align::Top,
        "center" => Align::Center,
        "bottom" => Align::Bottom,
        "auto" | "" if frame.vertical && frame.centred => Align::Center,
        _ => Align::Baseline,
    };
    line.vertical = frame.vertical;
    for p in &pieces[line.start..line.end.min(pieces.len())] {
        match p.kind {
            // A note separator's line is in its run's size.
            Kind::Text | Kind::Object | Kind::Separator | Kind::ContSeparator | Kind::FullSeparator => {
                has_content = true;
                has_text = true;
                let st = &sp.styles[p.style as usize];
                let e = if frame.vertical && p.kind == Kind::Text { crate::layout::vertical::extra(st.face.as_deref(), st.size) } else { 0.0 };
                let (mut pa, mut pd) = (p.ascent + e, p.descent + e);
                if p.kind == Kind::Text
                    && let Some(f) = &st.face
                {
                    let c = align_drop(f, st.size, line.align, frame.vertical);
                    (pa, pd) = (pa - c, pd + c);
                } else if let Some(Object::Picture(_)) = p.object.map(|o| &sp.objects[o as usize]) {
                    let c = object_drop(p.ascent, p.descent, line.align);
                    (pa, pd) = (pa - c, pd + c);
                }
                // A text border's sides make its runs taller; kept apart, as the spacing's extra may hold them.
                let side = if p.kind == Kind::Text { st.run.border.as_ref().map_or(0.0, |b| crate::layout::text::border_side(b).0) } else { 0.0 };
                (boxed_asc, boxed_desc) = (boxed_asc.max(pa), boxed_desc.max(pd));
                asc = asc.max(pa - side);
                desc = desc.max(pd - side);
                let deeper = match p.object.map(|o| &sp.objects[o as usize]) {
                    Some(Object::Ruby(r)) => r.at_least_descent - p.descent,
                    _ => 0.0,
                };
                at_least_desc = at_least_desc.max(pd + deeper);
                let r = if sp.rules.no_raise_lower { 0.0 } else { st.run.position };
                let (a0, d0) = ((pa - r).max(0.0), (pd + r).max(0.0));
                if p.kind == Kind::Text {
                    match st.run.em.as_deref() {
                        Some("underDot") => em_below = true,
                        Some(_) => em_above = true,
                        None => {}
                    }
                }
                un_asc = un_asc.max(a0);
                un_desc = un_desc.max(d0);
                if p.kind == Kind::Object {
                    object = true;
                } else if compat == Compat::Legacy && p.kind != Kind::Text {
                    separator = true;
                } else {
                    un_text_asc = un_text_asc.max(a0);
                    un_text_desc = un_text_desc.max(d0);
                    run_h = run_h.max(a0 + d0 - 2.0 * side);
                }
            }
            // A list number is a run in its own font under the paragraph's alignment.
            Kind::ListNumber => {
                has_content = true;
                let st = &sp.styles[p.style as usize];
                let c = st.face.as_ref().map_or(0.0, |f| align_drop(f, st.size, line.align, frame.vertical));
                num_asc = num_asc.max(p.ascent - c);
            }
            Kind::LineBreak => brk = Some((p.ascent, p.descent)),
            Kind::Space | Kind::Tab => {
                let st = &sp.styles[p.style as usize];
                let e = if frame.vertical { crate::layout::vertical::extra(st.face.as_deref(), st.size) } else { 0.0 };
                let (pa, pd) = (p.ascent + e, p.descent + e);
                blank = Some(blank.map_or((pa, pd), |(a, d): (f64, f64)| (a.max(pa), d.max(pd))));
            }
            _ => {}
        }
    }
    // A line of only spaces and tabs takes their height unless it is the paragraph's last line, which takes the
    // mark's; one ending in a break takes the break's.
    if !has_content && let Some((a, d)) = brk.or(blank.filter(|_| !last)) {
        asc = a;
        desc = d;
        un_asc = a;
        un_desc = d;
    } else if (!has_content || (last && !has_text))
        && let Some(f) = fonts.face(&mark.ascii, None, mark.bold, mark.italic)
    {
        let (a, d, e) = sp.metrics(&f, mark, mark_size);
        let v = if frame.vertical { crate::layout::vertical::extra(Some(&f), mark_size) } else { 0.0 };
        let (a, d) = (a + e + v, d + v);
        asc = asc.max(a);
        desc = desc.max(d);
        un_asc = un_asc.max(a);
        un_desc = un_desc.max(d);
    }
    // Compat ≤ 14: a note separator with no text before it counts as an object: the line is at least the mark's
    // height, and spaces by it.
    let mut sep_mark = None;
    if separator
        && run_h == 0.0
        && let Some(f) = fonts.face(&mark.ascii, None, mark.bold, mark.italic)
    {
        let (a, d, e) = sp.metrics(&f, mark, mark_size);
        asc = asc.max(a + e);
        desc = desc.max(d);
        sep_mark = Some(a + e + d);
    }
    // A paragraph's last line of only pictures is at least a single line high, all of the extra above the pictures:
    // the mark's single height in compat ≤ 14, the picture runs' own fonts' in compat 15.
    if last && object && un_text_asc + un_text_desc + num_asc == 0.0 {
        let min = match compat {
            Compat::Legacy => fonts.face(&mark.ascii, None, mark.bold, mark.italic).map_or(0.0, |f| {
                let (a, d, e) = sp.metrics(&f, mark, mark_size);
                a + e + d
            }),
            Compat::Generic => picture_line_height(sp, &pieces[line.start..line.end.min(pieces.len())], mark, fonts, last, compat, 0.0),
        };
        asc = asc.max(min - desc);
    }
    let asc = asc.max(num_asc);
    let un_asc = if !has_text { un_asc.max(num_asc) } else { un_asc };
    let grow = asc.max(boxed_asc) + desc.max(boxed_desc) - asc - desc;
    let (asc, desc) = (asc.max(boxed_asc), desc.max(boxed_desc));
    let h = asc + desc;
    // Emphasis marks take a quarter of the line's single height (half, split, with marks above and below), unless the
    // spacing's extra holds them; exact lines have no room for them.
    let (em, em_up) = {
        let hl = Lu::round_half_up(h).0;
        let k = if em_above && em_below {
            hl / 2
        } else if em_above || em_below {
            hl / 4
        } else {
            0
        };
        let up = if em_below { if em_above { k / 2 } else { 0 } } else { k };
        (Lu(k).to_pt(), Lu(up).to_pt())
    };
    line.ascent = asc + em_up;
    line.descent = desc + em - em_up;
    let mut text_h = if has_text && run_h > 0.0 { run_h } else { un_asc + un_desc };
    if object {
        text_h = run_h.max(picture_line_height(sp, &pieces[line.start..line.end.min(pieces.len())], mark, fonts, last, compat, run_h));
    }
    if let Some(m) = sep_mark {
        text_h = m;
    }
    // Line spacing
    match style.line_rule {
        LineRule::Auto => {
            let m = style.line;
            let extra = times_line(text_h, m) - text_h;
            // ponytail: the extra holds a text border's growth as it holds an emphasis mark; seen at 1.2 lines only.
            let held = if grow <= extra { grow } else { 0.0 };
            line.pitch = h - held + extra + if em <= extra - held { 0.0 } else { em };
            if m >= 1.0 {
                line.baseline = asc + em_up;
                line.below = line.pitch - h - em;
            } else {
                line.baseline = line.pitch - times_line(desc + em - em_up, m);
            }
        }
        LineRule::AtLeast => {
            let x = style.line;
            let grow = if em <= (x - h).max(0.0) { 0.0 } else { em };
            line.pitch = x.max(h) + grow;
            line.baseline = if x <= h { asc } else { asc.max(x - desc.max(at_least_desc)) } + if grow > 0.0 { em_up } else { 0.0 };
        }
        LineRule::Exact => {
            // Whole layout units, so a page's run of exact lines adds up as their rounded heights.
            let x = Lu::round_half_up(style.line).0;
            line.pitch = Lu(x).to_pt();
            let down = if frame.exact_on_top { (x - Lu::round_half_up(asc).0).max(0) } else { x / 5 };
            line.baseline = Lu(x - down).to_pt();
            if frame.exact_extra_after {
                line.below = (Lu(down).to_pt() - desc).max(0.0);
            }
        }
    }
    // Document grid
    if let Some(p) = frame.line_grid
        && style.snap_to_grid
        && style.line_rule != LineRule::Exact
        && style.line > 0.0
    {
        let p = i64::from(p);
        let h = i64::from(Lu::round_half_up(h).0);
        let m = if style.line_rule == LineRule::Auto { (style.line * 240.0).round() as i64 } else { 240 };
        let hg = ((h + p - 1) / p * p).max(p * m / 240);
        let below = (hg - h) / 2;
        let x = if style.line_rule == LineRule::AtLeast { i64::from(Lu::round_half_up(style.line).0) } else { 0 };
        let pitch = hg.max(x);
        let lu = |v: i64| Lu(v as i32).to_pt();
        line.pitch = lu(pitch);
        line.below = lu(below);
        line.baseline = lu(pitch - below) - desc;
    }
}
