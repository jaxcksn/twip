//! Metafile pictures: a device context (the object table, pens, brushes, fonts, the mapping mode and world transform,
//! saved states, the path bracket and the clip) that a metafile's records drive, replayed into a [`Sink`] in the
//! picture's space: its frame, in points, from its top left. [`emf`] reads EMF records into it.
//!
//! Metafiles are untrusted. Every record is checked against the data before it is read, and the records, objects,
//! saved states, path points, clips, bitmap pixels and the work replayed are capped. Nothing recurses.

mod codepages;
pub mod emf;
mod plus;
mod plus_objects;
mod plus_paint;
#[cfg(test)]
mod plus_tests;
mod plus_text;
mod raster;
mod region;
#[cfg(test)]
pub(crate) mod tests;
pub mod wmf;
#[cfg(test)]
mod wmf_tests;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::f64::consts::TAU;
use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, BezPath, PathEl, Point, Shape, Vec2};

use super::svg::{Brush, GradientKind, MaskStep, Sink, Spread, SvgGradient, Text};

/// The size in points of the metafile `d` (an EMF or a WMF), or none when it is neither.
pub fn size(d: &[u8]) -> Option<kurbo::Size> {
    emf::Header::read(d).map(|h| h.size).or_else(|| wmf::Header::read(d).map(|h| h.size))
}

/// Plays the metafile `d` (an EMF or a WMF) into `sink`, in the picture's space; false when it is neither.
pub fn play(sink: &mut impl Sink, d: &[u8]) -> bool {
    emf::play(sink, d) || wmf::play(sink, d)
}

/// The work and pixels a metafile has spent, and how many metafiles it is inside.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct Spent {
    work: u64,
    pixels: u64,
    depth: u8,
}

/// The most metafiles a metafile's pictures nest.
const MAX_DEPTH: u8 = 4;

/// The most records one metafile plays.
const MAX_RECORDS: u32 = 1 << 20;
/// The most saved states kept: saves past it restore nothing. A bound on what an untrusted file makes the player keep
/// (each state holds the clip); GDI itself refuses saves past its handle limits.
const MAX_SAVES: usize = 1024;
/// The most points the path bracket collects.
const MAX_PATH_POINTS: usize = 1 << 20;
/// The most shapes the clip intersects, and steps a mask clip takes: a bound on the clips each draw pushes, as an
/// untrusted file could make them grow without end. Combinations past it leave the clip as it was.
const MAX_CLIPS: usize = 64;
const MAX_MASK_STEPS: usize = 256;
/// The most work one metafile replays: the path elements it draws and clips by (each time a clip is sent again
/// included), characters, region rects and mask bits. A bound on the picture it records, whatever its records ask.
const MAX_WORK: u64 = 1 << 22;
/// What drawing over the whole picture costs, as work: a mesh's layers, and a layer a mask clip or an isolated
/// picture composites. A draw over part of it costs that share, whatever the picture's frame or the zoom, so the
/// budget bounds how many times over a picture's area these layers composite (16 for meshes, 4096 for masks).
const MESH_FULL: u64 = MAX_WORK / 16;
const LAYER_FULL: u64 = MAX_WORK / 4096;

/// The most bitmap pixels one metafile draws.
const MAX_PIXELS: u64 = 1 << 26;

/// `PS_*` pen styles.
const PS_NULL: u32 = 5;
const PS_USERSTYLE: u32 = 7;
const PS_ALTERNATE: u32 = 8;

/// A pen: its style, width (logical units; cosmetic pens are a device pixel), the brush it strokes with, ends and
/// joins.
#[derive(Clone, Debug)]
struct Pen {
    style: u32,
    width: f64,
    cosmetic: bool,
    brush: LogBrush,
    cap: kurbo::Cap,
    join: kurbo::Join,
    /// A user style's dashes, in widths (geometric) or device pixels (cosmetic).
    dashes: Rc<[f64]>,
}

impl Pen {
    fn new(style: u32, width: f64, colour: u32) -> Pen {
        let brush = LogBrush::Solid(colour);
        Pen { style, width, cosmetic: width == 0.0, brush, cap: kurbo::Cap::Round, join: kurbo::Join::Round, dashes: Rc::new([]) }
    }
}

/// A brush.
#[derive(Clone, Debug)]
enum LogBrush {
    Null,
    Solid(u32),
    /// A `HS_*` hatch style in a colour.
    Hatch(u32, u32),
    /// A pattern bitmap (a BMP file); a monochrome one draws its 0 bits in the text colour and its 1 bits in the
    /// background colour.
    Bitmap(Arc<Vec<u8>>, bool),
}

/// A logical font: the face named, its height in logical units (negative: the em; positive: the cell), style and
/// escapement (tenths of a degree, anticlockwise).
#[derive(Clone, Debug)]
struct LogFont {
    family: String,
    height: f64,
    weight: i32,
    italic: bool,
    underline: bool,
    strike: bool,
    escapement: f64,
    /// The GDI character set its 8-bit strings are in.
    charset: u8,
}

#[derive(Clone, Debug)]
enum Object {
    Pen(Pen),
    Brush(LogBrush),
    Font(Rc<LogFont>),
    /// A logical palette's colours.
    Palette(Rc<Vec<[u8; 3]>>),
}

/// `DEFAULT_PALETTE`'s twenty colours.
const DEFAULT_PALETTE: [u32; 20] = [
    0x00_0000, 0x80_0000, 0x00_8000, 0x80_8000, 0x00_0080, 0x80_0080, 0x00_8080, 0xC0_C0C0, 0xC0_DCC0, 0xA6_CAF0, 0xFF_FBF0, 0xA0_A0A4, 0x80_8080,
    0xFF_0000, 0x00_FF00, 0xFF_FF00, 0x00_00FF, 0xFF_00FF, 0x00_FFFF, 0xFF_FFFF,
];

/// The stock object `i` (`*_BRUSH`, `*_PEN`, `*_FONT`).
fn stock(i: u32) -> Option<Object> {
    let grey = |v: u32| Object::Brush(LogBrush::Solid(v | v << 8 | v << 16));
    Some(match i {
        0 | 18 => grey(0xFF),
        1 => grey(0xC0),
        2 => grey(0x80),
        3 => grey(0x40),
        4 => grey(0),
        5 => Object::Brush(LogBrush::Null),
        6 => Object::Pen(Pen::new(0, 0.0, 0xFF_FFFF)),
        7 | 19 => Object::Pen(Pen::new(0, 0.0, 0)),
        8 => Object::Pen(Pen::new(PS_NULL, 0.0, 0)),
        10..=14 | 16 | 17 => Object::Font(default_font()),
        15 => Object::Palette(Rc::new(DEFAULT_PALETTE.iter().map(|&c| [(c >> 16) as u8, (c >> 8) as u8, c as u8]).collect())),
        _ => return None,
    })
}

