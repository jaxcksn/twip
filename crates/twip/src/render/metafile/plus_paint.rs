//! EMF+ painting: brushes as the sink's paints, pens as strokes (their caps, dashes, alignment and compound strips),
//! path gradients as triangle meshes, textures and images, and metafiles drawn inside metafiles.

use std::f64::consts::{FRAC_PI_2, FRAC_PI_4, FRAC_PI_6};
use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, BezPath, ParamCurve, ParamCurveArclen, PathEl, Point, Shape, Vec2};

use super::plus::Plus;
use super::plus_objects::{self as obj, Attributes, CustomCap, Fill, Image, PathGradient, Pen, Ramp};
use super::{COPY, MAX_DEPTH, Player, Spent};
use crate::render::svg::{Brush, Combine, GradientKind, MESH_PASSES, MaskStep, Sink, Spread, SvgGradient};

/// What a brush paints with in the picture's space: one of the sink's paints, a path gradient's mesh (from its
/// space into the picture's), a bitmap drawn once (a clamped texture), or a metafile's tiles.
enum Source {
    Brush(Brush),
    Mesh(Rc<PathGradient>, Affine),
    Once(Arc<Vec<u8>>, kurbo::Rect, Affine),
    Picture(Rc<[u8]>, kurbo::Rect, Affine, u32),
}

/// The most tiles a tiled path gradient or metafile texture draws.
const MAX_TILES: usize = 4096;

impl Plus {
    /// `fill` in the picture's space.
    fn source<S: Sink>(&mut self, p: &Player<'_, S>, fill: &Fill) -> Option<Source> {
        let w = self.w(p);
        Some(Source::Brush(match fill {
            Fill::Solid(c) => Brush::Solid(*c),
            Fill::Hatch(style, fg, bg) => {
                let tile = self.hatch(p, *style, *fg, *bg)?;
                Brush::Bitmap { bitmap: tile, transform: p.dev * Affine::translate(self.s.origin.to_vec2()) }
            }
            Fill::Linear { rect, stops, wrap, transform } => {
                if rect.width() == 0.0 || stops.is_empty() {
                    return Some(Source::Brush(Brush::Solid(stops.first()?.1)));
                }
                let spread = match *wrap {
                    obj::WRAP_TILE | obj::WRAP_FLIP_Y => Spread::Repeat,
                    obj::WRAP_FLIP_X | obj::WRAP_FLIP_XY => Spread::Reflect,
                    _ => Spread::Pad,
                };
                let (start, end) = (Point::new(rect.x0, rect.y0), Point::new(rect.x1, rect.y0));
                Brush::Gradient(SvgGradient { kind: GradientKind::Linear { start, end }, stops: stops.clone(), spread, transform: w * *transform })
            }
            Fill::Texture { image, wrap, transform } => match &**image {
                Image::Bitmap { file, .. } if *wrap == obj::WRAP_CLAMP => {
                    return Some(Source::Once(Arc::clone(file), image.bounds(), w * *transform));
                }
                Image::Bitmap { file, .. } => Brush::Bitmap { bitmap: self.tile(p, file, *wrap)?, transform: w * *transform },
                Image::Metafile { data, bounds } => return Some(Source::Picture(Rc::clone(data), *bounds, w * *transform, *wrap)),
            },
            Fill::Path(g) => return Some(Source::Mesh(Rc::clone(g), w)),
        }))
    }

    /// Fills `path` (in the picture's space) with `fill`.
    pub(super) fn paint<S: Sink>(&mut self, p: &mut Player<'_, S>, path: &BezPath, even_odd: bool, fill: &Fill) {
        if !p.spend(path.elements().len()) {
            return;
        }
        self.use_clip(p);
        let Some(source) = self.source(p, fill) else { return };
        let area = path.bounding_box();
        p.under_clip(COPY, |me| match &source {
            Source::Brush(b) => me.sink.fill(path, even_odd, b),
            other => {
                me.sink.save();
                me.sink.clip_path(path, even_odd);
                special(me, other, area);
                me.sink.restore();
            }
        });
    }

    /// Fills `all` with `fill` where the mask `steps` covers.
    pub(super) fn paint_masked<S: Sink>(&mut self, p: &mut Player<'_, S>, all: &BezPath, fill: &Fill, steps: &Rc<[MaskStep]>) {
        self.use_clip(p);
        let Some(source) = self.source(p, fill) else { return };
        let area = all.bounding_box();
        p.under_clip(COPY, |me| {
            me.sink.begin_masked(steps);
            match &source {
                Source::Brush(b) => me.sink.fill(all, false, b),
                other => special(me, other, area),
            }
            me.sink.end_masked(steps);
        });
    }

    /// A hatch style's 8 × 8 tile in its colours: the diagonal styles anti-aliased, as GDI+ draws them.
    fn hatch<S: Sink>(&mut self, p: &Player<'_, S>, style: u32, fg: [f32; 4], bg: [f32; 4]) -> Option<Arc<Vec<u8>>> {
        let rows = HATCHES.get(style as usize)?;
        let q = |c: [f32; 4]| c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        let (f, b) = (q(fg), q(bg));
        let key = (style, [f[0], f[1], f[2], f[3], b[0], b[1], b[2], b[3]]);
        if let Some(t) = self.tiles.get(&key) {
            return Some(Arc::clone(t));
        }
        // A tile is counted as 32 × 32 pixels: its file, headers and all, and its place in the sink's registry.
        if !p.spend_pixels(32, 32) {
            return None;
        }
        // The share of a pixel a one-pixel diagonal line covers through its middle, and beside it.
        let mix = |k: f32| obj::lerp(bg, fg, f64::from(k));
        let palette = [bg, mix(0.25), mix(std::f32::consts::SQRT_2 - 0.5), fg].map(q);
        let mut px = Vec::with_capacity(256);
        for y in 0..8 {
            // Rows run bottom up in the table.
            let row = u32::from(rows.0[7 - y]) * 0x10101;
            for x in 0..8 {
                let bits = row >> (7 - x + 8);
                let i = if rows.1 {
                    if bits & 1 != 0 {
                        2
                    } else if bits & 0x82 != 0 {
                        1
                    } else {
                        0
                    }
                } else if bits & 1 != 0 {
                    3
                } else {
                    0
                };
                let [r, g, b, a] = palette[i];
                px.extend_from_slice(&[b, g, r, a]);
            }
        }
        let tile = obj::bgra_bmp(8, 8, &px);
        self.tiles.insert(key, Arc::clone(&tile));
        Some(tile)
    }

    /// A bitmap's tile for `wrap`: itself, or with its mirror images beside and below it for the flipping modes,
    /// made once a bitmap and mode, its pixels counted.
    fn tile<S: Sink>(&mut self, p: &Player<'_, S>, file: &Arc<Vec<u8>>, wrap: u32) -> Option<Arc<Vec<u8>>> {
        let (fx, fy) = match wrap {
            obj::WRAP_FLIP_X => (true, false),
            obj::WRAP_FLIP_Y => (false, true),
            obj::WRAP_FLIP_XY => (true, true),
            _ => return Some(Arc::clone(file)),
        };
        let key = (Arc::as_ptr(file) as usize, wrap);
        if let Some((_, t)) = self.flips.get(&key) {
            return Some(Arc::clone(t));
        }
        let (w, h, px) = twip_graphics::image::pixels(file).ok()?;
        let (tw, th) = (w * (1 + u32::from(fx)), h * (1 + u32::from(fy)));
        if !p.spend_pixels(tw, th) {
            return None;
        }
        let mut out = Vec::with_capacity(tw as usize * th as usize * 4);
        for y in 0..th {
            let sy = if y >= h { 2 * h - 1 - y } else { y };
            for x in 0..tw {
                let sx = if x >= w { 2 * w - 1 - x } else { x };
                let at = (sy as usize * w as usize + sx as usize) * 4;
                let [r, g, b, a] = px.get(at..at + 4).and_then(|s| s.try_into().ok()).unwrap_or([0; 4]);
                out.extend_from_slice(&[b, g, r, a]);
            }
        }
        let tile = obj::bgra_bmp(tw, th, &out);
        self.flips.insert(key, (Arc::clone(file), Arc::clone(&tile)));
        Some(tile)
    }

