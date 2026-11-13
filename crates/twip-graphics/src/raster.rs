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
//!   don't overlap, and right for holes and same-direction overlaps away from anti-aliased edges. The even-odd rule
//!   folds the sum instead: `|sum|` modulo 2, taken back down past 1 (as ab_glyph_rasterizer does). Integer sums don't
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
/// The most cells a mask accumulates whole (1 MB): a 256-pixel em's glyph needs about a tenth of it.
const MASK_CELLS: usize = 1 << 18;
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
    /// For a mask: the columns each row's pieces touched.
    touched: Vec<(usize, usize)>,
}

impl Rasterizer {
    pub fn new(level: Level) -> Rasterizer {
        Rasterizer { level, lines: Vec::new(), active: Vec::new(), acc: Vec::new(), row: Vec::new(), touched: Vec::new() }
    }

    /// Fills the path (`verbs` over `points`, in local space) mapped by `transform`, inside `clip`, with the non-zero
    /// rule, or the even-odd one. Calls `row(y, x, coverage)` for each pixel row with coverage, `coverage[i]` being pixel
    /// `x + i`'s (0 to 255). Without `aa`, a pixel is covered (255) when at least half of it is inside, else not.
    #[allow(clippy::too_many_arguments, reason = "a fill's path, mapping, area and rules")]
    pub fn fill(
        &mut self,
        verbs: &[Verb],
        points: &[Point],
        transform: Transform,
        clip: IRect,
        aa: bool,
        even_odd: bool,
        mut row: impl FnMut(i32, i32, &mut [u8]),
    ) {
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
                dispatch!(self.level, simd => coverage(simd, acc, cov, aa, even_odd));
                row(top + r as i32, area.x0 + lo as i32, cov);
            }
            top += rows;
        }
    }
}

impl Rasterizer {
    /// A glyph's coverage mask: the non-zero, anti-aliased fill of the path (in device pixels) over `area`, into `out`
    /// (`area`'s pixels row by row, zeroed), each pixel through `table`. The same coverage as [`Rasterizer::fill`],
    /// accumulated over the whole mask at once and swept in one call: a glyph is a few rows, where the bands, a call
    /// per row and the lines' sort cost more than the coverage (a quarter of a small glyph's time). A mask too large
    /// to accumulate whole (a broken outline's) is filled band by band.
    pub fn mask(&mut self, verbs: &[Verb], points: &[Point], area: IRect, out: &mut [u8], table: &[u8; 256]) {
        let (w, h) = (area.width().max(0) as usize, (area.y1 - area.y0).max(0) as usize);
        let stride = w + 2;
        if stride * h > MASK_CELLS {
            return self.fill(verbs, points, Transform::IDENTITY, area, true, false, |y, x, cov| {
                let at = (y - area.y0) as usize * w + (x - area.x0) as usize;
                for (d, &c) in out[at..at + cov.len()].iter_mut().zip(cov.iter()) {
                    *d = table[c as usize];
                }
            });
        }
        self.lines.clear();
        flatten(verbs, points, Transform::IDENTITY, &mut self.lines);
        if w == 0 || h == 0 {
            return;
        }
        self.acc.clear();
        self.acc.resize(stride * h, 0);
        self.touched.clear();
        self.touched.resize(h, (usize::MAX, 0));
        let (left, right) = (area.x0 * ONE, area.x1 * ONE);
        for &l in &self.lines {
            accumulate(l, area.y0, h as i32, left, right, stride, &mut self.acc, &mut self.touched);
        }
        let (acc, touched) = (&self.acc[..], &self.touched[..]);
        dispatch!(self.level, simd => mask_coverage(simd, acc, stride, touched, out, w, table));
    }
}

/// Each row's prefix sum, as anti-aliased non-zero coverage through `table`, into `out` (`w` wide).
#[simd]
fn mask_coverage<S: Simd>(simd: S, acc: &[i32], stride: usize, touched: &[(usize, usize)], out: &mut [u8], w: usize, table: &[u8; 256]) {
    for (r, &(lo, hi)) in touched.iter().enumerate() {
        if lo > hi {
            continue;
        }
        let mut sum = 0i32;
        let row = &acc[r * stride + lo..=r * stride + hi];
        for (o, a) in out[r * w + lo..r * w + (hi + 1).min(w)].iter_mut().zip(row) {
            sum += *a;
            *o = table[((sum.unsigned_abs().min(FULL as u32) * 255 + (FULL as u32 / 2)) >> 16) as usize];
        }
    }
}

