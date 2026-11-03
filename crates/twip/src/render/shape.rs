//! Drawing a shape item (DRAW-3 and the paint rules): its geometry's paths filled and stroked under its transform.

use std::collections::HashMap;
use std::rc::Rc;

use skia_safe::gradient::{Colors, Gradient, Interpolation, shaders};
use skia_safe::{Canvas, Color, Data, Image, Matrix, Paint, PaintStyle, Path, PathBuilder, PathFillType, Point, Rect, SamplingOptions, TileMode};

use super::Renderer;
use crate::drawing::colour::{Rgba, linear_to_srgb, srgb_to_linear};
use crate::drawing::geom::{Geom, PathFill, Seg};
use crate::drawing::paint::{BlipMode, Cap, Compound, Dash, GradientKind, GradientPath, Join, Paint as Fill, PresetDash, ResolvedLine, TileFlip};
use crate::drawing::units::Pct;
use crate::layout::fragment::{ImageKey, ShapeItem};

/// Skia paths of evaluated geometries, kept while the geometry is: the key is the geometry's address and the entry
/// holds the geometry, so the address can't be reused while it is cached.
#[derive(Default)]
pub struct PathCache {
    paths: HashMap<usize, (Rc<Geom>, Rc<Vec<Path>>)>,
}

impl PathCache {
    fn get(&mut self, g: &Rc<Geom>) -> Rc<Vec<Path>> {
        if self.paths.len() > 4096 {
            self.paths.clear();
        }
        self.paths
            .entry(Rc::as_ptr(g) as usize)
            .or_insert_with(|| (g.clone(), Rc::new(g.paths.iter().map(|p| skia_path(&p.segs)).collect())))
            .1
            .clone()
    }
}

