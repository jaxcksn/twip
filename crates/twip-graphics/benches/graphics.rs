//! The crate's benchmarks: `cargo bench -p twip-graphics`. Each states its budget in its doc comment; compare results
//! with earlier runs on the same machine.
//!
//! The page is synthetic, in points (US Letter, 612 × 792): paper, 46 lines of 11 pt text (about 80 glyphs each, set
//! in the test face by its advances), underlines, shading, a bordered table, shapes and dashed rules. It is drawn as
//! Twip draws a page: into a 1000 × 800 viewport at 96/72 pixels per point, times the device pixel ratio.

use std::sync::Arc;

use divan::Bencher;
use skrifa::MetadataProvider;
use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};
use twip_graphics::font::{FontData, FontId};
use twip_graphics::kurbo::{self, Affine, BezPath, Cap, Rect, Shape, Stroke};
use twip_graphics::paint::{Color, Paint};
use twip_graphics::scene::{GlyphRun, Picture, Recorder, Synthesis};

fn main() {
    divan::main();
}

const INK: Color = Color::rgb(0x20_2020);
const WORDS: &str = "the quick brown fox jumps over a lazy dog while seven wizards quietly hex five boxing judges ";

/// The test face, registered, and a line's worth of text set by its advances.
struct Text {
    resources: Resources,
    font: FontId,
    data: FontData,
}

impl Text {
    fn new() -> Text {
        let data: FontData = Arc::new(include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec());
        let mut resources = Resources::default();
        let font = resources.fonts.add(data.clone(), 0, &[]).expect("the test face registers");
        Text { resources, font, data }
    }

    /// The glyphs of `WORDS` from character `from`, at `size`, from `x` until `right`.
    fn set(&self, from: usize, size: f64, x: f64, right: f64) -> Vec<(u32, f64)> {
        let font = skrifa::FontRef::new((*self.data).as_ref()).expect("the test font reads");
        let metrics = font.glyph_metrics(skrifa::instance::Size::new(size as f32), skrifa::instance::LocationRef::default());
        let mut x = x;
        let mut out = Vec::new();
        for c in WORDS.chars().cycle().skip(from) {
            let g = font.charmap().map(c).unwrap_or_default();
            let advance = metrics.advance_width(g).unwrap_or_default() as f64;
            if x + advance > right {
                break;
            }
            out.push((g.to_u32(), x));
            x += advance;
        }
        out
    }
}

/// One line of the page at `y`: text, sometimes an underline or shading.
fn line(rec: &mut Recorder, text: &Text, glyphs: &[(u32, f64)], i: usize, y: f64) {
    if i.is_multiple_of(5) {
        rec.fill_rect(Rect::new(72.0, y - 9.0, 540.0, y + 3.0), Color::rgb(0xFF_F2CC));
    }
    let run = GlyphRun { font: text.font, size: 11.0, baseline: y, glyphs, synthesis: Synthesis::default() };
    rec.glyphs(&text.resources.fonts, &run, INK);
    if i % 4 == 1 {
        rec.fill_rect(Rect::new(72.0, y + 1.2, 260.0, y + 1.9), INK);
    }
}

fn page(rec: &mut Recorder, text: &Text, lines: &[Vec<(u32, f64)>]) -> Picture {
    rec.fill_rect(Rect::new(0.0, 0.0, 612.0, 792.0), aliased(Color::WHITE));
    for (i, glyphs) in lines.iter().enumerate() {
        line(rec, text, glyphs, i, 90.0 + i as f64 * 13.8);
    }
    // A 6 × 4 table's borders.
    for r in 0..=6 {
        let y = 560.0 + r as f64 * 18.0;
        rec.fill_rect(Rect::new(72.0, y, 540.0, y + 0.5), INK);
    }
    for c in 0..=4 {
        let x = 72.0 + c as f64 * 117.0;
        rec.fill_rect(Rect::new(x, 560.0, x + 0.5, 668.5), INK);
    }
    // Shapes with outlines.
    let circle = kurbo::Circle::new((150.0, 720.0), 30.0).to_path(0.01);
    rec.fill_path(circle.elements(), Color::rgb(0x44_72C4));
    rec.stroke_path(circle.elements(), &Stroke::new(1.5), INK);
    rec.fill_rrect(Rect::new(220.0, 690.0, 330.0, 750.0), 8.0, Color::rgb(0xED_7D31));
    let mut star = BezPath::new();
    for k in 0..5 {
        let a = std::f64::consts::PI * (0.5 + 0.8 * k as f64);
        let p = (440.0 + 32.0 * a.cos(), 720.0 - 32.0 * a.sin());
        if k == 0 { star.move_to(p) } else { star.line_to(p) }
    }
    star.close_path();
    rec.fill_path(star.elements(), Color::rgb(0x70_AD47));
    let rule = Stroke::new(0.75).with_caps(Cap::Butt).with_dashes(0.0, [3.0, 2.0]);
    rec.line((72.0, 772.0).into(), (540.0, 772.0).into(), &rule, INK);
    rec.finish()
}

