//! Golden images: a scene's pixels compared byte for byte with a committed PNG in `tests/golden/`.
//!
//! The PNGs hold premultiplied RGBA8 bytes exactly as the CPU backend writes them. Scenes draw over an opaque
//! background, so the files also view correctly. `TWIP_GRAPHICS_BLESS=1 cargo test -p twip-graphics --test golden`
//! writes the golden files from the current output; review them before committing. A mismatch writes
//! `tests/out/<name>.actual.png` and `tests/out/<name>.diff.png` (differing pixels red, the rest the expected image,
//! faded).

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

fn dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join(name)
}

fn write_png(path: &Path, (w, h): (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())
}

fn read_png(path: &Path) -> Result<((u32, u32), Vec<u8>), String> {
    let file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut reader = png::Decoder::new(BufReader::new(file)).read_info().map_err(|e| e.to_string())?;
    let mut data = vec![0; reader.output_buffer_size().ok_or("image too large")?];
    let info = reader.next_frame(&mut data).map_err(|e| e.to_string())?;
    if (info.color_type, info.bit_depth) != (png::ColorType::Rgba, png::BitDepth::Eight) {
        return Err(format!("{}: not 8-bit RGBA", path.display()));
    }
    data.truncate(info.buffer_size());
    Ok(((info.width, info.height), data))
}

/// Compares `rgba` (premultiplied RGBA8, `size` in pixels) with the golden image `name`, writing the actual and diff
/// images when they differ. Blesses instead when `TWIP_GRAPHICS_BLESS` is set.
fn compare(name: &str, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    assert_eq!(rgba.len(), size.0 as usize * size.1 as usize * 4, "{name}: buffer doesn't match its size");
    let golden = dir("golden").join(format!("{name}.png"));
    if std::env::var_os("TWIP_GRAPHICS_BLESS").is_some() {
        std::fs::create_dir_all(dir("golden")).map_err(|e| e.to_string())?;
        return write_png(&golden, size, rgba);
    }
    let (expected_size, expected) =
        read_png(&golden).map_err(|e| format!("{name}: no readable golden image ({e}); bless it with TWIP_GRAPHICS_BLESS=1"))?;
    if expected_size == size && expected == rgba {
        return Ok(());
    }
    let out = dir("out");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    write_png(&out.join(format!("{name}.actual.png")), size, rgba)?;
    if expected_size != size {
        return Err(format!("{name}: size {size:?}, golden {expected_size:?}"));
    }
    let (mut differing, mut max_delta) = (0, 0);
    let mut diff = Vec::with_capacity(rgba.len());
    for (a, e) in rgba.as_chunks::<4>().0.iter().zip(expected.as_chunks::<4>().0) {
        let delta = a.iter().zip(e).map(|(a, e)| a.abs_diff(*e)).max().unwrap_or(0);
        if delta > 0 {
            differing += 1;
            max_delta = max_delta.max(delta);
            diff.extend_from_slice(&[255, 0, 0, 255]);
        } else {
            diff.extend(e[..3].iter().map(|c| 191 + c / 4).chain([255]));
        }
    }
    write_png(&out.join(format!("{name}.diff.png")), size, &diff)?;
    Err(format!("{name}: {differing} pixels differ (largest channel difference {max_delta}); see tests/out/{name}.*.png"))
}

fn check(name: &str, size: (u32, u32), rgba: &[u8]) {
    if let Err(why) = compare(name, size, rgba) {
        panic!("{why}");
    }
}

// The scenes: each drawn by the CPU backend on white, at the detected SIMD level, the target's baseline level and the
// scalar fallback, and each drawing compared with the same golden image.

use fearless_simd::Level;
use twip_graphics::Resources;
use twip_graphics::cpu::{Draw, Format, Frame, Renderer, Target};
use twip_graphics::kurbo::{self, Affine, Shape, Stroke};
use twip_graphics::paint::Color;
use twip_graphics::scene::{Picture, Recorder};

mod common;
use common::scenes::*;

fn levels() -> [Level; 3] {
    [Level::new(), Level::baseline(), Level::fallback()]
}

fn draw(picture: &Picture, (w, h): (u32, u32), level: Level, format: Format) -> Vec<u8> {
    draw_with(picture, &Resources::default(), (w, h), level, format)
}

fn draw_with(picture: &Picture, resources: &Resources, (w, h): (u32, u32), level: Level, format: Format) -> Vec<u8> {
    let mut px = vec![0u8; (w * h * 4) as usize];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, format).expect("a buffer that fits");
    let draws = [Draw { picture, transform: Affine::IDENTITY }];
    Renderer::with_level(level).render(&mut target, resources, &Frame { clear: Some(Color::WHITE), draws: &draws });
    px
}

/// Draws `picture` at every SIMD level and compares each drawing with the golden image `name`.
fn golden(name: &str, size: (u32, u32), picture: &Picture) {
    golden_with(name, size, picture, &Resources::default());
}

fn golden_with(name: &str, size: (u32, u32), picture: &Picture, resources: &Resources) {
    for level in levels() {
        if let Err(why) = compare(name, size, &draw_with(picture, resources, size, level, Format::Rgba8)) {
            panic!("{why} (level {level:?})");
        }
    }
}

