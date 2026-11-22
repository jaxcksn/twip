//! The scenes the golden images and the GPU comparisons draw: one picture per feature, each in a fixed size.

#![allow(dead_code, reason = "each test crate uses the scenes it checks")]

use twip_graphics::Resources;
use twip_graphics::kurbo::{self, Affine, BezPath, Cap, Join, Rect, Shape, Stroke};
use twip_graphics::paint::{BlendMode, Color, Paint};
use twip_graphics::scene::{Clip, Picture, Recorder};

pub const RED: Color = Color::rgb(0xD0_3030);
pub const GREEN: Color = Color::rgb(0x30_A050);
pub const BLUE: Color = Color::rgb(0x30_60D0);
pub const INK: Color = Color::rgb(0x20_2020);

pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Rect {
    Rect::new(x0, y0, x1, y1)
}

pub fn aliased(c: Color) -> Paint<'static> {
    Paint { aa: false, ..c.into() }
}

pub fn polyline(points: &[(f64, f64)]) -> BezPath {
    let mut p = BezPath::new();
    p.move_to(points[0]);
    for &q in &points[1..] {
        p.line_to(q);
    }
    p
}

/// Anti-aliased and aliased rects on fractional coordinates, a translucent overlap, and sub-pixel slivers.
pub fn rects() -> Picture {
    let mut rec = Recorder::new();
    rec.fill_rect(rect(10.25, 10.5, 40.75, 30.25), RED);
    rec.fill_rect(rect(50.4, 10.6, 80.5, 30.5), aliased(BLUE));
    rec.fill_rect(rect(30.0, 20.0, 70.0, 50.0), GREEN.with_alpha(128));
    rec.fill_rect(rect(90.3, 5.0, 90.6, 60.0), INK);
    rec.fill_rect(rect(100.5, 5.0, 101.5, 60.0), INK);
    rec.fill_rect(rect(110.0, 5.0, 111.0, 60.0), INK);
    rec.fill_rect(rect(5.0, 70.0, 120.0, 70.5), INK);
    rec.fill_rect(rect(5.0, 80.2, 5.6, 80.8), INK);
    rec.finish()
}

/// The fill rules: three nested squares wound the same way, and a five-point star, each filled non-zero (left) and
/// even-odd (right); then a ring clip by the even-odd rule (two circles wound alike), over a filled rect.
pub fn fill_rules() -> Picture {
    use twip_graphics::scene::FillRule;
    let mut rec = Recorder::new();
    let mut nested = BezPath::new();
    for (a, b) in [(4.0, 52.0), (14.0, 42.0), (22.0, 34.0)] {
        nested.extend(rect(a, a, b, b).path_elements(0.1));
    }
    let star: Vec<(f64, f64)> = (0..5)
        .map(|i| {
            let a = std::f64::consts::PI * (0.5 + 0.8 * i as f64);
            (28.0 + 24.0 * a.cos(), 84.0 - 24.0 * a.sin())
        })
        .collect();
    let mut star = polyline(&star);
    star.close_path();
    for (dx, rule) in [(0.0, FillRule::NonZero), (64.0, FillRule::EvenOdd)] {
        let at = Affine::translate((dx, 0.0));
        rec.fill_path_rule((at * nested.clone()).elements(), rule, BLUE);
        rec.fill_path_rule((at * star.clone()).elements(), rule, RED);
    }
    let mut ring = kurbo::Circle::new((64.0, 136.0), 24.0).to_path(0.01);
    ring.extend(kurbo::Circle::new((64.0, 136.0), 12.0).to_path(0.01));
    rec.push_clip(Clip::EvenOdd(ring.elements()));
    rec.fill_rect(rect(30.0, 106.0, 98.0, 166.0), GREEN);
    rec.pop();
    rec.finish()
}

/// Uniform, per-corner, oversized and tiny radii, and a circle.
pub fn rrects() -> Picture {
    let mut rec = Recorder::new();
    rec.fill_rrect(rect(8.0, 8.0, 56.0, 40.0), 8.0, RED);
    rec.fill_rrect(rect(64.5, 8.5, 120.5, 40.5), kurbo::RoundedRectRadii::new(0.0, 12.0, 4.0, 20.0), BLUE);
    rec.fill_rrect(rect(8.0, 50.0, 56.0, 70.0), 40.0, GREEN);
    rec.fill_rrect(rect(64.0, 50.0, 80.0, 66.0), 8.0, INK);
    rec.fill_rrect(rect(88.25, 50.25, 120.25, 88.25), 1.5, GREEN.with_alpha(160));
    rec.finish()
}

/// A self-crossing star (non-zero fills its centre), a ring (a hole by opposite winding), curves, and an aliased
/// triangle.
pub fn paths() -> Picture {
    let mut rec = Recorder::new();
    let star: Vec<(f64, f64)> = (0..5)
        .map(|i| {
            let a = std::f64::consts::PI * (0.5 + 0.8 * i as f64);
            (32.0 + 26.0 * a.cos(), 34.0 - 26.0 * a.sin())
        })
        .collect();
    let mut s = polyline(&star);
    s.close_path();
    rec.fill_path(s.elements(), RED);
    let mut ring = kurbo::Circle::new((92.0, 32.0), 24.0).to_path(0.01);
    ring.extend(kurbo::Circle::new((92.0, 32.0), 12.0).to_path(0.01).reverse_subpaths());
    rec.fill_path(ring.elements(), BLUE);
    let mut blob = BezPath::new();
    blob.move_to((8.0, 90.0));
    blob.quad_to((30.0, 50.0), (52.0, 90.0));
    blob.curve_to((40.0, 70.0), (20.0, 110.0), (8.0, 90.0));
    rec.fill_path(blob.elements(), GREEN);
    let mut tri = polyline(&[(70.0, 70.5), (120.3, 80.0), (78.0, 110.7)]);
    tri.close_path();
    rec.fill_path(tri.elements(), aliased(INK));
    rec.finish()
}