/// DRAW-23: a picture's adjustments as a colour filter and alpha on `paint`, in document order.
fn picture_adjust(adjust: &[crate::drawing::paint::ResolvedBlipAdjust], paint: &mut Paint) {
    use crate::drawing::paint::ResolvedBlipAdjust as A;
    use skia_safe::color_filters;
    // Rec. 709 luminance weights, as the gray transform's (DRAW-6): `exp/s64` biLevel at 50% turns green white.
    const L: [f32; 3] = [0.2126, 0.7152, 0.0722];
    let gray: [f32; 20] = [L[0], L[1], L[2], 0.0, 0.0, L[0], L[1], L[2], 0.0, 0.0, L[0], L[1], L[2], 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mut filter: Option<skia_safe::ColorFilter> = None;
    let mut then = |f: Option<skia_safe::ColorFilter>| {
        if let Some(f) = f {
            filter = Some(match filter.take() {
                Some(inner) => color_filters::compose(f.clone(), inner).unwrap_or(f),
                None => f,
            });
        }
    };
    for a in adjust {
        match a {
            A::Alpha(v) => {
                let alpha = paint.alpha_f() * *v as f32;
                paint.set_alpha_f(alpha);
            }
            A::Grayscale => then(Some(color_filters::matrix_row_major(&gray, None))),
            A::Lum { bright, contrast } => {
                // Contrast scales about the middle (k = 1 / (1 - c) above 0, 1 + c below), brightness adds.
                let k = if *contrast > 0.0 { 1.0 / (1.0 - contrast.min(0.999)) } else { 1.0 + contrast } as f32;
                let o = (0.5 - 0.5 * f64::from(k) + bright) as f32;
                then(Some(color_filters::matrix_row_major(
                    &[k, 0.0, 0.0, 0.0, o, 0.0, k, 0.0, 0.0, o, 0.0, 0.0, k, 0.0, o, 0.0, 0.0, 0.0, 1.0, 0.0],
                    None,
                )));
            }
            A::BiLevel(t) => {
                let mut table = [0u8; 256];
                for (i, v) in table.iter_mut().enumerate() {
                    *v = if i as f64 / 255.0 >= *t { 255 } else { 0 };
                }
                then(Some(color_filters::matrix_row_major(&gray, None)));
                then(color_filters::table(&table));
            }
            A::Duotone([c0, c1]) => {
                let (a, b) = ([c0.r, c0.g, c0.b].map(|v| f32::from(v) / 255.0), [c1.r, c1.g, c1.b].map(|v| f32::from(v) / 255.0));
                let row = |i: usize| [(b[i] - a[i]) * L[0], (b[i] - a[i]) * L[1], (b[i] - a[i]) * L[2], 0.0, a[i]];
                let (r0, r1, r2) = (row(0), row(1), row(2));
                then(Some(color_filters::matrix_row_major(
                    &[
                        r0[0], r0[1], r0[2], r0[3], r0[4], r1[0], r1[1], r1[2], r1[3], r1[4], r2[0], r2[1], r2[2], r2[3], r2[4], 0.0, 0.0, 0.0, 1.0,
                        0.0,
                    ],
                    None,
                )));
            }
        }
    }
    if let Some(f) = filter {
        paint.set_color_filter(f);
    }
}

/// A blur's Gaussian sigma for an effect radius (DRAW-18: fitted on `exp/s63`).
fn sigma(radius: f64) -> f32 {
    (radius * SIGMA_PER_RADIUS) as f32
}

/// DRAW-18: Word's blur radius is about two sigmas of a Gaussian.
const SIGMA_PER_RADIUS: f64 = 0.5;
/// DRAW-20: a glow grows the silhouette by this part of its radius, blurred over this part.
const GLOW_GROW: f64 = 0.5;
const GLOW_SOFT: f64 = 0.5;
/// DRAW-21: soft edges erode the silhouette by this part of the radius and blur it over this part (sigma, as a part
/// of the radius, is half of it): Word's edge is at half opacity 0.85 r in, sigma 0.32 r (`exp/s63` soft_3/8/15).
const SOFT_ERODE: f64 = 0.85;
const SOFT_FADE: f64 = 0.64;

/// Where a shape's effects can reach on the page: its turned box grown by its line and every effect's distance and
/// blur. Effect layers are this big, not the page.
fn reach(s: &ShapeItem, m: &Matrix) -> Rect {
    let [x0, y0, x1, y1] = s.rect;
    let e = &s.effects;
    let shadow = |sh: &Option<crate::drawing::paint::ResolvedShadow>| {
        sh.as_ref().map_or(0.0, |sh| sh.dist * sh.scale[0].abs().max(sh.scale[1].abs()).max(1.0) + 3.0 * sh.blur)
    };
    let pad = s.line.width + shadow(&e.outer) + e.glow.map_or(0.0, |g| 3.0 * g.0) + 3.0 * e.blur.unwrap_or(0.0) + e.soft_edge.unwrap_or(0.0) + 4.0;
    let b = m.map_rect(Rect::new(0.0, 0.0, (x1 - x0) as f32, (y1 - y0) as f32)).0;
    // A skewed or scaled shadow can reach further: the box's size again on each side.
    let grow = pad as f32 + if e.outer.is_some() { b.width().max(b.height()) } else { 0.0 };
    b.with_outset((grow, grow))
}

/// The shape's own coordinates (from its box's top left) to the page's: flipped and turned about the box's centre.
fn shape_matrix(s: &ShapeItem) -> Matrix {
    let [x0, y0, x1, y1] = s.rect;
    let (cx, cy) = (((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32);
    let mut m = Matrix::translate((cx, cy));
    m.pre_rotate(s.rot as f32, None);
    m.pre_scale((if s.flip_h { -1.0 } else { 1.0 }, if s.flip_v { -1.0 } else { 1.0 }), None);
    m.pre_translate((-cx + x0 as f32, -cy + y0 as f32));
    m
}

/// A shadow's scale and skew about its alignment point on `b` (DRAW-18).
fn align_matrix(b: [f64; 4], sh: &crate::drawing::paint::ResolvedShadow) -> Matrix {
    use crate::drawing::paint::RectAlign::*;
    let (xm, ym) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
    let (ax, ay) = match sh.align {
        Tl => (b[0], b[1]),
        T => (xm, b[1]),
        Tr => (b[2], b[1]),
        L => (b[0], ym),
        Ctr => (xm, ym),
        R => (b[2], ym),
        Bl => (b[0], b[3]),
        B => (xm, b[3]),
        Br => (b[2], b[3]),
    };
    let mut m = Matrix::translate((ax as f32, ay as f32));
    let (kx, ky) = (sh.skew[0].to_radians().tan() as f32, sh.skew[1].to_radians().tan() as f32);
    m.pre_concat(&Matrix::new_all(1.0, kx, 0.0, ky, 1.0, 0.0, 0.0, 0.0, 1.0));
    m.pre_scale((sh.scale[0] as f32, sh.scale[1] as f32), None);
    m.pre_translate((-ax as f32, -ay as f32));
    m
}

fn skia_path(segs: &[Seg]) -> Path {
    // DRAW-14: a rectangle's outline starts at its bottom left corner and runs along its bottom first (where a dash
    // pattern begins).
    if let [Seg::Move(a), Seg::Line(b), Seg::Line(c), Seg::Line(d), Seg::Close] = *segs
        && a[1] == b[1]
        && b[0] == c[0]
        && c[1] == d[1]
        && d[0] == a[0]
        && a[0] < b[0]
        && a[1] < c[1]
    {
        let mut p = PathBuilder::new();
        p.move_to((d[0] as f32, d[1] as f32))
            .line_to((c[0] as f32, c[1] as f32))
            .line_to((b[0] as f32, b[1] as f32))
            .line_to((a[0] as f32, a[1] as f32))
            .close();
        return p.detach();
    }
    let mut b = PathBuilder::new();
    let p = |q: [f64; 2]| (q[0] as f32, q[1] as f32);
    for s in segs {
        match *s {
            Seg::Move(q) => {
                b.move_to(p(q));
            }
            Seg::Line(q) => {
                b.line_to(p(q));
            }
            Seg::Quad(c, q) => {
                b.quad_to(p(c), p(q));
            }
            Seg::Cubic(c1, c2, q) => {
                b.cubic_to(p(c1), p(c2), p(q));
            }
            Seg::Close => {
                b.close();
            }
        }
    }
    b.set_fill_type(PathFillType::Winding);
    b.detach()
}

fn colour(c: Rgba) -> Color {
    Color::from_argb(c.a, c.r, c.g, c.b)
}

/// DRAW-3: the colour of a path filled `darken` (each sRGB channel × 0.6), `darkenLess` (× 0.8), `lighten` (40 % of
/// the way to white) or `lightenLess` (20 %).
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

impl Renderer {
    /// Draws a shape item: each path of its geometry filled (by its fill mode) and stroked, rotated about the box's
    /// centre and flipped (DRAW-4), with its effects in Word's order (DRAW-17): the outer shadow and the glow under
    /// the shape, the inner shadow over its fill, and soft edges and blur on the whole of it.
    pub(super) fn shape(&mut self, canvas: &Canvas, s: &ShapeItem, images: &[Vec<u8>]) {
        let [x0, y0, x1, y1] = s.rect;
        let m = shape_matrix(s);
        let e = &s.effects;
        let area = reach(s, &m);
        // DRAW-21: blur blurs everything the shape draws; soft edges fade its edges, as a mask over all of it.
        let blur = e.blur.filter(|r| *r > 0.0);
        if let Some(r) = blur {
            let mut p = Paint::default();
            p.set_image_filter(skia_safe::image_filters::blur((sigma(r), sigma(r)), None, None, None));
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&p).bounds(&area));
        }
        // The outer shadow stays outside soft edges: Word casts it from the faded shape, and fades the shape and its glow.
        let soft = e.soft_edge.filter(|r| *r > 0.0);
        if let Some(sh) = &e.outer {
            self.outer_shadow(canvas, s, &m, sh, soft, &area);
        }
        if soft.is_some() {
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().bounds(&area));
        }
        if let Some((r, c)) = e.glow.filter(|g| g.0 > 0.0) {
            self.glow(canvas, s, &m, r, c);
        }
        canvas.save();
        canvas.concat(&m);
        let paths = self.paths.get(&s.geom);
        // DRAW-22: a picture's outline lies outside the picture: drawn twice as wide under it.
        let picture = s.picture;
        let line = if picture { ResolvedLine { width: s.line.width * 2.0, ..s.line.clone() } } else { s.line.clone() };
        for (gp, path) in s.geom.paths.iter().zip(paths.iter()) {
            if gp.stroke && picture {
                self.stroke(canvas, path, &line, [x1 - x0, y1 - y0], s.rot, [x0, y0], images);
            }
            if gp.fill != PathFill::None {
                self.fill(canvas, path, &s.fill, gp.fill, [x1 - x0, y1 - y0], s.rot, [x0, y0], images);
            }
            if gp.stroke && !picture {
                self.stroke(canvas, path, &line, [x1 - x0, y1 - y0], s.rot, [x0, y0], images);
            }
        }
        if let Some(sh) = &e.inner {
            self.inner_shadow(canvas, s, sh);
        }
        canvas.restore();
        if let Some(r) = soft {
            self.soft_edge(canvas, s, &m, r, &area);
            canvas.restore();
        }
        if blur.is_some() {
            canvas.restore();
        }
    }

    /// The shape's silhouette in `paint`: its filled paths (if it has a fill) and its outline (if it has one), in the
    /// shape's own coordinates. What shadows, glow and soft edges are made from.
    fn silhouette(&mut self, canvas: &Canvas, s: &ShapeItem, paint: &Paint) {
        let paths = self.paths.get(&s.geom);
        for (gp, path) in s.geom.paths.iter().zip(paths.iter()) {
            if gp.fill != PathFill::None && !s.fill.is_none() {
                let mut p = paint.clone();
                p.set_style(PaintStyle::Fill);
                canvas.draw_path(path, &p);
            }
            if gp.stroke && !s.line.paint.is_none() {
                let mut p = paint.clone();
                p.set_style(PaintStyle::Stroke);
                p.set_stroke_width(s.line.width.max(0.01) as f32);
                canvas.draw_path(path, &p);
            }
        }
    }

    /// DRAW-18: the outer (or preset) shadow: the silhouette in the shadow colour, scaled and skewed about its
    /// alignment point, moved `dist` towards `dir` and blurred by `blur`; turned with the shape when it rotates with
    /// it, else moved in the page's directions.
    fn outer_shadow(
        &mut self,
        canvas: &Canvas,
        s: &ShapeItem,
        m: &Matrix,
        sh: &crate::drawing::paint::ResolvedShadow,
        soft: Option<f64>,
        area: &Rect,
    ) {
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_color(colour(sh.colour));
        if sh.blur > 0.0 && soft.is_none() {
            p.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, sigma(sh.blur), true));
        }
        // A soft-edged shape's shadow: its silhouette eroded and faded as the shape is, then blurred.
        if let Some(r) = soft {
            let fade = f64::from(sigma(r * SOFT_FADE));
            let total = (fade * fade + f64::from(sigma(sh.blur)).powi(2)).sqrt() as f32;
            let erode = skia_safe::image_filters::erode(((r * SOFT_ERODE) as f32, (r * SOFT_ERODE) as f32), None, None);
            let mut lp = Paint::default();
            lp.set_image_filter(skia_safe::image_filters::blur((total, total), None, erode, None));
            canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&lp).bounds(area));
        }
        let (dx, dy) = (sh.dist * sh.dir.to_radians().cos(), sh.dist * sh.dir.to_radians().sin());
        let [x0, y0, x1, y1] = s.rect;
        // A translucent shadow is one layer: where the fill's and the outline's silhouettes overlap it is no darker.
        let translucent = sh.colour.a < 255 && soft.is_none();
        if translucent {
            canvas.save_layer_alpha_f(*area, f32::from(sh.colour.a) / 255.0);
            p.set_alpha(255);
        }
        canvas.save();
        if sh.rot_with_shape {
            canvas.concat(m);
            canvas.translate((dx as f32, dy as f32));
            canvas.concat(&align_matrix([0.0, 0.0, x1 - x0, y1 - y0], sh));
        } else {
            let b = m.map_rect(Rect::new(0.0, 0.0, (x1 - x0) as f32, (y1 - y0) as f32)).0;
            canvas.translate((dx as f32, dy as f32));
            canvas.concat(&align_matrix([f64::from(b.left), f64::from(b.top), f64::from(b.right), f64::from(b.bottom)], sh));
            canvas.concat(m);
        }
        self.silhouette(canvas, s, &p);
        canvas.restore();
        if soft.is_some() || translucent {
            canvas.restore();
        }
    }

    /// DRAW-20: the glow: the silhouette grown by `r` and blurred, in the glow colour, under the shape.
    fn glow(&mut self, canvas: &Canvas, s: &ShapeItem, m: &Matrix, r: f64, c: Rgba) {
        canvas.save();
        canvas.concat(m);
        let paths = self.paths.get(&s.geom);
        let mut q = Paint::default();
        q.set_anti_alias(true);
        q.set_color(colour(c));
        q.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, sigma(r * GLOW_SOFT), true));
        q.set_style(PaintStyle::StrokeAndFill);
        q.set_stroke_join(skia_safe::paint::Join::Round);
        for (gp, path) in s.geom.paths.iter().zip(paths.iter()) {
            let filled = gp.fill != PathFill::None && !s.fill.is_none();
            let lined = gp.stroke && !s.line.paint.is_none();
            if filled || lined {
                let mut q = q.clone();
                q.set_stroke_width((2.0 * r * GLOW_GROW + if lined { s.line.width } else { 0.0 }) as f32);
                if !filled {
                    q.set_style(PaintStyle::Stroke);
                }
                canvas.draw_path(path, &q);
            }
        }
        canvas.restore();
    }

    /// DRAW-19: the inner shadow: inside the fill only, the outside of the shape moved `dist` away from `dir` and
    /// blurred, in the shadow colour: the edges towards `dir` darken (in the shape's coordinates: it turns with it).
    fn inner_shadow(&mut self, canvas: &Canvas, s: &ShapeItem, sh: &crate::drawing::paint::ResolvedShadow) {
        let paths = self.paths.get(&s.geom);
        let mut inside = Path::new();
        for (gp, path) in s.geom.paths.iter().zip(paths.iter()) {
            if gp.fill != PathFill::None {
                inside = skia_safe::op(&inside, path, skia_safe::PathOp::Union).unwrap_or(inside);
            }
        }
        if inside.is_empty() || s.fill.is_none() {
            return;
        }
        let [x0, y0, x1, y1] = s.rect;
        let pad = (sh.dist + 3.0 * sh.blur + 1.0) as f32;
        let outer = Path::rect(Rect::new(-pad, -pad, (x1 - x0) as f32 + pad, (y1 - y0) as f32 + pad), None);
        let Some(ring) = skia_safe::op(&outer, &inside, skia_safe::PathOp::Difference) else { return };
        let mut p = Paint::default();
        p.set_anti_alias(true);
        p.set_color(colour(sh.colour));
        if sh.blur > 0.0 {
            p.set_mask_filter(skia_safe::MaskFilter::blur(skia_safe::BlurStyle::Normal, sigma(sh.blur), true));
        }
        let (dx, dy) = (sh.dist * sh.dir.to_radians().cos(), sh.dist * sh.dir.to_radians().sin());
        canvas.save();
        canvas.clip_path(&inside, None, true);
        canvas.translate((-dx as f32, -dy as f32));
        canvas.draw_path(&ring, &p);
        canvas.restore();
    }

    /// DRAW-21: soft edges: what the shape drew (the open layer) kept where its silhouette, shrunk by half of `r` and
    /// blurred, covers it: the edges fade to nothing over `r`.
    fn soft_edge(&mut self, canvas: &Canvas, s: &ShapeItem, m: &Matrix, r: f64, area: &Rect) {
        let mut mask = Paint::default();
        mask.set_blend_mode(skia_safe::BlendMode::DstIn);
        mask.set_image_filter(skia_safe::image_filters::blur((sigma(r * SOFT_FADE), sigma(r * SOFT_FADE)), None, None, None));
        canvas.save_layer(&skia_safe::canvas::SaveLayerRec::default().paint(&mask).bounds(area));
        canvas.concat(m);
        let mut ink = Paint::default();
        ink.set_anti_alias(true);
        ink.set_color(Color::BLACK);
        self.silhouette(canvas, s, &ink);
        // Eroded: the outline band cleared, half of it inside.
        let mut clear = Paint::default();
        clear.set_anti_alias(true);
        clear.set_blend_mode(skia_safe::BlendMode::Clear);
        clear.set_style(PaintStyle::Stroke);
        clear.set_stroke_width((r * SOFT_ERODE * 2.0 + if s.line.paint.is_none() { 0.0 } else { s.line.width }) as f32);
        clear.set_stroke_join(skia_safe::paint::Join::Round);
        let paths = self.paths.get(&s.geom);
        for path in paths.iter() {
            canvas.draw_path(path, &clear);
        }
        canvas.restore();
    }

    #[allow(clippy::too_many_arguments)]
    fn fill(&mut self, canvas: &Canvas, path: &Path, fill: &Fill, mode: PathFill, size: [f64; 2], rot: f64, origin: [f64; 2], images: &[Vec<u8>]) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        match fill {
            Fill::None => {}
            Fill::Solid(c) => {
                paint.set_color(colour(shaded(*c, mode)));
                canvas.draw_path(path, &paint);
            }
            Fill::Gradient { stops, kind, rot_with_shape, tile, flip } => {
                let stops: Vec<(f64, Rgba)> = stops.iter().map(|(p, c)| (*p, shaded(*c, mode))).collect();
                gradient(canvas, path, &stops, *kind, size, if *rot_with_shape { 0.0 } else { rot }, (tile, *flip));
            }
            Fill::Pattern { preset, fg, bg } => {
                // DRAW-11: the tile's cells are 0.75 pt, on a grid that starts at the page's top left.
                let bits = crate::drawing::patterns::tile(*preset);
                let (fg, bg) = (shaded(*fg, mode), shaded(*bg, mode));
                let mut px = Vec::with_capacity(64 * 4);
                for row in bits {
                    for i in 0..8 {
                        let c = if row & (0x80 >> i) != 0 { fg } else { bg };
                        px.extend([c.r, c.g, c.b, c.a]);
                    }
                }
                let info = skia_safe::ImageInfo::new((8, 8), skia_safe::ColorType::RGBA8888, skia_safe::AlphaType::Unpremul, None);
                if let Some(img) = skia_safe::images::raster_from_data(&info, Data::new_copy(&px), 8 * 4) {
                    let mut m = Matrix::translate((-origin[0] as f32, -origin[1] as f32));
                    m.pre_scale((0.75, 0.75), None);
                    paint.set_shader(img.to_shader((TileMode::Repeat, TileMode::Repeat), SamplingOptions::default(), Some(&m)));
                    canvas.draw_path(path, &paint);
                }
            }
            Fill::Picture { image, src, mode: blip, adjust, .. } => {
                picture_adjust(adjust, &mut paint);
                let key = ImageKey { index: *image, generation: 0 };
                let img = self
                    .images
                    .entry(key)
                    .or_insert_with(|| image.and_then(|i| images.get(i)).and_then(|b| Image::from_encoded(Data::new_copy(b))))
                    .clone();
                let Some(img) = img else {
                    // A picture without decodable data: a light frame, as an undecodable inline picture.
                    let mut frame = Paint::default();
                    frame.set_color(Color::from_rgb(0xc8, 0xc8, 0xc8)).set_style(PaintStyle::Stroke);
                    canvas.draw_path(path, &frame);
                    return;
                };
                let (iw, ih) = (f64::from(img.width()), f64::from(img.height()));
                // DRAW-22: srcRect: the part of the image used (negative insets add empty margins); fillRect: where it
                // goes, insets in fractions of the box.
                let sr = [src[0].fraction() * iw, src[1].fraction() * ih, iw - src[2].fraction() * iw, ih - src[3].fraction() * ih];
                let fr = match blip {
                    BlipMode::Stretch(f) => [f[0].fraction(), f[1].fraction(), f[2].fraction(), f[3].fraction()],
                    BlipMode::Tile { tx, ty, sx, sy, align, .. } => {
                        // DRAW-12: tiles the image's size at 96 dpi scaled by sx, sy, aligned in the box, then moved by
                        // tx, ty. A tile's `flip` is not drawn.
                        let (tw, th) = (iw * 0.75 * sx.fraction(), ih * 0.75 * sy.fraction());
                        let col = |a: u8| match a {
                            0 => 0.0,
                            1 => (size[0] - tw) / 2.0,
                            _ => size[0] - tw,
                        };
                        let row = |a: u8| match a {
                            0 => 0.0,
                            1 => (size[1] - th) / 2.0,
                            _ => size[1] - th,
                        };
                        use crate::drawing::paint::RectAlign::*;
                        let (c, r) = match align {
                            Tl => (0, 0),
                            T => (1, 0),
                            Tr => (2, 0),
                            L => (0, 1),
                            Ctr => (1, 1),
                            R => (2, 1),
                            Bl => (0, 2),
                            B => (1, 2),
                            Br => (2, 2),
                        };
                        let (ox, oy) = (col(c) + tx.to_pt(), row(r) + ty.to_pt());
                        let mut m = Matrix::translate((ox as f32, oy as f32));
                        m.pre_scale(((tw / iw) as f32, (th / ih) as f32), None);
                        paint.set_shader(img.to_shader((TileMode::Repeat, TileMode::Repeat), SamplingOptions::default(), Some(&m)));
                        canvas.draw_path(path, &paint);
                        return;
                    }
                };
                let dst = [fr[0] * size[0], fr[1] * size[1], size[0] - fr[2] * size[0], size[1] - fr[3] * size[1]];
                // The source rectangle's part inside the image, and where that part lands in the destination.
                let (sw, sh) = ((sr[2] - sr[0]).max(1e-9), (sr[3] - sr[1]).max(1e-9));
                let at = |v: f64, lo: f64, span: f64, d0: f64, d1: f64| d0 + (v - lo) / span * (d1 - d0);
                let (cx0, cy0, cx1, cy1) = (sr[0].max(0.0), sr[1].max(0.0), sr[2].min(iw), sr[3].min(ih));
                if cx1 <= cx0 || cy1 <= cy0 {
                    return;
                }
                let part = Rect::new(cx0 as f32, cy0 as f32, cx1 as f32, cy1 as f32);
                let to = Rect::new(
                    at(cx0, sr[0], sw, dst[0], dst[2]) as f32,
                    at(cy0, sr[1], sh, dst[1], dst[3]) as f32,
                    at(cx1, sr[0], sw, dst[0], dst[2]) as f32,
                    at(cy1, sr[1], sh, dst[1], dst[3]) as f32,
                );
                canvas.save();
                canvas.clip_path(path, None, true);
                canvas.draw_image_rect_with_sampling_options(
                    &img,
                    Some((&part, skia_safe::canvas::SrcRectConstraint::Fast)),
                    to,
                    SamplingOptions::default(),
                    &paint,
                );
                canvas.restore();
            }
        }
    }
}

