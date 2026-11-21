//! Line drawing

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::Jc;
use crate::docx::review::{AuthorPalette, MarkKind, MarkupView, ReviewView};
use crate::font::Fonts;
use crate::layout::fragment::{Atom, GlyphRun, ImageKey, Item, LineFragment};
use crate::layout::ids::{ParaId, Unit};
use crate::layout::text::{Kind, Object, Piece, ShapedPara, symbol_unicode};
use crate::layout::units::EPS;

use super::{Line, align_drop, breaking::text_of, object_drop};

// Text colour
/// A legacy check box's square, `advance` wide at `x`: its outer edge 0.595 pt after the start and 0.84 pt before the
/// end, a 0.72 pt line, the two diagonals when checked.
// ponytail: the square's bottom is 0.15 box sizes below the baseline (Arial's); it moves with the font.
fn check_box(x: f64, base: f64, advance: f64, e: f64, checked: bool, colour: Option<u32>) -> Item {
    use crate::drawing::geom::{Geom, GeomPath, PathFill, Seg};
    use crate::drawing::paint::{Paint, ResolvedEffects, ResolvedLine};
    const STROKE: f64 = 0.72;
    let side = advance - 0.595 - 0.84 - STROKE;
    let (x0, y1) = (x + 0.595 + STROKE / 2.0, base + 0.15 * e - STROKE / 2.0);
    let rect = [x0, y1 - side, x0 + side, y1];
    let mut segs = vec![Seg::Move([0.0, 0.0]), Seg::Line([side, 0.0]), Seg::Line([side, side]), Seg::Line([0.0, side]), Seg::Close];
    if checked {
        segs.extend([Seg::Move([0.0, 0.0]), Seg::Line([side, side]), Seg::Move([side, 0.0]), Seg::Line([0.0, side])]);
    }
    let geom =
        Geom { paths: vec![GeomPath { segs, fill: PathFill::None, stroke: true }], text_rect: [0.0, 0.0, side, side], handles: vec![], cxn: vec![] };
    let rgb = colour.unwrap_or(0);
    let paint = Paint::Solid(crate::drawing::colour::Rgba { r: (rgb >> 16) as u8, g: (rgb >> 8) as u8, b: rgb as u8, a: 255 });
    Item::Shape(Rc::new(crate::layout::fragment::ShapeItem {
        rect,
        rot: 0.0,
        flip_h: false,
        flip_v: false,
        geom: Rc::new(geom),
        fill: Paint::None,
        line: ResolvedLine {
            width: STROKE,
            paint,
            join: crate::drawing::paint::Join::Miter(crate::drawing::units::Pct(800_000)),
            ..ResolvedLine::none()
        },
        effects: ResolvedEffects::default(),
        behind: false,
        float: 0,
        path: Vec::new(),
        picture: false,
        ink: true,
    }))
}

fn ink(r: &RunStyle, review: &ReviewView, behind: Option<u32>) -> Option<u32> {
    if let Some(c) = revision_colour(r, review) {
        return Some(c);
    }
    r.color.or_else(|| r.shading.or(behind).filter(|c| crate::layout::env::is_dark(*c)).map(|_| 0xFFFFFF))
}

fn behind(o: &LineOrigin, src: &Paragraph) -> Option<u32> {
    src.para_style_in(o.review.view).shading.or(o.backdrop)
}

fn revision_colour(r: &RunStyle, review: &ReviewView) -> Option<u32> {
    let m = r.rev.del.as_ref().or(r.rev.ins.as_ref()).filter(|_| review.view == MarkupView::All)?;
    Some(if matches!(m.kind, MarkKind::MoveFrom | MarkKind::MoveTo) { AuthorPalette::MOVE } else { review.palette.color(m.author) })
}

