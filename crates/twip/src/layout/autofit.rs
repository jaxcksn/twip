//! Table autofit column widths, in integer twips (TABLE-6, TABLE-13 to TABLE-21). Cell minimums and maximums
//! (TABLE-7) come from the table code.

/// Widths and edges are capped at 22 inches (TABLE-6).
pub const MAXW: i64 = 31680;
/// TABLE-16: a spanning cell this close above its columns' maximum takes the maximum; TABLE-15: the minimum of a
/// skipped region.
const KLUGE: i64 = 5;

/// a·b/c rounded half away from zero; 0 when c = 0.
pub fn rdiv(a: i64, b: i64, c: i64) -> i64 {
    if c == 0 {
        return 0;
    }
    let q = a as f64 * b as f64 / c as f64;
    (q.abs() + 0.5).floor().copysign(q) as i64
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Auto,
    /// Fiftieths of a percent (5000 = 100%).
    Pct,
    Dxa,
}

/// A grid column's accumulated widths (TABLE-13).
#[derive(Clone, Copy, Debug, Default)]
pub struct Col {
    pub kind: Option<Kind>,
    pub min: i64,
    pub max: i64,
    pub hard: i64,
    pub pct: i64,
    pub dxa: i64,
}

impl Col {
    fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::Auto)
    }
}

/// One cell's inputs: its grid columns, TABLE-7's min/max/hardMin, and its preferred width (nil already made dxa).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub first: usize,
    pub span: usize,
    pub min: i64,
    pub max: i64,
    pub hard: i64,
    pub kind: Kind,
    pub pref: i64,
    /// The cell's row.
    pub row: usize,
    /// A vertical-merge continuation (skipped, except as a single-span cell in the table's first row).
    pub cont: bool,
}

/// A region a row skips (gridBefore / gridAfter) as a dxa segment: the grid column it ends in, its span and width.
pub type Gap = (usize, usize, i64);

/// TABLE-13 to TABLE-16: the grid columns from the cells (rows in order, cells left to right) and each row's skipped
/// regions (TABLE-15). `sp` = 2 × cell spacing.
pub fn columns(n: usize, cells: &[Cell], sp: i64, gaps: &[Vec<Gap>], fixed: bool) -> Vec<Col> {
    let mut cols = vec![Col::default(); n];
    for c in cells.iter().filter(|c| c.span == 1 && !(c.cont && c.row > 0) && c.first < n) {
        add_single(&mut cols[c.first], c.kind, c.pref, c.min, c.max, c.hard, fixed);
    }
    // TABLE-15: the helper row. Per grid column, the widest dxa segment ending there (a cell's or a skipped region's);
    // a spanning one counts what the columns before it (of the same type) leave.
    let mut seg: Vec<((usize, usize), i64)> = Vec::new();
    let mut put = |end: usize, span: usize, w: i64| match seg.iter_mut().find(|s| s.0 == (end, span)) {
        Some(s) => s.1 = s.1.max(w),
        None => seg.push(((end, span), w)),
    };
    let mut htyp: Vec<Option<Kind>> = vec![None; n];
    for &(end, span, w) in gaps.iter().flatten() {
        if end >= n || w <= 0 {
            continue;
        }
        put(end, span, w);
        if span == 1 && htyp[end].is_none() {
            htyp[end] = Some(Kind::Dxa);
        }
    }
    for c in cells.iter().filter(|c| !(c.cont && c.row > 0)) {
        let end = (c.first + c.span).min(n) - 1;
        if c.kind == Kind::Dxa && c.pref > 0 {
            put(end, c.span, c.pref);
        }
        if c.span == 1 && htyp[end].is_none() {
            htyp[end] = Some(c.kind);
        }
    }
    for &((end, _), _) in &seg {
        htyp[end].get_or_insert(Kind::Dxa);
    }
    let mut helper = vec![0i64; n];
    let mut raised = false;
    for g in 0..n {
        let w = |k: usize| seg.iter().find(|s| s.0 == (g, k)).map(|s| s.1);
        let mut best = w(1).unwrap_or(0);
        for k in 2..g + 2 {
            let Some(mut v) = w(k) else { continue };
            for i in 1..k {
                if htyp[g - i] != htyp[g] {
                    v = 0;
                    break;
                }
                v -= helper[g - i];
            }
            if v != 0 && htyp[g] == Some(Kind::Dxa) {
                v -= (k - 1) as i64 * sp;
            }
            if best < v {
                best = v;
                raised = true;
            }
        }
        helper[g] = best;
    }
    if raised {
        for g in (0..n).filter(|&g| htyp[g] == Some(Kind::Dxa) && helper[g] > 0) {
            add_single(&mut cols[g], Kind::Dxa, helper[g], KLUGE, helper[g], 0, fixed);
        }
    }
    // The last one-column region before the first column, and after the last, count whatever the helper row gave.
    let mut before = None;
    let mut after = None;
    for &(end, _, w) in gaps.iter().flatten().filter(|g| g.1 == 1 && g.2 > 0) {
        if end == 0 {
            before = Some(w);
        } else if end == n - 1 {
            after = Some(w);
        }
    }
    if let Some(w) = before {
        add_single(&mut cols[0], Kind::Dxa, w, KLUGE, w, 0, fixed);
    }
    if let Some(w) = after {
        add_single(&mut cols[n - 1], Kind::Dxa, w, KLUGE, w, 0, fixed);
    }
    // TABLE-14: every spanning cell, in row order and left to right, by TABLE-16's proportional spread.
    let mut spans: Vec<&Cell> = cells.iter().filter(|c| c.span >= 2 && !c.cont && c.first < n).collect();
    spans.sort_by_key(|c| (c.row, c.first));
    for c in spans {
        if c.first + 1 == n {
            add_single(&mut cols[c.first], c.kind, c.pref, c.min, c.max, c.hard, fixed);
        } else {
            spread(&mut cols, c, sp);
        }
    }
    for col in &mut cols {
        col.max = col.max.max(col.min).min(MAXW);
        col.min = col.min.min(MAXW);
        col.hard = col.hard.min(MAXW);
    }
    cols
}

