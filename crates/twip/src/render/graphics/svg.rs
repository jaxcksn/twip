//! Vector pictures with twip-graphics: what [`crate::render::svg::walk`] and the metafile player draw, recorded into a
//! picture.

use std::sync::Arc;

use std::rc::Rc;
use twip_graphics::image::ImageData;
use twip_graphics::kurbo::{self, Affine, BezPath};

use twip_graphics::kurbo::Shape;
use twip_graphics::paint::{BlendMode, Brush as Source, Color, ColorFilter, Extend, ImagePaint, Paint, Sampling, Stop};
use twip_graphics::scene::{Clip, FillRule, GlyphRun, Layer, Recorder, Synthesis};

use super::Graphics;
use crate::drawing::colour::Rgba;
use crate::font::Face;
use crate::render::svg::{Brush, GradientKind, MaskStep, Sink, Spread, Text, walk};

/// Records the SVG `bytes` into `rec`, filling `dst` (the picture's box, its viewport); false when the bytes aren't an
/// SVG.
pub(crate) fn draw(rec: &mut Recorder, bytes: &[u8], dst: kurbo::Rect) -> bool {
    walk(&mut Recording::new(rec, None), bytes, dst)
}

/// The recorder a vector picture draws into, and the pushes (transforms, clips) made since each open save, for its
/// restore to pop. With the presentation, bitmaps, pattern tiles and text draw too.
pub(super) struct Recording<'a> {
    rec: &'a mut Recorder,
    g: Option<&'a mut Graphics>,
    saved: Vec<u32>,
    pushed: u32,
    /// The raster operation draws combine by ([`Sink::set_raster`]), and those of the masked layers open.
    op: u8,
    masked: Vec<u8>,
    /// The masks recorded, by their steps' address (the steps kept, so it isn't another's).
    masks: std::collections::HashMap<usize, (Rc<[MaskStep]>, twip_graphics::scene::Picture)>,
    /// A gradient's stops, reused.
    stops: Vec<Stop>,
}

/// An SVG colour (unpremultiplied, `0..=1`) in 8 bits, rounded to nearest.
fn colour([r, g, b, a]: [f32; 4]) -> Color {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Color::rgba(q(r), q(g), q(b), q(a))
}

/// An SVG colour in 8 bits, as the drawing model's.
fn rgba(c: [f32; 4]) -> Rgba {
    let c = colour(c);
    Rgba { r: c.r, g: c.g, b: c.b, a: c.a }
}

/// The blend mode of raster operation `op`: source-over for copying.
fn blend(op: u8) -> BlendMode {
    if op & 15 == 12 { BlendMode::SrcOver } else { BlendMode::Raster(op & 15) }
}

/// A colour matrix keeping colour and scaling alpha by `k`.
fn fade(k: f32) -> ColorFilter {
    ColorFilter::Matrix([1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, k, 0.0])
}