    // ---------------- pens ----------------

    /// Strokes `path` (world units, its figures `closed` or not) with `pen`.
    pub(super) fn stroke<S: Sink>(&mut self, p: &mut Player<'_, S>, path: &BezPath, closed: bool, pen: &Pen) {
        if !p.spend(path.elements().len()) {
            return;
        }
        // The pen's own transform shapes its nib: the path stroked in the nib's space.
        let nib = Affine::new({
            let c = pen.transform.as_coeffs();
            [c[0], c[1], c[2], c[3], 0.0, 0.0]
        });
        let nib = if nib.is_finite() && nib.determinant().abs() > 1e-12 { nib } else { Affine::IDENTITY };
        let into = self.w(p) * nib;
        if !into.is_finite() || into.determinant() == 0.0 {
            return;
        }
        let mut path = path.clone();
        path.apply_affine(nib.inverse());
        // At least a device pixel wide.
        let px = p.px() / into.determinant().abs().sqrt();
        let width = self.to_world(pen.width, pen.unit).max(px);
        let limit = pen.miter.max(1.0);
        // A miter past its limit: clipped at it (`LineJoinMiter`, here by a corner added to the bevel), or bevelled
        // (`LineJoinMiterClipped`).
        let join = match pen.join {
            1 => kurbo::Join::Bevel,
            2 => kurbo::Join::Round,
            _ => kurbo::Join::Miter,
        };
        let base = kurbo::Stroke::new(width).with_miter_limit(limit).with_join(join);
        let line_cap = |c: u32| match c {
            obj::CAP_SQUARE => kurbo::Cap::Square,
            obj::CAP_ROUND => kurbo::Cap::Round,
            _ => kurbo::Cap::Butt,
        };
        let dashed = !pen.dashes.is_empty() && pen.dashes.iter().all(|d| d.is_finite() && *d >= 0.0) && pen.dashes.iter().sum::<f64>() > 0.0;
        let pattern: Vec<f64> = if dashed { pen.dashes.iter().map(|d| d.max(1e-3) * width).collect() } else { Vec::new() };
        let dash = dashed.then(|| (&pattern[..], pen.dash_offset * width));
        if let Some((pattern, offset)) = dash
            && !p.spend(super::dash_work(&path, &base.clone().with_dashes(offset, pattern.iter().copied())))
        {
            return;
        }
        // The caps beyond what the stroke draws, the line's ends pulled back under custom caps, and clipped corners.
        let mut marks = Vec::new();
        let custom =
            |i: usize, c: u32| if c == obj::CAP_CUSTOM { if i == 0 { pen.custom.0.as_deref() } else { pen.custom.1.as_deref() } } else { None };
        if !closed {
            path = ends(&path, width, [custom(0, pen.caps.0), custom(1, pen.caps.1)], [pen.caps.0, pen.caps.1], dashed, &mut marks);
        }
        if pen.join == 0 && !corners(p, &path, width / 2.0, limit, dash, &mut marks) {
            return;
        }
        let caps = (line_cap(pen.caps.0), line_cap(pen.caps.1));
        let mut strokes = match dash {
            Some((pattern, offset)) => {
                let ends = (pen.caps.0 == obj::CAP_TRIANGLE, pen.caps.1 == obj::CAP_TRIANGLE);
                dashes(&path, pattern, offset, (caps, ends), pen.dash_cap, &base, &mut marks)
            }
            None => vec![(path.clone(), kurbo::Stroke { start_cap: caps.0, end_cap: caps.1, ..base.clone() })],
        };
        let inset = pen.alignment == 1 && closed;
        if inset {
            for (_, s) in &mut strokes {
                s.width *= 2.0;
            }
        }
        let tolerance = (px * 0.1).max(1e-9);
        let rings = match compound(&pen.compound) {
            Compound::Solid => None,
            Compound::Mirrored => Some(rings(&pen.compound, width * if inset { 2.0 } else { 1.0 })),
            Compound::Strips => {
                let mut out = Vec::new();
                for (path, s) in &strokes {
                    if !strips(p, path, s, &pen.compound, tolerance * 0.5, &mut out) {
                        return;
                    }
                }
                strokes = out;
                None
            }
        };
        self.use_clip(p);
        let Some(source) = self.source(p, &pen.fill) else { return };
        let mut clip = BezPath::new();
        if inset {
            clip.clone_from(&path);
            clip.apply_affine(into);
        }
        // Under a transform that keeps shapes (turning and scaling evenly), the stroke is drawn in the picture's
        // space; under any other, in the nib's, transformed.
        let k = into.determinant().abs().sqrt();
        let c = into.as_coeffs();
        let even = (c[0] * c[2] + c[1] * c[3]).abs() <= 1e-9 * k * k && (c[0] * c[0] + c[1] * c[1] - c[2] * c[2] - c[3] * c[3]).abs() <= 1e-9 * k * k;
        let (view, mut width, mut rings) = (if even { Affine::IDENTITY } else { into }, width, rings);
        if even {
            for (path, s) in &mut strokes {
                path.apply_affine(into);
                s.width *= k;
            }
            for (m, _) in &mut marks {
                m.apply_affine(into);
            }
            if let Some(r) = &mut rings {
                r.iter_mut().for_each(|(a, b)| (*a, *b) = (*a * k, *b * k));
            }
            width *= k;
        }
        let tolerance = (px * k.min(1.0) * 0.1).max(1e-9);
        // Pieces that overlap (dashes and caps, strips) composite once: drawn as they are only in an opaque paint, or
        // alone; otherwise their union is a mask the paint fills.
        let opaque = match &source {
            Source::Brush(Brush::Solid(c)) => c[3] >= 1.0,
            Source::Brush(Brush::Gradient(g)) => g.stops.iter().all(|s| s.1[3] >= 1.0),
            _ => false,
        };
        if let (Source::Brush(b), None) = (&source, &rings)
            && (opaque || strokes.len() + marks.len() == 1)
        {
            let b = if even { b.clone() } else { brush_in(b, view.inverse()) };
            p.under_clip(COPY, |me| {
                me.sink.save();
                if inset {
                    me.sink.clip_path(&clip, false);
                }
                if !even {
                    me.sink.transform(view);
                }
                for (path, s) in &strokes {
                    me.sink.stroke(path, s, &b);
                }
                for (m, filled) in &marks {
                    if *filled {
                        me.sink.fill(m, false, &b);
                    } else {
                        me.sink.stroke(m, &kurbo::Stroke::new(width), &b);
                    }
                }
                me.sink.restore();
            });
            return;
        }
        let outline = |path: &BezPath, s: &kurbo::Stroke| {
            let mut o = kurbo::stroke(path.iter(), s, &kurbo::StrokeOpts::default(), tolerance);
            o.apply_affine(view);
            o
        };
        let mut steps = Vec::new();
        for (path, s) in &strokes {
            if !p.spend(path.elements().len()) {
                return;
            }
            match &rings {
                // A ring of a mirrored compound line: its outer outline less its inner, in a group of its own.
                Some(rings) => {
                    for &(a, b) in rings {
                        steps.push(MaskStep::Group(Combine::Union));
                        steps.push(MaskStep::Add(outline(path, &kurbo::Stroke { width: 2.0 * b, ..s.clone() }), false));
                        if a > 0.0 {
                            steps.push(MaskStep::Remove(outline(path, &kurbo::Stroke { width: 2.0 * a, ..s.clone() }), false));
                        }
                        steps.push(MaskStep::Pop);
                    }
                }
                None => steps.push(MaskStep::Add(outline(path, s), false)),
            }
        }
        for (m, filled) in marks {
            let mut m = if filled { m } else { kurbo::stroke(m.iter(), &kurbo::Stroke::new(width), &kurbo::StrokeOpts::default(), tolerance) };
            m.apply_affine(view);
            steps.push(MaskStep::Add(m, false));
        }
        if inset {
            steps.insert(0, MaskStep::Clip(clip, false));
            steps.push(MaskStep::Pop);
        }
        if !p.spend(steps.iter().map(MaskStep::cost).sum()) {
            return;
        }
        let all = p.everything();
        self.paint_masked(p, &all, &pen.fill, &steps.into());
    }