/// Every join with every cap on a zigzag, dashes on an axis-aligned line and a diagonal, a curve, and crisp hairlines.
pub fn strokes() -> Picture {
    let mut rec = Recorder::new();
    let joins = [Join::Miter, Join::Round, Join::Bevel];
    let caps = [Cap::Butt, Cap::Round, Cap::Square];
    for (row, join) in joins.iter().enumerate() {
        for (col, cap) in caps.iter().enumerate() {
            let (x, y) = (10.0 + col as f64 * 44.0, 12.0 + row as f64 * 26.0);
            let zig = polyline(&[(x, y + 12.0), (x + 10.0, y), (x + 20.0, y + 12.0), (x + 30.0, y + 2.0)]);
            rec.stroke_path(zig.elements(), &Stroke::new(4.0).with_join(*join).with_caps(*cap), INK);
        }
    }
    let dashed = Stroke::new(2.0).with_caps(Cap::Square).with_dashes(1.0, [4.0, 3.0]);
    rec.line((8.0, 96.0).into(), (120.0, 96.0).into(), &dashed, BLUE);
    rec.line((8.0, 104.0).into(), (120.0, 124.0).into(), &Stroke::new(1.5).with_caps(Cap::Butt).with_dashes(0.0, [6.0, 2.0, 1.0, 2.0]), RED);
    let mut curve = BezPath::new();
    curve.move_to((10.0, 88.0));
    curve.curve_to((40.0, 60.0), (80.0, 120.0), (120.0, 84.0));
    rec.stroke_path(curve.elements(), &Stroke::new(1.0), GREEN);
    let hair = Stroke::new(1.0).with_caps(Cap::Butt);
    rec.line((4.0, 2.5).into(), (124.0, 2.5).into(), &hair, INK);
    rec.line((2.5, 4.0).into(), (2.5, 124.0).into(), &hair, INK);
    rec.finish()
}

/// Rect clips (anti-aliased on fractions, aliased), a path clip with a rect clip nested inside it, and a rotated rect
/// clip.
pub fn clips() -> Picture {
    let mut rec = Recorder::new();
    let stripes = |rec: &mut Recorder, area: Rect, c: Color| {
        let mut x = area.x0;
        while x < area.x1 {
            rec.fill_rect(rect(x, area.y0, x + 3.0, area.y1), c);
            x += 6.0;
        }
    };
    rec.push_clip(Clip::Rect { rect: rect(8.3, 8.6, 56.7, 40.4), aa: true });
    rec.fill_path(kurbo::Circle::new((32.0, 24.0), 30.0).to_path(0.01).elements(), RED);
    rec.pop();
    rec.push_clip(Clip::Rect { rect: rect(64.3, 8.6, 120.7, 40.4), aa: false });
    rec.fill_path(kurbo::Circle::new((92.0, 24.0), 30.0).to_path(0.01).elements(), BLUE);
    rec.pop();
    let circle = kurbo::Circle::new((32.0, 84.0), 26.0).to_path(0.01);
    rec.push_clip(Clip::Path(circle.elements()));
    stripes(&mut rec, rect(0.0, 56.0, 64.0, 112.0), GREEN);
    rec.push_clip(Clip::Rect { rect: rect(20.0, 70.0, 70.0, 98.0), aa: true });
    rec.fill_rect(rect(0.0, 0.0, 128.0, 128.0), INK.with_alpha(140));
    rec.pop();
    rec.pop();
    // A rotated clip; the stripes inside it are drawn unrotated.
    let pivot = kurbo::Point::new(96.0, 88.0);
    rec.push_transform(Affine::rotate_about(0.4, pivot));
    rec.push_clip(Clip::Rect { rect: rect(80.0, 72.0, 112.0, 104.0), aa: true });
    rec.push_transform(Affine::rotate_about(-0.4, pivot));
    stripes(&mut rec, rect(64.0, 56.0, 128.0, 120.0), BLUE);
    rec.finish()
}

/// Rects and paths under rotation, skew, a flip and a non-uniform scale.
pub fn transforms() -> Picture {
    let mut rec = Recorder::new();
    let shape = |rec: &mut Recorder, c: Color| {
        rec.fill_rect(rect(-12.0, -8.0, 12.0, 8.0), c);
        let mut tri = polyline(&[(-6.0, -4.0), (8.0, 0.0), (-6.0, 4.0)]);
        tri.close_path();
        rec.fill_path(tri.elements(), Color::WHITE);
    };
    let placements = [
        (Affine::translate((32.0, 32.0)) * Affine::rotate(0.5), RED),
        (Affine::translate((96.0, 32.0)) * Affine::skew(0.6, 0.0), BLUE),
        (Affine::translate((32.0, 96.0)) * Affine::FLIP_X * Affine::rotate(0.3), GREEN),
        (Affine::translate((96.0, 96.0)) * Affine::scale_non_uniform(2.0, 0.75) * Affine::rotate(-0.8), INK),
    ];
    for (t, c) in placements {
        rec.push_transform(t);
        shape(&mut rec, c);
        rec.pop();
    }
    rec.finish()
}

/// Multiply over coloured bands, a destination-in mask, and a clearing stroke.
pub fn blends() -> Picture {
    let mut rec = Recorder::new();
    for (i, c) in [RED, GREEN, BLUE, Color::rgb(0xF0_D000)].into_iter().enumerate() {
        rec.fill_rect(rect(0.0, i as f64 * 32.0, 128.0, i as f64 * 32.0 + 32.0), c);
    }
    rec.fill_rect(rect(8.0, 8.0, 56.0, 120.0), Paint { blend: BlendMode::Multiply, ..Color::rgb(0x80_8080).into() });
    rec.fill_path(
        kurbo::Circle::new((92.0, 40.0), 24.0).to_path(0.01).elements(),
        Paint { blend: BlendMode::DstIn, ..Color::BLACK.with_alpha(128).into() },
    );
    rec.line((70.0, 80.0).into(), (120.0, 120.0).into(), &Stroke::new(6.0), Paint { blend: BlendMode::Clear, ..Color::BLACK.into() });
    rec.finish()
}

/// The test face (a JetBrains Mono subset, OFL, from Twip's chrome fonts) and the glyphs and origins of `s` set at
/// `size` from `x`, by its advances, as Twip's layout hands runs over.
pub struct Mono {
    pub data: std::sync::Arc<Vec<u8>>,
}

