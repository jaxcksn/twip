//! Ink shapes cut to a box

use crate::drawing::geom::{Geom, GeomPath, PathFill, Seg};
use crate::layout::fragment::ShapeItem;
use std::rc::Rc;

/// Line pieces a curve is cut into before it is clipped.
const CURVE_STEPS: usize = 16;

/// One subpath as points; `closed` when it ends with a close.
struct Poly {
    points: Vec<[f64; 2]>,
    closed: bool,
}

/// `s` cut to `clip` (page coordinates), or none if nothing of it is inside. A shape inside the box is returned as it
/// is. Otherwise each path is flattened and cut to the box: a fill as polygons, a stroke as polylines with its own
/// width left to overhang as an uncut line's would. A shape with a fill keeps its frame (its tiles start from it); one
/// without takes the cut box as its own.
pub(in crate::layout) fn clip_stroke(s: &ShapeItem, clip: [f64; 4]) -> Option<ShapeItem> {
    let [ox, oy, ex, ey] = s.rect;
    let [cx0, cy0, cx1, cy1] = [clip[0].max(ox), clip[1].max(oy), clip[2].min(ex), clip[3].min(ey)];
    if !(cx1 > cx0 && cy1 > cy0) {
        return None;
    }
    if [cx0, cy0, cx1, cy1] == s.rect {
        return Some(s.clone());
    }
    let framed = s.geom.paths.iter().any(|p| p.fill != PathFill::None);
    let (lo, hi) = ([cx0 - ox, cy0 - oy], [cx1 - ox, cy1 - oy]);
    let shift = if framed { [0.0, 0.0] } else { [ox - cx0, oy - cy0] };
    let mut paths = Vec::new();
    for path in &s.geom.paths {
        let subs = flatten(&path.segs);
        if path.fill != PathFill::None {
            let segs: Vec<Seg> = subs.iter().flat_map(|p| polygon(&p.points, lo, hi)).collect();
            if !segs.is_empty() {
                paths.push(GeomPath { segs, fill: path.fill, stroke: false });
            }
        }
        if path.stroke {
            let mut segs = Vec::new();
            for p in &subs {
                polyline(p, lo, hi, shift, &mut segs);
            }
            if !segs.is_empty() {
                paths.push(GeomPath { segs, fill: PathFill::None, stroke: true });
            }
        }
    }
    if paths.is_empty() {
        return None;
    }
    let geom = Geom { paths, text_rect: [0.0, 0.0, cx1 - cx0, cy1 - cy0], handles: vec![], cxn: vec![] };
    Some(ShapeItem { rect: if framed { s.rect } else { [cx0, cy0, cx1, cy1] }, geom: Rc::new(geom), ..s.clone() })
}

/// `segs`' subpaths as points, curves cut into [`CURVE_STEPS`] lines.
fn flatten(segs: &[Seg]) -> Vec<Poly> {
    let mut out: Vec<Poly> = Vec::new();
    for &seg in segs {
        match seg {
            Seg::Move(p) => out.push(Poly { points: vec![p], closed: false }),
            Seg::Close => {
                if let Some(last) = out.last_mut() {
                    last.closed = true;
                }
            }
            // A line or curve after a close starts again from the closed subpath's start.
            _ if out.last().is_none_or(|l| l.closed) => {
                let start = out.last().map_or([0.0; 2], |l| l.points[0]);
                out.push(Poly { points: vec![start], closed: false });
                extend(out.last_mut().expect("just pushed"), seg);
            }
            _ => extend(out.last_mut().expect("a subpath is open"), seg),
        }
    }
    out
}

/// `sub` continued by the line or curve `seg`.
fn extend(sub: &mut Poly, seg: Seg) {
    let lerp = |a: [f64; 2], b: [f64; 2], t: f64| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
    let a = *sub.points.last().expect("a subpath has a point");
    let t_of = |k: usize| k as f64 / CURVE_STEPS as f64;
    match seg {
        Seg::Line(p) => sub.points.push(p),
        Seg::Quad(c, p) => sub.points.extend((1..=CURVE_STEPS).map(|k| lerp(lerp(a, c, t_of(k)), lerp(c, p, t_of(k)), t_of(k)))),
        Seg::Cubic(c1, c2, p) => sub.points.extend((1..=CURVE_STEPS).map(|k| {
            let t = t_of(k);
            let (ab, bc, cd) = (lerp(a, c1, t), lerp(c1, c2, t), lerp(c2, p, t));
            lerp(lerp(ab, bc, t), lerp(bc, cd, t), t)
        })),
        Seg::Move(_) | Seg::Close => {}
    }
}

