//! EMF+: the GDI+ records an EMF carries in its comments, read into the player. The state they keep beyond the
//! player's (the object table, the world and page transforms, the clip, saved states and containers) lives here;
//! what they draw goes through the player's clip and budgets.

use std::collections::HashMap;
use std::f64::consts::TAU;
use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::kurbo::{self, Affine, BezPath, Point, Shape, Vec2};

use super::plus_objects::{self as obj, Attributes, Cur, Fill, Font, Format, Image, Node, Path, Pen};
use super::{ClipState, MAX_CLIPS, MAX_MASK_STEPS, MAX_RECORDS, MAX_SAVES, Player};
use crate::render::svg::{Combine, MaskStep, Sink};

/// `EmfPlusRecordType` values.
const HEADER: u16 = 0x4001;
const GET_DC: u16 = 0x4004;
const OBJECT: u16 = 0x4008;
const CLEAR: u16 = 0x4009;
const FILL_RECTS: u16 = 0x400A;
const DRAW_RECTS: u16 = 0x400B;
const FILL_POLYGON: u16 = 0x400C;
const DRAW_LINES: u16 = 0x400D;
const FILL_ELLIPSE: u16 = 0x400E;
const DRAW_ELLIPSE: u16 = 0x400F;
const FILL_PIE: u16 = 0x4010;
const DRAW_PIE: u16 = 0x4011;
const DRAW_ARC: u16 = 0x4012;
const FILL_REGION: u16 = 0x4013;
const FILL_PATH: u16 = 0x4014;
const DRAW_PATH: u16 = 0x4015;
const FILL_CLOSED_CURVE: u16 = 0x4016;
const DRAW_CLOSED_CURVE: u16 = 0x4017;
const DRAW_CURVE: u16 = 0x4018;
const DRAW_BEZIERS: u16 = 0x4019;
const DRAW_IMAGE: u16 = 0x401A;
const DRAW_IMAGE_POINTS: u16 = 0x401B;
const DRAW_STRING: u16 = 0x401C;
const SET_RENDERING_ORIGIN: u16 = 0x401D;
const SET_ANTI_ALIAS_MODE: u16 = 0x401E;
const SET_INTERPOLATION_MODE: u16 = 0x4021;
const SET_COMPOSITING_MODE: u16 = 0x4023;
const SAVE: u16 = 0x4025;
const RESTORE: u16 = 0x4026;
const BEGIN_CONTAINER: u16 = 0x4027;
const BEGIN_CONTAINER_NO_PARAMS: u16 = 0x4028;
const END_CONTAINER: u16 = 0x4029;
const SET_WORLD_TRANSFORM: u16 = 0x402A;
const RESET_WORLD_TRANSFORM: u16 = 0x402B;
const MULTIPLY_WORLD_TRANSFORM: u16 = 0x402C;
const TRANSLATE_WORLD_TRANSFORM: u16 = 0x402D;
const SCALE_WORLD_TRANSFORM: u16 = 0x402E;
const ROTATE_WORLD_TRANSFORM: u16 = 0x402F;
const SET_PAGE_TRANSFORM: u16 = 0x4030;
const RESET_CLIP: u16 = 0x4031;
const SET_CLIP_RECT: u16 = 0x4032;
const SET_CLIP_PATH: u16 = 0x4033;
const SET_CLIP_REGION: u16 = 0x4034;
const OFFSET_CLIP: u16 = 0x4035;
const DRAW_DRIVER_STRING: u16 = 0x4036;

/// Record flags: the brush is a colour; 16-bit points and rects; relative points; a transform applied after the
/// world transform; a closed figure (and a winding fill).
pub(super) const COLOUR: u16 = 0x8000;
const SHORT: u16 = 0x4000;
const RELATIVE: u16 = 0x0800;
const APPEND: u16 = 0x2000;
const CLOSED: u16 = 0x2000;

/// The object table's size.
const OBJECTS: usize = 64;

/// An object in the table.
#[derive(Clone)]
pub(super) enum Obj {
    Brush(Rc<Fill>),
    Pen(Rc<Pen>),
    Path(Rc<Path>),
    Region(Rc<[Node]>),
    Image(Rc<Image>),
    Font(Rc<Font>),
    Format(Rc<Format>),
    Attributes(Attributes),
}

/// The graphics state `Save` and `BeginContainer` keep: the transforms, the clip, the rendering origin and how
/// images and colours draw.
#[derive(Clone)]
pub(super) struct State {
    pub(super) world: Affine,
    unit: u32,
    scale: f64,
    /// A container's mapping: from inside it to the world outside.
    base: Affine,
    pub(super) clip: Rc<ClipState>,
    /// The clip a container's own clips are inside.
    base_clip: Rc<ClipState>,
    /// Where hatches start, in device pixels.
    pub(super) origin: Point,
    pub(super) smooth: bool,
    pub(super) copy: bool,
    /// Fills and strokes anti-aliased (`SetAntiAliasMode`); GDI+'s default smoothing doesn't.
    pub(super) antialias: bool,
}

