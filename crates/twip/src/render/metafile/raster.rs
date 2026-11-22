//! Blits: a bitmap, the brush's pattern and what is under them combined by a ternary raster operation, through a
//! mask and keyed by a transparent colour.

use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, Shape};

use super::{COPY, Player, region};
use crate::render::svg::{Brush, Sink};

/// A blit's source: a bitmap file, the rect of its pixels (rows top down) the blit takes, and its opacity.
#[derive(Clone, Copy)]
pub(super) struct Source<'a> {
    pub bitmap: &'a Arc<Vec<u8>>,
    pub src: kurbo::Rect,
    pub opacity: f32,
}

/// `NOP`: the destination as it is.
pub(super) const NOP: u8 = 0xAA;

/// The binary raster operation table (bit `2·a + b`) of `g`.
fn table(g: impl Fn(u8, u8) -> u8) -> u8 {
    (0..4).fold(0, |t, i| t | (g(i >> 1, i & 1) & 1) << i)
}

impl<S: Sink> Player<'_, S> {
    /// A blit by the ternary raster operation `code` (bit `4·p + 2·s + d`: the brush's pattern, the source, the
    /// destination) onto the parallelogram `to` maps the unit square onto. Each operation is drawn exactly: one of two
    /// inputs as a binary raster operation; of the pattern and the source, those two combined in a layer; of all
    /// three, `d' = (d & (f₁ ^ f₀)) ^ f₀` (f₁ and f₀ the pattern and source's results where d is 1 and 0) in two such
    /// layers.
    pub(super) fn blit(&mut self, code: u8, to: Affine, source: Option<Source<'_>>) {
        if self.in_path || to.determinant() == 0.0 || !to.is_finite() {
            return;
        }
        let f = |p: u8, s: u8, d: u8| code >> (p * 4 + s * 2 + d) & 1;
        let (uses_p, uses_s, uses_d) = (code >> 4 != code & 15, code >> 2 & 0x33 != code & 0x33, code >> 1 & 0x55 != code & 0x55);
        let mut area = kurbo::Rect::new(0.0, 0.0, 1.0, 1.0).to_path(0.1);
        area.apply_affine(to);
        let pattern = if uses_p {
            let Some(b) = self.brush(&self.dc.brush) else { return };
            b
        } else {
            Brush::Solid([0.0, 0.0, 0.0, 1.0])
        };
        let source = if uses_s {
            let Some(s) = source else { return };
            Some(s)
        } else {
            None
        };
        let smooth = self.dc.smooth;
        match (uses_p, source) {
            (_, None) => {
                let op = table(|p, d| f(p, 0, d));
                self.under_clip(op, |me| me.sink.fill(&area, false, &pattern));
            }
            (false, Some(s)) => {
                let op = table(|s, d| f(0, s, d));
                self.under_clip(op, |me| me.sink.image(s.bitmap, s.src, to, s.opacity, smooth));
            }
            (true, Some(s)) => {
                let pass = |me: &mut Self, composite: u8, g: u8| {
                    me.under_clip(composite, |me| {
                        me.sink.begin_layer(1.0);
                        me.sink.set_raster(COPY);
                        me.sink.fill(&area, false, &pattern);
                        me.sink.set_raster(g);
                        me.sink.image(s.bitmap, s.src, to, s.opacity, smooth);
                        me.sink.end_layer();
                    });
                };
                // In the layer the pattern is the destination and the source the source.
                let f0 = table(|s, p| f(p, s, 0));
                if !uses_d {
                    pass(self, COPY, f0);
                } else {
                    let x = table(|s, p| f(p, s, 1) ^ f(p, s, 0));
                    if x != 15 {
                        pass(self, 8, x);
                    }
                    if f0 != 0 {
                        pass(self, 6, f0);
                    }
                }
            }
        }
    }

    /// A blit through a mask: the foreground operation where `mask` (rects in the unit square `to` maps) covers, the
    /// background one elsewhere.
    pub(super) fn masked_blit(&mut self, fore: u8, back: u8, to: Affine, source: Option<Source<'_>>, mask: Option<&[kurbo::Rect]>) {
        let Some(mask) = mask else { return self.blit(fore, to, source) };
        if self.in_path {
            return;
        }
        let inside = self.rects_path(mask.iter().copied(), to);
        let mut outside = self.everything();
        outside.extend(inside.iter());
        for (code, clip, even_odd) in [(fore, inside, false), (back, outside, true)] {
            if code == NOP {
                continue;
            }
            self.sync_clip();
            self.sink.save();
            self.sink.clip_path(&clip, even_odd);
            self.blit(code, to, source);
            self.sink.restore();
        }
    }

    /// `TransparentBlt`: the source where it isn't `key` (a `COLORREF`).
    pub(super) fn keyed_blit(&mut self, to: Affine, s: Source<'_>, key: u32) {
        let Ok((w, h, px)) = twip_graphics::image::pixels(s.bitmap) else { return };
        let [r, g, b, _] = self.rgb(key).map(|v| (v * 255.0).round() as u8);
        let (x0, y0) = (s.src.x0.max(0.0) as u32, s.src.y0.max(0.0) as u32);
        let (x1, y1) = ((s.src.x1.min(f64::from(w))) as u32, (s.src.y1.min(f64::from(h))) as u32);
        if x1 <= x0 || y1 <= y0 || s.src.width() <= 0.0 || s.src.height() <= 0.0 {
            return;
        }
        let shown = |x: u32, y: u32| {
            let at = ((y0 + y) as usize * w as usize + (x0 + x) as usize) * 4;
            px.get(at..at + 4).is_some_and(|p| p[..3] != [r, g, b] || p[3] == 0)
        };
        let Some(rects) = region::runs(x1 - x0, y1 - y0, shown, |n| self.spend(n)) else { return };
        // From the source's pixels into the unit square.
        let unit = Affine::scale_non_uniform(1.0 / s.src.width(), 1.0 / s.src.height())
            * Affine::translate((f64::from(x0) - s.src.x0, f64::from(y0) - s.src.y0));
        let mask: Vec<kurbo::Rect> = rects.iter().map(|r| unit.transform_rect_bbox(*r)).collect();
        self.masked_blit(0xCC, NOP, to, Some(s), Some(&mask));
    }

    /// The unit square onto `dst` (a logical origin and signed size), in the picture's space.
    pub(super) fn onto(&self, dst: (kurbo::Point, kurbo::Vec2)) -> Affine {
        self.m() * Affine::translate(dst.0.to_vec2()) * Affine::scale_non_uniform(dst.1.x, dst.1.y)
    }
}

/// A monochrome mask's set bits, `w` × `h` of them from `(x, y)` (rows top down), as rects in the unit square; `None`
/// when `spend` stops it.
pub(super) fn mask_rects(
    bits: &dyn Fn(u32, u32) -> bool,
    (x, y, w, h): (u32, u32, u32, u32),
    spend: &mut dyn FnMut(usize) -> bool,
) -> Option<Vec<kurbo::Rect>> {
    if w == 0 || h == 0 {
        return Some(Vec::new());
    }
    let rects = region::runs(w, h, |i, j| bits(x + i, y + j), spend)?;
    let unit = Affine::scale_non_uniform(1.0 / f64::from(w), 1.0 / f64::from(h));
    Some(rects.into_iter().map(|r| unit.transform_rect_bbox(r)).collect())
}