/// Appends the path's segments in device space, each subpath closed, curves flattened to `TOLERANCE`.
fn flatten(verbs: &[Verb], points: &[Point], t: Transform, lines: &mut Vec<Line>) {
    // A glyph mask's outline is already in pixels: the identity leaves its points as they are.
    let identity = t == Transform::IDENTITY;
    let mut points = points.iter().map(|&p| if identity { p } else { t.map_point(p) });
    let mut next = || points.next().unwrap_or_default();
    let (mut start, mut last) = (Point::default(), Point::default());
    let mut pen = Pen { lines, start: (0, 0), last: (0, 0) };
    for verb in verbs {
        match verb {
            Verb::Move => {
                pen.close();
                start = next();
                last = start;
                pen.move_to(start);
            }
            Verb::Line => {
                let p = next();
                pen.line_to(p);
                last = p;
            }
            Verb::Quad => {
                let (c, p) = (next(), next());
                // A bend under a third of a pixel needs one line (most of a small glyph's curves): `segments` gives
                // 1 for any bend up to 0.4, so the threshold leaves every count as it was.
                if bend_squared(last, c, p) <= 0.15 {
                    pen.line_to(p);
                    last = p;
                    continue;
                }
                let n = segments(0.25 * second_difference(last, c, p));
                let p0 = last;
                for i in 1..=n {
                    let s = i as f32 / n as f32;
                    let u = 1.0 - s;
                    let q = Point { x: u * u * p0.x + 2.0 * u * s * c.x + s * s * p.x, y: u * u * p0.y + 2.0 * u * s * c.y + s * s * p.y };
                    pen.line_to(q);
                    last = q;
                }
            }
            Verb::Cubic => {
                let (c0, c1, p) = (next(), next(), next());
                // As for quads: `segments` gives 1 for any bend up to 0.4 / 3.
                if bend_squared(last, c0, c1).max(bend_squared(c0, c1, p)) <= 0.0175 {
                    pen.line_to(p);
                    last = p;
                    continue;
                }
                let n = segments(0.75 * second_difference(last, c0, c1).max(second_difference(c0, c1, p)));
                let p0 = last;
                for i in 1..=n {
                    let s = i as f32 / n as f32;
                    let u = 1.0 - s;
                    let (a, b, c, d) = (u * u * u, 3.0 * u * u * s, 3.0 * u * s * s, s * s * s);
                    let q = Point { x: a * p0.x + b * c0.x + c * c1.x + d * p.x, y: a * p0.y + b * c0.y + c * c1.y + d * p.y };
                    pen.line_to(q);
                    last = q;
                }
            }
            Verb::Close => {
                pen.close();
                last = start;
            }
        }
    }
    pen.close();
}

/// `|a - 2b + c|²`, without its square root.
fn bend_squared(a: Point, b: Point, c: Point) -> f32 {
    let (x, y) = (a.x - 2.0 * b.x + c.x, a.y - 2.0 * b.y + c.y);
    x * x + y * y
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

/// Lines appended in fixed point, each point converted once.
struct Pen<'a> {
    lines: &'a mut Vec<Line>,
    start: (i32, i32),
    last: (i32, i32),
}