impl Mono {
    pub fn new() -> Mono {
        Mono { data: std::sync::Arc::new(include_bytes!("../fonts/jetbrains-mono-500.ttf").to_vec()) }
    }

    pub fn set(&self, s: &str, size: f64, x: f64) -> Vec<(u32, f64)> {
        use skrifa::MetadataProvider;
        let font = skrifa::FontRef::new(&self.data).expect("the test font reads");
        let metrics = font.glyph_metrics(skrifa::instance::Size::new(size as f32), skrifa::instance::LocationRef::default());
        let mut x = x;
        s.chars()
            .map(|c| {
                let g = font.charmap().map(c).unwrap_or_default();
                let at = x;
                x += metrics.advance_width(g).unwrap_or_default() as f64;
                (g.to_u32(), at)
            })
            .collect()
    }
}

/// Text: sizes from 7 to 32 px, subpixel origins, synthetic bold and italic, colour, white on dark, a clip through
/// glyphs, a scaled run, a rotated bold run, a flipped and stretched run, and a glyph too large for a mask (drawn from
/// its outline).
pub fn text(resources: &mut Resources) -> Picture {
    use twip_graphics::scene::{GlyphRun, Synthesis};
    let mono = Mono::new();
    let font = resources.fonts.add(mono.data.clone(), 0, &[]).expect("the test face registers");
    let fonts = &resources.fonts;
    let mut rec = Recorder::new();
    let run = |rec: &mut Recorder, s: &str, size: f64, x: f64, baseline: f64, synthesis: Synthesis, c: Color| {
        let glyphs = mono.set(s, size, x);
        rec.glyphs(fonts, &GlyphRun { font, size, baseline, glyphs: &glyphs, synthesis }, c);
    };
    let plain = Synthesis::default();
    let mut y = 10.0;
    for size in [7.0, 9.0, 11.0, 14.0, 20.0, 32.0] {
        run(&mut rec, "Ag fi 0.5;", size, 4.0, y + size * 0.8, plain, INK);
        y += size * 1.2;
    }
    for (i, dx) in [0.0, 0.25, 0.5, 0.75].iter().enumerate() {
        run(&mut rec, "il", 12.0, 150.0 + i as f64 * 20.0 + dx, 20.0, plain, INK);
    }
    run(&mut rec, "Bold", 14.0, 150.0, 44.0, Synthesis { embolden: true, skew_x: 0.0 }, INK);
    run(&mut rec, "Italic", 14.0, 150.0, 62.0, Synthesis { embolden: false, skew_x: -0.2 }, INK);
    run(&mut rec, "Red", 14.0, 150.0, 80.0, plain, RED);
    rec.fill_rect(rect(146.0, 86.0, 250.0, 104.0), INK);
    run(&mut rec, "White", 14.0, 150.0, 100.0, plain, Color::WHITE);
    rec.push_clip(Clip::Rect { rect: rect(150.0, 110.0, 190.3, 121.5), aa: true });
    run(&mut rec, "Clipped", 14.0, 150.0, 122.0, plain, BLUE);
    rec.pop();
    rec.push_transform(Affine::translate((150.0, 130.0)) * Affine::scale(1.5));
    run(&mut rec, "x1.5", 12.0, 0.0, 12.0, plain, INK);
    rec.pop();
    rec.push_transform(Affine::rotate_about(-0.35, kurbo::Point::new(200.0, 175.0)));
    run(&mut rec, "Turned", 14.0, 170.0, 180.0, Synthesis { embolden: true, skew_x: 0.0 }, GREEN);
    rec.pop();
    rec.push_transform(Affine::translate((236.0, 120.0)) * Affine::FLIP_X * Affine::scale_non_uniform(1.0, 1.6));
    run(&mut rec, "Fx", 12.0, 0.0, 0.0, plain, RED);
    rec.pop();
    run(&mut rec, "g", 300.0, 40.0, 330.0, plain, BLUE.with_alpha(90));
    rec.finish()
}

/// Stroked glyph outlines (hollow text): at several sizes and widths, synthetic bold and italic, a colour and a
/// translucent ink, dashed, turned, over a dark band, around the same glyphs filled as paths, and a glyph without ink
/// (a space) and one the face lacks. Each stroked run carries its characters as invisible text.
pub fn stroked_text(resources: &mut Resources) -> Picture {
    use twip_graphics::scene::{GlyphRun, Synthesis};
    let mono = Mono::new();
    let font = resources.fonts.add(mono.data.clone(), 0, &[]).expect("the test face registers");
    let fonts = &resources.fonts;
    let mut rec = Recorder::new();
    let run = |rec: &mut Recorder, s: &str, size: f64, x: f64, baseline: f64, synthesis: Synthesis, stroke: &Stroke, c: Color| {
        let glyphs = mono.set(s, size, x);
        let run = GlyphRun { font, size, baseline, glyphs: &glyphs, synthesis };
        rec.stroke_glyphs(fonts, &run, stroke, c);
        // The run's characters, invisible: nothing more draws, on screen or in a PDF.
        let clusters: Vec<u32> = (0..s.len() as u32).collect();
        rec.invisible_text(fonts, &run, s, &clusters);
    };
    let plain = Synthesis::default();
    let mut y = 6.0;
    for (size, w) in [(11.0, 0.5), (16.0, 0.5), (24.0, 0.75), (40.0, 1.5)] {
        run(&mut rec, "Ag o", size, 4.0, y + size * 0.8, plain, &Stroke::new(w), INK);
        y += size * 1.15;
    }
    let thin = Stroke::new(0.6);
    run(&mut rec, "Bold", 18.0, 140.0, 22.0, Synthesis { embolden: true, skew_x: 0.0 }, &thin, INK);
    run(&mut rec, "Italic", 18.0, 140.0, 44.0, Synthesis { embolden: false, skew_x: -0.2 }, &thin, RED);
    run(&mut rec, "Both", 18.0, 140.0, 66.0, Synthesis { embolden: true, skew_x: -0.2 }, &thin, BLUE.with_alpha(140));
    run(&mut rec, "Dash", 18.0, 140.0, 88.0, plain, &Stroke::new(0.8).with_dashes(0.0, [2.0, 1.5]), GREEN);
    rec.fill_rect(rect(136.0, 94.0, 256.0, 118.0), INK);
    run(&mut rec, "On ink", 18.0, 140.0, 112.0, plain, &thin, Color::WHITE);
    rec.push_transform(Affine::rotate_about(-0.3, kurbo::Point::new(190.0, 150.0)));
    run(&mut rec, "Turned", 20.0, 150.0, 156.0, plain, &thin, INK);
    rec.pop();
    let glyphs = mono.set("Path", 18.0, 4.0);
    let bold = GlyphRun { font, size: 18.0, baseline: 190.0, glyphs: &glyphs, synthesis: Synthesis { embolden: true, skew_x: -0.2 } };
    rec.glyph_paths(fonts, &bold, BLUE);
    rec.stroke_glyphs(fonts, &bold, &thin, INK);
    let lacked = [(u32::MAX, 140.0), (mono.set(" ", 18.0, 0.0)[0].0, 160.0)];
    rec.stroke_glyphs(fonts, &GlyphRun { font, size: 18.0, baseline: 190.0, glyphs: &lacked, synthesis: plain }, &thin, INK);
    rec.finish()
}