impl<'a> Recording<'a> {
    pub(super) fn new(rec: &'a mut Recorder, g: Option<&'a mut Graphics>) -> Recording<'a> {
        Recording { rec, g, saved: Vec::new(), pushed: 0, op: 12, masked: Vec::new(), masks: Default::default(), stops: Vec::new() }
    }

    /// `bitmap` in the presentation's registry, kept until its pictures go.
    fn bitmap(&mut self, bitmap: &Arc<Vec<u8>>) -> Option<twip_graphics::image::ImageId> {
        let g = self.g.as_deref_mut()?;
        let id = g.res.images.add(Arc::clone(bitmap) as ImageData).ok()?;
        g.metafile_images.push(id);
        Some(id)
    }

    /// `brush` as a paint combining by the raster operation, drawn with `draw`.
    fn with_paint(&mut self, brush: &Brush, draw: impl FnOnce(&mut Recorder, Paint<'_>)) {
        let blend = blend(self.op);
        let pattern = |image, transform| Paint { blend, ..Paint::new(Source::Pattern { image, transform, sampling: Sampling::Nearest }) };
        let g = match brush {
            Brush::Solid(c) => return draw(self.rec, Paint { blend, ..Paint::from(colour(*c)) }),
            Brush::Gradient(g) => g,
            Brush::Hatch { bits, fg, bg, transform } => {
                let Some(image) = self.g.as_deref_mut().and_then(|g| g.tile(*bits, rgba(*fg), rgba(*bg))) else { return };
                return draw(self.rec, pattern(image, *transform));
            }
            Brush::Bitmap { bitmap, transform } => {
                let Some(image) = self.bitmap(bitmap) else { return };
                return draw(self.rec, pattern(image, *transform));
            }
        };
        self.stops.clear();
        self.stops.extend(g.stops.iter().map(|&(offset, c)| Stop { offset, color: colour(c) }));
        let extend = match g.spread {
            Spread::Pad => Extend::Clamp,
            Spread::Reflect => Extend::Mirror,
            Spread::Repeat => Extend::Repeat,
        };
        let (stops, transform) = (&self.stops[..], g.transform);
        let source = match g.kind {
            GradientKind::Linear { start, end } => Source::Linear { start, end, stops, extend, transform },
            // A plain radial gradient: from its centre, radius 0, out to the radius.
            GradientKind::Radial { centre, radius } => {
                Source::Radial { start: centre, start_radius: 0.0, end: centre, end_radius: radius, stops, extend, transform }
            }
        };
        draw(self.rec, Paint { blend, ..Paint::new(source) });
    }
}

