//! A glyph run's paint: its fill (solid, gradient or none), its outline, the copies drawn under it, and Word 2010's
//! text effects (shadow, reflection and glow) in the run's box, drawn with the shapes' effects.

use twip_graphics::kurbo::{self, Affine, BezPath};
use twip_graphics::paint::{Color, Paint};
use twip_graphics::scene::{GlyphRun, Recorder, glyph_run_outline};

use super::Graphics;
use super::shape::{GLOW_GROW, GLOW_SOFT, align_matrix, blur_layer, gradient, gradient_brush, line_pen, reflect, sigma};
use crate::drawing::colour::Rgba;
use crate::drawing::paint::{Compound, Paint as Fill, ResolvedLine, ResolvedShadow};
use crate::layout::fragment::{GlyphFill, GlyphPaint, GlyphRun as Run};

/// How a drawing of the run's glyphs ends up: as the run's text, or as a copy (an effect's, which is no text: in a
/// PDF, paths).
#[derive(Clone, Copy, PartialEq, Eq)]
enum As {
    Text,
    Copy,
}

/// The run's box, its advance wide and from the face's ascent to its descent: where its gradient spans and its
/// effects are placed. `None` for a run with no width.
fn run_box(g: &Run) -> Option<kurbo::Rect> {
    let k = g.face.scale(g.size);
    let (first, last) = (g.glyphs.first()?, g.glyphs.last()?);
    let (x0, x1) = (first.1, last.1 + g.face.advance(last.0) * k);
    let b = kurbo::Rect::new(x0, g.baseline - g.face.ascent * k, x1, g.baseline + g.face.descent * k);
    (b.width() > 0.0 && b.height() > 0.0 && b.is_finite()).then_some(b)
}

impl Graphics {
    /// A glyph run with a paint: its shadow, reflection and glow, the copies under it, its fill and its outline.
    /// `ink` maps the text's colours as the appearance shows them.
    pub(super) fn painted_run(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, p: &GlyphPaint, ink: &dyn Fn(u32) -> Color) {
        let fx = p.effects.as_deref();
        let bx = run_box(g);
        if let (Some(fx), Some(bx)) = (fx, bx) {
            if let Some(sh) = &fx.shadow {
                self.text_shadow(rec, g, run, p, bx, sh);
            }
            if let Some(r) = &fx.reflection {
                // The run's own box above its baseline: its reflection is about the baseline.
                let above = kurbo::Rect::new(bx.x0, bx.y0, bx.x1, g.baseline.max(bx.y0 + 1e-3));
                let m = Affine::translate((above.x0, above.y0));
                reflect(rec, m, [above.width(), above.height()], r, &mut |rec| {
                    rec.push_transform(m.inverse());
                    self.run_body(rec, g, run, p, bx, ink, As::Copy);
                    rec.pop();
                });
            }
            if let Some((r, c)) = fx.glow {
                self.text_glow(rec, g, run, p, bx, (r, c));
            }
        }
        // Effect copies aren't text: drawing with text (a PDF), they are paths.
        for &(dx, dy, c) in p.under.iter() {
            rec.push_transform(Affine::translate((dx, dy)));
            self.glyph_copy(rec, run, ink(c));
            rec.pop();
        }
        self.run_body(rec, g, run, p, bx.unwrap_or_default(), ink, As::Text);
    }