fn default_font() -> Rc<LogFont> {
    Rc::new(LogFont {
        family: "Arial".into(),
        height: -12.0,
        weight: 400,
        italic: false,
        underline: false,
        strike: false,
        escapement: 0.0,
        charset: 0,
    })
}

/// A `COLORREF` (`0x00BBGGRR`), opaque.
fn colour(c: u32) -> [f32; 4] {
    let ch = |s: u32| ((c >> s) & 0xFF) as f32 / 255.0;
    [ch(0), ch(8), ch(16), 1.0]
}

/// The `HS_*` hatch styles' 8 × 8 tiles.
fn hatch(style: u32) -> [u8; 8] {
    match style {
        0 => [0, 0, 0, 0, 0, 0, 0, 0xFF],
        1 => [0x08; 8],
        2 => [0x80, 0x40, 0x20, 0x10, 0x08, 0x04, 0x02, 0x01],
        3 => [0x01, 0x02, 0x04, 0x08, 0x10, 0x20, 0x40, 0x80],
        4 => [0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0x08, 0xFF],
        _ => [0x81, 0x42, 0x24, 0x18, 0x18, 0x24, 0x42, 0x81],
    }
}

/// The device context's state: what `SaveDC` saves.
#[derive(Clone)]
struct Dc {
    pen: Pen,
    brush: LogBrush,
    font: Rc<LogFont>,
    text_colour: u32,
    bk_colour: u32,
    /// `OPAQUE` background mode: hatches, and the box behind text, are backed by the background colour.
    opaque: bool,
    even_odd: bool,
    map_mode: u32,
    win_org: Point,
    win_ext: Vec2,
    vp_org: Point,
    vp_ext: Vec2,
    world: Affine,
    text_align: u32,
    /// `HALFTONE` stretching: bitmaps scale smoothly, not by repeating pixels.
    smooth: bool,
    miter: f64,
    clockwise: bool,
    /// The brush origin, in device pixels.
    brush_org: Point,
    /// The current position, in logical units.
    cur: Point,
    clip: Rc<ClipState>,
    /// The selected palette's handle.
    palette: u32,
    /// The binary raster operation pens and brushes draw by (`SetROP2`), as a truth table of bit `2·pen + dst`.
    rop2: u8,
}

/// The clip, in the picture's space.
#[derive(Clone, Debug, PartialEq)]
enum ClipState {
    /// The intersection of shapes' fills (even-odd or not); none clips nothing.
    Shapes(Vec<(BezPath, bool)>),
    /// A mask the steps draw, which keeps each draw where it covers.
    /// Shared, so the sink records each mask once and draws by reference.
    Mask(Rc<[MaskStep]>),
}

impl ClipState {
    /// The path elements sending the clip takes.
    fn cost(&self) -> usize {
        match self {
            ClipState::Shapes(v) => v.iter().map(|(p, _)| p.elements().len()).sum(),
            ClipState::Mask(m) => m.iter().map(MaskStep::cost).sum(),
        }
    }
}

impl ClipState {
    /// The clip moved by `d`.
    fn moved(&self, d: Affine) -> ClipState {
        match self {
            ClipState::Shapes(v) => ClipState::Shapes(
                v.iter()
                    .map(|(p, e)| {
                        let mut p = p.clone();
                        p.apply_affine(d);
                        (p, *e)
                    })
                    .collect(),
            ),
            ClipState::Mask(steps) => ClipState::Mask(
                steps
                    .iter()
                    .map(|s| {
                        let mut s = s.clone();
                        if let MaskStep::Add(p, _) | MaskStep::Remove(p, _) | MaskStep::Toggle(p, _) | MaskStep::Clip(p, _) = &mut s {
                            p.apply_affine(d);
                        }
                        s
                    })
                    .collect(),
            ),
        }
    }
}

impl MaskStep {
    fn cost(&self) -> usize {
        match self {
            MaskStep::Add(p, _) | MaskStep::Remove(p, _) | MaskStep::Toggle(p, _) | MaskStep::Clip(p, _) => p.elements().len(),
            MaskStep::Group(_) | MaskStep::Pop => 1,
        }
    }
}

impl Default for Dc {
    fn default() -> Dc {
        Dc {
            pen: Pen::new(0, 0.0, 0),
            brush: LogBrush::Solid(0xFF_FFFF),
            font: default_font(),
            text_colour: 0,
            bk_colour: 0xFF_FFFF,
            opaque: true,
            even_odd: true,
            map_mode: MM_TEXT,
            win_org: Point::ZERO,
            win_ext: Vec2::new(1.0, 1.0),
            vp_org: Point::ZERO,
            vp_ext: Vec2::new(1.0, 1.0),
            world: Affine::IDENTITY,
            text_align: 0,
            smooth: false,
            miter: 10.0,
            clockwise: false,
            brush_org: Point::ZERO,
            cur: Point::ZERO,
            clip: Rc::new(ClipState::Shapes(Vec::new())),
            palette: 0x8000_000F,
            rop2: COPY,
        }
    }
}

/// The raster operation table that copies the source.
const COPY: u8 = 12;

const MM_TEXT: u32 = 1;
const MM_ISOTROPIC: u32 = 7;
const MM_ANISOTROPIC: u32 = 8;

/// `RGN_*` modes.
const RGN_AND: u32 = 1;
const RGN_OR: u32 = 2;
const RGN_XOR: u32 = 3;
const RGN_DIFF: u32 = 4;
const RGN_COPY: u32 = 5;

/// How a run of points draws.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Poly {
    Line,
    Gon,
    Bezier,
    /// From the current position, which moves to the last point.
    LineTo,
    BezierTo,
}

/// A line of text a record draws, in logical units.
struct TextOut<'a> {
    at: Point,
    chars: &'a [u32],
    glyph_ids: bool,
    advances: Option<&'a [f64]>,
    /// Each character's move up to the next one's origin (`ETO_PDY`), in logical units.
    rises: Option<&'a [f64]>,
    /// Extra advance after each character and after each space (logical units), where there is no spacing array.
    extra: (f64, f64),
    /// The `ETO_OPAQUE` or `ETO_CLIPPED` rectangle.
    rect: Option<kurbo::Rect>,
    opaque: bool,
    clipped: bool,
}

