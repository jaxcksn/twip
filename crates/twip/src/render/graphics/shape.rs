//! Shape drawing with twip-graphics: fills (solid, gradient, pattern, picture), lines (dashed, compound, inset, with
//! arrowheads) and effects (outer and inner shadows, glow, soft edges, blur). Boolean path
//! operations are done with layers: an inner shadow is the shadow's ring blurred and kept inside the
//! shape with destination-in; a compound or inset line is its bands' coverage (strokes, gaps cleared) under the line's
//! paint, drawn source-in.

use std::collections::HashMap;
use std::rc::Rc;

use twip_graphics::kurbo::{self, Affine, BezPath, ParamCurve, ParamCurveArclen, ParamCurveDeriv, PathEl, PathSeg, Shape};
use twip_graphics::paint::{BlendMode, Brush, Color, ColorFilter, Extend, ImagePaint, Paint, Sampling, Stop};
use twip_graphics::scene::{Clip, Filter, GlyphRun, Layer, Recorder, Synthesis, glyph_run_outline};

use super::{Graphics, blended, pen, rect_path};
use crate::drawing::colour::{Rgba, TWO_STOP_BLEND, linear_to_srgb, srgb_to_linear};
use crate::drawing::geom::{Geom, PathFill, Seg};
use crate::drawing::paint::{
    BlipMode, Cap, Compound, Dash, GradientKind, GradientPath, Join, Paint as Fill, PresetDash, RectAlign, ResolvedBlipAdjust, ResolvedLine,
    ResolvedReflection, ResolvedShadow, TileFlip,
};
use crate::drawing::units::{Emu, Pct};
use crate::layout::fragment::{ImageKey, ShapeItem, TextPathItem};

/// A hatch tile's bit in a PDF, points: a pixel of Word's 300 dpi PDFs. On screen a bit is a device pixel.
const HATCH_BIT: f64 = 0.24;

/// Shapes' outlines as paths, by geometry, and text paths' by their glyphs (each entry holds its key, so its address
/// isn't reused).
#[derive(Default)]
pub(super) struct Paths {
    paths: HashMap<usize, (Rc<Geom>, Rc<Vec<BezPath>>)>,
    text: HashMap<usize, (Rc<TextPathItem>, Rc<Vec<BezPath>>)>,
}

impl Paths {
    fn get(&mut self, g: &Rc<Geom>) -> Rc<Vec<BezPath>> {
        if self.paths.len() > 4096 {
            self.paths.clear();
        }
        let (_, paths) =
            self.paths.entry(Rc::as_ptr(g) as usize).or_insert_with(|| (Rc::clone(g), Rc::new(g.paths.iter().map(|p| path(&p.segs)).collect())));
        Rc::clone(paths)
    }
}

/// A geometry path as a kurbo path. An axis-aligned rect is turned to start at its bottom left and run the other way,
/// the winding Twip has always drawn rects with.
fn path(segs: &[Seg]) -> BezPath {
    let p = |q: [f64; 2]| kurbo::Point::new(q[0], q[1]);
    if let [Seg::Move(a), Seg::Line(b), Seg::Line(c), Seg::Line(d), Seg::Close] = *segs
        && a[1] == b[1]
        && b[0] == c[0]
        && c[1] == d[1]
        && d[0] == a[0]
        && a[0] < b[0]
        && a[1] < c[1]
    {
        let mut out = BezPath::new();
        out.move_to(p(d));
        out.line_to(p(c));
        out.line_to(p(b));
        out.line_to(p(a));
        out.close_path();
        return out;
    }
    let mut out = BezPath::new();
    for s in segs {
        match *s {
            Seg::Move(q) => out.move_to(p(q)),
            Seg::Line(q) => out.line_to(p(q)),
            Seg::Quad(c, q) => out.quad_to(p(c), p(q)),
            Seg::Cubic(c1, c2, q) => out.curve_to(p(c1), p(c2), p(q)),
            Seg::Close => out.close_path(),
        }
    }
    out
}

/// Whether `path` is one axis-aligned rect (four sides, closed).
fn is_rect(path: &BezPath) -> bool {
    let els = path.elements();
    let pts: Vec<kurbo::Point> = match els {
        [PathEl::MoveTo(a), PathEl::LineTo(b), PathEl::LineTo(c), PathEl::LineTo(d), PathEl::ClosePath] => vec![*a, *b, *c, *d],
        [PathEl::MoveTo(a), PathEl::LineTo(b), PathEl::LineTo(c), PathEl::LineTo(d), PathEl::LineTo(e), PathEl::ClosePath] if e == a => {
            vec![*a, *b, *c, *d]
        }
        _ => return false,
    };
    let axis = |p: kurbo::Point, q: kurbo::Point| (p.x == q.x) != (p.y == q.y);
    (0..4).all(|i| axis(pts[i], pts[(i + 1) % 4])) && (pts[0].x == pts[1].x) != (pts[1].x == pts[2].x)
}

/// Where a tiled picture's first tile is and its size, in the shape's box `size` (points): the picture's own size
/// (`natural`, pixels at 96 dpi) scaled, placed by the alignment and moved by the offset.
fn tile_place(natural: (f64, f64), (tx, ty, sx, sy, align): (Emu, Emu, Pct, Pct, RectAlign), size: [f64; 2]) -> (f64, f64, f64, f64) {
    let (tw, th) = (natural.0 * 0.75 * sx.fraction(), natural.1 * 0.75 * sy.fraction());
    let at = |a: u8, room: f64, tile: f64| match a {
        0 => 0.0,
        1 => (room - tile) / 2.0,
        _ => room - tile,
    };
    let (c, r) = match align {
        RectAlign::Tl => (0, 0),
        RectAlign::T => (1, 0),
        RectAlign::Tr => (2, 0),
        RectAlign::L => (0, 1),
        RectAlign::Ctr => (1, 1),
        RectAlign::R => (2, 1),
        RectAlign::Bl => (0, 2),
        RectAlign::B => (1, 2),
        RectAlign::Br => (2, 2),
    };
    (at(c, size[0], tw) + tx.to_pt(), at(r, size[1], th) + ty.to_pt(), tw, th)
}

fn colour(c: Rgba) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

/// The shape's transform from its own box (origin at its top left) to the page: its rotation and flips about its
/// centre.
fn shape_matrix(s: &ShapeItem) -> Affine {
    let [x0, y0, x1, y1] = s.rect;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    Affine::translate((cx, cy))
        * Affine::rotate(s.rot.to_radians())
        * Affine::scale_non_uniform(if s.flip_h { -1.0 } else { 1.0 }, if s.flip_v { -1.0 } else { 1.0 })
        * Affine::translate((-cx + x0, -cy + y0))
}

/// How far the shape and its effects reach, on the page.
fn reach(s: &ShapeItem, m: Affine) -> kurbo::Rect {
    let [x0, y0, x1, y1] = s.rect;
    let e = &s.effects;
    let shadow =
        |sh: &Option<ResolvedShadow>| sh.as_ref().map_or(0.0, |sh| sh.dist * sh.scale[0].abs().max(sh.scale[1].abs()).max(1.0) + 3.0 * sh.blur);
    let pad = s.line.width + shadow(&e.outer) + e.glow.map_or(0.0, |g| 3.0 * g.0) + 3.0 * e.blur.unwrap_or(0.0) + e.soft_edge.unwrap_or(0.0) + 4.0;
    let mut b = m.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, x1 - x0, y1 - y0));
    let mut pad = pad;
    if let Some(r) = &e.reflection {
        let size = [x1 - x0, y1 - y0];
        b = b.union(reflection_matrix(m, size, r).transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, size[0], size[1])));
        pad += 3.0 * r.blur;
    }
    let grow = pad + if e.outer.is_some() { b.width().max(b.height()) } else { 0.0 };
    b.inflate(grow, grow)
}

