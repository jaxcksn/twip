//! Free intervals beside floats

use super::{Contour, EPS, Placed, Side, Wrap};

/// The pieces of `[x0, x1]` the floats leave free over a band; a piece a float cuts is dropped when narrower than its
/// [`Placed::gap`].
pub fn intervals(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64, legacy: bool) -> Option<Vec<(f64, f64)>> {
    let mut free = vec![(x0, x1)];
    for f in floats.iter().filter(|f| f.in_band(top, bottom)) {
        if f.excl[2] <= x0 + EPS || f.excl[0] >= x1 - EPS {
            continue;
        }
        if f.wrap == Wrap::TopAndBottom {
            return None;
        }
        let spans = match &f.contour {
            Some(c) => covered(c, f.dist, top, bottom),
            None => vec![(f.excl[0], f.excl[2])],
        };
        let (Some(&(l, _)), Some(&(_, r))) = (spans.first(), spans.last()) else { continue };
        let side = match f.side {
            Side::Largest => {
                if (l - x0) >= (x1 - r) - EPS {
                    Side::Left
                } else {
                    Side::Right
                }
            }
            s => s,
        };
        let gap = f.gap(legacy);
        let blocks = match side {
            Side::Left => vec![(l, f64::INFINITY)],
            Side::Right => vec![(f64::NEG_INFINITY, r)],
            _ => spans,
        };
        for block in blocks {
            let mut next = Vec::new();
            for (a, b) in free {
                if block.1 <= a + EPS || block.0 >= b - EPS {
                    next.push((a, b));
                    continue;
                }
                if block.0 > a + EPS && block.0 - a >= gap - EPS {
                    next.push((a, block.0));
                }
                if block.1 < b - EPS && b - block.1 >= gap - EPS {
                    next.push((block.1, b));
                }
            }
            free = next;
        }
    }
    Some(free)
}

pub fn next_edge(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<f64> {
    floats
        .iter()
        .filter(|f| f.in_band(top, bottom) && f.excl[2] > x0 + EPS && f.excl[0] < x1 - EPS)
        .map(|f| f.excl[3])
        .filter(|&b| b > top + EPS)
        .fold(None, |m: Option<f64>, b| Some(m.map_or(b, |m| m.min(b))))
}

pub fn steps(floats: &[Placed], top: f64, bottom: f64) -> bool {
    floats.iter().any(|f| f.contour.is_some() && f.in_band(top, bottom))
}

// Polygon contours
fn covered(c: &Contour, dist: [f64; 4], top: f64, bottom: f64) -> Vec<(f64, f64)> {
    let [dt, db, dl, dr] = dist;
    let (lo, hi) = (top - db, bottom + dt);
    let pts = &c.points;
    let mut spans: Vec<(f64, f64)> = Vec::new();
    for (k, &(x0, y0)) in pts.iter().enumerate() {
        let (x1, y1) = pts[(k + 1) % pts.len()];
        let xs = if (y1 - y0).abs() < EPS {
            (y0 >= lo - EPS && y0 < hi - EPS).then_some((x0.min(x1), x0.max(x1)))
        } else {
            let at = |y: f64| ((y - y0) / (y1 - y0)).clamp(0.0, 1.0);
            let (t0, t1) = (at(lo).min(at(hi)), at(lo).max(at(hi)));
            (y0.max(y1) >= lo - EPS && y0.min(y1) < hi - EPS).then(|| {
                let (xa, xb) = (x0 + t0 * (x1 - x0), x0 + t1 * (x1 - x0));
                (xa.min(xb), xa.max(xb))
            })
        };
        if let Some((a, b)) = xs {
            spans.push((a - dl, b + dr));
        }
    }
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for s in spans {
        match merged.last_mut() {
            Some(m) if s.0 <= m.1 + EPS => m.1 = m.1.max(s.1),
            _ => merged.push(s),
        }
    }
    if !c.through {
        return match (merged.first(), merged.last()) {
            (Some(a), Some(b)) => vec![(a.0, b.1)],
            _ => Vec::new(),
        };
    }
    let y = (top + bottom) / 2.0;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for s in merged {
        match out.last_mut() {
            Some(m) if inside(pts, ((m.1 + s.0) / 2.0, y)) => m.1 = s.1,
            _ => out.push(s),
        }
    }
    out
}

fn inside(pts: &[(f64, f64)], (x, y): (f64, f64)) -> bool {
    let mut odd = false;
    for (k, &(x0, y0)) in pts.iter().enumerate() {
        let (x1, y1) = pts[(k + 1) % pts.len()];
        if (y0 > y) != (y1 > y) && x < x0 + (y - y0) * (x1 - x0) / (y1 - y0) {
            odd = !odd;
        }
    }
    odd
}