/// TABLE-13: one single-span cell into its column.
fn add_single(col: &mut Col, kind: Kind, pref: i64, min: i64, max: i64, hard: i64, fixed: bool) {
    let cell_max = if kind == Kind::Dxa { if fixed { pref } else { min.max(pref) } } else { max };
    match kind {
        Kind::Pct => {
            col.pct = if col.kind() == Kind::Pct { col.pct.max(pref) } else { pref };
            col.kind = Some(Kind::Pct);
        }
        Kind::Dxa if col.kind() != Kind::Pct => {
            col.dxa = if col.kind() == Kind::Dxa { col.dxa.max(pref) } else { pref };
            col.kind = Some(Kind::Dxa);
        }
        _ => {}
    }
    // Fixed layout: a cell's minimum doesn't raise its column's max until after the spanning cells (TABLE-13).
    col.max = if fixed { col.max.max(cell_max) } else { col.max.max(min).max(cell_max) };
    col.min = col.min.max(min);
    col.hard = col.hard.max(hard);
}

/// TABLE-16: one spanning cell over its columns.
fn spread(cols: &mut [Col], c: &Cell, sp: i64) {
    let span = c.span.min(cols.len() - c.first);
    if span < 2 {
        return;
    }
    let rng = c.first..c.first + span;
    let k = (span - 1) as i64 * sp;
    let mut sum_min: i64 = cols[rng.clone()].iter().map(|x| x.min).sum();
    let mut sum_max: i64 = cols[rng.clone()].iter().map(|x| x.max).sum();
    if sum_max == 0 {
        sum_max = c.max - k;
    }
    if sum_min == 0 {
        sum_min = c.min - k;
    }
    let mut cmin = c.min - k;
    let mut cmax = (if c.kind == Kind::Dxa { c.pref } else { c.max }) - k;
    if sum_max < cmin && cmin < sum_max + KLUGE {
        cmin = sum_max;
    }
    if sum_max < cmax && cmax < sum_max + KLUGE {
        cmax = sum_max;
    }
    cmax = cmax.max(cmin);
    if c.kind == Kind::Pct && c.pref > 0 {
        let had: i64 = cols[rng.clone()].iter().filter(|x| x.kind() == Kind::Pct).map(|x| x.pct).sum();
        let without: Vec<usize> = rng.clone().filter(|&i| cols[i].kind() != Kind::Pct).collect();
        let rem = c.pref.min(5000) - had;
        if rem > 0 && !without.is_empty() {
            let tot: i64 = without.iter().map(|&i| cols[i].max).sum();
            for &i in &without {
                cols[i].pct = if tot > 0 { cols[i].max * rem / tot } else { rem / without.len() as i64 };
            }
        }
        for i in rng.clone() {
            cols[i].kind = Some(Kind::Pct);
        }
    }
    if cmin > sum_min {
        let mut left = cmin;
        let mut rest: Vec<usize> = rng.clone().collect();
        // pct columns first, by their share.
        if rest.iter().any(|&i| cols[i].kind() == Kind::Pct) {
            let pct_tot =
                if rest.iter().any(|&i| cols[i].kind() != Kind::Pct) { 5000 } else { rest.iter().map(|&i| cols[i].pct).sum::<i64>().min(5000) };
            let mut pct_left = pct_tot;
            for &i in rng.clone().filter(|&i| cols[i].kind() == Kind::Pct).collect::<Vec<_>>().iter() {
                let p = cols[i].pct.min(pct_left);
                let w = if pct_tot > 0 { cmin * p / pct_tot } else { 0 };
                sum_min -= cols[i].min;
                sum_max -= cols[i].max;
                cols[i].min = cols[i].min.max(w);
                pct_left -= p;
                left -= w;
                rest.retain(|&j| j != i);
            }
        }
        if !rest.is_empty() {
            if cmin > sum_max {
                let tot = sum_min + sum_max;
                let mut leftover = left;
                for &i in &rest {
                    let w = if tot > 0 { rdiv(left, cols[i].min + cols[i].max, tot) } else { rdiv(left, 1, rest.len() as i64) };
                    cols[i].min = cols[i].min.max(w);
                    leftover = if w > leftover { 0 } else { leftover - w };
                }
                for &i in &rest {
                    if leftover <= 0 {
                        break;
                    }
                    cols[i].min += 1;
                    leftover -= 1;
                }
            } else {
                let mut extra = if sum_min <= left { left - sum_min } else { 0 };
                let tot = (sum_min + sum_max) as f64;
                let mut r: Vec<f64> =
                    rest.iter().map(|&i| if tot > 0.0 { (cols[i].max + cols[i].min) as f64 / tot } else { 1.0 / rest.len() as f64 }).collect();
                let mut fixed = vec![false; rest.len()];
                let (mut over, mut free) = (0.0, 0.0);
                for (j, &i) in rest.iter().enumerate() {
                    if cols[i].min as f64 + extra as f64 * r[j] > cols[i].max as f64 {
                        fixed[j] = true;
                        let nr = if cols[i].max > cols[i].min { (cols[i].max - cols[i].min) as f64 / extra as f64 } else { 0.0 };
                        over += r[j] - nr;
                        r[j] = nr;
                    } else {
                        free += r[j];
                    }
                }
                for j in 0..rest.len() {
                    if !fixed[j] && free > 0.0 {
                        r[j] += over * r[j] / free;
                    }
                }
                for (j, &i) in rest.iter().enumerate() {
                    let d = ((extra as f64 * r[j]) as i64).min(cols[i].max - cols[i].min);
                    cols[i].min += d;
                    extra -= d;
                }
                while extra > 0 {
                    let e0 = extra;
                    for &i in &rest {
                        if cols[i].min < cols[i].max {
                            cols[i].min += 1;
                            extra -= 1;
                        }
                    }
                    if extra == e0 {
                        break;
                    }
                }
            }
        }
    }
    let mut s = 0;
    for i in rng.clone() {
        cols[i].max = cols[i].max.max(cols[i].min);
        s += cols[i].max;
    }
    if cmax > s {
        let mut left = cmax;
        for i in rng.clone() {
            let v = if s > 0 { rdiv(cols[i].max, cmax, s) } else { rdiv(cmax, 1, span as i64) };
            cols[i].max = v;
            left -= v;
        }
        for i in rng {
            if left <= 0 {
                break;
            }
            cols[i].max += 1;
            left -= 1;
        }
    }
}

