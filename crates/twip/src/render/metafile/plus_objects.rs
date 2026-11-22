//! EMF+ objects: brushes, pens, paths, regions, images, fonts, string formats and image attributes, read from their
//! serialised forms. Every count is checked against the bytes before anything is kept.

use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, BezPath, Point, Vec2};

use super::MAX_PATH_POINTS;

/// The most nodes a region holds.
const MAX_REGION_NODES: usize = 1 << 14;

/// A reader of little-endian fields, each checked against the bytes.
#[derive(Clone, Copy)]
pub(super) struct Cur<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Cur<'a> {
    pub(super) fn new(d: &'a [u8]) -> Cur<'a> {
        Cur { d, at: 0 }
    }
    pub(super) fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn arr<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }
    pub(super) fn u8(&mut self) -> Option<u8> {
        self.arr::<1>().map(|b| b[0])
    }
    pub(super) fn u16(&mut self) -> Option<u16> {
        self.arr().map(u16::from_le_bytes)
    }
    pub(super) fn i16(&mut self) -> Option<i16> {
        self.arr().map(i16::from_le_bytes)
    }
    pub(super) fn u32(&mut self) -> Option<u32> {
        self.arr().map(u32::from_le_bytes)
    }
    pub(super) fn i32(&mut self) -> Option<i32> {
        self.arr().map(i32::from_le_bytes)
    }
    /// A float; `None` when it isn't finite.
    pub(super) fn f32(&mut self) -> Option<f64> {
        self.arr().map(f32::from_le_bytes).map(f64::from).filter(|v| v.is_finite())
    }
    pub(super) fn point(&mut self) -> Option<Point> {
        Some(Point::new(self.f32()?, self.f32()?))
    }
    /// A rect as its corner and size: floats, or 16-bit integers when `short`.
    pub(super) fn rect(&mut self, short: bool) -> Option<kurbo::Rect> {
        let mut v = [0.0; 4];
        for x in &mut v {
            *x = if short { f64::from(self.i16()?) } else { self.f32()? };
        }
        Some(kurbo::Rect::new(v[0], v[1], v[0] + v[2], v[1] + v[3]))
    }
    /// A matrix `[m11, m12, m21, m22, dx, dy]` (row vectors, as GDI+ writes it).
    pub(super) fn matrix(&mut self) -> Option<Affine> {
        let mut v = [0.0; 6];
        for x in &mut v {
            *x = self.f32()?;
        }
        Some(Affine::new(v))
    }
    pub(super) fn left(&self) -> usize {
        self.d.len() - self.at
    }
    pub(super) fn rest(&mut self) -> &'a [u8] {
        let r = &self.d[self.at..];
        self.at = self.d.len();
        r
    }
    /// A length-prefixed block of bytes.
    fn block(&mut self) -> Option<&'a [u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

/// An `EmfPlusARGB` as unpremultiplied colour.
pub(super) fn argb(v: u32) -> [f32; 4] {
    let ch = |s: u32| ((v >> s) & 0xFF) as f32 / 255.0;
    [ch(16), ch(8), ch(0), ch(24)]
}

/// Point flags: relative (`EmfPlusPointR`) and 16-bit points.
pub(super) const RELATIVE: u32 = 0x0800;
pub(super) const SHORT: u32 = 0x4000;

