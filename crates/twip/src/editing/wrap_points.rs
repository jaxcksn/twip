//! Wrap points

use std::rc::Rc;

use crate::font::Fonts;
use crate::layout::FloatId;

use crate::api::command::CommandId;

use super::commands::Outcome;
use super::position::Position;
use super::selection::{ObjectRef, Selection};
use super::view::WrapPoints;
use super::{Editor, Mode};

/// The float whose wrap points are being edited, and the point being dragged.
#[derive(Clone, Debug)]
pub(super) struct WrapEdit {
    f: FloatId,
    drag: Option<PointDrag>,
}

/// A vertex dragged (one added on an edge when `added`), where it is now, in its page's points.
#[derive(Clone, Copy, Debug)]
struct PointDrag {
    page: usize,
    index: usize,
    added: bool,
    now: [f64; 2],
    moved: bool,
}

/// What a press on the polygon hits.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum PolyHit {
    /// A vertex, by index.
    Vertex(usize),
    /// The edge ending at vertex `index`: a point added there.
    Edge(usize),
}

/// The polygon of an unedited tight or through float: its box.
const BOX: [(f64, f64); 4] = [(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)];

/// The fewest vertices a polygon keeps when one is removed.
const MIN_POINTS: usize = 3;

/// The farthest a polygon's point goes from its object's box, in box sizes: the coordinates stay in the range a
/// saved polygon has.
const REACH: f64 = 16.0;

/// Where a float's polygon lies on its page: the box centre, its unturned size, its turn and flips.
#[derive(Clone, Copy, Debug)]
struct Frame {
    c: [f64; 2],
    size: [f64; 2],
    sin: f64,
    cos: f64,
    flip: [bool; 2],
}

impl Frame {
    /// A box fraction's page point, as layout places a wrap polygon.
    fn to_page(self, (px, py): (f64, f64)) -> [f64; 2] {
        let fx = if self.flip[0] { 1.0 - px } else { px };
        let fy = if self.flip[1] { 1.0 - py } else { py };
        let (dx, dy) = ((fx - 0.5) * self.size[0], (fy - 0.5) * self.size[1]);
        [self.c[0] + dx * self.cos - dy * self.sin, self.c[1] + dx * self.sin + dy * self.cos]
    }

    /// A page point's box fraction.
    fn to_box(self, [x, y]: [f64; 2]) -> (f64, f64) {
        let (dx, dy) = (x - self.c[0], y - self.c[1]);
        let (ux, uy) = (dx * self.cos + dy * self.sin, -dx * self.sin + dy * self.cos);
        let f = |u: f64, s: f64, flip: bool| {
            let v = (u / s.max(1e-6) + 0.5).clamp(-REACH, 1.0 + REACH);
            if flip { 1.0 - v } else { v }
        };
        (f(ux, self.size[0], self.flip[0]), f(uy, self.size[1], self.flip[1]))
    }
}

/// A polygon's vertices, the closing repeat of the first left off.
fn open(poly: &[(f64, f64)]) -> &[(f64, f64)] {
    match poly {
        [first, rest @ .., last] if !rest.is_empty() && first == last => &poly[..poly.len() - 1],
        _ => poly,
    }
}

fn near_segment(p: [f64; 2], a: [f64; 2], b: [f64; 2], reach: f64) -> bool {
    let (vx, vy, wx, wy) = (b[0] - a[0], b[1] - a[1], p[0] - a[0], p[1] - a[1]);
    let len = vx * vx + vy * vy;
    let t = if len > 0.0 { ((wx * vx + wy * vy) / len).clamp(0.0, 1.0) } else { 0.0 };
    let (dx, dy) = (a[0] + t * vx - p[0], a[1] + t * vy - p[1]);
    dx * dx + dy * dy <= reach * reach
}