/// The alignment point of `align` in the box `b`.
fn pivot(b: [f64; 4], align: RectAlign) -> (f64, f64) {
    let (xm, ym) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    match align {
        RectAlign::Tl => (b[0], b[1]),
        RectAlign::T => (xm, b[1]),
        RectAlign::Tr => (b[2], b[1]),
        RectAlign::L => (b[0], ym),
        RectAlign::Ctr => (xm, ym),
        RectAlign::R => (b[2], ym),
        RectAlign::Bl => (b[0], b[3]),
        RectAlign::B => (xm, b[3]),
        RectAlign::Br => (b[2], b[3]),
    }
}

/// A shadow's own transform about its alignment point in `b`: skew, then scale.
pub(super) fn align_matrix(b: [f64; 4], sh: &ResolvedShadow) -> Affine {
    let (ax, ay) = pivot(b, sh.align);
    let (kx, ky) = (sh.skew[0].to_radians().tan(), sh.skew[1].to_radians().tan());
    Affine::translate((ax, ay))
        * Affine::new([1.0, ky, kx, 1.0, 0.0, 0.0])
        * Affine::scale_non_uniform(sh.scale[0], sh.scale[1])
        * Affine::translate((-ax, -ay))
}

/// A reflection's transform from the box of `size` (the shape's, placed by `m`) to the page: skewed, then scaled about
/// its alignment point, then moved `dist` along `dir`. The point and the move are the shape's own when it turns with
/// the shape, else the page's.
fn reflection_matrix(m: Affine, size: [f64; 2], r: &ResolvedReflection) -> Affine {
    let (dx, dy) = (r.dist * r.dir.to_radians().cos(), r.dist * r.dir.to_radians().sin());
    let about = |b: [f64; 4]| {
        let (ax, ay) = pivot(b, r.align);
        let (kx, ky) = (r.skew[0].to_radians().tan(), r.skew[1].to_radians().tan());
        Affine::translate((ax, ay))
            * Affine::scale_non_uniform(r.scale[0], r.scale[1])
            * Affine::new([1.0, ky, kx, 1.0, 0.0, 0.0])
            * Affine::translate((-ax, -ay))
    };
    if r.rot_with_shape {
        m * Affine::translate((dx, dy)) * about([0.0, 0.0, size[0], size[1]])
    } else {
        let b = m.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, size[0], size[1]));
        Affine::translate((dx, dy)) * about([b.x0, b.y0, b.x1, b.y1]) * m
    }
}

/// Draws a reflection of what `draw` records in a box of `size`, placed by `m` (as the shape's own drawing is):
/// the copy under `r`'s transform, blurred, and faded by a gradient alpha along its fade direction, which turns and
/// flips with the shape when the reflection does. A shape's reflection and a text run's use it alike.
pub(super) fn reflect(rec: &mut Recorder, m: Affine, size: [f64; 2], r: &ResolvedReflection, draw: &mut dyn FnMut(&mut Recorder)) {
    let t = reflection_matrix(m, size, r);
    let bbox = t.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, size[0], size[1]));
    let (fx, fy) = (r.fade_dir.to_radians().cos(), r.fade_dir.to_radians().sin());
    let [a, b, c, d, ..] = m.as_coeffs();
    let (dx, dy) = if r.rot_with_shape { (a * fx + c * fy, b * fx + d * fy) } else { (fx, fy) };
    let n = dx.hypot(dy).max(f64::EPSILON);
    let (dx, dy) = (dx / n, dy / n);
    let len = bbox.width() * dx.abs() + bbox.height() * dy.abs();
    if len <= 0.0 || !len.is_finite() {
        return;
    }
    let c = bbox.center();
    let (start, end) = (kurbo::Point::new(c.x - dx * len / 2.0, c.y - dy * len / 2.0), kurbo::Point::new(c.x + dx * len / 2.0, c.y + dy * len / 2.0));
    // The fade holds its start alpha before its start and its end alpha after its end.
    let ((p0, a0), (p1, a1)) = if r.start.0 <= r.end.0 { (r.start, r.end) } else { (r.end, r.start) };
    let at = |p: f64, a: f64| Stop { offset: p as f32, color: Color::rgba(0, 0, 0, (a * 255.0).round() as u8) };
    let mut stops = [at(0.0, a0); 4];
    let mut n = 0;
    for (p, a, keep) in [(0.0, a0, p0 > 0.0), (p0, a0, true), (p1, a1, true), (1.0, a1, p1 < 1.0)] {
        if keep {
            stops[n] = at(p, a);
            n += 1;
        }
    }
    let sigma = sigma(r.blur);
    let filters = [Filter::Blur { sigma }];
    let bounds = bbox.inflate(f64::from(sigma) * 3.0 + 2.0, f64::from(sigma) * 3.0 + 2.0);
    rec.push_layer(&Layer { filters: if sigma > 0.0 { &filters } else { &[] }, bounds: Some(bounds), ..Layer::default() });
    rec.push_transform(t);
    draw(rec);
    rec.pop();
    rec.push_layer(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
    rec.fill_rect(bounds, Paint::new(Brush::Linear { start, end, stops: &stops[..n], extend: Extend::Clamp, transform: Affine::IDENTITY }));
    rec.pop();
    rec.pop();
}

/// A layer that blurs what is drawn in it by `sigma` (none at 0).
pub(super) fn blur_layer(rec: &mut Recorder, sigma: f32, opacity: f32, bounds: Option<kurbo::Rect>) {
    let filters = [Filter::Blur { sigma }];
    rec.push_layer(&Layer { opacity, filters: if sigma > 0.0 { &filters } else { &[] }, bounds, ..Layer::default() });
}