    // ---------------- images ----------------

    /// Draws the `src` rect (in the image's pixels) of `image` over the parallelogram `to` maps the unit square onto,
    /// in the picture's space; past the image's edges as the attributes say (clamped by default).
    pub(super) fn image<S: Sink>(&mut self, p: &mut Player<'_, S>, image: &Image, src: kurbo::Rect, to: Affine, attributes: Option<Attributes>) {
        if !to.is_finite() || to.determinant() == 0.0 || src.width() == 0.0 || src.height() == 0.0 {
            return;
        }
        self.use_clip(p);
        // From the image's pixels into the picture's space.
        let from = to * Affine::scale_non_uniform(1.0 / src.width(), 1.0 / src.height()) * Affine::translate((-src.x0, -src.y0));
        let mut area = kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
        area.apply_affine(to);
        let wrap = attributes.map_or(obj::WRAP_CLAMP, |a| a.wrap);
        let smooth = self.s.smooth;
        match image {
            Image::Bitmap { file, .. } if wrap != obj::WRAP_CLAMP => {
                let Some(tile) = self.tile(p, file, wrap) else { return };
                p.under_clip(COPY, |me| me.sink.fill(&area, false, &Brush::Bitmap { bitmap: tile, transform: from }));
            }
            Image::Bitmap { file, .. } => {
                let inside = src.intersect(image.bounds());
                let clamp = attributes.map_or([0.0; 4], |a| a.clamp);
                let mut rest = area.clone();
                let mut kept = inside.to_path(0.1);
                kept.apply_affine(from);
                rest.extend(kept.iter());
                p.under_clip(COPY, |me| {
                    if clamp[3] > 0.0 && inside != src.abs() {
                        me.sink.fill(&rest, true, &Brush::Solid(clamp));
                    }
                    if inside.width() > 0.0 && inside.height() > 0.0 {
                        let onto = from * Affine::new([inside.width(), 0.0, 0.0, inside.height(), inside.x0, inside.y0]);
                        me.sink.image(file, inside, onto, 1.0, smooth);
                    }
                });
            }
            Image::Metafile { data, bounds } => {
                let data = Rc::clone(data);
                let bounds = *bounds;
                p.under_clip(COPY, |me| {
                    me.sink.save();
                    me.sink.clip_path(&area, false);
                    picture(me, &data, bounds, from);
                    me.sink.restore();
                });
            }
        }
    }
}

/// `b` in a space `m` maps the picture's space into.
fn brush_in(b: &Brush, m: Affine) -> Brush {
    match b.clone() {
        Brush::Solid(c) => Brush::Solid(c),
        Brush::Gradient(g) => Brush::Gradient(SvgGradient { transform: m * g.transform, ..g }),
        Brush::Hatch { bits, fg, bg, transform } => Brush::Hatch { bits, fg, bg, transform: m * transform },
        Brush::Bitmap { bitmap, transform } => Brush::Bitmap { bitmap, transform: m * transform },
    }
}

/// Draws a source that isn't one of the sink's paints over `area` (the picture's space), under the clip set.
fn special<S: Sink>(me: &mut Player<'_, S>, source: &Source, area: kurbo::Rect) {
    match source {
        Source::Brush(b) => me.sink.fill(&area.to_path(0.1), false, b),
        Source::Mesh(g, w) => mesh(me, g, *w, area),
        Source::Once(file, bounds, m) => {
            let onto = *m * Affine::new([bounds.width(), 0.0, 0.0, bounds.height(), bounds.x0, bounds.y0]);
            me.sink.image(file, *bounds, onto, 1.0, true);
        }
        Source::Picture(data, bounds, m, wrap) => {
            for t in tiles(*bounds, *m, area, *wrap) {
                if me.stopped.get() {
                    break;
                }
                let at = *m * t;
                let mut cell = bounds.to_path(0.1);
                cell.apply_affine(at);
                me.sink.save();
                me.sink.clip_path(&cell, false);
                picture(me, data, *bounds, at);
                me.sink.restore();
            }
        }
    }
}

/// Where a tile of `bounds` (in a brush's space, `m` into the picture's) is placed to cover `area`: at its own
/// place when clamped, else each copy, mirrored on alternate columns and rows by the flipping modes.
fn tiles(bounds: kurbo::Rect, m: Affine, area: kurbo::Rect, wrap: u32) -> Vec<Affine> {
    if wrap == obj::WRAP_CLAMP || bounds.width() <= 0.0 || bounds.height() <= 0.0 || m.determinant() == 0.0 {
        return vec![Affine::IDENTITY];
    }
    let a = m.inverse().transform_rect_bbox(area);
    let (w, h) = (bounds.width(), bounds.height());
    let span = |lo: f64, hi: f64, o: f64, d: f64| ((lo - o) / d).floor()..((hi - o) / d).ceil();
    let (cols, rows) = (span(a.x0, a.x1, bounds.x0, w), span(a.y0, a.y1, bounds.y0, h));
    let n = (cols.end - cols.start).max(0.0) * (rows.end - rows.start).max(0.0);
    if !(n.is_finite() && n <= MAX_TILES as f64) {
        return Vec::new();
    }
    let (fx, fy) = (matches!(wrap, obj::WRAP_FLIP_X | obj::WRAP_FLIP_XY), matches!(wrap, obj::WRAP_FLIP_Y | obj::WRAP_FLIP_XY));
    let mut out = Vec::new();
    for j in rows.start as i64..rows.end as i64 {
        for i in cols.start as i64..cols.end as i64 {
            let mut t = Affine::translate((i as f64 * w, j as f64 * h));
            let c = bounds.center();
            if fx && i.rem_euclid(2) == 1 {
                t = t * Affine::translate((c.x, 0.0)) * Affine::scale_non_uniform(-1.0, 1.0) * Affine::translate((-c.x, 0.0));
            }
            if fy && j.rem_euclid(2) == 1 {
                t = t * Affine::translate((0.0, c.y)) * Affine::scale_non_uniform(1.0, -1.0) * Affine::translate((0.0, -c.y));
            }
            out.push(t);
        }
    }
    out
}