impl Editor {
    /// Starts editing the selected floating object's wrap points (Word's Wrap Text > Edit Wrap Points): its wrap
    /// polygon shows with a handle at each vertex. Dragging a vertex moves it, dragging an edge adds a vertex there,
    /// Ctrl+click removes one (down to three); each is one undo step ("Edit Wrap Points") that also makes a wrap that
    /// isn't tight or through tight. A press anywhere else, Escape or another selection ends it. Refused without one
    /// selected floating object, for a group's child, a fixed or protected object, or read-only.
    pub fn edit_wrap_points(&mut self) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        if self.selected_floats().len() != 1 || self.selected_child().is_some() {
            return Outcome::unavailable("select one floating object");
        }
        if self.object_fixed(ObjectRef::Floating(f)) || !self.float_editable(f) || self.gesture_kept(CommandId::SetObjectWrap) {
            return Outcome::unavailable("the object can't be changed");
        }
        if self.float_of(f).is_none_or(|fl| fl.in_note) {
            return Outcome::unavailable("no floating object selected");
        }
        self.wrap_edit = Some(WrapEdit { f, drag: None });
        Outcome::Moved
    }

    /// Whether the selected object's wrap points are being edited.
    pub fn editing_wrap_points(&self) -> bool {
        self.wrap_float().is_some()
    }

    /// Stops editing wrap points; false when they weren't being edited.
    pub fn end_wrap_points(&mut self) -> bool {
        let was = self.wrap_float().is_some();
        if self.wrap_edit.take().is_some_and(|w| w.drag.is_some()) {
            self.requests.push(super::input::Request::ReleasePointer);
        }
        was
    }

    /// The float being edited, while it is still the one selected object.
    fn wrap_float(&self) -> Option<FloatId> {
        let w = self.wrap_edit.as_ref()?;
        let one = self.selection == Selection::Object(ObjectRef::Floating(w.f)) && self.selected_child().is_none();
        (one && self.selected_floats().len() == 1 && self.float_of(w.f).is_some()).then_some(w.f)
    }

    fn float_editable(&self, f: FloatId) -> bool {
        let Some(anchor) = self.index.float_anchor(f) else { return false };
        let story = self.index.para_slot(anchor).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
        self.editable(&Position::text(story, anchor, crate::layout::Unit(0)))
    }

    /// Whether float `f` can be changed now: not read-only, in the stories being edited (the body's, or the headers'
    /// and footers'), its anchor editable (document protection, a locked control) and not in a locked picture control.
    pub(super) fn float_changeable(&self, f: FloatId) -> bool {
        let Some(at) = self.float_point(f) else { return false };
        self.mode != Mode::ReadOnly
            && super::stories::reachable(self.editing(), self.index.host(at.story))
            && self.float_editable(f)
            && !self.kept(crate::api::command::CommandId::ArrangeObject, &at, &at)
            && !self.object_fixed(ObjectRef::Floating(f))
    }

    /// Where `f`'s polygon lies on `page`.
    fn wrap_frame(&mut self, f: FloatId, page: usize) -> Option<Frame> {
        let fl = self.float_of(f)?;
        let xfrm = fl.shape.as_ref().map(|s| s.xfrm).unwrap_or_default();
        let swap = fl.rot_swap;
        let geo = self.pages.page(page, &self.index)?;
        let [x0, y0, x1, y1] = geo.stories.iter().find(|s| s.float == Some(f))?.rect;
        let (w, h) = (x1 - x0, y1 - y0);
        let (sin, cos) = xfrm.rot.radians().sin_cos();
        Some(Frame { c: [(x0 + x1) / 2.0, (y0 + y1) / 2.0], size: if swap { [h, w] } else { [w, h] }, sin, cos, flip: [xfrm.flip_h, xfrm.flip_v] })
    }

    /// `f`'s polygon as box fractions, its closing repeat left off: the model's, or the box.
    fn wrap_polygon(&self, f: FloatId) -> Vec<(f64, f64)> {
        match self.float_of(f).and_then(|fl| fl.poly.as_deref()) {
            Some(p) if open(p).len() >= MIN_POINTS => open(p).to_vec(),
            _ => BOX.to_vec(),
        }
    }

    /// The polygon on its page, the dragged point where it is now.
    fn wrap_points_on(&mut self, f: FloatId, page: usize) -> Option<Vec<[f64; 2]>> {
        let frame = self.wrap_frame(f, page)?;
        let mut points: Vec<[f64; 2]> = self.wrap_polygon(f).into_iter().map(|p| frame.to_page(p)).collect();
        if let Some(d) = self.wrap_edit.as_ref().and_then(|w| w.drag).filter(|d| d.page == page) {
            match d.added {
                true => points.insert(d.index.min(points.len()), d.now),
                false => {
                    if let Some(p) = points.get_mut(d.index) {
                        *p = d.now;
                    }
                }
            }
        }
        Some(points)
    }

    /// The wrap polygon being edited, on the first of `pages` it is laid out on, for drawing.
    pub fn wrap_points(&mut self, pages: std::ops::Range<usize>) -> Option<WrapPoints> {
        let f = self.wrap_float()?;
        pages.into_iter().find_map(|page| Some(WrapPoints { page, points: self.wrap_points_on(f, page)? }))
    }

    /// The vertex or edge of the polygon being edited under document point (`x`, `y`).
    pub(super) fn wrap_point_at(&mut self, x: f64, y: f64) -> Option<(FloatId, usize, PolyHit, [f64; 2])> {
        let f = self.wrap_float()?;
        let (page, px, py) = self.stack().page_at(x, y)?;
        let points = self.wrap_points_on(f, page)?;
        let reach = 4.0 * 0.75 / self.viewport.zoom.max(0.1);
        let p = [px, py];
        if let Some(i) = points.iter().position(|q| (q[0] - px).abs() <= reach && (q[1] - py).abs() <= reach) {
            return Some((f, page, PolyHit::Vertex(i), p));
        }
        let n = points.len();
        (0..n).find(|&i| near_segment(p, points[i], points[(i + 1) % n], reach)).map(|i| (f, page, PolyHit::Edge(i + 1), p))
    }

    /// A press while wrap points are edited: on a vertex or an edge it starts a drag (Ctrl on a vertex removes it);
    /// anywhere else it ends the editing and the press goes on as usual (false).
    pub(super) fn wrap_points_press(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) -> bool {
        if self.wrap_edit.is_none() {
            return false;
        }
        let Some((f, page, hit, at)) = self.wrap_point_at(x, y) else {
            self.wrap_edit = None;
            return false;
        };
        match hit {
            PolyHit::Vertex(i) if ctrl => {
                let mut poly = self.wrap_polygon(f);
                if poly.len() > MIN_POINTS {
                    poly.remove(i);
                    self.set_wrap_polygon(f, poly, fonts);
                }
            }
            PolyHit::Vertex(index) => self.start_point_drag(PointDrag { page, index, added: false, now: at, moved: false }),
            PolyHit::Edge(index) => self.start_point_drag(PointDrag { page, index, added: true, now: at, moved: false }),
        }
        true
    }

    fn start_point_drag(&mut self, d: PointDrag) {
        if let Some(w) = self.wrap_edit.as_mut() {
            w.drag = Some(d);
            self.requests.push(super::input::Request::CapturePointer);
        }
    }

    /// Whether a wrap point is being dragged.
    pub(super) fn wrap_point_dragging(&self) -> bool {
        self.wrap_edit.as_ref().is_some_and(|w| w.drag.is_some())
    }

    /// The dragged point follows the pointer; false when no point is dragged.
    pub(super) fn wrap_point_move(&mut self, x: f64, y: f64) -> bool {
        let Some(page) = self.wrap_edit.as_ref().and_then(|w| w.drag).map(|d| d.page) else { return false };
        let (ox, oy) = self.stack().to_doc(page, 0.0, 0.0).unwrap_or((0.0, 0.0));
        if let Some(d) = self.wrap_edit.as_mut().and_then(|w| w.drag.as_mut()) {
            let now = [x - ox, y - oy];
            d.moved |= d.now != now;
            d.now = now;
        }
        true
    }

    /// Ends a point drag: a moved point (or an added one) becomes the float's polygon, one undo step.
    pub(super) fn wrap_point_release(&mut self, fonts: &Fonts) -> bool {
        let Some((f, d)) = self.wrap_edit.as_mut().and_then(|w| Some((w.f, w.drag.take()?))) else { return false };
        self.requests.push(super::input::Request::ReleasePointer);
        // Another object selected meanwhile: the drag was of one no longer shown.
        if !d.moved || self.wrap_float() != Some(f) {
            return true;
        }
        let Some(frame) = self.wrap_frame(f, d.page) else { return true };
        let mut poly = self.wrap_polygon(f);
        let to = frame.to_box(d.now);
        match d.added {
            true => poly.insert(d.index.min(poly.len()), to),
            false => {
                if let Some(p) = poly.get_mut(d.index) {
                    *p = to;
                }
            }
        }
        self.set_wrap_polygon(f, poly, fonts);
        true
    }

    /// Drops a point drag without changing the polygon.
    pub(super) fn wrap_point_cancel(&mut self) -> bool {
        if self.wrap_edit.as_mut().and_then(|w| w.drag.take()).is_none() {
            return false;
        }
        self.requests.push(super::input::Request::ReleasePointer);
        true
    }

    /// Sets `f`'s wrap polygon (box fractions, closed on save), and a tight wrap unless it is tight or through.
    fn set_wrap_polygon(&mut self, f: FloatId, mut poly: Vec<(f64, f64)>, fonts: &Fonts) -> Outcome {
        if self.gesture_kept(CommandId::SetObjectWrap) {
            return Outcome::Unchanged("the wrap is protected");
        }
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        if let Some(&first) = poly.first() {
            poly.push(first);
        }
        n.poly = Some(Rc::from(poly));
        n.poly_edited = true;
        if !matches!(n.wrap.as_str(), "tight" | "through") {
            (n.wrap, n.behind) = ("tight".into(), false);
        }
        let out = self.set_float(n, "Edit Wrap Points", fonts);
        self.selection = Selection::Object(ObjectRef::Floating(f));
        out
    }
}