/// TABLE-20: spreads `target − Σout` over the columns, leftovers to the first ones, never below `floor`.
pub fn distribute(out: &mut [i64], floor: &[i64], target: i64) {
    let n = out.len();
    let mut s: i64 = out.iter().sum();
    let mut cnt = (0..n).filter(|&i| out[i] > floor[i]).count() as i64;
    if s == target {
        return;
    }
    for _ in 0..10 {
        let diff = target - s;
        if diff > 0 {
            cnt = n as i64;
        }
        if cnt == 0 {
            break;
        }
        let q = diff / cnt;
        let mut r = diff - q * cnt;
        let mut step = r.signum();
        let mut carry = 0;
        s = 0;
        let mut next = 0;
        for i in 0..n {
            if out[i] > floor[i] || diff > 0 {
                carry += q + step;
                out[i] += carry;
                if out[i] < floor[i] {
                    carry = out[i] - floor[i];
                    out[i] = floor[i];
                } else {
                    carry = 0;
                    next += 1;
                }
                r -= step;
                if r == 0 {
                    step = 0;
                }
            }
            s += out[i];
        }
        cnt = next;
        if s == target {
            break;
        }
    }
}

/// TABLE-19's result: the table width, the column widths (before TABLE-21's spacing), and the minimum and hard
/// minimum table widths.
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub width: i64,
    pub widths: Vec<i64>,
    pub min_t: i64,
    pub hard_t: i64,
}