/// Plays the metafile `data` (its pixels covering `bounds`) with `from` mapping its pixels into the picture's space:
/// a picture inside this one, counted against the same budgets, nested at most [`MAX_DEPTH`] deep.
fn picture<S: Sink>(me: &mut Player<'_, S>, data: &[u8], bounds: kurbo::Rect, from: Affine) {
    // Placing it is work, whatever it draws.
    if me.depth >= MAX_DEPTH || !me.spend(8) {
        return;
    }
    let size = super::size(data).unwrap_or_default();
    if size.width <= 0.0 || size.height <= 0.0 {
        return;
    }
    me.sink.save();
    me.sink.transform(
        from * Affine::translate(bounds.origin().to_vec2()) * Affine::scale_non_uniform(bounds.width() / size.width, bounds.height() / size.height),
    );
    let spent = Spent { depth: me.depth + 1, ..me.spent() };
    let out = super::emf::play_with(&mut *me.sink, data, spent).or_else(|| super::wmf::play_with(&mut *me.sink, data, spent));
    if let Some(out) = out {
        me.inherit(Spent { depth: me.depth, ..out });
    }
    me.sink.restore();
}

// ---------------- caps ----------------

/// `path`'s open figures with their ends pulled back under custom caps, and each end's cap that the stroke itself
/// doesn't draw (triangles, anchors, custom caps: in the path's space, filled or stroked) added to `marks`.
/// When `dashed`, triangle caps are the dashes' (drawn only where a dash reaches the end).
fn ends(path: &BezPath, width: f64, custom: [Option<&CustomCap>; 2], caps: [u32; 2], dashed: bool, marks: &mut Vec<(BezPath, bool)>) -> BezPath {
    let mut out = BezPath::new();
    let els = path.elements();
    let mut i = 0;
    while i < els.len() {
        let start = i;
        i += 1;
        while i < els.len() && !matches!(els[i], PathEl::MoveTo(_)) {
            i += 1;
        }
        let mut fig: Vec<PathEl> = els[start..i].to_vec();
        if fig.iter().any(|e| matches!(e, PathEl::ClosePath)) || fig.len() < 2 {
            out.extend(fig);
            continue;
        }
        for (end, cap) in custom.iter().zip(caps).enumerate() {
            let (at, dir) = tip(&fig, end == 1);
            if let (Some(c), Some(dir)) = (cap.0, dir) {
                pull(&mut fig, end == 1, c.inset * width * c.scale);
                let mut m = c.path.clone();
                m.apply_affine(Affine::translate(at.to_vec2()) * Affine::rotate(dir.atan2() - FRAC_PI_2) * Affine::scale(width * c.scale));
                marks.push((m, c.fill));
            } else if let Some(dir) = dir
                && !(dashed && cap.1 == obj::CAP_TRIANGLE)
                && let Some(m) = anchor(cap.1, at, dir, width)
            {
                marks.push((m, true));
            }
        }
        out.extend(fig);
    }
    out
}

/// A figure's first point (or last, with `end`) and the direction out of it, away from the line.
fn tip(fig: &[PathEl], end: bool) -> (Point, Option<Vec2>) {
    let mut pts = Vec::with_capacity(4);
    let (Some(first), Some(last)) = (fig.first().and_then(|e| e.end_point()), fig.last().and_then(|e| e.end_point())) else {
        return (Point::ZERO, None);
    };
    if end {
        let mut prev = first;
        for e in fig {
            match *e {
                PathEl::LineTo(q) | PathEl::MoveTo(q) => {
                    pts.clear();
                    pts.extend([prev, q]);
                    prev = q;
                }
                PathEl::QuadTo(a, q) => {
                    pts.clear();
                    pts.extend([prev, a, q]);
                    prev = q;
                }
                PathEl::CurveTo(a, b, q) => {
                    pts.clear();
                    pts.extend([prev, a, b, q]);
                    prev = q;
                }
                PathEl::ClosePath => {}
            }
        }
        let away = pts.iter().rev().skip(1).find(|q| (**q - last).hypot() > 1e-12).map(|q| last - *q);
        return (last, away.map(|v| v.normalize()));
    }
    pts.push(first);
    for e in &fig[1..] {
        match *e {
            PathEl::LineTo(q) => pts.push(q),
            PathEl::QuadTo(a, q) => pts.extend([a, q]),
            PathEl::CurveTo(a, b, q) => pts.extend([a, b, q]),
            _ => {}
        }
        if pts.iter().any(|q| (*q - first).hypot() > 1e-12) {
            break;
        }
    }
    let away = pts.iter().find(|q| (**q - first).hypot() > 1e-12).map(|q| first - *q);
    (first, away.map(|v| v.normalize()))
}

/// Moves a figure's first point (or last, with `end`) `by` along its first (or last) segment, the segment cut there.
fn pull(fig: &mut [PathEl], end: bool, by: f64) {
    if by.is_nan() || by <= 0.0 || fig.len() < 2 {
        return;
    }
    let (i, from) = if end {
        let i = fig.len() - 1;
        (i, fig[i - 1].end_point().unwrap_or_default())
    } else {
        (1, fig[0].end_point().unwrap_or_default())
    };
    let seg = match fig[i] {
        PathEl::LineTo(q) => kurbo::PathSeg::Line(kurbo::Line::new(from, q)),
        PathEl::QuadTo(a, q) => kurbo::PathSeg::Quad(kurbo::QuadBez::new(from, a, q)),
        PathEl::CurveTo(a, b, q) => kurbo::PathSeg::Cubic(kurbo::CubicBez::new(from, a, b, q)),
        _ => return,
    };
    let len = seg.arclen(1e-6);
    if len.is_nan() || len <= by {
        return;
    }
    let t = if end { seg.inv_arclen(len - by, 1e-6) } else { seg.inv_arclen(by, 1e-6) };
    let part = if end { seg.subsegment(0.0..t) } else { seg.subsegment(t..1.0) };
    fig[i] = match part {
        kurbo::PathSeg::Line(l) => PathEl::LineTo(l.p1),
        kurbo::PathSeg::Quad(q) => PathEl::QuadTo(q.p1, q.p2),
        kurbo::PathSeg::Cubic(c) => PathEl::CurveTo(c.p1, c.p2, c.p3),
    };
    if !end {
        fig[0] = PathEl::MoveTo(part.start());
    }
}

/// A cap the stroke doesn't draw itself, at `at` facing `dir` (away from the line), for a line `width` wide: a
/// triangle, or an anchor (square, round, diamond or arrow) twice the width across.
fn anchor(cap: u32, at: Point, dir: Vec2, width: f64) -> Option<BezPath> {
    let theta = dir.atan2();
    let polar = |a: f64, r: f64| at + Vec2::from_angle(a) * r;
    let poly = |pts: &[Point]| {
        let mut p = BezPath::new();
        p.move_to(pts[0]);
        for q in &pts[1..] {
            p.line_to(*q);
        }
        p.close_path();
        p
    };
    Some(match cap {
        obj::CAP_TRIANGLE => poly(&[polar(theta + FRAC_PI_2, width / 2.0), polar(theta, width / 2.0), polar(theta - FRAC_PI_2, width / 2.0)]),
        obj::CAP_SQUARE_ANCHOR | obj::CAP_DIAMOND_ANCHOR => {
            let a = theta + if cap == obj::CAP_DIAMOND_ANCHOR { 0.0 } else { FRAC_PI_4 };
            poly(&[0.0, 1.0, 2.0, 3.0].map(|k| polar(a + k * FRAC_PI_2, width)))
        }
        obj::CAP_ROUND_ANCHOR => kurbo::Circle::new(at, width).to_path(super::tolerance(width)),
        obj::CAP_ARROW_ANCHOR => {
            let size = width * 4.0 / 3f64.sqrt();
            poly(&[polar(theta + std::f64::consts::PI + FRAC_PI_6, size), at, polar(theta + std::f64::consts::PI - FRAC_PI_6, size)])
        }
        _ => return None,
    })
}