/// `n` points: relative (`EmfPlusPointR`), 16-bit or float, as `flags` says.
pub(super) fn points(c: &mut Cur<'_>, n: u32, flags: u32) -> Option<Vec<Point>> {
    let n = n as usize;
    let least = if flags & RELATIVE != 0 {
        2
    } else if flags & SHORT != 0 {
        4
    } else {
        8
    };
    if n > c.left() / least || n > MAX_PATH_POINTS {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    let mut at = Point::ZERO;
    for _ in 0..n {
        let q = if flags & RELATIVE != 0 {
            at += Vec2::new(f64::from(integer(c)?), f64::from(integer(c)?));
            at
        } else if flags & SHORT != 0 {
            Point::new(f64::from(c.i16()?), f64::from(c.i16()?))
        } else {
            c.point()?
        };
        out.push(q);
    }
    Some(out)
}

/// An `EmfPlusInteger7` or `EmfPlusInteger15`: a byte whose top bit is clear holds 7 signed bits; set, it and the
/// next byte hold 15.
fn integer(c: &mut Cur<'_>) -> Option<i32> {
    let b = c.u8()?;
    if b & 0x80 == 0 {
        Some(i32::from(((b << 1) as i8) >> 1))
    } else {
        let v = u16::from(b & 0x7F) << 8 | u16::from(c.u8()?);
        Some(i32::from(((v << 1) as i16) >> 1))
    }
}

// ---------------- paths ----------------

/// A path, in world units: its figures, and whether each is closed.
#[derive(Debug)]
pub(super) struct Path {
    pub(super) path: BezPath,
    pub(super) closed: bool,
}

/// `PathPointType` values: a figure's start, a line, a Bézier's point; the flag closing a figure.
const PT_START: u8 = 0;
const PT_BEZIER: u8 = 3;
const PT_CLOSE: u8 = 0x80;

/// An `EmfPlusPath`.
pub(super) fn path(d: &[u8]) -> Option<Path> {
    let mut c = Cur::new(d);
    c.u32()?;
    let (n, flags) = (c.u32()?, c.u32()?);
    let pts = points(&mut c, n, flags & (RELATIVE | SHORT))?;
    let mut types = Vec::with_capacity(pts.len());
    if flags & RELATIVE != 0 {
        // Run-length encoded: a count in the low six bits, then the type.
        while types.len() < pts.len() {
            let run = usize::from(c.u8()? & 0x3F);
            let t = c.u8()?;
            types.extend(std::iter::repeat_n(t, run.min(pts.len() - types.len())));
        }
    } else {
        types.extend_from_slice(c.take(pts.len())?);
    }
    Some(figures(&pts, &types))
}

/// Points and their `PathPointType`s as a path.
pub(super) fn figures(pts: &[Point], types: &[u8]) -> Path {
    let mut p = BezPath::new();
    let mut closed = true;
    let mut open = false;
    let mut i = 0;
    while i < pts.len() {
        let t = types[i];
        if t & 7 == PT_START || !open {
            if open && !matches!(p.elements().last(), Some(kurbo::PathEl::ClosePath)) {
                closed = false;
            }
            p.move_to(pts[i]);
            open = true;
        } else if t & 7 == PT_BEZIER && i + 2 < pts.len() {
            p.curve_to(pts[i], pts[i + 1], pts[i + 2]);
            i += 2;
        } else {
            p.line_to(pts[i]);
        }
        if types[i] & PT_CLOSE != 0 {
            p.close_path();
            open = false;
        }
        i += 1;
    }
    if open {
        closed = false;
    }
    Path { path: p, closed }
}

// ---------------- brushes ----------------

/// How a brush paints past its tile or its end: `WrapMode*`.
pub(super) const WRAP_TILE: u32 = 0;
pub(super) const WRAP_FLIP_X: u32 = 1;
pub(super) const WRAP_FLIP_Y: u32 = 2;
pub(super) const WRAP_FLIP_XY: u32 = 3;
pub(super) const WRAP_CLAMP: u32 = 4;

/// A brush, in world units.
#[derive(Debug)]
pub(super) enum Fill {
    Solid([f32; 4]),
    /// A `HatchStyle` in two colours.
    Hatch(u32, [f32; 4], [f32; 4]),
    /// An image, a pixel a unit under `transform`, repeated as `wrap` says.
    Texture {
        image: Rc<Image>,
        wrap: u32,
        transform: Affine,
    },
    /// Across `rect` from its left to its right edge under `transform`: its stops, and how it repeats.
    Linear {
        rect: kurbo::Rect,
        stops: Vec<(f32, [f32; 4])>,
        wrap: u32,
        transform: Affine,
    },
    Path(Rc<PathGradient>),
}

/// A path gradient brush: from `centre` (its colour) out to each point of the boundary (its surround colour).
#[derive(Debug)]
pub(super) struct PathGradient {
    pub(super) wrap: u32,
    pub(super) centre: Point,
    pub(super) centre_colour: [f32; 4],
    pub(super) surround: Vec<[f32; 4]>,
    pub(super) boundary: BezPath,
    pub(super) transform: Affine,
    pub(super) ramp: Ramp,
    /// The focus scales: the boundary scaled this much about the centre is filled with the centre colour, and the
    /// gradient runs from there out.
    pub(super) focus: Vec2,
    pub(super) gamma: bool,
}

/// How a path gradient's colour runs from the boundary (position 0) to the centre (1).
#[derive(Debug)]
pub(super) enum Ramp {
    /// From the surround colour to the centre's, evenly.
    Even,
    /// From the surround colour to the centre's by factors (0 the surround's, 1 the centre's) at positions.
    Factors(Vec<(f64, f64)>),
    /// Preset colours at positions, the surround and centre colours unused.
    Preset(Vec<(f64, [f32; 4])>),
}

/// `BrushData` flags.
const BRUSH_PATH: u32 = 0x01;
const BRUSH_TRANSFORM: u32 = 0x02;
const BRUSH_PRESET: u32 = 0x04;
const BRUSH_FACTORS_H: u32 = 0x08;
const BRUSH_FACTORS_V: u32 = 0x10;
const BRUSH_FOCUS: u32 = 0x40;
const BRUSH_GAMMA: u32 = 0x80;
const BRUSH_FIXED: u32 = 0x100;

/// An `EmfPlusBrush`, its images' pixels counted by `spend`.
pub(super) fn brush(c: &mut Cur<'_>, spend: &dyn Fn(u32, u32) -> bool) -> Option<Fill> {
    c.u32()?;
    Some(match c.u32()? {
        0 => Fill::Solid(argb(c.u32()?)),
        1 => Fill::Hatch(c.u32()?, argb(c.u32()?), argb(c.u32()?)),
        2 => {
            let (flags, wrap) = (c.u32()?, c.u32()?);
            let m = if flags & BRUSH_TRANSFORM != 0 { c.matrix()? } else { Affine::IDENTITY };
            let transform = if flags & BRUSH_FIXED != 0 { Affine::IDENTITY } else { m };
            Fill::Texture { image: Rc::new(image(c.rest(), spend)?), wrap, transform }
        }
        3 => Fill::Path(Rc::new(path_gradient(c)?)),
        4 => {
            let (flags, wrap, rect) = (c.u32()?, c.u32()?, c.rect(false)?);
            let (start, end) = (argb(c.u32()?), argb(c.u32()?));
            c.take(8)?;
            let transform = if flags & BRUSH_TRANSFORM != 0 { c.matrix()? } else { Affine::IDENTITY };
            let stops = match blend(c, flags)? {
                Ramp::Even => vec![(0.0, start), (1.0, end)],
                Ramp::Factors(f) => f.iter().map(|&(at, k)| (at as f32, lerp(start, end, k))).collect(),
                Ramp::Preset(p) => p.iter().map(|&(at, c)| (at as f32, c)).collect(),
            };
            let stops = if flags & BRUSH_GAMMA != 0 { gamma_stops(&stops) } else { stops };
            Fill::Linear { rect, stops, wrap, transform }
        }
        _ => return None,
    })
}

/// A gradient's blend: preset colours, or the factors (horizontal, else vertical), or neither; positions in order
/// from 0 to 1.
fn blend(c: &mut Cur<'_>, flags: u32) -> Option<Ramp> {
    let positions = |c: &mut Cur<'_>, each: usize| -> Option<Vec<f64>> {
        let n = c.u32()? as usize;
        if n > c.left() / each {
            return None;
        }
        (0..n).map(|_| c.f32().map(|v| v.clamp(0.0, 1.0))).collect()
    };
    if flags & BRUSH_PRESET != 0 {
        let at = positions(c, 8)?;
        let colours: Option<Vec<[f32; 4]>> = at.iter().map(|_| c.u32().map(argb)).collect();
        let mut v: Vec<(f64, [f32; 4])> = at.into_iter().zip(colours?).collect();
        v.sort_by(|a, b| a.0.total_cmp(&b.0));
        return Some(if v.is_empty() { Ramp::Even } else { Ramp::Preset(v) });
    }
    let mut factors = None;
    for flag in [BRUSH_FACTORS_H, BRUSH_FACTORS_V] {
        if flags & flag != 0 {
            let at = positions(c, 8)?;
            let k: Option<Vec<f64>> = at.iter().map(|_| c.f32()).collect();
            let mut v: Vec<(f64, f64)> = at.into_iter().zip(k?).collect();
            v.sort_by(|a, b| a.0.total_cmp(&b.0));
            factors.get_or_insert(v);
        }
    }
    Some(match factors {
        Some(f) if !f.is_empty() => Ramp::Factors(f),
        _ => Ramp::Even,
    })
}

