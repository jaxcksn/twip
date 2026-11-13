//! Coverage both backends compute alike: the clip stack (a device rect in 1/256 pixel, the whole pixels it may
//! touch, and a coverage mask when a path clips), a clip applied to a row of coverage, and the rect and rounded
//! rect outlines drawn as paths.

use fearless_simd::Level;

use crate::cpu::blend;
use crate::geom::{Point, Rect, Transform};
use crate::path::Verb;
use crate::raster::{IRect, ONE, Rasterizer, fixed};
use crate::scene::{ClipRec, Lists};

/// The active clip: a device rect in 1/256 pixel (applied analytically, anti-aliased edges included), the whole pixels
/// it may touch, and a coverage mask when a path clips.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Clip {
    pub rect: [i32; 4],
    pub bounds: IRect,
    pub mask: Option<usize>,
}

/// A path clip's coverage over `bounds`, row by row.
#[derive(Debug, Default)]
pub(crate) struct Mask {
    pub bounds: IRect,
    pub data: Vec<u8>,
}

impl Mask {
    /// Row `y` from column `x`, `len` long; the caller keeps them inside the mask's bounds.
    pub fn row(&self, y: i32, x: i32, len: usize) -> &[u8] {
        let w = self.bounds.width() as usize;
        let at = (y - self.bounds.y0) as usize * w + (x - self.bounds.x0) as usize;
        &self.data[at..at + len]
    }
}

/// `clip` narrowed by clip `rec` under transform `t`: an axis-aligned rect stays a rect, anything else becomes a
/// coverage mask over the clip's bounds, from the pool (`masks`, the first `used` taken by open clips).
#[allow(clippy::too_many_arguments, reason = "the rasteriser, the mask pool and the scratch path stay explicit")]
pub(crate) fn push_clip(
    raster: &mut Rasterizer,
    level: Level,
    (masks, used): (&mut Vec<Mask>, &mut usize),
    (verbs, points): (&mut Vec<Verb>, &mut Vec<Point>),
    lists: &Lists,
    rec: &ClipRec,
    t: Transform,
    clip: Clip,
) -> Clip {
    let (verbs, points, aa, even_odd) = match *rec {
        ClipRec::Rect { rect, aa } if t.is_scale_translate() => {
            let r = t.map_rect(rect);
            let q = |v: f32| if aa { fixed(v) } else { fixed((v + 0.5).floor()) };
            let c = clip.rect;
            let rect = [q(r.x0).max(c[0]), q(r.y0).max(c[1]), q(r.x1).min(c[2]), q(r.y1).min(c[3])];
            let bounds = IRect {
                x0: rect[0].div_euclid(ONE),
                y0: rect[1].div_euclid(ONE),
                x1: (rect[2] + ONE - 1).div_euclid(ONE),
                y1: (rect[3] + ONE - 1).div_euclid(ONE),
            };
            return Clip { rect, bounds: clip.bounds.intersect(bounds), mask: clip.mask };
        }
        ClipRec::Rect { rect, aa } => {
            rect_path(rect, verbs, points);
            (&verbs[..], &points[..], aa, false)
        }
        ClipRec::Path { path, even_odd } => (&lists.verbs[path.verbs.range()], &lists.points[path.points.range()], true, even_odd),
    };
    // A coverage mask over the clip's bounds: the path's coverage times the enclosing mask's.
    let index = *used;
    if index == masks.len() {
        masks.push(Mask::default());
    }
    *used += 1;
    let (done, rest) = masks.split_at_mut(index);
    let mask = &mut rest[0];
    let b = clip.bounds;
    mask.bounds = b;
    mask.data.clear();
    mask.data.resize(b.width().max(0) as usize * (b.y1 - b.y0).max(0) as usize, 0);
    let w = b.width() as usize;
    raster.fill(verbs, points, t, b, aa, even_odd, |y, x, cov| {
        let at = (y - b.y0) as usize * w + (x - b.x0) as usize;
        mask.data[at..at + cov.len()].copy_from_slice(cov);
    });
    if let Some(outer) = clip.mask {
        for y in b.y0..b.y1 {
            let at = (y - b.y0) as usize * w;
            blend::scale(level, &mut mask.data[at..at + w], done[outer].row(y, b.x0, w));
        }
    }
    Clip { rect: clip.rect, bounds: b, mask: Some(index) }
}