/// Glyphs painted as text effects draw them: filled with linear and radial gradients (as text, and as paths), stroked
/// with a gradient, a blurred translucent copy under them (a shadow), a blurred grown copy (a glow), and a flipped
/// copy faded by a destination-in gradient (a reflection), plain and synthetic bold and italic, and turned.
pub fn text_effects(resources: &mut Resources) -> Picture {
    use twip_graphics::paint::{Brush, Extend, Stop};
    use twip_graphics::scene::{Filter, GlyphRun, Layer, Synthesis};
    let mono = Mono::new();
    let font = resources.fonts.add(mono.data.clone(), 0, &[]).expect("the test face registers");
    let fonts = &resources.fonts;
    let mut rec = Recorder::new();
    let stops = [Stop { offset: 0.0, color: RED }, Stop { offset: 1.0, color: BLUE }];
    let down = |y0: f64, y1: f64| {
        Paint::new(Brush::Linear {
            start: (0.0, y0).into(),
            end: (0.0, y1).into(),
            stops: &stops,
            extend: Extend::Clamp,
            transform: Affine::IDENTITY,
        })
    };
    let plain = Synthesis::default();
    let glyphs = mono.set("Grad", 28.0, 6.0);
    let run = GlyphRun { font, size: 28.0, baseline: 30.0, glyphs: &glyphs, synthesis: plain };
    rec.glyphs(fonts, &run, down(8.0, 36.0));
    let glyphs = mono.set("Path", 28.0, 130.0);
    let run = GlyphRun { font, size: 28.0, baseline: 30.0, glyphs: &glyphs, synthesis: Synthesis { embolden: true, skew_x: -0.2 } };
    rec.glyph_paths(fonts, &run, down(8.0, 36.0));
    let glyphs = mono.set("Ring", 28.0, 6.0);
    let run = GlyphRun { font, size: 28.0, baseline: 66.0, glyphs: &glyphs, synthesis: plain };
    let radial = Brush::Radial {
        start: (40.0, 56.0).into(),
        start_radius: 0.0,
        end: (40.0, 56.0).into(),
        end_radius: 36.0,
        stops: &stops,
        extend: Extend::Clamp,
        transform: Affine::IDENTITY,
    };
    rec.glyphs(fonts, &run, Paint::new(radial));
    let glyphs = mono.set("Line", 28.0, 130.0);
    let run = GlyphRun { font, size: 28.0, baseline: 66.0, glyphs: &glyphs, synthesis: plain };
    rec.glyphs(fonts, &run, Color::WHITE);
    rec.stroke_glyphs(fonts, &run, &Stroke::new(1.5).with_join(Join::Round), down(44.0, 72.0));
    // A shadow: blurred, at the shadow colour's alpha, under the text.
    let glyphs = mono.set("Shade", 26.0, 6.0);
    let run = GlyphRun { font, size: 26.0, baseline: 104.0, glyphs: &glyphs, synthesis: plain };
    rec.push_layer(&Layer { opacity: 0.6, filters: &[Filter::Blur { sigma: 1.5 }], ..Layer::default() });
    rec.push_transform(Affine::translate((2.5, 2.5)));
    rec.glyphs(fonts, &run, INK);
    rec.pop();
    rec.pop();
    rec.glyphs(fonts, &run, INK);
    // A glow: the glyphs filled and stroked wide, blurred.
    let glyphs = mono.set("Glow", 26.0, 136.0);
    let run = GlyphRun { font, size: 26.0, baseline: 104.0, glyphs: &glyphs, synthesis: plain };
    rec.push_layer(&Layer { opacity: 0.5, filters: &[Filter::Blur { sigma: 2.0 }], ..Layer::default() });
    rec.glyph_paths(fonts, &run, BLUE);
    rec.stroke_glyphs(fonts, &run, &Stroke::new(4.0).with_join(Join::Round), BLUE);
    rec.pop();
    rec.glyphs(fonts, &run, INK);
    // A reflection: the glyphs flipped about their baseline, faded down from half alpha.
    let glyphs = mono.set("Mirror", 26.0, 6.0);
    let run = GlyphRun { font, size: 26.0, baseline: 150.0, glyphs: &glyphs, synthesis: plain };
    rec.glyphs(fonts, &run, down(128.0, 156.0));
    let fade = [Stop { offset: 0.0, color: Color::BLACK.with_alpha(128) }, Stop { offset: 1.0, color: Color::TRANSPARENT }];
    let area = rect(0.0, 150.0, 140.0, 172.0);
    rec.push_layer(&Layer { filters: &[Filter::Blur { sigma: 0.5 }], bounds: Some(area), ..Layer::default() });
    rec.push_transform(Affine::translate((0.0, 300.0)) * Affine::scale_non_uniform(1.0, -1.0));
    rec.glyphs(fonts, &run, down(128.0, 156.0));
    rec.pop();
    rec.push_layer(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
    let mask =
        Brush::Linear { start: (0.0, 150.0).into(), end: (0.0, 162.0).into(), stops: &fade, extend: Extend::Clamp, transform: Affine::IDENTITY };
    rec.fill_rect(area, Paint::new(mask));
    rec.pop();
    rec.pop();
    // A gradient under a turn: the paint turns with the glyphs.
    rec.push_transform(Affine::rotate_about(-0.3, kurbo::Point::new(200.0, 150.0)));
    let glyphs = mono.set("Turn", 24.0, 160.0);
    let run = GlyphRun { font, size: 24.0, baseline: 160.0, glyphs: &glyphs, synthesis: plain };
    rec.glyphs(fonts, &run, down(140.0, 166.0));
    rec.pop();
    rec.finish()
}

/// Hinted text off the upright: runs turned a quarter, a half and three quarters (as vertical sections and turned
/// cells draw them) at subpixel origins, against a run at 30 degrees (unhinted); and a glyph either side of the
/// largest em drawn from a mask (250 px, a mask, and 262 px, its outline as a path), black and white on black.
pub fn turned_text(resources: &mut Resources) -> Picture {
    use twip_graphics::scene::{GlyphRun, Synthesis};
    let mono = Mono::new();
    let font = resources.fonts.add(mono.data.clone(), 0, &[]).expect("the test face registers");
    let fonts = &resources.fonts;
    let mut rec = Recorder::new();
    let run = |rec: &mut Recorder, s: &str, size: f64, x: f64, baseline: f64, c: Color| {
        let glyphs = mono.set(s, size, x);
        rec.glyphs(fonts, &GlyphRun { font, size, baseline, glyphs: &glyphs, synthesis: Synthesis::default() }, c);
    };
    let quarter = std::f64::consts::FRAC_PI_2;
    for (i, size) in [9.0, 11.0, 14.0].into_iter().enumerate() {
        let (x, y) = (8.3 + i as f64 * 18.0, 14.2 + i as f64 * 20.0);
        for (turn, at) in [(quarter, (x, 8.0)), (-quarter, (x + 64.0, 150.0)), (2.0 * quarter, (350.0, y))] {
            rec.push_transform(Affine::translate(at) * Affine::rotate(turn));
            run(&mut rec, "Hinted 0.5 Eg", size, 0.0, 0.0, INK);
            rec.pop();
        }
        rec.push_transform(Affine::translate((150.0 + i as f64 * 70.0, 85.0)) * Affine::rotate(0.52));
        run(&mut rec, "Angled 0.5", size, 0.0, 0.0, INK);
        rec.pop();
    }
    rec.fill_rect(rect(280.0, 160.0, 560.0, 400.0), INK);
    for (size, x, c) in [(250.0, -20.0, INK), (262.0, 110.0, INK), (250.0, 270.0, Color::WHITE), (262.0, 400.0, Color::WHITE)] {
        run(&mut rec, "l", size, x, 380.0, c);
    }
    rec.finish()
}

pub fn image(resources: &mut Resources, bytes: &'static [u8]) -> twip_graphics::image::ImageId {
    resources.images.add(std::sync::Arc::new(bytes)).expect("the test image registers")
}

/// Images: PNG, JPEG with EXIF orientation 6 (drawn upright), GIF, BMP, ICO (holding a PNG) and WBMP; a source rect, nearest against bilinear when
/// enlarged, a mip level when shrunk, a rotated draw, colour filters (grayscale, an inverting table), and a pattern.
pub fn images(resources: &mut Resources) -> Picture {
    use twip_graphics::paint::{Brush, ColorFilter, ImagePaint, Sampling};
    let rgba: Vec<u8> = (0..16u32).flat_map(|i| [(i * 16) as u8, 255 - (i * 16) as u8, 128, if i % 5 == 0 { 96 } else { 255 }]).collect();
    let tiny = resources.images.add_pixels(4, 4, rgba).expect("pixels");
    let jpeg = image(resources, include_bytes!("../images/rotated-6.jpg"));
    let gif = image(resources, include_bytes!("../images/palette.gif"));
    let bmp = image(resources, include_bytes!("../images/plain.bmp"));
    // A 4 × 4 PNG, the same PNG inside an ICO, and a 4 × 4 WBMP checkerboard, built here.
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, 4, 4);
        encoder.set_color(png::ColorType::Rgba);
        let pixels: Vec<u8> = (0..16u32).flat_map(|i| [255 - (i * 16) as u8, 64, (i * 16) as u8, 255]).collect();
        encoder.write_header().expect("a header").write_image_data(&pixels).expect("pixels");
    }
    let mut ico = vec![0, 0, 1, 0, 1, 0, 4, 4, 0, 0, 1, 0, 32, 0];
    ico.extend((png.len() as u32).to_le_bytes());
    ico.extend(22u32.to_le_bytes());
    ico.extend(&png);
    let wbmp = vec![0, 0, 4, 4, 0b1010_0000, 0b0101_0000, 0b1010_0000, 0b0101_0000];
    let [png, ico, wbmp] = [png, ico, wbmp].map(|b| resources.images.add(std::sync::Arc::new(b)).expect("the test image registers"));
    let nearest = ImagePaint { sampling: Sampling::Nearest, color_filters: &[] };
    let linear = ImagePaint::default();
    let mut rec = Recorder::new();
    rec.image(jpeg, None, rect(4.0, 4.0, 36.0, 52.0), &linear);
    rec.image(gif, None, rect(44.0, 4.0, 68.0, 28.0), &linear);
    rec.image(bmp, Some(rect(8.0, 8.0, 40.0, 24.0)), rect(76.0, 4.0, 124.0, 28.0), &linear);
    rec.image(tiny, None, rect(4.0, 60.0, 36.0, 92.0), &nearest);
    rec.image(tiny, None, rect(44.0, 60.0, 76.0, 92.0), &linear);
    rec.image(bmp, None, rect(84.0, 60.0, 96.0, 68.0), &linear);
    rec.push_transform(Affine::rotate_about(0.3, kurbo::Point::new(108.0, 80.0)));
    rec.image(bmp, None, rect(96.0, 72.0, 120.0, 88.0), &linear);
    rec.pop();
    let grey = [0.2126, 0.7152, 0.0722, 0.0, 0.0, 0.2126, 0.7152, 0.0722, 0.0, 0.0, 0.2126, 0.7152, 0.0722, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let invert: [u8; 256] = std::array::from_fn(|i| if i == 255 { 255 } else { 255 - i as u8 });
    rec.image(jpeg, None, rect(4.0, 100.0, 28.0, 136.0), &ImagePaint { color_filters: &[ColorFilter::Matrix(grey)], ..linear });
    rec.image(jpeg, None, rect(32.0, 100.0, 56.0, 136.0), &ImagePaint { color_filters: &[ColorFilter::Table(invert)], ..linear });
    let pattern = Brush::Pattern { image: tiny, transform: Affine::scale(3.0), sampling: Sampling::Nearest };
    rec.fill_path(kurbo::Circle::new((92.0, 118.0), 18.0).to_path(0.01).elements(), Paint::new(pattern));
    rec.image(png, None, rect(60.0, 100.0, 72.0, 112.0), &nearest);
    rec.image(ico, None, rect(60.0, 116.0, 72.0, 128.0), &nearest);
    rec.image(wbmp, None, rect(112.0, 100.0, 124.0, 112.0), &nearest);
    rec.finish()
}