/// DRAW-10: how far a two-stop gradient has blended at sixteenths of the way from one stop to the other (in linear
/// light).
const TWO_STOP_BLEND: [f64; 17] =
    [0.0, 0.018, 0.0472, 0.0843, 0.1404, 0.209, 0.2927, 0.3873, 0.4934, 0.5922, 0.6913, 0.778, 0.8485, 0.9054, 0.949, 0.9779, 1.0];

/// DRAW-10: a gradient's colour at `t` (0–1). Two stops blend in linear light along [`TWO_STOP_BLEND`]; more stops
/// blend linearly in sRGB between neighbours. Opacity blends linearly.
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

/// The gradient as Skia stops: enough samples that Skia's linear blending between them follows [`gradient_at`].
fn skia_stops(stops: &[(f64, Rgba)]) -> (Vec<skia_safe::Color4f>, Vec<f32>) {
    let n = 48;
    (0..=n).map(|i| i as f64 / n as f64).map(|t| (skia_safe::Color4f::from(colour(gradient_at(stops, t))), t as f32)).unzip()
}

/// DRAW-10: fills `path` (in a `size` box from its top left) with a gradient. Linear: along the direction `ang`
/// (clockwise from the x axis), or with `scaled` along (h·cos ang, w·sin ang), from the box's first point on that
/// direction to its last. Circle: from the box's centre to its corners. Rect and shape: contours that are the box (rect)
/// or the outline (shape) scaled from the focus (`fillToRect`; shape: the centre; a shape whose outline is its box is
/// filled as rect) out to full size. `rot` turns the gradient back against the shape's rotation (`rotWithShape="0"`).
/// A linear gradient with a `tileRect` spans that rectangle (insets of the box, negative outward) and repeats past
/// it, mirrored with `flip`.
fn gradient(canvas: &Canvas, path: &Path, stops: &[(f64, Rgba)], kind: GradientKind, size: [f64; 2], rot: f64, (tile, flip): (&[Pct; 4], TileFlip)) {
    let [w, h] = size;
    let t = tile.map(|p| p.fraction());
    let (tx0, ty0, tx1, ty1) = (w * t[0], h * t[1], w * (1.0 - t[2]), h * (1.0 - t[3]));
    let (cx, cy) = (w / 2.0, h / 2.0);
    let kind = match kind {
        GradientKind::Path { path: GradientPath::Shape, focus } if path.is_rect().is_some() => GradientKind::Path { path: GradientPath::Rect, focus },
        k => k,
    };
    let mut paint = Paint::default();
    paint.set_anti_alias(true);
    let back = (rot != 0.0).then(|| Matrix::rotate_deg_pivot(-rot as f32, (cx as f32, cy as f32)));
    let (colours, pos) = skia_stops(stops);
    let mode = if t == [0.0; 4] {
        TileMode::Clamp
    } else if flip == TileFlip::None {
        TileMode::Repeat
    } else {
        TileMode::Mirror
    };
    let grad = Gradient::new(Colors::new(&colours, Some(pos.as_slice()), mode, None), Interpolation::default());
    match kind {
        GradientKind::Linear { angle, scaled } => {
            let a = angle.radians();
            let (mut dx, mut dy) = (a.cos(), a.sin());
            if scaled {
                let (sx, sy) = (h * dx, w * dy);
                let n = sx.hypot(sy).max(f64::EPSILON);
                (dx, dy) = (sx / n, sy / n);
            }
            let (tw, th, tcx, tcy) = (tx1 - tx0, ty1 - ty0, (tx0 + tx1) / 2.0, (ty0 + ty1) / 2.0);
            let len = tw * dx.abs() + th * dy.abs();
            let p0 = Point::new((tcx - dx * len / 2.0) as f32, (tcy - dy * len / 2.0) as f32);
            let p1 = Point::new((tcx + dx * len / 2.0) as f32, (tcy + dy * len / 2.0) as f32);
            paint.set_shader(shaders::linear_gradient((p0, p1), &grad, back.as_ref()));
            canvas.draw_path(path, &paint);
        }
        GradientKind::Path { path: GradientPath::Circle, .. } => {
            let r = cx.hypot(cy).max(f64::EPSILON) as f32;
            paint.set_shader(shaders::radial_gradient(((cx as f32, cy as f32), r), &grad, back.as_ref()));
            canvas.draw_path(path, &paint);
        }
        GradientKind::Path { path: kind @ (GradientPath::Rect | GradientPath::Shape), focus } => {
            // Bands from the outside in, each a contour at its colour.
            let fr = match kind {
                GradientPath::Rect => {
                    let f = focus.map(|p| p.fraction());
                    [w * f[0], h * f[1], w * (1.0 - f[2]), h * (1.0 - f[3])]
                }
                _ => [cx, cy, cx, cy],
            };
            canvas.save();
            canvas.clip_path(path, None, true);
            let n = 128;
            for k in (1..=n).rev() {
                let t = k as f64 / n as f64;
                paint.set_color(colour(gradient_at(stops, (k as f64 - 0.5) / n as f64)));
                match kind {
                    GradientPath::Rect => {
                        let l = |a: f64, b: f64| (a + (b - a) * t) as f32;
                        canvas.draw_rect(Rect::new(l(fr[0], 0.0), l(fr[1], 0.0), l(fr[2], w), l(fr[3], h)), &paint);
                    }
                    _ => {
                        let m = Matrix::scale_translate((t as f32, t as f32), ((cx * (1.0 - t)) as f32, (cy * (1.0 - t)) as f32));
                        canvas.draw_path(&path.make_transform(&m), &paint);
                    }
                }
            }
            canvas.restore();
        }
    }
}

