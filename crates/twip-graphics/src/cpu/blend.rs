//! Pixel kernels: a premultiplied source (one colour, or one per pixel), scaled by coverage, blended into premultiplied
//! 8-bit pixels.
//!
//! Every kernel is generic over the SIMD level (`fearless_simd`'s `#[simd]` compiles each loop with that level's
//! instructions). Coverage `c` lerps the destination towards the blend's result, `d + c·(blend(s, d) - d)`.
//!
//! - Source-over, multiply, destination-in and clear (what Twip's own drawing uses) are exact 8-bit integer
//!   arithmetic. For source-over and multiply, both linear in the source, the lerp is the same as scaling the source
//!   by `c` first, which is what they do.
//! - Every other mode (the rest of the Porter-Duff operators, and the separable and non-separable blend modes of W3C
//!   Compositing and Blending, which COLRv1 composites with) is computed per pixel in `f32`, in a fixed order with no
//!   fused operations, and rounded back.
//!
//! Either way every SIMD level writes the same bytes.

use fearless_simd::{Level, Simd, dispatch};
use fearless_simd_macros::simd;

use crate::paint::BlendMode;

/// `round(x / 255)` for `x = a·b` with `a, b <= 255`, in 16-bit arithmetic: exactly `paint::mul_div255`.
#[inline(always)]
fn div255(x: u16) -> u16 {
    let t = x + 128;
    (t + (t >> 8)) >> 8
}

#[inline(always)]
fn mul(a: u8, b: u8) -> u8 {
    div255(a as u16 * b as u16) as u8
}

/// A solid source: a premultiplied colour in the target's channel order, and how it blends. `bgra` says the order (the
/// non-separable modes weigh red, green and blue differently).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Solid {
    pub color: [u8; 4],
    pub blend: BlendMode,
    pub bgra: bool,
}

/// Blends `src` into `dst` (whole pixels), pixel `i` at coverage `cov[i]`.
pub(super) fn mask(level: Level, dst: &mut [u8], cov: &[u8], src: Solid) {
    dispatch!(level, simd => mask_kernel(simd, dst, cov, src));
}

/// Blends `src` into every pixel of `dst` at coverage `cov`.
pub(super) fn uniform(level: Level, dst: &mut [u8], cov: u8, src: Solid) {
    dispatch!(level, simd => uniform_kernel(simd, dst, cov, src));
}

/// Blends a source of one premultiplied colour per pixel, `src[i]`, into `dst` at coverage `cov[i]`. `src.color` is
/// unused.
pub(super) fn shaded(level: Level, dst: &mut [u8], cov: &[u8], colors: &[[u8; 4]], src: Solid) {
    dispatch!(level, simd => shaded_kernel(simd, dst, cov, colors, src));
}

/// Blends `src` into a `width × height` block of pixels at coverage from a block of coverage: `dst` starts at the
/// block's first pixel with rows `dst_stride` bytes apart, `cov` at its first value with rows `cov_stride` apart. One
/// dispatch for the whole block: a glyph's rows are short.
#[allow(clippy::too_many_arguments, reason = "two strided blocks and a source")]
pub(super) fn mask_block(level: Level, dst: &mut [u8], dst_stride: usize, cov: &[u8], cov_stride: usize, width: usize, height: usize, src: Solid) {
    dispatch!(level, simd => mask_block_kernel(simd, dst, dst_stride, cov, cov_stride, width, height, src));
}

/// Scales each coverage value by the matching one in `by`.
pub(crate) fn scale(level: Level, cov: &mut [u8], by: &[u8]) {
    dispatch!(level, simd => scale_kernel(simd, cov, by));
}

#[simd]
#[allow(clippy::too_many_arguments, reason = "two strided blocks and a source")]
fn mask_block_kernel<S: Simd>(simd: S, dst: &mut [u8], dst_stride: usize, cov: &[u8], cov_stride: usize, width: usize, height: usize, src: Solid) {
    // Source-over, the glyph blit's mode, row by row here: a glyph's rows are a few pixels each, so one call per row
    // would cost more than the row.
    let rows = (0..height).map(|row| (row * dst_stride, row * cov_stride));
    if src.blend == BlendMode::SrcOver {
        for (d, c) in rows {
            over_row(&mut dst[d..d + width * 4], &cov[c..c + width], src.color);
        }
        return;
    }
    for (d, c) in rows {
        mask_kernel(simd, &mut dst[d..d + width * 4], &cov[c..c + width], src);
    }
}

