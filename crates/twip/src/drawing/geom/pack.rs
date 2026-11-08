//! Packed preset definitions

use super::{Arg, Cmd, CxnDef, Formula, GeomDef, HandleDef, Op, PathDef, PathFill, Var};

const FORMULAS: [Formula; 17] = [
    Formula::MulDiv,
    Formula::AddSub,
    Formula::AddDiv,
    Formula::IfElse,
    Formula::Abs,
    Formula::At2,
    Formula::Cat2,
    Formula::Cos,
    Formula::Max,
    Formula::Min,
    Formula::Mod,
    Formula::Pin,
    Formula::Sat2,
    Formula::Sin,
    Formula::Sqrt,
    Formula::Tan,
    Formula::Val,
];
const CMDS: [Cmd; 6] = [Cmd::MoveTo, Cmd::LineTo, Cmd::ArcTo, Cmd::QuadTo, Cmd::CubicTo, Cmd::Close];
const FILLS: [PathFill; 6] = [PathFill::None, PathFill::Norm, PathFill::Lighten, PathFill::LightenLess, PathFill::Darken, PathFill::DarkenLess];

#[cfg_attr(target_os = "emscripten", allow(dead_code))]
fn var_code(v: Var) -> u64 {
    match v {
        Var::W => 0,
        Var::H => 1,
        Var::R => 2,
        Var::B => 3,
        Var::Hc => 4,
        Var::Vc => 5,
        Var::Ss => 6,
        Var::Ls => 7,
        Var::Wd(n) => 8 + 3 * u64::from(n),
        Var::Hd(n) => 9 + 3 * u64::from(n),
        Var::Ssd(n) => 10 + 3 * u64::from(n),
    }
}

fn var_of(c: u64) -> Var {
    match c {
        0 => Var::W,
        1 => Var::H,
        2 => Var::R,
        3 => Var::B,
        4 => Var::Hc,
        5 => Var::Vc,
        6 => Var::Ss,
        7 => Var::Ls,
        c => {
            let n = ((c - 8) / 3) as u8;
            match (c - 8) % 3 {
                0 => Var::Wd(n),
                1 => Var::Hd(n),
                _ => Var::Ssd(n),
            }
        }
    }
}

#[cfg_attr(target_os = "emscripten", allow(dead_code))]
struct Out(Vec<u8>);

#[cfg_attr(target_os = "emscripten", allow(dead_code))]
impl Out {
    fn u(&mut self, mut v: u64) {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                self.0.push(b);
                return;
            }
            self.0.push(b | 0x80);
        }
    }

    fn i(&mut self, v: i64) {
        self.u(((v << 1) ^ (v >> 63)) as u64);
    }

    fn arg(&mut self, a: Arg) {
        let (v, tag) = match a {
            Arg::Const(c) => ((((i64::from(c)) << 1) ^ (i64::from(c) >> 63)) as u64, 0),
            Arg::Var(v) => (var_code(v), 1),
            Arg::Adj(i) => (u64::from(i), 2),
            Arg::Guide(i) => (u64::from(i), 3),
        };
        self.u((v << 2) | tag);
    }

    fn opt(&mut self, a: Option<Arg>) {
        match a {
            Some(a) => {
                self.u(1);
                self.arg(a);
            }
            None => self.u(0),
        }
    }

    fn lim(&mut self, l: Option<(u8, Option<Arg>, Option<Arg>)>) {
        match l {
            Some((i, lo, hi)) => {
                self.u(1);
                self.u(u64::from(i));
                self.opt(lo);
                self.opt(hi);
            }
            None => self.u(0),
        }
    }
}