fn path_gradient(c: &mut Cur<'_>) -> Option<PathGradient> {
    let (flags, wrap) = (c.u32()?, c.u32()?);
    let centre_colour = argb(c.u32()?);
    let centre = c.point()?;
    let n = c.u32()? as usize;
    if n > c.left() / 4 {
        return None;
    }
    let surround: Option<Vec<[f32; 4]>> = (0..n).map(|_| c.u32().map(argb)).collect();
    let boundary = if flags & BRUSH_PATH != 0 {
        path(c.block()?)?.path
    } else {
        let n = c.u32()?;
        let pts = points(c, n, 0)?;
        let mut p = BezPath::new();
        for (i, q) in pts.iter().enumerate() {
            if i == 0 { p.move_to(*q) } else { p.line_to(*q) }
        }
        p.close_path();
        p
    };
    let transform = if flags & BRUSH_TRANSFORM != 0 { c.matrix()? } else { Affine::IDENTITY };
    let ramp = blend(c, flags)?;
    let focus = if flags & BRUSH_FOCUS != 0 {
        c.u32()?;
        let k = |v: f64| if (0.0..1.0).contains(&v) { v } else { 0.0 };
        Vec2::new(k(c.f32()?), k(c.f32()?))
    } else {
        Vec2::ZERO
    };
    Some(PathGradient { wrap, centre, centre_colour, surround: surround?, boundary, transform, ramp, focus, gamma: flags & BRUSH_GAMMA != 0 })
}

