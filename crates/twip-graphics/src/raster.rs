//! Coverage: turns a filled path under a device transform into anti-aliased coverage rows, by banded signed-area
//! accumulation (the font-rs and ab_glyph_rasterizer method, in integer fixed point). One rasteriser serves every
//! consumer: the CPU compositor, glyph masks, and the GPU backend's path and clip masks.
//!
//! - Device positions are quantised to 1/256 pixel. Rows split at whole pixels, so every piece of a line has an exact
//!   integer height, and a closed path's pieces on a row sum to exactly zero: no coverage leaks past its right edge.
//! - Each piece's signed area is split over the pixels it crosses (in `f64`, in a fixed order) and added to an `i32`
//!   buffer in 1/65536 of a pixel, with the last share taking whatever rounding left, so a piece adds exactly its
//!   height.
//! - A row's prefix sum is the winding-weighted coverage. `min(|sum|, 1)` is the non-zero rule: exact for contours that
//!   don't overlap, and right for holes and same-direction overlaps away from anti-aliased edges. Integer sums don't
//!   depend on the order of addition, so any SIMD level gives the same bytes.

use fearless_simd::{Level, Simd, dispatch};
use fearless_simd_macros::simd;

use crate::geom::{Point, Transform};
use crate::path::Verb;

/// Subpixel steps per pixel.
pub(crate) const ONE: i32 = 256;
/// A fully covered pixel in the accumulation buffer.
const FULL: i32 = ONE * ONE;
/// Rows accumulated at a time: the buffer is `(width + 2) × BAND`.
const BAND: i32 = 16;
/// How far a flattened curve may stray from the true one, in device pixels.
const TOLERANCE: f32 = 0.1;
/// The most line segments one curve becomes, so absurd coordinates cost bounded time.
const MAX_SEGMENTS: u32 = 1024;
/// Device coordinates are clamped to ±2^20 pixels, so fixed-point differences fit in an `i32`.
const LIMIT: f32 = (1 << 20) as f32;

/// A device rect of whole pixels: `x0..x1` × `y0..y1`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct IRect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl IRect {
    pub fn is_empty(self) -> bool {
        self.x0 >= self.x1 || self.y0 >= self.y1
    }

    pub fn intersect(self, o: IRect) -> IRect {
        IRect { x0: self.x0.max(o.x0), y0: self.y0.max(o.y0), x1: self.x1.min(o.x1), y1: self.y1.min(o.y1) }
    }

    pub fn width(self) -> i32 {
        self.x1 - self.x0
    }
}

/// A device position in 1/256 pixel.
pub(crate) fn fixed(v: f32) -> i32 {
    (v.clamp(-LIMIT, LIMIT) * ONE as f32).round() as i32
}

/// A line segment in fixed point, never horizontal.
#[derive(Clone, Copy, Debug)]
struct Line {
    x0: i32,
    y0: i32,
    x1: i32,
    y1: i32,
}

/// The rasteriser: its SIMD level and its scratch memory, kept between fills.
#[derive(Debug)]
pub(crate) struct Rasterizer {
    level: Level,
    /// The path's lines, sorted by their top.
    lines: Vec<Line>,
    /// The lines crossing the current band, as indexes into `lines`.
    active: Vec<u32>,
    acc: Vec<i32>,
    row: Vec<u8>,
}

impl Rasterizer {
    pub fn new(level: Level) -> Rasterizer {
        Rasterizer { level, lines: Vec::new(), active: Vec::new(), acc: Vec::new(), row: Vec::new() }
    }

