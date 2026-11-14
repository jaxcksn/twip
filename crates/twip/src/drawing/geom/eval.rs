//! Geometry compiling and evaluation

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use roxmltree::Node;

use super::{Arg, Cmd, CustomGeom, CxnDef, Formula, Geom, GeomDef, GeomPath, HandleDef, HandlePos, Op, PathDef, PathFill, Pt, Seg, Var};
use crate::drawing::units::{EMU_PER_PT, Emu};

// Compiling

struct Scope {
    av: Vec<String>,
    gd: Vec<String>,
}

impl Scope {
    fn arg(&self, s: &str) -> Arg {
        if let Some(i) = self.gd.iter().rposition(|n| n == s) {
            return Arg::Guide(i as u16);
        }
        if let Some(i) = self.av.iter().position(|n| n == s) {
            return Arg::Adj(i as u8);
        }
        if let Some(a) = Var::parse(s) {
            return a;
        }
        let v = s.trim().parse::<i64>().or_else(|_| s.trim().parse::<f64>().map(|v| v.round() as i64)).unwrap_or(0);
        Arg::Const(v.clamp(i32::MIN.into(), i32::MAX.into()) as i32)
    }

    fn opt(&self, s: Option<&str>) -> Option<Arg> {
        s.map(|s| self.arg(s))
    }

    fn adj(&self, s: Option<&str>) -> Option<u8> {
        let s = s?;
        self.av.iter().position(|n| n == s).map(|i| i as u8)
    }
}

fn el<'a, 'i>(n: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    n.children().find(|c| c.is_element() && c.tag_name().name() == name)
}

fn els<'a, 'i>(n: Node<'a, 'i>, name: &'static str) -> impl Iterator<Item = Node<'a, 'i>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == name)
}

fn pt(scope: &Scope, n: Option<Node>) -> Pt {
    let n = n.and_then(|n| if n.tag_name().name() == "pt" || n.tag_name().name() == "pos" { Some(n) } else { el(n, "pt") });
    [scope.arg(n.and_then(|p| p.attribute("x")).unwrap_or("0")), scope.arg(n.and_then(|p| p.attribute("y")).unwrap_or("0"))]
}

