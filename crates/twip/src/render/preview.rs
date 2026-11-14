//! Preview pictures: items drawn by twip-graphics on the CPU, as PNG.

use twip_graphics::cpu::{Format, Target};
use twip_graphics::kurbo::Affine;
use twip_graphics::scene::Recorder;

use super::Renderer;
use super::appearance::Appearance;
use super::graphics::Graphics;
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
    let (w, h) = ((width * scale).round().max(1.0) as u32, (height * scale).round().max(1.0) as u32);
    let r = Renderer { appearance, ..Renderer::default() };
    let px_per_pt = scale * 96.0 / 72.0;
    let mut t = Affine::scale(px_per_pt);
    let height_pt = height * 72.0 / 96.0;
    if fit && content_height > 0.0 {
        let k = (height_pt / content_height).min(1.0);
        t = t * Affine::translate((0.0, (height_pt - content_height * k) / 2.0)) * Affine::scale(k);
    }
    let mut g = Graphics::default();
    let mut rec = Recorder::new();
    rec.push_transform(t);
    for item in items {
        g.record_item(&mut rec, &r, item, images);
    }
    rec.pop();
    let picture = rec.finish();
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).ok()?;
    // Over the opaque page colour, so the pixels are opaque: premultiplied is straight.
    g.render_picture(&mut target, &picture, Affine::IDENTITY, r.ink_for(false, 0xFF_FFFF));
    Some((super::svg::png(w, h, &px), w, h))
}
