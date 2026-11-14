//! Per-pixel sources and layer filters: gradients (linear, two-point conical, sweep) through a 256-entry ramp, images
//! and patterns (nearest or bilinear, from a mip level), colour filters, and blur and erode.
//!
//! A shader maps each device pixel's centre back into its own space and computes that pixel's premultiplied colour,
//! in the target's channel order. Gradients interpolate their stops unpremultiplied (as Skia's do by default) into a
//! 256-entry ramp, premultiplied, which the GPU backend shares, so both read the same colours. Image sampling and the
//! filters are integer arithmetic; gradient geometry is `f32` in a fixed order. Either way every SIMD level writes the
//! same bytes.
//!
//! - Two-point conical: a point takes the largest `t` whose circle `c0 + t·(c1 − c0)`, radius `r0 + t·(r1 − r0)`,
//!   passes through it with a radius of 0 or more (the canvas and COLRv1 definition, Skia's
//!   `SkTwoPointConicalGradient`).
//! - Sweep: the angle from +x, clockwise with y down, in degrees, mapped from `start..end` to `0..1`.
//! - Blur: as Skia's CPU mask blur (`SkMaskBlurFilter`) does it. From a standard deviation of 2 up, three box blurs a
//!   side, sized as SVG's `feGaussianBlur` sizes them; below 2, a sampled Gaussian kernel; below 0.53, nothing.
//! - Erode: each channel's minimum over a square window.

use fearless_simd::{Level, Simd, dispatch};
use fearless_simd_macros::simd;

use crate::geom::{Point, Transform};
use crate::image::Pixmap;
use crate::paint::{ColorFilter, Extend, Sampling, Stop};

/// A gradient's colours at 256 evenly spaced positions, premultiplied, in the target's channel order.
pub(crate) type Ramp = [[u8; 4]; 256];

/// The ramp for `stops` (sorted by offset), each entry interpolated unpremultiplied and then premultiplied.
pub(crate) fn ramp(stops: &[Stop], bgra: bool) -> Ramp {
    let mut out = [[0u8; 4]; 256];
    let Some(first) = stops.first() else { return out };
    for (i, entry) in out.iter_mut().enumerate() {
        let t = i as f32 / 255.0;
        // The last stop at or before t, and the first after it.
        let after = stops.iter().position(|s| s.offset > t);
        let c = match after {
            None => stops[stops.len() - 1].color,
            Some(0) => first.color,
            Some(k) => {
                let (a, b) = (stops[k - 1], stops[k]);
                let span = b.offset - a.offset;
                let f = if span > 0.0 { (t - a.offset) / span } else { 1.0 };
                let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f + 0.5) as u8;
                crate::paint::Color::rgba(mix(a.color.r, b.color.r), mix(a.color.g, b.color.g), mix(a.color.b, b.color.b), mix(a.color.a, b.color.a))
            }
        };
        let p = c.premultiply();
        *entry = if bgra { [p.b, p.g, p.r, p.a] } else { [p.r, p.g, p.b, p.a] };
    }
    out
}

/// `t` folded into `0..=1` by the extend mode.
fn extend(t: f32, e: Extend) -> f32 {
    match e {
        Extend::Clamp => t.clamp(0.0, 1.0),
        Extend::Repeat => t - t.floor(),
        Extend::Mirror => {
            let f = t.rem_euclid(2.0);
            if f > 1.0 { 2.0 - f } else { f }
        }
    }
}

/// The ramp entry for `t` in `0..=1`. As a `u8` the index needs no bounds check.
fn index(t: f32) -> usize {
    (t * 255.0 + 0.5) as u8 as usize
}

pub(super) enum Kind<'a> {
    Linear {
        p0: Point,
        d: Point,
        inv_len2: f32,
    },
    Radial {
        c0: Point,
        r0: f32,
        cd: Point,
        dr: f32,
    },
    Sweep {
        c: Point,
        start: f32,
        span: f32,
    },
    /// A mip level, `scale` level pixels per image pixel in x and y; `repeat` tiles it, else edges clamp.
    Image {
        pixmap: &'a Pixmap,
        scale: (f32, f32),
        sampling: Sampling,
        repeat: bool,
    },
}

/// A per-pixel source: what it computes, the device-to-shader transform, and the colour filters after it.
pub(super) struct Shader<'a> {
    pub kind: Kind<'a>,
    pub ramp: Ramp,
    pub extend: Extend,
    pub inv: Transform,
    pub filters: &'a [ColorFilter],
    pub bgra: bool,
}