    /// Fills the path (`verbs` over `points`, in local space) mapped by `transform`, inside `clip`, with the non-zero
    /// rule. Calls `row(y, x, coverage)` for each pixel row with coverage, `coverage[i]` being pixel `x + i`'s (0 to 255).
    /// Without `aa`, a pixel is covered (255) when at least half of it is inside, else not.
    pub fn fill(&mut self, verbs: &[Verb], points: &[Point], transform: Transform, clip: IRect, aa: bool, mut row: impl FnMut(i32, i32, &mut [u8])) {
        self.lines.clear();
        flatten(verbs, points, transform, &mut self.lines);
        let Some(bounds) = bounds(&self.lines) else { return };
        let area = bounds.intersect(clip);
        if area.is_empty() {
            return;
        }
        let w = area.width() as usize;
        let stride = w + 2;
        self.acc.clear();
        self.acc.resize(stride * BAND as usize, 0);
        self.row.resize(w, 0);
        let (left, right) = (area.x0 * ONE, area.x1 * ONE);
        // Each band visits only the lines crossing it: lines join the active list as the bands reach their top and
        // leave it once the bands pass their bottom.
        self.lines.sort_unstable_by_key(|l| l.y0.min(l.y1));
        self.active.clear();
        let mut next = 0;
        let mut top = area.y0;
        while top < area.y1 {
            let rows = BAND.min(area.y1 - top);
            let bottom = (top + rows) * ONE;
            while next < self.lines.len() && self.lines[next].y0.min(self.lines[next].y1) < bottom {
                self.active.push(next as u32);
                next += 1;
            }
            // The columns each row's pieces touched, relative to `area.x0`.
            let mut touched = [(usize::MAX, 0usize); BAND as usize];
            for &i in &self.active {
                accumulate(self.lines[i as usize], top, rows, left, right, stride, &mut self.acc, &mut touched);
            }
            let lines = &self.lines;
            self.active.retain(|&i| lines[i as usize].y0.max(lines[i as usize].y1) > bottom);
            for (r, &(lo, hi)) in touched[..rows as usize].iter().enumerate() {
                if lo > hi {
                    continue;
                }
                let acc = &mut self.acc[r * stride + lo..=r * stride + hi];
                let cov = &mut self.row[lo..=hi.min(w - 1)];
                dispatch!(self.level, simd => coverage(simd, acc, cov, aa));
                row(top + r as i32, area.x0 + lo as i32, cov);
            }
            top += rows;
        }
    }
}

/// Appends the path's segments in device space, each subpath closed, curves flattened to `TOLERANCE`.
fn flatten(verbs: &[Verb], points: &[Point], t: Transform, lines: &mut Vec<Line>) {
    let mut points = points.iter().map(|&p| t.map_point(p));
    let mut next = || points.next().unwrap_or_default();
    let (mut start, mut last) = (Point::default(), Point::default());
    for verb in verbs {
        match verb {
            Verb::Move => {
                push(lines, last, start);
                start = next();
                last = start;
            }
            Verb::Line => {
                let p = next();
                push(lines, last, p);
                last = p;
            }
            Verb::Quad => {
                let (c, p) = (next(), next());
                let n = segments(0.25 * second_difference(last, c, p));
                let p0 = last;
                for i in 1..=n {
                    let s = i as f32 / n as f32;
                    let u = 1.0 - s;
                    let q = Point { x: u * u * p0.x + 2.0 * u * s * c.x + s * s * p.x, y: u * u * p0.y + 2.0 * u * s * c.y + s * s * p.y };
                    push(lines, last, q);
                    last = q;
                }
            }
            Verb::Cubic => {
                let (c0, c1, p) = (next(), next(), next());
                let n = segments(0.75 * second_difference(last, c0, c1).max(second_difference(c0, c1, p)));
                let p0 = last;
                for i in 1..=n {
                    let s = i as f32 / n as f32;
                    let u = 1.0 - s;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * s, 3.0 * u * s * s, s * s * s);
                    let q = Point { x: a * p0.x + b * c0.x + c * c1.x + d * p.x, y: a * p0.y + b * c0.y + c * c1.y + d * p.y };
                    push(lines, last, q);
                    last = q;
                }
            }
            Verb::Close => {
                push(lines, last, start);
                last = start;
            }
        }
    }
    push(lines, last, start);
}

/// `|a - 2b + c|`, the size of a curve's bend.
fn second_difference(a: Point, b: Point, c: Point) -> f32 {
    (a.x - 2.0 * b.x + c.x).hypot(a.y - 2.0 * b.y + c.y)
}

/// How many lines a curve with bend measure `m` needs to stay within `TOLERANCE` (Wang's formula).
fn segments(m: f32) -> u32 {
    let n = (m / TOLERANCE).sqrt().ceil();
    if n.is_finite() { (n as u32).clamp(1, MAX_SEGMENTS) } else { 1 }
}

fn push(lines: &mut Vec<Line>, a: Point, b: Point) {
    let (x0, y0, x1, y1) = (fixed(a.x), fixed(a.y), fixed(b.x), fixed(b.y));
    if y0 != y1 {
        lines.push(Line { x0, y0, x1, y1 });
    }
}