/// The player: the device context and the sink it draws into.
pub struct Player<'s, S: Sink> {
    sink: &'s mut S,
    dc: Dc,
    saved: Vec<Dc>,
    /// Saves past [`MAX_SAVES`], which their restores pop first.
    overflow: usize,
    objects: Vec<Option<Object>>,
    handles: usize,
    /// Device pixels to the picture's space.
    dev: Affine,
    /// The picture's size, in points.
    size: kurbo::Size,
    /// Device pixels per millimetre.
    px_per_mm: Vec2,
    /// The path bracket: open, the path (in the picture's space), and whether `WidenPath` made it the pen's outline.
    in_path: bool,
    path: BezPath,
    widened: bool,
    clip_dirty: bool,
    work: Cell<u64>,
    pixels: Cell<u64>,
    stopped: Cell<bool>,
    /// The mask the sink last recorded, and the share of the picture its shapes cover.
    sent_mask: Option<Rc<[MaskStep]>>,
    mask_share: f64,
    /// How many metafiles this one is inside: a picture drawing a picture.
    depth: u8,
    /// Scratch for text and spacing.
    chars: Vec<u32>,
    advances_in: Vec<f64>,
    rises_in: Vec<f64>,
    advances: Vec<f64>,
    rises: Vec<f64>,
    /// The last monochrome brush coloured: its bitmap's address, the text and background colours, and the bitmap
    /// coloured by them.
    mono: RefCell<HashMap<(usize, [u8; 6]), (Arc<Vec<u8>>, Arc<Vec<u8>>)>>,
}