impl Shader<'_> {
    /// The colours of the pixels from (`x`, `y`) on, as many as `out` holds.
    /// `columns` is scratch memory for bilinear sampling, kept between rows.
    pub fn row(&self, level: Level, y: i32, x: i32, out: &mut [[u8; 4]], columns: &mut Vec<u32>) {
        // Each kind has its own loop, so the per-pixel work is only its own arithmetic.
        let inv = self.inv;
        let at = |i: usize| inv.map_point(Point { x: (x + i as i32) as f32 + 0.5, y: y as f32 + 0.5 });
        let (ramp, e) = (&self.ramp, self.extend);
        match self.kind {
            Kind::Linear { p0, d, inv_len2 } => {
                // t is affine along the row: the first pixel's, plus a step per pixel.
                let p = at(0);
                let t0 = ((p.x - p0.x) * d.x + (p.y - p0.y) * d.y) * inv_len2;
                let dt = (inv.a * d.x + inv.b * d.y) * inv_len2;
                dispatch!(level, simd => linear_row(simd, out, ramp, e, t0, dt));
            }
            Kind::Radial { c0, r0, cd, dr } => {
                for (i, px) in out.iter_mut().enumerate() {
                    *px = conical_t(at(i), c0, r0, cd, dr).map_or([0; 4], |t| ramp[index(extend(t, e))]);
                }
            }
            Kind::Sweep { c, start, span } => {
                for (i, px) in out.iter_mut().enumerate() {
                    let p = at(i);
                    let angle = (p.y - c.y).atan2(p.x - c.x).to_degrees().rem_euclid(360.0);
                    let t = if span != 0.0 { (angle - start) / span } else { 0.0 };
                    *px = ramp[index(extend(t, e))];
                }
            }
            Kind::Image { pixmap, scale, sampling, repeat } => {
                let uv = |i: usize| {
                    let p = at(i);
                    (p.x * scale.0, p.y * scale.1)
                };
                // Without rotation or skew a row samples one image row: bilinear then splits into two passes.
                let axis = sampling == Sampling::Linear && inv.b == 0.0 && !repeat && {
                    let r = AxisRow { inv, x, y, scale };
                    dispatch!(level, simd => bilinear_axis_row(simd, out, pixmap, r, columns))
                };
                match sampling {
                    _ if axis => {}
                    Sampling::Nearest => out.iter_mut().enumerate().for_each(|(i, px)| {
                        let (u, v) = uv(i);
                        *px = texel(pixmap, u.floor() as i64, v.floor() as i64, repeat);
                    }),
                    Sampling::Linear => out.iter_mut().enumerate().for_each(|(i, px)| {
                        let (u, v) = uv(i);
                        *px = bilinear(pixmap, u, v, repeat);
                    }),
                }
                if self.bgra {
                    out.iter_mut().for_each(|px| *px = [px[2], px[1], px[0], px[3]]);
                }
            }
        }
        if !self.filters.is_empty() {
            for px in out.iter_mut() {
                *px = filter_in_order(*px, self.filters, self.bgra);
            }
        }
    }
}

/// An image row drawn without rotation or skew: device pixel (`x + i`, `y`) maps through `inv` and then `scale`.
#[derive(Clone, Copy)]
struct AxisRow {
    inv: Transform,
    x: i32,
    y: i32,
    scale: (f32, f32),
}

/// Bilinear along a row that samples the same image row at every pixel, edges clamped. The two image rows are blended
/// once per column the row reads (a loop that vectorises), then each pixel blends two columns. Multiplied out, that is
/// [`bilinear`]'s sum, so the bytes are the same. `false`, with nothing written, when the columns read span more than
/// twice the row's pixels (a mip level is never that much larger than the draw).
#[simd]
fn bilinear_axis_row<S: Simd>(simd: S, out: &mut [[u8; 4]], p: &Pixmap, r: AxisRow, columns: &mut Vec<u32>) -> bool {
    let n = out.len();
    if n == 0 {
        return true;
    }
    let at = |i: usize| r.inv.map_point(Point { x: (r.x + i as i32) as f32 + 0.5, y: r.y as f32 + 0.5 });
    let u = |i: usize| at(i).x * r.scale.0 - 0.5;
    let (w, h) = (p.width as i64, p.height as i64);
    let v = at(0).y * r.scale.1 - 0.5;
    let y0 = v.floor();
    let fy = ((v - y0) * 256.0) as u32;
    let y0 = y0 as i64;
    let (top, bottom) = (y0.clamp(0, h - 1) as usize, (y0 + 1).clamp(0, h - 1) as usize);
    // u grows (or shrinks) steadily along the row, so its two ends bound the columns read.
    let ends = [u(0).floor() as i64, u(n - 1).floor() as i64];
    let (lo, hi) = (ends[0].min(ends[1]).clamp(0, w - 1), (ends[0].max(ends[1]) + 1).clamp(0, w - 1));
    let span = (hi - lo + 1) as usize;
    if span > 2 * n + 4 {
        return false;
    }
    let (stride, from) = (p.width as usize * 4, lo as usize * 4);
    let (t, b) = (&p.data[top * stride + from..][..span * 4], &p.data[bottom * stride + from..][..span * 4]);
    columns.clear();
    columns.extend(t.iter().zip(b).map(|(&t, &b)| t as u32 * (256 - fy) + b as u32 * fy));
    for (i, px) in out.iter_mut().enumerate() {
        let u = u(i);
        let x0 = u.floor();
        let fx = ((u - x0) * 256.0) as u32;
        let x0 = x0 as i64;
        let column = |x: i64| (x.clamp(0, w - 1) - lo) as usize * 4;
        let (a, b) = (&columns[column(x0)..][..4], &columns[column(x0 + 1)..][..4]);
        *px = [0, 1, 2, 3].map(|c| ((a[c] * (256 - fx) + b[c] * fx + 32768) >> 16) as u8);
    }
    true
}

