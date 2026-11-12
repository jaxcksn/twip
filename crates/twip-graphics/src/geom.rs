//! Geometry as pictures store it: points, rects and 2×3 affine transforms in `f32`.
//!
//! The recording API takes kurbo's `f64` types, the ones Twip's layout works in, and narrows them here once. Pictures are
//! in local units (Twip's page points), y down. A backend maps them to device pixels with the frame's transform; pixel
//! centres sit at `.5`. A page is at most 22 inches (1584 pt), so `f32` keeps well under a hundredth of a device pixel
//! at any zoom Twip offers. A page's place in a scrolled document never enters a picture: it is a frame transform.

/// A point in a picture's local space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Point {
    pub x: f32,
    pub y: f32,
}

impl From<kurbo::Point> for Point {
    fn from(p: kurbo::Point) -> Point {
        Point { x: p.x as f32, y: p.y as f32 }
    }
}

/// An axis-aligned rect with `x0 <= x1` and `y0 <= y1`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Rect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl Rect {
    /// Narrows `r`, ordering its edges (kurbo allows a rect with `x1 < x0`).
    pub fn from_kurbo(r: kurbo::Rect) -> Rect {
        let r = r.abs();
        Rect { x0: r.x0 as f32, y0: r.y0 as f32, x1: r.x1 as f32, y1: r.y1 as f32 }
    }

    pub fn to_kurbo(self) -> kurbo::Rect {
        kurbo::Rect::new(self.x0 as f64, self.y0 as f64, self.x1 as f64, self.y1 as f64)
    }
}

/// A 2×3 affine transform: `x' = a·x + c·y + e`, `y' = b·x + d·y + f` (kurbo's coefficient order).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Transform {
    pub a: f32,
    pub b: f32,
    pub c: f32,
    pub d: f32,
    pub e: f32,
    pub f: f32,
}

impl Transform {
    pub const IDENTITY: Transform = Transform { a: 1.0, b: 0.0, c: 0.0, d: 1.0, e: 0.0, f: 0.0 };

    pub fn from_kurbo(t: kurbo::Affine) -> Transform {
        let [a, b, c, d, e, f] = t.as_coeffs().map(|v| v as f32);
        Transform { a, b, c, d, e, f }
    }

    /// `self` applied first, then `next`. Backends compose a nested picture's transform with this, so a picture drawn
    /// nested and the same commands drawn inline under the same transforms map their points alike.
    pub fn then(self, next: Transform) -> Transform {
        Transform {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            e: next.a * self.e + next.c * self.f + next.e,
            f: next.b * self.e + next.d * self.f + next.f,
        }
    }

    /// The inverse, or `None` for a transform that collapses the plane (or isn't finite).
    pub fn inverse(self) -> Option<Transform> {
        let det = self.a * self.d - self.b * self.c;
        if det == 0.0 || !det.is_finite() {
            return None;
        }
        let k = 1.0 / det;
        Some(Transform {
            a: self.d * k,
            b: -self.b * k,
            c: -self.c * k,
            d: self.a * k,
            e: (self.c * self.f - self.d * self.e) * k,
            f: (self.b * self.e - self.a * self.f) * k,
        })
    }

    pub fn map_point(self, p: Point) -> Point {
        Point { x: self.a * p.x + self.c * p.y + self.e, y: self.b * p.x + self.d * p.y + self.f }
    }

    /// The bounding box of `r`'s four mapped corners.
    pub fn map_rect(self, r: Rect) -> Rect {
        let corners = [(r.x0, r.y0), (r.x1, r.y0), (r.x0, r.y1), (r.x1, r.y1)].map(|(x, y)| self.map_point(Point { x, y }));
        let empty = Rect { x0: f32::INFINITY, y0: f32::INFINITY, x1: f32::NEG_INFINITY, y1: f32::NEG_INFINITY };
        corners.iter().fold(empty, |b, p| Rect { x0: b.x0.min(p.x), y0: b.y0.min(p.y), x1: b.x1.max(p.x), y1: b.y1.max(p.y) })
    }

    /// Whether axis-aligned rects stay axis-aligned rects: no rotation and no skew.
    pub fn is_scale_translate(self) -> bool {
        self.b == 0.0 && self.c == 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_keep_kurbos_coefficient_order() {
        let t = Transform::from_kurbo(kurbo::Affine::new([2.0, 0.5, -1.0, 3.0, 7.0, -4.0]));
        assert_eq!(t, Transform { a: 2.0, b: 0.5, c: -1.0, d: 3.0, e: 7.0, f: -4.0 });
    }

    #[test]
    fn then_matches_kurbo_composition() {
        let first = kurbo::Affine::rotate(0.3).then_translate((5.0, -2.0).into());
        let next = kurbo::Affine::scale_non_uniform(2.0, -1.0).then_translate((1.0, 4.0).into());
        let ours = Transform::from_kurbo(first).then(Transform::from_kurbo(next));
        let want = Transform::from_kurbo(next * first);
        let diff = [ours.a - want.a, ours.b - want.b, ours.c - want.c, ours.d - want.d, ours.e - want.e, ours.f - want.f];
        assert!(diff.iter().all(|d| d.abs() < 1e-5), "{ours:?} vs {want:?}");
    }

    #[test]
    fn inverse_undoes_and_singular_has_none() {
        let t = Transform::from_kurbo(kurbo::Affine::new([2.0, 0.5, -1.0, 3.0, 7.0, -4.0]));
        let i = t.inverse().expect("invertible");
        let p = Point { x: 3.0, y: -2.0 };
        let back = i.map_point(t.map_point(p));
        assert!((back.x - p.x).abs() < 1e-5 && (back.y - p.y).abs() < 1e-5, "{back:?}");
        assert_eq!(Transform::from_kurbo(kurbo::Affine::scale_non_uniform(1.0, 0.0)).inverse(), None);
    }

    #[test]
    fn map_rect_bounds_the_rotated_corners() {
        let r = Rect { x0: 0.0, y0: 0.0, x1: 2.0, y1: 1.0 };
        let m = Transform::from_kurbo(kurbo::Affine::rotate(std::f64::consts::FRAC_PI_2)).map_rect(r);
        assert!((m.x0 + 1.0).abs() < 1e-6 && m.x1.abs() < 1e-6 && m.y0.abs() < 1e-6 && (m.y1 - 2.0).abs() < 1e-6, "{m:?}");
        assert!(Transform::from_kurbo(kurbo::Affine::scale(3.0)).is_scale_translate());
        assert!(!Transform::from_kurbo(kurbo::Affine::rotate(0.1)).is_scale_translate());
    }

    #[test]
    fn rects_order_reversed_edges() {
        assert_eq!(Rect::from_kurbo(kurbo::Rect::new(4.0, 3.0, 1.0, 2.0)), Rect { x0: 1.0, y0: 2.0, x1: 4.0, y1: 3.0 });
        assert_eq!(Rect::from_kurbo(kurbo::Rect::new(1.0, 2.0, 4.0, 3.0)).to_kurbo(), kurbo::Rect::new(1.0, 2.0, 4.0, 3.0));
    }
}