/// An object whose bytes arrive in parts: its record's flags, its whole size and the bytes so far.
struct Pending {
    flags: u16,
    total: usize,
    bytes: Vec<u8>,
}

/// The EMF+ player's state.
pub(super) struct Plus {
    /// An EMF+ header was read: plain records play only after a `GetDC`.
    on: bool,
    /// The last comment's records asked for the device context: the plain records after it play.
    dc: bool,
    dpi: Vec2,
    objects: Vec<Option<Obj>>,
    pending: Option<Pending>,
    pub(super) s: State,
    stack: Vec<(u32, State)>,
    /// The player's clip is EMF+'s, and the plain records' is kept here.
    holding: bool,
    gdi: Rc<ClipState>,
    /// The sink is in copy mode, or draws aliased, for the record being drawn.
    copying: bool,
    aliasing: bool,
    records: u32,
    /// Hatch tiles by style and colours, and mirrored tiles by bitmap address and wrap mode (the bitmap kept, so its
    /// address isn't another's).
    pub(super) tiles: HashMap<(u32, [u8; 8]), Arc<Vec<u8>>>,
    pub(super) flips: HashMap<(usize, u32), (Arc<Vec<u8>>, Arc<Vec<u8>>)>,
    /// Scratch for text.
    pub(super) chars: Vec<u32>,
    pub(super) advances: Vec<f64>,
}

impl Default for Plus {
    fn default() -> Plus {
        let none = Rc::new(ClipState::Shapes(Vec::new()));
        Plus {
            on: false,
            dc: false,
            dpi: Vec2::new(96.0, 96.0),
            objects: vec![None; OBJECTS],
            pending: None,
            s: State {
                world: Affine::IDENTITY,
                unit: 2,
                scale: 1.0,
                base: Affine::IDENTITY,
                clip: Rc::clone(&none),
                base_clip: Rc::clone(&none),
                origin: Point::ZERO,
                smooth: true,
                copy: false,
                antialias: false,
            },
            stack: Vec::new(),
            holding: false,
            gdi: none,
            copying: false,
            aliasing: false,
            records: 0,
            tiles: HashMap::new(),
            flips: HashMap::new(),
            chars: Vec::new(),
            advances: Vec::new(),
        }
    }
}

/// Whether the EMF+ records `d` replace what is under them: a `Clear`, or copy compositing; each record looked at
/// taken from `left`, and none past it.
pub(super) fn replaces(d: &[u8], left: &mut usize) -> bool {
    let mut at = 0usize;
    while d.len() - at >= 12 {
        let Some(l) = left.checked_sub(1) else { return false };
        *left = l;
        let mut c = Cur::new(&d[at..]);
        let (Some(kind), Some(flags), Some(size)) = (c.u16(), c.u16(), c.u32()) else { break };
        let size = size as usize;
        if size < 12 || !size.is_multiple_of(4) || size > d.len() - at {
            break;
        }
        if kind == CLEAR || kind == SET_COMPOSITING_MODE && flags & 0xFF == 1 {
            return true;
        }
        at += size;
    }
    false
}

impl Plus {
    /// Whether a plain record plays now: always before any EMF+, after it only behind a `GetDC`. When it does, the
    /// plain records' own clip is back in the player.
    pub(super) fn plain<S: Sink>(&mut self, p: &mut Player<'_, S>) -> bool {
        if !self.on {
            return true;
        }
        if self.dc && self.holding {
            self.holding = false;
            p.dc.clip = Rc::clone(&self.gdi);
            p.clip_dirty = true;
        }
        self.dc
    }

    /// Plays the EMF+ records `d` (one comment's).
    pub(super) fn play<S: Sink>(&mut self, p: &mut Player<'_, S>, d: &[u8]) {
        self.dc = false;
        let mut at = 0usize;
        while d.len() - at >= 12 && !p.stopped.get() && self.records < MAX_RECORDS {
            let mut c = Cur::new(&d[at..]);
            let (Some(kind), Some(flags), Some(size), Some(len)) = (c.u16(), c.u16(), c.u32(), c.u32()) else { break };
            let size = size as usize;
            if size < 12 || !size.is_multiple_of(4) || size > d.len() - at {
                break;
            }
            let data = &d[at + 12..at + 12 + (len as usize).min(size - 12)];
            self.records += 1;
            if !p.spend(1) {
                break;
            }
            at += size;
            if kind == HEADER {
                let mut h = Cur::new(data);
                h.take(8);
                let dpi = |v: Option<u32>| v.filter(|v| (1..=100_000).contains(v)).map_or(96.0, f64::from);
                self.dpi = Vec2::new(dpi(h.u32()), dpi(h.u32()));
                self.on = true;
                continue;
            }
            if !self.on {
                continue;
            }
            if kind == OBJECT && self.part(p, flags, data) {
                continue;
            }
            self.finish_part(p);
            // A record too short for what it holds draws nothing, and the next is read.
            let _ = self.record(p, kind, flags, data);
            if std::mem::take(&mut self.copying) {
                p.sink.set_copy(false);
            }
            if std::mem::take(&mut self.aliasing) {
                p.sink.set_antialias(true);
            }
        }
    }