/// How a compound line's strips lie: one solid strip, mirrored about the centre, or otherwise.
enum Compound {
    Solid,
    Mirrored,
    Strips,
}

fn compound(c: &[f64]) -> Compound {
    if c.len() < 2 || !c.len().is_multiple_of(2) || !c.windows(2).all(|w| w[0] <= w[1]) || c.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Compound::Solid;
    }
    if c.len() == 2 && c[0] == 0.0 && c[1] == 1.0 {
        return Compound::Solid;
    }
    let n = c.len();
    if (0..n).all(|i| (c[i] + c[n - 1 - i] - 1.0).abs() < 1e-6) { Compound::Mirrored } else { Compound::Strips }
}

/// A mirrored compound line's strips, `width` across, as rings of distance from the line (inner and outer).
fn rings(compound: &[f64], width: f64) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> = compound
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&[a, b]| {
            let (d0, d1) = ((a - 0.5) * width, (b - 0.5) * width);
            if d0 <= 0.0 && d1 >= 0.0 { (0.0, d0.abs().max(d1.abs())) } else { (d0.abs().min(d1.abs()), d0.abs().max(d1.abs())) }
        })
        .collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v.dedup_by(|a, b| a == b);
    v
}

/// The strips of a compound line along `path` (stroked as `s`), each on its own side: the path offset to the
/// strip's middle (across its width from 0 to 1, 0 on the left as the line runs), stroked the strip's width, into
/// `out`. False past the budget.
fn strips<S: Sink>(
    p: &Player<'_, S>,
    path: &BezPath,
    s: &kurbo::Stroke,
    compound: &[f64],
    tolerance: f64,
    out: &mut Vec<(BezPath, kurbo::Stroke)>,
) -> bool {
    let figures = flat(p, path, tolerance);
    let Some(figures) = figures else { return false };
    for &[a, b] in compound.as_chunks::<2>().0 {
        let (d0, d1) = ((0.5 - b) * s.width, (0.5 - a) * s.width);
        if d1 - d0 <= 0.0 {
            continue;
        }
        let mut off = BezPath::new();
        for (pts, closed) in &figures {
            offset(&mut off, pts, *closed, (d0 + d1) / 2.0, s);
        }
        if !p.spend(off.elements().len()) {
            return false;
        }
        out.push((off, kurbo::Stroke { width: d1 - d0, start_cap: kurbo::Cap::Butt, end_cap: kurbo::Cap::Butt, ..s.clone() }));
    }
    true
}

/// A path flattened into its figures' points and whether each is closed, each point counted; none past the budget.
fn flat<S: Sink>(p: &Player<'_, S>, path: &BezPath, tolerance: f64) -> Option<Vec<(Vec<Point>, bool)>> {
    let mut figures: Vec<(Vec<Point>, bool)> = Vec::new();
    let mut ok = true;
    kurbo::flatten(path.iter(), tolerance.max(1e-9), |el| {
        if !ok || !p.spend(1) {
            ok = false;
            return;
        }
        match el {
            PathEl::MoveTo(q) => figures.push((vec![q], false)),
            PathEl::LineTo(q) => {
                if let Some((f, _)) = figures.last_mut()
                    && f.last().is_none_or(|l| (*l - q).hypot() > 1e-12)
                {
                    f.push(q);
                }
            }
            PathEl::ClosePath => {
                if let Some(f) = figures.last_mut() {
                    f.1 = true;
                    if f.0.len() > 1 && (f.0[0] - f.0[f.0.len() - 1]).hypot() <= 1e-12 {
                        f.0.pop();
                    }
                }
            }
            _ => {}
        }
    });
    ok.then_some(figures)
}

/// A polyline offset `d` to its left (as it runs), into `out`: each segment moved along its normal, the moved
/// segments meeting as `s` joins them (where they part: mitered within its limit, else bevelled, or round), or at
/// their crossing (where they overlap).
fn offset(out: &mut BezPath, pts: &[Point], closed: bool, d: f64, s: &kurbo::Stroke) {
    let n = pts.len();
    if n < 2 {
        return;
    }
    let segs = if closed { n } else { n - 1 };
    let dir = |i: usize| (pts[(i + 1) % n] - pts[i]).normalize();
    let normal = |t: Vec2| Vec2::new(t.y, -t.x);
    let moved = |i: usize, end: bool| pts[(i + usize::from(end)) % n] + normal(dir(i)) * d;
    let mut first = true;
    let mut push = |out: &mut BezPath, q: Point| {
        if first {
            out.move_to(q);
            first = false;
        } else {
            out.line_to(q);
        }
    };
    for i in 0..segs {
        if i == 0 && !closed {
            push(out, moved(0, false));
        }
        // The join at the segment's end, with the next.
        let j = i + 1;
        if j >= segs && !closed {
            push(out, moved(i, true));
            break;
        }
        let j = j % segs;
        let (t1, t2) = (dir(i), dir(j));
        let (a, c, v) = (moved(i, true), moved(j, false), pts[(i + 1) % n]);
        let cross = t1.cross(t2);
        // The moved segments part on the side away from the turn.
        let parting = cross * d > 0.0;
        let meet = (cross.abs() > 1e-12).then(|| a + t1 * ((c - a).cross(t2) / cross));
        match (parting, meet) {
            (true, Some(m)) if s.join == kurbo::Join::Miter && (m - v).hypot() <= s.miter_limit * d.abs().max(s.width / 2.0) => push(out, m),
            (true, _) if s.join == kurbo::Join::Round => {
                push(out, a);
                let (a0, a1) = ((a - v).atan2(), (c - v).atan2());
                let mut sweep = a1 - a0;
                while sweep > std::f64::consts::PI {
                    sweep -= std::f64::consts::TAU;
                }
                while sweep < -std::f64::consts::PI {
                    sweep += std::f64::consts::TAU;
                }
                let arc = kurbo::Arc::new(v, (d.abs(), d.abs()), a0, sweep, 0.0);
                for el in arc.append_iter(super::tolerance(d.abs())) {
                    out.push(el);
                }
            }
            (false, Some(m)) => push(out, m),
            _ => {
                push(out, a);
                push(out, c);
            }
        }
        if closed && i == segs - 1 {
            out.close_path();
        }
    }
}