fn line_changed(o: &LineOrigin, line: &Line, src: &Paragraph) -> bool {
    let view = o.review.view;
    let last = line.end >= o.pieces().len();
    if src.style.change.as_ref().is_some_and(|c| c.previous_raw.0.is_some()) || (last && !src.mark.rev.is_empty()) {
        return true;
    }
    if !src.inlines.iter().any(|i| i.run().is_some_and(|r| !r.rev.is_empty())) {
        return false;
    }
    let pieces = &o.pieces()[line.start..line.end];
    let lo = pieces.iter().filter_map(|p| p.src).map(|u| u.0).min();
    let hi = pieces.iter().filter_map(|p| p.src.map(|u| u.0 + text_of(o.shaped, p).chars().count().max(1) as u32)).max();
    let (Some(lo), Some(hi)) = (lo, hi) else { return false };
    let mut at = 0u32;
    src.inlines.iter().any(|i| {
        let (a, n) = (at, i.units() as u32);
        at += n;
        i.run().is_some_and(|r| !r.rev.is_empty() && n > 0 && if view.hides(&r.rev) { a <= hi && a + n >= lo } else { a < hi && a + n > lo })
    })
}

pub struct LineOrigin<'a> {
    pub shaped: &'a ShapedPara,
    pub from: usize,
    pub width: f64,
    pub x0: f64,
    pub compat: Compat,
    pub para: ParaId,
    pub review: &'a ReviewView,
    pub ul_trail_space: bool,
    pub palette: crate::drawing::paint::Palette<'a>,
    pub backdrop: Option<u32>,
}

impl LineOrigin<'_> {
    fn pieces(&self) -> &[Piece] {
        &self.shaped.pieces[self.from.min(self.shaped.pieces.len())..]
    }
}

