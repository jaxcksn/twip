//! Shape geometry

pub mod ends;
mod eval;
mod pack;
mod presets;
#[cfg(test)]
mod tests;

use std::rc::Rc;

pub use presets::Preset;

use super::units::Emu;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Formula {
    MulDiv,
    AddSub,
    AddDiv,
    IfElse,
    Abs,
    At2,
    Cat2,
    Cos,
    Max,
    Min,
    Mod,
    Pin,
    Sat2,
    Sin,
    Sqrt,
    Tan,
    Val,
}

impl Formula {
    pub fn parse(s: &str) -> Option<Formula> {
        use Formula::*;
        Some(match s {
            "*/" => MulDiv,
            "+-" => AddSub,
            "+/" => AddDiv,
            "?:" => IfElse,
            "abs" => Abs,
            "at2" => At2,
            "cat2" => Cat2,
            "cos" => Cos,
            "max" => Max,
            "min" => Min,
            "mod" => Mod,
            "pin" => Pin,
            "sat2" => Sat2,
            "sin" => Sin,
            "sqrt" => Sqrt,
            "tan" => Tan,
            "val" => Val,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Var {
    W,
    H,
    R,
    B,
    Hc,
    Vc,
    Ss,
    Ls,
    Wd(u8),
    Hd(u8),
    Ssd(u8),
}

impl Var {
    pub fn parse(s: &str) -> Option<Arg> {
        use Var::*;
        let n = |p: &str| s.strip_prefix(p).and_then(|d| d.parse::<u8>().ok());
        Some(Arg::Var(match s {
            "w" => W,
            "h" => H,
            "r" => R,
            "b" => B,
            "hc" => Hc,
            "vc" => Vc,
            "ss" => Ss,
            "ls" => Ls,
            "l" | "t" => return Some(Arg::Const(0)),
            "cd2" => return Some(Arg::Const(10_800_000)),
            "cd4" => return Some(Arg::Const(5_400_000)),
            "cd8" => return Some(Arg::Const(2_700_000)),
            "3cd4" => return Some(Arg::Const(16_200_000)),
            "3cd8" => return Some(Arg::Const(8_100_000)),
            "5cd8" => return Some(Arg::Const(13_500_000)),
            "7cd8" => return Some(Arg::Const(18_900_000)),
            _ if s.starts_with("ssd") => Ssd(n("ssd")?),
            _ if s.starts_with("wd") => Wd(n("wd")?),
            _ if s.starts_with("hd") => Hd(n("hd")?),
            _ => return None,
        }))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arg {
    Const(i32),
    Var(Var),
    Adj(u8),
    Guide(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Op {
    pub f: Formula,
    pub a: [Arg; 3],
}

pub type Pt = [Arg; 2];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleDef {
    Xy { x: Option<(u8, Option<Arg>, Option<Arg>)>, y: Option<(u8, Option<Arg>, Option<Arg>)>, pos: Pt },
    Polar { r: Option<(u8, Option<Arg>, Option<Arg>)>, ang: Option<(u8, Option<Arg>, Option<Arg>)>, pos: Pt },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CxnDef {
    pub ang: Arg,
    pub pos: Pt,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    MoveTo,
    LineTo,
    ArcTo,
    QuadTo,
    CubicTo,
    Close,
}

impl Cmd {
    pub fn arity(self) -> usize {
        match self {
            Cmd::MoveTo | Cmd::LineTo => 2,
            Cmd::ArcTo | Cmd::QuadTo => 4,
            Cmd::CubicTo => 6,
            Cmd::Close => 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PathFill {
    None,
    #[default]
    Norm,
    Lighten,
    LightenLess,
    Darken,
    DarkenLess,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathDef<'a> {
    pub w: i64,
    pub h: i64,
    pub fill: PathFill,
    pub stroke: bool,
    pub cmds: &'a [Cmd],
    pub args: &'a [Arg],
}

#[derive(Clone, Copy, Debug)]
pub struct GeomDef<'a> {
    pub av: &'a [(&'a str, i64)],
    pub gd: &'a [Op],
    pub ah: &'a [HandleDef],
    pub cxn: &'a [CxnDef],
    pub rect: Option<[Arg; 4]>,
    pub paths: &'a [PathDef<'a>],
}

#[derive(Debug, PartialEq)]
pub struct CustomGeom {
    av_names: Vec<String>,
    av: Vec<i64>,
    gd: Vec<Op>,
    ah: Vec<HandleDef>,
    cxn: Vec<CxnDef>,
    rect: Option<[Arg; 4]>,
    paths: Vec<(i64, i64, PathFill, bool, Vec<Cmd>, Vec<Arg>)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    Preset { preset: Preset, adj: Vec<(String, i64)> },
    Custom(Rc<CustomGeom>),
}

impl Default for Geometry {
    fn default() -> Geometry {
        Geometry::Preset { preset: Preset::Rect, adj: Vec::new() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    Move([f64; 2]),
    Line([f64; 2]),
    Quad([f64; 2], [f64; 2]),
    Cubic([f64; 2], [f64; 2], [f64; 2]),
    Close,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeomPath {
    pub segs: Vec<Seg>,
    pub fill: PathFill,
    pub stroke: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandlePos {
    pub at: [f64; 2],
    pub def_index: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Geom {
    pub paths: Vec<GeomPath>,
    pub text_rect: [f64; 4],
    pub handles: Vec<HandlePos>,
    pub cxn: Vec<[f64; 2]>,
}

impl Geom {
    pub fn hit(&self, p: [f64; 2], filled: bool, reach: Option<f64>) -> bool {
        self.paths.iter().any(|gp| {
            let subs = flatten(&gp.segs);
            let inside = || {
                let mut odd = false;
                for (pts, _) in &subs {
                    for (i, a) in pts.iter().enumerate() {
                        let b = pts[(i + 1) % pts.len()];
                        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
                            odd = !odd;
                        }
                    }
                }
                odd
            };
            let near = |r: f64| {
                subs.iter().any(|(pts, closed)| {
                    let n = if *closed { pts.len() } else { pts.len().saturating_sub(1) };
                    (0..n).any(|i| seg_distance(p, pts[i], pts[(i + 1) % pts.len()]) <= r) || pts.len() == 1 && seg_distance(p, pts[0], pts[0]) <= r
                })
            };
            filled && gp.fill != PathFill::None && inside() || gp.stroke && reach.is_some_and(near)
        })
    }
}

fn flatten(segs: &[Seg]) -> Vec<(Vec<[f64; 2]>, bool)> {
    let mut out: Vec<(Vec<[f64; 2]>, bool)> = Vec::new();
    let mut at = [0.0, 0.0];
    for s in segs {
        match *s {
            Seg::Move(p) => {
                out.push((vec![p], false));
                at = p;
                continue;
            }
            Seg::Close => {
                if let Some(last) = out.last_mut() {
                    last.1 = true;
                }
                continue;
            }
            _ => {}
        }
        if out.is_empty() {
            out.push((vec![at], false));
        }
        let pts = &mut out.last_mut().expect("a subpath").0;
        match *s {
            Seg::Line(p) => pts.push(p),
            Seg::Quad(c, p) => pts.extend((1..=16).map(|i| {
                let t = i as f64 / 16.0;
                let u = 1.0 - t;
                [u * u * at[0] + 2.0 * u * t * c[0] + t * t * p[0], u * u * at[1] + 2.0 * u * t * c[1] + t * t * p[1]]
            })),
            Seg::Cubic(c1, c2, p) => pts.extend((1..=16).map(|i| {
                let t = i as f64 / 16.0;
                let u = 1.0 - t;
                let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
                [a * at[0] + b * c1[0] + c * c2[0] + d * p[0], a * at[1] + b * c1[1] + c * c2[1] + d * p[1]]
            })),
            Seg::Move(_) | Seg::Close => {}
        }
        at = *pts.last().expect("a point");
    }
    out
}

fn seg_distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / l2).clamp(0.0, 1.0) };
    ((p[0] - a[0] - t * dx).powi(2) + (p[1] - a[1] - t * dy).powi(2)).sqrt()
}

impl Geometry {
    fn with_def<R>(&self, f: impl FnOnce(&GeomDef, &[i64]) -> R) -> R {
        match self {
            Geometry::Preset { preset, adj } => {
                let def = preset.def();
                let vals: Vec<i64> = def.av.iter().map(|(n, d)| adj.iter().find(|(k, _)| k == n).map_or(*d, |(_, v)| *v)).collect();
                if let Some((body, body_adj)) = tailless_body(*preset, &vals) {
                    return f(&body.def(), &body_adj);
                }
                f(&def, &vals)
            }
            Geometry::Custom(c) => {
                let names: Vec<(&str, i64)> = c.av_names.iter().zip(&c.av).map(|(n, v)| (n.as_str(), *v)).collect();
                let paths: Vec<PathDef> = c
                    .paths
                    .iter()
                    .map(|(w, h, fill, stroke, cmds, args)| PathDef { w: *w, h: *h, fill: *fill, stroke: *stroke, cmds, args })
                    .collect();
                let def = GeomDef { av: &names, gd: &c.gd, ah: &c.ah, cxn: &c.cxn, rect: c.rect, paths: &paths };
                f(&def, &c.av)
            }
        }
    }

    pub fn evaluate(&self, w: Emu, h: Emu) -> Rc<Geom> {
        self.with_def(|def, adj| Rc::new(eval::evaluate_def(def, adj, w, h)))
    }

    pub fn preset(&self) -> Option<Preset> {
        match self {
            Geometry::Preset { preset, .. } => Some(*preset),
            Geometry::Custom(_) => None,
        }
    }

    pub fn adjust_values(&self) -> Vec<i64> {
        self.with_def(|_, adj| adj.to_vec())
    }

    pub fn handles(&self) -> Vec<HandleDef> {
        self.with_def(|def, _| def.ah.to_vec())
    }

    pub fn adjust_names(&self) -> Vec<String> {
        self.with_def(|def, _| def.av.iter().map(|(n, _)| n.to_string()).collect())
    }

    pub fn with_values(&self, vals: &[i64]) -> Option<Geometry> {
        let Geometry::Preset { preset, .. } = self else { return None };
        let adj = self.adjust_names().into_iter().zip(vals.iter().copied()).collect();
        Some(Geometry::Preset { preset: *preset, adj })
    }

    pub fn drag_handle(&self, i: usize, w: Emu, h: Emu, to: [f64; 2]) -> Option<Geometry> {
        let axes: Vec<(u8, Option<Arg>, Option<Arg>)> = match self.handles().get(i)? {
            HandleDef::Xy { x, y, .. } => [*x, *y],
            HandleDef::Polar { r, ang, .. } => [*r, *ang],
        }
        .into_iter()
        .flatten()
        .collect();
        let mut g = self.with_values(&self.adjust_values())?;
        let miss = |g: &Geometry| g.evaluate(w, h).handles.get(i).map_or(f64::MAX, |p| (p.at[0] - to[0]).powi(2) + (p.at[1] - to[1]).powi(2));
        for _ in 0..2 {
            for &(k, lo, hi) in &axes {
                let mut vals = g.adjust_values();
                let Some(&now) = vals.get(k as usize) else { continue };
                let lim = g.with_def(|def, adj| eval::eval_args(def, adj, w, h, &[lo.unwrap_or(Arg::Const(0)), hi.unwrap_or(Arg::Const(0))]));
                let lo = lo.map_or(now as f64 - 200000.0, |_| lim[0]);
                let hi = hi.map_or(now as f64 + 200000.0, |_| lim[1]).max(lo);
                let at = |v: f64, vals: &mut Vec<i64>| {
                    vals[k as usize] = v.round() as i64;
                    g.with_values(vals).map_or(f64::MAX, |c| miss(&c))
                };
                let (mut best, mut d) = (now as f64, at(now as f64, &mut vals));
                for s in 0..=32 {
                    let v = lo + (hi - lo) * f64::from(s) / 32.0;
                    let dv = at(v, &mut vals);
                    if dv < d {
                        (best, d) = (v, dv);
                    }
                }
                let mut step = (hi - lo) / 32.0;
                while step > 0.5 {
                    for v in [best - step, best + step] {
                        let v = v.clamp(lo, hi);
                        let dv = at(v, &mut vals);
                        if dv < d {
                            (best, d) = (v, dv);
                        }
                    }
                    step /= 2.0;
                }
                vals[k as usize] = best.round() as i64;
                g = g.with_values(&vals)?;
            }
        }
        Some(g)
    }
}

fn tailless_body(p: Preset, adj: &[i64]) -> Option<(Preset, Vec<i64>)> {
    let (x, y) = (*adj.first()? as f64 / 50000.0, *adj.get(1)? as f64 / 50000.0);
    match p.name() {
        "wedgeRectCallout" if x.abs() < 1.0 && y.abs() < 1.0 => Some((Preset::from_name("rect")?, Vec::new())),
        "wedgeRoundRectCallout" if x.abs() < 1.0 && y.abs() < 1.0 => Some((Preset::from_name("roundRect")?, vec![*adj.get(2)?])),
        "wedgeEllipseCallout" if x * x + y * y < 1.0 => Some((Preset::from_name("ellipse")?, Vec::new())),
        _ => None,
    }
}