/// The whole pixels the lines touch.
fn bounds(lines: &[Line]) -> Option<IRect> {
    let first = lines.first()?;
    let mut b = IRect { x0: first.x0.min(first.x1), y0: first.y0.min(first.y1), x1: first.x0.max(first.x1), y1: first.y0.max(first.y1) };
    for l in &lines[1..] {
        b = IRect { x0: b.x0.min(l.x0.min(l.x1)), y0: b.y0.min(l.y0.min(l.y1)), x1: b.x1.max(l.x0.max(l.x1)), y1: b.y1.max(l.y0.max(l.y1)) };
    }
    Some(IRect { x0: b.x0.div_euclid(ONE), y0: b.y0.div_euclid(ONE), x1: (b.x1 + ONE - 1).div_euclid(ONE), y1: (b.y1 + ONE - 1).div_euclid(ONE) })
}

/// Adds `line`'s signed area on the band of `rows` rows starting at pixel row `top`, its x clamped to `left..=right`
/// (fixed point): a line left of the area still adds its winding at the first column.
#[allow(clippy::too_many_arguments, reason = "the band's state stays explicit")]
fn accumulate(line: Line, top: i32, rows: i32, left: i32, right: i32, stride: usize, acc: &mut [i32], touched: &mut [(usize, usize)]) {
    let (dir, Line { x0, y0, x1, y1 }) =
        if line.y0 < line.y1 { (1, line) } else { (-1, Line { x0: line.x1, y0: line.y1, x1: line.x0, y1: line.y0 }) };
    let (band_top, band_bottom) = (top * ONE, (top + rows) * ONE);
    let (ys, ye) = (y0.max(band_top), y1.min(band_bottom));
    if ys >= ye {
        return;
    }
    let dxdy = (x1 - x0) as f64 / (y1 - y0) as f64;
    let x_at = |y: i32| (x0 as f64 + (y - y0) as f64 * dxdy).clamp(left as f64, right as f64) - left as f64;
    let mut y = ys;
    while y < ye {
        let row_top = y.div_euclid(ONE) * ONE;
        let y_next = (row_top + ONE).min(ye);
        let r = (row_top - band_top) / ONE;
        let base = r as usize * stride;
        let (lo, hi) = add_row(&mut acc[base..base + stride], x_at(y) / ONE as f64, x_at(y_next) / ONE as f64, dir * (y_next - y) * ONE);
        let t = &mut touched[r as usize];
        *t = (t.0.min(lo), t.1.max(hi));
        y = y_next;
    }
}

/// Splits a piece crossing a row from `xa` to `xb` (pixels, relative to the row's first column, `>= 0`) whose signed
/// height is `total` (in 1/65536 of a pixel) over the columns it crosses. Returns the first and last column touched.
fn add_row(row: &mut [i32], xa: f64, xb: f64, total: i32) -> (usize, usize) {
    let (x0, x1) = if xa < xb { (xa, xb) } else { (xb, xa) };
    let d = total as f64;
    let x0f = x0.floor();
    let x0i = x0f as usize;
    let x1c = x1.ceil();
    let x1i = x1c as usize;
    if x1i <= x0i + 1 {
        // Within one column: the area right of the piece's midpoint goes to the next column.
        let xm = 0.5 * (xa + xb) - x0f;
        let here = (d * (1.0 - xm)).round() as i32;
        row[x0i] += here;
        row[x0i + 1] += total - here;
        return (x0i, x0i + 1);
    }
    let s = 1.0 / (x1 - x0);
    let fx0 = x0 - x0f;
    let a0 = 0.5 * s * (1.0 - fx0) * (1.0 - fx0);
    let fx1 = x1 - x1c + 1.0;
    let am = 0.5 * s * fx1 * fx1;
    let mut added = 0;
    let mut add = |i: usize, share: f64| {
        let v = (d * share).round() as i32;
        row[i] += v;
        added += v;
    };
    add(x0i, a0);
    if x1i == x0i + 2 {
        add(x0i + 1, 1.0 - a0 - am);
    } else {
        let a1 = s * (1.5 - fx0);
        add(x0i + 1, a1 - a0);
        for i in x0i + 2..x1i - 1 {
            add(i, s);
        }
        let a2 = a1 + (x1i - x0i - 3) as f64 * s;
        add(x1i - 1, 1.0 - a2 - am);
    }
    row[x1i] += total - added;
    (x0i, x1i)
}

