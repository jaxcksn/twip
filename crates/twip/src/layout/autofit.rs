//! Table autofit

pub const MAXW: i64 = 31680;
/// A span need this close above its columns' sum snaps to the sum.
pub const SNAP_TOLERANCE: i64 = 5;
/// The min of helper cells from row gaps and spanning preferred widths.
pub const PSEUDO_CELL_MIN: i64 = 5;

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
    Pct,
    Dxa,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Col {
    pub kind: Option<Kind>,
    pub min: i64,
    pub max: i64,
    pub hard: i64,
    pub pct: i64,
    pub dxa: i64,
    /// A single-span pct cell or skip set its pct.
    pub own_pct: bool,
}

impl Col {
    fn kind(&self) -> Kind {
        self.kind.unwrap_or(Kind::Auto)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    pub first: usize,
    pub span: usize,
    pub min: i64,
    pub max: i64,
    pub hard: i64,
    pub kind: Kind,
    pub pref: i64,
    pub row: usize,
    pub cont: bool,
}

pub type Gap = (usize, usize, i64);

pub fn columns(n: usize, cells: &[Cell], sp: i64, gaps: &[Vec<Gap>], fixed: bool) -> Vec<Col> {
    let mut cols = vec![Col::default(); n];
    if n == 0 {
        return cols;
    }
    let cells: Vec<&Cell> = cells.iter().filter(|c| !(c.cont && c.row > 0) && c.first < n).collect();
    let last = |c: &Cell| (c.first + c.span.max(1) - 1).min(n - 1);
    for c in cells.iter().filter(|c| c.span == 1) {
        add_single(&mut cols[c.first], c.kind, c.pref, c.min, c.max, c.hard, fixed);
    }
    let gaps: Vec<Gap> = gaps.iter().flatten().copied().filter(|&(e, k, _)| e < n && (1..=e + 1).contains(&k)).collect();
    let mut widest: Vec<Vec<(usize, i64)>> = vec![Vec::new(); n];
    let prefs = cells.iter().filter(|c| c.kind == Kind::Dxa && c.pref > 0).map(|c| (last(c), last(c) - c.first + 1, c.pref));
    for (e, k, w) in gaps.iter().copied().chain(prefs).filter(|s| s.2 > 0) {
        match widest[e].iter_mut().find(|s| s.0 == k) {
            Some(s) => s.1 = s.1.max(w),
            None => widest[e].push((k, w)),
        }
    }
    let mut ty: Vec<Option<Kind>> = vec![None; n];
    for &(e, _, _) in gaps.iter().filter(|g| g.1 == 1) {
        ty[e].get_or_insert(Kind::Dxa);
    }
    for c in cells.iter().filter(|c| last(c) == c.first) {
        ty[c.first].get_or_insert(c.kind);
    }
    for e in 0..n {
        if !widest[e].is_empty() {
            ty[e].get_or_insert(Kind::Dxa);
        }
    }
    let mut h = vec![0i64; n];
    let mut spanned = false;
    for g in 0..n {
        let start = widest[g].iter().find(|s| s.0 == 1).map_or(0, |s| s.1);
        h[g] = start;
        for &(k, w) in widest[g].iter().filter(|s| s.0 >= 2) {
            let before = g + 1 - k..g;
            let mut v = if before.clone().any(|j| ty[j] != ty[g]) { 0 } else { w - h[before].iter().sum::<i64>() };
            if v != 0 && ty[g] == Some(Kind::Dxa) {
                v -= (k as i64 - 1) * sp;
            }
            spanned |= v > start;
            h[g] = h[g].max(v);
        }
    }
    if spanned {
        for g in (0..n).filter(|&g| ty[g] == Some(Kind::Dxa) && h[g] > 0) {
            add_single(&mut cols[g], Kind::Dxa, h[g], PSEUDO_CELL_MIN, h[g], 0, fixed);
        }
    }
    for e in if n == 1 { vec![0] } else { vec![0, n - 1] } {
        if let Some(&(_, _, w)) = gaps.iter().rfind(|g| g.0 == e && g.1 == 1 && g.2 > 0) {
            add_single(&mut cols[e], Kind::Dxa, w, PSEUDO_CELL_MIN, w, 0, fixed);
        }
    }
    let mut spans: Vec<&Cell> = cells.iter().copied().filter(|c| c.span >= 2).collect();
    spans.sort_by_key(|c| (c.row, c.first));
    for c in spans {
        if c.first == n - 1 {
            add_single(&mut cols[c.first], c.kind, c.pref, c.min, c.max, c.hard, fixed);
        } else {
            spread(&mut cols, c, sp);
        }
    }
    for c in &mut cols {
        c.max = c.max.max(c.min).min(MAXW);
        c.min = c.min.min(MAXW);
        c.hard = c.hard.min(MAXW);
    }
    cols
}

fn add_single(col: &mut Col, kind: Kind, pref: i64, min: i64, max: i64, hard: i64, fixed: bool) {
    let wide = match kind {
        Kind::Dxa if fixed => pref,
        Kind::Dxa => min.max(pref),
        _ => max,
    };
    match kind {
        Kind::Pct => {
            col.pct = if col.kind == Some(Kind::Pct) { col.pct.max(pref) } else { pref };
            col.kind = Some(Kind::Pct);
            col.own_pct = true;
        }
        Kind::Dxa if col.kind != Some(Kind::Pct) => {
            col.dxa = if col.kind == Some(Kind::Dxa) { col.dxa.max(pref) } else { pref };
            col.kind = Some(Kind::Dxa);
        }
        _ => {}
    }
    col.max = if fixed { col.max.max(wide) } else { col.max.max(min).max(wide) };
    col.min = col.min.max(min);
    col.hard = col.hard.max(hard);
}

fn spread(cols: &mut [Col], c: &Cell, sp: i64) {
    let end = (c.first + c.span).min(cols.len());
    if c.first + 2 > end {
        return;
    }
    let cs = &mut cols[c.first..end];
    let span = cs.len() as i64;
    let g = (span - 1) * sp;
    let mut smin: i64 = cs.iter().map(|k| k.min).sum();
    let mut smax: i64 = cs.iter().map(|k| k.max).sum();
    if smax == 0 {
        smax = c.max - g;
    }
    if smin == 0 {
        smin = c.min - g;
    }
    let snap = |need: i64| if smax < need && need < smax + SNAP_TOLERANCE { smax } else { need };
    let need_min = snap(c.min - g);
    let need_max = snap(if c.kind == Kind::Dxa { c.pref } else { c.max } - g).max(need_min);
    let pct = |k: &Col| k.kind == Some(Kind::Pct);
    if c.kind == Kind::Pct && c.pref > 0 {
        let rest = c.pref.min(5000) - cs.iter().filter(|k| pct(k)).map(|k| k.pct).sum::<i64>();
        let others = cs.iter().filter(|k| !pct(k)).count() as i64;
        if rest > 0 && others > 0 {
            let wide: i64 = cs.iter().filter(|k| !pct(k)).map(|k| k.max).sum();
            for k in cs.iter_mut().filter(|k| !pct(k)) {
                k.pct = if wide == 0 { rest / others } else { rest * k.max / wide };
            }
        } else if rest > 0 && cs[cs.len() - 1].own_pct {
            // Every column is pct already: the last one, when a cell of its own made it pct, takes the rest.
            // ponytail: the condition fits the measured skip tables; other all-pct spans are unmeasured.
            cs[cs.len() - 1].pct += rest;
        }
        for k in cs.iter_mut() {
            k.kind = Some(Kind::Pct);
        }
    }
    if need_min > smin {
        let mut left = need_min;
        let mut open = vec![true; cs.len()];
        if cs.iter().any(pct) {
            let whole = if cs.iter().all(pct) { cs.iter().map(|k| k.pct).sum::<i64>().min(5000) } else { 5000 };
            let mut share_left = whole;
            for (i, k) in cs.iter_mut().enumerate().filter(|(_, k)| pct(k)) {
                let p = k.pct.min(share_left);
                let w = if whole == 0 { 0 } else { need_min * p / whole };
                smin -= k.min;
                smax -= k.max;
                k.min = k.min.max(w);
                share_left -= p;
                left -= w;
                open[i] = false;
            }
        }
        let open: Vec<usize> = (0..cs.len()).filter(|&i| open[i]).collect();
        let m = open.len() as i64;
        let both = smin + smax;
        if need_min > smax {
            let mut rem = left;
            for &i in &open {
                let k = &mut cs[i];
                let w = if both == 0 { rdiv(left, 1, m) } else { rdiv(left, k.min + k.max, both) };
                k.min = k.min.max(w);
                rem = (rem - w).max(0);
            }
            for &i in open.iter().take(rem as usize) {
                cs[i].min += 1;
            }
        } else {
            let mut e = (left - smin).max(0);
            let ef = e as f64;
            let mut r: Vec<f64> =
                open.iter().map(|&i| if both == 0 { 1.0 / m as f64 } else { (cs[i].min + cs[i].max) as f64 / both as f64 }).collect();
            let mut capped = vec![false; open.len()];
            let mut freed = 0.0;
            for (j, &i) in open.iter().enumerate() {
                let k = &cs[i];
                if k.min as f64 + ef * r[j] > k.max as f64 {
                    let to_max = if k.max <= k.min { 0.0 } else { (k.max - k.min) as f64 / ef };
                    freed += r[j] - to_max;
                    r[j] = to_max;
                    capped[j] = true;
                }
            }
            let free: f64 = (0..r.len()).filter(|&j| !capped[j]).map(|j| r[j]).sum();
            if freed != 0.0 && free != 0.0 {
                for j in (0..r.len()).filter(|&j| !capped[j]) {
                    r[j] += freed * r[j] / free;
                }
            }
            for (j, &i) in open.iter().enumerate() {
                let d = ((e as f64 * r[j]).trunc() as i64).min(cs[i].max - cs[i].min);
                cs[i].min += d;
                e -= d;
            }
            if e > 0 {
                let room: Vec<i64> = open.iter().map(|&i| (cs[i].max - cs[i].min).max(0)).collect();
                let given = |p: i64| room.iter().map(|&r| r.min(p)).sum::<i64>();
                let (mut lo, mut hi) = (0, room.iter().copied().max().unwrap_or(0));
                while lo < hi {
                    let mid = (lo + hi) / 2;
                    if given(mid) >= e {
                        hi = mid;
                    } else {
                        lo = mid + 1;
                    }
                }
                for (j, &i) in open.iter().enumerate() {
                    cs[i].min += room[j].min(lo);
                }
            }
        }
    }
    for k in cs.iter_mut() {
        k.max = k.max.max(k.min);
    }
    let s: i64 = cs.iter().map(|k| k.max).sum();
    if need_max > s {
        for k in cs.iter_mut() {
            k.max = if s == 0 { rdiv(need_max, 1, span) } else { rdiv(k.max, need_max, s) };
        }
        let missing = need_max - cs.iter().map(|k| k.max).sum::<i64>();
        for k in cs.iter_mut().take(missing.max(0) as usize) {
            k.max += 1;
        }
    }
}

pub fn distribute(out: &mut [i64], floor: &[i64], target: i64) {
    let mut open = vec![true; out.len()];
    for round in 0..10 {
        let d = target - out.iter().sum::<i64>();
        if d == 0 {
            return;
        }
        let take: Vec<usize> = (0..out.len()).filter(|&i| d > 0 || if round == 0 { out[i] > floor[i] } else { open[i] }).collect();
        if take.is_empty() {
            return;
        }
        let m = take.len() as i64;
        let q = d / m;
        let extra = (d - q * m).abs();
        let mut carry = 0;
        open.fill(false);
        for (j, &i) in take.iter().enumerate() {
            let w = out[i] + q + if (j as i64) < extra { d.signum() } else { 0 } + carry;
            carry = (w - floor[i]).min(0);
            out[i] = w.max(floor[i]);
            open[i] = carry == 0;
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub width: i64,
    pub widths: Vec<i64>,
    pub min_t: i64,
    pub hard_t: i64,
}

pub fn resolve(cols: &[Col], pad_total: i64, tbl_w: i64, avail: i64, fixed: bool, no_grow: bool) -> Resolved {
    let pad = pad_total;
    let mut share = vec![0i64; cols.len()];
    let (mut p_tot, mut ratio) = (0i64, (0i64, 1i64));
    let (mut sum_min, mut sum_max, mut sum_hard) = (0i64, 0i64, 0i64);
    let (mut xd, mut nd, mut xa, mut na, mut np) = (0i64, 0i64, 0i64, 0i64, 0i64);
    for (i, c) in cols.iter().enumerate() {
        sum_min += c.min;
        sum_max += c.max;
        sum_hard += c.hard;
        match c.kind() {
            Kind::Pct => {
                let p = c.pct.min(5000);
                if p_tot + p > 5000 {
                    share[i] = 5000 - p_tot;
                    p_tot = 5000;
                } else {
                    p_tot += p;
                    share[i] = c.pct;
                }
                if p != 0 && c.max * ratio.1 > ratio.0 * p {
                    ratio = (c.max, p);
                }
                np += c.min;
            }
            Kind::Dxa => (xd, nd) = (xd + c.max, nd + c.min),
            Kind::Auto => (xa, na) = (xa + c.max, na + c.min),
        }
    }
    let p_rest = (5000 - p_tot).max(0);
    let (min_t, hard_t) = (sum_min + pad, sum_hard + pad);
    let max_t = (sum_max + pad).max(min_t);
    let mut w = if tbl_w > 0 {
        tbl_w.max(min_t)
    } else if p_tot == 0 {
        min_t.max(max_t.min(avail))
    } else {
        let mut w = avail;
        if p_rest != 0 || xd + xa == 0 {
            if p_rest != 0 && (xd + xa) * ratio.1 > ratio.0 * p_rest {
                ratio = (xd + xa, p_rest);
            }
            let need = rdiv(5000, ratio.0, ratio.1);
            if need + pad < avail {
                w = need + pad;
            }
        }
        w.max(min_t)
    };
    if fixed {
        w = w.max(if tbl_w > 0 { max_t.min(min_t.max(tbl_w)) } else { max_t });
    }
    let mins: Vec<i64> = cols.iter().map(|c| c.min).collect();
    let hards: Vec<i64> = cols.iter().map(|c| c.hard).collect();
    if !fixed && no_grow && w == min_t && avail < w {
        if avail <= hard_t || min_t <= hard_t {
            return Resolved { width: hard_t, widths: hards, min_t, hard_t };
        }
        let mut widths: Vec<i64> = cols.iter().map(|c| c.hard + rdiv(avail - hard_t, c.min - c.hard, min_t - hard_t)).collect();
        distribute(&mut widths, &hards, avail - pad);
        return Resolved { width: avail, widths, min_t, hard_t };
    }
    let cw = if w < pad { w } else { w - pad };
    let budget = if xd + xa == 0 { 0 } else { rdiv(p_rest, cw, 5000).max(nd + na).min(cw - np) };
    let (bd, ba) = match (xd > 0, xa > 0) {
        (true, true) => {
            let bd = xd.min(budget);
            if bd + na <= budget {
                (bd, budget - bd)
            } else {
                (if nd + na > budget { nd } else { budget - na }, na)
            }
        }
        (true, false) => (budget, 0),
        (false, true) => (0, na.max(budget)),
        (false, false) => (0, 0),
    };
    let p_left = if cw == 0 { 0 } else { rdiv(5000, cw - bd - ba, cw) };
    let mut widths: Vec<i64> = cols
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let (b, x, n) = match c.kind() {
                Kind::Pct => {
                    let t = if p_tot == 0 { 0 } else { rdiv(cw, rdiv(p_left, share[i], p_tot), 5000) };
                    return t.max(c.min).min(MAXW);
                }
                Kind::Dxa => (bd, xd, nd),
                Kind::Auto => (ba, xa, na),
            };
            if b > x {
                c.max + rdiv(c.max, b - x, x)
            } else if b == x {
                c.max
            } else if b == n {
                c.min
            } else {
                c.max - rdiv(c.max - c.min, x - b, x - n)
            }
        })
        .collect();
    distribute(&mut widths, &mins, cw);
    Resolved { width: cw + pad, widths, min_t, hard_t }
}

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
    use serde_json::Value;