/// `over` of `s` at each pixel's coverage: the source scaled, then the destination by the source's inverse alpha,
/// added. The same bytes as `over(d, s.map(|v| mul(v, c)))`; no coverage leaves a pixel.
///
/// Eight pixels at a time with no branch inside, so the level's vector instructions take them together (on NEON,
/// interleaved loads split the channels); the last few are padded to a block, since a glyph's rows are often shorter
/// than one. A block with no coverage is skipped and one fully covered by an opaque source is stored: most of a glyph
/// mask is one or the other.
#[inline(always)]
fn over_row(dst: &mut [u8], cov: &[u8], s: [u8; 4]) {
    const N: usize = 8;
    #[inline(always)]
    fn block(d: &mut [[u8; 4]; N], c: &[u8; N], s: [u8; 4]) {
        if *c == [0; N] {
            return;
        }
        if s[3] == 255 && *c == [255; N] {
            *d = [s; N];
            return;
        }
        let s16 = s.map(u16::from);
        for (d, &c) in d.iter_mut().zip(c) {
            let k = u16::from(c);
            let inv = 255 - div255(s16[3] * k);
            for (d, &s) in d.iter_mut().zip(&s16) {
                *d = (div255(s * k) + div255(u16::from(*d) * inv)).min(255) as u8;
            }
        }
    }
    let px = dst.as_chunks_mut::<4>().0;
    let n = px.len().min(cov.len());
    let (blocks, px_tail) = px[..n].as_chunks_mut::<N>();
    let (covs, cov_tail) = cov[..n].as_chunks::<N>();
    for (d, c) in blocks.iter_mut().zip(covs) {
        block(d, c, s);
    }
    if !px_tail.is_empty() {
        // Fixed-length copies: a few pixels each, where `copy_from_slice` calls `memcpy`.
        let (mut d, mut c) = ([[0; 4]; N], [0; N]);
        for i in 0..N {
            if let (Some(p), Some(&k)) = (px_tail.get(i), cov_tail.get(i)) {
                (d[i], c[i]) = (*p, k);
            }
        }
        block(&mut d, &c, s);
        for (i, d) in d.into_iter().enumerate() {
            if let Some(p) = px_tail.get_mut(i) {
                *p = d;
            }
        }
    }
}

#[simd]
fn mask_kernel<S: Simd>(simd: S, dst: &mut [u8], cov: &[u8], src: Solid) {
    let px = dst.as_chunks_mut::<4>().0;
    let s = src.color;
    match src.blend {
        BlendMode::SrcOver => over_row(dst, cov, s),
        BlendMode::Multiply => {
            for (d, &c) in px.iter_mut().zip(cov) {
                multiply(d, s.map(|v| mul(v, c)));
            }
        }
        BlendMode::DstIn => {
            for (d, &c) in px.iter_mut().zip(cov) {
                scale_px(d, 255 - mul(c, 255 - s[3]));
            }
        }
        BlendMode::Clear => {
            for (d, &c) in px.iter_mut().zip(cov) {
                scale_px(d, 255 - c);
            }
        }
        mode => {
            for (d, &c) in px.iter_mut().zip(cov) {
                *d = lerp(*d, general(s, *d, mode, src.bgra), c);
            }
        }
    }
}