/// Scales a row of coverage (`cov[i]` for pixel `x + i`, all inside the clip's bounds) by the clip rect's coverage of
/// each pixel and by the clip's mask. Between the rect's edge columns a row is covered alike, fully on all but its top
/// and bottom rows, so only those rows and the two edge columns are scaled.
pub(crate) fn apply_clip(level: Level, clip: &Clip, masks: &[Mask], y: i32, x: i32, cov: &mut [u8]) {
    let [cx0, cy0, cx1, cy1] = clip.rect;
    let cy = overlap(cy0, cy1, y);
    let (left, right) = (cx0.div_euclid(ONE), (cx1 - 1).div_euclid(ONE));
    let at = |col: i32| usize::try_from(col - x).ok().filter(|&i| i < cov.len());
    let (l, r) = (at(left), at(right));
    let k = factor(ONE, cy);
    if k < 255 {
        for (i, c) in cov.iter_mut().enumerate() {
            if Some(i) != l && Some(i) != r {
                *c = crate::paint::mul_div255(*c, k);
            }
        }
    }
    for (i, col) in [(l, left), (r.filter(|_| right != left), right)] {
        let k = factor(overlap(cx0, cx1, col), cy);
        if let Some(c) = i.map(|i| &mut cov[i])
            && k < 255
        {
            *c = crate::paint::mul_div255(*c, k);
        }
    }
    if let Some(m) = clip.mask {
        blend::scale(level, cov, masks[m].row(y, x, cov.len()));
    }
}

/// How much of pixel `p` (its column or row) the span `a..b` (1/256 pixel) covers, 0 to 256.
#[inline(always)]
pub(crate) fn overlap(a: i32, b: i32, p: i32) -> i32 {
    (b.min((p + 1) * ONE) - a.max(p * ONE)).clamp(0, ONE)
}

/// The coverage of a pixel covered `cx` of its width and `cy` of its height (each 0 to 256), 0 to 255.
#[inline(always)]
pub(crate) fn factor(cx: i32, cy: i32) -> u8 {
    cover(cx * cy)
}

/// The coverage of a pixel `area` of which (0 to 65536, in 1/65536 pixel) is inside, 0 to 255: as the rasteriser
/// rounds an area.
#[inline(always)]
pub(crate) fn cover(area: i32) -> u8 {
    ((area as u32 * 255 + 32768) >> 16) as u8
}

/// The coverage of column `col` of a row of a frame: the area inside `outer` (1/256 pixel) less the area inside its
/// hole `inner`, the row covered `oy` by the one and `iy` by the other.
#[inline(always)]
pub(crate) fn frame_cover(outer: [i32; 4], inner: [i32; 4], oy: i32, iy: i32, col: i32) -> u8 {
    cover(overlap(outer[0], outer[2], col) * oy - overlap(inner[0], inner[2], col) * iy)
}

