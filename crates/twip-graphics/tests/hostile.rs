//! Drawing a hostile document finishes, without panicking, overflowing the stack or taking unbounded memory: each
//! case here once did one of those (release builds abort on a panic).

use std::sync::Arc;
use std::time::{Duration, Instant};

use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};
use twip_graphics::font::FontData;
use twip_graphics::kurbo::{Affine, Circle, Line, Rect, Shape, Stroke};
use twip_graphics::paint::Color;
use twip_graphics::scene::{Clip, GlyphRun, Layer, Picture, Recorder, Synthesis};

fn draw(resources: &Resources, picture: &Picture, (w, h): (u32, u32)) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).expect("fits");
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    Renderer::new().render(&mut target, resources, &Frame { clear: Some(Color::WHITE), draws: &draws });
    px
}

#[test]
fn a_colour_glyph_left_of_the_clip() {
    use skrifa::MetadataProvider;
    use skrifa::raw::TableProvider;
    let data = font_test_data::COLRV0V1;
    let f = skrifa::FontRef::new(data).expect("reads");
    let glyph =
        (0..f.maxp().expect("maxp").num_glyphs() as u32).find(|&g| f.color_glyphs().get(skrifa::GlyphId::new(g)).is_some()).expect("a COLR glyph");
    let mut resources = Resources::default();
    let font = resources.fonts.add(Arc::new(data.to_vec()) as FontData, 0, &[]).expect("a face");
    let mut rec = Recorder::new();
    // The line is on the target; its first emoji is scrolled far off its left edge.
    let run = GlyphRun { font, size: 24.0, baseline: 50.0, glyphs: &[(glyph, -5000.0), (glyph, 50.0)], synthesis: Synthesis::default() };
    rec.glyphs(&resources.fonts, &run, Color::BLACK);
    let px = draw(&resources, &rec.finish(), (200, 100));
    assert!(px.as_chunks::<4>().0.iter().any(|p| p != &[255; 4]), "the visible glyph draws");
}

#[test]
fn a_path_clip_inside_a_rect_clip_off_the_target() {
    let mut rec = Recorder::new();
    // Something on the target, so the picture isn't culled whole.
    rec.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::BLACK);
    let circle = Circle::new((100.0, 50.0), 200.0).to_path(0.1);
    rec.push_clip(Clip::Path(circle.elements()));
    rec.push_clip(Clip::Rect { rect: Rect::new(500.0, 0.0, 600.0, 100.0), aa: true });
    rec.push_clip(Clip::Path(circle.elements()));
    rec.fill_rect(Rect::new(0.0, 0.0, 1000.0, 100.0), Color::BLACK);
    draw(&Resources::default(), &rec.finish(), (200, 100));
}

#[test]
fn dash_patterns_that_never_end_or_never_stop() {
    let diagonal = Line::new((0.0, 0.0), (1.0e5, 1.0e5));
    let mut cases = vec![
        // Sums to infinity.
        (Stroke::new(1.0).with_dashes(-1.0, [1.0e308, 1.0e308]), Line::new((0.0, 0.0), (100.0, 0.0))),
        (Stroke::new(1.0).with_dashes(-1.0, [1.0e308, 1.0e308]), diagonal),
        // An offset kurbo would walk to one entry at a time.
        (Stroke::new(1.0).with_dashes(1.0e300, [1.0, 1.0]), diagonal),
        // A diagonal of ten million dashes.
        (Stroke::new(1.0).with_dashes(0.0, [0.01, 0.01]), diagonal),
    ];
    // An axis-aligned line of a hundred million zero-length dashes.
    let mut zeros = vec![0.0; 1000];
    zeros.push(1.0);
    cases.push((Stroke::new(1.0).with_caps(twip_graphics::kurbo::Cap::Butt).with_dashes(0.0, zeros), Line::new((0.0, 0.0), (2.0e5, 0.0))));
    for (stroke, line) in cases {
        let start = Instant::now();
        let mut rec = Recorder::new();
        rec.stroke_path(line.path_elements(0.1).collect::<Vec<_>>().as_slice(), &stroke, Color::BLACK);
        let picture = rec.finish();
        assert!(start.elapsed() < Duration::from_secs(5), "{:?}", &stroke.dash_pattern[..2]);
        assert!(picture.bytes() < 64 << 20, "{} bytes for {:?}", picture.bytes(), &stroke.dash_pattern[..2]);
    }
}

