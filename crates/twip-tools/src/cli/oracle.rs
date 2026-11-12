//! Renderer oracle: every page of a document drawn by the Skia renderer and by twip-graphics, side by side, with a diff
//! image and a per-page report, to tune twip-graphics against what the editor draws today.
//!
//! Both sides draw each page's content as the screen does (`render::graphics` on the twip-graphics side): its glyph
//! runs, rects, gridlines, images, shapes and turned groups, on white, with no editor overlays. Pixels under a picture
//! are counted apart: twip-graphics samples pictures bilinearly from a mip chain, Skia's default is nearest, so they
//! differ on purpose and are reviewed by eye.

use std::path::Path;

use skia_safe::{AlphaType, Color, ColorType, Data, EncodedImageFormat, ImageInfo, images, surfaces};
use twip::internal::layout::fragment::Item;
use twip_graphics::cpu::{Format, Target};
use twip_graphics::kurbo::Affine;

/// A page's comparison: pixels where either side drew something (`ink`), and how many of those differ by more than
/// 16 and more than 64 in some channel.
pub struct Page {
    pub ink: usize,
    pub over16: usize,
    pub over64: usize,
    pub max: u8,
    /// Pixels under pictures, and how many of them differ by more than 64.
    pub image: usize,
    pub image_over64: usize,
}

/// Writes `out/page-NNN.skia.png`, `.graphics.png` and `.diff.png` for each page of `docx` at `scale` pixels per
/// point, and returns each page's comparison.
pub fn run(docx: &Path, out: &Path, scale: f32) -> Result<Vec<Page>, String> {
    let bytes = std::fs::read(docx).map_err(|e| format!("{}: {e}", docx.display()))?;
    // A fixed clock (2026-10-30T00:00:00Z), as `pdf` and `dump` use: DATE/TIME fields print the same on every run.
    let mut doc = twip::internal::docx::read_at(&bytes, 1_793_318_400).map_err(|e| format!("{}: {e:?}", docx.display()))?;
    doc.review.view = twip::internal::docx::review::MarkupView::NoMarkup;
    let fonts = twip::internal::font::Fonts::system();
    let layout = twip::internal::layout::layout(&doc, &fonts);
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;

    let mut skia = twip::internal::render::Renderer::default();
    let mut graphics = twip::internal::render::graphics::Graphics::default();
    let mut pages = Vec::new();
    for (i, page) in layout.pages().iter().enumerate() {
        let items = twip::internal::layout::traverse::paint_items(page);
        let (w, h) = ((page.w as f32 * scale).ceil() as i32, (page.h as f32 * scale).ceil() as i32);
        let info = ImageInfo::new((w, h), ColorType::RGBA8888, AlphaType::Premul, None);
        let stride = w as usize * 4;

        let mut surface = surfaces::raster(&info, None, None).ok_or("no raster surface for the page")?;
        let canvas = surface.canvas();
        canvas.clear(Color::WHITE);
        canvas.scale((scale, scale));
        for item in &items {
            skia.draw_item(canvas, item, &doc.images);
        }
        let mut theirs = vec![0u8; stride * h as usize];
        if !surface.read_pixels(&info, &mut theirs, stride, (0, 0)) {
            return Err("reading Skia's pixels failed".into());
        }

        let mut under_images = vec![false; w as usize * h as usize];
        for item in &items {
            let picture = match item {
                Item::Image { rect, .. } => Some(*rect),
                Item::Shape(s) if s.picture || matches!(s.fill, twip::internal::drawing::paint::Paint::Picture { .. }) => Some(s.rect),
                _ => None,
            };
            if let Some([x0, y0, x1, y1]) = picture {
                let (px0, py0) = ((x0 as f32 * scale).floor().max(0.0) as usize, (y0 as f32 * scale).floor().max(0.0) as usize);
                let (px1, py1) = (((x1 as f32 * scale).ceil() as usize).min(w as usize), ((y1 as f32 * scale).ceil() as usize).min(h as usize));
                for y in py0..py1 {
                    under_images[y * w as usize + px0..y * w as usize + px1.max(px0)].fill(true);
                }
            }
        }
        let picture = graphics.page_picture(&skia, page, &doc.images);
        let mut ours = vec![0u8; stride * h as usize];
        let mut target = Target::new(&mut ours, w as u32, h as u32, stride, Format::Rgba8).map_err(|e| e.to_string())?;
        graphics.render_picture(&mut target, &picture, Affine::scale(scale as f64), twip_graphics::paint::Color::WHITE);

        let (report, diff) = compare(&theirs, &ours, &under_images);
        let name = |kind: &str| out.join(format!("page-{:03}.{kind}.png", i + 1));
        write_png(&name("skia"), &info, &theirs)?;
        write_png(&name("graphics"), &info, &ours)?;
        write_png(&name("diff"), &info, &diff)?;
        pages.push(report);
    }
    Ok(pages)
}
/// The pages' comparison and a diff image: differing pixels red, brighter the larger the difference; ink both sides
/// share faded grey.
fn compare(theirs: &[u8], ours: &[u8], under_images: &[bool]) -> (Page, Vec<u8>) {
    let mut page = Page { ink: 0, over16: 0, over64: 0, max: 0, image: 0, image_over64: 0 };
    let mut diff = Vec::with_capacity(theirs.len());
    for ((a, b), &image) in theirs.as_chunks::<4>().0.iter().zip(ours.as_chunks::<4>().0).zip(under_images) {
        let white = |p: &[u8; 4]| p[..3].iter().all(|&c| c == 255);
        let delta = a.iter().zip(b).map(|(x, y)| x.abs_diff(*y)).max().unwrap_or(0);
        if image {
            page.image += 1;
            page.image_over64 += usize::from(delta > 64);
        } else if !white(a) || !white(b) {
            page.ink += 1;
            page.over16 += usize::from(delta > 16);
            page.over64 += usize::from(delta > 64);
        }
        if !image {
            page.max = page.max.max(delta);
        }
        if delta > 16 {
            diff.extend_from_slice(&[255, 255 - delta, 255 - delta, 255]);
        } else {
            diff.extend(a[..3].iter().map(|c| 191 + c / 4).chain([255]));
        }
    }
    (page, diff)
}

fn write_png(path: &Path, info: &ImageInfo, pixels: &[u8]) -> Result<(), String> {
    let image = images::raster_from_data(info, Data::new_copy(pixels), info.min_row_bytes()).ok_or("an image from the pixels")?;
    let png = image.encode(None, EncodedImageFormat::PNG, None).ok_or("PNG encoding failed")?;
    std::fs::write(path, png.as_bytes()).map_err(|e| format!("{}: {e}", path.display()))
}