    // ---------------- objects ----------------

    /// An object record's bytes, when they are part of an object that arrives in parts; true when taken.
    fn part<S: Sink>(&mut self, p: &mut Player<'_, S>, flags: u16, data: &[u8]) -> bool {
        let key = flags & 0x7FFF;
        if let Some(pd) = &mut self.pending
            && pd.flags == key
        {
            // Each part starts with the object's whole size.
            let more = data.get(4..).unwrap_or(&[]);
            let room = pd.total.saturating_sub(pd.bytes.len());
            pd.bytes.extend_from_slice(&more[..more.len().min(room)]);
            if flags & 0x8000 == 0 || pd.bytes.len() >= pd.total {
                self.finish_part(p);
            }
            return true;
        }
        self.finish_part(p);
        if flags & 0x8000 == 0 {
            return false;
        }
        let mut c = Cur::new(data);
        let Some(total) = c.u32() else { return true };
        let rest = c.rest();
        let total = total as usize;
        self.pending = Some(Pending { flags: key, total, bytes: rest[..rest.len().min(total)].to_vec() });
        if rest.len() >= total {
            self.finish_part(p);
        }
        true
    }

    fn finish_part<S: Sink>(&mut self, p: &mut Player<'_, S>) {
        if let Some(pd) = self.pending.take() {
            self.object(p, pd.flags, &pd.bytes);
        }
    }

    /// Reads an object into the table at its id.
    fn object<S: Sink>(&mut self, p: &mut Player<'_, S>, flags: u16, d: &[u8]) {
        let id = usize::from(flags & 0xFF);
        if id >= OBJECTS {
            return;
        }
        let spend = |w: u32, h: u32| p.spend_pixels(w, h);
        let mut c = Cur::new(d);
        let o = match (flags >> 8) & 0x7F {
            1 => obj::brush(&mut c, &spend).map(|b| Obj::Brush(Rc::new(b))),
            2 => obj::pen(&mut c, &spend).map(|b| Obj::Pen(Rc::new(b))),
            3 => obj::path(d).map(|b| Obj::Path(Rc::new(b))),
            4 => obj::region(&mut c).map(|r| Obj::Region(r.into())),
            5 => obj::image(d, &spend).map(|i| Obj::Image(Rc::new(i))),
            6 => obj::font(&mut c).map(|f| Obj::Font(Rc::new(f))),
            7 => obj::format(&mut c).map(|f| Obj::Format(Rc::new(f))),
            8 => obj::attributes(&mut c).map(Obj::Attributes),
            _ => None,
        };
        // Reading an object is work as drawing is: each byte of it counted a sixteenth.
        p.spend(d.len() / 16);
        self.objects[id] = o;
    }

    fn get(&self, id: u32) -> Option<&Obj> {
        self.objects.get(id as usize)?.as_ref()
    }

    /// The brush a record names: a colour when `flags` says so, else the brush object.
    pub(super) fn fill(&self, flags: u16, id: u32) -> Option<Rc<Fill>> {
        if flags & COLOUR != 0 {
            return Some(Rc::new(Fill::Solid(obj::argb(id))));
        }
        match self.get(id)? {
            Obj::Brush(b) => Some(Rc::clone(b)),
            _ => None,
        }
    }

    fn pen(&self, id: u32) -> Option<Rc<Pen>> {
        match self.get(id)? {
            Obj::Pen(p) => Some(Rc::clone(p)),
            _ => None,
        }
    }

    fn path(&self, id: u32) -> Option<Rc<Path>> {
        match self.get(id)? {
            Obj::Path(p) => Some(Rc::clone(p)),
            _ => None,
        }
    }

    // ---------------- coordinates ----------------

    /// Device pixels per `unit` (`UnitType`) along an axis of `dpi`.
    fn per(unit: u32, dpi: f64) -> f64 {
        match unit {
            3 => dpi / 72.0,
            4 => dpi,
            5 => dpi / 300.0,
            6 => dpi / 25.4,
            _ => 1.0,
        }
    }

    /// World units to the picture's space: the world transform, a container's, the page's, then device pixels.
    pub(super) fn w<S: Sink>(&self, p: &Player<'_, S>) -> Affine {
        let k = Vec2::new(Self::per(self.s.unit, self.dpi.x), Self::per(self.s.unit, self.dpi.y)) * self.s.scale;
        p.dev * Affine::scale_non_uniform(k.x, k.y) * self.s.base * self.s.world
    }

    /// A length in `unit` as world units: as it is in world units, else through the page's unit.
    pub(super) fn to_world(&self, v: f64, unit: u32) -> f64 {
        if unit == 0 { v } else { v * Self::per(unit, self.dpi.y) / (Self::per(self.s.unit, self.dpi.y) * self.s.scale) }
    }