#[simd]
fn uniform_kernel<S: Simd>(simd: S, dst: &mut [u8], c: u8, src: Solid) {
    let px = dst.as_chunks_mut::<4>().0;
    let s = src.color;
    match src.blend {
        // An opaque source at full coverage replaces the pixels.
        BlendMode::SrcOver if c == 255 && s[3] == 255 => px.fill(s),
        BlendMode::SrcOver => {
            let s = s.map(|v| mul(v, c));
            for d in px {
                over(d, s);
            }
        }
        BlendMode::Multiply => {
            let s = s.map(|v| mul(v, c));
            for d in px {
                multiply(d, s);
            }
        }
        BlendMode::DstIn | BlendMode::Clear => {
            let k = if src.blend == BlendMode::DstIn { 255 - mul(c, 255 - s[3]) } else { 255 - c };
            if k == 0 {
                px.fill([0; 4]);
            } else {
                for d in px {
                    scale_px(d, k);
                }
            }
        }
        mode => {
            for d in px {
                *d = lerp(*d, general(s, *d, mode, src.bgra), c);
            }
        }
    }
}

#[simd]
fn shaded_kernel<S: Simd>(simd: S, dst: &mut [u8], cov: &[u8], colors: &[[u8; 4]], src: Solid) {
    let px = dst.as_chunks_mut::<4>().0;
    let each = px.iter_mut().zip(cov).zip(colors);
    match src.blend {
        BlendMode::SrcOver => each.for_each(|((d, &c), &s)| over(d, s.map(|v| mul(v, c)))),
        BlendMode::Multiply => each.for_each(|((d, &c), &s)| multiply(d, s.map(|v| mul(v, c)))),
        BlendMode::DstIn => each.for_each(|((d, &c), &s)| scale_px(d, 255 - mul(c, 255 - s[3]))),
        BlendMode::Clear => each.for_each(|((d, &c), _)| scale_px(d, 255 - c)),
        mode => each.for_each(|((d, &c), &s)| *d = lerp(*d, general(s, *d, mode, src.bgra), c)),
    }
}

#[simd]
fn scale_kernel<S: Simd>(simd: S, cov: &mut [u8], by: &[u8]) {
    for (c, &b) in cov.iter_mut().zip(by) {
        *c = mul(*c, b);
    }
}

#[inline(always)]
fn over(d: &mut [u8; 4], s: [u8; 4]) {
    let inv = 255 - s[3];
    for (d, s) in d.iter_mut().zip(s) {
        *d = s.saturating_add(mul(*d, inv));
    }
}

#[inline(always)]
fn multiply(d: &mut [u8; 4], s: [u8; 4]) {
    let (inv_sa, inv_da) = (255 - s[3], 255 - d[3]);
    for (d, s) in d.iter_mut().zip(s) {
        *d = mul(s, *d).saturating_add(mul(s, inv_da)).saturating_add(mul(*d, inv_sa));
    }
}

#[inline(always)]
fn scale_px(d: &mut [u8; 4], k: u8) {
    for d in d.iter_mut() {
        *d = mul(*d, k);
    }
}

/// `d` moved towards `r` by coverage `c`, per channel: `d + c·(r - d)`, rounded.
#[inline(always)]
fn lerp(d: [u8; 4], r: [u8; 4], c: u8) -> [u8; 4] {
    if c == 255 {
        return r;
    }
    let k = c as i32;
    [0, 1, 2, 3].map(|i| (d[i] as i32 + ((r[i] as i32 - d[i] as i32) * k + if r[i] >= d[i] { 127 } else { -127 }) / 255) as u8)
}