/// DRAW-14: a preset dash in line widths (dash, space, …).
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

/// DRAW-14: a stroke paint for `line` at width `w` (points): its cap, join and miter limit, and its dashes in widths,
/// each dash one width shorter and each space one width longer under round and square caps (the caps add them back).
fn pen(line: &ResolvedLine, w: f64) -> Paint {
    let mut paint = Paint::default();
    paint.set_anti_alias(true).set_style(PaintStyle::Stroke).set_stroke_width(w as f32);
    paint.set_stroke_cap(match line.cap {
        Cap::Round => skia_safe::paint::Cap::Round,
        Cap::Square => skia_safe::paint::Cap::Square,
        Cap::Flat => skia_safe::paint::Cap::Butt,
    });
    match line.join {
        Join::Round => {
            paint.set_stroke_join(skia_safe::paint::Join::Round);
        }
        Join::Bevel => {
            paint.set_stroke_join(skia_safe::paint::Join::Bevel);
        }
        Join::Miter(lim) => {
            paint.set_stroke_join(skia_safe::paint::Join::Miter).set_stroke_miter(lim.fraction() as f32);
        }
    }
    let lengths: Vec<f64> = match &line.dash {
        Dash::Preset(p) => dash_lengths(*p).to_vec(),
        Dash::Custom(ds) => ds.iter().flat_map(|(d, sp)| [d.fraction(), sp.fraction()]).collect(),
    };
    if !lengths.is_empty() && w > 0.0 {
        let capped = line.cap != Cap::Flat;
        let intervals: Vec<f32> = lengths
            .iter()
            .enumerate()
            .map(|(i, l)| {
                ((l + if !capped {
                    0.0
                } else if i % 2 == 0 {
                    -1.0
                } else {
                    1.0
                }) * w)
                    .max(0.0) as f32
            })
            .collect();
        if let Some(e) = skia_safe::PathEffect::dash(&intervals, 0.0) {
            paint.set_path_effect(e);
        }
    }
    paint
}