/// A linear gradient's row: pixel `i` at `t0 + i·dt`. The ramp indices are computed 64 at a time, so that loop
/// vectorises; each extend mode has its own.
#[simd]
fn linear_row<S: Simd>(simd: S, out: &mut [[u8; 4]], ramp: &Ramp, e: Extend, t0: f32, dt: f32) {
    match e {
        Extend::Clamp => ramp_indices(out, ramp, t0, dt, |t| t.clamp(0.0, 1.0)),
        Extend::Repeat => ramp_indices(out, ramp, t0, dt, |t| extend(t, Extend::Repeat)),
        Extend::Mirror => ramp_indices(out, ramp, t0, dt, |t| extend(t, Extend::Mirror)),
    }
}

#[inline(always)]
fn ramp_indices(out: &mut [[u8; 4]], ramp: &Ramp, t0: f32, dt: f32, fold: impl Fn(f32) -> f32) {
    let mut idx = [0u8; 64];
    for (c, chunk) in out.chunks_mut(64).enumerate() {
        for (j, k) in idx.iter_mut().enumerate() {
            // In 0..=1 after the fold (NaN converts to 0), so the i32 is in 0..=255.
            *k = (fold(t0 + (c * 64 + j) as f32 * dt) * 255.0 + 0.5) as i32 as u8;
        }
        for (px, &k) in chunk.iter_mut().zip(&idx) {
            *px = ramp[k as usize];
        }
    }
}
/// The two-point conical gradient's `t` at `p`: the largest root of `|p − c(t)| = r(t)` with `r(t) ≥ 0`.
fn conical_t(p: Point, c0: Point, r0: f32, cd: Point, dr: f32) -> Option<f32> {
    let (px, py) = (p.x - c0.x, p.y - c0.y);
    let a = cd.x * cd.x + cd.y * cd.y - dr * dr;
    let b = px * cd.x + py * cd.y + r0 * dr;
    let c = px * px + py * py - r0 * r0;
    let ok = |t: f32| r0 + t * dr >= 0.0;
    if a.abs() < 1e-9 {
        // One root: the circles' radii grow as fast as their centres move.
        if b == 0.0 {
            return None;
        }
        let t = c / (2.0 * b);
        return ok(t).then_some(t);
    }
    let disc = b * b - a * c;
    if disc < 0.0 {
        return None;
    }
    let root = disc.sqrt();
    let (t1, t2) = ((b + root) / a, (b - root) / a);
    let (hi, lo) = if t1 > t2 { (t1, t2) } else { (t2, t1) };
    if ok(hi) {
        Some(hi)
    } else if ok(lo) {
        Some(lo)
    } else {
        None
    }
}

fn texel(p: &Pixmap, x: i64, y: i64, repeat: bool) -> [u8; 4] {
    let (w, h) = (p.width as i64, p.height as i64);
    let (x, y) = if repeat { (x.rem_euclid(w), y.rem_euclid(h)) } else { (x.clamp(0, w - 1), y.clamp(0, h - 1)) };
    p.pixel(x as u32, y as u32)
}