/// The picture adjustments as colour filters in the order they apply, and the opacity they leave.
fn picture_adjust(adjust: &[ResolvedBlipAdjust]) -> (Vec<ColorFilter>, f32) {
    const L: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let gray: [f32; 20] = [L[0], L[1], L[2], 0.0, 0.0, L[0], L[1], L[2], 0.0, 0.0, L[0], L[1], L[2], 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let (mut filters, mut alpha) = (Vec::new(), 1.0f32);
    for a in adjust {
        match a {
            ResolvedBlipAdjust::Alpha(v) => alpha *= *v as f32,
            ResolvedBlipAdjust::Grayscale => filters.push(ColorFilter::Matrix(gray)),
            ResolvedBlipAdjust::Lum { bright, contrast } => {
                // Contrast scales about mid grey; brightness adds to it, scaled by the same slope's share of the range left.
                let k = if *contrast > 0.0 { 1.0 / (1.0 - contrast.min(0.999)) } else { (1.0 + contrast).max(0.0) };
                let o = (0.5 * (1.0 - k) + bright * (0.5 + 0.5 * k)) as f32;
                let k = k as f32;
                filters.push(ColorFilter::Matrix([k, 0.0, 0.0, 0.0, o, 0.0, k, 0.0, 0.0, o, 0.0, 0.0, k, 0.0, o, 0.0, 0.0, 0.0, 1.0, 0.0]));
            }
            // Drawn on the image's pixels.
            ResolvedBlipAdjust::ClrChange { .. } => {}
            ResolvedBlipAdjust::BiLevel(t) => {
                let table: [u8; 256] = std::array::from_fn(|i| if i as f64 / 255.0 >= *t { 255 } else { 0 });
                filters.push(ColorFilter::Matrix(gray));
                filters.push(ColorFilter::Table(table));
            }
            ResolvedBlipAdjust::Duotone([c0, c1]) => {
                let (a, b) = ([c0.r, c0.g, c0.b].map(|v| f32::from(v) / 255.0), [c1.r, c1.g, c1.b].map(|v| f32::from(v) / 255.0));
                let row = |i: usize| [(b[i] - a[i]) * L[0], (b[i] - a[i]) * L[1], (b[i] - a[i]) * L[2], 0.0, a[i]];
                let (r0, r1, r2) = (row(0), row(1), row(2));
                filters.push(ColorFilter::Matrix([
                    r0[0], r0[1], r0[2], r0[3], r0[4], r1[0], r1[1], r1[2], r1[3], r1[4], r2[0], r2[1], r2[2], r2[3], r2[4], 0.0, 0.0, 0.0, 1.0, 0.0,
                ]));
            }
        }
    }
    (filters, alpha.clamp(0.0, 1.0))
}

/// A line's pen at width `w`: its caps, join and dashes. A capped dash's dashes are one width shorter and its gaps one
/// width longer, since the caps grow each dash by half a width at either end.
pub(super) fn line_pen(line: &ResolvedLine, w: f64) -> kurbo::Stroke {
    let cap = match line.cap {
        Cap::Round => kurbo::Cap::Round,
        Cap::Square => kurbo::Cap::Square,
        Cap::Flat => kurbo::Cap::Butt,
    };
    let mut s = kurbo::Stroke::new(w).with_caps(cap);
    s = match line.join {
        Join::Round => s.with_join(kurbo::Join::Round),
        Join::Bevel => s.with_join(kurbo::Join::Bevel),
        Join::Miter(lim) => s.with_join(kurbo::Join::Miter).with_miter_limit(lim.fraction()),
    };
    let lengths: Vec<f64> = match &line.dash {
        Dash::Preset(p) => dash_lengths(*p).to_vec(),
        Dash::Custom(ds) => ds.iter().flat_map(|(d, sp)| [d.fraction(), sp.fraction()]).collect(),
    };
    if !lengths.is_empty() && w > 0.0 {
        let capped = line.cap != Cap::Flat;
        let adjust = |i: usize| {
            if !capped {
                0.0
            } else if i.is_multiple_of(2) {
                -1.0
            } else {
                1.0
            }
        };
        s = s.with_dashes(0.0, lengths.iter().enumerate().map(|(i, l)| ((l + adjust(i)) * w).max(0.0)));
    }
    s
}

/// The filled outline of `path` stroked with `line`'s pen at width `w`.
fn outline(path: &BezPath, line: &ResolvedLine, w: f64) -> BezPath {
    kurbo::stroke(path.elements().iter().copied(), &line_pen(line, w), &kurbo::StrokeOpts::default(), 0.01)
}

/// Whether `path`'s last contour is closed.
fn last_closed(path: &BezPath) -> bool {
    path.elements().iter().rev().find(|e| !matches!(e, PathEl::MoveTo(_))).is_some_and(|e| matches!(e, PathEl::ClosePath))
}

/// `path`'s contours as segments, each with its length.
fn contours(path: &BezPath) -> Vec<(Vec<PathSeg>, f64)> {
    let mut out: Vec<(Vec<PathSeg>, f64)> = Vec::new();
    let (mut start, mut at) = (kurbo::Point::ZERO, kurbo::Point::ZERO);
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(p) => {
                (start, at) = (p, p);
                out.push((Vec::new(), 0.0));
                continue;
            }
            PathEl::LineTo(p) => PathSeg::Line(kurbo::Line::new(at, p)),
            PathEl::QuadTo(c, p) => PathSeg::Quad(kurbo::QuadBez::new(at, c, p)),
            PathEl::CurveTo(c1, c2, p) => PathSeg::Cubic(kurbo::CubicBez::new(at, c1, c2, p)),
            PathEl::ClosePath if at != start => PathSeg::Line(kurbo::Line::new(at, start)),
            PathEl::ClosePath => continue,
        };
        at = seg.end();
        if let (Some((segs, len)), Some(accuracy)) = (out.last_mut(), crate::render::arclen_accuracy(&seg, 1e-4)) {
            *len += seg.arclen(accuracy);
            segs.push(seg);
        }
    }
    out.retain(|(segs, len)| !segs.is_empty() && *len > 0.0);
    out
}

/// The part of a contour from `from` to `to` along it, appended to `out` from a move.
fn segment(segs: &[PathSeg], from: f64, to: f64, out: &mut BezPath) {
    let mut acc = 0.0;
    let mut started = false;
    for seg in segs {
        let Some(accuracy) = crate::render::arclen_accuracy(seg, 1e-4) else { continue };
        let len = seg.arclen(accuracy);
        let (a, b) = ((from - acc).max(0.0), (to - acc).min(len));
        acc += len;
        if b <= a || len <= 0.0 {
            continue;
        }
        let t = |d: f64| if d >= len { 1.0 } else { seg.inv_arclen(d, accuracy) };
        let part = seg.subsegment(t(a)..t(b));
        if !started {
            out.move_to(part.start());
            started = true;
        }
        match part {
            PathSeg::Line(l) => out.line_to(l.p1),
            PathSeg::Quad(q) => out.quad_to(q.p1, q.p2),
            PathSeg::Cubic(c) => out.curve_to(c.p1, c.p2, c.p3),
        }
    }
}

/// A segment's direction at `t`, as a unit vector.
fn direction(seg: &PathSeg, t: f64) -> kurbo::Vec2 {
    let d = match seg {
        PathSeg::Line(l) => l.p1 - l.p0,
        PathSeg::Quad(q) => q.deriv().eval(t).to_vec2(),
        PathSeg::Cubic(c) => c.deriv().eval(t).to_vec2(),
    };
    d / d.hypot().max(f64::EPSILON)
}

impl Graphics {
    /// The shape's outlines, one per geometry path: its geometry's, or for a text path its glyphs stretched to their
    /// box (the geometry then has the one path).
    fn outlines(&mut self, s: &ShapeItem) -> Rc<Vec<BezPath>> {
        let Some(t) = &s.text_path else { return self.paths.get(&s.geom) };
        let key = Rc::as_ptr(t) as usize;
        if let Some((_, p)) = self.paths.text.get(&key) {
            return Rc::clone(p);
        }
        if self.paths.text.len() > 256 {
            self.paths.text.clear();
        }
        let p = Rc::new(vec![self.text_outline(t)]);
        self.paths.text.insert(key, (Rc::clone(t), Rc::clone(&p)));
        p
    }

    /// A text path's glyph outlines (synthetic styles applied), stretched so that their ink fills its box.
    fn text_outline(&mut self, t: &TextPathItem) -> BezPath {
        let [x0, y0, x1, y1] = t.fit;
        let mut out = BezPath::new();
        if !(x1 > x0 && y1 > y0) {
            return out;
        }
        // Outlined at about the size they are drawn, for synthetic bold's width.
        let size = y1 - y0;
        let mut placed = Vec::new();
        let mut ring = kurbo::StrokeCtx::default();
        for (face, glyphs) in t.runs.iter() {
            let Some(font) = self.font(face) else { continue };
            placed.clear();
            placed.extend(glyphs.iter().map(|&(g, x)| (g, x * size)));
            let synthesis = Synthesis { embolden: face.sim_bold, skew_x: if face.sim_italic { -0.2 } else { 0.0 } };
            glyph_run_outline(&self.res.fonts, &GlyphRun { font, size, baseline: 0.0, glyphs: &placed, synthesis }, &mut out, &mut ring);
        }
        let b = out.bounding_box();
        if !(b.width() > 0.0 && b.height() > 0.0) {
            return BezPath::new();
        }
        out.apply_affine(
            Affine::translate((x0, y0))
                * Affine::scale_non_uniform((x1 - x0) / b.width(), (y1 - y0) / b.height())
                * Affine::translate((-b.x0, -b.y0)),
        );
        out
    }

    /// The page background's fill over a `size` page (in its points), when it has one: drawn as a shape's fill is.
    pub(super) fn page_fill(&mut self, rec: &mut Recorder, r: &super::Renderer, size: [f64; 2], images: &[Vec<u8>]) {
        let Some(fill) = &r.page_fill else { return };
        let mut page = BezPath::new();
        rect_path(kurbo::Rect::new(0.0, 0.0, size[0], size[1])).into_iter().for_each(|e| page.push(e));
        self.fill(rec, &page, fill, PathFill::Norm, size, 0.0, [0.0, 0.0], images);
    }