/// JPEG XR pictures made by jxrlib: 4:2:0 lossy RGB with both overlap levels, lossy grey, and RGBA with interleaved and
/// with separate, premultiplied alpha, both over a checkerboard so their transparency shows.
pub fn jpeg_xr(resources: &mut Resources) -> Picture {
    let rgb = image(resources, include_bytes!("../images/rgb-420.jxr"));
    let gray = image(resources, include_bytes!("../images/gray.jxr"));
    let alpha = image(resources, include_bytes!("../images/alpha.jxr"));
    let premultiplied = image(resources, include_bytes!("../images/premultiplied.jxr"));
    let linear = twip_graphics::paint::ImagePaint::default();
    let mut rec = Recorder::new();
    for y in 0..6 {
        for x in 0..16 {
            if (x + y) % 2 == 0 {
                let (x, y) = (x as f64 * 8.0, 64.0 + y as f64 * 8.0);
                rec.fill_rect(rect(x, y, x + 8.0, y + 8.0), Color::rgb(0xA0_C4E8));
            }
        }
    }
    rec.image(rgb, None, rect(4.0, 4.0, 64.0, 54.0), &linear);
    rec.image(gray, None, rect(72.0, 4.0, 119.0, 43.0), &linear);
    rec.image(alpha, None, rect(0.0, 66.0, 64.0, 96.0), &linear);
    rec.image(premultiplied, None, rect(64.0, 66.0, 128.0, 96.0), &linear);
    rec.finish()
}