/// The region within `d` of `path`'s outline, as `pen` draws it.
fn band(path: &Path, line: &ResolvedLine, d: f64) -> Path {
    let mut b = PathBuilder::new();
    skia_safe::path_utils::fill_path_with_paint(path, &pen(line, 2.0 * d), &mut b, None, None);
    b.detach()
}

/// DRAW-15: the bands of a compound line, as signed offsets from the outline in line widths (negative: outside the
/// shape): `dbl` 1 : 1 : 1, `thickThin` 3 : 1 : 1 and `thinThick` 1 : 1 : 3 (the thick band outside, resp. inside), `tri`
/// 1 : 1 : 2 : 1 : 1. A pen inside the path (`algn="in"`) takes [0, 1].
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

/// DRAW-14, DRAW-15: the region a line covers: its bands, each the part of the right width on its side of the
/// outline (an open path has no sides: its bands are centred).
fn line_region(path: &Path, line: &ResolvedLine, closed: bool) -> Option<Path> {
    use skia_safe::PathOp;
    let w = line.width;
    let mut region: Option<Path> = None;
    for (lo, hi) in bands(line) {
        let part = if !closed {
            let (a, b) = (lo.abs().min(hi.abs()), lo.abs().max(hi.abs()));
            let outer = band(path, line, b * w);
            if lo < 0.0 && hi > 0.0 { Some(outer) } else { skia_safe::op(&outer, &band(path, line, a * w), PathOp::Difference) }
        } else {
            let inside = |a: f64, b: f64| {
                let outer = band(path, line, b * w);
                let ring = if a > 0.0 { skia_safe::op(&outer, &band(path, line, a * w), PathOp::Difference)? } else { outer };
                skia_safe::op(&ring, path, PathOp::Intersect)
            };
            let outside = |a: f64, b: f64| {
                let outer = band(path, line, b * w);
                let ring = if a > 0.0 { skia_safe::op(&outer, &band(path, line, a * w), PathOp::Difference)? } else { outer };
                skia_safe::op(&ring, path, PathOp::Difference)
            };
            match (lo < 0.0, hi > 0.0) {
                (true, true) => skia_safe::op(&outside(0.0, -lo)?, &inside(0.0, hi)?, PathOp::Union),
                (true, false) => outside(-hi, -lo),
                (false, _) => inside(lo, hi),
            }
        };
        region = match (region, part) {
            (Some(r), Some(p)) => skia_safe::op(&r, &p, PathOp::Union),
            (r, p) => r.or(p),
        };
    }
    region
}

