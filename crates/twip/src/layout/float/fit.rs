//! Free intervals beside floats

use super::{EPS, Placed, Side, Wrap};

pub fn intervals(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<Vec<(f64, f64)>> {
    let mut free = vec![(x0, x1)];
    for f in floats.iter().filter(|f| f.in_band(top, bottom)) {
        if f.excl[2] <= x0 + EPS || f.excl[0] >= x1 - EPS {
            continue;
        }
        if f.wrap == Wrap::TopAndBottom {
            return None;
        }
        let (l, r) = (f.excl[0], f.excl[2]);
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
        let block = match side {
            Side::Left => (l, f64::INFINITY),
            Side::Right => (f64::NEG_INFINITY, r),
            _ => (l, r),
        };
        let mut next = Vec::new();
        for (a, b) in free {
            if block.1 <= a + EPS || block.0 >= b - EPS {
                next.push((a, b));
                continue;
            }
            if block.0 > a + EPS {
                next.push((a, block.0));
            }
            if block.1 < b - EPS {
                next.push((block.1, b));
            }
        }
        free = next;
    }
    if free.len() != 1 || free[0] != (x0, x1) {
        free.retain(|&(a, b)| b - a >= MIN_GAP - EPS);
    }
    Some(free)
}

const MIN_GAP: f64 = 18.0;

pub fn next_edge(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<f64> {
    floats
        .iter()
        .filter(|f| f.in_band(top, bottom) && f.excl[2] > x0 + EPS && f.excl[0] < x1 - EPS)
        .map(|f| f.excl[3])
        .filter(|&b| b > top + EPS)
        .fold(None, |m: Option<f64>, b| Some(m.map_or(b, |m| m.min(b))))
}