/// The polygon `points` (closed implicitly) cut to the box `lo .. hi` (Sutherland-Hodgman), as a closed subpath; none
/// when nothing is left.
fn polygon(points: &[[f64; 2]], lo: [f64; 2], hi: [f64; 2]) -> Vec<Seg> {
    let mut poly = points.to_vec();
    for axis in 0..2 {
        for (edge, keep_above) in [(lo[axis], true), (hi[axis], false)] {
            let inside = |p: [f64; 2]| if keep_above { p[axis] >= edge } else { p[axis] <= edge };
            let mut next = Vec::with_capacity(poly.len() + 2);
            for (k, &b) in poly.iter().enumerate() {
                let a = poly[(k + poly.len() - 1) % poly.len()];
                if inside(b) != inside(a) {
                    let t = (edge - a[axis]) / (b[axis] - a[axis]);
                    next.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
                }
                if inside(b) {
                    next.push(b);
                }
            }
            poly = next;
            if poly.len() < 3 {
                return Vec::new();
            }
        }
    }
    let mut segs = vec![Seg::Move(poly[0])];
    segs.extend(poly[1..].iter().map(|&p| Seg::Line(p)));
    segs.push(Seg::Close);
    segs
}

/// `sub`'s lines cut to the box `lo .. hi` (Liang-Barsky), moved by `shift`, as runs of lines: one run goes on while the
/// line stays inside.
fn polyline(sub: &Poly, lo: [f64; 2], hi: [f64; 2], shift: [f64; 2], segs: &mut Vec<Seg>) {
    let at = |p: [f64; 2]| [p[0] + shift[0], p[1] + shift[1]];
    let mut pen_down = false;
    let closing = sub.closed.then(|| sub.points[0]);
    for (&from, &to) in sub.points.iter().zip(sub.points.iter().skip(1).chain(closing.iter())) {
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        let mut outside = false;
        for axis in 0..2 {
            let d = to[axis] - from[axis];
            let (a, b) = (lo[axis] - from[axis], hi[axis] - from[axis]);
            if d.abs() < 1e-12 {
                outside |= a > 0.0 || b < 0.0;
            } else {
                let (mut u0, mut u1) = (a / d, b / d);
                if u0 > u1 {
                    std::mem::swap(&mut u0, &mut u1);
                }
                t0 = t0.max(u0);
                t1 = t1.min(u1);
            }
        }
        if outside || t0 > t1 {
            pen_down = false;
            continue;
        }
        let point = |t: f64| [from[0] + (to[0] - from[0]) * t, from[1] + (to[1] - from[1]) * t];
        if !pen_down || t0 > 0.0 {
            segs.push(Seg::Move(at(point(t0))));
        }
        segs.push(Seg::Line(at(point(t1))));
        pen_down = t1 >= 1.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::table::borders::stroke_shape;

    fn points(g: &Geom, path: usize) -> Vec<[f64; 2]> {
        g.paths[path]
            .segs
            .iter()
            .filter_map(|s| match *s {
                Seg::Move(p) | Seg::Line(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn a_shape_inside_the_box_is_kept_as_it_is() {
        let s = stroke_shape([0.0, 0.0, 10.0, 10.0], 1.0, 0, vec![Seg::Move([0.0, 0.0]), Seg::Cubic([2.0, 0.0], [4.0, 10.0], [10.0, 10.0])]);
        let kept = clip_stroke(&s, [-5.0, -5.0, 20.0, 20.0]).expect("inside");
        assert!(Rc::ptr_eq(&kept.geom, &s.geom), "curves and all");
    }

    #[test]
    fn a_curve_across_the_box_is_cut_and_not_dropped() {
        let s = stroke_shape([0.0, 0.0, 10.0, 10.0], 1.0, 0, vec![Seg::Move([0.0, 5.0]), Seg::Cubic([3.0, 0.0], [6.0, 10.0], [10.0, 5.0])]);
        let cut = clip_stroke(&s, [0.0, 0.0, 5.0, 10.0]).expect("half of it is inside");
        assert_eq!(cut.rect, [0.0, 0.0, 5.0, 10.0]);
        let p = points(&cut.geom, 0);
        assert!(p.len() > 4, "the curve stays a run of lines: {p:?}");
        assert!(p.iter().all(|q| q[0] <= 5.0 + 1e-9), "{p:?}");
        assert!(p.iter().any(|q| (q[1] - 5.0).abs() > 0.5), "it still bends: {p:?}");
    }

    #[test]
    fn a_fill_is_cut_to_a_polygon_and_keeps_its_frame() {
        let mut s = (*stroke_shape([10.0, 10.0, 20.0, 20.0], 1.0, 0, Vec::new())).clone();
        let square = vec![Seg::Move([0.0, 0.0]), Seg::Line([10.0, 0.0]), Seg::Line([10.0, 10.0]), Seg::Line([0.0, 10.0]), Seg::Close];
        s.geom = Rc::new(Geom { paths: vec![GeomPath { segs: square, fill: PathFill::Norm, stroke: false }], ..(*s.geom).clone() });
        let cut = clip_stroke(&s, [10.0, 10.0, 15.0, 30.0]).expect("half of it is inside");
        assert_eq!(cut.rect, s.rect, "the frame its tiles start from");
        let path = &cut.geom.paths[0];
        assert_eq!((path.fill, path.stroke), (PathFill::Norm, false));
        let p = points(&cut.geom, 0);
        assert!(p.len() == 4 && p.iter().all(|q| q[0] <= 5.0 + 1e-9), "{p:?}");
        assert!(matches!(path.segs.last(), Some(Seg::Close)));
        assert!(clip_stroke(&s, [30.0, 30.0, 40.0, 40.0]).is_none());
    }
}