    fn transform(&mut self, m: Affine, flags: u16) {
        let w = &mut self.s.world;
        // GDI+ multiplies row vectors: appending applies the matrix after the world transform.
        *w = if flags & APPEND != 0 { m * *w } else { *w * m };
        if !w.is_finite() || w.determinant() == 0.0 {
            *w = Affine::IDENTITY;
        }
    }

    // ---------------- state ----------------

    fn save(&mut self, id: u32) {
        if self.stack.len() < MAX_SAVES {
            self.stack.push((id, self.s.clone()));
        }
    }

    /// Back to the state saved as `id`, dropping those saved after it.
    fn restore<S: Sink>(&mut self, p: &Player<'_, S>, id: u32) {
        // Looking for it is work: a restore of a state never saved, under a full stack, is that stack's length.
        if !p.spend(self.stack.len()) {
            return;
        }
        if let Some(i) = self.stack.iter().rposition(|(k, _)| *k == id) {
            self.stack.truncate(i + 1);
            if let Some((_, s)) = self.stack.pop() {
                self.s = s;
            }
        }
    }

    /// `BeginContainer`: the state saved, and from inside the container `src` (in `unit`) maps onto `dst`.
    fn begin_container(&mut self, id: u32, dst: kurbo::Rect, src: kurbo::Rect, unit: u32) {
        self.save(id);
        let k = |v: f64| self.to_world(v, unit);
        let src = kurbo::Rect::new(k(src.x0), k(src.y0), k(src.x1), k(src.y1));
        let map = if src.width() != 0.0 && src.height() != 0.0 {
            Affine::translate((dst.x0, dst.y0))
                * Affine::scale_non_uniform(dst.width() / src.width(), dst.height() / src.height())
                * Affine::translate((-src.x0, -src.y0))
        } else {
            Affine::IDENTITY
        };
        self.enter(map);
    }

    /// Inside a container: its mapping after the world transform, the world transform reset, and the clip what its
    /// clips stay inside.
    fn enter(&mut self, map: Affine) {
        let base = self.s.base * self.s.world * map;
        if base.is_finite() && base.determinant() != 0.0 {
            self.s.base = base;
        }
        self.s.world = Affine::IDENTITY;
        self.s.base_clip = Rc::clone(&self.s.clip);
    }

    // ---------------- clip ----------------

    /// The player draws under EMF+'s clip, the plain records' kept.
    pub(super) fn use_clip<S: Sink>(&mut self, p: &mut Player<'_, S>) {
        if !self.holding {
            self.gdi = Rc::clone(&p.dc.clip);
            self.holding = true;
        }
        if !Rc::ptr_eq(&p.dc.clip, &self.s.clip) {
            p.dc.clip = Rc::clone(&self.s.clip);
            p.clip_dirty = true;
        }
        if self.s.copy && !self.copying {
            p.sink.set_copy(true);
            self.copying = true;
        }
        if !self.s.antialias && !self.aliasing {
            p.sink.set_antialias(false);
            self.aliasing = true;
        }
    }

    /// A region's nodes as mask steps (from no coverage), in the picture's space.
    fn region_steps<S: Sink>(&self, p: &Player<'_, S>, nodes: &[Node]) -> Option<Vec<MaskStep>> {
        let w = self.w(p);
        let mut steps = Vec::new();
        // The combinations open, and how many operands of each are done.
        let mut open: Vec<(Combine, u8)> = Vec::new();
        for n in nodes {
            if !p.spend(4) {
                return None;
            }
            match n {
                Node::Combine(t) => {
                    open.push((combine_mode(*t)?, 0));
                    continue;
                }
                Node::Rect(r) => steps.push(MaskStep::Add(rect_path(*r, w), false)),
                Node::Path(b) => {
                    if !p.spend(b.elements().len()) {
                        return None;
                    }
                    let mut b = b.clone();
                    b.apply_affine(w);
                    steps.push(MaskStep::Add(b, true));
                }
                Node::Empty => {}
                Node::Infinite => steps.push(MaskStep::Add(p.everything(), false)),
            }
            // An operand done: after a left one the group opens; after a right one it closes, and is an operand.
            while let Some(top) = open.last_mut() {
                top.1 += 1;
                if top.1 == 1 {
                    steps.push(MaskStep::Group(top.0));
                    break;
                }
                steps.push(MaskStep::Pop);
                open.pop();
            }
        }
        (steps.len() <= MAX_MASK_STEPS).then_some(steps)
    }

    /// Combines the clip with `area` (`None` everything) by `mode` (`CombineMode`); a container's clips stay
    /// inside its own.
    fn clip<S: Sink>(&mut self, p: &Player<'_, S>, area: Area, mode: u32) {
        let everything = p.everything();
        let cost = self.s.clip.cost() + self.s.base_clip.cost() + area.cost();
        if !p.spend(cost) {
            return;
        }
        let Some(next) = combined(&self.s.clip, area, mode, &everything) else { return };
        let next = if matches!(mode, 1 | 4) { Some(next) } else { intersect(next, &self.s.base_clip, &everything) };
        if let Some(next) = next.filter(fits) {
            self.s.clip = Rc::new(next);
        }
    }