// Drawing
pub fn draw_line(o: &LineOrigin, line: &Line, src: &Paragraph, fonts: &Fonts) -> LineFragment {
    {
        let sp = o.shaped;
        let pieces = o.pieces();
        let para = o.para;
        let (x0, top) = (o.x0, 0.0);
        let base = top + line.baseline;
        let mut out: Vec<Item> = Vec::with_capacity(line.rects.len() + 2);
        let (mut glyphs_left, mut chars_left) = (line.start..line.end).fold((0, 0), |(g, c), ix| {
            let (a, b) = pieces[ix].glyphs;
            (g + (b - a) as usize, c + text_of(sp, &pieces[ix]).len())
        });
        for (r, c) in &line.rects {
            out.push(Item::rect([x0 + r[0], top + r[1], x0 + r[2], top + r[3]], *c));
        }
        out.extend(line.strokes.iter().map(|s| Item::Shape(Rc::new(s.moved(x0, top)))));
        let rects_end = out.len();
        let behind_text = behind(o, src);
        let mut run: Option<GlyphRunBuilder> = None;
        let flush = |run: &mut Option<GlyphRunBuilder>, out: &mut Vec<Item>| {
            if let Some(r) = run.take()
                && !r.glyphs.is_empty()
            {
                out.push(Item::Glyphs(r.finish()));
            }
        };
        let mut atoms: Vec<Atom> = Vec::new();
        let mut texts: Vec<([f64; 4], crate::docx::Picture)> = Vec::new();
        let mut expansions: Vec<([f64; 4], Rc<crate::layout::expansion::Expansion>)> = Vec::new();
        let join = sp.join.as_deref();
        let source = |p: &Piece| match (join, p.src) {
            (Some(j), Some(u)) => {
                let (q, v) = j.source(u);
                (q, Some(v))
            }
            (Some(j), None) => (j.first(), None),
            (None, u) => (para, u),
        };
        let mut places = caret_places(o, line).into_iter().peekable();
        let mark = src.mark_in(o.review.view);
        let face = places.peek().and_then(|_| fonts.face(&mark.ascii, None, mark.bold, mark.italic));
        let caret = |x: f64, q: ParaId| {
            face.as_ref().map(|f| {
                let (glyphs, chars) = (Box::new([]), Box::new([]));
                Item::Glyphs(GlyphRun { face: f.clone(), size: mark.size, baseline: base, x, glyphs, chars, colour: None, para: Some(q) })
            })
        };
        for (k, ix) in (line.start..line.end).enumerate() {
            let p = &pieces[ix];
            let st = &sp.styles[p.style as usize];
            let x = x0 + line.xs[k];
            while let Some((_, q)) = places.next_if(|e| e.0 == Some(k)) {
                flush(&mut run, &mut out);
                out.extend(caret(x, q));
            }
            let (q, first) = source(p);
            if matches!(p.kind, Kind::Separator | Kind::ContSeparator | Kind::FullSeparator) {
                let full = p.kind == Kind::FullSeparator || p.kind == Kind::ContSeparator && o.compat == Compat::Legacy;
                let len = if full { o.width } else { 144.0 };
                out.push(Item::rect([x, base - 2.7, x + len, base - 2.1], 0));
                continue;
            }
            let next_x = line.xs.get(k + 1).map(|&nx| x0 + nx);
            if p.kind == Kind::Tab {
                flush(&mut run, &mut out);
                draw_leader(o, line, k, x0, base, &mut out);
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + line.widths[k]), para: q, unit, brk: false });
                }
                continue;
            }
            if let (Kind::Object, Some(Object::Expansion(e))) = (p.kind, p.object.map(|o| &sp.objects[o as usize])) {
                flush(&mut run, &mut out);
                let rect = [x, base - e.size.1, x + e.size.0, base];
                expansions.push((rect, e.clone()));
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + e.size.0), para: q, unit, brk: false });
                }
                continue;
            }
            if let Some(&Object::CheckBox(e, checked)) = p.object.map(|o| &sp.objects[o as usize]) {
                flush(&mut run, &mut out);
                out.push(check_box(x, base, line.widths[k], e, checked, ink(&st.run, o.review, behind_text)));
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + line.widths[k]), para: q, unit, brk: false });
                }
                continue;
            }
            if let Some(Object::Ruby(r)) = p.object.map(|o| &sp.objects[o as usize]) {
                flush(&mut run, &mut out);
                ruby_glyphs(&r.base, &r.base_x, x, base, r.narrow, q, o.review, behind_text, &mut out);
                match r.turned {
                    Some([dx, dy]) => {
                        let (centre, mut items) = ([x + dx, base + dy], Vec::new());
                        ruby_glyphs(&r.text, &r.text_x, centre[0], centre[1], 0.0, q, o.review, behind_text, &mut items);
                        // A vertical line of its own: East Asian glyphs stand upright in it.
                        let items = crate::layout::vertical::east_asian_turned(&items);
                        out.push(Item::Turned(Rc::new(crate::layout::fragment::Turned { centre, deg: 90.0, items })));
                    }
                    None => ruby_glyphs(&r.text, &r.text_x, x, base - r.raise, 0.0, q, o.review, behind_text, &mut out),
                }
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + line.widths[k]), para: q, unit, brk: false });
                }
                continue;
            }
            // A picture bullet is a list label that is a picture.
            if p.kind == Kind::Object || (p.kind == Kind::ListNumber && p.object.is_some()) {
                if let Some(Object::Picture(pic)) = p.object.map(|o| &sp.objects[o as usize]) {
                    flush(&mut run, &mut out);
                    let drop = if p.kind == Kind::Object { object_drop(p.ascent, p.descent, line.align) } else { 0.0 };
                    let bottom = base + drop - sp.styles[p.style as usize].run.position - pic.effect[3];
                    let x0 = x + pic.effect[0];
                    let rect = [x0, bottom - pic.h, x0 + pic.w, bottom];
                    match pic.shape.as_deref() {
                        Some(shape) => out.extend(crate::layout::story::shape_items(shape, rect, 0, false, o.palette).into_iter().map(Item::Shape)),
                        None => out.push(Item::Image { rect, key: ImageKey { index: pic.image, generation: 0 } }),
                    }
                    if pic.text.is_some() {
                        texts.push((rect, pic.clone()));
                    }
                    if let Some(unit) = first {
                        atoms.push(Atom { x0: x, x1: next_x.unwrap_or(x + pic.w), para: q, unit, brk: false });
                    }
                }
                continue;
            }
            if matches!(p.kind, Kind::LineBreak | Kind::PageBreak | Kind::ColumnBreak) {
                if let Some(unit) = first {
                    atoms.push(Atom { x0: x, x1: x, para: q, unit, brk: true });
                }
                continue;
            }
            if !matches!(p.kind, Kind::Text | Kind::Space | Kind::ListNumber) {
                continue;
            }
            // A piece that draws nothing (a legacy list number's gap) ends the glyph run.
            let Some(face) = &st.face else {
                flush(&mut run, &mut out);
                continue;
            };
            if p.soft_hyphen && !(line.hyphen && ix + 1 == line.end) {
                continue;
            }
            let y = base - st.raise + align_drop(face, st.size, line.align, line.vertical);
            let color = ink(&st.run, o.review, behind_text);
            let same = run.as_ref().is_some_and(|r| {
                Rc::ptr_eq(&r.face, face) && (r.size - st.size).abs() < EPS && (r.y - y).abs() < EPS && r.color == color && r.para == Some(q)
            });
            if !same {
                flush(&mut run, &mut out);
                run = Some(GlyphRunBuilder {
                    face: face.clone(),
                    size: st.size,
                    y,
                    glyphs: Vec::with_capacity(glyphs_left + 1),
                    chars: Vec::with_capacity(chars_left + 1),
                    color,
                    para: Some(q),
                    x,
                });
            }
            let Some(r) = run.as_mut() else { continue };
            let scale = face.scale(st.size) * st.run.scale / 100.0;
            // A text border's box starts here: its left side comes before the glyphs.
            let i = o.from + ix;
            let x = match crate::layout::text::boxed(&sp.pieces, &sp.styles, i) {
                Some(b) if ix == line.start || crate::layout::text::boxed(&sp.pieces, &sp.styles, i - 1) != Some(b) => {
                    x + crate::layout::text::border_side(b).1
                }
                _ => x,
            };
            let mut gx = x;
            if p.soft_hyphen {
                if let Some(g) = face.glyph('-') {
                    r.glyphs.push((g, gx));
                    r.chars.push(('-', gx, first));
                }
                continue;
            }
            for (id, advance, dx) in sp.glyphs(o.from + ix) {
                r.glyphs.push((id, gx + dx * scale));
                gx += advance * scale;
            }
            let text = text_of(sp, p);
            for (j, c) in text.chars().enumerate() {
                r.chars.push((symbol_unicode(&face.family, c), x, first.map(|u| Unit(u.0 + j as u32))));
            }
            if line.hyphen
                && ix + 1 == line.end
                && let Some(g) = face.glyph('-')
            {
                r.glyphs.push((g, gx));
                r.chars.push(('-', gx, None));
            }
            glyphs_left = glyphs_left.saturating_sub((p.glyphs.1 - p.glyphs.0) as usize);
            chars_left = chars_left.saturating_sub(text.len());
        }
        flush(&mut run, &mut out);
        draw_decorations(o, line, base, rects_end, behind_text, &mut out);
        let end = match (line.xs.last(), line.widths.last()) {
            (Some(x), Some(w)) => x0 + x + w,
            _ => {
                let s = src.para_style_in(o.review.view);
                let left = s.ind_left + if o.from == 0 { s.ind_first } else { 0.0 };
                let right = o.width - s.ind_right;
                x0 + match (s.jc, s.bidi) {
                    (Jc::Center, _) => (left + right) / 2.0,
                    (Jc::Right, false) | (Jc::Left | Jc::Both, true) => right,
                    _ => left,
                }
            }
        };
        for (_, q) in places.by_ref() {
            out.extend(caret(end, q));
        }
        let changed = o.review.view.change_bars() && line_changed(o, line, src);
        let mark = (line.start == line.end && o.shaped.join.is_none()).then(|| Unit(src.inlines.iter().map(Inline::units).sum::<usize>() as u32));
        LineFragment {
            items: out.into_boxed_slice(),
            pitch: line.pitch,
            baseline: line.baseline,
            below: line.below,
            atoms: atoms.into_boxed_slice(),
            changed,
            mark,
            texts: texts.into_boxed_slice(),
            expansions: expansions.into_boxed_slice(),
        }
    }
}

