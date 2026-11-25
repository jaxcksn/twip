//! Preview pictures: items drawn by twip-graphics on the CPU, as PNG.

use twip_graphics::cpu::{Format, Target};
use twip_graphics::kurbo::Affine;
use twip_graphics::scene::Recorder;

use super::Renderer;
use super::appearance::Appearance;
use super::graphics::Graphics;
use crate::layout::fragment::Item;

/// Items drawn into a `width` x `height` px picture at `scale` device pixels per px, as PNG: on the page's colour in
/// `appearance`, scaled down to fit the height when `fit` (a gallery tile), centred vertically then. Metafile text
/// draws with `fonts`.
#[allow(clippy::too_many_arguments, reason = "the items, their pictures and fonts, and the tile's size and look")]
pub fn png(
    items: &[Item],
    fonts: &crate::font::Fonts,
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
    g.set_fonts(fonts);
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

/// The most device pixels on a side of an [`object_png`].
const MAX_OBJECT_SIDE: f64 = 4096.0;

/// Items drawn as they look on the page, cut to what they draw (effects included) at 96 dpi, as PNG with transparency:
/// a copied object's picture. No larger than [`MAX_OBJECT_SIDE`] on a side, scaled down evenly to fit; `None` when
/// they draw nothing or would take more than `max_pixels`. The PNG, its size in pixels and the size it draws at,
/// points.
pub fn object_png(
    items: &[crate::layout::fragment::Clipped],
    fonts: &crate::font::Fonts,
    images: &[Vec<u8>],
    max_pixels: u64,
) -> Option<(Vec<u8>, [u32; 2], [f64; 2])> {
    use twip_graphics::scene::Clip;
    let r = Renderer::default();
    let mut g = Graphics::default();
    g.set_fonts(fonts);
    let mut rec = Recorder::new();
    for (item, clip) in items {
        if let Some(c) = clip {
            rec.push_clip(Clip::Rect { rect: twip_graphics::kurbo::Rect::new(c[0], c[1], c[2], c[3]), aa: true });
        }
        g.record_item(&mut rec, &r, item, images);
        if clip.is_some() {
            rec.pop();
        }
    }
    let picture = rec.finish();
    let b = picture.bounds().filter(|b| b.width() > 0.0 && b.height() > 0.0 && b.is_finite())?;
    let k = (96.0 / 72.0f64).min(MAX_OBJECT_SIDE / b.width()).min(MAX_OBJECT_SIDE / b.height());
    let (w, h) = ((b.width() * k).ceil().max(1.0) as u32, (b.height() * k).ceil().max(1.0) as u32);
    if u64::from(w) * u64::from(h) > max_pixels {
        return None;
    }
    let mut px = vec![0u8; w as usize * h as usize * 4];
    let mut target = Target::new(&mut px, w, h, w as usize * 4, Format::Rgba8).ok()?;
    let at = Affine::scale(k) * Affine::translate((-b.x0, -b.y0));
    g.render_picture(&mut target, &picture, at, twip_graphics::paint::Color::TRANSPARENT);
    super::svg::unpremultiply(&mut px);
    Some((super::svg::png(w, h, &px), [w, h], [b.width(), b.height()]))
}