/// DRAW-16: an open line with its ends: the line shortened under each end, as the region its pen covers, then the
/// ends' outlines placed on the endpoints along the line's direction there (each its own region).
fn with_ends(path: &Path, line: &ResolvedLine) -> Option<Vec<Path>> {
    use crate::drawing::geom::ends::{EndOutline, end_shape};
    use skia_safe::ContourMeasureIter;
    let contours: Vec<_> = ContourMeasureIter::new(path, false, None).collect();
    let (first, last) = (contours.first()?, contours.last()?);
    let w = line.width;
    // The head end goes on the path's first point, the tail end on its last.
    let head = line.head.map(|e| end_shape(e, w));
    let tail = line.tail.map(|e| end_shape(e, w));
    // The shortened line: the first contour from the head's cut, the last to the tail's.
    let mut b = PathBuilder::new();
    for (i, c) in contours.iter().enumerate() {
        let from = if i == 0 { head.as_ref().map_or(0.0, |h| h.cut as f32) } else { 0.0 };
        let to = if i + 1 == contours.len() { c.length() - tail.as_ref().map_or(0.0, |t| t.cut as f32) } else { c.length() };
        let _ = c.get_segment(from, to, &mut b, true);
    }
    let shortened = b.detach();
    let mut regions = vec![band(&shortened, line, w / 2.0)];
    let mut place = |end: &crate::drawing::geom::ends::EndShape, at: Point, back: skia_safe::Vector| {
        let n = back.length().max(f32::EPSILON);
        let (ux, uy) = (back.x / n, back.y / n);
        let m = Matrix::new_all(ux, -uy, at.x, uy, ux, at.y, 0.0, 0.0, 1.0);
        let shape = match &end.outline {
            EndOutline::Polygon(pts) => {
                let mut p = PathBuilder::new();
                p.move_to((pts[0][0] as f32, pts[0][1] as f32));
                for q in &pts[1..] {
                    p.line_to((q[0] as f32, q[1] as f32));
                }
                p.close();
                p.detach()
            }
            EndOutline::Oval { rx, ry } => Path::oval(Rect::new(-*rx as f32, -*ry as f32, *rx as f32, *ry as f32), None),
            EndOutline::Open(pts) => {
                let mut p = PathBuilder::new();
                p.move_to((pts[0][0] as f32, pts[0][1] as f32));
                for q in &pts[1..] {
                    p.line_to((q[0] as f32, q[1] as f32));
                }
                let mut pen = Paint::default();
                pen.set_style(PaintStyle::Stroke).set_stroke_width(w as f32).set_stroke_join(skia_safe::paint::Join::Miter).set_stroke_miter(10.0);
                let mut out = PathBuilder::new();
                skia_safe::path_utils::fill_path_with_paint(&p.detach(), &pen, &mut out, None, None);
                out.detach()
            }
        };
        regions.push(shape.make_transform(&m));
    };
    if let (Some(h), Some((at, tan))) = (head.as_ref(), first.pos_tan(0.0)) {
        place(h, at, tan);
    }
    if let (Some(t), Some((at, tan))) = (tail.as_ref(), last.pos_tan(last.length())) {
        place(t, at, -tan);
    }
    Some(regions)
}

