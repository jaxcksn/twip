//! The free intervals a line band leaves beside floats (FLOAT-1 to FLOAT-4, FLOAT-7).

use super::{EPS, Placed, Side, Wrap};

/// FLOAT-2, FLOAT-3: the free intervals of a column [x0, x1] in a band, around every square-wrapped float overlapping
/// it. Returns None when a topAndBottom float overlaps the band (the line must move below it, FLOAT-8).
pub fn intervals(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<Vec<(f64, f64)>> {
    let mut free = vec![(x0, x1)];
    for f in floats.iter().filter(|f| f.in_band(top, bottom)) {
        // Only floats overlapping the column horizontally affect it (FLOAT-11).
        if f.excl[2] <= x0 + EPS || f.excl[0] >= x1 - EPS {
            continue;
        }
        if f.wrap == Wrap::TopAndBottom {
            return None;
        }
        let (l, r) = (f.excl[0], f.excl[2]);
        let side = match f.side {
            Side::Largest => {
                // A tie goes to the left side (FLOAT-3).
                if (l - x0) >= (x1 - r) - EPS { Side::Left } else { Side::Right }
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
    // FLOAT-7: once a float cuts the column, an interval narrower than 18pt takes no text.
    if free.len() != 1 || free[0] != (x0, x1) {
        free.retain(|&(a, b)| b - a >= MIN_GAP - EPS);
    }
    Some(free)
}

/// FLOAT-7: the narrowest interval beside a float that takes text.
const MIN_GAP: f64 = 18.0;

/// FLOAT-4: the lowest edge a blocked band can move down to: the nearest exclusion bottom below `top` among floats
/// overlapping the band.
pub fn next_edge(floats: &[Placed], x0: f64, x1: f64, top: f64, bottom: f64) -> Option<f64> {
    floats
        .iter()
        .filter(|f| f.in_band(top, bottom) && f.excl[2] > x0 + EPS && f.excl[0] < x1 - EPS)
        .map(|f| f.excl[3])
        .filter(|&b| b > top + EPS)
        .fold(None, |m: Option<f64>, b| Some(m.map_or(b, |m| m.min(b))))
}