/// A row's prefix sum, as coverage: `acc` covers the touched columns (and is zeroed for the next band), `out` the ones
/// inside the area.
#[simd]
fn coverage<S: Simd>(simd: S, acc: &mut [i32], out: &mut [u8], aa: bool) {
    let mut sum = 0i32;
    for (i, a) in acc.iter_mut().enumerate() {
        sum += *a;
        *a = 0;
        if let Some(o) = out.get_mut(i) {
            let v = sum.unsigned_abs().min(FULL as u32);
            *o = if aa {
                ((v * 255 + (FULL as u32 / 2)) >> 16) as u8
            } else if 2 * v >= FULL as u32 {
                255
            } else {
                0
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    /// The path's coverage on a `w × h` grid, identity transform.
    fn raster(els: &[kurbo::PathEl], w: i32, h: i32, aa: bool) -> Vec<u8> {
        let (mut verbs, mut points) = (Vec::new(), Vec::new());
        crate::path::push(els, &mut verbs, &mut points);
        let identity = Transform::from_kurbo(kurbo::Affine::IDENTITY);
        let mut out = vec![0u8; (w * h) as usize];
        let mut r = Rasterizer::new(Level::new());
        r.fill(&verbs, &points, identity, IRect { x0: 0, y0: 0, x1: w, y1: h }, aa, |y, x, cov| {
            let at = (y * w + x) as usize;
            out[at..at + cov.len()].copy_from_slice(cov);
        });
        out
    }

    #[test]
    fn a_rect_covers_its_pixels_by_area() {
        let c = raster(kurbo::Rect::new(0.5, 1.0, 3.25, 2.5).to_path(0.1).elements(), 4, 3, true);
        // Row 1 is fully inside vertically; row 2 half. Columns: 0.5, 1, 1, 0.25.
        assert_eq!(&c[4..8], &[128, 255, 255, 64]);
        assert_eq!(&c[8..12], &[64, 128, 128, 32]);
        assert_eq!(&c[0..4], &[0; 4]);
    }

    #[test]
    fn holes_and_overlaps_follow_the_non_zero_rule() {
        let outer = kurbo::Rect::new(0.0, 0.0, 6.0, 6.0).to_path(0.1);
        // Opposite winding: a hole.
        let mut hole = outer.clone();
        hole.extend(kurbo::Rect::new(4.0, 2.0, 2.0, 4.0).to_path(0.1));
        let c = raster(hole.elements(), 6, 6, true);
        assert_eq!((c[3 * 6 + 3], c[3 * 6 + 1]), (0, 255));
        // Same winding: still covered once.
        let mut twice = outer.clone();
        twice.extend(kurbo::Rect::new(2.0, 2.0, 4.0, 4.0).to_path(0.1));
        assert_eq!(raster(twice.elements(), 6, 6, true)[3 * 6 + 3], 255);
    }

    #[test]
    fn nothing_leaks_right_of_a_long_thin_sliver() {
        let sliver = [
            kurbo::PathEl::MoveTo((0.3, 0.1).into()),
            kurbo::PathEl::LineTo((60.7, 7.9).into()),
            kurbo::PathEl::LineTo((60.9, 7.95).into()),
            kurbo::PathEl::ClosePath,
        ];
        let c = raster(&sliver, 64, 8, true);
        for y in 0..8 {
            let last_ink = (0..64).rev().find(|&x| c[y * 64 + x] > 0).unwrap_or(0);
            assert!(last_ink <= 61, "row {y} has coverage at {last_ink}");
        }
    }

    #[test]
    fn a_circle_has_the_right_area() {
        let c = raster(kurbo::Circle::new((32.0, 32.0), 20.0).to_path(0.01).elements(), 64, 64, true);
        let area: f64 = c.iter().map(|&v| v as f64 / 255.0).sum();
        let want = std::f64::consts::PI * 400.0;
        // Flattening stays within the tolerance inside the curve, so the area may fall short by up to the perimeter
        // times the tolerance.
        let slack = 2.0 * std::f64::consts::PI * 20.0 * TOLERANCE as f64;
        assert!(area <= want + 0.5 && want - area < slack, "{area} vs {want}");
    }

    #[test]
    fn aliased_fills_are_all_or_nothing() {
        let c = raster(kurbo::Circle::new((8.0, 8.0), 6.0).to_path(0.01).elements(), 16, 16, false);
        assert!(c.iter().all(|&v| v == 0 || v == 255));
        assert_eq!(c[8 * 16 + 8], 255);
    }

    #[test]
    fn absurd_coordinates_do_not_panic() {
        let wild = [
            kurbo::PathEl::MoveTo((-1e30, 5.0).into()),
            kurbo::PathEl::CurveTo((f64::NAN, 1e30).into(), (3.0, f64::INFINITY).into(), (1e12, -1e12).into()),
            kurbo::PathEl::LineTo((4.0, 4.0).into()),
        ];
        raster(&wild, 16, 16, true);
    }
}