    fn offset_clip(&mut self, d: Vec2, w: Affine) {
        let v = w * Point::new(d.x, d.y) - w * Point::ZERO;
        self.s.clip = Rc::new(self.s.clip.moved(Affine::translate(v)));
    }

    // ---------------- records ----------------

    fn record<S: Sink>(&mut self, p: &mut Player<'_, S>, kind: u16, flags: u16, d: &[u8]) -> Option<()> {
        let mut c = Cur::new(d);
        let short = flags & SHORT != 0;
        let pts = |c: &mut Cur<'_>, n: u32| obj::points(c, n, u32::from(flags & (SHORT | RELATIVE)));
        let id = u32::from(flags & 0xFF);
        match kind {
            GET_DC => self.dc = true,
            OBJECT => self.object(p, flags, d),
            CLEAR => {
                let colour = obj::argb(c.u32()?);
                let all = p.everything();
                self.use_clip(p);
                p.sink.set_copy(true);
                self.copying = true;
                p.under_clip(super::COPY, |me| me.sink.fill(&all, false, &crate::render::svg::Brush::Solid(colour)));
            }
            FILL_RECTS => {
                let fill = self.fill(flags, c.u32()?)?;
                let n = c.u32()? as usize;
                if n > c.left() / if short { 8 } else { 16 } {
                    return None;
                }
                let w = self.w(p);
                for _ in 0..n {
                    let r = rect_path(c.rect(short)?, w);
                    self.paint(p, &r, true, &fill);
                }
            }
            DRAW_RECTS => {
                let pen = self.pen(id)?;
                let n = c.u32()? as usize;
                if n > c.left() / if short { 8 } else { 16 } {
                    return None;
                }
                let mut path = BezPath::new();
                for _ in 0..n {
                    path.extend(c.rect(short)?.abs().path_elements(0.1));
                }
                self.stroke(p, &path, true, &pen);
            }
            FILL_POLYGON => {
                let fill = self.fill(flags, c.u32()?)?;
                let n = c.u32()?;
                let path = polygon(&pts(&mut c, n)?, true);
                self.fill_world(p, path, true, &fill);
            }
            DRAW_LINES => {
                let pen = self.pen(id)?;
                let n = c.u32()?;
                let closed = flags & CLOSED != 0;
                self.stroke(p, &polygon(&pts(&mut c, n)?, closed), closed, &pen);
            }
            FILL_ELLIPSE => {
                let fill = self.fill(flags, c.u32()?)?;
                self.fill_world(p, ellipse(c.rect(short)?), false, &fill);
            }
            DRAW_ELLIPSE => {
                let pen = self.pen(id)?;
                self.stroke(p, &ellipse(c.rect(short)?), true, &pen);
            }
            FILL_PIE => {
                let fill = self.fill(flags, c.u32()?)?;
                let (start, sweep) = (c.f32()?, c.f32()?);
                self.fill_world(p, arc(c.rect(short)?, start, sweep, true), false, &fill);
            }
            DRAW_PIE | DRAW_ARC => {
                let pen = self.pen(id)?;
                let (start, sweep) = (c.f32()?, c.f32()?);
                let pie = kind == DRAW_PIE;
                self.stroke(p, &arc(c.rect(short)?, start, sweep, pie), pie, &pen);
            }
            FILL_REGION => {
                let fill = self.fill(flags, c.u32()?)?;
                let Some(Obj::Region(nodes)) = self.get(id).cloned() else { return None };
                self.fill_region(p, &nodes, &fill);
            }
            FILL_PATH => {
                let fill = self.fill(flags, c.u32()?)?;
                let path = self.path(id)?;
                self.fill_world(p, path.path.clone(), true, &fill);
            }
            DRAW_PATH => {
                let pen = self.pen(c.u32()?)?;
                let path = self.path(id)?;
                self.stroke(p, &path.path, path.closed, &pen);
            }
            FILL_CLOSED_CURVE => {
                let fill = self.fill(flags, c.u32()?)?;
                let tension = c.f32()?;
                let n = c.u32()?;
                let path = cardinal(&pts(&mut c, n)?, tension, true, (0, usize::MAX));
                self.fill_world(p, path, flags & CLOSED == 0, &fill);
            }
            DRAW_CLOSED_CURVE => {
                let pen = self.pen(id)?;
                let tension = c.f32()?;
                let n = c.u32()?;
                self.stroke(p, &cardinal(&pts(&mut c, n)?, tension, true, (0, usize::MAX)), true, &pen);
            }
            DRAW_CURVE => {
                let pen = self.pen(id)?;
                let (tension, offset, segments, n) = (c.f32()?, c.u32()? as usize, c.u32()? as usize, c.u32()?);
                let pts = obj::points(&mut c, n, u32::from(flags & SHORT))?;
                self.stroke(p, &cardinal(&pts, tension, false, (offset, segments)), false, &pen);
            }
            DRAW_BEZIERS => {
                let pen = self.pen(id)?;
                let n = c.u32()?;
                let pts = pts(&mut c, n)?;
                let mut path = BezPath::new();
                if let Some((first, rest)) = pts.split_first() {
                    path.move_to(*first);
                    for q in rest.as_chunks::<3>().0 {
                        path.curve_to(q[0], q[1], q[2]);
                    }
                }
                self.stroke(p, &path, false, &pen);
            }
            DRAW_IMAGE | DRAW_IMAGE_POINTS => {
                let Some(Obj::Image(image)) = self.get(id).cloned() else { return None };
                let attributes = match self.get(c.u32()?) {
                    Some(Obj::Attributes(a)) => Some(*a),
                    _ => None,
                };
                let unit = c.u32()?;
                let src = c.rect(false)?;
                let to = if kind == DRAW_IMAGE {
                    let r = c.rect(short)?;
                    Affine::new([r.width(), 0.0, 0.0, r.height(), r.x0, r.y0])
                } else {
                    let n = c.u32()?;
                    let q = pts(&mut c, n)?;
                    let [a, b, d] = q.get(..3)?.try_into().ok()?;
                    Affine::new([b.x - a.x, b.y - a.y, d.x - a.x, d.y - a.y, a.x, a.y])
                };
                // The source rect in the image's pixels, at 96 to the inch.
                let k = Self::per(unit, 96.0);
                let src = kurbo::Rect::new(src.x0 * k, src.y0 * k, src.x1 * k, src.y1 * k);
                self.image(p, &image, src, self.w(p) * to, attributes);
            }
            DRAW_STRING => {
                let fill = self.fill(flags, c.u32()?)?;
                let format = match self.get(c.u32()?) {
                    Some(Obj::Format(f)) => Rc::clone(f),
                    _ => Rc::new(Format::default()),
                };
                let n = c.u32()? as usize;
                let rect = c.rect(false)?;
                let Some(Obj::Font(font)) = self.get(id).cloned() else { return None };
                let mut chars = std::mem::take(&mut self.chars);
                chars.clear();
                super::emf::utf16(&mut chars, c.take(n.checked_mul(2)?)?);
                self.string(p, &font, &fill, &format, rect, &chars);
                self.chars = chars;
            }
            DRAW_DRIVER_STRING => {
                let fill = self.fill(flags, c.u32()?)?;
                let (options, has_matrix, n) = (c.u32()?, c.u32()?, c.u32()? as usize);
                let glyphs = c.take(n.checked_mul(2)?)?;
                let at = obj::points(&mut c, n as u32, 0)?;
                let m = if has_matrix != 0 { c.matrix()? } else { Affine::IDENTITY };
                let Some(Obj::Font(font)) = self.get(id).cloned() else { return None };
                let mut chars = std::mem::take(&mut self.chars);
                chars.clear();
                chars.extend(glyphs.as_chunks::<2>().0.iter().map(|g| u32::from(u16::from_le_bytes(*g))));
                self.driver_string(p, &font, &fill, options, &chars, &at, m);
                self.chars = chars;
            }
            SET_RENDERING_ORIGIN => self.s.origin = Point::new(f64::from(c.i32()?), f64::from(c.i32()?)),
            SET_INTERPOLATION_MODE => self.s.smooth = flags & 0xFF != 5,
            SET_COMPOSITING_MODE => self.s.copy = flags & 0xFF == 1,
            SET_ANTI_ALIAS_MODE => self.s.antialias = flags & 1 != 0,
            SAVE => self.save(c.u32()?),
            RESTORE | END_CONTAINER => self.restore(p, c.u32()?),
            BEGIN_CONTAINER => {
                let (dst, src) = (c.rect(false)?, c.rect(false)?);
                self.begin_container(c.u32()?, dst, src, u32::from(flags & 0xFF));
            }
            BEGIN_CONTAINER_NO_PARAMS => {
                self.save(c.u32()?);
                self.enter(Affine::IDENTITY);
            }
            SET_WORLD_TRANSFORM => {
                let m = c.matrix()?;
                self.s.world = if m.is_finite() && m.determinant() != 0.0 { m } else { Affine::IDENTITY };
            }
            RESET_WORLD_TRANSFORM => self.s.world = Affine::IDENTITY,
            MULTIPLY_WORLD_TRANSFORM => self.transform(c.matrix()?, flags),
            TRANSLATE_WORLD_TRANSFORM => self.transform(Affine::translate((c.f32()?, c.f32()?)), flags),
            SCALE_WORLD_TRANSFORM => self.transform(Affine::scale_non_uniform(c.f32()?, c.f32()?), flags),
            ROTATE_WORLD_TRANSFORM => self.transform(Affine::rotate(c.f32()?.to_radians()), flags),
            SET_PAGE_TRANSFORM => {
                let scale = c.f32()?;
                self.s.unit = u32::from(flags & 0xFF);
                self.s.scale = if scale > 1e-9 && scale < 1e9 { scale } else { 1.0 };
            }
            RESET_CLIP => self.s.clip = Rc::clone(&self.s.base_clip),
            SET_CLIP_RECT => {
                let r = rect_path(c.rect(false)?, self.w(p));
                self.clip(p, Area::Path(r, false), u32::from(flags >> 8 & 0xF));
            }
            SET_CLIP_PATH => {
                let mut path = self.path(id)?.path.clone();
                if !p.spend(path.elements().len()) {
                    return None;
                }
                path.apply_affine(self.w(p));
                self.clip(p, Area::Path(path, true), u32::from(flags >> 8 & 0xF));
            }
            SET_CLIP_REGION => {
                let Some(Obj::Region(nodes)) = self.get(id).cloned() else { return None };
                let area = self.area(p, &nodes)?;
                self.clip(p, area, u32::from(flags >> 8 & 0xF));
            }
            OFFSET_CLIP => {
                if !p.spend(self.s.clip.cost()) {
                    return None;
                }
                let d = Vec2::new(c.f32()?, c.f32()?);
                self.offset_clip(d, self.w(p));
            }
            // The header (read above), the end, comments, multiple formats, text hints and contrast (text is drawn
            // anti-aliased whatever they ask), pixel offsets, compositing quality, image effects, and the terminal
            // server records: none draws.
            _ => {}
        }
        Some(())
    }