/// `a` to `b` by `k`, each channel unpremultiplied.
pub(super) fn lerp(a: [f32; 4], b: [f32; 4], k: f64) -> [f32; 4] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * k as f32)
}

/// sRGB-encoded `v` in linear light, and back: GDI+'s gamma of 2.2.
fn linear(v: f32) -> f32 {
    v.max(0.0).powf(2.2)
}
fn encoded(v: f32) -> f32 {
    v.max(0.0).powf(1.0 / 2.2)
}

/// `a` to `b` by `k`, its colour mixed in linear light.
pub(super) fn lerp_gamma(a: [f32; 4], b: [f32; 4], k: f64) -> [f32; 4] {
    let k = k as f32;
    std::array::from_fn(|i| if i == 3 { a[3] + (b[3] - a[3]) * k } else { encoded(linear(a[i]) + (linear(b[i]) - linear(a[i])) * k) })
}

/// Steps between each pair of a gamma-corrected gradient's stops, the curve its colour takes in sRGB.
pub(super) const GAMMA_STEPS: usize = 32;

/// Stops a gradient mixing in linear light takes: each span cut into [`GAMMA_STEPS`] linear pieces.
fn gamma_stops(stops: &[(f32, [f32; 4])]) -> Vec<(f32, [f32; 4])> {
    let mut out = Vec::with_capacity(stops.len() * GAMMA_STEPS + 1);
    for w in stops.windows(2) {
        let ((a, ca), (b, cb)) = (w[0], w[1]);
        for i in 0..GAMMA_STEPS {
            let k = i as f64 / GAMMA_STEPS as f64;
            out.push((a + (b - a) * k as f32, lerp_gamma(ca, cb, k)));
        }
    }
    if let Some(&last) = stops.last() {
        out.push(last);
    }
    out
}

// ---------------- pens ----------------

/// `LineCapType`: flat, square, round, triangle; the anchors; custom.
pub(super) const CAP_FLAT: u32 = 0;
pub(super) const CAP_SQUARE: u32 = 1;
pub(super) const CAP_ROUND: u32 = 2;
pub(super) const CAP_TRIANGLE: u32 = 3;
pub(super) const CAP_SQUARE_ANCHOR: u32 = 0x11;
pub(super) const CAP_ROUND_ANCHOR: u32 = 0x12;
pub(super) const CAP_DIAMOND_ANCHOR: u32 = 0x13;
pub(super) const CAP_ARROW_ANCHOR: u32 = 0x14;
pub(super) const CAP_CUSTOM: u32 = 0xFF;

/// A pen.
#[derive(Debug)]
pub(super) struct Pen {
    pub(super) width: f64,
    pub(super) unit: u32,
    pub(super) transform: Affine,
    pub(super) caps: (u32, u32),
    /// `LineJoinType`: miter, bevel, round, clipped miter.
    pub(super) join: u32,
    pub(super) miter: f64,
    /// The dashes, in widths, and where the pattern starts.
    pub(super) dashes: Vec<f64>,
    pub(super) dash_offset: f64,
    /// `DashedLineCapType`: flat, round, triangle.
    pub(super) dash_cap: u32,
    /// `PenAlignment`: centre 0, inset 1.
    pub(super) alignment: u32,
    /// Where the strips of a compound line start and end, across its width from 0 to 1.
    pub(super) compound: Vec<f64>,
    pub(super) custom: (Option<Rc<CustomCap>>, Option<Rc<CustomCap>>),
    pub(super) fill: Fill,
}

/// A custom line cap: its outline in the cap's space (a unit a line width, the line ending at the origin and running
/// up the negative y axis), filled or stroked, and how far it pulls the line's end back.
#[derive(Debug)]
pub(super) struct CustomCap {
    pub(super) path: BezPath,
    pub(super) fill: bool,
    pub(super) inset: f64,
    pub(super) scale: f64,
}

