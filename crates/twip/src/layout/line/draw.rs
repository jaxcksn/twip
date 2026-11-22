//! Line drawing

use std::rc::Rc;

use crate::docx::Compat;
use crate::docx::model::*;
use crate::docx::props::Jc;
use crate::docx::review::{AuthorPalette, MarkKind, MarkupView, ReviewView};
use crate::drawing::paint::Palette;
use crate::font::Fonts;
use crate::layout::fragment::{Atom, GlyphRun, ImageKey, Item, LineFragment, PictureOf};
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
        decoration: true,
        snap: false,
        clip_bottom: None,
        text_float: 0,
        text: None,
        of: Default::default(),
        text_path: None,
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
    if src.style.change.as_ref().is_some_and(|c| !c.numbering_only()) || (last && !src.mark.rev.is_empty()) {
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

/// How far a line's paragraph goes on below it.
#[derive(Clone, Copy)]
pub enum Below<'a> {
    /// Its paragraph's lines, and its place among them.
    Lines(&'a super::ParaLines, usize),
    /// None: it is the last.
    Last,
    /// The distance from its top to the bottom of the paragraph's last line.
    To(f64),
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
    pub page_colour: Option<u32>,
    /// Whether a paragraph's automatic spacing is the fixed 14 points (HTML paragraph auto spacing is on).
    pub html_auto_spacing: bool,
    pub below: Below<'a>,
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
        out.extend(line.shade.iter().map(|s| Item::Shape(Rc::new(s.moved(x0, top)))));
        for &(r, colour, snap, dash, phase) in &line.rects {
            out.push(Item::Rect { rect: [x0 + r[0], top + r[1], x0 + r[2], top + r[3]], colour, snap, dash, phase });
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
                Item::Glyphs(GlyphRun {
                    face: f.clone(),
                    size: mark.size,
                    baseline: base,
                    x,
                    glyphs,
                    chars,
                    colour: None,
                    para: Some(q),
                    paint: None,
                })
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
                draw_leader(o, line, k, x0, base, behind_text, &mut out);
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
                ruby_glyphs(&r.base, &r.base_x, x, base, r.narrow, q, o.review, behind_text, (o.page_colour, o.palette), &mut out);
                match r.turned {
                    Some([dx, dy]) => {
                        let (centre, mut items) = ([x + dx, base + dy], Vec::new());
                        ruby_glyphs(&r.text, &r.text_x, centre[0], centre[1], 0.0, q, o.review, behind_text, (o.page_colour, o.palette), &mut items);
                        // A vertical line of its own: East Asian glyphs stand upright in it.
                        let items = crate::layout::vertical::east_asian_turned(&items);
                        out.push(Item::Turned(Rc::new(crate::layout::fragment::Turned { centre, deg: 90.0, items })));
                    }
                    None => ruby_glyphs(&r.text, &r.text_x, x, base - r.raise, 0.0, q, o.review, behind_text, (o.page_colour, o.palette), &mut out),
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
                    let of = first.map_or(PictureOf::None, |u| PictureOf::Inline(q, u));
                    match pic.shape.as_deref() {
                        Some(shape) => {
                            let mut items = crate::layout::story::shape_items(shape, rect, 0, false, o.palette, None, of, fonts);
                            // A reflection of an inline picture, in a group too, stops at the bottom of its paragraph, spacing
                            // after included.
                            let reflected = |s: &crate::layout::fragment::ShapeItem| s.picture && s.effects.reflection.is_some();
                            if items.iter().any(|s| reflected(s)) {
                                let style = src.para_style_in(o.review.view);
                                let after = if style.after_auto && o.html_auto_spacing { 14.0 } else { style.space_after };
                                let to_bottom = match o.below {
                                    Below::Lines(lines, i) => lines.height_from(i),
                                    Below::Last => line.pitch,
                                    Below::To(d) => d,
                                };
                                let clip = top + to_bottom + after;
                                // Made just now, so unshared.
                                items.iter_mut().filter_map(Rc::get_mut).filter(|s| reflected(s)).for_each(|s| s.clip_bottom = Some(clip));
                            }
                            if let Some(t) = &pic.text {
                                // Made just now, so unshared.
                                items.iter_mut().filter_map(Rc::get_mut).filter(|s| s.path.is_empty()).for_each(|s| s.text_float = t.id);
                            }
                            out.extend(items.into_iter().map(Item::Shape))
                        }
                        None => out.push(Item::Image { rect, key: ImageKey { index: pic.image, generation: 0 }, of }),
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
            let paint = glyph_paint(&st.run, st.size, color, under_text(&st.run, behind_text, o.page_colour), o.palette);
            let same = run.as_ref().is_some_and(|r| {
                Rc::ptr_eq(&r.face, face)
                    && (r.size - st.size).abs() < EPS
                    && (r.y - y).abs() < EPS
                    && r.color == color
                    && r.para == Some(q)
                    && r.paint == paint
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
                    paint,
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
    (page_colour, palette): (Option<u32>, Palette),
    out: &mut Vec<Item>,
) {
    let mut run: Option<GlyphRunBuilder> = None;
    for (i, &px) in xs.iter().enumerate() {
        let st = s.style(i);
        let Some(face) = &st.face else { continue };
        let (y, color, x0) = (base - st.raise, ink(&st.run, review, behind), x + px);
        let paint = glyph_paint(&st.run, st.size, color, under_text(&st.run, behind, page_colour), palette);
        if !run.as_ref().is_some_and(|r| Rc::ptr_eq(&r.face, face) && r.size == st.size && r.y == y && r.color == color && r.paint == paint) {
            out.extend(run.take().map(|r| Item::Glyphs(r.finish())));
            let (glyphs, chars) = (Vec::new(), Vec::new());
            run = Some(GlyphRunBuilder { face: face.clone(), size: st.size, y, glyphs, chars, color, para: Some(para), x: x0, paint });
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

fn draw_leader(o: &LineOrigin, line: &Line, k: usize, x0: f64, base: f64, behind: Option<u32>, out: &mut Vec<Item>) {
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
            paint: glyph_paint(&st.run, st.size, st.run.color, under_text(&st.run, behind, o.page_colour), o.palette),
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
    paint: Option<Rc<crate::layout::fragment::GlyphPaint>>,
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
            paint: self.paint,
        }
    }
}

fn draw_decorations(o: &LineOrigin, line: &Line, base: f64, bg_at: usize, behind: Option<u32>, out: &mut Vec<Item>) {
    let (sp, x0, review) = (o.shaped, o.x0, o.review);
    let pieces = &o.pieces()[line.start..line.end];
    let content = pieces.iter().rposition(|p| p.kind != Kind::Space).map_or(0, |k| k + 1);
    let (mut bg, mut fg) = (Vec::new(), Vec::new());
    // Hatched run shading: (rect, fill, hatch) of the stretch of runs drawn alike.
    let mut hatched: Vec<([f64; 4], Option<u32>, crate::docx::props::Hatch)> = Vec::new();
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
    // Dashed and wavy underlines go on across pieces drawn alike, their pattern unbroken.
    let mut span: Option<Span> = None;
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
        let rect = [x, y - face.ascent * em, x + w, y + face.descent * em];
        match r.shd.as_ref().and_then(crate::docx::props::Shd::hatch).filter(|_| r.highlight.is_none() && !label) {
            Some(h) => match hatched.last_mut() {
                Some((l, f, lh)) if *f == r.shading && *lh == h && l[1] == rect[1] && l[3] == rect[3] && (rect[0] - l[2]).abs() < 0.01 => {
                    l[2] = rect[2]
                }
                _ => hatched.push((rect, r.shading, h)),
            },
            None => {
                if let Some(fill) = fill {
                    join(&mut bg, rect, fill, true);
                }
            }
        }
        let trailing = k >= content;
        if trailing && !o.ul_trail_space {
            continue;
        }
        let ink = ink(r, review, behind).unwrap_or(0);
        let copies = copies(r, st.size, under_text(r, behind, o.page_colour));
        let line_at = |v: &mut Vec<Item>, y: f64, t: f64, c: u32| lines(v, [x, y - t / 2.0, x + w, y + t / 2.0], c, &copies);
        let marked = review.view == MarkupView::All && !label;
        let kind = |m: &Option<Rc<crate::docx::review::RevisionMark>>| m.as_ref().filter(|_| marked).map(|m| m.kind);
        let (ins, del) = (kind(&r.rev.ins), kind(&r.rev.del));
        let rev_u = ins.map(|k| if k == MarkKind::MoveTo { "double" } else { "single" });
        let underline = rev_u.or(r.underline.as_deref()).filter(|u| !label && !(*u == "words" && matches!(p.kind, Kind::Space | Kind::Tab)));
        let mut drawn = None;
        if let Some(u) = underline {
            let (pos, thick) =
                if face.underline_thick > 0.0 { (face.underline_pos, face.underline_thick) } else { (0.1 * face.upem, 0.05 * face.upem) };
            let (uy, t) = (y + pos * em, (thick * em).max(0.5));
            // A revision's underline is in the revision's colour, as its text is.
            let c = if rev_u.is_some() || revision_colour(r, review).is_some() { ink } else { r.underline_color.unwrap_or(ink) };
            let form = UnderlineForm::of(u);
            // Outlined text's underline is hollow too (its waves are already lines).
            let hollow = r.outline && !matches!(form, UnderlineForm::Wave { .. });
            match form {
                UnderlineForm::Solid { heavy: false } if !hollow => line_at(&mut fg, uy, t, c),
                UnderlineForm::Solid { heavy: true } if !hollow => line_at(&mut fg, uy + t / 2.0, 2.0 * t, c),
                UnderlineForm::Double if !hollow => {
                    line_at(&mut fg, uy, t, c);
                    line_at(&mut fg, uy + 2.0 * t, t, c);
                }
                form => drawn = Some(Span { form, x0: x, x1: x + w, y: uy, t, colour: c, copies, size: st.size, hollow }),
            }
        }
        match (&mut span, drawn) {
            (Some(open), Some(d)) if open.continues(&d) => open.x1 = d.x1,
            (_, d) => {
                if let Some(done) = std::mem::replace(&mut span, d) {
                    done.draw(&mut fg);
                }
            }
        }
        let dstrike = r.dstrike || del == Some(MarkKind::MoveFrom);
        if (r.strike || dstrike || del.is_some()) && !trailing {
            let t = (face.strike_thick * em).max(0.5);
            let sy = y - face.strike_pos * em;
            if dstrike {
                line_at(&mut fg, sy - t, t, ink);
                line_at(&mut fg, sy + t, t, ink);
            } else {
                line_at(&mut fg, sy, t, ink);
            }
        }
    }
    if let Some(done) = span {
        done.draw(&mut fg);
    }
    flush_box(&mut boxed, &mut fg);
    bg.extend(hatched.into_iter().map(|(r, fill, h)| Item::Shape(crate::layout::shade::hatched(r, r, fill, h))));
    out.splice(bg_at..bg_at, bg);
    out.extend(fg);
}

/// Pushes a solid rect (aliased with `snap`: shading), or grows the one just before it that it continues (same colour,
/// snapping and height, touching).
fn join(v: &mut Vec<Item>, r: [f64; 4], c: u32, snap: bool) {
    let joins = |it: &&mut Item| matches!(it, Item::Rect { rect: l, colour: lc, snap: ls, dash: crate::layout::fragment::Dash::Solid, .. } if *lc == c && *ls == snap && (l[1] - r[1]).abs() < EPS && (l[3] - r[3]).abs() < EPS && (r[0] - l[2]).abs() < 0.01);
    match v.iter_mut().rev().take(4).find(joins) {
        Some(Item::Rect { rect: last, .. }) => last[2] = r[2],
        _ => v.push(if snap { Item::fill(r, c) } else { Item::rect(r, c) }),
    }
}

/// A decoration's rect in colour `c`, over the run's effect copies of it.
fn lines(v: &mut Vec<Item>, r: [f64; 4], c: u32, copies: &Copies) {
    for &(dx, dy, tone) in copies.iter().flatten() {
        join(v, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy], tone, false);
    }
    join(v, r, c, false);
}

// Character effects
/// Emboss and engrave move their copies this far each way, points: Word's PDFs move them 0.96 pt at 11, 16 and 44 pt.
const RELIEF: f64 = 0.96;
/// A shadow's offset right and down per point of font size: Word's PDFs move it 0.48 pt at 11 pt, 0.72 at 16, 1.92 at
/// 44.
const SHADOW_PER_PT: f64 = 0.0436;
/// A shadow's grey: Word's, on white and shaded backgrounds alike.
const SHADOW_TONE: u32 = 0xC0_C0C0;
/// Emboss and engrave's light copy is the background at this share: Word's PDFs draw #999999 on white and #818181
/// on #D9D9D9.
const RELIEF_LIGHT: f64 = 0.6;
/// Outlined text's stroke, points. Word strokes it one device pixel wide (0.24 pt in its 300 dpi PDFs); drawn at any
/// zoom, it takes the decorations' least thickness.
const HAIRLINE: f64 = 0.5;

/// A run's effect copies drawn under its text and decorations: (dx, dy, colour), at most three.
type Copies = [Option<(f64, f64, u32)>; 3];

/// The copies `r`'s shadow, emboss or engrave draws at `size` points over `bg`: emboss a light copy down and right and
/// a black one up and left, engrave the reverse, a shadow a grey one down and right.
fn copies(r: &RunStyle, size: f64, bg: u32) -> Copies {
    let light = {
        let ch = |shift: u32| ((((bg >> shift) & 0xFF) as f64 * RELIEF_LIGHT).round() as u32) << shift;
        ch(16) | ch(8) | ch(0)
    };
    let d = SHADOW_PER_PT * size;
    let relief = match (r.emboss, r.imprint) {
        (true, _) => [Some((RELIEF, RELIEF, light)), Some((-RELIEF, -RELIEF, 0))],
        (false, true) => [Some((RELIEF, RELIEF, 0)), Some((-RELIEF, -RELIEF, light))],
        _ => [None, None],
    };
    [relief[0], relief[1], r.shadow.then_some((d, d, SHADOW_TONE))]
}

/// What is behind a run's text: its highlight or shading, else `behind` (the paragraph's shading, a cell's or a box's
/// fill), else the page colour, else white.
fn under_text(r: &RunStyle, behind: Option<u32>, page_colour: Option<u32>) -> u32 {
    r.highlight.or(r.shading).or(behind).or(page_colour).unwrap_or(0xFF_FFFF)
}

/// The widest text outline drawn, points: Word's widest line.
const MAX_OUTLINE: f64 = 1584.0;

/// How a run's glyphs paint, in `colour` over `bg`, its effects' colours from `palette`: `None` for text without
/// effects. A Word 2010 effect takes the place of the legacy one it shares a part with: a text fill or outline the
/// hollow outline's, a shadow the grey copy.
fn glyph_paint(r: &RunStyle, size: f64, colour: Option<u32>, bg: u32, palette: Palette) -> Option<Rc<crate::layout::fragment::GlyphPaint>> {
    use crate::drawing::paint::{Paint, paint};
    use crate::layout::fragment::{GlyphFill, GlyphPaint};
    let fx = r.effects.as_deref();
    if !(r.emboss || r.imprint || r.outline || r.shadow || fx.is_some()) {
        return None;
    }
    let c = colour.unwrap_or(0);
    let fill = match fx.and_then(|e| e.fill.as_ref()) {
        Some(f) => match paint(f, palette, None, &Paint::None) {
            Paint::None => GlyphFill::None,
            Paint::Solid(rgba) if rgba.a == 255 => GlyphFill::Solid(rgba.to_rgb()),
            p => GlyphFill::Paint(Box::new(p)),
        },
        None if r.outline => GlyphFill::None,
        None => GlyphFill::Solid(c),
    };
    let own_outline = fx.is_some_and(|e| e.outline.is_some());
    let mut copies = copies(r, size, bg);
    if fx.is_some_and(|e| e.shadow.is_some()) {
        copies[2] = None;
    }
    Some(Rc::new(GlyphPaint {
        fill,
        outline: r.outline.then_some((HAIRLINE, c)).filter(|_| !own_outline),
        under: copies.into_iter().flatten().collect(),
        effects: fx.and_then(|e| text_effect_paint(e, palette)),
    }))
}

/// A run's text effects resolved for drawing: `None` when none draws anything.
fn text_effect_paint(e: &crate::docx::model::TextEffects, palette: Palette) -> Option<Box<crate::layout::fragment::TextEffectPaint>> {
    use crate::drawing::paint::{EffectList, Paint, resolve_effects, resolve_line};
    let list = EffectList { outer: e.shadow.clone(), glow: e.glow.clone(), reflection: e.reflection.clone(), ..EffectList::default() };
    let fx = resolve_effects(Some(&list), None, palette);
    let outline = e
        .outline
        .as_ref()
        .map(|l| resolve_line(Some(l), None, palette, &Paint::None))
        .filter(|l| l.is_visible() && l.width > 0.0)
        .map(|l| crate::drawing::paint::ResolvedLine { width: l.width.min(MAX_OUTLINE), ..l });
    let p = crate::layout::fragment::TextEffectPaint { outline, shadow: fx.outer, glow: fx.glow.filter(|g| g.0 > 0.0), reflection: fx.reflection };
    (p.outline.is_some() || p.shadow.is_some() || p.glow.is_some() || p.reflection.is_some()).then(|| Box::new(p))
}

// Underline forms
/// Word's dash patterns, on and off lengths in points: the same at every size and weight.
const DOTTED: &[f64] = &[1.44, 1.44];
const DASH: &[f64] = &[3.84, 1.92];
const DASH_LONG: &[f64] = &[7.92, 3.84];
const DOT_DASH: &[f64] = &[2.88, 0.96, 0.96, 0.96];
const DOT_DOT_DASH: &[f64] = &[2.16, 0.72, 0.72, 0.72, 0.72, 0.72];
/// A wave's half period, points (Word's, at every size), and its height per square root of the font size in points
/// (Word's 0.48 pt at 8 pt, 0.72 at 11, 0.96 at 22 and 1.44 at 44).
const WAVE_HALF: f64 = 1.2;
const WAVE_HEIGHT: f64 = 0.217;
/// The most dashes or half waves a span draws; the rest of it is solid. Word's widest page, 22 inches, holds 550 of
/// the shortest dash patterns.
const MAX_MARKS: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq)]
enum UnderlineForm {
    Solid { heavy: bool },
    Double,
    Dashed { pattern: &'static [f64], heavy: bool },
    Wave { heavy: bool, double: bool },
}

impl UnderlineForm {
    /// The form of `w:u` value `v`: `single`, `words` and values Word doesn't define draw a single line.
    fn of(v: &str) -> UnderlineForm {
        use UnderlineForm::*;
        let dashed = |pattern, heavy| Dashed { pattern, heavy };
        match v {
            "double" => Double,
            "thick" => Solid { heavy: true },
            "dotted" => dashed(DOTTED, false),
            "dottedHeavy" => dashed(DOTTED, true),
            "dash" => dashed(DASH, false),
            "dashedHeavy" => dashed(DASH, true),
            "dashLong" => dashed(DASH_LONG, false),
            "dashLongHeavy" => dashed(DASH_LONG, true),
            "dotDash" => dashed(DOT_DASH, false),
            "dashDotHeavy" => dashed(DOT_DASH, true),
            "dotDotDash" => dashed(DOT_DOT_DASH, false),
            "dashDotDotHeavy" => dashed(DOT_DOT_DASH, true),
            "wave" => Wave { heavy: false, double: false },
            "wavyHeavy" => Wave { heavy: true, double: false },
            "wavyDouble" => Wave { heavy: false, double: true },
            _ => Solid { heavy: false },
        }
    }
}

/// A dashed or wavy underline from `x0` to `x1`, centred on `y`, `t` the font's underline thickness.
#[derive(Clone, Copy, Debug)]
struct Span {
    form: UnderlineForm,
    x0: f64,
    x1: f64,
    y: f64,
    t: f64,
    colour: u32,
    copies: Copies,
    size: f64,
    /// Each line drawn as its outline, outside it.
    hollow: bool,
}

impl Span {
    /// Whether `next` carries this span on: drawn alike, starting where it ends.
    fn continues(&self, next: &Span) -> bool {
        self.form == next.form
            && (next.x0 - self.x1).abs() < 0.01
            && (self.y - next.y).abs() < EPS
            && (self.t - next.t).abs() < EPS
            && self.colour == next.colour
            && self.copies == next.copies
            && (self.size - next.size).abs() < EPS
            && self.hollow == next.hollow
    }

    /// One of the span's lines, `rect`: solid, or hollow, a hairline frame around it.
    fn bar(&self, out: &mut Vec<Item>, rect: [f64; 4]) {
        if !self.hollow {
            return lines(out, rect, self.colour, &self.copies);
        }
        let ([x0, y0, x1, y1], h) = (rect, HAIRLINE);
        for side in [[x0 - h, y0 - h, x1 + h, y0], [x0 - h, y1, x1 + h, y1 + h], [x0 - h, y0, x0, y1], [x1, y0, x1 + h, y1]] {
            lines(out, side, self.colour, &self.copies);
        }
    }

    fn draw(&self, out: &mut Vec<Item>) {
        let (x0, x1) = (self.x0, self.x1);
        if ![x0, x1, self.y, self.t, self.size].iter().all(|v| v.is_finite()) {
            return;
        }
        match self.form {
            UnderlineForm::Solid { heavy } => {
                let (y, t) = if heavy { (self.y + self.t / 2.0, 2.0 * self.t) } else { (self.y, self.t) };
                self.bar(out, [x0, y - t / 2.0, x1, y + t / 2.0]);
            }
            UnderlineForm::Double => {
                let t = self.t;
                self.bar(out, [x0, self.y - t / 2.0, x1, self.y + t / 2.0]);
                self.bar(out, [x0, self.y + 1.5 * t, x1, self.y + 2.5 * t]);
            }
            UnderlineForm::Dashed { pattern, heavy } => {
                // Heavy forms are twice as thick, their top where the light form's is.
                let (y, t) = if heavy { (self.y + self.t / 2.0, 2.0 * self.t) } else { (self.y, self.t) };
                let mut at = self.x0;
                for (k, len) in pattern.iter().cycle().enumerate() {
                    if at >= self.x1 - EPS || k > MAX_MARKS {
                        break;
                    }
                    let to = if k >= MAX_MARKS { self.x1 } else { (at + len).min(self.x1) };
                    if k % 2 == 0 || k >= MAX_MARKS {
                        self.bar(out, [at, y - t / 2.0, to, y + t / 2.0]);
                    }
                    at = to;
                }
            }
            UnderlineForm::Wave { heavy, double } => {
                let height = WAVE_HEIGHT * self.size.max(0.0).sqrt();
                // Double: two hairline waves a hairline high, three hairlines apart (Word's 1, 1 and 3 device pixels).
                let waves: &[(f64, f64, f64)] = if double {
                    &[(-1.5 * HAIRLINE, HAIRLINE, HAIRLINE), (1.5 * HAIRLINE, HAIRLINE, HAIRLINE)]
                } else if heavy {
                    &[(0.0, height, 2.0 * HAIRLINE)]
                } else {
                    &[(0.0, height, HAIRLINE)]
                };
                for &(dy, h, w) in waves {
                    for &(cx, cy, tone) in self.copies.iter().flatten() {
                        wave(out, self.x0 + cx, self.x1 + cx, self.y + dy + cy, h, w, tone);
                    }
                    wave(out, self.x0, self.x1, self.y + dy, h, w, self.colour);
                }
            }
        }
    }
}

/// A wave from `x0` to `x1` about `mid`, `height` from crest to trough, stroked `width` wide in `colour`: one cubic
/// per half period from the trough, the last cut where the span ends.
fn wave(out: &mut Vec<Item>, x0: f64, x1: f64, mid: f64, height: f64, width: f64, colour: u32) {
    use crate::drawing::geom::Seg;
    let reach = height / 2.0 + width;
    let (low, high) = (reach + height / 2.0, reach - height / 2.0);
    let len = (x1 - x0).max(0.0);
    let mut segs = vec![Seg::Move([0.0, low])];
    let (mut at, mut up) = (0.0, true);
    while at < len - EPS {
        let (from, to) = if up { (low, high) } else { (high, low) };
        let h = WAVE_HALF.min(len - at);
        if segs.len() > MAX_MARKS {
            segs.push(Seg::Line([len, from]));
            break;
        }
        if h < WAVE_HALF {
            // The half wave's first part, to where it reaches the span's end: its x is `at + WAVE_HALF·f(u)`.
            let f = |u: f64| 1.5 * u * (1.0 - u) + u * u * u;
            let (mut lo, mut hi) = (0.0, 1.0);
            for _ in 0..30 {
                let m = (lo + hi) / 2.0;
                if f(m) * WAVE_HALF < h { lo = m } else { hi = m }
            }
            segs.push(split_cubic([at, from], [at + WAVE_HALF / 2.0, from], [at + WAVE_HALF / 2.0, to], [at + WAVE_HALF, to], lo));
            break;
        }
        segs.push(Seg::Cubic([at + h / 2.0, from], [at + h / 2.0, to], [at + h, to]));
        at += h;
        up = !up;
    }
    crate::layout::table::stroked(out, [x0, mid - reach, x0 + len, mid + reach], true, 2.0 * reach, width, colour, segs);
}

/// The part of the cubic `p0 p1 p2 p3` up to parameter `u`, as a segment from `p0`.
fn split_cubic(p0: [f64; 2], p1: [f64; 2], p2: [f64; 2], p3: [f64; 2], u: f64) -> crate::drawing::geom::Seg {
    let lerp = |a: [f64; 2], b: [f64; 2]| [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u];
    let (a, b, c) = (lerp(p0, p1), lerp(p1, p2), lerp(p2, p3));
    let (d, e) = (lerp(a, b), lerp(b, c));
    crate::drawing::geom::Seg::Cubic(a, d, lerp(d, e))
}

/// `page_colour` tones an effect's copies as the page's text.
pub fn draw_label(text: &str, run: &Run, x_end: f64, y: f64, (page_colour, palette): (Option<u32>, Palette), fonts: &Fonts) -> Vec<Item> {
    let sp = crate::layout::text::shape_run(text, run, fonts);
    let w: f64 = sp.pieces.iter().map(|p| p.width).sum();
    let mut x = x_end - w;
    let mut out = Vec::new();
    if x < 0.0 {
        return out;
    }
    // Pieces alike in face, size and paint are one run, so a gradient spans the whole label.
    let mut run: Option<GlyphRunBuilder> = None;
    for (i, p) in sp.pieces.iter().enumerate() {
        let st = &sp.styles[p.style as usize];
        let Some(face) = &st.face else { continue };
        let scale = face.scale(st.size);
        let paint = glyph_paint(&st.run, st.size, st.run.color, under_text(&st.run, None, page_colour), palette);
        let same = run.as_ref().is_some_and(|r| Rc::ptr_eq(&r.face, face) && r.size == st.size && r.color == st.run.color && r.paint == paint);
        if !same {
            out.extend(run.take().map(|r| Item::Glyphs(r.finish())));
            run = Some(GlyphRunBuilder {
                face: face.clone(),
                size: st.size,
                y,
                glyphs: Vec::new(),
                chars: Vec::new(),
                color: st.run.color,
                para: None,
                x,
                paint,
            });
        }
        let Some(r) = run.as_mut() else { continue };
        let mut gx = x;
        for (id, advance, dx) in sp.glyphs(i) {
            r.glyphs.push((id, gx + dx * scale));
            gx += advance * scale;
        }
        for c in text_of(&sp, p).chars() {
            r.chars.push((c, x, None));
        }
        x += p.width;
    }
    out.extend(run.map(|r| Item::Glyphs(r.finish())));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A document's page and tabs can be any width: a span's marks stay bounded, and the rest of it is solid.
    #[test]
    fn hostile_widths_draw_bounded_underlines() {
        for (form, hollow) in [("dotted", false), ("dotted", true), ("wavyDouble", false)] {
            let form = UnderlineForm::of(form);
            let span = Span { form, x0: 0.0, x1: 1e300, y: 10.0, t: 0.5, colour: 0, copies: [None; 3], size: 11.0, hollow };
            let mut out = Vec::new();
            span.draw(&mut out);
            let marks: usize = out
                .iter()
                .map(|i| match i {
                    Item::Shape(s) => s.geom.paths[0].segs.len(),
                    _ => 1,
                })
                .sum();
            assert!(!out.is_empty() && marks <= 4 * (MAX_MARKS + 3), "{form:?}: {marks}");
        }
    }

    /// A span whose extent isn't a number draws nothing, and every walk ends.
    #[test]
    fn non_finite_widths_draw_nothing() {
        for v in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for (form, hollow) in [("dotted", false), ("dash", true), ("wave", false), ("single", true)] {
                let span = Span { form: UnderlineForm::of(form), x0: 0.0, x1: v, y: 10.0, t: 0.5, colour: 0, copies: [None; 3], size: 11.0, hollow };
                let mut out = Vec::new();
                span.draw(&mut out);
                assert!(out.is_empty(), "{form} to {v}");
                Span { x0: v, x1: 0.0, ..span }.draw(&mut out);
                Span { t: v, x1: 10.0, ..span }.draw(&mut out);
                assert!(out.is_empty(), "{form} at {v}");
            }
        }
    }

    /// A label's digits are one glyph run (one paint to fit a gradient to), as long as they share face, size and paint.
    #[test]
    fn a_labels_pieces_alike_are_one_run() {
        let (theme, map) = (crate::drawing::theme::Theme::default(), crate::drawing::theme::ColourMap::default());
        let palette = Palette { theme: &theme, map: &map };
        let fonts = crate::layout::tests::output::bundled_fonts();
        let Some(doc) = crate::layout::tests::plain_doc(&["x".into()]) else { return };
        let Some(Block::Para(p)) = doc.sections[0].blocks.first() else { unreachable!() };
        let base = p.inlines.iter().find_map(Inline::run).expect("a run");
        let run: Run = Rc::new(RunStyle { outline: true, ..(**base).clone() });
        let items = draw_label("12", &run, 100.0, 20.0, (None, palette), &fonts);
        let [Item::Glyphs(g)] = items.as_slice() else { panic!("{} items", items.len()) };
        assert_eq!(g.chars.iter().map(|c| c.0).collect::<String>(), "12");
    }

    #[test]
    fn a_wave_ends_where_its_span_does() {
        let mut out = Vec::new();
        wave(&mut out, 10.0, 13.0, 5.0, 0.72, 0.5, 0);
        let Item::Shape(s) = &out[0] else { panic!() };
        let crate::drawing::geom::Seg::Cubic(_, _, end) = s.geom.paths[0].segs.last().copied().expect("segments") else { panic!() };
        assert!((s.rect[0] + end[0] - 13.0).abs() < 1e-6, "{end:?}");
    }
}