impl Sink for Recording<'_> {
    fn save(&mut self) {
        self.saved.push(self.pushed);
        self.pushed = 0;
    }

    fn restore(&mut self) {
        for _ in 0..self.pushed {
            self.rec.pop();
        }
        self.pushed = self.saved.pop().unwrap_or(0);
    }

    fn transform(&mut self, m: Affine) {
        self.rec.push_transform(m);
        self.pushed += 1;
    }

    fn clip_rect(&mut self, r: kurbo::Rect) {
        self.rec.push_clip(Clip::Rect { rect: r, aa: true });
        self.pushed += 1;
    }

    fn clip_path(&mut self, path: &BezPath, even_odd: bool) {
        let els = path.elements();
        self.rec.push_clip(if even_odd { Clip::EvenOdd(els) } else { Clip::Path(els) });
        self.pushed += 1;
    }

    fn begin_layer(&mut self, opacity: f32) {
        self.rec.push_layer(&Layer { opacity, blend: blend(self.op), ..Layer::default() });
        self.save();
    }

    fn end_layer(&mut self) {
        self.restore();
        self.rec.pop();
    }

    fn fill(&mut self, path: &BezPath, even_odd: bool, brush: &Brush) {
        let rule = if even_odd { FillRule::EvenOdd } else { FillRule::NonZero };
        self.with_paint(brush, |rec, paint| rec.fill_path_rule(path.elements(), rule, paint));
    }

    fn stroke(&mut self, path: &BezPath, stroke: &kurbo::Stroke, brush: &Brush) {
        self.with_paint(brush, |rec, paint| rec.stroke_path(path.elements(), stroke, paint));
    }

    fn image(&mut self, bitmap: &Arc<Vec<u8>>, src: kurbo::Rect, to: Affine, opacity: f32, smooth: bool) {
        let Some(id) = self.bitmap(bitmap) else { return };
        let fade = [fade(opacity)];
        let filters: &[ColorFilter] = if opacity < 1.0 { &fade } else { &[] };
        // An image paint has no blend mode: a layer combines it by the raster operation.
        let layered = self.op & 15 != 12;
        if layered {
            self.rec.push_layer(&Layer { blend: blend(self.op), ..Layer::default() });
        }
        self.rec.push_transform(to);
        let sampling = if smooth { Sampling::Linear } else { Sampling::Nearest };
        let paint = ImagePaint { color_filters: filters, sampling };
        self.rec.image(id, Some(src), kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), &paint);
        self.rec.pop();
        if layered {
            self.rec.pop();
        }
    }

    fn text(&mut self, t: &Text<'_>) -> f64 {
        let op = self.op;
        let Some(g) = self.g.as_deref_mut() else { return 0.0 };
        let Some(line) = lay(g, t) else { return 0.0 };
        draw_text(self.rec, g, t, &line, blend(op));
        line.width
    }

    fn text_outline(&mut self, t: &Text<'_>, out: &mut BezPath) -> f64 {
        let Some(g) = self.g.as_deref_mut() else { return 0.0 };
        let Some(line) = lay(g, t) else { return 0.0 };
        let mut glyph = BezPath::new();
        let at = t.transform * Affine::translate((line.ox, 0.0));
        for p in &line.glyphs {
            let Some(font) = g.font(&p.face) else { continue };
            glyph.truncate(0);
            if !g.res.fonts.em_outline(font, p.glyph, &mut glyph) {
                continue;
            }
            let skew = if p.face.sim_italic { Affine::new([1.0, 0.0, -0.2, 1.0, 0.0, 0.0]) } else { Affine::IDENTITY };
            glyph.apply_affine(at * Affine::translate((p.x, line.baseline + p.y)) * Affine::scale(line.em) * skew);
            out.extend(glyph.iter());
        }
        for r in decorations(t, &line) {
            let mut p = r.to_path(0.1);
            p.apply_affine(at);
            out.extend(p.iter());
        }
        line.width
    }

    fn set_raster(&mut self, op: u8) {
        self.op = op & 15;
    }

    fn triangle(&mut self, pts: [kurbo::Point; 3], colours: [[f32; 4]; 3]) {
        let (e1, e2) = (pts[1] - pts[0], pts[2] - pts[0]);
        let det = e1.x * e2.y - e1.y * e2.x;
        if !(det.abs() > 1e-12 && det.is_finite()) {
            return;
        }
        let mut tri = BezPath::new();
        tri.move_to(pts[0]);
        tri.line_to(pts[1]);
        tri.line_to(pts[2]);
        tri.close_path();
        let area = tri.bounding_box().inflate(1.0, 1.0);
        let diagonal = (area.width().powi(2) + area.height().powi(2)).sqrt();
        // Colour is affine across the triangle, each channel shading along its own direction: one linear gradient a
        // channel, the others black, combined by lightening (each channel's maximum, which with the other channels
        // black is their sum), in a layer clipped to the triangle.
        self.rec.push_clip(Clip::Path(tri.elements()));
        self.rec.push_layer(&Layer { blend: blend(self.op), ..Layer::default() });
        for ch in 0..3 {
            let [c0, c1, c2] = [0, 1, 2].map(|i| f64::from(colours[i][ch].clamp(0.0, 1.0)));
            let gx = ((c1 - c0) * e2.y - (c2 - c0) * e1.y) / det;
            let gy = ((c2 - c0) * e1.x - (c1 - c0) * e2.x) / det;
            let n2 = gx * gx + gy * gy;
            let only = |v: f64| {
                let mut c = [0.0f32, 0.0, 0.0, 1.0];
                c[ch] = v as f32;
                colour(c)
            };
            let mode = if ch == 0 { BlendMode::SrcOver } else { BlendMode::Lighten };
            // Within half a step of 8 bits across the triangle: one colour.
            if n2.sqrt() * diagonal < 0.5 / 255.0 {
                self.rec.fill_rect(area, Paint { blend: mode, ..Paint::from(only(c0)) });
                continue;
            }
            let g = kurbo::Vec2::new(gx, gy) / n2;
            let (start, end) = (pts[0] - g * c0, pts[0] + g * (1.0 - c0));
            let stops = [Stop { offset: 0.0, color: only(0.0) }, Stop { offset: 1.0, color: only(1.0) }];
            let source = Source::Linear { start, end, stops: &stops, extend: Extend::Clamp, transform: Affine::IDENTITY };
            self.rec.fill_rect(area, Paint { blend: mode, ..Paint::new(source) });
        }
        self.rec.pop();
        self.rec.pop();
    }

    fn begin_masked(&mut self) {
        self.rec.push_layer(&Layer { blend: blend(self.op), ..Layer::default() });
        self.masked.push(self.op);
        self.op = 12;
        self.save();
    }

    fn end_masked(&mut self, mask: &Rc<[MaskStep]>) {
        self.restore();
        // The mask's coverage, drawn from none, keeps the content where it covers: recorded once, placed after.
        let picture = match self.masks.get(&(Rc::as_ptr(mask) as *const () as usize)) {
            Some((_, p)) => p.clone(),
            None => {
                let p = mask_picture(mask);
                self.masks.insert(Rc::as_ptr(mask) as *const () as usize, (Rc::clone(mask), p.clone()));
                p
            }
        };
        self.rec.push_layer(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
        self.rec.picture(&picture, Affine::IDENTITY);
        self.rec.pop();
        self.rec.pop();
        self.op = self.masked.pop().unwrap_or(12);
    }
}