    /// A region as an area: a rect or a path as it is, everything or nothing, or the steps that draw it.
    fn area<S: Sink>(&self, p: &Player<'_, S>, nodes: &[Node]) -> Option<Area> {
        let w = self.w(p);
        Some(match nodes {
            [Node::Rect(r)] => Area::Path(rect_path(*r, w), false),
            [Node::Path(b)] => {
                let mut b = b.clone();
                b.apply_affine(w);
                Area::Path(b, true)
            }
            [Node::Infinite] => Area::Everything,
            [Node::Empty] => Area::Nothing,
            _ => Area::Steps(self.region_steps(p, nodes)?),
        })
    }

    /// Fills a region with `fill`: a single shape as itself, else everything through the region's mask.
    fn fill_region<S: Sink>(&mut self, p: &mut Player<'_, S>, nodes: &[Node], fill: &Fill) {
        match self.area(p, nodes) {
            Some(Area::Path(path, even_odd)) => self.paint(p, &path, even_odd, fill),
            Some(Area::Everything) => {
                let all = p.everything();
                self.paint(p, &all, false, fill);
            }
            Some(Area::Steps(steps)) => {
                let steps: Rc<[MaskStep]> = steps.into();
                let all = p.everything();
                if !p.spend(steps.iter().map(MaskStep::cost).sum()) {
                    return;
                }
                self.paint_masked(p, &all, fill, &steps);
            }
            Some(Area::Nothing) | None => {}
        }
    }