impl<'s, S: Sink> Player<'s, S> {
    fn new(sink: &'s mut S, dev: Affine, size: kurbo::Size, px_per_mm: Vec2, handles: usize) -> Player<'s, S> {
        sink.save();
        Player {
            sink,
            dc: Dc::default(),
            saved: Vec::new(),
            overflow: 0,
            objects: Vec::new(),
            handles,
            dev,
            size,
            px_per_mm,
            in_path: false,
            path: BezPath::new(),
            widened: false,
            clip_dirty: false,
            work: Cell::new(0),
            pixels: Cell::new(0),
            stopped: Cell::new(false),
            sent_mask: None,
            mask_share: 1.0,
            depth: 0,
            chars: Vec::new(),
            advances_in: Vec::new(),
            rises_in: Vec::new(),
            advances: Vec::new(),
            rises: Vec::new(),
            mono: RefCell::new(HashMap::new()),
        }
    }

    fn finish(self) -> Spent {
        self.sink.restore();
        self.spent()
    }

    /// Takes on what an enclosing metafile has spent, so a picture inside another counts against the same budgets.
    fn inherit(&mut self, s: Spent) {
        self.work.set(s.work);
        self.pixels.set(s.pixels);
        self.stopped.set(s.work > MAX_WORK);
        self.depth = s.depth;
    }

    fn spent(&self) -> Spent {
        Spent { work: self.work.get(), pixels: self.pixels.get(), depth: self.depth }
    }

    /// The share of the picture's area `r` (in its space) covers, 0 to 1.
    fn share(&self, r: kurbo::Rect) -> f64 {
        let picture = kurbo::Rect::from_origin_size(Point::ZERO, self.size);
        let r = r.intersect(picture);
        let (a, whole) = (r.width().max(0.0) * r.height().max(0.0), picture.area());
        if whole > 0.0 && a.is_finite() { (a / whole).clamp(0.0, 1.0) } else { 1.0 }
    }

    /// Counts `share` of drawing over the whole picture at `full`; false past the budget.
    fn spend_share(&self, share: f64, full: u64) -> bool {
        self.spend((share * full as f64).ceil() as usize)
    }

    /// Counts `n` units of work; false (and the player stops) past [`MAX_WORK`].
    fn spend(&self, n: usize) -> bool {
        self.work.set(self.work.get().saturating_add(n as u64));
        if self.work.get() > MAX_WORK {
            self.stopped.set(true);
        }
        !self.stopped.get()
    }

    // ---------------- coordinates ----------------

    /// Page space to device pixels: the window mapped onto the viewport.
    fn page(&self) -> Affine {
        let d = &self.dc;
        let mm = |units_per_mm: f64| Vec2::new(self.px_per_mm.x / units_per_mm, -self.px_per_mm.y / units_per_mm);
        let s = match d.map_mode {
            2 => mm(10.0),
            3 => mm(100.0),
            4 => mm(100.0 / 25.4),
            5 => mm(1000.0 / 25.4),
            6 => mm(1440.0 / 25.4),
            MM_ISOTROPIC | MM_ANISOTROPIC => {
                let ratio = |v: f64, w: f64| if w == 0.0 { 1.0 } else { v / w };
                let (sx, sy) = (ratio(d.vp_ext.x, d.win_ext.x), ratio(d.vp_ext.y, d.win_ext.y));
                if d.map_mode == MM_ISOTROPIC {
                    let k = sx.abs().min(sy.abs());
                    Vec2::new(k.copysign(sx), k.copysign(sy))
                } else {
                    Vec2::new(sx, sy)
                }
            }
            _ => Vec2::new(1.0, 1.0),
        };
        Affine::translate(d.vp_org.to_vec2()) * Affine::scale_non_uniform(s.x, s.y) * Affine::translate(-d.win_org.to_vec2())
    }

    /// Logical units to the picture's space.
    fn m(&self) -> Affine {
        self.dev * self.page() * self.dc.world
    }

    /// A device pixel's size in the picture.
    fn px(&self) -> f64 {
        self.dev.determinant().abs().sqrt()
    }

    // ---------------- state ----------------

    fn save(&mut self) {
        if self.saved.len() < MAX_SAVES {
            self.saved.push(self.dc.clone());
        } else {
            // ponytail: saves past MAX_SAVES keep no state and their restores leave the context as it is: the cap bounds
            // what an untrusted file makes the player hold.
            self.overflow += 1;
        }
    }

    /// `RestoreDC`: negative, that many saves back; positive, to that save.
    fn restore(&mut self, n: i32) {
        let depth = self.saved.len() + self.overflow;
        let levels = if n < 0 { n.unsigned_abs() as usize } else { (depth + 1).saturating_sub(n as usize) };
        if n == 0 || levels == 0 || levels > depth {
            return;
        }
        let from_overflow = levels.min(self.overflow);
        self.overflow -= from_overflow;
        let rest = levels - from_overflow;
        if rest > 0 {
            let keep = self.saved.len() - rest;
            self.saved.truncate(keep + 1);
            if let Some(dc) = self.saved.pop() {
                let same = Rc::ptr_eq(&dc.clip, &self.dc.clip);
                self.dc = dc;
                self.clip_dirty |= !same;
            }
        }
    }

    /// Stores `o` at handle `i` (1 to the header's count; 0 is the metafile itself).
    fn create(&mut self, i: u32, o: Object) {
        let i = i as usize;
        if i == 0 || i >= self.handles {
            return;
        }
        if i >= self.objects.len() {
            self.objects.resize_with(i + 1, || None);
        }
        self.objects[i] = Some(o);
    }

    fn object(&self, i: u32) -> Option<Object> {
        if i & 0x8000_0000 != 0 {
            return stock(i & 0x7FFF_FFFF);
        }
        self.objects.get(i as usize)?.clone()
    }

    fn select(&mut self, i: u32) {
        match self.object(i) {
            Some(Object::Pen(p)) => self.dc.pen = p,
            Some(Object::Brush(b)) => self.dc.brush = b,
            Some(Object::Font(f)) => self.dc.font = f,
            Some(Object::Palette(_)) | None => {}
        }
    }

    fn delete(&mut self, i: u32) {
        if let Some(o) = self.objects.get_mut(i as usize) {
            *o = None;
        }
    }

    fn set_map_mode(&mut self, mode: u32) {
        self.dc.map_mode = if (1..=8).contains(&mode) { mode } else { MM_TEXT };
    }

    /// `SetWindowExtEx` and `SetViewportExtEx`, which only the scalable modes take.
    fn set_ext(&mut self, viewport: bool, v: Vec2) {
        // Extents that scaling has driven out of a sane range are refused, as a zero one is.
        let sane = |x: f64| x.is_finite() && (1e-6..=1e12).contains(&x.abs());
        if matches!(self.dc.map_mode, MM_ISOTROPIC | MM_ANISOTROPIC) && sane(v.x) && sane(v.y) {
            *(if viewport { &mut self.dc.vp_ext } else { &mut self.dc.win_ext }) = v;
        }
    }

    /// `ModifyWorldTransform`: identity, left or right multiply, set.
    fn modify_world(&mut self, x: Affine, mode: u32) {
        let w = &mut self.dc.world;
        match mode {
            1 => *w = Affine::IDENTITY,
            2 => *w *= x,
            3 => *w = x * *w,
            4 => *w = x,
            _ => {}
        }
        if !w.is_finite() || w.determinant() == 0.0 {
            *w = Affine::IDENTITY;
        }
    }

    // ---------------- palette and colours ----------------

    /// The selected palette's colours.
    fn palette(&self) -> Rc<Vec<[u8; 3]>> {
        match self.object(self.dc.palette).or_else(|| stock(15)) {
            Some(Object::Palette(p)) => p,
            _ => Rc::new(Vec::new()),
        }
    }

    /// A `COLORREF`: `0x00BBGGRR`, or with `0x01` in its top byte, an index into the selected palette.
    fn rgb(&self, c: u32) -> [f32; 4] {
        if c >> 24 == 1 {
            let p = self.palette();
            let [r, g, b] = p.get((c & 0xFFFF) as usize).copied().unwrap_or([0; 3]);
            return colour(u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16);
        }
        colour(c)
    }

    /// Palette `i`'s colours, changed by `edit`.
    fn edit_palette(&mut self, i: u32, edit: impl FnOnce(&mut Vec<[u8; 3]>)) {
        if let Some(Some(Object::Palette(p))) = self.objects.get_mut(i as usize) {
            edit(Rc::make_mut(p));
        }
    }

    // ---------------- clip ----------------

    /// A rect far outside the picture: what an excluding clip keeps the part of outside the region.
    fn everything(&self) -> BezPath {
        let (w, h) = (self.size.width, self.size.height);
        kurbo::Rect::new(-w - 1.0, -h - 1.0, 2.0 * w + 1.0, 2.0 * h + 1.0).to_path(0.1)
    }

    /// Combines the clip with `region` (in the picture's space, its fill even-odd or not) by `mode`; `None` with
    /// `RGN_COPY` resets it.
    fn clip(&mut self, region: Option<(BezPath, bool)>, mode: u32) {
        let everything = self.everything();
        // Combining copies the clip.
        if !self.spend(self.dc.clip.cost() + region.as_ref().map_or(0, |r| r.0.elements().len())) {
            return;
        }
        let next = combine(&self.dc.clip, region, mode, everything);
        if let Some(c) = next {
            self.dc.clip = Rc::new(c);
            self.clip_dirty = true;
        }
    }

    /// Moves the clip by `v` logical units.
    fn offset_clip(&mut self, v: Vec2) {
        if !self.spend(self.dc.clip.cost()) {
            return;
        }
        let m = self.m();
        let d = Affine::translate(m * Point::new(v.x, v.y) - m * Point::ZERO);
        self.dc.clip = Rc::new(self.dc.clip.moved(d));
        self.clip_dirty = true;
    }

    /// A rect in logical units as a path in the picture's space.
    fn rect_path(&self, r: kurbo::Rect) -> BezPath {
        let mut p = r.abs().to_path(0.1);
        p.apply_affine(self.m());
        p
    }

    /// Region rects (device pixels when `device`, else logical units) as one path in the picture's space, each rect
    /// wound the same way.
    fn region(&self, rects: impl Iterator<Item = kurbo::Rect>, device: bool) -> BezPath {
        self.rects_path(rects, if device { self.dev } else { self.m() })
    }

    /// Rects as one path, each wound the same way, mapped by `m`.
    fn rects_path(&self, rects: impl Iterator<Item = kurbo::Rect>, m: Affine) -> BezPath {
        let mut p = BezPath::new();
        for r in rects {
            if !self.spend(5) {
                break;
            }
            p.extend(r.abs().path_elements(0.1));
        }
        p.apply_affine(m);
        p
    }

    /// Sends the clip again when it changed, its elements counted each time.
    fn sync_clip(&mut self) {
        if self.clip_dirty {
            self.clip_dirty = false;
            self.sink.restore();
            self.sink.save();
            if let ClipState::Shapes(v) = &*self.dc.clip {
                if !self.spend(self.dc.clip.cost()) {
                    // Past the budget nothing more draws: clipped to nothing.
                    self.sink.clip_path(&BezPath::new(), false);
                    return;
                }
                for (p, even_odd) in v {
                    self.sink.clip_path(p, *even_odd);
                }
            }
        }
    }

    /// Runs `draw` (drawing into the sink) under the clip, combining with what is under it by raster operation `op`:
    /// in a layer kept where the mask covers when the clip is a mask.
    fn under_clip(&mut self, op: u8, draw: impl FnOnce(&mut Self)) {
        self.sync_clip();
        self.sink.set_raster(op);
        let clip = Rc::clone(&self.dc.clip);
        if let ClipState::Mask(steps) = &*clip {
            // The sink records a mask the first time it sees it, and places it after: a new one is counted.
            let new = !self.sent_mask.as_ref().is_some_and(|m| Rc::ptr_eq(m, steps));
            if new {
                // The layer it keeps the draw in covers the mask's shapes.
                let bounds = steps.iter().filter_map(|s| match s {
                    MaskStep::Add(p, _) | MaskStep::Toggle(p, _) => Some(p.bounding_box()),
                    _ => None,
                });
                self.mask_share = self.share(bounds.reduce(|a, b| a.union(b)).unwrap_or(kurbo::Rect::ZERO));
            }
            if !self.spend(if new { clip.cost() } else { 4 }) || !self.spend_share(self.mask_share, LAYER_FULL) {
                self.sink.set_raster(COPY);
                return;
            }
            self.sent_mask = Some(Rc::clone(steps));
            self.sink.begin_masked(steps);
            draw(self);
            self.sink.end_masked(steps);
        } else {
            draw(self);
        }
        self.sink.set_raster(COPY);
    }

    // ---------------- paint ----------------

    fn brush(&self, b: &LogBrush) -> Option<Brush> {
        let tile = || self.dev * Affine::translate(self.dc.brush_org.to_vec2());
        Some(match b {
            LogBrush::Null => return None,
            LogBrush::Solid(c) => Brush::Solid(self.rgb(*c)),
            LogBrush::Hatch(style, c) => {
                let bg = if self.dc.opaque { self.rgb(self.dc.bk_colour) } else { [0.0; 4] };
                Brush::Hatch { bits: hatch(*style), fg: self.rgb(*c), bg, transform: tile() }
            }
            LogBrush::Bitmap(b, false) => Brush::Bitmap { bitmap: Arc::clone(b), transform: tile() },
            LogBrush::Bitmap(b, true) => Brush::Bitmap { bitmap: self.mono(b)?, transform: tile() },
        })
    }

    /// A monochrome pattern in the text and background colours: its two colour table entries replaced, once for each
    /// pattern and pair of colours, each copy's pixels counted against the budget. `None` past it.
    fn mono(&self, b: &Arc<Vec<u8>>) -> Option<Arc<Vec<u8>>> {
        let ink = |c: u32| self.rgb(c).map(|v| (v * 255.0).round() as u8);
        let ([tr, tg, tb, _], [br, bg, bb, _]) = (ink(self.dc.text_colour), ink(self.dc.bk_colour));
        let key = (Arc::as_ptr(b) as usize, [tr, tg, tb, br, bg, bb]);
        if let Some((_, coloured)) = self.mono.borrow().get(&key) {
            return Some(Arc::clone(coloured));
        }
        let header = b.get(14..26).map(|h| {
            let v = |o: usize| u32::from_le_bytes([h[o], h[o + 1], h[o + 2], h[o + 3]]);
            (v(0) as usize, v(4), (v(8) as i32).unsigned_abs())
        });
        let (size, w, h) = header?;
        let (w, h) = if size == 12 { (w & 0xFFFF, w >> 16) } else { (w, h) };
        // A copy is 24 bits a pixel: counted three times.
        if !self.spend_pixels(w, h) {
            return None;
        }
        self.pixels.set(self.pixels.get().saturating_add(2 * u64::from(w) * u64::from(h)));
        if self.pixels.get() > MAX_PIXELS {
            return None;
        }
        let coloured = emf::unpack(b, Some([[tb, tg, tr], [bb, bg, br]]))?;
        // The pattern is kept with its copies, so its address isn't another's while they are cached.
        self.mono.borrow_mut().insert(key, (Arc::clone(b), Arc::clone(&coloured)));
        Some(coloured)
    }

    /// The current pen as a stroke in the picture's space, with what it strokes with.
    fn pen(&self) -> Option<(kurbo::Stroke, Brush)> {
        let p = &self.dc.pen;
        let style = p.style & 0xF;
        if style == PS_NULL {
            return None;
        }
        let px = self.px();
        let width = if p.cosmetic { px } else { (p.width * self.m().determinant().abs().sqrt()).max(px) };
        let unit = if p.cosmetic { px } else { width };
        let dashes: &[f64] = match style {
            1 if p.cosmetic => &[18.0, 6.0],
            1 => &[3.0, 1.0],
            2 if p.cosmetic => &[3.0, 3.0],
            2 | PS_ALTERNATE => &[1.0, 1.0],
            3 if p.cosmetic => &[9.0, 6.0, 3.0, 6.0],
            3 => &[3.0, 1.0, 1.0, 1.0],
            4 if p.cosmetic => &[9.0, 3.0, 3.0, 3.0, 3.0, 3.0],
            4 => &[3.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            PS_USERSTYLE => &p.dashes,
            _ => &[],
        };
        let mut s = kurbo::Stroke::new(width).with_caps(p.cap).with_join(p.join).with_miter_limit(self.dc.miter.max(1.0));
        if dashes.iter().any(|&d| d > 0.0) && dashes.iter().all(|d| d.is_finite() && *d >= 0.0) {
            s = s.with_dashes(0.0, dashes.iter().map(|d| d * unit));
        }
        Some((s, self.brush(&p.brush)?))
    }

    /// Fills and strokes `p` (in the picture's space) with the current brush and pen, by the binary raster operation.
    fn paint(&mut self, p: &BezPath, fill: bool, stroke: bool) {
        if fill && let Some(b) = self.brush(&self.dc.brush) {
            let even_odd = self.dc.even_odd;
            self.under_clip(self.dc.rop2, |me| me.sink.fill(p, even_odd, &b));
        }
        if stroke && let Some((s, b)) = self.pen() {
            if !s.dash_pattern.is_empty() && !self.spend(dash_work(p, &s)) {
                return;
            }
            self.under_clip(self.dc.rop2, |me| me.sink.stroke(p, &s, &b));
        }
    }

    /// Draws `p` (logical units), or adds it to the open path.
    fn draw(&mut self, mut p: BezPath, fill: bool, stroke: bool) {
        if !self.spend(p.elements().len()) {
            return;
        }
        p.apply_affine(self.m());
        if self.in_path {
            self.append(p);
        } else {
            self.paint(&p, fill, stroke);
        }
    }

    /// Adds `p` (in the picture's space) to the open path, continuing its figure from where it ends.
    fn append(&mut self, p: BezPath) {
        if self.path.elements().len() + p.elements().len() > MAX_PATH_POINTS {
            return;
        }
        let els = p.elements();
        let joins = match (els.first(), self.path.elements().last()) {
            (Some(PathEl::MoveTo(a)), Some(last)) if !matches!(last, PathEl::ClosePath) => last.end_point().is_some_and(|e| (e - *a).hypot() < 1e-9),
            _ => false,
        };
        for &el in &els[usize::from(joins)..] {
            self.path.push(el);
        }
    }

    // ---------------- path bracket ----------------

    fn begin_path(&mut self) {
        self.in_path = true;
        self.widened = false;
        self.path.truncate(0);
    }

    fn end_path(&mut self) {
        self.in_path = false;
    }

    fn abort_path(&mut self) {
        self.in_path = false;
        self.path.truncate(0);
    }

    fn close_figure(&mut self) {
        if self.in_path && !self.path.elements().is_empty() {
            self.path.close_path();
        }
    }

    fn widen_path(&mut self) {
        self.widened = true;
    }

    /// Fills and strokes the path the bracket made, and empties it. A widened path fills as the pen's outline.
    fn draw_path(&mut self, fill: bool, stroke: bool) {
        if self.in_path {
            return;
        }
        let p = std::mem::take(&mut self.path);
        if self.widened && fill && !stroke {
            if let (Some((s, _)), Some(b)) = (self.pen(), self.brush(&self.dc.brush)) {
                self.under_clip(self.dc.rop2, |me| me.sink.stroke(&p, &s, &b));
            }
        } else {
            self.paint(&p, fill, stroke);
        }
        self.widened = false;
    }

    fn clip_path(&mut self, mode: u32) {
        if self.in_path {
            return;
        }
        let p = std::mem::take(&mut self.path);
        // The path becomes a region by the fill mode.
        self.clip(Some((p, self.dc.even_odd)), mode);
    }

    // ---------------- shapes ----------------

    fn move_to(&mut self, p: Point) {
        self.dc.cur = p;
    }

    fn line_to(&mut self, p: Point) {
        let mut path = BezPath::new();
        path.move_to(self.dc.cur);
        path.line_to(p);
        self.dc.cur = p;
        self.draw(path, false, true);
    }

    /// One figure of points, drawn as `kind` says; a Bézier's points come in threes after the first.
    fn poly(&mut self, pts: &[Point], kind: Poly) {
        let mut p = BezPath::new();
        self.figure(&mut p, pts, kind);
        if matches!(kind, Poly::LineTo | Poly::BezierTo)
            && let Some(&last) = pts.last()
        {
            self.dc.cur = last;
        }
        self.draw(p, kind == Poly::Gon, true);
    }

    fn figure(&self, p: &mut BezPath, pts: &[Point], kind: Poly) {
        let (start, rest) = match kind {
            Poly::LineTo | Poly::BezierTo => (Some(self.dc.cur), pts),
            _ => (pts.first().copied(), pts.get(1..).unwrap_or(&[])),
        };
        let Some(start) = start else { return };
        p.move_to(start);
        match kind {
            Poly::Bezier | Poly::BezierTo => {
                for c in rest.as_chunks::<3>().0 {
                    p.curve_to(c[0], c[1], c[2]);
                }
            }
            _ => {
                for &q in rest {
                    p.line_to(q);
                }
            }
        }
        if kind == Poly::Gon {
            p.close_path();
        }
    }

    /// Several figures as one path: `counts` points each, from `pts`.
    fn poly_poly(&mut self, pts: &[Point], counts: impl Iterator<Item = usize>, gon: bool) {
        let mut p = BezPath::new();
        let mut at = 0usize;
        for n in counts {
            let Some(run) = pts.get(at..at.saturating_add(n)) else { break };
            self.figure(&mut p, run, if gon { Poly::Gon } else { Poly::Line });
            at += n;
        }
        self.draw(p, gon, true);
    }

    /// `PolyDraw`: points with `PT_*` types.
    fn poly_draw(&mut self, pts: &[Point], types: &[u8]) {
        let mut p = BezPath::new();
        let mut started = false;
        let mut i = 0;
        while i < pts.len().min(types.len()) {
            let t = types[i];
            match t & !1 {
                6 => {
                    p.move_to(pts[i]);
                    started = true;
                }
                4 if i + 2 < pts.len().min(types.len()) => {
                    if !started {
                        p.move_to(self.dc.cur);
                        started = true;
                    }
                    p.curve_to(pts[i], pts[i + 1], pts[i + 2]);
                    i += 2;
                }
                _ => {
                    if !started {
                        p.move_to(self.dc.cur);
                        started = true;
                    }
                    p.line_to(pts[i]);
                }
            }
            if types[i] & 1 != 0 {
                p.close_path();
            }
            self.dc.cur = pts[i];
            i += 1;
        }
        self.draw(p, false, true);
    }

    fn rectangle(&mut self, r: kurbo::Rect) {
        self.draw(r.abs().to_path(0.1), true, true);
    }

    /// A rect with elliptical corners `corner` (the corner ellipse's width and height) across.
    fn round_rect(&mut self, r: kurbo::Rect, corner: Vec2) {
        let r = r.abs();
        let (rx, ry) = ((corner.x.abs() / 2.0).min(r.width() / 2.0), (corner.y.abs() / 2.0).min(r.height() / 2.0));
        let mut p = BezPath::new();
        let quarter = |p: &mut BezPath, c: Point, a: f64| {
            p.extend(kurbo::Arc::new(c, (rx, ry), a, TAU / 4.0, 0.0).append_iter(tolerance(rx.min(ry))));
        };
        p.move_to((r.x0 + rx, r.y0));
        p.line_to((r.x1 - rx, r.y0));
        quarter(&mut p, Point::new(r.x1 - rx, r.y0 + ry), -TAU / 4.0);
        p.line_to((r.x1, r.y1 - ry));
        quarter(&mut p, Point::new(r.x1 - rx, r.y1 - ry), 0.0);
        p.line_to((r.x0 + rx, r.y1));
        quarter(&mut p, Point::new(r.x0 + rx, r.y1 - ry), TAU / 4.0);
        p.line_to((r.x0, r.y0 + ry));
        quarter(&mut p, Point::new(r.x0 + rx, r.y0 + ry), TAU / 2.0);
        p.close_path();
        self.draw(p, true, true);
    }

    fn ellipse(&mut self, r: kurbo::Rect) {
        let e = kurbo::Ellipse::from_rect(r.abs());
        let t = tolerance(e.radii().x.min(e.radii().y));
        self.draw(e.to_path(t), true, true);
    }

    /// The arc of the ellipse in `r` from the ray through `start` to the ray through `end`, in the context's
    /// direction (anticlockwise as drawn, by default).
    fn arc_of(&self, r: kurbo::Rect, start: Point, end: Point) -> kurbo::Arc {
        let r = r.abs();
        let (c, radii) = (r.center(), Vec2::new(r.width() / 2.0, r.height() / 2.0));
        let angle = |p: Point| ((p.y - c.y) / radii.y.max(1e-12)).atan2((p.x - c.x) / radii.x.max(1e-12));
        let (a0, a1) = (angle(start), angle(end));
        let sweep = if self.dc.clockwise { (a1 - a0).rem_euclid(TAU) } else { -(a0 - a1).rem_euclid(TAU) };
        let sweep = if sweep == 0.0 { if self.dc.clockwise { TAU } else { -TAU } } else { sweep };
        kurbo::Arc::new(c, radii, a0, sweep, 0.0)
    }

    /// `Arc` (open), `Chord` (closed by a line) or `Pie` (closed through the centre); `ArcTo` draws a line to the arc
    /// from the current position and moves it to the arc's end.
    fn arc(&mut self, r: kurbo::Rect, start: Point, end: Point, kind: u32) {
        let a = self.arc_of(r, start, end);
        let mut p = BezPath::new();
        let first = a.center + Vec2::new(a.radii.x * a.start_angle.cos(), a.radii.y * a.start_angle.sin());
        match kind {
            // Pie
            47 => {
                p.move_to(a.center);
                p.line_to(first);
            }
            // ArcTo
            55 => {
                p.move_to(self.dc.cur);
                p.line_to(first);
            }
            _ => p.move_to(first),
        }
        p.extend(a.append_iter(tolerance(a.radii.x.min(a.radii.y))));
        if matches!(kind, 46 | 47) {
            p.close_path();
        }
        if kind == 55 {
            self.dc.cur = p.elements().last().and_then(|e| e.end_point()).unwrap_or(self.dc.cur);
        }
        self.draw(p, matches!(kind, 46 | 47), true);
    }

    /// `AngleArc`: a line from the current position to the circle's arc from `start` degrees, `sweep` degrees
    /// anticlockwise; the current position moves to its end.
    fn angle_arc(&mut self, c: Point, radius: f64, start: f64, sweep: f64) {
        if !(start.is_finite() && sweep.is_finite()) {
            return;
        }
        let a = kurbo::Arc::new(c, (radius, radius), -start.to_radians(), -sweep.to_radians().clamp(-TAU, TAU), 0.0);
        let mut p = BezPath::new();
        p.move_to(self.dc.cur);
        p.line_to(c + Vec2::new(radius * a.start_angle.cos(), radius * a.start_angle.sin()));
        p.extend(a.append_iter(tolerance(radius)));
        self.dc.cur = p.elements().last().and_then(|e| e.end_point()).unwrap_or(self.dc.cur);
        self.draw(p, false, true);
    }

    /// Fills `r` (logical units) with `c`, outside the path bracket.
    fn fill_rect(&mut self, r: kurbo::Rect, c: [f32; 4]) {
        if self.in_path {
            return;
        }
        let p = self.rect_path(r);
        self.under_clip(COPY, |me| me.sink.fill(&p, false, &Brush::Solid(c)));
    }

    /// Fills `p` (in the picture's space) with the brush `b`, by the binary raster operation.
    fn fill_with(&mut self, p: &BezPath, b: &LogBrush) {
        if self.in_path {
            return;
        }
        if let Some(b) = self.brush(b) {
            self.under_clip(self.dc.rop2, |me| me.sink.fill(p, false, &b));
        }
    }

    /// `InvertRgn`: the region's colours inverted.
    fn invert(&mut self, p: &BezPath) {
        if !self.in_path {
            // Bit `2·s + d` set where d is 0: the destination inverted, whatever the source.
            self.under_clip(5, |me| me.sink.fill(p, false, &Brush::Solid([0.0, 0.0, 0.0, 1.0])));
        }
    }

    /// `FrameRgn`: the brush `b` painted `w` × `h` logical units in from the region's edges: the region less its
    /// part that stays inside it moved that far left, right, up and down.
    fn frame_with(&mut self, rects: &[kurbo::Rect], b: &LogBrush, w: f64, h: f64) {
        if self.in_path {
            return;
        }
        let moved = |dx: f64, dy: f64| rects.iter().map(|r| *r + Vec2::new(dx, dy)).collect::<Vec<_>>();
        let (l, r, u, d) = (moved(-w, 0.0), moved(w, 0.0), moved(0.0, -h), moved(0.0, h));
        let mut budget = 0usize;
        let frame = region::combine(
            &[rects, &l, &r, &u, &d],
            |m| m & 1 != 0 && m != 31,
            |n| {
                budget += n;
                budget <= 1 << 22
            },
        );
        if !self.spend(budget) {
            return;
        }
        let p = self.region(frame.into_iter(), false);
        self.fill_with(&p, b);
    }

    /// A gradient fill's rect from `a` to `b` (logical units), shading from `ca` to `cb` across or down.
    fn gradient_rect(&mut self, a: Point, b: Point, ca: [f32; 4], cb: [f32; 4], down: bool) {
        if self.in_path || !self.spend(4) {
            return;
        }
        let p = self.rect_path(kurbo::Rect::from_points(a, b));
        let end = if down { Point::new(a.x, b.y) } else { Point::new(b.x, a.y) };
        let g =
            SvgGradient { kind: GradientKind::Linear { start: a, end }, stops: vec![(0.0, ca), (1.0, cb)], spread: Spread::Pad, transform: self.m() };
        self.under_clip(COPY, |me| me.sink.fill(&p, false, &Brush::Gradient(g)));
    }

    /// `SetPixel`: a device pixel at the logical point `at`.
    fn pixel(&mut self, at: Point, c: u32) {
        if self.in_path {
            return;
        }
        let at = self.m() * at;
        let c = self.rgb(c);
        let px = self.px();
        let rect = kurbo::Rect::new(at.x, at.y, at.x + px, at.y + px).to_path(0.1);
        self.under_clip(COPY, |me| me.sink.fill(&rect, false, &Brush::Solid(c)));
    }

    /// A gradient fill's triangle (logical units), shaded between its corners' colours.
    fn gradient_triangle(&mut self, pts: [Point; 3], cs: [[f32; 4]; 3]) {
        if self.in_path || !self.spend(4) {
            return;
        }
        let m = self.m();
        self.under_clip(COPY, |me| me.sink.triangle(pts.map(|q| m * q), cs));
    }

    /// Counts a bitmap's pixels: false past what one metafile may draw.
    fn spend_pixels(&self, w: u32, h: u32) -> bool {
        if twip_graphics::image::fits(w, h).is_err() {
            return false;
        }
        self.pixels.set(self.pixels.get().saturating_add(u64::from(w) * u64::from(h)));
        self.pixels.get() <= MAX_PIXELS
    }

    /// Draws a line of text, or adds its glyphs' outlines to the open path. Glyph-index text (`ETO_GLYPH_INDEX`)
    /// draws only in the face it names.
    fn text(&mut self, t: TextOut<'_>) {
        if !self.spend(t.chars.len()) {
            return;
        }
        let m = self.m();
        if let Some(r) = t.rect
            && t.opaque
        {
            self.fill_rect(r, self.rgb(self.dc.bk_colour));
        }
        let update_cp = self.dc.text_align & 1 != 0;
        let at = if update_cp { self.dc.cur } else { t.at };
        // The line's space: upright at the reference point, an em of the logical font's height across, its baseline
        // along the logical x axis turned by the escapement.
        let (ux, uy) = (m * Point::new(1.0, 0.0) - m * Point::ZERO, m * Point::new(0.0, 1.0) - m * Point::ZERO);
        let (kx, ky) = (ux.hypot(), uy.hypot());
        if !(kx > 0.0 && ky > 0.0) {
            return;
        }
        let f = Rc::clone(&self.dc.font);
        let turn = ux.y.atan2(ux.x) - (f.escapement / 10.0).to_radians();
        let transform = Affine::translate((m * at).to_vec2()) * Affine::rotate(turn) * Affine::scale(ky);
        let (mut scaled, mut rises) = (std::mem::take(&mut self.advances), std::mem::take(&mut self.rises));
        scaled.clear();
        rises.clear();
        if let Some(a) = t.advances {
            scaled.extend(a.iter().map(|d| d * kx / ky));
        }
        // A rise is up the font, whatever the mapping: down the line's y axis negated.
        if let Some(r) = t.rises {
            rises.extend(r.iter().map(|d| -d));
        }
        let a = self.dc.text_align;
        let align = (
            match a & 6 {
                6 => 1,
                2 => 2,
                _ => 0,
            },
            match a & 24 {
                24 => 1,
                8 => 2,
                _ => 0,
            },
        );
        let text = Text {
            family: &f.family,
            bold: f.weight >= 600,
            italic: f.italic,
            underline: f.underline,
            strike: f.strike,
            height: if f.height == 0.0 { 12.0 } else { f.height.abs() },
            cell: f.height > 0.0,
            chars: t.chars,
            glyph_ids: t.glyph_ids,
            advances: t.advances.map(|_| &scaled[..]),
            extra: t.extra.0 * kx / ky,
            space_extra: t.extra.1 * kx / ky,
            rises: t.rises.map(|_| &rises[..]),
            align,
            transform,
            colour: self.rgb(self.dc.text_colour),
            background: (self.dc.opaque && !t.opaque).then(|| self.rgb(self.dc.bk_colour)),
        };
        let advance = if self.in_path {
            let mut outline = BezPath::new();
            let advance = self.sink.text_outline(&text, &mut outline);
            if self.spend(outline.elements().len()) {
                self.append(outline);
            }
            advance
        } else {
            let clip = t.rect.filter(|_| t.clipped).map(|r| self.rect_path(r));
            let mut advance = 0.0;
            self.under_clip(COPY, |me| {
                if let Some(p) = &clip {
                    me.sink.save();
                    me.sink.clip_path(p, false);
                }
                advance = me.sink.text(&text);
                if clip.is_some() {
                    me.sink.restore();
                }
            });
            advance
        };
        self.advances = scaled;
        self.rises = rises;
        if update_cp {
            self.dc.cur.x += advance * ky / kx;
        }
    }
}

/// The clip `c` combined with `region` by `mode` (`RGN_*`), `everything` a rect far outside the picture; `None` when
/// it stays as it is.
fn combine(c: &ClipState, region: Option<(BezPath, bool)>, mode: u32, everything: BezPath) -> Option<ClipState> {
    use ClipState::{Mask, Shapes};
    let (r, even_odd) = match (mode, region) {
        (RGN_COPY, None) => return Some(Shapes(Vec::new())),
        (_, None) => return None,
        (_, Some(r)) => r,
    };
    let next = match (mode, c) {
        (RGN_COPY, _) => Shapes(vec![(r, even_odd)]),
        (RGN_AND, Shapes(v)) => Shapes(v.iter().cloned().chain([(r, even_odd)]).collect()),
        // Even-odd, the region's complement is it with a far rect around it.
        (RGN_DIFF, Shapes(v)) if even_odd => {
            let mut outside = everything;
            outside.extend(r);
            Shapes(v.iter().cloned().chain([(outside, true)]).collect())
        }
        (RGN_OR, Shapes(v)) if v.is_empty() => return None,
        (RGN_AND, Mask(m)) => {
            // What was kept, and within the region.
            let mut steps = vec![MaskStep::Clip(r, even_odd)];
            steps.extend(m.iter().cloned());
            steps.push(MaskStep::Pop);
            Mask(steps.into())
        }
        (RGN_DIFF | RGN_OR | RGN_XOR, _) => {
            let mut steps = match c {
                Shapes(v) => program(v, &everything),
                Mask(m) => m.to_vec(),
            };
            steps.push(match mode {
                RGN_DIFF => MaskStep::Remove(r, even_odd),
                RGN_OR => MaskStep::Add(r, even_odd),
                _ => MaskStep::Toggle(r, even_odd),
            });
            Mask(steps.into())
        }
        _ => return None,
    };
    let fits = match &next {
        Shapes(v) => v.len() <= MAX_CLIPS,
        Mask(m) => m.len() <= MAX_MASK_STEPS,
    };
    fits.then_some(next)
}

/// The mask of an intersection of shapes: everything, clipped to each.
fn program(v: &[(BezPath, bool)], everything: &BezPath) -> Vec<MaskStep> {
    let mut steps: Vec<MaskStep> = v.iter().map(|(p, e)| MaskStep::Clip(p.clone(), *e)).collect();
    steps.push(MaskStep::Add(everything.clone(), false));
    steps.extend(v.iter().map(|_| MaskStep::Pop));
    steps
}

/// The dashes stroking `p` with `s` makes, about: along its control polygon, at most what the recorder walks before it
/// strokes solid.
fn dash_work(p: &BezPath, s: &kurbo::Stroke) -> usize {
    let period: f64 = s.dash_pattern.iter().sum();
    let (mut len, mut at, mut start) = (0.0, Point::ZERO, Point::ZERO);
    for el in p.elements() {
        let to = match *el {
            PathEl::MoveTo(q) => {
                start = q;
                at = q;
                continue;
            }
            PathEl::LineTo(q) => q,
            PathEl::QuadTo(a, q) => {
                len += (a - at).hypot();
                at = a;
                q
            }
            PathEl::CurveTo(a, b, q) => {
                len += (a - at).hypot() + (b - a).hypot();
                at = b;
                q
            }
            PathEl::ClosePath => start,
        };
        len += (to - at).hypot();
        at = to;
    }
    let n = len / period * s.dash_pattern.len() as f64;
    if n.is_finite() { n.min(200_000.0) as usize } else { 200_000 }
}

/// A flattening tolerance for curves of radius `r`: a ten-thousandth of it.
fn tolerance(r: f64) -> f64 {
    (r.abs() * 1e-4).max(1e-9)
}