/// Pixel tiles: an 8 × 8 one-pixel stripe tile drawn at fractional scales below one device pixel a texel (nearest
/// drops and doubles stripes) and at two above, under a fractional offset; snapped, a texel a pixel at every scale;
/// and rotated (left as nearest).
pub fn pixel_tiles(resources: &mut Resources) -> Picture {
    use twip_graphics::paint::{Brush, Sampling};
    let bits = [0xC0u8, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0xC0, 0x81];
    let px: Vec<u8> =
        bits.iter().flat_map(|row| (0..8).flat_map(move |i| if row & (0x80 >> i) != 0 { [192, 0, 0, 255] } else { [255, 255, 0, 255] })).collect();
    let tile = resources.images.add_pixels(8, 8, px).expect("pixels");
    let mut rec = Recorder::new();
    let row = |rec: &mut Recorder, y: f64, sampling: Sampling| {
        for (k, scale) in [0.32, 0.41, 0.77, 1.6, 2.3].into_iter().enumerate() {
            let x = 4.0 + k as f64 * 24.0;
            let transform = Affine::translate((x + 0.3, y + 0.6)) * Affine::scale(scale);
            rec.fill_rect(rect(x, y, x + 20.0, y + 20.0), Paint::new(Brush::Pattern { image: tile, transform, sampling }));
        }
    };
    row(&mut rec, 4.0, Sampling::Nearest);
    row(&mut rec, 28.0, Sampling::Pixel);
    rec.push_transform(Affine::rotate_about(0.4, kurbo::Point::new(64.0, 70.0)));
    let transform = Affine::translate((44.0, 56.0)) * Affine::scale(0.77);
    rec.fill_rect(rect(44.0, 56.0, 84.0, 84.0), Paint::new(Brush::Pattern { image: tile, transform, sampling: Sampling::Pixel }));
    rec.pop();
    rec.finish()
}

/// Word's five border dash patterns measured on the device, on lines 0.4, 1 and 3 units thick, at a scale of 1, 0.5
/// and 2 (a thin line's dashes stay their size in pixels, a thick one's scale with it); then vertical ones, a
/// fractional start, and a turned one.
pub fn dashed_rects() -> Picture {
    const PATTERNS: [&[u8]; 5] = [&[1, 1], &[4, 4], &[7, 3, 3, 3], &[6, 2, 2, 2, 2, 2], &[4, 1]];
    let mut rec = Recorder::new();
    for (y, scale) in [(4.0, 1.0), (40.0, 0.5), (62.0, 2.0)] {
        rec.push_transform(Affine::translate((0.0, y)) * Affine::scale(scale));
        for (i, pattern) in PATTERNS.into_iter().enumerate() {
            for (j, thick) in [0.4, 1.0, 3.0].into_iter().enumerate() {
                let (x, y) = (4.0 + j as f64 * 40.0, i as f64 * 6.0);
                rec.fill_dashed_rect(rect(x, y, x + 36.0, y + thick), pattern, INK);
            }
        }
        rec.pop();
    }
    for (k, (thick, pattern)) in [(1.0, PATTERNS[1]), (3.0, PATTERNS[2]), (2.0, PATTERNS[4])].into_iter().enumerate() {
        let x = 4.0 + k as f64 * 8.0;
        rec.fill_dashed_rect(rect(x, 132.0, x + thick, 172.0), pattern, BLUE);
    }
    rec.fill_dashed_rect(rect(40.3, 134.6, 100.0, 136.6), PATTERNS[1], RED);
    rec.push_transform(Affine::rotate_about(0.3, kurbo::Point::new(150.0, 150.0)));
    rec.fill_dashed_rect(rect(110.0, 150.0, 190.0, 152.0), PATTERNS[2], GREEN);
    rec.pop();
    rec.finish()
}