#[cfg_attr(target_os = "emscripten", allow(dead_code))]
pub fn encode(d: &GeomDef) -> Vec<u8> {
    let mut o = Out(Vec::new());
    o.u(d.av.len() as u64);
    for (n, v) in d.av {
        o.u(n.len() as u64);
        o.0.extend(n.as_bytes());
        o.i(*v);
    }
    o.u(d.gd.len() as u64);
    for op in d.gd {
        o.u(FORMULAS.iter().position(|f| *f == op.f).unwrap_or(0) as u64);
        op.a.iter().for_each(|a| o.arg(*a));
    }
    o.u(d.ah.len() as u64);
    for h in d.ah {
        let (k, a, b, pos) = match *h {
            HandleDef::Xy { x, y, pos } => (0, x, y, pos),
            HandleDef::Polar { r, ang, pos } => (1, r, ang, pos),
        };
        o.u(k);
        o.lim(a);
        o.lim(b);
        pos.iter().for_each(|a| o.arg(*a));
    }
    o.u(d.cxn.len() as u64);
    for c in d.cxn {
        o.arg(c.ang);
        c.pos.iter().for_each(|a| o.arg(*a));
    }
    match d.rect {
        Some(r) => {
            o.u(1);
            r.iter().for_each(|a| o.arg(*a));
        }
        None => o.u(0),
    }
    o.u(d.paths.len() as u64);
    for p in d.paths {
        o.i(p.w);
        o.i(p.h);
        o.u(FILLS.iter().position(|f| *f == p.fill).unwrap_or(1) as u64);
        o.u(u64::from(p.stroke));
        o.u(p.cmds.len() as u64);
        p.cmds.iter().for_each(|c| o.u(CMDS.iter().position(|k| k == c).unwrap_or(0) as u64));
        o.u(p.args.len() as u64);
        p.args.iter().for_each(|a| o.arg(*a));
    }
    o.0
}

struct In<'b> {
    b: &'b [u8],
    at: usize,
}

impl In<'_> {
    fn u(&mut self) -> u64 {
        let (mut v, mut s) = (0u64, 0);
        while let Some(&b) = self.b.get(self.at) {
            self.at += 1;
            v |= u64::from(b & 0x7f) << s;
            s += 7;
            if b < 0x80 {
                break;
            }
        }
        v
    }

    fn i(&mut self) -> i64 {
        let v = self.u();
        ((v >> 1) as i64) ^ -((v & 1) as i64)
    }

    fn n(&mut self) -> usize {
        self.u() as usize
    }

    fn arg(&mut self) -> Arg {
        let v = self.u();
        let x = v >> 2;
        match v & 3 {
            0 => Arg::Const((((x >> 1) as i64) ^ -((x & 1) as i64)) as i32),
            1 => Arg::Var(var_of(x)),
            2 => Arg::Adj(x as u8),
            _ => Arg::Guide(x as u16),
        }
    }

    fn opt(&mut self) -> Option<Arg> {
        (self.u() == 1).then(|| self.arg())
    }

    fn lim(&mut self) -> Option<(u8, Option<Arg>, Option<Arg>)> {
        (self.u() == 1).then(|| {
            let i = self.u() as u8;
            let lo = self.opt();
            (i, lo, self.opt())
        })
    }
}

fn leak<T>(v: Vec<T>) -> &'static [T] {
    Box::leak(v.into_boxed_slice())
}

pub fn decode(b: &[u8]) -> GeomDef<'static> {
    let mut r = In { b, at: 0 };
    let av = (0..r.n())
        .map(|_| {
            let len = r.n();
            let name = std::str::from_utf8(&r.b[r.at..r.at + len]).unwrap_or("");
            let name: &'static str = Box::leak(name.to_string().into_boxed_str());
            r.at += len;
            (name, r.i())
        })
        .collect();
    let gd = (0..r.n())
        .map(|_| {
            let f = FORMULAS.get(r.n()).copied().unwrap_or(Formula::Val);
            Op { f, a: [r.arg(), r.arg(), r.arg()] }
        })
        .collect();
    let ah = (0..r.n())
        .map(|_| {
            let k = r.u();
            let (a, b) = (r.lim(), r.lim());
            let pos = [r.arg(), r.arg()];
            if k == 0 { HandleDef::Xy { x: a, y: b, pos } } else { HandleDef::Polar { r: a, ang: b, pos } }
        })
        .collect();
    let cxn = (0..r.n()).map(|_| CxnDef { ang: r.arg(), pos: [r.arg(), r.arg()] }).collect();
    let rect = (r.u() == 1).then(|| [r.arg(), r.arg(), r.arg(), r.arg()]);
    let paths = (0..r.n())
        .map(|_| {
            let (w, h) = (r.i(), r.i());
            let fill = FILLS.get(r.n()).copied().unwrap_or_default();
            let stroke = r.u() == 1;
            let cmds = leak((0..r.n()).map(|_| CMDS.get(r.n()).copied().unwrap_or(Cmd::Close)).collect());
            let args = leak((0..r.n()).map(|_| r.arg()).collect());
            PathDef { w, h, fill, stroke, cmds, args }
        })
        .collect();
    GeomDef { av: leak(av), gd: leak(gd), ah: leak(ah), cxn: leak(cxn), rect, paths: leak(paths) }
}