/// A mask's coverage as a picture: its steps from none.
fn mask_picture(mask: &[MaskStep]) -> twip_graphics::scene::Picture {
    let mut rec = Recorder::new();
    let mut open = 0u32;
    for step in mask {
        let (path, even_odd, mode) = match step {
            MaskStep::Add(p, e) => (p, *e, BlendMode::SrcOver),
            MaskStep::Remove(p, e) => (p, *e, BlendMode::DstOut),
            MaskStep::Toggle(p, e) => (p, *e, BlendMode::Xor),
            MaskStep::Clip(p, e) => {
                rec.push_clip(if *e { Clip::EvenOdd(p.elements()) } else { Clip::Path(p.elements()) });
                open += 1;
                continue;
            }
            MaskStep::Pop => {
                if open > 0 {
                    rec.pop();
                    open -= 1;
                }
                continue;
            }
        };
        let rule = if even_odd { FillRule::EvenOdd } else { FillRule::NonZero };
        rec.fill_path_rule(path.elements(), rule, Paint { blend: mode, ..Paint::from(Color::BLACK) });
    }
    rec.finish()
}

/// A glyph of a line of text: its face, id and place on the line.
struct Placed {
    face: Rc<Face>,
    glyph: u32,
    x: f64,
    y: f64,
}

/// A line of text laid out with the presentation's fonts: the face it names, characters it lacks from the fallback
/// faces.
struct Line {
    face: Rc<Face>,
    em: f64,
    glyphs: Vec<Placed>,
    width: f64,
    /// The line's start and baseline, from its origin.
    ox: f64,
    baseline: f64,
}

fn lay(g: &mut Graphics, t: &Text<'_>) -> Option<Line> {
    let fonts = g.lookup.as_ref()?;
    let face = fonts.face(t.family, None, t.bold, t.italic)?;
    // Glyph ids are the named font's: in any other face they would be other glyphs, so they draw only in it.
    if t.glyph_ids && !face.family.eq_ignore_ascii_case(t.family) {
        return None;
    }
    let em = if t.cell && face.ascent + face.descent > 0.0 { t.height * face.upem / (face.ascent + face.descent) } else { t.height };
    if !(em > 0.0 && em.is_finite()) {
        return None;
    }
    let mut glyphs = Vec::with_capacity(t.chars.len());
    let (mut x, mut y) = (0.0, 0.0);
    for (i, &c) in t.chars.iter().enumerate() {
        let (f, glyph) = if t.glyph_ids {
            (Rc::clone(&face), c)
        } else {
            let ch = char::from_u32(c).unwrap_or('\u{FFFD}');
            match face.glyph(ch) {
                Some(gl) => (Rc::clone(&face), gl),
                None => match fonts.fallback(ch, t.bold, t.italic).and_then(|f| Some((f.glyph(ch)?, f))) {
                    Some((gl, f)) => (f, gl),
                    None => (Rc::clone(&face), 0),
                },
            }
        };
        let advance = t.advances.and_then(|a| a.get(i).copied()).unwrap_or_else(|| f.advance(glyph) * f.scale(em));
        glyphs.push(Placed { face: f, glyph, x, y });
        x += advance;
        y += t.rises.and_then(|r| r.get(i).copied()).unwrap_or(0.0);
    }
    let k = face.scale(em);
    let ox = match t.align.0 {
        1 => -x / 2.0,
        2 => -x,
        _ => 0.0,
    };
    let baseline = match t.align.1 {
        0 => face.ascent * k,
        2 => -face.descent * k,
        _ => 0.0,
    };
    Some(Line { face, em, glyphs, width: x, ox, baseline })
}