    /// Fills `path` (world units) with `fill`.
    fn fill_world<S: Sink>(&mut self, p: &mut Player<'_, S>, mut path: BezPath, even_odd: bool, fill: &Fill) {
        path.apply_affine(self.w(p));
        self.paint(p, &path, even_odd, fill);
    }
}

/// An area a clip combines with: a path's fill (even-odd or not), everything, nothing, or mask steps that draw it.
pub(super) enum Area {
    Path(BezPath, bool),
    Everything,
    Nothing,
    Steps(Vec<MaskStep>),
}

impl Area {
    fn cost(&self) -> usize {
        match self {
            Area::Path(p, _) => p.elements().len(),
            Area::Steps(s) => s.iter().map(MaskStep::cost).sum(),
            _ => 1,
        }
    }

    fn steps(self, everything: &BezPath) -> Vec<MaskStep> {
        match self {
            Area::Path(p, e) => vec![MaskStep::Add(p, e)],
            Area::Everything => vec![MaskStep::Add(everything.clone(), false)],
            Area::Nothing => Vec::new(),
            Area::Steps(s) => s,
        }
    }
}

/// A `CombineMode` (besides replace) as a mask group's.
fn combine_mode(mode: u32) -> Option<Combine> {
    Some(match mode {
        1 => Combine::Intersect,
        2 => Combine::Union,
        3 => Combine::Xor,
        4 => Combine::Exclude,
        5 => Combine::Complement,
        _ => return None,
    })
}

/// The steps drawing a clip's coverage from none.
fn program(c: &ClipState, everything: &BezPath) -> Vec<MaskStep> {
    match c {
        ClipState::Shapes(v) => super::program(v, everything),
        ClipState::Mask(m) => m.to_vec(),
    }
}