/// The most vertices a wrap polygon set or made takes.
pub use crate::docx::MAX_WRAP_POINTS;

/// How many bands a picture's outline is traced in, top to bottom.
const PICTURE_BANDS: usize = 32;

/// A float's wrap polygon.
#[derive(Clone, Debug, PartialEq)]
pub struct WrapPolygon {
    /// Its vertices as fractions of the object's box (0 to 1 across and down; outside it beyond), unclosed.
    pub points: Vec<(f64, f64)>,
    /// Edited by hand: kept as it is when the object changes.
    pub edited: bool,
}

/// A picture's opaque pixels as a polygon down its left side and up its right, band by band, in fractions of the
/// shown part (`src`, the crop's fractions from each edge); `None` for a picture that doesn't decode or is opaque to its
/// edges (its box is its outline).
fn picture_outline(data: &[u8], src: [f64; 4]) -> Option<Vec<(f64, f64)>> {
    let (w, h, px) = twip_graphics::image::pixels_straight(data).ok()?;
    let (w, h) = (w as usize, h as usize);
    let span = |lo: f64, hi: f64, n: usize| {
        let a = ((lo.clamp(0.0, 1.0) * n as f64) as usize).min(n);
        let b = (((1.0 - hi).clamp(0.0, 1.0) * n as f64).ceil() as usize).clamp(a, n);
        (a, b)
    };
    let ((x0, x1), (y0, y1)) = (span(src[0], src[2], w), span(src[1], src[3], h));
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let opaque = |x: usize, y: usize| px[(y * w + x) * 4 + 3] > 0;
    let bands = PICTURE_BANDS.min(y1 - y0);
    let fx = |x: usize| (x - x0) as f64 / (x1 - x0) as f64;
    let (mut left, mut right) = (Vec::new(), Vec::new());
    for k in 0..bands {
        let (ya, yb) = (y0 + k * (y1 - y0) / bands, y0 + (k + 1) * (y1 - y0) / bands);
        let (mut lo, mut hi) = (usize::MAX, 0);
        for y in ya..yb {
            if let Some(a) = (x0..x1).find(|&x| opaque(x, y)) {
                lo = lo.min(a);
                hi = hi.max((a..x1).rev().find(|&x| opaque(x, y)).unwrap_or(a) + 1);
            }
        }
        if lo == usize::MAX {
            continue;
        }
        let (ta, tb) = ((ya - y0) as f64 / (y1 - y0) as f64, (yb - y0) as f64 / (y1 - y0) as f64);
        left.extend([(fx(lo), ta), (fx(lo), tb)]);
        right.extend([(fx(hi), ta), (fx(hi), tb)]);
    }
    let full = left.iter().all(|p| p.0 == 0.0)
        && right.iter().all(|p| p.0 == 1.0)
        && left.first().is_some_and(|p| p.1 == 0.0)
        && left.last().is_some_and(|p| p.1 == 1.0);
    if left.is_empty() || full {
        return None;
    }
    right.reverse();
    left.extend(right);
    Some(simplified(left))
}

