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

/// A guide's formula (`ST_GeomGuideFormula`), of its arguments `x`, `y` and `z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Formula {
    /// `x * y / z`.
    MulDiv,
    /// `x + y - z`.
    AddSub,
    /// `(x + y) / z`.
    AddDiv,
    /// `y` if `x` is over 0, else `z`.
    IfElse,
    /// `|x|`.
    Abs,
    /// The angle of the point (`x`, `y`).
    At2,
    /// `x * cos(atan2(z, y))`.
    Cat2,
    /// `x * cos(y)`.
    Cos,
    /// The greater of `x` and `y`.
    Max,
    /// The lesser of `x` and `y`.
    Min,
    /// The length of (`x`, `y`, `z`).
    Mod,
    /// `y` held between `x` and `z`.
    Pin,
    /// `x * sin(atan2(z, y))`.
    Sat2,
    /// `x * sin(y)`.
    Sin,
    /// The square root of `x`.
    Sqrt,
    /// `x * tan(y)`.
    Tan,
    /// `x`.
    Val,
}

impl Formula {
    /// The formula named `s`.
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

/// A value of the shape's box a guide can name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Var {
    /// Its width.
    W,
    /// Its height.
    H,
    /// Its right edge.
    R,
    /// Its bottom edge.
    B,
    /// Its horizontal centre.
    Hc,
    /// Its vertical centre.
    Vc,
    /// Its shorter side.
    Ss,
    /// Its longer side.
    Ls,
    /// Its width divided by this.
    Wd(u8),
    /// Its height divided by this.
    Hd(u8),
    /// Its shorter side divided by this.
    Ssd(u8),
}

impl Var {
    /// The argument a built-in name `s` is: a value of the box, or a constant (`l`, `t`, the `cd` angles).
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

/// A formula's or a path's argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Arg {
    /// A number.
    Const(i32),
    /// A value of the box.
    Var(Var),
    /// An adjust value, by its index.
    Adj(u8),
    /// A guide's value, by its index.
    Guide(u16),
}

/// A guide: a formula of three arguments.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Op {
    /// The formula.
    pub f: Formula,
    /// Its arguments.
    pub a: [Arg; 3],
}

/// A point: its x and y.
pub type Pt = [Arg; 2];

/// An adjust handle: the adjust values it moves, each with its least and greatest, and where it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HandleDef {
    /// Moved across and down.
    Xy {
        /// The adjust value it moves across, with its limits.
        x: Option<(u8, Option<Arg>, Option<Arg>)>,
        /// The adjust value it moves down, with its limits.
        y: Option<(u8, Option<Arg>, Option<Arg>)>,
        /// Where it is.
        pos: Pt,
    },
    /// Moved out from and round a centre.
    Polar {
        /// The adjust value its distance moves, with its limits.
        r: Option<(u8, Option<Arg>, Option<Arg>)>,
        /// The adjust value its angle moves, with its limits.
        ang: Option<(u8, Option<Arg>, Option<Arg>)>,
        /// Where it is.
        pos: Pt,
    },
}

/// A connection site: where a connector can join the shape, and the way it leaves.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CxnDef {
    /// The way a connector leaves.
    pub ang: Arg,
    /// Where it is.
    pub pos: Pt,
}

/// A path command.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cmd {
    /// Moves to a point.
    MoveTo,
    /// A line to a point.
    LineTo,
    /// An arc: its width and height radii, start angle and sweep.
    ArcTo,
    /// A quadratic curve: its control point and end.
    QuadTo,
    /// A cubic curve: its two control points and end.
    CubicTo,
    /// Closes the subpath.
    Close,
}

impl Cmd {
    /// How many arguments it takes.
    pub fn arity(self) -> usize {
        match self {
            Cmd::MoveTo | Cmd::LineTo => 2,
            Cmd::ArcTo | Cmd::QuadTo => 4,
            Cmd::CubicTo => 6,
            Cmd::Close => 0,
        }
    }
}

/// How a path is filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PathFill {
    /// Not filled.
    None,
    /// With the shape's fill.
    #[default]
    Norm,
    /// With the shape's fill lightened.
    Lighten,
    /// With the shape's fill lightened a little.
    LightenLess,
    /// With the shape's fill darkened.
    Darken,
    /// With the shape's fill darkened a little.
    DarkenLess,
}

/// A path of a geometry's definition.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PathDef<'a> {
    /// The width its coordinates span; 0 for the box's own.
    pub w: i64,
    /// The height its coordinates span; 0 for the box's own.
    pub h: i64,
    /// How it is filled.
    pub fill: PathFill,
    /// It is stroked with the shape's line.
    pub stroke: bool,
    /// Its commands.
    pub cmds: &'a [Cmd],
    /// Its commands' arguments, in order.
    pub args: &'a [Arg],
}

/// A geometry's definition: its adjust values, guides, handles, connection sites, text rectangle and paths.
#[derive(Clone, Copy, Debug)]
pub struct GeomDef<'a> {
    /// Its adjust values, by name, with their defaults.
    pub av: &'a [(&'a str, i64)],
    /// Its guides, in order: each may name those before it.
    pub gd: &'a [Op],
    /// Its handles.
    pub ah: &'a [HandleDef],
    /// Its connection sites.
    pub cxn: &'a [CxnDef],
    /// Its text rectangle: left, top, right, bottom; the box when `None`.
    pub rect: Option<[Arg; 4]>,
    /// Its paths.
    pub paths: &'a [PathDef<'a>],
}