/// `path` cut into its dashes (`pattern` from `offset`), stroked with the dash cap (round, else flat; a triangle
/// as a mark) where dashes end within the line, and with the line's own caps (`ends.0`, triangles as `ends.1`)
/// where they reach its two ends: up to four strokes, the dashes grouped by their caps.
fn dashes(
    path: &BezPath,
    pattern: &[f64],
    offset: f64,
    ends: ((kurbo::Cap, kurbo::Cap), (bool, bool)),
    dash_cap: u32,
    base: &kurbo::Stroke,
    marks: &mut Vec<(BezPath, bool)>,
) -> Vec<(BezPath, kurbo::Stroke)> {
    let ((start_cap, end_cap), (start_tri, end_tri)) = ends;
    let inner = if dash_cap == 2 { kurbo::Cap::Round } else { kurbo::Cap::Butt };
    // The dashes within the line, those from its start, those to its end, and one spanning it.
    let mut groups: [BezPath; 4] = Default::default();
    let width = base.width;
    let els = path.elements();
    let mut i = 0;
    while i < els.len() {
        let from = i;
        i += 1;
        while i < els.len() && !matches!(els[i], PathEl::MoveTo(_)) {
            i += 1;
        }
        let fig = &els[from..i];
        let closed = fig.iter().any(|e| matches!(e, PathEl::ClosePath));
        let (start, end) = (fig.first().and_then(|e| e.end_point()), fig.iter().rev().find_map(|e| e.end_point()));
        let mut parts: Vec<Vec<PathEl>> = Vec::new();
        for el in kurbo::dash(fig.iter().copied(), offset, pattern) {
            match el {
                PathEl::MoveTo(_) => parts.push(vec![el]),
                _ => {
                    if let Some(d) = parts.last_mut() {
                        d.push(el);
                    }
                }
            }
        }
        let near = |a: Option<Point>, b: Option<Point>| matches!((a, b), (Some(a), Some(b)) if (a - b).hypot() <= 1e-9 * (1.0 + a.to_vec2().hypot()));
        // The dashes come in any order (the first may be held back to meet one wrapping round): those at the ends
        // are known by where they start and stop.
        for d in &parts {
            let at_start = !closed && near(d.first().and_then(|e| e.end_point()), start);
            let at_end = !closed && near(d.last().and_then(|e| e.end_point()), end);
            for (is_end, reaches, tri) in [(false, at_start, start_tri), (true, at_end, end_tri)] {
                if (reaches && tri) || (!reaches && dash_cap == 3) {
                    let (at, dir) = tip(d, is_end);
                    if let Some(m) = dir.and_then(|dir| anchor(obj::CAP_TRIANGLE, at, dir, width)) {
                        marks.push((m, true));
                    }
                }
            }
            groups[usize::from(at_start) | usize::from(at_end) << 1].extend(d.iter().copied());
        }
    }
    let caps = [(inner, inner), (start_cap, inner), (inner, end_cap), (start_cap, end_cap)];
    groups
        .into_iter()
        .zip(caps)
        .filter(|(g, _)| !g.elements().is_empty())
        .map(|(g, (a, b))| (g, kurbo::Stroke { start_cap: a, end_cap: b, dash_pattern: Default::default(), ..base.clone() }))
        .collect()
}

/// The corners a miter join past its limit is clipped to (`LineJoinMiter`): at each join of `path` whose miter
/// reaches past `limit` half widths (`h` the half width) from its point, and that a dash covers, the part of the
/// miter between the stroke's bevel and the limit, as a mark. False past the budget.
fn corners<S: Sink>(p: &Player<'_, S>, path: &BezPath, h: f64, limit: f64, dash: Option<(&[f64], f64)>, marks: &mut Vec<(BezPath, bool)>) -> bool {
    // A dash pattern doubled when odd, as the stroke's is.
    let pattern: Vec<f64> = dash.map(|(d, _)| if d.len() % 2 == 1 { d.iter().chain(d).copied().collect() } else { d.to_vec() }).unwrap_or_default();
    let period: f64 = pattern.iter().sum();
    let on = |at: f64| {
        let Some((_, offset)) = dash else { return true };
        let mut x = (at + offset).rem_euclid(period);
        for (i, len) in pattern.iter().enumerate() {
            if x < *len {
                return i % 2 == 0;
            }
            x -= len;
        }
        true
    };
    let mut segs: Vec<kurbo::PathSeg> = Vec::new();
    let mut flush = |segs: &mut Vec<kurbo::PathSeg>, closed: bool| -> bool {
        let n = segs.len();
        let mut at = 0.0;
        for i in 0..n {
            if !p.spend(4) {
                return false;
            }
            at += segs[i].arclen(1e-3);
            if i + 1 == n && !closed {
                break;
            }
            let (a, b) = (segs[i], segs[(i + 1) % n]);
            let (Some(t1), Some(t2)) = (tangent(a, true), tangent(b, false)) else { continue };
            let (cross, dot) = (t1.cross(t2), t1.dot(t2));
            // The miter's tip is h / cos(θ / 2) from the join, θ the turn.
            if cross.abs() < 1e-12 || ((1.0 + dot) / 2.0).max(0.0).sqrt() * limit >= 1.0 || !on(at) {
                continue;
            }
            let v = a.end();
            let side = -cross.signum();
            let normal = |t: Vec2| Vec2::new(-t.y, t.x) * h * side;
            let (p1, p2) = (v + normal(t1), v + normal(t2));
            let out = ((p1 - v) + (p2 - v)).normalize();
            let reach = limit * h;
            let (k1, k2) = ((reach - (p1 - v).dot(out)) / t1.dot(out), (reach - (p2 - v).dot(out)) / -t2.dot(out));
            if !(k1.is_finite() && k2.is_finite() && k1 >= 0.0 && k2 >= 0.0) {
                continue;
            }
            let mut m = BezPath::new();
            m.move_to(p1);
            m.line_to(p1 + t1 * k1);
            m.line_to(p2 - t2 * k2);
            m.line_to(p2);
            m.close_path();
            marks.push((m, true));
        }
        segs.clear();
        true
    };
    let mut start = Point::ZERO;
    let mut last = Point::ZERO;
    for el in path.elements() {
        let seg = match *el {
            PathEl::MoveTo(q) => {
                if !flush(&mut segs, false) {
                    return false;
                }
                (start, last) = (q, q);
                continue;
            }
            PathEl::LineTo(q) => kurbo::PathSeg::Line(kurbo::Line::new(last, q)),
            PathEl::QuadTo(a, q) => kurbo::PathSeg::Quad(kurbo::QuadBez::new(last, a, q)),
            PathEl::CurveTo(a, b, q) => kurbo::PathSeg::Cubic(kurbo::CubicBez::new(last, a, b, q)),
            PathEl::ClosePath => {
                if (last - start).hypot() > 1e-12 {
                    segs.push(kurbo::PathSeg::Line(kurbo::Line::new(last, start)));
                }
                last = start;
                if !flush(&mut segs, true) {
                    return false;
                }
                continue;
            }
        };
        last = seg.end();
        if (seg.end() - seg.start()).hypot() > 1e-12 || !matches!(seg, kurbo::PathSeg::Line(_)) {
            segs.push(seg);
        }
    }
    flush(&mut segs, false)
}

/// A segment's direction at its end (or start), from the control points nearest it that differ.
fn tangent(s: kurbo::PathSeg, end: bool) -> Option<Vec2> {
    let pts: Vec<Point> = match s {
        kurbo::PathSeg::Line(l) => vec![l.p0, l.p1],
        kurbo::PathSeg::Quad(q) => vec![q.p0, q.p1, q.p2],
        kurbo::PathSeg::Cubic(c) => vec![c.p0, c.p1, c.p2, c.p3],
    };
    let v = if end {
        let e = *pts.last()?;
        pts.iter().rev().find(|q| (**q - e).hypot() > 1e-12).map(|q| e - *q)
    } else {
        let s = pts[0];
        pts.iter().find(|q| (**q - s).hypot() > 1e-12).map(|q| *q - s)
    };
    v.map(Vec2::normalize)
}

// ---------------- path gradients ----------------