/// Bilinear between the four texels around (`u`, `v`), with 8-bit weights: integer arithmetic.
fn bilinear(p: &Pixmap, u: f32, v: f32, repeat: bool) -> [u8; 4] {
    let (u, v) = (u - 0.5, v - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (((u - x0) * 256.0) as u32, ((v - y0) * 256.0) as u32);
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (w, h) = (p.width as i64, p.height as i64);
    let q = if x0 >= 0 && y0 >= 0 && x0 + 1 < w && y0 + 1 < h {
        // All four inside: two reads of two adjacent texels, no wrapping or clamping.
        let at = ((y0 * w + x0) * 4) as usize;
        let (top, bottom) = (&p.data[at..at + 8], &p.data[at + (w * 4) as usize..at + (w * 4) as usize + 8]);
        let px = |s: &[u8], o: usize| [s[o], s[o + 1], s[o + 2], s[o + 3]];
        [px(top, 0), px(top, 4), px(bottom, 0), px(bottom, 4)]
    } else {
        [texel(p, x0, y0, repeat), texel(p, x0 + 1, y0, repeat), texel(p, x0, y0 + 1, repeat), texel(p, x0 + 1, y0 + 1, repeat)]
    };
    [0, 1, 2, 3].map(|c| {
        let top = q[0][c] as u32 * (256 - fx) + q[1][c] as u32 * fx;
        let bottom = q[2][c] as u32 * (256 - fx) + q[3][c] as u32 * fx;
        ((top * (256 - fy) + bottom * fy + 32768) >> 16) as u8
    })
}

/// `px` (premultiplied, in the target's order) through the colour filters, which work on unpremultiplied RGBA in
/// `0..=1`, each result clamped before the next.
pub(super) fn filter_in_order(px: [u8; 4], filters: &[ColorFilter], bgra: bool) -> [u8; 4] {
    let rgba = if bgra { [px[2], px[1], px[0], px[3]] } else { px };
    let a = rgba[3] as f32 / 255.0;
    let mut c = [0, 1, 2].map(|i| if a > 0.0 { (rgba[i] as f32 / 255.0 / a).min(1.0) } else { 0.0 });
    let mut alpha = a;
    for f in filters {
        match f {
            ColorFilter::Matrix(m) => {
                let v = [c[0], c[1], c[2], alpha];
                let row =
                    |r: usize| (m[r * 5] * v[0] + m[r * 5 + 1] * v[1] + m[r * 5 + 2] * v[2] + m[r * 5 + 3] * v[3] + m[r * 5 + 4]).clamp(0.0, 1.0);
                (c, alpha) = ([row(0), row(1), row(2)], row(3));
            }
            ColorFilter::Table(t) => {
                let look = |v: f32| t[(v * 255.0 + 0.5) as usize] as f32 / 255.0;
                (c, alpha) = (c.map(look), look(alpha));
            }
        }
    }
    let a8 = (alpha * 255.0 + 0.5) as u8;
    let ch = |v: f32| crate::paint::mul_div255((v * 255.0 + 0.5) as u8, a8);
    let out = [ch(c[0]), ch(c[1]), ch(c[2]), a8];
    if bgra { [out[2], out[1], out[0], out[3]] } else { out }
}

/// The three box sizes SVG's `feGaussianBlur` uses for standard deviation `sigma`, as (left reach, right reach) each.
pub(crate) fn box_passes(sigma: f32) -> [(usize, usize); 3] {
    let d = (sigma * 3.0 * (2.0 * std::f32::consts::PI).sqrt() / 4.0 + 0.5).floor() as usize;
    if d <= 1 {
        return [(0, 0); 3];
    }
    if d % 2 == 1 {
        let r = d / 2;
        [(r, r); 3]
    } else {
        let h = d / 2;
        [(h, h - 1), (h - 1, h), (h, h)]
    }
}

/// Blurs premultiplied pixels (`w × h`, rows packed) by standard deviation `sigma` pixels, transparent beyond the edges.
/// `scratch` is reused.
pub(super) fn blur(level: Level, px: &mut [u8], w: usize, h: usize, sigma: f32, scratch: &mut Vec<u8>) {
    // Below this a blur changes nothing visible, and Skia skips it.
    if sigma.is_nan() || sigma < 0.531_923 || w == 0 || h == 0 {
        return;
    }
    scratch.resize(px.len(), 0);
    if sigma < 2.0 {
        // A small blur is a Gaussian kernel itself: three boxes approximate it badly (Skia's `small_blur`).
        let (weights, radius) = gaussian(sigma);
        let k = &weights[..2 * radius + 1];
        dispatch!(level, simd => kernel_pass(simd, px, scratch, w, h, 4, w * 4, k));
        dispatch!(level, simd => kernel_pass(simd, px, scratch, h, w, w * 4, 4, k));
        return;
    }
    let passes = box_passes(sigma);
    for (left, right) in passes {
        dispatch!(level, simd => box_pass(simd, px, scratch, w, h, 4, w * 4, left, right));
    }
    for (left, right) in passes {
        dispatch!(level, simd => box_pass(simd, px, scratch, h, w, w * 4, 4, left, right));
    }
}

/// A sampled Gaussian of standard deviation `sigma` (under 2), out to `ceil(3 sigma)` either side, as 16-bit weights
/// summing to exactly 65536 (the centre takes the rounding), and that radius.
pub(crate) fn gaussian(sigma: f32) -> ([u32; 13], usize) {
    let radius = ((3.0 * sigma).ceil() as usize).min(6);
    let s = sigma as f64;
    let raw: [f64; 13] = std::array::from_fn(|i| {
        let x = i as f64 - radius as f64;
        if i <= 2 * radius { (-x * x / (2.0 * s * s)).exp() } else { 0.0 }
    });
    let total: f64 = raw.iter().sum();
    let mut w = raw.map(|v| (v / total * 65536.0).round() as u32);
    let sum: u32 = w.iter().sum();
    w[radius] = w[radius] + 65536 - sum;
    (w, radius)
}

/// One pass of a symmetric kernel along lines (laid out as in [`box_pass`]): each pixel the weighted sum of its
/// neighbours, `k[radius]` its own weight, zero beyond the ends.
#[simd]
#[allow(clippy::too_many_arguments, reason = "a strided pass's geometry stays explicit")]
fn kernel_pass<S: Simd>(simd: S, px: &mut [u8], tmp: &mut [u8], n: usize, lines: usize, step: usize, line_step: usize, k: &[u32]) {
    let radius = k.len() / 2;
    for line in 0..lines {
        let base = line * line_step;
        for i in 0..n {
            let mut sum = [32768u32; 4];
            let (from, to) = (i.saturating_sub(radius), (i + radius).min(n - 1));
            for j in from..=to {
                let (o, w) = (base + j * step, k[j + radius - i]);
                (0..4).for_each(|c| sum[c] += px[o + c] as u32 * w);
            }
            let o = base + i * step;
            (0..4).for_each(|c| tmp[o + c] = (sum[c] >> 16) as u8);
        }
        for i in 0..n {
            let o = base + i * step;
            px[o..o + 4].copy_from_slice(&tmp[o..o + 4]);
        }
    }
}

/// One box pass along lines of `n` pixels (`step` bytes apart), `lines` lines (`line_step` bytes apart): each pixel
/// the average of the pixels from `left` before it to `right` after, zero beyond the ends.
#[simd]
#[allow(clippy::too_many_arguments, reason = "a strided pass's geometry stays explicit")]
fn box_pass<S: Simd>(simd: S, px: &mut [u8], tmp: &mut [u8], n: usize, lines: usize, step: usize, line_step: usize, left: usize, right: usize) {
    let size = (left + right + 1) as u32;
    let divide = Divide::new(size);
    for line in 0..lines {
        let base = line * line_step;
        let texel = |px: &[u8], i: usize| -> [u32; 4] {
            let o = base + i * step;
            [px[o] as u32, px[o + 1] as u32, px[o + 2] as u32, px[o + 3] as u32]
        };
        // The window's sum, all four channels together.
        let mut sum = [0u32; 4];
        for i in 0..=right.min(n - 1) {
            let t = texel(px, i);
            (0..4).for_each(|c| sum[c] += t[c]);
        }
        for i in 0..n {
            let o = base + i * step;
            for c in 0..4 {
                tmp[o + c] = divide.of(sum[c] + size / 2) as u8;
            }
            if i + right + 1 < n {
                let t = texel(px, i + right + 1);
                (0..4).for_each(|c| sum[c] += t[c]);
            }
            if i >= left {
                let t = texel(px, i - left);
                (0..4).for_each(|c| sum[c] -= t[c]);
            }
        }
        for i in 0..n {
            let o = base + i * step;
            px[o..o + 4].copy_from_slice(&tmp[o..o + 4]);
        }
    }
}
/// Division by a box's size, `x / size`, for every `x` below `256 · size` (a window's sum of 8-bit values, plus half
/// the size to round). Under 4096 it is a multiply by `m = ⌊2³² / size⌋ + 1` and a shift: `x · m / 2³²` exceeds
/// `x / size` by less than `x / 2³² < 256 · size / 2³²`, which is under `1 / size` (so never reaches the next whole
/// number) while `size² < 2²⁴`. Larger boxes divide.
#[derive(Clone, Copy)]
struct Divide {
    size: u32,
    m: u64,
}

impl Divide {
    fn new(size: u32) -> Divide {
        Divide { size, m: (1u64 << 32) / size as u64 + 1 }
    }

    #[inline(always)]
    fn of(self, x: u32) -> u32 {
        if self.size < 4096 { ((x as u64 * self.m) >> 32) as u32 } else { x / self.size }
    }
}

/// Erodes premultiplied pixels (`w × h`): each channel's minimum within `radius` pixels in x and y, transparent beyond
/// the edges.
pub(super) fn erode(level: Level, px: &mut [u8], w: usize, h: usize, radius: usize, scratch: &mut Vec<u8>) {
    if radius == 0 || w == 0 || h == 0 {
        return;
    }
    // The forward running minima take the image's size, the backward ones a line's.
    scratch.resize(px.len() + 4 * w.max(h), 0);
    let (forward, back) = scratch.split_at_mut(px.len());
    dispatch!(level, simd => min_pass(simd, px, forward, back, w, h, 4, w * 4, radius));
    dispatch!(level, simd => min_pass(simd, px, forward, back, h, w, w * 4, 4, radius));
}

/// One erode pass along lines (laid out as in [`box_pass`]): each pixel the minimum of the `2 radius + 1` around it,
/// in constant time at any radius (van Herk and Gil–Werman). The line is cut into blocks a window long; `forward` holds
/// each pixel's minimum from its block's start, `back` (one line) from it to its block's end, and a window spanning
/// two blocks is the minimum of its start's `back` and its end's `forward`.
#[simd]
#[allow(clippy::too_many_arguments, reason = "a strided pass's geometry stays explicit")]
fn min_pass<S: Simd>(
    simd: S,
    px: &mut [u8],
    forward: &mut [u8],
    back: &mut [u8],
    n: usize,
    lines: usize,
    step: usize,
    line_step: usize,
    radius: usize,
) {
    let size = 2 * radius + 1;
    let at = |base: usize, i: usize| base + i * step;
    for line in 0..lines {
        let base = line * line_step;
        // Beyond the ends is transparent, so a window reaching past them erodes to zero: with no window inside the
        // line, all of it does.
        if n < size {
            (0..n).for_each(|i| px[at(base, i)..at(base, i) + 4].fill(0));
            continue;
        }
        for i in 0..n {
            let o = at(base, i);
            if i % size == 0 {
                forward[o..o + 4].copy_from_slice(&px[o..o + 4]);
            } else {
                let p = at(base, i - 1);
                (0..4).for_each(|c| forward[o + c] = forward[p + c].min(px[o + c]));
            }
        }
        for i in (0..n).rev() {
            let o = at(base, i);
            for c in 0..4 {
                back[4 * i + c] = if i == n - 1 || (i + 1) % size == 0 { px[o + c] } else { back[4 * (i + 1) + c].min(px[o + c]) };
            }
        }
        for i in 0..n {
            let o = at(base, i);
            for c in 0..4 {
                px[o + c] = if i >= radius && i + radius < n { back[4 * (i - radius) + c].min(forward[at(base, i + radius) + c]) } else { 0 };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::Color;

    #[test]
    fn ramps_interpolate_unpremultiplied_and_keep_hard_stops() {
        let stops = [Stop { offset: 0.0, color: Color::rgba(255, 0, 0, 255) }, Stop { offset: 1.0, color: Color::rgba(0, 0, 255, 0) }];
        let r = ramp(&stops, false);
        assert_eq!(r[0], [255, 0, 0, 255]);
        assert_eq!(r[255], [0, 0, 0, 0]);
        // Entry 128 is t = 128/255: (127, 0, 128) unpremultiplied at alpha 127, then premultiplied.
        assert_eq!(r[128], [63, 0, 64, 127]);
        let hard = [
            Stop { offset: 0.0, color: Color::WHITE },
            Stop { offset: 0.5, color: Color::WHITE },
            Stop { offset: 0.5, color: Color::BLACK },
            Stop { offset: 1.0, color: Color::BLACK },
        ];
        let h = ramp(&hard, false);
        assert_eq!((h[127], h[128]), ([255; 4], [0, 0, 0, 255]));
    }

    #[test]
    fn extend_modes_fold_t() {
        assert_eq!(extend(1.25, Extend::Clamp), 1.0);
        assert_eq!(extend(1.25, Extend::Repeat), 0.25);
        assert_eq!(extend(1.25, Extend::Mirror), 0.75);
        assert_eq!(extend(-0.25, Extend::Mirror), 0.25);
    }

    #[test]
    fn conical_t_covers_the_radial_and_two_circle_cases() {
        let o = Point { x: 0.0, y: 0.0 };
        // A plain radial gradient, radius 10: t is the distance over 10.
        let t = conical_t(Point { x: 6.0, y: 8.0 }, o, 0.0, o, 10.0).expect("inside");
        assert!((t - 1.0).abs() < 1e-6, "{t}");
        // Two circles of radius 5, 20 apart: the circles at t = 0.25 and 0.75 pass through (10, 0), and the larger t
        // wins; outside the tube their tangents bound, nothing is painted.
        let (c0, cd) = (o, Point { x: 20.0, y: 0.0 });
        assert!((conical_t(Point { x: 10.0, y: 0.0 }, c0, 5.0, cd, 0.0).expect("between") - 0.75).abs() < 1e-6);
        assert_eq!(conical_t(Point { x: 10.0, y: 9.0 }, c0, 5.0, cd, 0.0), None, "outside the tube");
    }

    #[test]
    fn colour_filters_apply_in_order_with_clamping() {
        // Brighten by 0.5, then halve: clamped to 1 first, so the result is 0.5, not 0.75.
        let mut brighten = [0.0; 20];
        let mut halve = [0.0; 20];
        for r in 0..4 {
            brighten[r * 5 + r] = 1.0;
            halve[r * 5 + r] = if r < 3 { 0.5 } else { 1.0 };
        }
        for r in 0..3 {
            brighten[r * 5 + 4] = 0.5;
        }
        let px = filter_in_order([153, 153, 153, 255], &[ColorFilter::Matrix(brighten), ColorFilter::Matrix(halve)], false);
        assert_eq!(px, [128, 128, 128, 255]);
        let table: [u8; 256] = std::array::from_fn(|i| 255 - i as u8);
        // The table applies to alpha too: an opaque pixel turns transparent, which premultiplied is all zero.
        assert_eq!(filter_in_order([0, 255, 0, 255], &[ColorFilter::Table(table)], false), [0, 0, 0, 0]);
        // At half alpha the inverted colour survives: green (0, 255, 0) at 128 becomes magenta at 127.
        assert_eq!(filter_in_order([0, 128, 0, 128], &[ColorFilter::Table(table)], false), [127, 0, 127, 127]);
    }

    #[test]
    fn blur_spreads_and_keeps_the_total() {
        // A vertical line down the middle of a 31 × 31 image: across its middle row it spreads and keeps its total.
        let (w, h) = (31, 31);
        let mut px = vec![0u8; w * h * 4];
        for y in 0..h {
            px[(y * w + 15) * 4 + 3] = 255;
        }
        let mut scratch = Vec::new();
        blur(Level::new(), &mut px, w, h, 2.0, &mut scratch);
        let alpha: Vec<u32> = px[15 * w * 4..16 * w * 4].chunks(4).map(|p| p[3] as u32).collect();
        assert!(alpha[15] < 255 && alpha[15] >= alpha[13] && alpha[13] > alpha[11], "{alpha:?}");
        let total: u32 = alpha.iter().sum();
        assert!(total.abs_diff(255) < 16, "{total}");
        let mut levels = [Level::fallback(), Level::baseline(), Level::new()].map(|level| {
            let mut p = vec![0u8; 16 * 16 * 4];
            p.iter_mut().enumerate().for_each(|(i, v)| *v = (i * 37 % 251) as u8);
            blur(level, &mut p, 16, 16, 3.0, &mut Vec::new());
            blur(level, &mut p, 16, 16, 1.2, &mut Vec::new());
            erode(level, &mut p, 16, 16, 1, &mut Vec::new());
            p
        });
        let last = levels[2].clone();
        assert!(levels.iter_mut().all(|p| *p == last), "every SIMD level filters alike");
    }

    #[test]
    fn box_division_by_multiplying_is_exact() {
        // Where a quotient changes is where an error would show: either side of every multiple, up to the largest sum.
        for size in 1..4200u32 {
            let d = Divide::new(size);
            for k in 1..=256 {
                for x in [k * size - 1, k * size] {
                    assert_eq!(d.of(x), x / size, "{x} / {size}");
                }
            }
        }
    }

    #[test]
    fn small_blurs_are_gaussian() {
        // A single opaque pixel blurred by sigma 1: its alpha spreads as the Gaussian does (about 0.4, 0.24 and 0.05
        // of it at 0, 1 and 2 pixels along each axis), keeps its total, and a sigma under 0.53 leaves it alone.
        let (w, h) = (9, 9);
        let dot = || {
            let mut px = vec![0u8; w * h * 4];
            px[(4 * w + 4) * 4 + 3] = 255;
            px
        };
        let mut px = dot();
        blur(Level::new(), &mut px, w, h, 1.0, &mut Vec::new());
        let a = |x: usize, y: usize| px[(y * w + x) * 4 + 3] as u32;
        let total: u32 = (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).map(|(x, y)| a(x, y)).sum();
        assert!(total.abs_diff(255) < 8, "{total}");
        let expect = |k: f64| (255.0 * k).round() as u32;
        assert!(a(4, 4).abs_diff(expect(0.399 * 0.399)) <= 1 && a(5, 4).abs_diff(expect(0.399 * 0.242)) <= 1, "{} {}", a(4, 4), a(5, 4));
        assert!(a(6, 4).abs_diff(expect(0.399 * 0.054)) <= 1 && a(4, 1) == 0, "{} {}", a(6, 4), a(4, 1));
        let mut faint = dot();
        blur(Level::new(), &mut faint, w, h, 0.5, &mut Vec::new());
        assert_eq!(faint, dot());
        let (weights, radius) = gaussian(1.9);
        assert_eq!((weights.iter().sum::<u32>(), radius), (65536, 6));
    }

    #[test]
    fn axis_aligned_bilinear_rows_match_the_per_pixel_sampler() {
        let pixmap = Pixmap { width: 7, height: 5, data: (0..7 * 5 * 4).map(|i| (i * 53 % 256) as u8).collect() };
        let mut columns = Vec::new();
        // Shrunk, enlarged, flipped, and reaching past every edge (clamped there).
        for (a, d, e, f) in [(1.7, 1.3, -2.0, -1.5), (0.31, 0.27, 0.4, 0.1), (-0.9, 0.6, 8.0, -0.7), (0.05, 2.0, -0.3, 0.0)] {
            let inv = Transform { a, b: 0.0, c: 0.0, d, e, f };
            for level in [Level::fallback(), Level::baseline(), Level::new()] {
                for y in -2..8 {
                    let mut out = [[0u8; 4]; 13];
                    let r = AxisRow { inv, x: -3, y, scale: (1.0, 1.0) };
                    assert!(dispatch!(level, simd => bilinear_axis_row(simd, &mut out, &pixmap, r, &mut columns)));
                    for (i, px) in out.iter().enumerate() {
                        let p = inv.map_point(Point { x: (i as i32 - 3) as f32 + 0.5, y: y as f32 + 0.5 });
                        assert_eq!(*px, bilinear(&pixmap, p.x, p.y, false), "{inv:?} at ({i}, {y})");
                    }
                }
            }
        }
    }

    #[test]
    fn erode_is_the_window_minimum_at_any_radius() {
        // A naive minimum over each pixel's window, x then y, zero where the window passes an edge.
        let naive = |px: &[u8], w: usize, h: usize, r: usize| {
            let pass = |src: &[u8], n: usize, lines: usize, step: usize, line_step: usize| {
                let mut out = src.to_vec();
                for line in 0..lines {
                    for i in 0..n {
                        for c in 0..4 {
                            let o = line * line_step + i * step + c;
                            out[o] = if i >= r && i + r < n {
                                (i - r..=i + r).map(|j| src[line * line_step + j * step + c]).min().unwrap_or(0)
                            } else {
                                0
                            };
                        }
                    }
                }
                out
            };
            let x = pass(px, w, h, 4, w * 4);
            pass(&x, h, w, w * 4, 4)
        };
        let (w, h) = (23, 17);
        let px: Vec<u8> = (0..w * h * 4).map(|i| ((i * 2_654_435_761usize) >> 7) as u8).collect();
        for r in [1, 2, 3, 5, 8, 11, 12, 40] {
            let mut fast = px.clone();
            erode(Level::new(), &mut fast, w, h, r, &mut Vec::new());
            assert_eq!(fast, naive(&px, w, h, r), "radius {r}");
        }
    }

    #[test]
    fn erode_shrinks_shapes() {
        let (w, h) = (7, 7);
        let mut px = vec![0u8; w * h * 4];
        for y in 1..6 {
            for x in 1..6 {
                px[(y * w + x) * 4 + 3] = 255;
            }
        }
        erode(Level::new(), &mut px, w, h, 1, &mut Vec::new());
        let at = |x: usize, y: usize| px[(y * w + x) * 4 + 3];
        assert_eq!((at(1, 1), at(2, 2), at(4, 4), at(5, 5)), (0, 255, 255, 0));
    }
}