/// `PenData` flags.
const PEN_TRANSFORM: u32 = 0x1;
const PEN_START_CAP: u32 = 0x2;
const PEN_END_CAP: u32 = 0x4;
const PEN_JOIN: u32 = 0x8;
const PEN_MITER: u32 = 0x10;
const PEN_LINE_STYLE: u32 = 0x20;
const PEN_DASH_CAP: u32 = 0x40;
const PEN_DASH_OFFSET: u32 = 0x80;
const PEN_DASHES: u32 = 0x100;
const PEN_ALIGNMENT: u32 = 0x200;
const PEN_COMPOUND: u32 = 0x400;
const PEN_CUSTOM_START: u32 = 0x800;
const PEN_CUSTOM_END: u32 = 0x1000;

/// The most dashes and compound strips a pen keeps.
const MAX_DASHES: usize = 64;

/// An `EmfPlusPen`.
pub(super) fn pen(c: &mut Cur<'_>, spend: &dyn Fn(u32, u32) -> bool) -> Option<Pen> {
    c.u32()?;
    c.u32()?;
    let (flags, unit, width) = (c.u32()?, c.u32()?, c.f32()?.abs());
    let has = |f: u32| flags & f != 0;
    let transform = if has(PEN_TRANSFORM) { c.matrix()? } else { Affine::IDENTITY };
    let start = if has(PEN_START_CAP) { c.u32()? } else { CAP_FLAT };
    let end = if has(PEN_END_CAP) { c.u32()? } else { CAP_FLAT };
    let join = if has(PEN_JOIN) { c.u32()? } else { 0 };
    let miter = if has(PEN_MITER) { c.f32()? } else { 10.0 };
    let style = if has(PEN_LINE_STYLE) { c.u32()? } else { 0 };
    let dash_cap = if has(PEN_DASH_CAP) { c.u32()? } else { 0 };
    let dash_offset = if has(PEN_DASH_OFFSET) { c.f32()? } else { 0.0 };
    let floats = |c: &mut Cur<'_>| -> Option<Vec<f64>> {
        let n = c.u32()? as usize;
        if n > c.left() / 4 {
            return None;
        }
        let v: Option<Vec<f64>> = (0..n).map(|_| c.f32()).collect();
        Some(v?.into_iter().take(MAX_DASHES).collect())
    };
    let custom_dashes = if has(PEN_DASHES) { floats(c)? } else { Vec::new() };
    let dashes = match style {
        1 => vec![3.0, 1.0],
        2 => vec![1.0, 1.0],
        3 => vec![3.0, 1.0, 1.0, 1.0],
        4 => vec![3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
        5 => custom_dashes,
        _ => Vec::new(),
    };
    let alignment = if has(PEN_ALIGNMENT) { c.u32()? } else { 0 };
    let compound = if has(PEN_COMPOUND) { floats(c)? } else { Vec::new() };
    let custom_start = if has(PEN_CUSTOM_START) { custom_cap(c.block()?).map(Rc::new) } else { None };
    let custom_end = if has(PEN_CUSTOM_END) { custom_cap(c.block()?).map(Rc::new) } else { None };
    let fill = brush(c, spend)?;
    Some(Pen {
        width,
        unit,
        transform,
        caps: (start, end),
        join,
        miter,
        dashes,
        dash_offset,
        dash_cap,
        alignment,
        compound,
        custom: (custom_start, custom_end),
        fill,
    })
}

/// An `EmfPlusCustomLineCap`: a path cap, or an adjustable arrow.
fn custom_cap(d: &[u8]) -> Option<CustomCap> {
    let mut c = Cur::new(d);
    c.u32()?;
    match c.u32()? {
        0 => {
            let flags = c.u32()?;
            c.u32()?;
            let inset = c.f32()?;
            c.take(12)?;
            c.f32()?;
            let scale = c.f32()?;
            c.take(16)?;
            let (fill, data) = if flags & 1 != 0 { (true, c.block()?) } else { (false, c.block()?) };
            Some(CustomCap { path: path(data)?.path, fill, inset, scale })
        }
        1 => {
            let (width, height, middle) = (c.f32()?, c.f32()?, c.f32()?);
            let fill = c.u32()? != 0;
            c.take(12)?;
            c.f32()?;
            let scale = c.f32()?;
            // The arrow's tip at the line's end, its wings `height` back.
            let mut p = BezPath::new();
            if fill {
                p.move_to((width / 2.0, -height));
                p.line_to((0.0, 0.0));
                p.line_to((-width / 2.0, -height));
                p.line_to((0.0, -height + middle));
                p.close_path();
            } else {
                p.move_to((-width / 2.0, -height));
                p.line_to((0.0, 0.0));
                p.line_to((width / 2.0, -height));
            }
            let inset = if width == 0.0 { 0.0 } else { height / width };
            Some(CustomCap { path: p, fill, inset, scale })
        }
        _ => None,
    }
}