/// The colour a path gradient takes `t` of the way from its focus out to its boundary (0 the focus, 1 the
/// boundary), where the boundary's colour is `s`.
fn colour_at(g: &PathGradient, s: [f32; 4], t: f64) -> [f32; 4] {
    let pos = 1.0 - t.clamp(0.0, 1.0);
    let mix = |a, b, k| if g.gamma { obj::lerp_gamma(a, b, k) } else { obj::lerp(a, b, k) };
    match &g.ramp {
        Ramp::Even => mix(s, g.centre_colour, pos),
        Ramp::Factors(f) => mix(s, g.centre_colour, piecewise(f, pos, |a, b, k| a + (b - a) * k)),
        Ramp::Preset(p) => piecewise(p, pos, mix),
    }
}

/// The value at `x` of the piecewise-linear `stops`, its ends held past them.
fn piecewise<T: Copy>(stops: &[(f64, T)], x: f64, lerp: impl Fn(T, T, f64) -> T) -> T {
    let i = stops.partition_point(|s| s.0 <= x);
    match (i.checked_sub(1).and_then(|j| stops.get(j)), stops.get(i)) {
        (Some(a), Some(b)) if b.0 > a.0 => lerp(a.1, b.1, (x - a.0) / (b.0 - a.0)),
        (Some(a), _) => a.1,
        (None, Some(b)) => b.1,
        (None, None) => unreachable!("a ramp has stops"),
    }
}

/// A mesh is drawn in up to four layers, each its triangles placed several times over: a triangle's work is that
/// many, and its layers' a share of [`super::MESH_FULL`] (the share of the picture they cover) and 32 each.
const LAYERS: usize = 4 * MESH_PASSES;
const PER_TRIANGLE: usize = LAYERS;

/// The most times a mesh cell is halved each way to bring it within 8 bits of the gradient.
const MAX_SPLITS: u32 = 4;

/// Draws a path gradient (`w` its world into the picture's space) over `area`: for each edge of its boundary, the
/// band between the focus and the edge cut where the blend bends into cells, each cell two triangles shading
/// linearly, halved until they are within half an 8-bit step of the gradient's colour; outside its focus. Tiled,
/// every copy that covers the area; else inside its boundary alone.
fn mesh<S: Sink>(me: &mut Player<'_, S>, g: &PathGradient, w: Affine, area: kurbo::Rect) {
    let m = w * g.transform;
    if !m.is_finite() || m.determinant() == 0.0 {
        return;
    }
    let px = me.px() / m.determinant().abs().sqrt();
    let figures = flatten(me, &g.boundary, px * 0.1);
    let bounds = g.boundary.bounding_box();
    let mut tris = Vec::new();
    for t in tiles(bounds, m, area, g.wrap) {
        if me.stopped.get() {
            return;
        }
        let at = m * t;
        let mut clip = g.boundary.clone();
        clip.apply_affine(at);
        tris.clear();
        if !build(me, g, &figures, at, px, &mut tris) {
            return;
        }
        // Its triangles were counted as they were made; its layers by the share of the picture they cover.
        let covered = me.share(clip.bounding_box().intersect(area));
        if !me.spend(LAYERS * 32) || !me.spend_share(covered, super::MESH_FULL) {
            return;
        }
        me.sink.save();
        me.sink.clip_path(&clip, true);
        me.sink.mesh(&tris);
        me.sink.restore();
    }
}

/// A path flattened into its figures' points (brush space), the closing point dropped, each point counted.
fn flatten<S: Sink>(me: &Player<'_, S>, path: &BezPath, tolerance: f64) -> Vec<Vec<Point>> {
    let mut figures: Vec<Vec<Point>> = Vec::new();
    let mut ok = true;
    kurbo::flatten(path.iter(), tolerance.max(1e-9), |el| {
        if !ok || !me.spend(1) {
            ok = false;
            return;
        }
        match el {
            PathEl::MoveTo(q) => figures.push(vec![q]),
            PathEl::LineTo(q) => {
                if let Some(f) = figures.last_mut() {
                    f.push(q);
                }
            }
            _ => {}
        }
    });
    for f in &mut figures {
        if f.len() > 1 && f.first() == f.last() {
            f.pop();
        }
    }
    figures.retain(|f| f.len() >= 3);
    figures
}

/// The triangles of a path gradient's tile, mapped by `at` into the picture's space; false past the budget.
fn build<S: Sink>(me: &Player<'_, S>, g: &PathGradient, figures: &[Vec<Point>], at: Affine, px: f64, tris: &mut Vec<[(Point, [f32; 4]); 3]>) -> bool {
    let c = g.centre;
    let white = [1.0; 4];
    let surround = |i: usize| g.surround.get(i.min(g.surround.len().saturating_sub(1))).copied().unwrap_or(white);
    let focus = g.focus;
    let scale = |t: f64| Vec2::new(focus.x + t * (1.0 - focus.x), focus.y + t * (1.0 - focus.y));
    // Where the colour bends: the blend's positions, and between them the gamma curve's steps.
    let mut breaks = vec![0.0, 1.0];
    match &g.ramp {
        Ramp::Factors(f) => breaks.extend(f.iter().map(|s| 1.0 - s.0)),
        Ramp::Preset(p) => breaks.extend(p.iter().map(|s| 1.0 - s.0)),
        Ramp::Even => {}
    }
    breaks.sort_by(f64::total_cmp);
    breaks.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    if g.gamma {
        let coarse = std::mem::take(&mut breaks);
        for w in coarse.windows(2) {
            breaks.extend((0..obj::GAMMA_STEPS).map(|k| w[0] + (w[1] - w[0]) * k as f64 / obj::GAMMA_STEPS as f64));
        }
        breaks.push(1.0);
    }
    let mut cells = Vec::new();
    // First the band just outside each edge, its boundary colour carried on, under the clip's anti-aliased edge;
    // then the gradient over it.
    for pass in 0..2 {
        let mut first = 0;
        for f in figures {
            for i in 0..f.len() {
                let (a, b) = (f[i], f[(i + 1) % f.len()]);
                let (sa, sb) = (surround(first + i), surround(first + (i + 1) % f.len()));
                let point = |t: f64, u: f64| {
                    let e = a.lerp(b, u) - c;
                    let k = scale(t);
                    c + Vec2::new(e.x * k.x, e.y * k.y)
                };
                let colour = |t: f64, u: f64| colour_at(g, if g.gamma { obj::lerp_gamma(sa, sb, u) } else { obj::lerp(sa, sb, u) }, t);
                if pass == 0 {
                    if !me.spend(2 * PER_TRIANGLE) {
                        return false;
                    }
                    let edge = (b - a).hypot();
                    let reach = if edge > 0.0 { ((b - a).cross(c - a) / edge).abs() } else { 0.0 };
                    if reach > 0.0 {
                        let out = 1.0 + 1.5 * px / reach;
                        let q = [point(1.0, 0.0), point(1.0, 1.0), point(out, 1.0), point(out, 0.0)];
                        let k = [colour(1.0, 0.0), colour(1.0, 1.0), colour(1.0, 1.0), colour(1.0, 0.0)];
                        tris.push([(at * q[0], k[0]), (at * q[1], k[1]), (at * q[2], k[2])]);
                        tris.push([(at * q[0], k[0]), (at * q[2], k[2]), (at * q[3], k[3])]);
                    }
                    continue;
                }
                if (focus.x > 0.0 || focus.y > 0.0) && !me.spend(PER_TRIANGLE) {
                    return false;
                }
                if focus.x > 0.0 || focus.y > 0.0 {
                    tris.push([(at * c, colour(0.0, 0.5)), (at * point(0.0, 0.0), colour(0.0, 0.0)), (at * point(0.0, 1.0), colour(0.0, 1.0))]);
                }
                for band in breaks.windows(2) {
                    cells.clear();
                    cells.push((band[0], band[1], 0.0, 1.0, 0u32));
                    while let Some((t0, t1, u0, u1, depth)) = cells.pop() {
                        if !me.spend(2 * PER_TRIANGLE) {
                            return false;
                        }
                        let (tm, um) = ((t0 + t1) / 2.0, (u0 + u1) / 2.0);
                        let q = [point(t0, u0), point(t1, u0), point(t1, u1), point(t0, u1)];
                        let k = [colour(t0, u0), colour(t1, u0), colour(t1, u1), colour(t0, u1)];
                        if depth < MAX_SPLITS && off(point(tm, um), colour(tm, um), &q, &k) {
                            cells.extend([
                                (t0, tm, u0, um, depth + 1),
                                (tm, t1, u0, um, depth + 1),
                                (t0, tm, um, u1, depth + 1),
                                (tm, t1, um, u1, depth + 1),
                            ]);
                            continue;
                        }
                        tris.push([(at * q[0], k[0]), (at * q[1], k[1]), (at * q[2], k[2])]);
                        tris.push([(at * q[0], k[0]), (at * q[2], k[2]), (at * q[3], k[3])]);
                    }
                }
            }
            first += f.len();
        }
    }
    true
}