/// TABLE-19. `tbl_w` <= 0: no preferred table width. `fixed`: w:tblLayout fixed. `no_grow`: not w:growAutofit.
pub fn resolve(cols: &[Col], pad_total: i64, tbl_w: i64, avail: i64, fixed: bool, no_grow: bool) -> Resolved {
    let n = cols.len();
    let mn: Vec<i64> = cols.iter().map(|c| c.min).collect();
    let mx: Vec<i64> = cols.iter().map(|c| c.max).collect();
    let hd: Vec<i64> = cols.iter().map(|c| c.hard).collect();
    let mut pct: Vec<i64> = cols.iter().map(|c| c.pct).collect();
    let (sum_min, sum_max, sum_hard): (i64, i64, i64) = (mn.iter().sum(), mx.iter().sum(), hd.iter().sum());
    let (mut sum_pct, mut ratio_num, mut ratio_den) = (0i64, 0i64, 1i64);
    let (mut min_pct, mut max_dxa, mut min_dxa, mut max_auto, mut min_auto) = (0, 0, 0, 0, 0);
    for (i, c) in cols.iter().enumerate() {
        match c.kind() {
            Kind::Pct => {
                let p = pct[i].min(5000);
                if p != 0 && mx[i] as i128 * ratio_den as i128 > p as i128 * ratio_num as i128 {
                    (ratio_num, ratio_den) = (mx[i], p);
                }
                if p + sum_pct > 5000 {
                    pct[i] = 5000 - sum_pct;
                    sum_pct = 5000;
                } else {
                    sum_pct += p;
                }
                min_pct += mn[i];
            }
            Kind::Dxa => {
                max_dxa += mx[i];
                min_dxa += mn[i];
            }
            Kind::Auto => {
                max_auto += mx[i];
                min_auto += mn[i];
            }
        }
    }
    let rem_pct = (5000 - sum_pct).max(0);
    let (min_t, hard_t) = (sum_min + pad_total, sum_hard + pad_total);
    let max_t = (sum_max + pad_total).max(min_t);
    let (clamp, mut w);
    if tbl_w > 0 {
        clamp = max_t.min(min_t.max(tbl_w));
        w = tbl_w.max(min_t);
    } else {
        clamp = max_t;
        if sum_pct == 0 {
            w = min_t.max(max_t.min(avail));
        } else {
            let non_pct_max = max_auto + max_dxa;
            w = avail;
            if !(rem_pct == 0 && non_pct_max != 0) {
                if rem_pct != 0 && (ratio_num as i128) * (rem_pct as i128) < (non_pct_max as i128) * (ratio_den as i128) {
                    (ratio_num, ratio_den) = (non_pct_max, rem_pct);
                }
                let need = rdiv(5000, ratio_num, ratio_den);
                if need + pad_total < avail {
                    w = need + pad_total;
                }
            }
            w = w.max(min_t);
        }
    }
    if fixed {
        w = clamp.max(w);
    }
    if !fixed && no_grow && w == min_t && avail < w {
        let mut out = hd.clone();
        if avail <= hard_t || min_t <= hard_t {
            return Resolved { width: hard_t, widths: out, min_t, hard_t };
        }
        for i in 0..n {
            out[i] += rdiv(avail - hard_t, mn[i] - hd[i], min_t - hard_t);
        }
        distribute(&mut out, &hd, avail - pad_total);
        return Resolved { width: avail, widths: out, min_t, hard_t };
    }
    let ww = if w >= pad_total { w - pad_total } else { w };
    let non_pct = if max_auto + max_dxa == 0 { 0 } else { rdiv(rem_pct, ww, 5000).max(min_dxa + min_auto).min(ww - min_pct) };
    let (dxa, aut);
    if max_dxa != 0 {
        let mut d = max_dxa.min(non_pct);
        if max_auto != 0 {
            if d + min_auto <= non_pct {
                aut = non_pct - d;
            } else {
                aut = min_auto;
                d = if min_dxa + min_auto <= non_pct { non_pct - min_auto } else { min_dxa };
            }
        } else {
            aut = 0;
            if d < non_pct {
                d = non_pct;
            }
        }
        dxa = d;
    } else {
        dxa = 0;
        aut = if max_auto != 0 { min_auto.max(non_pct) } else { 0 };
    }
    let pct_rem = if ww != 0 { rdiv(5000, ww - dxa - aut, ww) } else { 0 };
    let mut out: Vec<i64> = (0..n)
        .map(|i| match cols[i].kind() {
            Kind::Pct => {
                let t = if sum_pct != 0 { rdiv(ww, rdiv(pct_rem, pct[i], sum_pct), 5000) } else { 0 };
                mn[i] + (t - mn[i]).max(0).min(MAXW - mn[i])
            }
            k => {
                let (s_max, s_min, space) = if k == Kind::Dxa { (max_dxa, min_dxa, dxa) } else { (max_auto, min_auto, aut) };
                if space > s_max {
                    mx[i] + rdiv(mx[i], space - s_max, s_max)
                } else if space == s_max {
                    mx[i]
                } else if space == s_min {
                    mn[i]
                } else if space < s_max {
                    mx[i] - rdiv(mx[i] - mn[i], s_max - space, s_max - s_min)
                } else if s_max != 0 {
                    mn[i] + rdiv(mx[i], space - s_min, s_max)
                } else {
                    0
                }
            }
        })
        .collect();
    distribute(&mut out, &mn, ww);
    Resolved { width: ww + pad_total, widths: out, min_t, hard_t }
}