/// The line's underline and strikethrough, in its space from its start: the underline below the baseline, the
/// strikethrough above it.
fn decorations(t: &Text<'_>, line: &Line) -> impl Iterator<Item = kurbo::Rect> {
    let (f, k) = (&line.face, line.face.scale(line.em));
    let marks = [(t.underline, f.underline_pos, f.underline_thick), (t.strike, -f.strike_pos, f.strike_thick)];
    let (em, baseline, width) = (line.em, line.baseline, line.width);
    marks.into_iter().filter(|m| m.0).map(move |(_, below, thick)| {
        let thick = if thick > 0.0 { thick * k } else { em / 14.0 };
        let y = baseline + below * k;
        kurbo::Rect::new(0.0, y - thick / 2.0, width, y + thick / 2.0)
    })
}

/// Records a laid-out line of text, combining by `blend`: a glyph run for each stretch of one face and one baseline.
fn draw_text(rec: &mut Recorder, g: &mut Graphics, t: &Text<'_>, line: &Line, blend: BlendMode) {
    let k = line.face.scale(line.em);
    rec.push_transform(t.transform * Affine::translate((line.ox, 0.0)));
    if let Some(bg) = t.background {
        let box_ = kurbo::Rect::new(0.0, line.baseline - line.face.ascent * k, line.width, line.baseline + line.face.descent * k);
        rec.fill_rect(box_, Paint { blend, ..Paint::from(colour(bg)) });
    }
    let ink = Paint { blend, ..Paint::from(colour(t.colour)) };
    let mut glyphs = std::mem::take(&mut g.glyphs);
    let placed = &line.glyphs;
    let mut start = 0;
    while start < placed.len() {
        let (f, y) = (&placed[start].face, placed[start].y);
        let end = placed[start..].iter().position(|p| !Rc::ptr_eq(&p.face, f) || p.y != y).map_or(placed.len(), |n| start + n);
        if let Some(font) = g.font(f) {
            glyphs.clear();
            glyphs.extend(placed[start..end].iter().map(|p| (p.glyph, p.x)));
            let synthesis = Synthesis { embolden: f.sim_bold, skew_x: if f.sim_italic { -0.2 } else { 0.0 } };
            let run = GlyphRun { font, size: line.em, baseline: line.baseline + y, glyphs: &glyphs, synthesis };
            if g.with_text && !t.glyph_ids {
                let text: String = t.chars[start..end].iter().map(|&c| char::from_u32(c).unwrap_or('\u{FFFD}')).collect();
                let clusters: Vec<u32> = text.char_indices().map(|(b, _)| b as u32).collect();
                rec.glyphs_text(&g.res.fonts, &run, &text, &clusters, ink);
            } else {
                rec.glyphs(&g.res.fonts, &run, ink);
            }
        }
        start = end;
    }
    g.glyphs = glyphs;
    for r in decorations(t, line) {
        rec.fill_rect(r, ink);
    }
    rec.pop();
}

#[cfg(test)]
mod tests {
    use crate::render::svg::tests::pixels;

    #[test]
    fn even_odd_fills_leave_their_holes() {
        // Two squares wound the same way: the inner one a hole by the even-odd rule.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
  <path fill-rule="evenodd" fill="#204080" d="M2 2H38V38H2Z M10 10H30V30H10Z"/>
</svg>"##;
        let px = pixels(svg, 80, 80);
        assert_eq!((px[10 * 80 + 10][3], px[40 * 80 + 40][3]), (255, 0), "the ring filled, its middle not");
    }
}
