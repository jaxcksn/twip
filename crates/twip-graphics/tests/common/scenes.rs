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
    data: std::sync::Arc<Vec<u8>>,
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