/// TABLE-21: the cell spacing added back (`s2` = 2 × spacing per column, half more at the outer columns).
pub fn with_spacing(widths: &mut [i64], s2: i64) {
    if s2 <= 0 || widths.is_empty() {
        return;
    }
    for w in widths.iter_mut() {
        *w += s2.min(MAXW - *w);
    }
    widths[0] += s2 / 2;
    let last = widths.len() - 1;
    widths[last] += s2 / 2;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dxa(w: i64, min: i64) -> Col {
        Col { kind: Some(Kind::Dxa), min, max: w.max(min), hard: 0, pct: 0, dxa: w }
    }

    #[test]
    fn rounding_leftovers_go_to_the_first_columns() {
        let mut a = [100, 100, 100];
        distribute(&mut a, &[0, 0, 0], 302);
        assert_eq!(a, [101, 101, 100]);
        let mut b = [100, 100, 100];
        distribute(&mut b, &[0, 0, 0], 298);
        assert_eq!(b, [99, 99, 100]);
    }

    #[test]
    fn three_preferred_columns_in_9350() {
        // TABLE-20's example: 3 × 3120 in 9350 → 3116 + 3117 + 3117.
        let cols = [dxa(3120, 300), dxa(3120, 300), dxa(3120, 300)];
        assert_eq!(resolve(&cols, 0, 0, 9350, false, true).widths, vec![3116, 3117, 3117]);
    }

    #[test]
    fn reference_example() {
        // Two auto columns, one long, one short, in 9350.
        let cols = [Col { min: 500, max: 9000, ..Default::default() }, Col { min: 312, max: 312, ..Default::default() }];
        let r = resolve(&cols, 0, 0, 9350, false, true);
        assert_eq!((r.width, r.widths.iter().sum::<i64>()), (9312, 9312));
        assert_eq!(r.widths, vec![9000, 312]);
    }

    #[test]
    fn a_spanning_cell_widens_its_columns_in_proportion() {
        let mut cols = vec![Col { min: 100, max: 200, ..Default::default() }, Col { min: 100, max: 200, ..Default::default() }];
        spread(&mut cols, &Cell { first: 0, span: 2, min: 1000, max: 1000, hard: 0, kind: Kind::Auto, pref: 0, row: 0, cont: false }, 0);
        assert_eq!(cols.iter().map(|c| c.min).sum::<i64>(), 1000);
        assert!(cols.iter().all(|c| c.max >= c.min));
    }
}