impl CustomGeom {
    pub fn parse(n: Node) -> CustomGeom {
        let mut scope = Scope { av: Vec::new(), gd: Vec::new() };
        let mut av = Vec::new();
        for gd in el(n, "avLst").into_iter().flat_map(|l| els(l, "gd")) {
            let v = gd.attribute("fmla").and_then(|f| f.trim().strip_prefix("val ")).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
            scope.av.push(gd.attribute("name").unwrap_or("").to_string());
            av.push(v);
        }
        let mut ops = Vec::new();
        for gd in el(n, "gdLst").into_iter().flat_map(|l| els(l, "gd")) {
            let fmla = gd.attribute("fmla").unwrap_or("val 0");
            let mut parts = fmla.split_whitespace();
            let f = parts.next().and_then(Formula::parse).unwrap_or(Formula::Val);
            let args: Vec<Arg> = parts.map(|p| scope.arg(p)).collect();
            let a = |i: usize| args.get(i).copied().unwrap_or(Arg::Const(0));
            ops.push(Op { f, a: [a(0), a(1), a(2)] });
            scope.gd.push(gd.attribute("name").unwrap_or("").to_string());
        }
        let lim =
            |h: Node, r: &str, lo: &str, hi: &str| scope.adj(h.attribute(r)).map(|i| (i, scope.opt(h.attribute(lo)), scope.opt(h.attribute(hi))));
        let ah = el(n, "ahLst")
            .into_iter()
            .flat_map(|l| l.children().filter(Node::is_element))
            .filter_map(|h| match h.tag_name().name() {
                "ahXY" => {
                    Some(HandleDef::Xy { x: lim(h, "gdRefX", "minX", "maxX"), y: lim(h, "gdRefY", "minY", "maxY"), pos: pt(&scope, el(h, "pos")) })
                }
                "ahPolar" => Some(HandleDef::Polar {
                    r: lim(h, "gdRefR", "minR", "maxR"),
                    ang: lim(h, "gdRefAng", "minAng", "maxAng"),
                    pos: pt(&scope, el(h, "pos")),
                }),
                _ => None,
            })
            .collect();
        let cxn = el(n, "cxnLst")
            .into_iter()
            .flat_map(|l| els(l, "cxn"))
            .map(|c| CxnDef { ang: scope.arg(c.attribute("ang").unwrap_or("0")), pos: pt(&scope, el(c, "pos")) })
            .collect();
        let rect = el(n, "rect").map(|r| ["l", "t", "r", "b"].map(|k| scope.arg(r.attribute(k).unwrap_or("0"))));
        let paths = el(n, "pathLst")
            .into_iter()
            .flat_map(|l| els(l, "path"))
            .map(|p| {
                let num = |k: &str| p.attribute(k).and_then(|v| v.parse::<i64>().ok()).unwrap_or(0);
                let fill = match p.attribute("fill") {
                    Some("none") => PathFill::None,
                    Some("lighten") => PathFill::Lighten,
                    Some("lightenLess") => PathFill::LightenLess,
                    Some("darken") => PathFill::Darken,
                    Some("darkenLess") => PathFill::DarkenLess,
                    _ => PathFill::Norm,
                };
                let stroke = !matches!(p.attribute("stroke"), Some("0" | "false"));
                let mut cmds = Vec::new();
                let mut args = Vec::new();
                for c in p.children().filter(Node::is_element) {
                    let cmd = match c.tag_name().name() {
                        "moveTo" => Cmd::MoveTo,
                        "lnTo" => Cmd::LineTo,
                        "arcTo" => Cmd::ArcTo,
                        "quadBezTo" => Cmd::QuadTo,
                        "cubicBezTo" => Cmd::CubicTo,
                        "close" => Cmd::Close,
                        _ => continue,
                    };
                    match cmd {
                        Cmd::ArcTo => args.extend(["wR", "hR", "stAng", "swAng"].map(|k| scope.arg(c.attribute(k).unwrap_or("0")))),
                        Cmd::Close => {}
                        _ => {
                            let mut qs = els(c, "pt");
                            for _ in 0..cmd.arity() / 2 {
                                args.extend(pt(&scope, qs.next()));
                            }
                        }
                    }
                    cmds.push(cmd);
                }
                (num("w"), num("h"), fill, stroke, cmds, args)
            })
            .collect();
        CustomGeom { av_names: scope.av, av, gd: ops, ah, cxn, rect, paths }
    }

    #[cfg(not(target_family = "wasm"))]
    pub fn to_bytes(&self) -> Vec<u8> {
        let names: Vec<(&str, i64)> = self.av_names.iter().zip(&self.av).map(|(n, v)| (n.as_str(), *v)).collect();
        let paths: Vec<super::PathDef> = self
            .paths
            .iter()
            .map(|(w, h, fill, stroke, cmds, args)| super::PathDef { w: *w, h: *h, fill: *fill, stroke: *stroke, cmds, args })
            .collect();
        super::pack::encode(&GeomDef { av: &names, gd: &self.gd, ah: &self.ah, cxn: &self.cxn, rect: self.rect, paths: &paths })
    }
}

// Evaluating

fn rad(a: f64) -> f64 {
    a / 60000.0 * PI / 180.0
}