/// Whether the cell `q` (corners and their colours `k`) drawn as two triangles strays from `want` at `x` by half an
/// 8-bit step or more.
fn off(x: Point, want: [f32; 4], q: &[Point; 4], k: &[[f32; 4]; 4]) -> bool {
    let got = [(0, 1, 2), (0, 2, 3)].iter().find_map(|&(i, j, l)| {
        let (a, b, c) = (q[i], q[j], q[l]);
        let det = (b - a).cross(c - a);
        if det.abs() < 1e-18 {
            return None;
        }
        let (v, w) = ((x - a).cross(c - a) / det, (b - a).cross(x - a) / det);
        let u = 1.0 - v - w;
        (u >= -1e-9 && v >= -1e-9 && w >= -1e-9)
            .then(|| std::array::from_fn(|ch| (k[i][ch] as f64 * u + k[j][ch] as f64 * v + k[l][ch] as f64 * w) as f32))
    });
    got.is_some_and(|g: [f32; 4]| g.iter().zip(want).any(|(a, b)| (a - b).abs() * 255.0 >= 0.5))
}

/// GDI+'s hatch styles: each row of the 8 × 8 tile (bottom up, the high bit leftmost), and whether the style's
/// diagonals are anti-aliased.
const HATCHES: [([u8; 8], bool); 53] = [
    ([0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff], false),
    ([0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80], false),
    ([0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80], true),
    ([0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01], true),
    ([0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0x80, 0xff], false),
    ([0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81], true),
    ([0x00, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0x80], false),
    ([0x00, 0x08, 0x00, 0x80, 0x00, 0x08, 0x00, 0x80], false),
    ([0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00, 0x88], false),
    ([0x22, 0x88, 0x22, 0x88, 0x22, 0x88, 0x22, 0x88], false),
    ([0x11, 0xaa, 0x44, 0xaa, 0x11, 0xaa, 0x44, 0xaa], false),
    ([0x15, 0xaa, 0x55, 0xaa, 0x51, 0xaa, 0x55, 0xaa], false),
    ([0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa], false),
    ([0x55, 0xbb, 0x55, 0xee, 0x55, 0xbb, 0x55, 0xee], false),
    ([0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77, 0xdd, 0x77], false),
    ([0xff, 0xdd, 0xff, 0x77, 0xff, 0xdd, 0xff, 0x77], false),
    ([0xff, 0xfe, 0xff, 0xef, 0xff, 0xfe, 0xff, 0xef], false),
    ([0x7f, 0xff, 0xff, 0xff, 0xf7, 0xff, 0xff, 0xff], false),
    ([0x11, 0x22, 0x44, 0x88, 0x11, 0x22, 0x44, 0x88], false),
    ([0x88, 0x44, 0x22, 0x11, 0x88, 0x44, 0x22, 0x11], false),
    ([0x99, 0x33, 0x66, 0xcc, 0x99, 0x33, 0x66, 0xcc], false),
    ([0x99, 0xcc, 0x66, 0x33, 0x99, 0xcc, 0x66, 0x33], false),
    ([0x83, 0x07, 0x0e, 0x1c, 0x38, 0x70, 0xe0, 0xc1], false),
    ([0xc1, 0xe0, 0x70, 0x38, 0x1c, 0x0e, 0x07, 0x83], false),
    ([0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88, 0x88], false),
    ([0x00, 0x00, 0x00, 0xff, 0x00, 0x00, 0x00, 0xff], false),
    ([0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55], false),
    ([0x00, 0xff, 0x00, 0xff, 0x00, 0xff, 0x00, 0xff], false),
    ([0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc, 0xcc], false),
    ([0x00, 0x00, 0xff, 0xff, 0x00, 0x00, 0xff, 0xff], false),
    ([0x00, 0x00, 0x11, 0x22, 0x44, 0x88, 0x00, 0x00], false),
    ([0x00, 0x00, 0x88, 0x44, 0x22, 0x11, 0x00, 0x00], false),
    ([0x00, 0x00, 0x00, 0x0f, 0x00, 0x00, 0x00, 0xf0], false),
    ([0x08, 0x08, 0x08, 0x08, 0x80, 0x80, 0x80, 0x80], false),
    ([0x04, 0x20, 0x01, 0x10, 0x02, 0x40, 0x08, 0x80], false),
    ([0x8d, 0x0c, 0xc0, 0xd8, 0x1b, 0x03, 0x30, 0xb1], false),
    ([0x18, 0x24, 0x42, 0x81, 0x18, 0x24, 0x42, 0x81], false),
    ([0xc0, 0x25, 0x18, 0x00, 0xc0, 0x25, 0x18, 0x00], false),
    ([0x81, 0x42, 0x24, 0x18, 0x08, 0x04, 0x02, 0x01], false),
    ([0x08, 0x08, 0x08, 0xff, 0x80, 0x80, 0x80, 0xff], false),
    ([0x51, 0x22, 0x14, 0x88, 0x45, 0x22, 0x54, 0x88], false),
    ([0xf0, 0xf0, 0xf0, 0xf0, 0x55, 0xaa, 0x55, 0xaa], false),
    ([0x80, 0x01, 0x80, 0x00, 0x10, 0x08, 0x10, 0x00], false),
    ([0x00, 0x80, 0x00, 0x80, 0x00, 0x80, 0x00, 0xaa], false),
    ([0x00, 0x22, 0x00, 0x08, 0x00, 0x22, 0x00, 0x80], false),
    ([0x01, 0x01, 0x02, 0x0c, 0x30, 0x48, 0x84, 0x03], false),
    ([0x99, 0xff, 0x66, 0xff, 0x99, 0xff, 0x66, 0xff], false),
    ([0xf8, 0xf8, 0x98, 0x77, 0x8f, 0x8f, 0x89, 0x77], false),
    ([0x88, 0x88, 0x88, 0xff, 0x88, 0x88, 0x88, 0xff], false),
    ([0x99, 0x66, 0x66, 0x99, 0x99, 0x66, 0x66, 0x99], false),
    ([0x0f, 0x0f, 0x0f, 0x0f, 0xf0, 0xf0, 0xf0, 0xf0], false),
    ([0x01, 0x82, 0x44, 0x28, 0x10, 0x28, 0x44, 0x82], false),
    ([0x00, 0x10, 0x38, 0x7c, 0xfe, 0x7c, 0x38, 0x10], false),
];