/// A ruby's base or annotation: its pieces at `xs` from `x`, on `base`, each glyph `narrow` closer to the previous one
/// than its advance.
#[allow(clippy::too_many_arguments, reason = "the line's place, review view and backdrop, as for the line's own glyphs")]
fn ruby_glyphs(
    s: &ShapedPara,
    xs: &[f64],
    x: f64,
    base: f64,
    narrow: f64,
    para: ParaId,
    review: &ReviewView,
    behind: Option<u32>,
    out: &mut Vec<Item>,
) {
    let mut run: Option<GlyphRunBuilder> = None;
    for (i, &px) in xs.iter().enumerate() {
        let st = s.style(i);
        let Some(face) = &st.face else { continue };
        let (y, color, x0) = (base - st.raise, ink(&st.run, review, behind), x + px);
        if !run.as_ref().is_some_and(|r| Rc::ptr_eq(&r.face, face) && r.size == st.size && r.y == y && r.color == color) {
            out.extend(run.take().map(|r| Item::Glyphs(r.finish())));
            let (glyphs, chars) = (Vec::new(), Vec::new());
            run = Some(GlyphRunBuilder { face: face.clone(), size: st.size, y, glyphs, chars, color, para: Some(para), x: x0 });
        }
        let Some(r) = run.as_mut() else { continue };
        let scale = face.scale(st.size) * st.run.scale / 100.0;
        let mut gx = x0;
        for (id, advance, dx) in s.glyphs(i) {
            r.glyphs.push((id, gx + dx * scale));
            gx += (advance * scale - narrow).max(0.0);
        }
        r.chars.extend(s.text(i).chars().map(|c| (symbol_unicode(&face.family, c), x0, None)));
    }
    out.extend(run.map(|r| Item::Glyphs(r.finish())));
}