impl Renderer {
    /// Strokes `path` with `line` (DRAW-9, DRAW-14): a single solid line centred on the path as a stroke (a hairline,
    /// width 0, one device pixel wide); a compound line, an inside pen or a line filled with a gradient, pattern or
    /// picture as the region it covers, filled with that paint.
    #[allow(clippy::too_many_arguments)]
    fn stroke(&mut self, canvas: &Canvas, path: &Path, line: &ResolvedLine, size: [f64; 2], rot: f64, origin: [f64; 2], images: &[Vec<u8>]) {
        if line.paint.is_none() {
            return;
        }
        let closed = path.is_last_contour_closed();
        if !closed && (line.head.is_some() || line.tail.is_some()) {
            for region in with_ends(path, line).unwrap_or_default() {
                self.fill(canvas, &region, &line.paint, PathFill::Norm, size, rot, origin, images);
            }
            return;
        }
        if let Fill::Solid(c) = line.paint
            && line.compound == Compound::Single
            && !line.inset
        {
            let mut paint = pen(line, line.width);
            paint.set_color(colour(c));
            canvas.draw_path(path, &paint);
            return;
        }
        if let Some(region) = line_region(path, line, closed) {
            self.fill(canvas, &region, &line.paint, PathFill::Norm, size, rot, origin, images);
        }
    }
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
        // The two-stop midpoint is the linear-light blend at 0.4934: (187, 154, 94) as the reference draws it.
        let mid = gradient_at(&[(0.0, a), (1.0, b)], 0.5);
        assert!((i32::from(mid.r) - 187).abs() <= 1 && (i32::from(mid.g) - 154).abs() <= 1 && (i32::from(mid.b) - 94).abs() <= 1, "{mid:?}");
        let three = gradient_at(&[(0.0, Rgba::rgb(0x000000)), (0.5, Rgba::rgb(0x808080)), (1.0, Rgba::rgb(0xFFFFFF))], 0.25);
        assert_eq!(three.r, 64);
        assert_eq!(gradient_at(&[(0.2, a), (0.8, b)], 0.0), a);
    }
}