    /// The glyphs in `paint`, as their text or as a copy.
    fn glyphs_as<'p>(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, paint: impl Into<Paint<'p>>, how: As) {
        match how {
            As::Text if self.with_text => {
                let text: String = g.chars.iter().map(|c| c.0).collect();
                let clusters = super::clusters(g).unwrap_or_default();
                rec.glyphs_text(&self.res.fonts, run, &text, &clusters, paint);
            }
            As::Copy if self.with_text => rec.glyph_paths(&self.res.fonts, run, paint),
            _ => rec.glyphs(&self.res.fonts, run, paint),
        }
    }

    fn glyph_copy(&mut self, rec: &mut Recorder, run: &GlyphRun<'_>, c: Color) {
        if self.with_text {
            rec.glyph_paths(&self.res.fonts, run, c);
        } else {
            rec.glyphs(&self.res.fonts, run, c);
        }
    }

    /// The characters of glyphs drawn some other way: a PDF's invisible text.
    fn invisible(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>) {
        if self.with_text {
            let text: String = g.chars.iter().map(|c| c.0).collect();
            let clusters = super::clusters(g).unwrap_or_default();
            rec.invisible_text(&self.res.fonts, run, &text, &clusters);
        }
    }

    /// The run's fill and outline, in the box `bx` its gradients span.
    #[allow(clippy::too_many_arguments, reason = "the run, its paint and box, and how it draws")]
    fn run_body(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, p: &GlyphPaint, bx: kurbo::Rect, ink: &dyn Fn(u32) -> Color, how: As) {
        match &p.fill {
            GlyphFill::None => {
                if how == As::Text {
                    self.invisible(rec, g, run);
                }
            }
            GlyphFill::Solid(c) => self.glyphs_as(rec, g, run, ink(*c), how),
            GlyphFill::Paint(fill) => match &**fill {
                Fill::Solid(c) => self.glyphs_as(rec, g, run, inked(ink, *c), how),
                Fill::Gradient { .. } => self.gradient_text(rec, g, run, fill, bx, ink, how),
                _ => {}
            },
        }
        if let Some((width, c)) = p.outline {
            rec.stroke_glyphs(&self.res.fonts, run, &kurbo::Stroke::new(width), ink(c));
        }
        if let Some(line) = p.effects.as_deref().and_then(|e| e.outline.as_ref()) {
            self.run_outline(rec, run, line, bx, ink);
        }
    }

    /// Glyphs filled with a gradient across `bx`: a linear or circular one as their paint (a PDF keeps the text), a
    /// rectangular or path one as their outlines filled, the text under them invisible.
    #[allow(clippy::too_many_arguments, reason = "the run, its fill and box, and how it draws")]
    fn gradient_text(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, fill: &Fill, bx: kurbo::Rect, ink: &dyn Fn(u32) -> Color, how: As) {
        let Fill::Gradient { stops: written, kind, tile, flip, .. } = fill else { return };
        // The scratch buffers are taken while the glyphs draw, and kept for the next run.
        let (mut stops, mut samples) = (std::mem::take(&mut self.fx_stops), std::mem::take(&mut self.fx_samples));
        stops.clear();
        stops.extend(written.iter().map(|&(t, c)| (t, rgba(inked(ink, c)))));
        let size = [bx.width(), bx.height()];
        let to_box = Affine::translate((bx.x0, bx.y0));
        let mut drawn = false;
        gradient_brush(&stops, *kind, size, 0.0, (tile, *flip), to_box, &mut samples, &mut |paint| {
            self.glyphs_as(rec, g, run, paint, how);
            drawn = true;
        });
        if !drawn {
            let mut path = self.run_path(run);
            path.apply_affine(to_box.inverse());
            rec.push_transform(to_box);
            gradient(rec, &path, &stops, *kind, size, 0.0, (tile, *flip));
            rec.pop();
            self.fx_path = path;
            if how == As::Text {
                self.invisible(rec, g, run);
            }
        }
        (self.fx_stops, self.fx_samples) = (stops, samples);
    }

    /// A text outline (`w14:textOutline`): a single solid line as the glyphs' stroke, any other (a gradient, a compound
    /// line) as the shapes' lines draw it around the glyphs' outlines.
    fn run_outline(&mut self, rec: &mut Recorder, run: &GlyphRun<'_>, line: &ResolvedLine, bx: kurbo::Rect, ink: &dyn Fn(u32) -> Color) {
        if let Fill::Solid(c) = line.paint
            && line.compound == Compound::Single
            && !line.inset
        {
            rec.stroke_glyphs(&self.res.fonts, run, &line_pen(line, line.width), inked(ink, c));
            return;
        }
        let mut path = self.run_path(run);
        let to_box = Affine::translate((bx.x0, bx.y0));
        path.apply_affine(to_box.inverse());
        rec.push_transform(to_box);
        // The line as written, unless the appearance shows its colours otherwise.
        if inked_alike(&line.paint, ink) {
            self.stroke(rec, &path, line, [bx.width(), bx.height()], 0.0, [bx.x0, bx.y0], &[]);
        } else {
            let mut line = line.clone();
            map_paint(&mut line.paint, ink);
            self.stroke(rec, &path, &line, [bx.width(), bx.height()], 0.0, [bx.x0, bx.y0], &[]);
        }
        rec.pop();
        self.fx_path = path;
    }

    /// The run's glyph outlines, in the scratch path (the caller puts it back in `fx_path`).
    fn run_path(&mut self, run: &GlyphRun<'_>) -> BezPath {
        let mut path = std::mem::take(&mut self.fx_path);
        path.truncate(0);
        glyph_run_outline(&self.res.fonts, run, &mut path);
        path
    }

    /// The glyphs' shape in `c`: their fill when they have one, their outline's stroke `grow` wider when they have
    /// one, or (`grow` over 0) a stroke that wide around their edges.
    fn run_silhouette(&mut self, rec: &mut Recorder, run: &GlyphRun<'_>, p: &GlyphPaint, c: Color, grow: f64) {
        let filled = !matches!(p.fill, GlyphFill::None);
        let line = p.effects.as_deref().and_then(|e| e.outline.as_ref()).map(|l| l.width).or(p.outline.map(|o| o.0));
        if filled {
            if self.with_text {
                rec.glyph_paths(&self.res.fonts, run, c);
            } else {
                rec.glyphs(&self.res.fonts, run, c);
            }
        }
        let w = line.unwrap_or(0.0) + grow;
        if (filled || line.is_some()) && w > 0.0 {
            rec.stroke_glyphs(&self.res.fonts, run, &kurbo::Stroke::new(w).with_join(kurbo::Join::Round), c);
        }
    }

    /// A text shadow: the glyphs' shape in its colour, moved, scaled and skewed about its alignment point in the run's
    /// box, blurred.
    #[allow(clippy::too_many_arguments, reason = "the run, its paint and box, and the shadow")]
    fn text_shadow(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, p: &GlyphPaint, bx: kurbo::Rect, sh: &ResolvedShadow) {
        let (dx, dy) = (sh.dist * sh.dir.to_radians().cos(), sh.dist * sh.dir.to_radians().sin());
        let c = Color::rgba(sh.colour.r, sh.colour.g, sh.colour.b, 255);
        let s = sigma(sh.blur);
        let t = Affine::translate((dx, dy)) * align_matrix([bx.x0, bx.y0, bx.x1, bx.y1], sh);
        let reach = ink_reach(g, p);
        let pad = f64::from(s) * 3.0 + 2.0;
        blur_layer(rec, s, f32::from(sh.colour.a) / 255.0, Some(t.transform_rect_bbox(bx.inflate(reach, reach)).inflate(pad, pad)));
        rec.push_transform(t);
        self.run_silhouette(rec, run, p, c, 0.0);
        rec.pop();
        rec.pop();
    }

    /// A text glow: the glyphs' shape grown by the glow's radius, blurred, in its colour.
    #[allow(clippy::too_many_arguments, reason = "the run, its paint and box, and the glow")]
    fn text_glow(&mut self, rec: &mut Recorder, g: &Run, run: &GlyphRun<'_>, p: &GlyphPaint, bx: kurbo::Rect, (r, c): (f64, Rgba)) {
        let s = sigma(r * GLOW_SOFT);
        let pad = ink_reach(g, p) + 2.0 * r * GLOW_GROW + f64::from(s) * 3.0 + 2.0;
        blur_layer(rec, s, f32::from(c.a) / 255.0, Some(bx.inflate(pad, pad)));
        self.run_silhouette(rec, run, p, Color::rgba(c.r, c.g, c.b, 255), 2.0 * r * GLOW_GROW);
        rec.pop();
    }
}