fn caret_places(o: &LineOrigin, line: &Line) -> Vec<(Option<usize>, ParaId)> {
    let mut out = Vec::new();
    if let Some(j) = o.shaped.join.as_deref() {
        let all = &o.shaped.pieces;
        let here = o.from + line.start..o.from + line.end;
        for (q, start, end) in j.parts() {
            if all.iter().any(|p| p.src.is_some_and(|u| u >= start && end.is_none_or(|e| u < e))) {
                continue;
            }
            match all.iter().position(|p| p.src.is_some_and(|u| u >= start)) {
                Some(g) if here.contains(&g) => out.push((Some(g - here.start), q)),
                None if here.end >= all.len() => out.push((None, q)),
                _ => {}
            }
        }
    }
    if line.start == line.end && !out.iter().any(|e| e.1 == o.para) {
        out.push((None, o.para));
    }
    out
}

fn draw_leader(o: &LineOrigin, line: &Line, k: usize, x0: f64, base: f64, out: &mut Vec<Item>) {
    {
        let sp = o.shaped;
        let p = &o.pieces()[line.start + k];
        let st = &sp.styles[p.style as usize];
        let Some(leader) = line.leaders.iter().find(|(j, _)| *j == k).and_then(|(_, l)| l.glyph()) else { return };
        let Some(face) = &st.face else { return };
        let Some(g) = face.glyph(leader) else { return };
        let w = (face.advance(g) * face.scale(st.size) * 600.0 / 72.0).round() * 72.0 / 600.0;
        if w <= 0.0 {
            return;
        }
        let from = x0 + line.xs[k];
        let to = from + line.widths[k];
        let mut slot = (from / w - 1e-9).ceil();
        let mut r = GlyphRunBuilder {
            face: face.clone(),
            size: st.size,
            y: base - st.raise,
            glyphs: Vec::new(),
            chars: Vec::new(),
            color: st.run.color,
            para: None,
            x: from,
        };
        while (slot + 1.0) * w <= to + 1e-6 {
            r.glyphs.push((g, slot * w));
            r.chars.push((leader, slot * w, None));
            slot += 1.0;
        }
        if !r.glyphs.is_empty() {
            out.push(Item::Glyphs(r.finish()));
        }
    }
}