    pub(super) fn shape(&mut self, rec: &mut Recorder, s: &ShapeItem, images: &[Vec<u8>]) {
        self.shape_with_text(rec, s, images, None);
    }

    /// [`Graphics::shape`], its reflection holding what `text` draws too (given the transform from the page to the
    /// shape's own box): a shape's text is reflected with it.
    pub(super) fn shape_with_text(&mut self, rec: &mut Recorder, s: &ShapeItem, images: &[Vec<u8>], mut text: Option<ReflectedText>) {
        let [x0, y0, x1, y1] = s.rect;
        let m = shape_matrix(s);
        let e = &s.effects;
        let area = reach(s, m);
        let blur = e.blur.filter(|r| *r > 0.0);
        if let Some(r) = blur {
            blur_layer(rec, sigma(r), 1.0, Some(area));
        }
        let soft = e.soft_edge.filter(|r| *r > 0.0);
        if let Some(sh) = &e.outer {
            self.outer_shadow(rec, s, m, sh, soft, area);
        }
        if let Some(r) = &e.reflection {
            // An inline picture's reflection ends where its paragraph does.
            let clip = s.clip_bottom.map(|y| kurbo::Rect::new(area.x0, area.y0, area.x1, y.clamp(area.y0, area.y1)));
            if let Some(c) = clip {
                rec.push_clip(Clip::Path(&rect_path(c)));
            }
            reflect(rec, m, [x1 - x0, y1 - y0], r, &mut |rec| {
                self.body(rec, s, images);
                if let (Some(text), Some(back)) = (text.as_mut(), m.inverse().into()) {
                    text(self, rec, back);
                }
            });
            if clip.is_some() {
                rec.pop();
            }
        }
        if soft.is_some() {
            rec.push_layer(&Layer { bounds: Some(area), ..Layer::default() });
        }
        if let Some((r, c)) = e.glow.filter(|g| g.0 > 0.0) {
            self.glow(rec, s, m, r, c);
        }
        rec.push_transform(m);
        self.body(rec, s, images);
        if let Some(sh) = &e.inner {
            self.inner_shadow(rec, s, sh);
        }
        rec.pop();
        if let Some(r) = soft {
            self.soft_edge(rec, s, m, r, area);
            rec.pop();
        }
        if blur.is_some() {
            rec.pop();
        }
    }

    /// The shape's fill and line in its own box.
    fn body(&mut self, rec: &mut Recorder, s: &ShapeItem, images: &[Vec<u8>]) {
        let [x0, y0, x1, y1] = s.rect;
        let paths = self.outlines(s);
        let line = if s.picture { ResolvedLine { width: s.line.width * 2.0, ..s.line.clone() } } else { s.line.clone() };
        let (size, origin) = ([x1 - x0, y1 - y0], [x0, y0]);
        for (gp, p) in s.geom.paths.iter().zip(paths.iter()) {
            if gp.stroke && s.picture {
                self.stroke(rec, p, &line, size, s.rot, origin, images);
            }
            if gp.fill != PathFill::None && s.snap {
                self.shade(rec, p, &s.fill, false);
            } else if gp.fill != PathFill::None {
                self.fill(rec, p, &s.fill, gp.fill, size, s.rot, origin, images);
            }
            if gp.stroke && !s.picture {
                self.stroke(rec, p, &line, size, s.rot, origin, images);
            }
        }
    }

    /// The shape's filled and stroked parts in `c`, each blurred by `sigma` on its own when it is over 0 (as a
    /// mask-filtered paint blurs each draw).
    fn silhouette(&mut self, rec: &mut Recorder, s: &ShapeItem, c: Color, sigma: f32) {
        let paths = self.outlines(s);
        let one = |rec: &mut Recorder, draw: &dyn Fn(&mut Recorder, Color)| {
            if sigma > 0.0 {
                blur_layer(rec, sigma, f32::from(c.a) / 255.0, None);
                draw(rec, c.with_alpha(255));
                rec.pop();
            } else {
                draw(rec, c);
            }
        };
        for (gp, p) in s.geom.paths.iter().zip(paths.iter()) {
            if gp.fill != PathFill::None && !s.fill.is_none() {
                one(rec, &|rec, c| rec.fill_path(p.elements(), c));
            }
            if gp.stroke && !s.line.paint.is_none() {
                let w = s.line.width.max(0.01);
                one(rec, &|rec, c| rec.stroke_path(p.elements(), &pen(w), c));
            }
        }
    }

    fn outer_shadow(&mut self, rec: &mut Recorder, s: &ShapeItem, m: Affine, sh: &ResolvedShadow, soft: Option<f64>, area: kurbo::Rect) {
        let mut c = colour(sh.colour);
        let blur = if sh.blur > 0.0 && soft.is_none() { sigma(sh.blur) } else { 0.0 };
        if let Some(r) = soft {
            let fade = f64::from(sigma(r * SOFT_FADE));
            let total = (fade * fade + f64::from(sigma(sh.blur)).powi(2)).sqrt() as f32;
            let filters = [Filter::Erode { radius: (r * SOFT_ERODE) as f32 }, Filter::Blur { sigma: total }];
            rec.push_layer(&Layer { filters: &filters, bounds: Some(area), ..Layer::default() });
        }
        let (dx, dy) = (sh.dist * sh.dir.to_radians().cos(), sh.dist * sh.dir.to_radians().sin());
        let [x0, y0, x1, y1] = s.rect;
        let translucent = sh.colour.a < 255 && soft.is_none();
        if translucent {
            rec.push_layer(&Layer { opacity: f32::from(sh.colour.a) / 255.0, bounds: Some(area), ..Layer::default() });
            c = c.with_alpha(255);
        }
        let t = if sh.rot_with_shape {
            m * Affine::translate((dx, dy)) * align_matrix([0.0, 0.0, x1 - x0, y1 - y0], sh)
        } else {
            let b = m.transform_rect_bbox(kurbo::Rect::new(0.0, 0.0, x1 - x0, y1 - y0));
            Affine::translate((dx, dy)) * align_matrix([b.x0, b.y0, b.x1, b.y1], sh) * m
        };
        rec.push_transform(t);
        self.silhouette(rec, s, c, blur);
        rec.pop();
        if translucent {
            rec.pop();
        }
        if soft.is_some() {
            rec.pop();
        }
    }

    fn glow(&mut self, rec: &mut Recorder, s: &ShapeItem, m: Affine, r: f64, c: Rgba) {
        rec.push_transform(m);
        let paths = self.outlines(s);
        let c = colour(c);
        for (gp, p) in s.geom.paths.iter().zip(paths.iter()) {
            let filled = gp.fill != PathFill::None && !s.fill.is_none();
            let lined = gp.stroke && !s.line.paint.is_none();
            if !(filled || lined) {
                continue;
            }
            // Stroke and fill as one coverage, blurred, then at the colour's alpha.
            blur_layer(rec, sigma(r * GLOW_SOFT), f32::from(c.a) / 255.0, None);
            let w = 2.0 * r * GLOW_GROW + if lined { s.line.width } else { 0.0 };
            let opaque = c.with_alpha(255);
            if filled {
                rec.fill_path(p.elements(), opaque);
            }
            rec.stroke_path(p.elements(), &pen(w).with_join(kurbo::Join::Round), opaque);
            rec.pop();
        }
        rec.pop();
    }