/// `pts` without the points on a straight line between their neighbours, at most [`MAX_WRAP_POINTS`].
fn simplified(pts: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(pts.len());
    for p in pts {
        if out.last() == Some(&p) {
            continue;
        }
        if let [.., a, b] = out[..]
            && ((b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0)).abs() < 1e-9
        {
            out.pop();
        }
        out.push(p);
    }
    if out.len() > MAX_WRAP_POINTS {
        let step = out.len().div_ceil(MAX_WRAP_POINTS);
        out = out.into_iter().step_by(step).collect();
    }
    out
}

/// A float's outline as its wrap polygon, as Word makes one for a tight or through wrap: a shape's geometry, a
/// picture's opaque pixels, else its box.
pub(super) fn outline_polygon<'a>(image: &dyn Fn(usize) -> Option<&'a [u8]>, f: &crate::docx::Float) -> Vec<(f64, f64)> {
    use crate::drawing::ShapeKind;
    let made = match f.shape.as_deref().map(|s| (&s.kind, s.xfrm.ext)) {
        Some((ShapeKind::Sp(sp), ext)) if ext[0].0 > 0 && ext[1].0 > 0 => {
            let (w, h) = (ext[0].0 as f64 / 12700.0, ext[1].0 as f64 / 12700.0);
            let pts: Vec<(f64, f64)> = sp.geom.evaluate(ext[0], ext[1]).outline().into_iter().map(|[x, y]| (x / w, y / h)).collect();
            (pts.len() >= MIN_POINTS).then(|| simplified(pts))
        }
        Some((ShapeKind::Pic(p), _)) => {
            p.blip.image.or(f.image).and_then(image).and_then(|data| picture_outline(data, p.blip.src.map(|v| v.fraction())))
        }
        _ => None,
    };
    made.filter(|p| p.len() >= MIN_POINTS).unwrap_or_else(|| BOX.to_vec())
}