/// Gradients: linear in each extend mode, radial, two-point conical, sweep, on a rounded rect, and under rotation.
pub fn gradients() -> Picture {
    use twip_graphics::paint::{Brush, Extend, Stop};
    let stops =
        [Stop { offset: 0.0, color: RED }, Stop { offset: 0.5, color: Color::rgb(0xF0_D000).with_alpha(160) }, Stop { offset: 1.0, color: BLUE }];
    let mut rec = Recorder::new();
    for (i, extend) in [Extend::Clamp, Extend::Repeat, Extend::Mirror].into_iter().enumerate() {
        let y = 4.0 + i as f64 * 20.0;
        let brush = Brush::Linear { start: (40.0, 0.0).into(), end: (70.0, 0.0).into(), stops: &stops, extend, transform: Affine::IDENTITY };
        rec.fill_rect(rect(4.0, y, 124.0, y + 16.0), Paint::new(brush));
    }
    let radial = Brush::Radial {
        start: (28.0, 88.0).into(),
        start_radius: 0.0,
        end: (28.0, 88.0).into(),
        end_radius: 22.0,
        stops: &stops,
        extend: Extend::Clamp,
        transform: Affine::IDENTITY,
    };
    rec.fill_rect(rect(4.0, 66.0, 52.0, 110.0), Paint::new(radial));
    // A focal gradient: the start circle inside the end one, so every point has a t.
    let conical = Brush::Radial {
        start: (70.0, 80.0).into(),
        start_radius: 2.0,
        end: (78.0, 88.0).into(),
        end_radius: 20.0,
        stops: &stops,
        extend: Extend::Clamp,
        transform: Affine::IDENTITY,
    };
    rec.fill_rect(rect(56.0, 66.0, 100.0, 110.0), Paint::new(conical));
    let sweep = Brush::Sweep {
        center: (112.0, 88.0).into(),
        start_angle: 0.0,
        end_angle: 270.0,
        stops: &stops,
        extend: Extend::Clamp,
        transform: Affine::IDENTITY,
    };
    rec.fill_path(kurbo::Circle::new((112.0, 88.0), 14.0).to_path(0.01).elements(), Paint::new(sweep));
    let rotated =
        Brush::Linear { start: (0.0, 0.0).into(), end: (12.0, 0.0).into(), stops: &stops, extend: Extend::Mirror, transform: Affine::rotate(0.7) };
    rec.fill_rrect(rect(4.0, 114.0, 60.0, 140.0), 8.0, Paint::new(rotated));
    rec.push_transform(Affine::rotate_about(-0.4, kurbo::Point::new(94.0, 127.0)));
    let tilted =
        Brush::Linear { start: (70.0, 0.0).into(), end: (118.0, 0.0).into(), stops: &stops, extend: Extend::Clamp, transform: Affine::IDENTITY };
    rec.fill_rect(rect(70.0, 118.0, 118.0, 136.0), Paint::new(tilted));
    rec.pop();
    rec.finish()
}

