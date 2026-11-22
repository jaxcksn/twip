//! Regions as rects: boolean combination by sweeping bands, and the rects a bit mask covers.

use twip_graphics::kurbo::Rect;

/// The rects covering where `keep` holds of each point's membership of `regions` (bit `i` set inside region `i`;
/// at most 32 regions), as disjoint rects band by band, down the page. `spend` is charged the sweep's work and
/// stops it when it returns false.
pub(super) fn combine(regions: &[&[Rect]], keep: impl Fn(u32) -> bool, mut spend: impl FnMut(usize) -> bool) -> Vec<Rect> {
    let mut ys: Vec<f64> = regions.iter().flat_map(|r| r.iter()).flat_map(|r| [r.y0, r.y1]).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    // The rects by their top, and those across the band being swept.
    let mut by_top: Vec<(usize, Rect)> =
        regions.iter().enumerate().flat_map(|(i, r)| r.iter().filter(|r| r.width() > 0.0 && r.height() > 0.0).map(move |r| (i, *r))).collect();
    by_top.sort_by(|a, b| a.1.y0.total_cmp(&b.1.y0));
    let (mut next, mut active, mut edges, mut out) = (0, Vec::new(), Vec::new(), Vec::new());
    let mut inside = vec![0u32; regions.len()];
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        while next < by_top.len() && by_top[next].1.y0 <= y0 {
            active.push(by_top[next]);
            next += 1;
        }
        active.retain(|r: &(usize, Rect)| r.1.y1 > y0);
        if !spend(active.len() + 1) {
            break;
        }
        edges.clear();
        edges.extend(active.iter().filter(|r| r.1.y0 <= y0 && r.1.y1 >= y1).flat_map(|&(i, r)| [(r.x0, i, true), (r.x1, i, false)]));
        edges.sort_by(|a: &(f64, usize, bool), b| a.0.total_cmp(&b.0));
        inside.fill(0);
        let mut open: Option<f64> = None;
        let mut i = 0;
        while i < edges.len() {
            let x = edges[i].0;
            while i < edges.len() && edges[i].0 == x {
                let (_, r, enter) = edges[i];
                inside[r] = if enter { inside[r] + 1 } else { inside[r].saturating_sub(1) };
                i += 1;
            }
            let mask = inside.iter().enumerate().fold(0u32, |m, (k, &n)| if n > 0 { m | 1 << k } else { m });
            match (open, keep(mask)) {
                (None, true) => open = Some(x),
                (Some(x0), false) => {
                    out.push(Rect::new(x0, y0, x, y1));
                    open = None;
                }
                _ => {}
            }
        }
    }
    out
}

/// The rects where `bit(x, y)` holds in a `w` × `h` grid (rows top down): each row's runs, a run joined to the one
/// above it when they match. `spend` is charged a unit a row and a unit for each new rect before it is kept; `None`
/// when it returns false.
pub(super) fn runs(w: u32, h: u32, bit: impl Fn(u32, u32) -> bool, mut spend: impl FnMut(usize) -> bool) -> Option<Vec<Rect>> {
    let mut out: Vec<Rect> = Vec::new();
    // The rects the row above ended, by their index in `out`.
    let (mut above, mut here): (Vec<usize>, Vec<usize>) = (Vec::new(), Vec::new());
    for y in 0..h {
        if !spend(w as usize / 64 + 1) {
            return None;
        }
        here.clear();
        // The row above's runs are in order: a cursor through them finds a match.
        let mut j = 0;
        let mut x = 0;
        while x < w {
            if !bit(x, y) {
                x += 1;
                continue;
            }
            let start = x;
            while x < w && bit(x, y) {
                x += 1;
            }
            let (x0, x1) = (f64::from(start), f64::from(x));
            while j < above.len() && out[above[j]].x0 < x0 {
                j += 1;
            }
            match above.get(j).copied().filter(|&k| out[k].x0 == x0 && out[k].x1 == x1) {
                Some(k) => {
                    out[k].y1 = f64::from(y + 1);
                    here.push(k);
                }
                None => {
                    if !spend(1) {
                        return None;
                    }
                    here.push(out.len());
                    out.push(Rect::new(x0, f64::from(y), x1, f64::from(y + 1)));
                }
            }
        }
        std::mem::swap(&mut above, &mut here);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(rs: &[Rect]) -> f64 {
        rs.iter().map(|r| r.area()).sum()
    }

    #[test]
    fn regions_combine_exactly() {
        let a = [Rect::new(0.0, 0.0, 10.0, 10.0)];
        let b = [Rect::new(5.0, 5.0, 15.0, 15.0)];
        let all = |f: fn(u32) -> bool| combine(&[&a, &b], f, |_| true);
        assert_eq!(area(&all(|m| m != 0)), 175.0, "union");
        assert_eq!(area(&all(|m| m == 3)), 25.0, "intersection");
        assert_eq!(area(&all(|m| m == 1 || m == 2)), 150.0, "exclusive or");
        assert_eq!(area(&all(|m| m == 1)), 75.0, "difference");
    }

    #[test]
    fn a_mask_becomes_merged_runs() {
        // An L: two rows of four, then two rows of one.
        let bits = |x: u32, y: u32| y < 2 && x < 4 || x == 0;
        let rs = runs(6, 4, bits, |_| true);
        assert_eq!(rs, Some(vec![Rect::new(0.0, 0.0, 4.0, 2.0), Rect::new(0.0, 2.0, 1.0, 4.0)]));
    }

    /// A checkerboard's rects are charged as they are made: a budget its rows fit in stops it.
    #[test]
    fn a_masks_rects_are_charged_before_they_are_kept() {
        let checker = |x: u32, y: u32| (x + y).is_multiple_of(2);
        let mut spent = 0;
        let rs = runs(64, 64, checker, |n| {
            spent += n;
            true
        });
        assert_eq!(rs.map(|r| r.len()), Some(64 * 32));
        assert_eq!(spent, 64 * 2 + 64 * 32, "a unit a row, a unit a rect");
        let mut left: usize = 64 * 2 + 100;
        let mut kept = 0;
        let rs = runs(64, 64, checker, |n| {
            kept += usize::from(n == 1);
            left = left.saturating_sub(n);
            left > 0
        });
        assert_eq!(rs, None);
        assert!(kept < 64 * 32, "stopped before every rect was made");
    }
}