impl Editor {
    /// The selected floating object's wrap polygon as set; a tight or through one without a polygon wraps round its
    /// box. `None` for another wrap without one.
    pub fn selected_wrap_polygon(&self) -> Option<WrapPolygon> {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return None };
        let fl = self.float_of(f)?;
        match fl.poly.as_deref().map(open).filter(|p| p.len() >= MIN_POINTS) {
            Some(p) => Some(WrapPolygon { points: p.to_vec(), edited: fl.poly_edited }),
            None if matches!(fl.wrap.as_str(), "tight" | "through") => Some(WrapPolygon { points: BOX.to_vec(), edited: false }),
            None => None,
        }
    }

    /// Sets the selected floating object's wrap polygon (box fractions, 3 to [`MAX_WRAP_POINTS`] vertices), edited by
    /// hand, and a tight wrap unless it is tight or through. One undo step ("Edit Wrap Points").
    pub fn set_wrap_points(&mut self, points: &[(f64, f64)], fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        if !(MIN_POINTS..=MAX_WRAP_POINTS).contains(&points.len()) {
            return Outcome::invalid("a wrap polygon has 3 to 1024 points");
        }
        if points.iter().any(|(x, y)| !x.is_finite() || !y.is_finite()) {
            return Outcome::invalid("a wrap point is a number");
        }
        let poly: Vec<(f64, f64)> = points.iter().map(|&(x, y)| (x.clamp(-REACH, 1.0 + REACH), y.clamp(-REACH, 1.0 + REACH))).collect();
        self.set_wrap_polygon(f, open(&poly).to_vec(), fonts)
    }

    /// Word's Reset of the wrap points: the selected floating object's polygon made again from its outline, unedited.
    /// One undo step ("Reset Wrap Points").
    pub fn reset_wrap_points(&mut self, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        let mut poly = outline_polygon(&|k| self.image_bytes(k), &n);
        if let Some(&first) = poly.first() {
            poly.push(first);
        }
        (n.poly, n.poly_edited) = (Some(Rc::from(poly)), false);
        self.set_float(n, "Reset Wrap Points", fonts)
    }
}