fn apply(f: Formula, x: f64, y: f64, z: f64) -> f64 {
    let div = |a: f64, b: f64| if b == 0.0 { 0.0 } else { a / b };
    let at2 = |x: f64, y: f64| y.atan2(x) * 180.0 / PI * 60000.0;
    let v = match f {
        Formula::MulDiv => div(x * y, z),
        Formula::AddSub => x + y - z,
        Formula::AddDiv => div(x + y, z),
        Formula::IfElse => {
            if x > 0.0 {
                y
            } else {
                z
            }
        }
        Formula::Abs => x.abs(),
        Formula::At2 => at2(x, y),
        Formula::Cat2 => x * z.atan2(y).cos(),
        Formula::Cos => x * rad(y).cos(),
        Formula::Max => x.max(y),
        Formula::Min => x.min(y),
        Formula::Mod => (x * x + y * y + z * z).sqrt(),
        Formula::Pin => {
            if y < x {
                x
            } else if y > z {
                z
            } else {
                y
            }
        }
        Formula::Sat2 => x * z.atan2(y).sin(),
        Formula::Sin => x * rad(y).sin(),
        Formula::Sqrt => x.max(0.0).sqrt(),
        Formula::Tan => x * rad(y).tan(),
        Formula::Val => x,
    };
    if v.is_finite() { v } else { 0.0 }
}

struct Env<'a> {
    w: f64,
    h: f64,
    adj: &'a [i64],
    gd: Vec<f64>,
}

impl Env<'_> {
    fn val(&self, a: Arg) -> f64 {
        match a {
            Arg::Const(c) => f64::from(c),
            Arg::Adj(i) => self.adj.get(i as usize).copied().unwrap_or(0) as f64,
            Arg::Guide(i) => self.gd.get(i as usize).copied().unwrap_or(0.0),
            Arg::Var(v) => {
                let ss = self.w.min(self.h);
                let n = |d: u8| f64::from(d.max(1));
                match v {
                    Var::W | Var::R => self.w,
                    Var::H | Var::B => self.h,
                    Var::Hc => self.w / 2.0,
                    Var::Vc => self.h / 2.0,
                    Var::Ss => ss,
                    Var::Ls => self.w.max(self.h),
                    Var::Wd(d) => self.w / n(d),
                    Var::Hd(d) => self.h / n(d),
                    Var::Ssd(d) => ss / n(d),
                }
            }
        }
    }
}

struct Pen {
    sx: f64,
    sy: f64,
    cur: [f64; 2],
    start: [f64; 2],
    segs: Vec<Seg>,
}

impl Pen {
    fn at(&self, e: &Env, p: &Pt) -> [f64; 2] {
        [e.val(p[0]) * self.sx, e.val(p[1]) * self.sy]
    }
}

const PT: f64 = EMU_PER_PT as f64;