/// A custom geometry: a definition of its own.
#[derive(Debug, PartialEq)]
pub struct CustomGeom {
    /// Its adjust values' names.
    pub av_names: Vec<String>,
    /// Its adjust values, in the order of their names.
    pub av: Vec<i64>,
    /// Its guides, in order.
    pub gd: Vec<Op>,
    /// Its handles.
    pub ah: Vec<HandleDef>,
    /// Its connection sites.
    pub cxn: Vec<CxnDef>,
    /// Its text rectangle: left, top, right, bottom; the box when `None`.
    pub rect: Option<[Arg; 4]>,
    /// Its paths: width, height, fill, stroke, commands and their arguments, as [`PathDef`]'s.
    pub paths: Vec<(i64, i64, PathFill, bool, Vec<Cmd>, Vec<Arg>)>,
}

/// A shape's geometry.
#[derive(Clone, Debug, PartialEq)]
pub enum Geometry {
    /// A preset, with the adjust values set by name.
    Preset {
        /// The preset.
        preset: Preset,
        /// Its adjust values set, by name.
        adj: Vec<(String, i64)>,
    },
    /// A custom geometry.
    Custom(Rc<CustomGeom>),
}

impl Default for Geometry {
    fn default() -> Geometry {
        Geometry::Preset { preset: Preset::Rect, adj: Vec::new() }
    }
}

/// A segment of an evaluated path, in points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Seg {
    /// Moves to a point, starting a subpath.
    Move([f64; 2]),
    /// A line to a point.
    Line([f64; 2]),
    /// A quadratic curve: its control point and end.
    Quad([f64; 2], [f64; 2]),
    /// A cubic curve: its two control points and end.
    Cubic([f64; 2], [f64; 2], [f64; 2]),
    /// Closes the subpath.
    Close,
}

/// An evaluated path.
#[derive(Clone, Debug, PartialEq)]
pub struct GeomPath {
    /// Its segments.
    pub segs: Vec<Seg>,
    /// How it is filled.
    pub fill: PathFill,
    /// It is stroked.
    pub stroke: bool,
}

/// An adjust handle, evaluated.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandlePos {
    /// Where it is, in points.
    pub at: [f64; 2],
    /// Its index among the definition's handles.
    pub def_index: usize,
}

/// A geometry evaluated at a size, in points from the box's top left.
#[derive(Clone, Debug, PartialEq)]
pub struct Geom {
    /// Its paths.
    pub paths: Vec<GeomPath>,
    /// Its text rectangle: left, top, right, bottom.
    pub text_rect: [f64; 4],
    /// Its handles.
    pub handles: Vec<HandlePos>,
    /// Its connection sites.
    pub cxn: Vec<[f64; 2]>,
}

impl Geom {
    /// Whether `p` is on it: inside a filled path with `filled`, or within `reach` of a stroked one.
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

impl Geom {
    /// The outline its filled paths make: the flattened subpath enclosing the most area (a line's path when none is
    /// filled).
    pub fn outline(&self) -> Vec<[f64; 2]> {
        let area = |pts: &[[f64; 2]]| {
            let n = pts.len();
            (0..n).map(|i| pts[i][0] * pts[(i + 1) % n][1] - pts[(i + 1) % n][0] * pts[i][1]).sum::<f64>().abs() / 2.0
        };
        let filled = self.paths.iter().any(|p| p.fill != PathFill::None);
        self.paths
            .iter()
            .filter(|p| !filled || p.fill != PathFill::None)
            .flat_map(|p| flatten(&p.segs))
            .map(|(pts, _)| (area(&pts), pts))
            .max_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, pts)| pts)
            .unwrap_or_default()
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
                let mut def = preset.def();
                // The pie's text rectangle takes its inner rectangle's top and right where the definition swaps them.
                if *preset == Preset::Pie {
                    def.rect = def.rect.map(|[l, t, r, b]| [l, r, t, b]);
                }
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

    /// The geometry evaluated at `w` by `h`.
    pub fn evaluate(&self, w: Emu, h: Emu) -> Rc<Geom> {
        self.with_def(|def, adj| Rc::new(eval::evaluate_def(def, adj, w, h)))
    }

    /// The shape shows text: the chart presets and the inverted line take a text box but show none.
    pub fn holds_text(&self) -> bool {
        !matches!(self.preset(), Some(Preset::ChartPlus | Preset::ChartStar | Preset::ChartX | Preset::LineInv))
    }

    /// The preset it is.
    pub fn preset(&self) -> Option<Preset> {
        match self {
            Geometry::Preset { preset, .. } => Some(*preset),
            Geometry::Custom(_) => None,
        }
    }

    /// Its adjust values, each set one or its default, in its definition's order.
    pub fn adjust_values(&self) -> Vec<i64> {
        self.with_def(|_, adj| adj.to_vec())
    }

    /// Its adjust handles.
    pub fn handles(&self) -> Vec<HandleDef> {
        self.with_def(|def, _| def.ah.to_vec())
    }

    /// Its adjust values' names, in its definition's order.
    pub fn adjust_names(&self) -> Vec<String> {
        self.with_def(|def, _| def.av.iter().map(|(n, _)| n.to_string()).collect())
    }

    /// The preset with its adjust values set to `vals`, in its definition's order; `None` for a custom geometry.
    pub fn with_values(&self, vals: &[i64]) -> Option<Geometry> {
        let Geometry::Preset { preset, .. } = self else { return None };
        let adj = self.adjust_names().into_iter().zip(vals.iter().copied()).collect();
        Some(Geometry::Preset { preset: *preset, adj })
    }

    /// The preset at `w` by `h` with its handle `i` dragged as near `to` (points) as its limits let it go.
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
