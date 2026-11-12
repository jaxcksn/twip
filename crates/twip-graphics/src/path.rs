//! Paths as pictures store them: `f32` points and one verb per segment (move, line, quad, cubic, close), filled with
//! the non-zero rule.
//!
//! Recording takes kurbo path elements. Stroking, dashing, arcs and SVG path data are kurbo's work at record time, so
//! backends only ever fill. This module also recognises the shapes recording draws faster than a path: an axis-aligned
//! rect, and a single straight segment.

use kurbo::PathEl;

use crate::geom::Point;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Verb {
    /// One point: starts a subpath.
    Move,
    /// One point.
    Line,
    /// Two points: control, end.
    Quad,
    /// Three points: two controls, end.
    Cubic,
    /// No points: closes the subpath.
    Close,
}

/// Appends `els` to `verbs` and `points`, returning the bounds of the points (control points included: they contain the
/// curve), or `None` for a path with no points.
pub(crate) fn push(els: &[PathEl], verbs: &mut Vec<Verb>, points: &mut Vec<Point>) -> Option<kurbo::Rect> {
    push_iter(els.iter().copied(), verbs, points)
}

/// Appends `els` mapped by `map`.
pub(crate) fn push_mapped(els: &[PathEl], map: kurbo::Affine, verbs: &mut Vec<Verb>, points: &mut Vec<Point>) {
    push_iter(els.iter().map(|&e| map * e), verbs, points);
}

fn push_iter(els: impl Iterator<Item = PathEl>, verbs: &mut Vec<Verb>, points: &mut Vec<Point>) -> Option<kurbo::Rect> {
    let mut bounds: Option<kurbo::Rect> = None;
    let mut add = |p: kurbo::Point, points: &mut Vec<Point>| {
        points.push(p.into());
        bounds = Some(bounds.map_or(kurbo::Rect::from_points(p, p), |b| b.union_pt(p)));
    };
    for el in els {
        match el {
            PathEl::MoveTo(p) => {
                verbs.push(Verb::Move);
                add(p, points);
            }
            PathEl::LineTo(p) => {
                verbs.push(Verb::Line);
                add(p, points);
            }
            PathEl::QuadTo(c, p) => {
                verbs.push(Verb::Quad);
                add(c, points);
                add(p, points);
            }
            PathEl::CurveTo(c0, c1, p) => {
                verbs.push(Verb::Cubic);
                add(c0, points);
                add(c1, points);
                add(p, points);
            }
            PathEl::ClosePath => verbs.push(Verb::Close),
        }
    }
    bounds
}

/// The rect `els` outlines when it is exactly one axis-aligned rectangle (four corners, optionally returning to the
/// first, optionally closed), in either winding. A non-zero fill of it is the rect.
pub(crate) fn as_rect(els: &[PathEl]) -> Option<kurbo::Rect> {
    let (first, rest) = els.split_first()?;
    let PathEl::MoveTo(a) = *first else { return None };
    let rest = rest.strip_suffix(&[PathEl::ClosePath]).unwrap_or(rest);
    let rest = rest.strip_suffix(&[PathEl::LineTo(a)]).unwrap_or(rest);
    let &[PathEl::LineTo(b), PathEl::LineTo(c), PathEl::LineTo(d)] = rest else { return None };
    let horizontal_first = a.y == b.y && b.x == c.x && c.y == d.y && d.x == a.x;
    let vertical_first = a.x == b.x && b.y == c.y && c.x == d.x && d.y == a.y;
    (horizontal_first || vertical_first).then(|| kurbo::Rect::from_points(a, c))
}

/// The ends of `els` when it is one straight segment and nothing else.
pub(crate) fn as_line(els: &[PathEl]) -> Option<(kurbo::Point, kurbo::Point)> {
    match *els {
        [PathEl::MoveTo(a), PathEl::LineTo(b)] => Some((a, b)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;

    #[test]
    fn push_keeps_verbs_points_and_control_bounds() {
        let els = [PathEl::MoveTo((0.0, 0.0).into()), PathEl::QuadTo((5.0, -3.0).into(), (2.0, 1.0).into()), PathEl::ClosePath];
        let (mut verbs, mut points) = (Vec::new(), Vec::new());
        let bounds = push(&els, &mut verbs, &mut points);
        assert_eq!(verbs, [Verb::Move, Verb::Quad, Verb::Close]);
        assert_eq!(points.len(), 3);
        assert_eq!(bounds, Some(kurbo::Rect::new(0.0, -3.0, 5.0, 1.0)));
        assert_eq!(push(&[], &mut verbs, &mut points), None);
    }

    #[test]
    fn rects_are_recognised_in_either_winding() {
        let r = kurbo::Rect::new(1.0, 2.0, 4.0, 6.0);
        assert_eq!(as_rect(r.to_path(0.1).elements()), Some(r));
        let reversed = [
            PathEl::MoveTo((1.0, 2.0).into()),
            PathEl::LineTo((1.0, 6.0).into()),
            PathEl::LineTo((4.0, 6.0).into()),
            PathEl::LineTo((4.0, 2.0).into()),
            PathEl::LineTo((1.0, 2.0).into()),
        ];
        assert_eq!(as_rect(&reversed), Some(r));
    }

    #[test]
    fn non_rects_are_not() {
        let skewed = [
            PathEl::MoveTo((0.0, 0.0).into()),
            PathEl::LineTo((4.0, 0.0).into()),
            PathEl::LineTo((5.0, 3.0).into()),
            PathEl::LineTo((0.0, 3.0).into()),
            PathEl::ClosePath,
        ];
        assert_eq!(as_rect(&skewed), None);
        assert_eq!(as_rect(kurbo::Circle::new((0.0, 0.0), 2.0).to_path(0.1).elements()), None);
        assert_eq!(as_line(&skewed), None);
        assert_eq!(as_line(&skewed[..2]), Some(((0.0, 0.0).into(), (4.0, 0.0).into())));
    }
}