#[test]
fn nesting_too_deep_to_walk() {
    let mut rec = Recorder::new();
    for _ in 0..100_000 {
        rec.push_layer(&Layer { opacity: 0.9, ..Layer::default() });
    }
    rec.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::BLACK);
    let layers = rec.finish();
    let mut picture = layers.clone();
    for _ in 0..100_000 {
        rec.picture(&picture, Affine::IDENTITY);
        picture = rec.finish();
    }
    for p in [&layers, &picture] {
        draw(&Resources::default(), p, (4, 4));
    }
}

/// The PDF writer: loops linear in what the document has, recursion bounded.
#[cfg(feature = "pdf")]
mod pdf {
    use super::*;
    use twip_graphics::paint::{BlendMode, Paint};
    use twip_graphics::pdf::{ArtifactKind, Element, Heading, Info, Kind, Mark, Page, write};

    /// `pages` written well inside the time a quadratic loop over them would take.
    fn writes(resources: &Resources, pages: &[Page], structure: &[Element], outline: &[Heading]) {
        let start = Instant::now();
        write(resources, pages, structure, outline, &Info::default()).expect("writes");
        assert!(start.elapsed() < Duration::from_secs(20), "{:?}", start.elapsed());
    }

    fn square() -> Picture {
        let mut rec = Recorder::new();
        rec.fill_rect(Rect::new(0.0, 0.0, 4.0, 4.0), Color::BLACK);
        rec.finish()
    }

    #[test]
    fn fifty_thousand_headings() {
        let (n, square) = (50_000, square());
        let page = Page { width: 10.0, height: 10.0, parts: (0..n).map(|c| (Mark::Content(c), square.clone())).collect() };
        let headings: Vec<Heading> = (0..n).map(|c| Heading { level: 1 + (c % 2) as u8, content: c, title: "h".into() }).collect();
        writes(&Resources::default(), &[page], &[], &headings);
    }

    #[test]
    fn one_cluster_of_200_000_glyphs() {
        let mut resources = Resources::default();
        let data = Arc::new(include_bytes!("fonts/jetbrains-mono-500.ttf").to_vec()) as FontData;
        let font = resources.fonts.add(data, 0, &[]).expect("a face");
        let glyphs: Vec<(u32, f64)> = (0..200_000).map(|i| (36, f64::from(i) * 0.002)).collect();
        let run = GlyphRun { font, size: 10.0, baseline: 10.0, glyphs: &glyphs, synthesis: Synthesis::default() };
        let mut rec = Recorder::new();
        rec.glyphs_text(&resources.fonts, &run, "a", &vec![0; glyphs.len()], Color::BLACK);
        writes(&resources, &[Page { width: 600.0, height: 20.0, parts: vec![(Mark::Artifact(ArtifactKind::Other), rec.finish())] }], &[], &[]);
    }

    #[test]
    fn a_structure_tree_too_deep_to_walk() {
        let mut e = Element { kind: Kind::Paragraph, content: Some(0), kids: Vec::new() };
        for _ in 0..100_000 {
            e = Element { kind: Kind::List, content: None, kids: vec![e] };
        }
        writes(
            &Resources::default(),
            &[Page { width: 10.0, height: 10.0, parts: vec![(Mark::Content(0), square())] }],
            std::slice::from_ref(&e),
            &[],
        );
        // Dropping it recurses as deep (the caller's tree, not the writer's walk).
        std::mem::forget(e);
    }

    #[test]
    fn a_group_of_more_masks_than_nest() {
        let mut rec = Recorder::new();
        rec.fill_rect(Rect::new(0.0, 0.0, 10.0, 10.0), Color::BLACK);
        let clear = Paint { blend: BlendMode::Clear, ..Paint::from(Color::BLACK) };
        for i in 0..50_000 {
            let x = f64::from(i % 10);
            rec.fill_rect(Rect::new(x, 0.0, x + 0.5, 1.0), clear);
        }
        writes(
            &Resources::default(),
            &[Page { width: 10.0, height: 10.0, parts: vec![(Mark::Artifact(ArtifactKind::Other), rec.finish())] }],
            &[],
            &[],
        );
    }
}
