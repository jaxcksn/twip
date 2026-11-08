//! Preview pictures

use skia_safe::EncodedImageFormat;

use super::Renderer;
use super::appearance::Appearance;
use crate::layout::fragment::Item;

/// Items drawn into a `width` x `height` px picture at `scale` device pixels per px, as PNG: on the page's colour in
/// `appearance`, scaled down to fit the height when `fit` (a gallery tile), centred vertically then.
pub fn png(
    items: &[Item],
    content_height: f64,
    images: &[Vec<u8>],
    (width, height, scale): (f64, f64, f64),
    fit: bool,
    appearance: Appearance,
) -> Option<(Vec<u8>, u32, u32)> {
    let (w, h) = ((width * scale).round().max(1.0) as i32, (height * scale).round().max(1.0) as i32);
    let mut surface = skia_safe::surfaces::raster_n32_premul((w, h))?;
    let mut r = Renderer { appearance, ..Renderer::default() };
    let canvas = surface.canvas();
    canvas.clear(r.ink(0xFF_FFFF));
    let px_per_pt = scale * 96.0 / 72.0;
    canvas.scale((px_per_pt as f32, px_per_pt as f32));
    let height_pt = height * 72.0 / 96.0;
    if fit && content_height > 0.0 {
        let k = (height_pt / content_height).min(1.0);
        canvas.translate((0.0, ((height_pt - content_height * k) / 2.0) as f32));
        canvas.scale((k as f32, k as f32));
    }
    for item in items {
        r.draw_item(canvas, item, images);
    }
    let png = surface.image_snapshot().encode(None, EncodedImageFormat::PNG, None)?;
    Some((png.as_bytes().to_vec(), w as u32, h as u32))
}