impl Editor {
    /// Image `k`'s bytes: the document's, or one added in the transaction being built.
    fn image_bytes(&self, k: usize) -> Option<&[u8]> {
        match k.checked_sub(self.doc.images.len()) {
            None => self.doc.images.get(k).map(Vec::as_slice),
            Some(j) => self.pending_images.get(j).map(Vec::as_slice),
        }
    }

    /// `n`'s wrap polygon made again from its outline when it wraps tight or through, wasn't edited by hand and its
    /// outline may have changed since `was` (its wrap, shape, size or picture), as Word does.
    pub(super) fn rewrap(&self, was: &crate::docx::Float, n: &mut crate::docx::Float) {
        if !matches!(n.wrap.as_str(), "tight" | "through") || n.poly_edited {
            return;
        }
        fn inputs(f: &crate::docx::Float) -> impl PartialEq + '_ {
            let s = f.shape.as_deref();
            // A picture's outline is in fractions of its box: its size changes nothing.
            let geom = s.and_then(|s| Some((&s.sp()?.geom, s.xfrm.ext)));
            (geom, s.and_then(|s| s.pic()).map(|p| (&p.blip.src, p.blip.image)), f.image)
        }
        let fresh = !matches!(was.wrap.as_str(), "tight" | "through") || n.poly.is_none();
        if !fresh && inputs(was) == inputs(n) {
            return;
        }
        let mut poly = outline_polygon(&|k| self.image_bytes(k), n);
        if let Some(&first) = poly.first() {
            poly.push(first);
        }
        n.poly = Some(Rc::from(poly));
    }
}

#[cfg(test)]
mod tests {
    /// A PNG of `w` × `h` RGBA pixels.
    fn png(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
        use std::io::Write;
        let chunk = |out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]| {
            out.extend((data.len() as u32).to_be_bytes());
            let mut crc = flate2::Crc::new();
            crc.update(kind);
            crc.update(data);
            out.extend(kind);
            out.extend(data);
            out.extend(crc.sum().to_be_bytes());
        };
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend(w.to_be_bytes());
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 6, 0, 0, 0]);
        chunk(&mut out, b"IHDR", &ihdr);
        let mut z = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        for row in rgba.chunks(w as usize * 4) {
            z.write_all(&[0]).expect("filter");
            z.write_all(row).expect("row");
        }
        chunk(&mut out, b"IDAT", &z.finish().expect("deflated"));
        chunk(&mut out, b"IEND", &[]);
        out
    }

    #[test]
    fn a_pictures_outline_follows_its_opaque_pixels() {
        // A 64 × 64 picture opaque in its middle 32 columns only.
        let (w, h) = (64u32, 64u32);
        let mut px = vec![0u8; (w * h * 4) as usize];
        for y in 0..h {
            for x in 16..48 {
                px[((y * w + x) * 4 + 3) as usize] = 255;
            }
        }
        let poly = super::picture_outline(&png(w, h, &px), [0.0; 4]).expect("an outline");
        assert!(poly.iter().all(|&(x, _)| x == 0.25 || x == 0.75), "{poly:?}");
        assert_eq!(poly.len(), 4, "a rectangle: {poly:?}");
        // Opaque to its edges: its box is its outline.
        assert_eq!(super::picture_outline(&png(2, 2, &[255; 16]), [0.0; 4]), None);
        // Cropped to its opaque middle: its box again.
        assert_eq!(super::picture_outline(&png(w, h, &px), [0.25, 0.0, 0.25, 0.0]), None);
    }
}