fn arc(pen: &mut Pen, wr: f64, hr: f64, ratio: f64, st: f64, sw: f64) {
    let param = |a: f64| (ratio * a.sin()).atan2(a.cos());
    let (sa, swa) = (rad(st), rad(sw));
    let t0 = param(sa);
    let mut sweep = param(sa + swa) - t0;
    if swa > 0.0 && sweep <= 0.0 {
        sweep += TAU;
    } else if swa < 0.0 && sweep >= 0.0 {
        sweep -= TAU;
    }
    if swa.abs() >= TAU {
        sweep += TAU * (swa / TAU).trunc() - if swa > 0.0 { TAU } else { -TAU } * f64::from(u8::from(sweep.abs() > 1e-9));
    }
    if swa == 0.0 {
        sweep = 0.0;
    }
    let c = [pen.cur[0] - wr * t0.cos(), pen.cur[1] - hr * t0.sin()];
    let n = (sweep.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
    let step = sweep / n as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = t0;
    let p = |t: f64| [c[0] + wr * t.cos(), c[1] + hr * t.sin()];
    let d = |t: f64| [-wr * t.sin(), hr * t.cos()];
    for _ in 0..n {
        let b = a + step;
        let (p0, p3, d0, d3) = (p(a), p(b), d(a), d(b));
        let c1 = [p0[0] + k * d0[0], p0[1] + k * d0[1]];
        let c2 = [p3[0] - k * d3[0], p3[1] - k * d3[1]];
        pen.segs.push(Seg::Cubic(c1, c2, p3));
        a = b;
    }
    pen.cur = p(t0 + sweep);
}

pub fn eval_args(def: &GeomDef, adj: &[i64], w: Emu, h: Emu, args: &[Arg]) -> Vec<f64> {
    let mut e = Env { w: w.0 as f64, h: h.0 as f64, adj, gd: Vec::with_capacity(def.gd.len()) };
    for op in def.gd {
        let v = apply(op.f, e.val(op.a[0]), e.val(op.a[1]), e.val(op.a[2]));
        e.gd.push(v);
    }
    args.iter().map(|a| e.val(*a)).collect()
}

pub fn evaluate_def(def: &GeomDef, adj: &[i64], w: Emu, h: Emu) -> Geom {
    let mut e = Env { w: w.0 as f64, h: h.0 as f64, adj, gd: Vec::with_capacity(def.gd.len()) };
    for op in def.gd {
        let v = apply(op.f, e.val(op.a[0]), e.val(op.a[1]), e.val(op.a[2]));
        e.gd.push(v);
    }
    let paths = def
        .paths
        .iter()
        .map(|p: &PathDef| {
            let sx = if p.w > 0 { e.w / p.w as f64 } else { 1.0 } / PT;
            let sy = if p.h > 0 { e.h / p.h as f64 } else { 1.0 } / PT;
            let mut pen = Pen { sx, sy, cur: [0.0; 2], start: [0.0; 2], segs: Vec::new() };
            let mut args = p.args.iter().copied();
            let mut next = move || args.next().unwrap_or(Arg::Const(0));
            for c in p.cmds {
                match c {
                    Cmd::MoveTo => {
                        let at = pen.at(&e, &[next(), next()]);
                        pen.segs.push(Seg::Move(at));
                        (pen.cur, pen.start) = (at, at);
                    }
                    Cmd::LineTo => {
                        let at = pen.at(&e, &[next(), next()]);
                        pen.segs.push(Seg::Line(at));
                        pen.cur = at;
                    }
                    Cmd::ArcTo => {
                        let [wr, hr, st, sw] = [next(), next(), next(), next()].map(|a| e.val(a));
                        let ratio = if hr == 0.0 { 0.0 } else { wr / hr };
                        let (wr, hr) = (wr * pen.sx, hr * pen.sy);
                        arc(&mut pen, wr, hr, ratio, st, sw);
                    }
                    Cmd::QuadTo => {
                        let a = pen.at(&e, &[next(), next()]);
                        let b = pen.at(&e, &[next(), next()]);
                        pen.segs.push(Seg::Quad(a, b));
                        pen.cur = b;
                    }
                    Cmd::CubicTo => {
                        let a = pen.at(&e, &[next(), next()]);
                        let b = pen.at(&e, &[next(), next()]);
                        let c = pen.at(&e, &[next(), next()]);
                        pen.segs.push(Seg::Cubic(a, b, c));
                        pen.cur = c;
                    }
                    Cmd::Close => {
                        pen.segs.push(Seg::Close);
                        pen.cur = pen.start;
                    }
                }
            }
            GeomPath { segs: pen.segs, fill: p.fill, stroke: p.stroke }
        })
        .collect();
    let to_pt = |v: f64| v / PT;
    let text_rect = def.rect.map_or([0.0, 0.0, to_pt(e.w), to_pt(e.h)], |r| r.map(|a| to_pt(e.val(a))));
    let handles = def
        .ah
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let pos = match h {
                HandleDef::Xy { pos, .. } | HandleDef::Polar { pos, .. } => pos,
            };
            HandlePos { at: [to_pt(e.val(pos[0])), to_pt(e.val(pos[1]))], def_index: i }
        })
        .collect();
    let cxn = def.cxn.iter().map(|c| [to_pt(e.val(c.pos[0])), to_pt(e.val(c.pos[1]))]).collect();
    Geom { paths, text_rect, handles, cxn }
}