impl Pen<'_> {
    fn move_to(&mut self, p: Point) {
        self.start = (fixed(p.x), fixed(p.y));
        self.last = self.start;
    }

    fn line_to(&mut self, p: Point) {
        let to = (fixed(p.x), fixed(p.y));
        self.push(to);
    }

    /// Back to the subpath's start.
    fn close(&mut self) {
        self.push(self.start);
    }

    fn push(&mut self, to: (i32, i32)) {
        let ((x0, y0), (x1, y1)) = (self.last, to);
        if y0 != y1 {
            self.lines.push(Line { x0, y0, x1, y1 });
        }
        self.last = to;
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
    // A line inside the area (a glyph's always are) needs no clamp.
    let inside = x0.min(x1) >= left && x0.max(x1) <= right;
    let x_at = |y: i32| {
        let x = x0 as f64 + (y - y0) as f64 * dxdy;
        (if inside { x } else { x.clamp(left as f64, right as f64) }) - left as f64
    };
    let (mut y, mut xa) = (ys, x_at(ys) / ONE as f64);
    while y < ye {
        let row_top = y.div_euclid(ONE) * ONE;
        let y_next = (row_top + ONE).min(ye);
        let r = (row_top - band_top) / ONE;
        let base = r as usize * stride;
        let xb = x_at(y_next) / ONE as f64;
        let (lo, hi) = add_row(&mut acc[base..base + stride], xa, xb, dir * (y_next - y) * ONE);
        xa = xb;
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
fn coverage<S: Simd>(simd: S, acc: &mut [i32], out: &mut [u8], aa: bool, even_odd: bool) {
    let mut sum = 0i32;
    for (i, a) in acc.iter_mut().enumerate() {
        sum += *a;
        *a = 0;
        if let Some(o) = out.get_mut(i) {
            let v = if even_odd {
                let m = sum.unsigned_abs() % (2 * FULL as u32);
                if m > FULL as u32 { 2 * FULL as u32 - m } else { m }
            } else {
                sum.unsigned_abs().min(FULL as u32)
            };
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
        raster_rule(els, w, h, aa, false)
    }

    fn raster_rule(els: &[kurbo::PathEl], w: i32, h: i32, aa: bool, even_odd: bool) -> Vec<u8> {
        let (mut verbs, mut points) = (Vec::new(), Vec::new());
        crate::path::push(els, &mut verbs, &mut points);
        let identity = Transform::from_kurbo(kurbo::Affine::IDENTITY);
        let mut out = vec![0u8; (w * h) as usize];
        let mut r = Rasterizer::new(Level::new());
        r.fill(&verbs, &points, identity, IRect { x0: 0, y0: 0, x1: w, y1: h }, aa, even_odd, |y, x, cov| {
            let at = (y * w + x) as usize;
            out[at..at + cov.len()].copy_from_slice(cov);
        });
        out
    }

    #[test]
    fn a_mask_is_the_fill_through_its_table() {
        let table: [u8; 256] = std::array::from_fn(|i| (255 - i) as u8);
        let mut star = kurbo::BezPath::new();
        for k in 0..5 {
            let a = std::f64::consts::PI * (0.5 + 0.8 * k as f64);
            let p = (20.3 + 15.0 * a.cos(), 18.7 - 15.0 * a.sin());
            if k == 0 { star.move_to(p) } else { star.line_to(p) }
        }
        star.close_path();
        let small = [kurbo::Circle::new((9.3, 7.6), 6.2).to_path(0.01), star];
        // Wide enough to be filled band by band.
        let large = kurbo::Rect::new(0.5, 0.25, 1100.75, 260.5).to_path(0.1);
        let mut r = Rasterizer::new(Level::new());
        for path in small.iter().chain([&large]) {
            let (mut verbs, mut points) = (Vec::new(), Vec::new());
            let b = crate::path::push(path.elements(), &mut verbs, &mut points).expect("points");
            let area = IRect { x0: b.x0.floor() as i32, y0: b.y0.floor() as i32, x1: b.x1.ceil() as i32, y1: b.y1.ceil() as i32 };
            let w = area.width() as usize;
            let mut want = vec![0; w * (area.y1 - area.y0) as usize];
            r.fill(&verbs, &points, Transform::IDENTITY, area, true, false, |y, x, cov| {
                let at = (y - area.y0) as usize * w + (x - area.x0) as usize;
                for (d, &c) in want[at..at + cov.len()].iter_mut().zip(cov.iter()) {
                    *d = table[c as usize];
                }
            });
            let mut got = vec![0; want.len()];
            r.mask(&verbs, &points, area, &mut got, &table);
            assert!(got == want, "{area:?}");
        }
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

    #[test]
    fn even_odd_leaves_every_other_nested_contour_empty() {
        // Three squares, nested, all wound the same way: non-zero covers all of them; even-odd the outer ring and the
        // innermost square.
        let mut p = kurbo::BezPath::new();
        for (a, b) in [(0.0, 12.0), (3.0, 9.0), (5.0, 7.0)] {
            p.extend(kurbo::Rect::new(a, a, b, b).path_elements(0.1));
        }
        let (nonzero, evenodd) = (raster(p.elements(), 12, 12, true), raster_rule(p.elements(), 12, 12, true, true));
        let at = |c: &[u8], x: usize, y: usize| c[y * 12 + x];
        for (x, y) in [(1, 1), (4, 4), (6, 6)] {
            assert_eq!(at(&nonzero, x, y), 255, "non-zero at {x},{y}");
        }
        assert_eq!((at(&evenodd, 1, 1), at(&evenodd, 4, 4), at(&evenodd, 6, 6)), (255, 0, 255));
    }
}