fn lines(text: &Text) -> Vec<Vec<(u32, f64)>> {
    (0..46).map(|i| text.set(i * 17, 11.0, 72.0 + (i % 3) as f64 * 0.37, 540.0)).collect()
}

/// Aliased paint, as Twip paints the paper.
fn aliased(c: Color) -> Paint<'static> {
    Paint { aa: false, ..c.into() }
}

fn frame(b: Bencher, dpr: f64) {
    let text = Text::new();
    let picture = page(&mut Recorder::new(), &text, &lines(&text));
    let (w, h) = ((1000.0 * dpr) as u32, (800.0 * dpr) as u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut renderer = Renderer::new();
    let transform = Affine::scale(dpr) * Affine::translate((92.0, 16.0)) * Affine::scale(96.0 / 72.0);
    let draws = [Draw { picture: &picture, transform }];
    let frame = Frame { clear: Some(Color::rgb(0xE8_E8E8)), draws: &draws };
    b.bench_local(|| {
        let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("a buffer that fits");
        renderer.render(&mut target, &text.resources, &frame);
    });
}

/// An unchanged frame on the CPU at dpr 1: a retained text page replayed into the viewport, its glyph masks cached.
/// Budget: at most 1.2 ms (Skia's CPU replay of a real page today).
#[divan::bench]
fn cpu_unchanged_dpr1(b: Bencher) {
    frame(b, 1.0);
}

/// The same at dpr 2 (four times the pixels). Budget: at most 3.0 ms.
#[divan::bench]
fn cpu_unchanged_dpr2(b: Bencher) {
    frame(b, 2.0);
}

/// One glyph mask made from scratch: outline, rasterisation and gamma, a 12–40 px glyph each time (sizes and glyphs
/// cycle through more combinations than the cache holds between repeats). Budget: tens of microseconds; a page's
/// masks are made once per zoom.
#[divan::bench]
fn glyph_mask_miss(b: Bencher) {
    let text = Text::new();
    let glyphs = text.set(0, 1.0, 0.0, 60.0);
    let mut px = vec![0u8; 64 * 64 * 4];
    let mut renderer = Renderer::new();
    let mut i = 0u32;
    let mut rec = Recorder::new();
    b.bench_local(|| {
        i = i.wrapping_add(1);
        let glyph = glyphs[(i as usize) % glyphs.len()].0;
        let size = 12.0 + (i % 1792) as f64 / 64.0;
        let run = GlyphRun { font: text.font, size, baseline: 48.0, glyphs: &[(glyph, 8.0)], synthesis: Synthesis::default() };
        rec.glyphs(&text.resources.fonts, &run, INK);
        let picture = rec.finish();
        let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
        let mut target = Target::new(&mut px, 64, 64, 256, Format::Rgba8).expect("fits");
        renderer.render(&mut target, &text.resources, &Frame { clear: None, draws: &draws });
    });
}

/// A line of 80 cached glyphs blitted at 11 pt, dpr 1. Budget: a few microseconds.
#[divan::bench]
fn glyph_mask_hit(b: Bencher) {
    let text = Text::new();
    let glyphs = text.set(0, 11.0, 4.0, 600.0);
    let glyphs = &glyphs[..80];
    let mut rec = Recorder::new();
    rec.glyphs(&text.resources.fonts, &GlyphRun { font: text.font, size: 11.0, baseline: 12.0, glyphs, synthesis: Synthesis::default() }, INK);
    let picture = rec.finish();
    let (w, h) = (600u32, 20u32);
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut renderer = Renderer::new();
    let draws = [Draw { picture: &picture, transform: Affine::scale(96.0 / 72.0) }];
    let frame = Frame { clear: None, draws: &draws };
    b.bench_local(|| {
        let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("fits");
        renderer.render(&mut target, &text.resources, &frame);
    });
}

/// Draws `picture` into a `w × h` target each iteration, with `resources`.
fn draw_each(b: Bencher, picture: &Picture, resources: &Resources, (w, h): (u32, u32)) {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut renderer = Renderer::new();
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    let frame = Frame { clear: Some(Color::WHITE), draws: &draws };
    b.bench_local(|| {
        let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("fits");
        renderer.render(&mut target, resources, &frame);
    });
}

/// A 1600 × 1200 photo drawn at 400 × 300: bilinear from its mip level (the decode and mip chain are cached after
/// the first frame). Budget: well inside a frame (a page with pictures still meets the frame budget).
#[divan::bench]
fn image_downscaled(b: Bencher) {
    let mut resources = Resources::default();
    let pixels: Vec<u8> = (0..1600 * 1200u32).flat_map(|i| [(i % 1600 / 7) as u8, (i / 1600 / 5) as u8, (i % 251) as u8, 255]).collect();
    let image = resources.images.add_pixels(1600, 1200, pixels).expect("pixels");
    let mut rec = Recorder::new();
    rec.image(image, None, Rect::new(0.0, 0.0, 400.0, 300.0), &twip_graphics::paint::ImagePaint::default());
    draw_each(b, &rec.finish(), &resources, (400, 300));
}

/// A text selection's highlight: a 1000 × 400 rect multiplied over the page, as the editor draws it on a light page.
/// Budget: well inside a frame.
#[divan::bench]
fn selection_multiply(b: Bencher) {
    use twip_graphics::paint::BlendMode;
    let mut rec = Recorder::new();
    rec.fill_rect(Rect::new(0.0, 0.0, 1000.0, 400.0), Paint { blend: BlendMode::Multiply, ..Paint::from(Color::rgb(0xB4_D5FE)) });
    draw_each(b, &rec.finish(), &Resources::default(), (1000, 400));
}

/// A 400 × 300 rect filled with a linear gradient. Budget: well inside a frame.
#[divan::bench]
fn gradient_fill(b: Bencher) {
    use twip_graphics::paint::{Brush, Extend, Stop};
    let stops = [Stop { offset: 0.0, color: Color::rgb(0x44_72C4) }, Stop { offset: 1.0, color: Color::rgb(0xED_7D31) }];
    let brush =
        Brush::Linear { start: (0.0, 0.0).into(), end: (400.0, 300.0).into(), stops: &stops, extend: Extend::Clamp, transform: Affine::IDENTITY };
    let mut rec = Recorder::new();
    rec.fill_rect(Rect::new(0.0, 0.0, 400.0, 300.0), Paint::new(brush));
    draw_each(b, &rec.finish(), &Resources::default(), (400, 300));
}

/// A shape's drop shadow: a 200 × 100 rounded rect in a layer blurred by sigma 4, then the shape. Budget: well inside
/// a frame (shapes with effects are drawn into their page's picture once per change, then replayed).
#[divan::bench]
fn blurred_layer(b: Bencher) {
    use twip_graphics::scene::{Filter, Layer};
    let mut rec = Recorder::new();
    rec.push_layer(&Layer { opacity: 0.5, filters: &[Filter::Blur { sigma: 4.0 }], ..Layer::default() });
    rec.fill_rrect(Rect::new(54.0, 54.0, 254.0, 154.0), 8.0, INK);
    rec.pop();
    rec.fill_rrect(Rect::new(50.0, 50.0, 250.0, 150.0), 8.0, Color::rgb(0x44_72C4));
    draw_each(b, &rec.finish(), &Resources::default(), (300, 200));
}

/// Recording a whole page into a reused recorder. Budget: well under a millisecond; a page is recorded only when its
/// layout changes.
#[divan::bench]
fn record_page(b: Bencher) {
    let text = Text::new();
    let lines = lines(&text);
    let mut rec = Recorder::new();
    drop(page(&mut rec, &text, &lines));
    b.bench_local(|| page(&mut rec, &text, &lines));
}

/// Recording one line, what a keystroke re-records. Budget: a few microseconds (a keystroke's whole re-record and page
/// re-assembly is at most 0.2 ms).
#[divan::bench]
fn record_line(b: Bencher) {
    let text = Text::new();
    let glyphs = text.set(0, 11.0, 72.0, 540.0);
    let mut rec = Recorder::new();
    line(&mut rec, &text, &glyphs, 1, 100.0);
    drop(rec.finish());
    b.bench_local(|| {
        line(&mut rec, &text, &glyphs, 1, 100.0);
        rec.finish()
    });
}