/// `blend(s, d)` for the modes without an integer fast path, on premultiplied colour in `f32` (W3C Compositing and
/// Blending, as Skia computes them). Kept out of line: inlined, it bloats every kernel's hot loops.
#[inline(never)]
fn general(s: [u8; 4], d: [u8; 4], mode: BlendMode, bgra: bool) -> [u8; 4] {
    let f = |v: u8| v as f32 / 255.0;
    let (sa, da) = (f(s[3]), f(d[3]));
    let (sc, dc) = ([f(s[0]), f(s[1]), f(s[2])], [f(d[0]), f(d[1]), f(d[2])]);
    let each = |g: &dyn Fn(f32, f32) -> f32| [g(sc[0], dc[0]), g(sc[1], dc[1]), g(sc[2], dc[2])];
    let (rgb, a) = match mode {
        BlendMode::Clear => ([0.0; 3], 0.0),
        BlendMode::Src => (sc, sa),
        BlendMode::Dst => (dc, da),
        BlendMode::SrcOver => (each(&|s, d| s + d * (1.0 - sa)), sa + da * (1.0 - sa)),
        BlendMode::DstOver => (each(&|s, d| d + s * (1.0 - da)), da + sa * (1.0 - da)),
        BlendMode::SrcIn => (each(&|s, _| s * da), sa * da),
        BlendMode::DstIn => (each(&|_, d| d * sa), da * sa),
        BlendMode::SrcOut => (each(&|s, _| s * (1.0 - da)), sa * (1.0 - da)),
        BlendMode::DstOut => (each(&|_, d| d * (1.0 - sa)), da * (1.0 - sa)),
        BlendMode::SrcAtop => (each(&|s, d| s * da + d * (1.0 - sa)), da),
        BlendMode::DstAtop => (each(&|s, d| d * sa + s * (1.0 - da)), sa),
        BlendMode::Xor => (each(&|s, d| s * (1.0 - da) + d * (1.0 - sa)), sa * (1.0 - da) + da * (1.0 - sa)),
        BlendMode::Plus => (each(&|s, d| (s + d).min(1.0)), (sa + da).min(1.0)),
        BlendMode::Screen
        | BlendMode::Overlay
        | BlendMode::Darken
        | BlendMode::Lighten
        | BlendMode::ColorDodge
        | BlendMode::ColorBurn
        | BlendMode::HardLight
        | BlendMode::SoftLight
        | BlendMode::Difference
        | BlendMode::Exclusion
        | BlendMode::Multiply => {
            let un = |c: f32, a: f32| if a > 0.0 { (c / a).min(1.0) } else { 0.0 };
            let mix = |s: f32, d: f32| s * (1.0 - da) + d * (1.0 - sa) + sa * da * separable(mode, un(s, sa), un(d, da));
            (each(&mix), sa + da - sa * da)
        }
        BlendMode::Hue | BlendMode::Saturation | BlendMode::Color | BlendMode::Luminosity => {
            let un = |c: [f32; 3], a: f32| c.map(|v| if a > 0.0 { (v / a).min(1.0) } else { 0.0 });
            let weights = if bgra { [0.11, 0.59, 0.3] } else { [0.3, 0.59, 0.11] };
            let b = non_separable(mode, un(sc, sa), un(dc, da), weights);
            ([0, 1, 2].map(|i| sc[i] * (1.0 - da) + dc[i] * (1.0 - sa) + sa * da * b[i]), sa + da - sa * da)
        }
    };
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
    let a = q(a);
    // Premultiplied: no channel above alpha.
    [q(rgb[0]).min(a), q(rgb[1]).min(a), q(rgb[2]).min(a), a]
}

/// A separable blend function on unpremultiplied channels.
#[inline(always)]
fn separable(mode: BlendMode, s: f32, d: f32) -> f32 {
    let multiply = |a: f32, b: f32| a * b;
    let screen = |a: f32, b: f32| a + b - a * b;
    let hard_light = |s: f32, d: f32| if s <= 0.5 { multiply(d, 2.0 * s) } else { screen(d, 2.0 * s - 1.0) };
    match mode {
        BlendMode::Multiply => multiply(s, d),
        BlendMode::Screen => screen(s, d),
        BlendMode::Overlay => hard_light(d, s),
        BlendMode::Darken => s.min(d),
        BlendMode::Lighten => s.max(d),
        BlendMode::ColorDodge => {
            if d == 0.0 {
                0.0
            } else if s >= 1.0 {
                1.0
            } else {
                (d / (1.0 - s)).min(1.0)
            }
        }
        BlendMode::ColorBurn => {
            if d >= 1.0 {
                1.0
            } else if s <= 0.0 {
                0.0
            } else {
                1.0 - ((1.0 - d) / s).min(1.0)
            }
        }
        BlendMode::HardLight => hard_light(s, d),
        BlendMode::SoftLight => {
            if s <= 0.5 {
                d - (1.0 - 2.0 * s) * d * (1.0 - d)
            } else {
                let dd = if d <= 0.25 { ((16.0 * d - 12.0) * d + 4.0) * d } else { d.sqrt() };
                d + (2.0 * s - 1.0) * (dd - d)
            }
        }
        BlendMode::Difference => (s - d).abs(),
        BlendMode::Exclusion => s + d - 2.0 * s * d,
        // Not separable modes: the caller sends only the ones above.
        _ => s,
    }
}