// ---------------- regions ----------------

/// A node of a region, in prefix order: a combination's left and right operands follow it.
#[derive(Debug)]
pub(super) enum Node {
    /// `CombineMode`: intersect 1, union 2, xor 3, exclude 4, complement 5.
    Combine(u32),
    Rect(kurbo::Rect),
    Path(BezPath),
    Empty,
    Infinite,
}

/// An `EmfPlusRegion`'s nodes.
pub(super) fn region(c: &mut Cur<'_>) -> Option<Vec<Node>> {
    c.u32()?;
    c.u32()?;
    let mut nodes = Vec::new();
    // The nodes still to read: a combination is followed by two.
    let mut wanted = 1usize;
    while wanted > 0 {
        if nodes.len() >= MAX_REGION_NODES {
            return None;
        }
        wanted -= 1;
        nodes.push(match c.u32()? {
            t @ 1..=5 => {
                wanted += 2;
                Node::Combine(t)
            }
            0x1000_0000 => Node::Rect(c.rect(false)?),
            0x1000_0001 => Node::Path(path(c.block()?)?.path),
            0x1000_0002 => Node::Empty,
            0x1000_0003 => Node::Infinite,
            _ => return None,
        });
    }
    Some(nodes)
}

// ---------------- images ----------------

/// An image: a bitmap (a file the image registry reads) and its size in pixels, or a metafile and the rect its
/// pixels cover.
#[derive(Debug)]
pub(super) enum Image {
    Bitmap { file: Arc<Vec<u8>>, w: u32, h: u32 },
    Metafile { data: Rc<[u8]>, bounds: kurbo::Rect },
}

impl Image {
    /// The rect its pixels cover: a bitmap's from its corner.
    pub(super) fn bounds(&self) -> kurbo::Rect {
        match self {
            Image::Bitmap { w, h, .. } => kurbo::Rect::new(0.0, 0.0, f64::from(*w), f64::from(*h)),
            Image::Metafile { bounds, .. } => *bounds,
        }
    }
}

/// An `EmfPlusImage`, its pixels counted by `spend`.
pub(super) fn image(d: &[u8], spend: &dyn Fn(u32, u32) -> bool) -> Option<Image> {
    let mut c = Cur::new(d);
    c.u32()?;
    match c.u32()? {
        1 => {
            let (w, h, stride, format, kind) = (c.i32()?, c.i32()?, c.i32()?, c.u32()?, c.u32()?);
            if kind == 1 {
                let bytes = c.rest();
                let info = twip_graphics::image::probe(bytes)?;
                if !spend(info.width, info.height) {
                    return None;
                }
                return Some(Image::Bitmap { file: Arc::new(bytes.to_vec()), w: info.width, h: info.height });
            }
            let (w, h) = (u32::try_from(w).ok()?, u32::try_from(h).ok()?);
            if w == 0 || h == 0 || !spend(w, h) {
                return None;
            }
            let pixels = bitmap(&mut c, w, h, usize::try_from(stride).ok()?, format)?;
            Some(Image::Bitmap { file: bgra_bmp(w, h, &pixels), w, h })
        }
        2 => {
            let (kind, size) = (c.u32()?, c.u32()? as usize);
            let rest = c.rest();
            let data: Rc<[u8]> = rest.get(..size.min(rest.len()))?.into();
            let bounds = match kind {
                1 | 2 => {
                    let h = super::wmf::Header::read(&data)?;
                    kurbo::Rect::new(0.0, 0.0, h.size.width * 96.0 / 72.0, h.size.height * 96.0 / 72.0)
                }
                3..=5 => super::emf::Header::read(&data)?.frame_px(),
                _ => return None,
            };
            Some(Image::Metafile { data, bounds })
        }
        _ => None,
    }
}

/// `PixelFormat` values.
const PF_INDEXED: u32 = 0x0001_0000;