#[test]
fn image_scene() {
    let mut resources = Resources::default();
    let picture = images(&mut resources);
    golden_with("images", (128, 144), &picture, &resources);
}

#[test]
fn the_jpeg_s_orientation_turns_it_upright() {
    let info = twip_graphics::image::probe(include_bytes!("images/rotated-6.jpg")).expect("a JPEG");
    assert_eq!((info.orientation, info.width, info.height), (6, 32, 48), "stored 48 × 32, shown turned a quarter");
}

#[test]
fn gradient_scene() {
    golden("gradients", (128, 144), &gradients());
}

#[test]
fn layer_scene() {
    golden("layers", (128, 144), &layers());
}

#[test]
fn blend_mode_scene() {
    golden("blend-modes", (254, 146), &blend_modes());
}

#[test]
fn colour_glyph_scene() {
    let mut resources = Resources::default();
    let picture = colour_glyphs(&mut resources);
    golden_with("colour-glyphs", (256, 240), &picture, &resources);
}

#[test]
fn text_scene() {
    let mut resources = Resources::default();
    let picture = text(&mut resources);
    golden_with("text", (256, 200), &picture, &resources);
}

#[test]
fn rect_scene() {
    golden("rects", (128, 96), &rects());
}

#[test]
fn rrect_scene() {
    golden("rrects", (128, 96), &rrects());
}

#[test]
fn path_scene() {
    golden("paths", (128, 112), &paths());
}

#[test]
fn stroke_scene() {
    golden("strokes", (128, 128), &strokes());
}

#[test]
fn clip_scene() {
    golden("clips", (128, 120), &clips());
}

#[test]
fn transform_scene() {
    golden("transforms", (128, 128), &transforms());
}

#[test]
fn blend_scene() {
    golden("blends", (128, 128), &blends());
}

#[test]
fn bgra_targets_hold_the_same_pixels_swapped() {
    let picture = rects();
    let rgba = draw(&picture, (128, 96), Level::new(), Format::Rgba8);
    let bgra = draw(&picture, (128, 96), Level::new(), Format::Bgra8);
    let swapped: Vec<u8> = bgra.as_chunks::<4>().0.iter().flat_map(|&[b, g, r, a]| [r, g, b, a]).collect();
    assert_eq!(swapped, rgba);
}

#[test]
fn a_nested_picture_draws_as_its_commands_inline() {
    let line = |rec: &mut Recorder| {
        rec.fill_rect(rect(0.3, 0.0, 40.7, 1.0), INK);
        rec.fill_path(kurbo::Circle::new((50.0, 4.0), 4.5).to_path(0.01).elements(), RED);
        rec.stroke_path(polyline(&[(60.0, 0.0), (70.0, 8.0), (80.0, 0.0)]).elements(), &Stroke::new(1.5), BLUE);
    };
    let place = [Affine::translate((4.25, 10.5)), Affine::translate((4.25, 30.5)) * Affine::rotate(0.05), Affine::scale(1.3)];
    let mut rec = Recorder::new();
    line(&mut rec);
    let shared = rec.finish();
    for t in place {
        rec.picture(&shared, t);
    }
    let nested = rec.finish();
    for t in place {
        rec.push_transform(t);
        line(&mut rec);
        rec.pop();
    }
    let inline = rec.finish();
    assert_eq!(draw(&nested, (128, 64), Level::new(), Format::Rgba8), draw(&inline, (128, 64), Level::new(), Format::Rgba8));
}

#[test]
fn frames_without_a_clear_draw_over_the_target() {
    let mut px = vec![0u8; 4 * 4 * 4];
    let mut target = Target::new(&mut px, 4, 4, 16, Format::Rgba8).expect("fits");
    let mut rec = Recorder::new();
    rec.fill_rect(rect(1.0, 1.0, 3.0, 3.0), Color::WHITE);
    let picture = rec.finish();
    Renderer::new().render(
        &mut target,
        &Resources::default(),
        &Frame { clear: None, draws: &[Draw { picture: &picture, transform: Affine::IDENTITY }] },
    );
    assert_eq!(&px[0..4], &[0, 0, 0, 0]);
    assert_eq!(&px[(4 + 1) * 4..(4 + 2) * 4], &[255; 4]);
}

/// An opaque 8 × 8 white image with a 2 × 2 blue square at (3, 3). Filled by hand: the harness's own check, apart
/// from the renderer.
fn smoke() -> Vec<u8> {
    let mut px = [255u8; 8 * 8 * 4];
    for (i, p) in px.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let (x, y) = (i % 8, i / 8);
        if (3..5).contains(&x) && (3..5).contains(&y) {
            *p = [0, 0, 255, 255];
        }
    }
    px.to_vec()
}

#[test]
fn harness_matches_its_golden_image() {
    check("harness-smoke", (8, 8), &smoke());
}

#[test]
fn harness_reports_one_changed_pixel() {
    if std::env::var_os("TWIP_GRAPHICS_BLESS").is_some() {
        return;
    }
    let mut px = smoke();
    px[0] = 254;
    let why = compare("harness-smoke", (8, 8), &px).expect_err("a changed pixel is a mismatch");
    assert!(why.contains("1 pixels differ (largest channel difference 1)"), "{why}");
}