/// Layers: opacity, a multiply layer, a blurred shadow, erode then blur (a soft edge), a colour-filter layer, nested
/// layers, a layer under a path clip, and a blur next to a clip edge pulling in what is just outside it.
pub fn layers() -> Picture {
    use twip_graphics::paint::ColorFilter;
    use twip_graphics::scene::{Filter, Layer};
    let mut rec = Recorder::new();
    for (i, c) in [RED, GREEN, BLUE, Color::rgb(0xF0_D000)].into_iter().enumerate() {
        rec.fill_rect(rect(i as f64 * 32.0, 0.0, i as f64 * 32.0 + 32.0, 8.0), c);
    }
    let two = |rec: &mut Recorder, x: f64, y: f64| {
        rec.fill_rect(rect(x, y, x + 24.0, y + 24.0), RED);
        rec.fill_path(kurbo::Circle::new((x + 22.0, y + 22.0), 12.0).to_path(0.01).elements(), BLUE);
    };
    rec.push_layer(&Layer { opacity: 0.5, ..Layer::default() });
    two(&mut rec, 4.0, 12.0);
    rec.pop();
    rec.fill_rect(rect(48.0, 12.0, 84.0, 48.0), GREEN);
    rec.push_layer(&Layer { blend: BlendMode::Multiply, ..Layer::default() });
    two(&mut rec, 52.0, 16.0);
    rec.pop();
    rec.push_layer(&Layer { opacity: 0.6, filters: &[Filter::Blur { sigma: 3.0 }], ..Layer::default() });
    rec.fill_rrect(rect(96.0, 18.0, 120.0, 42.0), 4.0, INK);
    rec.pop();
    rec.fill_rrect(rect(92.0, 14.0, 116.0, 38.0), 4.0, Color::rgb(0xF0_D000));
    rec.push_layer(&Layer { filters: &[Filter::Erode { radius: 2.0 }, Filter::Blur { sigma: 2.0 }], ..Layer::default() });
    rec.fill_path(kurbo::Circle::new((20.0, 76.0), 16.0).to_path(0.01).elements(), GREEN);
    rec.pop();
    let sepia = [0.393, 0.769, 0.189, 0.0, 0.0, 0.349, 0.686, 0.168, 0.0, 0.0, 0.272, 0.534, 0.131, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    rec.push_layer(&Layer { color_filters: &[ColorFilter::Matrix(sepia)], ..Layer::default() });
    two(&mut rec, 44.0, 60.0);
    rec.pop();
    rec.push_layer(&Layer { opacity: 0.7, ..Layer::default() });
    rec.push_layer(&Layer { blend: BlendMode::Screen, ..Layer::default() });
    two(&mut rec, 88.0, 60.0);
    rec.pop();
    rec.pop();
    let ring = kurbo::Circle::new((28.0, 120.0), 18.0).to_path(0.01);
    rec.push_clip(Clip::Path(ring.elements()));
    rec.push_layer(&Layer { opacity: 0.8, filters: &[Filter::Blur { sigma: 1.5 }], ..Layer::default() });
    two(&mut rec, 12.0, 104.0);
    rec.pop();
    rec.pop();
    rec.push_clip(Clip::Rect { rect: rect(64.0, 100.0, 124.0, 140.0), aa: true });
    rec.push_layer(&Layer { filters: &[Filter::Blur { sigma: 4.0 }], ..Layer::default() });
    rec.fill_rect(rect(54.0, 110.0, 70.0, 130.0), BLUE);
    rec.pop();
    rec.pop();
    rec.finish()
}

/// Every blend mode: an opaque orange circle and a half-transparent blue square under each mode, over a light grey
/// and black backdrop, one mode per cell (row by row in the order `BlendMode` lists them).
pub fn blend_modes() -> Picture {
    use twip_graphics::scene::Layer;
    let modes = [
        BlendMode::Clear,
        BlendMode::Src,
        BlendMode::Dst,
        BlendMode::SrcOver,
        BlendMode::DstOver,
        BlendMode::SrcIn,
        BlendMode::DstIn,
        BlendMode::SrcOut,
        BlendMode::DstOut,
        BlendMode::SrcAtop,
        BlendMode::DstAtop,
        BlendMode::Xor,
        BlendMode::Plus,
        BlendMode::Screen,
        BlendMode::Overlay,
        BlendMode::Darken,
        BlendMode::Lighten,
        BlendMode::ColorDodge,
        BlendMode::ColorBurn,
        BlendMode::HardLight,
        BlendMode::SoftLight,
        BlendMode::Difference,
        BlendMode::Exclusion,
        BlendMode::Multiply,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
    ];
    let mut rec = Recorder::new();
    for (i, mode) in modes.into_iter().enumerate() {
        let (x, y) = ((i % 7) as f64 * 36.0 + 2.0, (i / 7) as f64 * 36.0 + 2.0);
        rec.fill_rect(rect(x, y, x + 32.0, y + 16.0), Color::rgb(0xDD_DDDD));
        rec.fill_rect(rect(x, y + 16.0, x + 32.0, y + 32.0), INK);
        rec.push_layer(&Layer::default());
        rec.fill_path(kurbo::Circle::new((x + 12.0, y + 12.0), 10.0).to_path(0.01).elements(), Color::rgb(0xF0_8020));
        rec.fill_rect(rect(x + 10.0, y + 10.0, x + 30.0, y + 30.0), Paint { blend: mode, ..BLUE.with_alpha(170).into() });
        rec.pop();
    }
    rec.finish()
}

/// The sixteen GDI raster operations, one a tile: an anti-aliased disc and a half-transparent square, each over a
/// destination of four colours whose bits differ, on white.
pub fn raster_ops() -> Picture {
    let mut rec = Recorder::new();
    // An opaque ground: what the operations read between the tiles too.
    rec.fill_rect(rect(0.0, 0.0, 162.0, 162.0), Color::WHITE);
    for op in 0..16u8 {
        let (x, y) = (f64::from(op % 4) * 40.0 + 2.0, f64::from(op / 4) * 40.0 + 2.0);
        for (k, c) in [0x00_0000, 0xFF_FFFF, 0x5A_A50F, 0xC3_3C96].into_iter().enumerate() {
            let k = k as f64;
            rec.fill_rect(rect(x, y + k * 9.0, x + 36.0, y + k * 9.0 + 9.0), Color::rgb(c));
        }
        let paint = |c: Color| Paint { blend: BlendMode::Raster(op), ..c.into() };
        rec.fill_path(kurbo::Circle::new((x + 12.0, y + 12.0), 9.5).to_path(0.01).elements(), paint(Color::rgb(0xF0_8020)));
        rec.fill_rect(rect(x + 18.0, y + 18.0, x + 34.0, y + 34.0), paint(BLUE.with_alpha(170)));
    }
    rec.finish()
}

/// Colour glyphs: COLRv0 and COLRv1 glyphs from fontations' test font (every paint type), a CBDT and an sbix glyph,
/// and CFF outlines (Noto Sans JP), at 24 px.
pub fn colour_glyphs(resources: &mut Resources) -> Picture {
    use skrifa::MetadataProvider;
    use skrifa::raw::TableProvider;
    use twip_graphics::scene::{GlyphRun, Synthesis};
    let mut rec = Recorder::new();
    let row = |rec: &mut Recorder, resources: &mut Resources, data: &'static [u8], ids: &[u32], y: f64, size: f64| {
        let font = resources.fonts.add(std::sync::Arc::new(data), 0, &[]).expect("the test face registers");
        let glyphs: Vec<(u32, f64)> = ids.iter().enumerate().map(|(i, &g)| (g, 4.0 + i as f64 * (size + 4.0))).collect();
        rec.glyphs(&resources.fonts, &GlyphRun { font, size, baseline: y, glyphs: &glyphs, synthesis: Synthesis::default() }, INK);
    };
    let colr = skrifa::FontRef::new(font_test_data::COLRV0V1).expect("reads");
    let ids: Vec<u32> =
        (0..colr.maxp().map_or(0, |m| m.num_glyphs() as u32)).filter(|&g| colr.color_glyphs().get(skrifa::GlyphId::new(g)).is_some()).collect();
    for (r, chunk) in ids.chunks(9).take(5).enumerate() {
        row(&mut rec, resources, font_test_data::COLRV0V1, chunk, 26.0 + r as f64 * 28.0, 24.0);
    }
    row(&mut rec, resources, font_test_data::CBDT, &[1, 2, 3], 172.0, 24.0);
    // The subsets' first glyphs are empty: the sbix bitmap is glyph 7, the CFF outlines are A, B, C, M, N, P and Z.
    row(&mut rec, resources, font_test_data::NOTO_HANDWRITING_SBIX, &[7], 200.0, 24.0);
    row(&mut rec, resources, font_test_data::NOTO_SANS_JP_CFF, &[34, 35, 36, 46, 47, 49, 59], 228.0, 24.0);
    rec.finish()
}