struct GlyphRunBuilder {
    face: Rc<crate::font::Face>,
    size: f64,
    y: f64,
    glyphs: Vec<(u32, f64)>,
    chars: Vec<(char, f64, Option<Unit>)>,
    color: Option<u32>,
    para: Option<ParaId>,
    x: f64,
}

impl GlyphRunBuilder {
    fn finish(self) -> GlyphRun {
        GlyphRun {
            face: self.face,
            size: self.size,
            baseline: self.y,
            x: self.x,
            glyphs: self.glyphs.into_boxed_slice(),
            chars: self.chars.into_boxed_slice(),
            colour: self.color,
            para: self.para,
        }
    }
}

fn draw_decorations(o: &LineOrigin, line: &Line, base: f64, bg_at: usize, behind: Option<u32>, out: &mut Vec<Item>) {
    let (sp, x0, review) = (o.shaped, o.x0, o.review);
    let pieces = &o.pieces()[line.start..line.end];
    let content = pieces.iter().rposition(|p| p.kind != Kind::Space).map_or(0, |k| k + 1);
    let (mut bg, mut fg) = (Vec::new(), Vec::new());
    let push = |v: &mut Vec<Item>, r: [f64; 4], c: u32| {
        let joins = |it: &&mut Item| matches!(it, Item::Rect { rect: l, colour: lc } if *lc == c && (l[1] - r[1]).abs() < EPS && (l[3] - r[3]).abs() < EPS && (r[0] - l[2]).abs() < 0.01);
        match v.iter_mut().rev().take(4).find(joins) {
            Some(Item::Rect { rect: last, .. }) => last[2] = r[2],
            _ => v.push(Item::rect(r, c)),
        }
    };
    // Text borders: (border, left, right, top, bottom) of the stretch of runs drawn so far.
    let mut boxed: Option<(crate::docx::props::Border, f64, f64, f64, f64)> = None;
    let flush_box = |boxed: &mut Option<(crate::docx::props::Border, f64, f64, f64, f64)>, fg: &mut Vec<Item>| {
        let Some((b, l, r, t, btm)) = boxed.take().filter(|x| x.0.visible()) else { return };
        let w = crate::layout::table::thick(&b);
        // The runs' widths hold the box's sides.
        let (t, btm) = (t - b.space - w, btm + b.space + w);
        crate::layout::table::edge(fg, [l, t, r, t + w], true, false, &b);
        crate::layout::table::edge(fg, [l, btm - w, r, btm], true, true, &b);
        crate::layout::table::edge(fg, [l, t, l + w, btm], false, false, &b);
        crate::layout::table::edge(fg, [r - w, t, r, btm], false, true, &b);
        crate::layout::table::shadow(fg, [l, btm - w, r, btm], true, true, &b, true, true);
        crate::layout::table::shadow(fg, [r - w, t, r, btm], false, true, &b, true, false);
    };
    for (k, p) in pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = st.face.as_ref().filter(|_| matches!(p.kind, Kind::Text | Kind::Space | Kind::Tab | Kind::ListNumber)) else { continue };
        let (r, label) = (&st.run, p.kind == Kind::ListNumber);
        let (x, w) = (x0 + line.xs[k], line.widths[k]);
        if w <= 0.0 {
            continue;
        }
        let em = face.scale(st.size);
        let y = base - st.raise;
        let border = r.border.as_ref().filter(|_| !label && k < content);
        match (border, &mut boxed) {
            (Some(b), Some((was, _, right, top, bottom))) if b == was => {
                *right = x + w;
                *top = top.min(y - face.ascent * em);
                *bottom = bottom.max(y + face.descent * em);
            }
            (Some(b), _) => {
                flush_box(&mut boxed, &mut fg);
                boxed = Some((b.clone(), x, x + w, y - face.ascent * em, y + face.descent * em));
            }
            (None, _) => flush_box(&mut boxed, &mut fg),
        }
        let fill = r.highlight.or(r.shading).filter(|_| !label);
        if let Some(fill) = fill {
            push(&mut bg, [x, y - face.ascent * em, x + w, y + face.descent * em], fill);
        }
        let trailing = k >= content;
        if trailing && !o.ul_trail_space {
            continue;
        }
        let ink = ink(r, review, behind).unwrap_or(0);
        let line_at = |v: &mut Vec<Item>, y: f64, t: f64| push(v, [x, y - t / 2.0, x + w, y + t / 2.0], ink);
        let marked = review.view == MarkupView::All && !label;
        let kind = |m: &Option<Rc<crate::docx::review::RevisionMark>>| m.as_ref().filter(|_| marked).map(|m| m.kind);
        let (ins, del) = (kind(&r.rev.ins), kind(&r.rev.del));
        let rev_u = ins.map(|k| if k == MarkKind::MoveTo { "double" } else { "single" });
        if let Some(u) = rev_u.or(r.underline.as_deref()).filter(|u| !label && !(*u == "words" && matches!(p.kind, Kind::Space | Kind::Tab))) {
            let (pos, thick) =
                if face.underline_thick > 0.0 { (face.underline_pos, face.underline_thick) } else { (0.1 * face.upem, 0.05 * face.upem) };
            let (uy, t) = (y + pos * em, (thick * em).max(0.5));
            match u {
                "double" => {
                    line_at(&mut fg, uy, t);
                    line_at(&mut fg, uy + 2.0 * t, t);
                }
                "thick" => line_at(&mut fg, uy + t / 2.0, 2.0 * t),
                _ => line_at(&mut fg, uy, t),
            }
        }
        let dstrike = r.dstrike || del == Some(MarkKind::MoveFrom);
        if (r.strike || dstrike || del.is_some()) && !trailing {
            let t = (face.strike_thick * em).max(0.5);
            let sy = y - face.strike_pos * em;
            if dstrike {
                line_at(&mut fg, sy - t, t);
                line_at(&mut fg, sy + t, t);
            } else {
                line_at(&mut fg, sy, t);
            }
        }
    }
    flush_box(&mut boxed, &mut fg);
    out.splice(bg_at..bg_at, bg);
    out.extend(fg);
}

pub fn draw_label(text: &str, run: &Run, x_end: f64, y: f64, fonts: &Fonts) -> Vec<Item> {
    let sp = crate::layout::text::shape_run(text, run, fonts);
    let w: f64 = sp.pieces.iter().map(|p| p.width).sum();
    let mut x = x_end - w;
    let mut out = Vec::new();
    if x < 0.0 {
        return out;
    }
    for (i, p) in sp.pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = &st.face else { continue };
        let scale = face.scale(st.size);
        let mut r =
            GlyphRunBuilder { face: face.clone(), size: st.size, y, glyphs: Vec::new(), chars: Vec::new(), color: st.run.color, para: None, x };
        let mut gx = x;
        for (id, advance, dx) in sp.glyphs(i) {
            r.glyphs.push((id, gx + dx * scale));
            gx += advance * scale;
        }
        for c in text_of(&sp, p).chars() {
            r.chars.push((c, x, None));
        }
        out.push(Item::Glyphs(r.finish()));
        x += p.width;
    }
    out
}
