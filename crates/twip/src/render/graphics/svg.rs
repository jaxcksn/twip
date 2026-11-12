//! SVG pictures with twip-graphics: what [`crate::render::svg::walk`] reads, recorded into a picture, as the Skia screen
//! draws it through its own sink.

use twip_graphics::kurbo::{self, Affine, BezPath};
use twip_graphics::paint::{Brush as Source, Color, Extend, Paint, Stop};
use twip_graphics::scene::{Clip, FillRule, Layer, Recorder};

use crate::render::svg::{Brush, GradientKind, Sink, Spread, walk};

/// Records the SVG `bytes` into `rec`, filling `dst` (the picture's box, its viewport); false when the bytes aren't an
/// SVG.
pub(super) fn draw(rec: &mut Recorder, bytes: &[u8], dst: kurbo::Rect) -> bool {
    let mut sink = Recording { rec, saved: Vec::new(), pushed: 0, stops: Vec::new() };
    walk(&mut sink, bytes, dst)
}

/// The recorder an SVG draws into, and the pushes (transforms, clips) made since each open save, for its restore to pop.
struct Recording<'a> {
    rec: &'a mut Recorder,
    saved: Vec<u32>,
    pushed: u32,
    /// A gradient's stops, reused.
    stops: Vec<Stop>,
}

/// An SVG colour (unpremultiplied, `0..=1`) in 8 bits, rounded as Skia rounds a float colour.
fn colour([r, g, b, a]: [f32; 4]) -> Color {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    Color::rgba(q(r), q(g), q(b), q(a))
}

impl Recording<'_> {
    /// `brush` as a paint, drawn with `draw`.
    fn with_paint(&mut self, brush: &Brush, draw: impl FnOnce(&mut Recorder, Paint<'_>)) {
        let g = match brush {
            Brush::Solid(c) => return draw(self.rec, Paint::from(colour(*c))),
            Brush::Gradient(g) => g,
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
        draw(self.rec, Paint::new(source));
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
        self.rec.push_layer(&Layer { opacity, ..Layer::default() });
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
}

#[cfg(test)]
mod tests {
    use twip_graphics::Resources;
    use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};

    use super::*;
    use crate::render::svg::tests::{ADAPTIVE, SIZES, adaptive_layout, pixels};

    /// Premultiplied RGBA pixels of the SVG recorded and drawn by twip-graphics at `w` × `h`.
    fn recorded(svg: &str, w: i32, h: i32) -> Vec<[u8; 4]> {
        let mut rec = Recorder::new();
        assert!(draw(&mut rec, svg.as_bytes(), kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h))), "an SVG");
        let picture = rec.finish();
        let mut px = vec![0u8; (w * h * 4) as usize];
        let mut target = Target::new(&mut px, w as u32, h as u32, w as usize * 4, Format::Rgba8).expect("a target");
        let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
        Renderer::new().render(&mut target, &Resources::default(), &Frame { clear: Some(Color::TRANSPARENT), draws: &draws });
        px.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
    }

    fn unpremultiplied(px: &[[u8; 4]]) -> Vec<[u8; 4]> {
        let un = |v: u8, a: u8| ((u32::from(v) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
        px.iter().map(|&[r, g, b, a]| if a == 0 { [0; 4] } else { [un(r, a), un(g, a), un(b, a), a] }).collect()
    }

    fn premultiplied(px: &[[u8; 4]]) -> Vec<[u8; 4]> {
        let mul = |v: u8, a: u8| ((u32::from(v) * u32::from(a) + 127) / 255) as u8;
        px.iter().map(|&[r, g, b, a]| [mul(r, a), mul(g, a), mul(b, a), a]).collect()
    }

    #[test]
    fn the_adaptive_placeholder_draws_as_on_the_skia_screen() {
        for (w, h) in SIZES {
            let ours = recorded(ADAPTIVE, w, h);
            adaptive_layout(&unpremultiplied(&ours), w, h);
            // Anti-aliased edges round a little differently; the drawing is the same.
            let theirs = premultiplied(&pixels(ADAPTIVE, w, h));
            let far = ours.iter().zip(&theirs).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 32)).count();
            assert!(far * 100 <= ours.len(), "{w}×{h}: {far} pixels differ from Skia's by more than 32");
        }
    }

    #[test]
    fn even_odd_fills_leave_their_holes_as_on_the_skia_screen() {
        // Two squares wound the same way: the inner one a hole by the even-odd rule.
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 40 40">
  <path fill-rule="evenodd" fill="#204080" d="M2 2H38V38H2Z M10 10H30V30H10Z"/>
</svg>"##;
        let (ours, theirs) = (recorded(svg, 80, 80), premultiplied(&pixels(svg, 80, 80)));
        assert_eq!((ours[10 * 80 + 10][3], ours[40 * 80 + 40][3]), (255, 0), "the ring filled, its middle not");
        let far = ours.iter().zip(&theirs).filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| x.abs_diff(*y) > 32)).count();
        assert!(far * 100 <= ours.len(), "{far} pixels differ from Skia's by more than 32");
    }
}