    fn dxa(w: i64, min: i64) -> Col {
        Col { kind: Some(Kind::Dxa), min, max: w.max(min), hard: 0, pct: 0, dxa: w, own_pct: false }
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
    fn a_clamped_column_passes_its_deficit_on() {
        let mut a = [500, 100, 300];
        distribute(&mut a, &[400, 100, 0], 600);
        assert_eq!(a, [400, 100, 100]);
        let mut b = [500, 120, 130];
        distribute(&mut b, &[480, 100, 120], 700);
        assert_eq!(b, [480, 100, 120]);
    }

    #[test]
    fn three_preferred_columns_in_9350() {
        let cols = [dxa(3120, 300), dxa(3120, 300), dxa(3120, 300)];
        assert_eq!(resolve(&cols, 0, 0, 9350, false, true).widths, vec![3116, 3117, 3117]);
    }

    #[test]
    fn reference_example() {
        let cols = [Col { min: 500, max: 9000, ..Default::default() }, Col { min: 312, max: 312, ..Default::default() }];
        let r = resolve(&cols, 0, 0, 9350, false, true);
        assert_eq!((r.width, r.widths.iter().sum::<i64>()), (9312, 9312));
        assert_eq!(r.widths, vec![9000, 312]);
    }

    #[test]
    fn a_table_that_may_not_grow_squeezes() {
        let cols = [Col { min: 1500, max: 4000, ..Default::default() }, Col { min: 2000, max: 5000, ..Default::default() }];
        let r = resolve(&cols, 0, 0, 3000, false, true);
        assert_eq!((r.width, r.widths), (3000, vec![1286, 1714]));
    }

    #[test]
    fn an_empty_auto_column_takes_no_budget() {
        let cols = [dxa(4680, 236), Col::default()];
        let r = resolve(&cols, 0, 9360, 9350, true, false);
        assert_eq!((r.width, r.widths), (9360, vec![9360, 0]));
    }

    #[test]
    fn a_spanning_cell_widens_its_columns_in_proportion() {
        let cell = |min, max| Cell { first: 0, span: 2, min, max, hard: 0, kind: Kind::Auto, pref: 0, row: 0, cont: false };
        let mut cols = vec![Col { min: 100, max: 200, ..Default::default() }, Col { min: 100, max: 200, ..Default::default() }];
        spread(&mut cols, &cell(1000, 1000), 0);
        assert_eq!(cols.iter().map(|c| (c.min, c.max)).collect::<Vec<_>>(), [(500, 500), (500, 500)]);
        let mut cols = vec![Col { min: 100, max: 900, ..Default::default() }, Col { min: 100, max: 300, ..Default::default() }];
        spread(&mut cols, &cell(700, 1500), 0);
        assert_eq!(cols.iter().map(|c| (c.min, c.max)).collect::<Vec<_>>(), [(509, 1125), (192, 375)]);
    }

    fn kind(v: &Value) -> Option<Kind> {
        match v.as_str()? {
            "pct" => Some(Kind::Pct),
            "dxa" => Some(Kind::Dxa),
            _ => Some(Kind::Auto),
        }
    }

    fn ints(v: &Value) -> Vec<i64> {
        v.as_array().unwrap().iter().map(|x| x.as_i64().unwrap()).collect()
    }

    fn col(v: &Value) -> Col {
        let f = |k: &str| v[k].as_i64().unwrap();
        Col { kind: kind(&v["kind"]), min: f("min"), max: f("max"), hard: f("hard"), pct: f("pct"), dxa: f("dxa"), own_pct: false }
    }

    fn same(a: &Col, b: &Col) -> bool {
        (a.kind, a.min, a.max, a.hard, a.pct, a.dxa) == (b.kind, b.min, b.max, b.hard, b.pct, b.dxa)
    }

    /// The acceptance vectors.
    #[test]
    fn acceptance_vectors() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cleanroom/autofit_vectors.jsonl");
        let text = std::fs::read_to_string(path).expect("autofit vectors");
        let (mut runs, mut fails) = (std::collections::BTreeMap::<String, usize>::new(), Vec::new());
        for (line, l) in text.lines().enumerate() {
            let v: Value = serde_json::from_str(l).unwrap();
            let (f, i, o) = (v["f"].as_str().unwrap(), &v["in"], &v["out"]);
            *runs.entry(f.to_string()).or_default() += 1;
            let ok = match f {
                "distribute" => {
                    let mut out = ints(&i["out"]);
                    distribute(&mut out, &ints(&i["floor"]), i["target"].as_i64().unwrap());
                    out == ints(o)
                }
                "resolve" => {
                    let cols: Vec<Col> = i["cols"].as_array().unwrap().iter().map(col).collect();
                    let g = |k: &str| i[k].as_i64().unwrap();
                    let b = |k: &str| i[k].as_bool().unwrap();
                    let r = resolve(&cols, g("pad"), g("tbl"), g("avail"), b("fixed"), b("no_grow"));
                    let want = Resolved {
                        width: o["width"].as_i64().unwrap(),
                        widths: ints(&o["widths"]),
                        min_t: o["min_t"].as_i64().unwrap(),
                        hard_t: o["hard_t"].as_i64().unwrap(),
                    };
                    r == want
                }
                "columns" => {
                    let cells: Vec<Cell> = i["cells"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|c| {
                            let g = |k: &str| c[k].as_i64().unwrap();
                            Cell {
                                first: g("first") as usize,
                                span: g("span") as usize,
                                min: g("min"),
                                max: g("max"),
                                hard: g("hard"),
                                kind: kind(&c["kind"]).unwrap_or(Kind::Auto),
                                pref: g("pref"),
                                row: g("row") as usize,
                                cont: c["cont"].as_bool().unwrap(),
                            }
                        })
                        .collect();
                    let gaps: Vec<Vec<Gap>> = i["gaps"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|r| r.as_array().unwrap().iter().map(|g| (g[0].as_u64().unwrap() as usize, g[1].as_u64().unwrap() as usize, g[2].as_i64().unwrap())).collect())
                        .collect();
                    let got = columns(i["n"].as_u64().unwrap() as usize, &cells, i["sp"].as_i64().unwrap(), &gaps, i["fixed"].as_bool().unwrap());
                    let want: Vec<Col> = o.as_array().unwrap().iter().map(col).collect();
                    got.len() == want.len() && got.iter().zip(&want).all(|(a, b)| same(a, b))
                }
                _ => panic!("unknown function {f}"),
            };
            if !ok {
                fails.push((line + 1, f.to_string()));
            }
        }
        assert_eq!(runs.values().sum::<usize>(), 3400, "{runs:?}");
        assert!(fails.is_empty(), "{} of 3400 differ, first: {:?}", fails.len(), &fails[..fails.len().min(20)]);
    }
}