/// How far a run's glyphs and their outline can reach past its box: half an em (synthetic italic's slant, marks above
/// the ascent) and the outline's width.
fn ink_reach(g: &Run, p: &GlyphPaint) -> f64 {
    let line = p.effects.as_deref().and_then(|e| e.outline.as_ref()).map_or(0.0, |l| l.width);
    g.size * 0.5 + line.max(p.outline.map_or(0.0, |o| o.0))
}

/// Whether the appearance shows `p`'s colours as written.
fn inked_alike(p: &Fill, ink: &dyn Fn(u32) -> Color) -> bool {
    let same = |c: &Rgba| inked(ink, *c) == Color::rgba(c.r, c.g, c.b, c.a);
    match p {
        Fill::Solid(c) => same(c),
        Fill::Gradient { stops, .. } => stops.iter().all(|(_, c)| same(c)),
        _ => true,
    }
}

/// `c` as the appearance shows text, keeping its alpha.
fn inked(ink: &dyn Fn(u32) -> Color, c: Rgba) -> Color {
    ink(u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)).with_alpha(c.a)
}

fn rgba(c: Color) -> Rgba {
    Rgba { r: c.r, g: c.g, b: c.b, a: c.a }
}

/// A line's paint with its colours as the appearance shows text.
fn map_paint(p: &mut Fill, ink: &dyn Fn(u32) -> Color) {
    match p {
        Fill::Solid(c) => *c = rgba(inked(ink, *c)),
        Fill::Gradient { stops, .. } => stops.iter_mut().for_each(|(_, c)| *c = rgba(inked(ink, *c))),
        _ => {}
    }
}