/// The clip `c` combined with `area` by `mode` (`CombineMode`): exactly, as shapes where it stays an intersection
/// of them, else as a mask.
fn combined(c: &ClipState, area: Area, mode: u32, everything: &BezPath) -> Option<ClipState> {
    use ClipState::{Mask, Shapes};
    let nothing = || Shapes(vec![(BezPath::new(), false)]);
    Some(match (mode, area) {
        (0, Area::Path(p, e)) => Shapes(vec![(p, e)]),
        (0, Area::Everything) | (2, Area::Everything) => Shapes(Vec::new()),
        (0 | 1, Area::Nothing) | (4, Area::Everything) | (5, Area::Nothing) => nothing(),
        (1, Area::Everything) | (2..=4, Area::Nothing) => c.clone(),
        (0, Area::Steps(s)) => Mask(s.into()),
        (1, Area::Path(p, e)) => super::combine(c, Some((p, e)), super::RGN_AND, everything.clone())?,
        (mode, area) => {
            let mut steps = program(c, everything);
            steps.push(MaskStep::Group(combine_mode(mode)?));
            steps.extend(area.steps(everything));
            steps.push(MaskStep::Pop);
            Mask(steps.into())
        }
    })
}

/// `c` within `base`; none past the bounds.
fn intersect(c: ClipState, base: &ClipState, everything: &BezPath) -> Option<ClipState> {
    use ClipState::{Mask, Shapes};
    Some(match (c, base) {
        (c, Shapes(b)) if b.is_empty() => c,
        (Shapes(mut v), Shapes(b)) => {
            v.extend(b.iter().cloned());
            Shapes(v)
        }
        (c, base) => {
            let mut steps = program(&c, everything);
            steps.push(MaskStep::Group(Combine::Intersect));
            steps.extend(program(base, everything));
            steps.push(MaskStep::Pop);
            Mask(steps.into())
        }
    })
}

fn fits(c: &ClipState) -> bool {
    match c {
        ClipState::Shapes(v) => v.len() <= MAX_CLIPS,
        ClipState::Mask(m) => m.len() <= MAX_MASK_STEPS,
    }
}

// ---------------- shapes ----------------

/// A rect (world units) as a path in the picture's space.
pub(super) fn rect_path(r: kurbo::Rect, w: Affine) -> BezPath {
    let mut p = r.abs().to_path(0.1);
    p.apply_affine(w);
    p
}

/// Points as one figure, closed or not.
fn polygon(pts: &[Point], closed: bool) -> BezPath {
    let mut p = BezPath::new();
    for (i, q) in pts.iter().enumerate() {
        if i == 0 { p.move_to(*q) } else { p.line_to(*q) }
    }
    if closed && !pts.is_empty() {
        p.close_path();
    }
    p
}

fn ellipse(r: kurbo::Rect) -> BezPath {
    let e = kurbo::Ellipse::from_rect(r.abs());
    e.to_path(super::tolerance(e.radii().x.min(e.radii().y)))
}

/// The arc of the ellipse in `r` from `start` degrees, `sweep` degrees on (clockwise as drawn, y down), its angles
/// those of rays from its centre; a pie closes it through the centre.
fn arc(r: kurbo::Rect, start: f64, sweep: f64, pie: bool) -> BezPath {
    let r = r.abs();
    let (c, a, b) = (r.center(), r.width() / 2.0, r.height() / 2.0);
    let mut p = BezPath::new();
    if !(a > 0.0 && b > 0.0) {
        return p;
    }
    let sweep = sweep.clamp(-360.0, 360.0);
    // A ray's angle as the ellipse's parameter, in the same turn.
    let param = |deg: f64| {
        let t = deg.to_radians();
        let s = (a * t.sin()).atan2(b * t.cos());
        s + TAU * ((t - s) / TAU).round()
    };
    let t0 = param(start);
    let t1 = if sweep.abs() >= 360.0 { t0 + TAU.copysign(sweep) } else { param(start + sweep) };
    let arc = kurbo::Arc::new(c, (a, b), t0, t1 - t0, 0.0);
    let first = c + Vec2::new(a * t0.cos(), b * t0.sin());
    if pie {
        p.move_to(c);
        p.line_to(first);
    } else {
        p.move_to(first);
    }
    p.extend(arc.append_iter(super::tolerance(a.min(b))));
    if pie {
        p.close_path();
    }
    p
}

/// A cardinal spline through `pts` with `tension` (GDI+'s: a third of it times the chord either side, as each
/// point's tangent): closed, or `segments` of it from point `offset` (its neighbours shaping its ends).
fn cardinal(pts: &[Point], tension: f64, closed: bool, (offset, segments): (usize, usize)) -> BezPath {
    let n = pts.len();
    let mut p = BezPath::new();
    if n < 2 {
        return p;
    }
    let k = tension / 3.0;
    let tangent = |i: usize| -> Vec2 {
        if closed {
            (pts[(i + 1) % n] - pts[(i + n - 1) % n]) * k
        } else if i == 0 {
            (pts[1] - pts[0]) * k
        } else if i == n - 1 {
            (pts[n - 1] - pts[n - 2]) * k
        } else {
            (pts[i + 1] - pts[i - 1]) * k
        }
    };
    let (first, count) = if closed { (0, n) } else { (offset.min(n - 1), segments.min(n - 1 - offset.min(n - 1))) };
    p.move_to(pts[first]);
    for i in first..first + count {
        let j = (i + 1) % n;
        p.curve_to(pts[i] + tangent(i), pts[j] - tangent(j), pts[j]);
    }
    if closed {
        p.close_path();
    }
    p
}