    /// The shadow inside the shape: a ring (the shape's box, grown, less the shape) moved against the shadow's
    /// direction, blurred, and kept inside the shape. In the shape's own box.
    fn inner_shadow(&mut self, rec: &mut Recorder, s: &ShapeItem, sh: &ResolvedShadow) {
        let paths = self.outlines(s);
        let inside: Vec<&BezPath> = s.geom.paths.iter().zip(paths.iter()).filter(|(gp, _)| gp.fill != PathFill::None).map(|(_, p)| p).collect();
        if inside.is_empty() || s.fill.is_none() {
            return;
        }
        let [x0, y0, x1, y1] = s.rect;
        let pad = sh.dist + 3.0 * sh.blur + 1.0;
        let outer = kurbo::Rect::new(-pad, -pad, x1 - x0 + pad, y1 - y0 + pad);
        let (dx, dy) = (sh.dist * sh.dir.to_radians().cos(), sh.dist * sh.dir.to_radians().sin());
        rec.push_layer(&Layer::default());
        blur_layer(rec, if sh.blur > 0.0 { sigma(sh.blur) } else { 0.0 }, 1.0, None);
        rec.push_transform(Affine::translate((-dx, -dy)));
        rec.fill_rect(outer, colour(sh.colour));
        for p in &inside {
            rec.fill_path(p.elements(), blended(Color::BLACK, BlendMode::Clear));
        }
        rec.pop();
        rec.pop();
        rec.push_layer(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
        for p in &inside {
            rec.fill_path(p.elements(), Color::BLACK);
        }
        rec.pop();
        rec.pop();
    }

    fn soft_edge(&mut self, rec: &mut Recorder, s: &ShapeItem, m: Affine, r: f64, area: kurbo::Rect) {
        let filters = [Filter::Blur { sigma: sigma(r * SOFT_FADE) }];
        rec.push_layer(&Layer { blend: BlendMode::DstIn, filters: &filters, bounds: Some(area), ..Layer::default() });
        rec.push_transform(m);
        self.silhouette(rec, s, Color::BLACK, 0.0);
        let w = r * SOFT_ERODE * 2.0 + if s.line.paint.is_none() { 0.0 } else { s.line.width };
        let clear = pen(w).with_join(kurbo::Join::Round);
        for p in self.outlines(s).iter() {
            rec.stroke_path(p.elements(), &clear, blended(Color::BLACK, BlendMode::Clear));
        }
        rec.pop();
        rec.pop();
    }

    /// The picture `key` names with its pixels within one level of `from` in every channel drawn as `to`, made on first
    /// use: the decoded pixels changed in place and handed to the registry, so a large picture costs what drawing it
    /// does (twip-graphics reads at most 2^28 pixels). A picture that doesn't decode is the picture as it is. They are
    /// kept by the registry's id of the picture, which no other document's picture shares, and dropped to
    /// [`RECOLOUR_MAX_BYTES`] between frames by [`Graphics::trim_recoloured`].
    pub(super) fn recoloured(&mut self, key: ImageKey, from: Rgba, to: Rgba, images: &[Vec<u8>]) -> Option<twip_graphics::image::ImageId> {
        let base = self.image(key, images)?;
        let info = self.res.images.info(base)?;
        let k = (base, [from.r, from.g, from.b, from.a], [to.r, to.g, to.b, to.a]);
        let frame = self.frames;
        if let Some(r) = self.recoloured.get_mut(&k) {
            r.last = frame;
            if let Some(id) = r.id {
                self.recolour_log.push(id);
            }
            return r.id.or(Some(base));
        }
        self.recolour_decodes += 1;
        let made =
            key.index.and_then(|i| images.get(i)).and_then(|bytes| twip_graphics::image::pixels_straight(bytes).ok()).and_then(|(w, h, mut px)| {
                let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
                for p in px.as_chunks_mut::<4>().0 {
                    if p[3] != 0 && near(p[0], from.r) && near(p[1], from.g) && near(p[2], from.b) {
                        p.copy_from_slice(&[to.r, to.g, to.b, to.a]);
                    }
                }
                self.res.images.add_pixels(w, h, px).ok()
            });
        let bytes = if made.is_some() { u64::from(info.width) * u64::from(info.height) * 4 } else { 0 };
        self.recoloured_bytes += bytes;
        self.recolour_log.extend(made);
        self.recoloured.insert(k, Recoloured { id: made, bytes, last: frame });
        made.or(Some(base))
    }

    /// Starts a frame: pictures with a changed colour past `max` bytes are dropped, the ones neither drawn in the frame
    /// before nor held by a page recording kept for later first. What is in use is never dropped, so a view of more than
    /// `max` bytes keeps them all and decodes nothing again.
    pub(super) fn trim_recoloured(&mut self, max: u64) {
        self.frames += 1;
        self.recolour_log.clear();
        if self.recoloured_bytes <= max {
            return;
        }
        let held: std::collections::HashSet<_> = self.retained.pinned().collect();
        let frame = self.frames;
        let res = &mut self.res;
        let mut bytes = self.recoloured_bytes;
        self.recoloured.retain(|_, r| {
            let Some(id) = r.id else { return false };
            if bytes <= max || r.last + 1 >= frame || held.contains(&id) {
                return true;
            }
            res.images.remove(id);
            bytes -= r.bytes;
            false
        });
        self.recoloured_bytes = bytes;
    }

    /// Shading's fill; `aa` false: aliased as Word draws it, so fills meeting at a fractional pixel (spaced rows, a
    /// hatch beside a fill) share the pixel instead of each covering part of it, which leaves a pale seam. A hatch's
    /// bits are device pixels, as Word draws them at any zoom, so its stripes stay visible zoomed out.
    fn shade(&mut self, rec: &mut Recorder, path: &BezPath, fill: &Fill, aa: bool) {
        let paint = match fill {
            Fill::Solid(c) => Paint::from(colour(*c)),
            Fill::Hatch { stripe, fg, bg } => {
                let Some(image) = self.tile(crate::drawing::patterns::stripe(*stripe), *fg, *bg) else { return };
                let transform = Affine::scale(HATCH_BIT) * Affine::translate((-1.0, -1.0));
                Paint::new(Brush::Pattern { image, transform, sampling: Sampling::Pixel })
            }
            _ => return,
        };
        rec.fill_path(path.elements(), Paint { aa, ..paint });
    }

    /// The 8×8 tile of a pattern fill, registered on first use.
    pub(super) fn tile(&mut self, bits: [u8; 8], fg: Rgba, bg: Rgba) -> Option<twip_graphics::image::ImageId> {
        let key = (bits, [fg.r, fg.g, fg.b, fg.a], [bg.r, bg.g, bg.b, bg.a]);
        let res = &mut self.res;
        *self.tiles.entry(key).or_insert_with(|| {
            let mut px = Vec::with_capacity(64 * 4);
            for row in bits {
                for i in 0..8 {
                    let c = if row & (0x80 >> i) != 0 { fg } else { bg };
                    px.extend([c.r, c.g, c.b, c.a]);
                }
            }
            res.images.add_pixels(8, 8, px).ok()
        })
    }

    #[allow(clippy::too_many_arguments, reason = "the fill, the path it fills, and the shape's box and turn")]
    fn fill(
        &mut self,
        rec: &mut Recorder,
        path: &BezPath,
        fill: &Fill,
        mode: PathFill,
        size: [f64; 2],
        rot: f64,
        origin: [f64; 2],
        images: &[Vec<u8>],
    ) {
        match fill {
            Fill::None => {}
            Fill::Solid(c) => rec.fill_path(path.elements(), colour(shaded(*c, mode))),
            Fill::Gradient { stops, kind, rot_with_shape, tile, flip } => {
                let stops: Vec<(f64, Rgba)> = stops.iter().map(|(p, c)| (*p, shaded(*c, mode))).collect();
                gradient(rec, path, &stops, *kind, size, if *rot_with_shape { 0.0 } else { rot }, (tile, *flip));
            }
            Fill::Pattern { preset, fg, bg } => {
                let bits = crate::drawing::patterns::tile(*preset);
                if let Some(image) = self.tile(bits, shaded(*fg, mode), shaded(*bg, mode)) {
                    let transform = Affine::translate((-origin[0], -origin[1])) * Affine::scale(0.75);
                    rec.fill_path(path.elements(), Paint::new(Brush::Pattern { image, transform, sampling: Sampling::Nearest }));
                }
            }
            Fill::Hatch { .. } => self.shade(rec, path, fill, true),
            Fill::Picture { image, src, mode: BlipMode::Stretch(f), .. }
                if image.and_then(|i| images.get(i)).is_some_and(|b| crate::docx::image::vector(b).is_some()) =>
            {
                // A vector picture: the box the stretch fills, the crop cut from it, clipped to the shape.
                let f = f.map(|v| v.fraction());
                let (w, h) = (size[0], size[1]);
                let dst = kurbo::Rect::new(f[0] * w, f[1] * h, w - f[2] * w, h - f[3] * h);
                rec.push_clip(Clip::Path(path.elements()));
                self.vector(rec, ImageKey { index: *image, generation: 0 }, images, dst, src.map(|v| v.fraction()));
                rec.pop();
            }
            Fill::Picture { image, src, mode: BlipMode::Tile { tx, ty, sx, sy, flip, align }, .. }
                if image.and_then(|i| images.get(i)).is_some_and(|b| crate::docx::image::vector(b).is_some()) =>
            {
                // A vector picture tiled: at its own size (an SVG without one fills the box), each tile its picture.
                let bytes = image.and_then(|i| images.get(i)).map_or(&[][..], Vec::as_slice);
                let natural = match crate::docx::image::vector(bytes) {
                    Some(crate::docx::image::Vector::Emf(s) | crate::docx::image::Vector::Wmf(s)) => Some((s.0 / 0.75, s.1 / 0.75)),
                    _ => crate::render::svg::natural_size(bytes),
                };
                let (iw, ih) = natural.unwrap_or((size[0] / 0.75, size[1] / 0.75));
                let (ox, oy, tw, th) = tile_place((iw, ih), (*tx, *ty, *sx, *sy, *align), size);
                let key = ImageKey { index: *image, generation: 0 };
                rec.push_clip(Clip::Path(path.elements()));
                self.vector_tiles(rec, key, images, (ox, oy, tw, th), src.map(|v| v.fraction()), *flip, path.bounding_box());
                rec.pop();
            }
            Fill::Picture { image, src, mode: blip, adjust, .. } => {
                let (filters, alpha) = picture_adjust(adjust);
                let key = ImageKey { index: *image, generation: 0 };
                let change = adjust.iter().find_map(|a| match a {
                    ResolvedBlipAdjust::ClrChange { from, to } => Some((*from, *to)),
                    _ => None,
                });
                let id = match change {
                    Some((from, to)) => self.recoloured(key, from, to, images),
                    None => self.image(key, images),
                };
                let Some((id, info)) = id.and_then(|id| Some((id, self.res.images.info(id)?))) else {
                    // A picture without an image twip-graphics reads: a light frame, a CSS pixel wide at 100%.
                    rec.stroke_path(path.elements(), &pen(0.75), Paint { aa: false, ..Paint::from(Color::rgb(0xC8_C8C8)) });
                    return;
                };
                let (iw, ih) = (f64::from(info.width), f64::from(info.height));
                let sr = [src[0].fraction() * iw, src[1].fraction() * ih, iw - src[2].fraction() * iw, ih - src[3].fraction() * ih];
                let fr = match blip {
                    BlipMode::Stretch(f) => [f[0].fraction(), f[1].fraction(), f[2].fraction(), f[3].fraction()],
                    BlipMode::Tile { tx, ty, sx, sy, align, .. } => {
                        let (ox, oy, tw, th) = tile_place((iw, ih), (*tx, *ty, *sx, *sy, *align), size);
                        let transform = Affine::translate((ox, oy)) * Affine::scale_non_uniform(tw / iw, th / ih);
                        // A pattern brush has no colour filters: the layer carries them, and the opacity.
                        let layered = !filters.is_empty() || alpha < 1.0;
                        if layered {
                            rec.push_layer(&Layer { opacity: alpha, color_filters: &filters, ..Layer::default() });
                        }
                        rec.fill_path(path.elements(), Paint::new(Brush::Pattern { image: id, transform, sampling: Sampling::Linear }));
                        if layered {
                            rec.pop();
                        }
                        return;
                    }
                };
                let dst = [fr[0] * size[0], fr[1] * size[1], size[0] - fr[2] * size[0], size[1] - fr[3] * size[1]];
                let (sw, sh) = ((sr[2] - sr[0]).max(1e-9), (sr[3] - sr[1]).max(1e-9));
                let at = |v: f64, lo: f64, span: f64, d0: f64, d1: f64| d0 + (v - lo) / span * (d1 - d0);
                let (cx0, cy0, cx1, cy1) = (sr[0].max(0.0), sr[1].max(0.0), sr[2].min(iw), sr[3].min(ih));
                if cx1 <= cx0 || cy1 <= cy0 {
                    return;
                }
                let part = kurbo::Rect::new(cx0, cy0, cx1, cy1);
                let to = kurbo::Rect::new(
                    at(cx0, sr[0], sw, dst[0], dst[2]),
                    at(cy0, sr[1], sh, dst[1], dst[3]),
                    at(cx1, sr[0], sw, dst[0], dst[2]),
                    at(cy1, sr[1], sh, dst[1], dst[3]),
                );
                let mut all = filters;
                if alpha < 1.0 {
                    all.push(ColorFilter::Matrix([
                        1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, alpha, 0.0,
                    ]));
                }
                rec.push_clip(Clip::Path(path.elements()));
                rec.image(id, Some(part), to, &ImagePaint { color_filters: &all, ..ImagePaint::default() });
                rec.pop();
            }
        }
    }

    #[allow(clippy::too_many_arguments, reason = "the line, the path it strokes, and the shape's box and turn")]
    pub(super) fn stroke(
        &mut self,
        rec: &mut Recorder,
        path: &BezPath,
        line: &ResolvedLine,
        size: [f64; 2],
        rot: f64,
        origin: [f64; 2],
        images: &[Vec<u8>],
    ) {
        if line.paint.is_none() {
            return;
        }
        let closed = last_closed(path);
        if !closed && (line.head.is_some() || line.tail.is_some()) {
            for region in with_ends(path, line) {
                self.fill(rec, &region, &line.paint, PathFill::Norm, size, rot, origin, images);
            }
            return;
        }
        if let Fill::Solid(c) = line.paint
            && line.compound == crate::drawing::paint::Compound::Single
            && !line.inset
        {
            rec.stroke_path(path.elements(), &line_pen(line, line.width), colour(c));
            return;
        }
        // The bands' coverage, in a layer, then the line's paint over it, source-in.
        let w = line.width;
        rec.push_layer(&Layer::default());
        let strokes = |rec: &mut Recorder, a: f64, b: f64| {
            rec.stroke_path(path.elements(), &line_pen(line, 2.0 * b * w), Color::BLACK);
            if a > 0.0 {
                rec.stroke_path(path.elements(), &line_pen(line, 2.0 * a * w), blended(Color::BLACK, BlendMode::Clear));
            }
        };
        let inside = |rec: &mut Recorder, a: f64, b: f64| {
            rec.push_layer(&Layer::default());
            rec.push_clip(Clip::Path(path.elements()));
            strokes(rec, a, b);
            rec.pop();
            rec.pop();
        };
        let outside = |rec: &mut Recorder, a: f64, b: f64| {
            rec.push_layer(&Layer::default());
            strokes(rec, a, b);
            rec.fill_path(path.elements(), blended(Color::BLACK, BlendMode::Clear));
            rec.pop();
        };
        for (lo, hi) in bands(line) {
            if !closed {
                let (a, b) = (lo.abs().min(hi.abs()), lo.abs().max(hi.abs()));
                rec.push_layer(&Layer::default());
                strokes(rec, if lo < 0.0 && hi > 0.0 { 0.0 } else { a }, b);
                rec.pop();
                continue;
            }
            match (lo < 0.0, hi > 0.0) {
                (true, true) => {
                    outside(rec, 0.0, -lo);
                    inside(rec, 0.0, hi);
                }
                (true, false) => outside(rec, -hi, -lo),
                (false, _) => inside(rec, lo, hi),
            }
        }
        rec.push_layer(&Layer { blend: BlendMode::SrcIn, ..Layer::default() });
        let reach = path.bounding_box().inflate(2.0 * w + 1.0, 2.0 * w + 1.0);
        let mut cover = BezPath::new();
        rect_path(reach).into_iter().for_each(|e| cover.push(e));
        self.fill(rec, &cover, &line.paint, PathFill::Norm, size, rot, origin, images);
        rec.pop();
        rec.pop();
    }
}

/// The 49 stops a gradient is sampled at, as twip-graphics stops, into `out`.
fn sample_stops(stops: &[(f64, Rgba)], out: &mut Vec<Stop>) {
    let n = 48;
    out.clear();
    out.extend((0..=n).map(|i| i as f64 / n as f64).map(|t| Stop { offset: t as f32, color: colour(gradient_at(stops, t)) }));
}

/// Calls `fill` with a linear or circular gradient's paint over a box of `size` that `to_box` places (its brush
/// mapped there); a rectangular or shape path gradient is no single paint, and `fill` isn't called.
pub(super) fn gradient_brush(
    stop_list: &[(f64, Rgba)],
    kind: GradientKind,
    size: [f64; 2],
    rot: f64,
    (tile, flip): (&[Pct; 4], TileFlip),
    to_box: Affine,
    samples: &mut Vec<Stop>,
    fill: &mut dyn FnMut(Paint<'_>),
) {
    let [w, h] = size;
    let t = tile.map(|p| p.fraction());
    let (tx0, ty0, tx1, ty1) = (w * t[0], h * t[1], w * (1.0 - t[2]), h * (1.0 - t[3]));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let back = to_box * if rot != 0.0 { Affine::rotate_about((-rot).to_radians(), kurbo::Point::new(cx, cy)) } else { Affine::IDENTITY };
    let extend = if t == [0.0; 4] {
        Extend::Clamp
    } else if flip == TileFlip::None {
        Extend::Repeat
    } else {
        Extend::Mirror
    };
    match kind {
        GradientKind::Linear { angle, scaled } => {
            sample_stops(stop_list, samples);
            let a = angle.radians();
            let (mut dx, mut dy) = (a.cos(), a.sin());
            if scaled {
                let (sx, sy) = (h * dx, w * dy);
                let n = sx.hypot(sy).max(f64::EPSILON);
                (dx, dy) = (sx / n, sy / n);
            }
            let (tw, th, tcx, tcy) = (tx1 - tx0, ty1 - ty0, (tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0);
            let len = tw * dx.abs() + th * dy.abs();
            let start = (tcx - dx * len / 2.0, tcy - dy * len / 2.0).into();
            let end = (tcx + dx * len / 2.0, tcy + dy * len / 2.0).into();
            fill(Paint::new(Brush::Linear { start, end, stops: samples, extend, transform: back }));
        }
        GradientKind::Path { path: GradientPath::Circle, .. } => {
            sample_stops(stop_list, samples);
            let r = cx.hypot(cy).max(f64::EPSILON);
            let c = (cx, cy).into();
            fill(Paint::new(Brush::Radial { start: c, start_radius: 0.0, end: c, end_radius: r, stops: samples, extend, transform: back }));
        }
        GradientKind::Path { .. } => {}
    }
}

pub(super) fn gradient(
    rec: &mut Recorder,
    path: &BezPath,
    stop_list: &[(f64, Rgba)],
    kind: GradientKind,
    size: [f64; 2],
    rot: f64,
    (tile, flip): (&[Pct; 4], TileFlip),
) {
    let [w, h] = size;
    let (cx, cy) = (w / 2.0, h / 2.0);
    let kind = match kind {
        GradientKind::Path { path: GradientPath::Shape, focus } if is_rect(path) => GradientKind::Path { path: GradientPath::Rect, focus },
        k => k,
    };
    match kind {
        GradientKind::Linear { .. } | GradientKind::Path { path: GradientPath::Circle, .. } => {
            let mut samples = Vec::new();
            gradient_brush(stop_list, kind, size, rot, (tile, flip), Affine::IDENTITY, &mut samples, &mut |paint| {
                rec.fill_path(path.elements(), paint)
            });
        }
        GradientKind::Path { path: kind @ (GradientPath::Rect | GradientPath::Shape), focus } => {
            let fr = match kind {
                GradientPath::Rect => {
                    let f = focus.map(|p| p.fraction());
                    [w * f[0], h * f[1], w * (1.0 - f[2]), h * (1.0 - f[3])]
                }
                _ => [cx, cy, cx, cy],
            };
            rec.push_clip(Clip::Path(path.elements()));
            let n = 128;
            for k in (1..=n).rev() {
                let t = k as f64 / n as f64;
                let c = colour(gradient_at(stop_list, (k as f64 - 0.5) / n as f64));
                match kind {
                    GradientPath::Rect => {
                        let l = |a: f64, b: f64| a + (b - a) * t;
                        rec.fill_rect(kurbo::Rect::new(l(fr[0], 0.0), l(fr[1], 0.0), l(fr[2], w), l(fr[3], h)), c);
                    }
                    _ => {
                        rec.push_transform(Affine::translate((cx * (1.0 - t), cy * (1.0 - t))) * Affine::scale(t));
                        rec.fill_path(path.elements(), c);
                        rec.pop();
                    }
                }
            }
            rec.pop();
        }
    }
}

/// An open line with arrowheads: its shaft shortened by each end's cut, as a filled outline, and each end's shape placed
/// at its end, pointing along it.
fn with_ends(path: &BezPath, line: &ResolvedLine) -> Vec<BezPath> {
    use crate::drawing::geom::ends::{EndOutline, end_shape};
    let contours = contours(path);
    let (Some(first), Some(last)) = (contours.first(), contours.last()) else { return Vec::new() };
    let w = line.width;
    let head = line.head.map(|e| end_shape(e, w));
    let tail = line.tail.map(|e| end_shape(e, w));
    let mut shortened = BezPath::new();
    for (i, (segs, len)) in contours.iter().enumerate() {
        let from = if i == 0 { head.as_ref().map_or(0.0, |h| h.cut) } else { 0.0 };
        let to = if i + 1 == contours.len() { len - tail.as_ref().map_or(0.0, |t| t.cut) } else { *len };
        segment(segs, from, to, &mut shortened);
    }
    let mut regions = vec![outline(&shortened, line, w)];
    let mut place = |end: &crate::drawing::geom::ends::EndShape, at: kurbo::Point, back: kurbo::Vec2| {
        let m = Affine::new([back.x, back.y, -back.y, back.x, at.x, at.y]);
        let poly = |pts: &[[f64; 2]], close: bool| {
            let mut p = BezPath::new();
            p.move_to((pts[0][0], pts[0][1]));
            for q in &pts[1..] {
                p.line_to((q[0], q[1]));
            }
            if close {
                p.close_path();
            }
            p
        };
        let shape = match &end.outline {
            EndOutline::Polygon(pts) => poly(pts, true),
            EndOutline::Oval { rx, ry } => kurbo::Ellipse::new((0.0, 0.0), (*rx, *ry), 0.0).to_path(0.01),
            EndOutline::Open(pts) => {
                let pen = kurbo::Stroke::new(w).with_caps(kurbo::Cap::Butt).with_join(kurbo::Join::Miter).with_miter_limit(10.0);
                kurbo::stroke(poly(pts, false).elements().iter().copied(), &pen, &kurbo::StrokeOpts::default(), 0.01)
            }
        };
        regions.push(m * shape);
    };
    if let Some(h) = head.as_ref()
        && let Some(seg) = first.0.first()
    {
        place(h, seg.start(), direction(seg, 0.0));
    }
    if let Some(t) = tail.as_ref()
        && let Some(seg) = last.0.last()
    {
        place(t, seg.end(), -direction(seg, 1.0));
    }
    regions
}

pub(super) fn sigma(radius: f64) -> f32 {
    (radius * SIGMA_PER_RADIUS) as f32
}

/// A picture with a changed colour: its registry id (none when it didn't decode), its pixels' bytes and the frame it was
/// last drawn in.
pub(super) struct Recoloured {
    pub id: Option<twip_graphics::image::ImageId>,
    pub bytes: u64,
    pub last: u64,
}

/// What draws a shape's text into its reflection: the graphics, the recording and the transform from the page to the
/// shape's own box.
pub(super) type ReflectedText<'a> = &'a mut dyn FnMut(&mut Graphics, &mut Recorder, Affine);

/// The most bytes of pictures with a changed colour kept between frames.
pub(super) const RECOLOUR_MAX_BYTES: u64 = 256 << 20;

const SIGMA_PER_RADIUS: f64 = 0.5;
pub(super) const GLOW_GROW: f64 = 0.5;
pub(super) const GLOW_SOFT: f64 = 0.5;
const SOFT_ERODE: f64 = 0.85;
const SOFT_FADE: f64 = 0.64;

fn shaded(c: Rgba, mode: PathFill) -> Rgba {
    let (k, white) = match mode {
        PathFill::Darken => (0.6, 0.0),
        PathFill::DarkenLess => (0.8, 0.0),
        PathFill::Lighten => (0.6, 0.4),
        PathFill::LightenLess => (0.8, 0.2),
        PathFill::Norm | PathFill::None => return c,
    };
    let f = |v: u8| (f64::from(v) * k + 255.0 * white).round() as u8;
    Rgba { r: f(c.r), g: f(c.g), b: f(c.b), a: c.a }
}

fn gradient_at(stops: &[(f64, Rgba)], t: f64) -> Rgba {
    let (Some(first), Some(last)) = (stops.first(), stops.last()) else { return Rgba::default() };
    if t <= first.0 {
        return first.1;
    }
    if t >= last.0 {
        return last.1;
    }
    let i = stops.iter().position(|(p, _)| *p >= t).unwrap_or(stops.len() - 1).max(1);
    let ((p0, a), (p1, b)) = (stops[i - 1], stops[i]);
    let f = if p1 > p0 { (t - p0) / (p1 - p0) } else { 1.0 };
    let mix = |x: u8, y: u8, f: f64| f64::from(x) + (f64::from(y) - f64::from(x)) * f;
    if stops.len() == 2 {
        let at = f.clamp(0.0, 1.0) * 16.0;
        let k = (at.floor() as usize).min(15);
        let e = TWO_STOP_BLEND[k] + (TWO_STOP_BLEND[k + 1] - TWO_STOP_BLEND[k]) * (at - k as f64);
        let lin = |x: u8, y: u8| {
            let (x, y) = (srgb_to_linear(f64::from(x) / 255.0), srgb_to_linear(f64::from(y) / 255.0));
            (linear_to_srgb((x + (y - x) * e).clamp(0.0, 1.0)) * 255.0).round() as u8
        };
        return Rgba { r: lin(a.r, b.r), g: lin(a.g, b.g), b: lin(a.b, b.b), a: mix(a.a, b.a, f).round() as u8 };
    }
    let m = |x: u8, y: u8| mix(x, y, f).round() as u8;
    Rgba { r: m(a.r, b.r), g: m(a.g, b.g), b: m(a.b, b.b), a: m(a.a, b.a) }
}

fn dash_lengths(d: PresetDash) -> &'static [f64] {
    match d {
        PresetDash::Solid => &[],
        PresetDash::Dot => &[1.0, 3.0],
        PresetDash::Dash => &[4.0, 3.0],
        PresetDash::LgDash => &[8.0, 3.0],
        PresetDash::DashDot => &[4.0, 3.0, 1.0, 3.0],
        PresetDash::LgDashDot => &[8.0, 3.0, 1.0, 3.0],
        PresetDash::LgDashDotDot => &[8.0, 3.0, 1.0, 3.0, 1.0, 3.0],
        PresetDash::SysDash => &[3.0, 1.0],
        PresetDash::SysDot => &[1.0, 1.0],
        PresetDash::SysDashDot => &[3.0, 1.0, 1.0, 1.0],
        PresetDash::SysDashDotDot => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    }
}

fn bands(line: &ResolvedLine) -> Vec<(f64, f64)> {
    let mut v = match line.compound {
        Compound::Single => vec![(-0.5, 0.5)],
        Compound::Double => vec![(-0.5, -1.0 / 6.0), (1.0 / 6.0, 0.5)],
        Compound::ThickThin => vec![(-0.5, 0.1), (0.3, 0.5)],
        Compound::ThinThick => vec![(-0.5, -0.3), (-0.1, 0.5)],
        Compound::Triple => vec![(-0.5, -1.0 / 3.0), (-1.0 / 6.0, 1.0 / 6.0), (1.0 / 3.0, 0.5)],
    };
    if line.inset {
        v = v.into_iter().map(|(a, b)| (a + 0.5, b + 0.5)).collect();
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shaded_fills_move_toward_black_and_white() {
        let c = Rgba::rgb(0x4472C4);
        assert_eq!(shaded(c, PathFill::Darken), Rgba::rgb(0x294476));
        assert_eq!(shaded(c, PathFill::DarkenLess), Rgba::rgb(0x365B9D));
        assert_eq!(shaded(c, PathFill::Lighten), Rgba::rgb(0x8FAADC));
        assert_eq!(shaded(c, PathFill::LightenLess), Rgba::rgb(0x698ED0));
        assert_eq!(shaded(c, PathFill::Norm), c);
    }

    #[test]
    fn two_stops_blend_along_the_curve_in_linear_light_and_more_stops_linearly() {
        let (a, b) = (Rgba::rgb(0x156082), Rgba::rgb(0xFFC000));
        let mid = gradient_at(&[(0.0, a), (1.0, b)], 0.5);
        assert!((i32::from(mid.r) - 187).abs() <= 1 && (i32::from(mid.g) - 154).abs() <= 1 && (i32::from(mid.b) - 94).abs() <= 1, "{mid:?}");
        let three = gradient_at(&[(0.0, Rgba::rgb(0x000000)), (0.5, Rgba::rgb(0x808080)), (1.0, Rgba::rgb(0xFFFFFF))], 0.25);
        assert_eq!(three.r, 64);
        assert_eq!(gradient_at(&[(0.2, a), (0.8, b)], 0.0), a);
    }

    #[test]
    fn line_ends_on_a_shape_of_absurd_extent_finish() {
        use crate::drawing::paint::{EndKind, EndSize, LineEnd};
        let end = Some(LineEnd { kind: EndKind::Arrow, width: EndSize::Medium, length: EndSize::Medium });
        let line = ResolvedLine { width: 1.0, head: end, tail: end, ..ResolvedLine::none() };
        let start = std::time::Instant::now();
        for s in [1e11, 1e18, 1e30, 1e300, f64::MAX] {
            let mut path = BezPath::new();
            path.move_to((0.0, 0.0));
            path.curve_to((s, s), (-s, s), (0.0, 1.0));
            with_ends(&path, &line);
        }
        assert!(start.elapsed() < std::time::Duration::from_secs(5), "took {:?}", start.elapsed());
    }
}