/// A bitmap's rows (`stride` bytes each, top down) of `format` as BGRA, unpremultiplied.
fn bitmap(c: &mut Cur<'_>, w: u32, h: u32, stride: usize, format: u32) -> Option<Vec<u8>> {
    // The format's top byte is reserved, and readers ignore it.
    let format = format & 0x00FF_FFFF;
    let palette: Vec<[u8; 4]> = if format & PF_INDEXED != 0 {
        c.u32()?;
        let n = c.u32()? as usize;
        if n > c.left() / 4 {
            return None;
        }
        (0..n).map(|_| c.u32().map(u32::to_le_bytes)).collect::<Option<_>>()?
    } else {
        Vec::new()
    };
    let bpp = ((format >> 8) & 0xFF) as usize;
    let (w, h) = (w as usize, h as usize);
    let row = (w * bpp).div_ceil(8);
    if bpp == 0 || stride < row {
        return None;
    }
    let bits = c.take(stride.checked_mul(h - 1)?.checked_add(row)?)?;
    let mut out = Vec::with_capacity(w * h * 4);
    let wide = |r: &[u8], i: usize| u16::from_le_bytes([r[i], r[i + 1]]);
    // A 48 or 64-bit channel: linear light, 8192 its full scale.
    let deep = |v: u16| (encoded(f32::from(v.min(8192)) / 8192.0) * 255.0).round() as u8;
    let deep_alpha = |v: u16| (f32::from(v.min(8192)) / 8192.0 * 255.0).round() as u8;
    for y in 0..h {
        let r = &bits[y * stride..y * stride + row];
        for x in 0..w {
            let px: [u8; 4] = match format {
                0x0003_0101 | 0x0003_0402 | 0x0003_0803 => {
                    let at = x * bpp;
                    let index = usize::from(r[at / 8] >> (8 - bpp - at % 8) & ((1u16 << bpp) - 1) as u8);
                    palette.get(index).copied().unwrap_or([0, 0, 0, 255])
                }
                0x0010_1004 => {
                    let v = (wide(r, x * 2) >> 8) as u8;
                    [v, v, v, 255]
                }
                0x0002_1005 | 0x0006_1007 => {
                    let v = wide(r, x * 2);
                    let five = |s: u16| {
                        let c = ((v >> s) & 31) as u8;
                        c << 3 | c >> 2
                    };
                    let a = if format == 0x0006_1007 && v & 0x8000 == 0 { 0 } else { 255 };
                    [five(0), five(5), five(10), a]
                }
                0x0002_1006 => {
                    let v = wide(r, x * 2);
                    let (b, g, rr) = ((v & 31) as u8, ((v >> 5) & 63) as u8, (v >> 11) as u8);
                    [b << 3 | b >> 2, g << 2 | g >> 4, rr << 3 | rr >> 2, 255]
                }
                0x0002_1808 => [r[x * 3], r[x * 3 + 1], r[x * 3 + 2], 255],
                0x0002_2009 => [r[x * 4], r[x * 4 + 1], r[x * 4 + 2], 255],
                0x0026_200A => [r[x * 4], r[x * 4 + 1], r[x * 4 + 2], r[x * 4 + 3]],
                0x000E_200B => unpremultiply([r[x * 4], r[x * 4 + 1], r[x * 4 + 2], r[x * 4 + 3]]),
                0x0010_300C => [deep(wide(r, x * 6)), deep(wide(r, x * 6 + 2)), deep(wide(r, x * 6 + 4)), 255],
                0x0034_400D | 0x001A_400E => {
                    let a = deep_alpha(wide(r, x * 8 + 6));
                    let px = [deep(wide(r, x * 8)), deep(wide(r, x * 8 + 2)), deep(wide(r, x * 8 + 4)), a];
                    if format == 0x001A_400E { unpremultiply_deep(r, x, a) } else { px }
                }
                _ => return None,
            };
            out.extend_from_slice(&px);
        }
    }
    Some(out)
}

/// A premultiplied BGRA pixel unpremultiplied.
fn unpremultiply(px: [u8; 4]) -> [u8; 4] {
    let a = u32::from(px[3]);
    if a == 0 || a == 255 {
        return px;
    }
    let k = |c: u8| ((u32::from(c) * 255 + a / 2) / a).min(255) as u8;
    [k(px[0]), k(px[1]), k(px[2]), px[3]]
}

/// A premultiplied 64-bit pixel's colour, unpremultiplied in linear light, then encoded.
fn unpremultiply_deep(r: &[u8], x: usize, a: u8) -> [u8; 4] {
    let wide = |i: usize| f32::from(u16::from_le_bytes([r[i], r[i + 1]]).min(8192)) / 8192.0;
    let alpha = wide(x * 8 + 6);
    let ch = |i: usize| if alpha > 0.0 { (encoded((wide(x * 8 + i) / alpha).min(1.0)) * 255.0).round() as u8 } else { 0 };
    [ch(0), ch(2), ch(4), a]
}