/// A non-separable blend function on unpremultiplied red, green and blue (in the target's order, weighted by
/// `weights`).
#[inline(always)]
fn non_separable(mode: BlendMode, s: [f32; 3], d: [f32; 3], weights: [f32; 3]) -> [f32; 3] {
    let lum = |c: [f32; 3]| c[0] * weights[0] + c[1] * weights[1] + c[2] * weights[2];
    let clip = |c: [f32; 3]| {
        let l = lum(c);
        let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
        let mut c = c;
        if n < 0.0 && l - n != 0.0 {
            c = c.map(|v| l + (v - l) * l / (l - n));
        }
        if x > 1.0 && x - l != 0.0 {
            c = c.map(|v| l + (v - l) * (1.0 - l) / (x - l));
        }
        c
    };
    let set_lum = |c: [f32; 3], l: f32| {
        let dl = l - lum(c);
        clip(c.map(|v| v + dl))
    };
    let sat = |c: [f32; 3]| c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2]);
    let set_sat = |c: [f32; 3], s: f32| {
        let (n, x) = (c[0].min(c[1]).min(c[2]), c[0].max(c[1]).max(c[2]));
        if x > n { c.map(|v| (v - n) * s / (x - n)) } else { [0.0; 3] }
    };
    match mode {
        BlendMode::Hue => set_lum(set_sat(s, sat(d)), lum(d)),
        BlendMode::Saturation => set_lum(set_sat(d, sat(s)), lum(d)),
        BlendMode::Color => set_lum(s, lum(d)),
        BlendMode::Luminosity => set_lum(d, lum(s)),
        // Separable modes: the caller sends only the four above.
        _ => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paint::mul_div255;

    const MODES: [BlendMode; 28] = [
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

    fn solid(color: [u8; 4], blend: BlendMode) -> Solid {
        Solid { color, blend, bgra: false }
    }

    #[test]
    fn over_row_is_over() {
        // The same bytes as `over`, with whole blocks of no and full coverage among the others, and opaque sources
        // (the blocks that are skipped or stored).
        let (px, mut cov) = sample();
        cov[32..64].fill(0);
        cov[96..128].fill(255);
        cov[200..216].fill(255);
        let opaque = [[0, 0, 0, 255], [255, 255, 255, 255], [10, 200, 30, 255]];
        for s in px.as_chunks::<4>().0.iter().step_by(7).chain(&opaque) {
            let mut a = px.clone();
            let mut b = px.clone();
            over_row(&mut a, &cov, *s);
            for (d, &c) in b.as_chunks_mut::<4>().0.iter_mut().zip(&cov) {
                over(d, s.map(|v| mul(v, c)));
            }
            assert_eq!(a, b, "{s:?}");
        }
    }

    #[test]
    fn div255_is_the_exact_product() {
        for a in 0..=255u8 {
            for b in 0..=255u8 {
                assert_eq!(mul(a, b), mul_div255(a, b), "{a} × {b}");
            }
        }
    }

    /// Pixels and coverage with every value of each byte represented, and premultiplied pixels.
    fn sample() -> (Vec<u8>, Vec<u8>) {
        let mut px = Vec::new();
        let mut cov = Vec::new();
        for i in 0..1021u32 {
            let a = (i * 37 % 256) as u8;
            px.extend([(i * 11 % 256) as u8, (i * 101 % 256) as u8, (i * 7 % 256) as u8].map(|v| mul(v, a)).into_iter().chain([a]));
            cov.push((i * 53 % 256) as u8);
        }
        (px, cov)
    }

    #[test]
    fn every_level_blends_alike() {
        let levels = [Level::fallback(), Level::baseline(), Level::new()];
        for blend in MODES {
            for color in [[200, 100, 0, 200], [10, 20, 30, 255], [0, 0, 0, 0]] {
                let src = solid(color, blend);
                let results: Vec<_> = levels
                    .iter()
                    .map(|&level| {
                        let (mut a, cov) = sample();
                        mask(level, &mut a, &cov, src);
                        let (mut b, _) = sample();
                        uniform(level, &mut b, 77, src);
                        let (mut c, cov) = sample();
                        let colors: Vec<[u8; 4]> = cov.iter().map(|&v| [v / 2, v / 3, v / 4, v]).collect();
                        shaded(level, &mut c, &cov, &colors, src);
                        (a, b, c)
                    })
                    .collect();
                assert!(results.windows(2).all(|w| w[0] == w[1]), "{blend:?} {color:?}");
            }
        }
    }

    #[test]
    fn uniform_and_shaded_match_mask_at_the_same_coverage() {
        for blend in MODES {
            for c in [0, 1, 128, 254, 255] {
                let src = solid([90, 60, 30, 120], blend);
                let (mut a, _) = sample();
                let (mut b, mut s) = (a.clone(), a.clone());
                let n = b.len() / 4;
                mask(Level::new(), &mut a, &vec![c; n], src);
                uniform(Level::new(), &mut b, c, src);
                shaded(Level::new(), &mut s, &vec![c; n], &vec![src.color; n], src);
                assert_eq!(a, b, "{blend:?} at {c}");
                assert_eq!(a, s, "{blend:?} at {c}");
            }
        }
    }

    #[test]
    fn source_over_is_the_porter_duff_formula() {
        let mut d = vec![40, 80, 120, 160];
        mask(Level::new(), &mut d, &[255], solid([100, 50, 0, 128], BlendMode::SrcOver));
        let want: Vec<u8> = [40, 80, 120, 160].iter().zip([100, 50, 0, 128]).map(|(&d, s)| s + mul_div255(d, 127)).collect();
        assert_eq!(d, want);
        let mut d = vec![40, 80, 120, 160];
        mask(Level::new(), &mut d, &[0], solid([100, 50, 0, 128], BlendMode::SrcOver));
        assert_eq!(d, [40, 80, 120, 160], "no coverage, no change");
    }

    #[test]
    fn the_general_modes_agree_with_the_fast_ones_and_the_definitions() {
        // On opaque pixels, the f32 forms of the fast modes land within a step of the integer ones.
        for mode in [BlendMode::SrcOver, BlendMode::Multiply, BlendMode::DstIn] {
            let (s, d) = ([30, 160, 90, 200], [200, 40, 120, 255]);
            let mut fast = d;
            mask(Level::new(), fast.as_mut_slice(), &[255], solid(s, mode));
            let slow = general(s, d, mode, false);
            assert!(fast.iter().zip(slow).all(|(a, b)| a.abs_diff(b) <= 1), "{mode:?}: {fast:?} vs {slow:?}");
        }
        let (white, black, grey) = ([255; 4], [0, 0, 0, 255], [128, 128, 128, 255]);
        assert_eq!(general(white, black, BlendMode::Screen, false), white);
        assert_eq!(general(grey, white, BlendMode::Difference, false), [127, 127, 127, 255]);
        assert_eq!(general(white, black, BlendMode::Xor, false), [0, 0, 0, 0], "both opaque: nothing left");
        assert_eq!(general([0, 0, 0, 0], grey, BlendMode::Src, false), [0, 0, 0, 0]);
        assert_eq!(general([200, 0, 0, 255], [0, 0, 200, 255], BlendMode::Plus, false), [200, 0, 200, 255]);
        // Luminosity keeps the destination's hue and takes the source's lightness: grey on red gives a lighter red.
        let l = general(white, [255, 0, 0, 255], BlendMode::Luminosity, false);
        assert!(l[0] == 255 && l[1] > 0 && l[1] == l[2], "{l:?}");
        // The channel order matters for the non-separable modes.
        let rgb = general([20, 200, 60, 255], [200, 30, 90, 255], BlendMode::Hue, false);
        let bgr = general([60, 200, 20, 255], [90, 30, 200, 255], BlendMode::Hue, true);
        assert_eq!(rgb, [bgr[2], bgr[1], bgr[0], bgr[3]]);
    }
}