/// The largest factor a 2×2 matrix `[a, b, c, d]` (kurbo's order) stretches a length by: its larger singular value.
pub(crate) fn max_scale([a, b, c, d]: [f64; 4]) -> f64 {
    let sum = a * a + b * b + c * c + d * d;
    let det = a * d - b * c;
    ((sum + (sum * sum - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
}

/// The pixels a clip covers fully: inside its rect's fractional edges and its bounds.
pub(crate) fn interior(clip: &Clip) -> IRect {
    let [x0, y0, x1, y1] = clip.rect;
    IRect { x0: (x0 + ONE - 1).div_euclid(ONE), y0: (y0 + ONE - 1).div_euclid(ONE), x1: x1.div_euclid(ONE), y1: y1.div_euclid(ONE) }
        .intersect(clip.bounds)
}

/// The whole pixels a device rect touches.
pub(crate) fn pixel_bounds(r: Rect) -> IRect {
    let (x0, y0, x1, y1) = (fixed(r.x0), fixed(r.y0), fixed(r.x1), fixed(r.y1));
    IRect { x0: x0.div_euclid(ONE), y0: y0.div_euclid(ONE), x1: (x1 + ONE - 1).div_euclid(ONE), y1: (y1 + ONE - 1).div_euclid(ONE) }
}

/// A rect as a closed path, into `verbs` and `points`.
pub(crate) fn rect_path(r: Rect, verbs: &mut Vec<Verb>, points: &mut Vec<Point>) {
    verbs.clear();
    points.clear();
    verbs.extend([Verb::Move, Verb::Line, Verb::Line, Verb::Line, Verb::Close]);
    points.extend([(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)].map(|(x, y)| Point { x, y }));
}

/// The columns of a frame's row (`width` wide) covered alike, `a..b`, and their coverage: inside the hole's whole
/// columns, the outer rect's cover less the hole's; on a row the hole misses, inside the outer rect's whole columns.
/// The other columns are its edges.
#[inline(always)]
pub(crate) fn frame_middle(outer: [i32; 4], inner: [i32; 4], oy: i32, iy: i32, width: usize) -> (usize, usize, u8) {
    let whole = |a: i32, b: i32| {
        let first = (a + ONE - 1).div_euclid(ONE).clamp(0, width as i32);
        (first as usize, b.div_euclid(ONE).clamp(first, width as i32) as usize)
    };
    let ((a, b), c) = if iy == 0 { (whole(outer[0], outer[2]), factor(ONE, oy)) } else { (whole(inner[0], inner[2]), factor(ONE, oy - iy)) };
    (a, b, c)
}

/// A frame as a closed path: `outer` one way round, its hole `inner` the other.
pub(crate) fn frame_path(outer: Rect, inner: Rect, verbs: &mut Vec<Verb>, points: &mut Vec<Point>) {
    rect_path(outer, verbs, points);
    verbs.extend([Verb::Move, Verb::Line, Verb::Line, Verb::Line, Verb::Close]);
    points.extend([(inner.x0, inner.y0), (inner.x0, inner.y1), (inner.x1, inner.y1), (inner.x1, inner.y0)].map(|(x, y)| Point { x, y }));
}

/// A rounded rect as a closed path of lines and quarter-circle cubics, into `verbs` and `points`. `radii` are top
/// left, top right, bottom right, bottom left, already fitted to the rect.
pub(crate) fn rrect_path(r: Rect, [tl, tr, br, bl]: [f32; 4], verbs: &mut Vec<Verb>, points: &mut Vec<Point>) {
    // The cubic control distance that best approximates a quarter circle, as a fraction of the radius.
    const KAPPA: f32 = 0.552_284_8;
    verbs.clear();
    points.clear();
    let p = |x: f32, y: f32| Point { x, y };
    verbs.push(Verb::Move);
    points.push(p(r.x0 + tl, r.y0));
    let mut corner = |from: Point, c0: Point, c1: Point, to: Point| {
        verbs.extend([Verb::Line, Verb::Cubic]);
        points.extend([from, c0, c1, to]);
    };
    corner(p(r.x1 - tr, r.y0), p(r.x1 - tr + KAPPA * tr, r.y0), p(r.x1, r.y0 + tr - KAPPA * tr), p(r.x1, r.y0 + tr));
    corner(p(r.x1, r.y1 - br), p(r.x1, r.y1 - br + KAPPA * br), p(r.x1 - br + KAPPA * br, r.y1), p(r.x1 - br, r.y1));
    corner(p(r.x0 + bl, r.y1), p(r.x0 + bl - KAPPA * bl, r.y1), p(r.x0, r.y1 - bl + KAPPA * bl), p(r.x0, r.y1 - bl));
    corner(p(r.x0, r.y0 + tl), p(r.x0, r.y0 + tl - KAPPA * tl), p(r.x0 + tl - KAPPA * tl, r.y0), p(r.x0 + tl, r.y0));
    verbs.push(Verb::Close);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipping_a_row_scales_each_pixel_by_its_clip_cover() {
        // Every pixel by the formula, for clip rects with whole and fractional edges, one to nine pixels wide: every row
        // of the clip's pixels, from each of its columns to its right edge.
        let rects = [[256, 256, 2560, 2560], [300, 290, 2000, 1999], [300, 290, 400, 700], [300, 290, 500, 700], [0, 0, 256, 256]];
        for rect in rects {
            let bounds = IRect { x0: rect[0] / ONE, y0: rect[1] / ONE, x1: (rect[2] + ONE - 1) / ONE, y1: (rect[3] + ONE - 1) / ONE };
            let clip = Clip { rect, bounds, mask: None };
            for y in bounds.y0..bounds.y1 {
                for x in bounds.x0..bounds.x1 {
                    let mut cov: Vec<u8> = (0..bounds.x1 - x).map(|i| 255 - i as u8 * 13).collect();
                    let want: Vec<u8> = cov
                        .iter()
                        .enumerate()
                        .map(|(i, &c)| crate::paint::mul_div255(c, factor(overlap(rect[0], rect[2], x + i as i32), overlap(rect[1], rect[3], y))))
                        .collect();
                    apply_clip(Level::new(), &clip, &[], y, x, &mut cov);
                    assert_eq!(cov, want, "{rect:?} at {x}, {y}");
                }
            }
        }
    }
}