/// BGRA pixels (rows top down, unpremultiplied) as a BMP file: a `BITMAPV4HEADER` whose masks name the fourth byte
/// alpha, padded for the decoder's mask read.
pub(super) fn bgra_bmp(w: u32, h: u32, pixels: &[u8]) -> Arc<Vec<u8>> {
    const PAD: usize = 16;
    let size = 14 + 108 + pixels.len() + PAD;
    let mut f = Vec::with_capacity(size);
    f.extend_from_slice(b"BM");
    f.extend_from_slice(&(size as u32).to_le_bytes());
    f.extend_from_slice(&[0; 4]);
    f.extend_from_slice(&(14u32 + 108).to_le_bytes());
    f.extend_from_slice(&108u32.to_le_bytes());
    f.extend_from_slice(&(w as i32).to_le_bytes());
    f.extend_from_slice(&(-(h as i32)).to_le_bytes());
    f.extend_from_slice(&1u16.to_le_bytes());
    f.extend_from_slice(&32u16.to_le_bytes());
    f.extend_from_slice(&3u32.to_le_bytes());
    f.extend_from_slice(&(pixels.len() as u32).to_le_bytes());
    f.extend_from_slice(&[0; 16]);
    for mask in [0x00FF_0000u32, 0xFF00, 0xFF, 0xFF00_0000] {
        f.extend_from_slice(&mask.to_le_bytes());
    }
    f.resize(14 + 108, 0);
    f.extend_from_slice(pixels);
    f.resize(size, 0);
    Arc::new(f)
}

// ---------------- text ----------------

/// A font: its family, em size in its unit, and style.
#[derive(Debug)]
pub(super) struct Font {
    pub(super) family: String,
    pub(super) em: f64,
    pub(super) unit: u32,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strike: bool,
}

/// An `EmfPlusFont`.
pub(super) fn font(c: &mut Cur<'_>) -> Option<Font> {
    c.u32()?;
    let (em, unit, style) = (c.f32()?.abs(), c.u32()?, c.u32()?);
    c.u32()?;
    let n = c.u32()? as usize;
    let name = c.take(n.checked_mul(2)?)?;
    let family = char::decode_utf16(name.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).take_while(|&u| u != 0))
        .map(|r| r.unwrap_or('\u{FFFD}'))
        .collect();
    Some(Font { family, em, unit, bold: style & 1 != 0, italic: style & 2 != 0, underline: style & 4 != 0, strike: style & 8 != 0 })
}

/// A string format: its flags, alignments (near 0, centre 1, far 2), hotkey prefix handling, margins and tracking,
/// and tab stops.
#[derive(Debug)]
pub(super) struct Format {
    pub(super) flags: u32,
    pub(super) align: u32,
    pub(super) line_align: u32,
    pub(super) hotkey: u32,
    pub(super) leading: f64,
    pub(super) trailing: f64,
    pub(super) tracking: f64,
    pub(super) first_tab: f64,
    pub(super) tabs: Vec<f64>,
}

impl Default for Format {
    /// GDI+'s default format: a sixth of an em's margin either side.
    fn default() -> Format {
        Format {
            flags: 0,
            align: 0,
            line_align: 0,
            hotkey: 0,
            leading: 1.0 / 6.0,
            trailing: 1.0 / 6.0,
            tracking: 1.0,
            first_tab: 0.0,
            tabs: Vec::new(),
        }
    }
}

/// The most tab stops a format keeps.
const MAX_TABS: usize = 256;

/// An `EmfPlusStringFormat`.
pub(super) fn format(c: &mut Cur<'_>) -> Option<Format> {
    c.u32()?;
    let flags = c.u32()?;
    c.u32()?;
    let (align, line_align) = (c.u32()?, c.u32()?);
    c.take(8)?;
    let first_tab = c.f32()?;
    let hotkey = c.u32()?;
    let (leading, trailing, tracking) = (c.f32()?, c.f32()?, c.f32()?);
    c.u32()?;
    let tab_count = (c.u32()? as usize).min(MAX_TABS);
    c.u32()?;
    let tabs = (0..tab_count).map_while(|_| c.f32()).collect();
    Some(Format { flags, align, line_align, hotkey, leading, trailing, tracking: if tracking > 0.0 { tracking } else { 1.0 }, first_tab, tabs })
}

/// Image attributes: how an image draws past its edges, and the colour it is clamped to.
#[derive(Clone, Copy, Debug)]
pub(super) struct Attributes {
    pub(super) wrap: u32,
    pub(super) clamp: [f32; 4],
}

/// An `EmfPlusImageAttributes`.
pub(super) fn attributes(c: &mut Cur<'_>) -> Option<Attributes> {
    c.take(8)?;
    Some(Attributes { wrap: c.u32()?, clamp: argb(c.u32()?) })
}
