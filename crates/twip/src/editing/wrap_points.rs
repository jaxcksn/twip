//! Wrap points

use std::rc::Rc;

use crate::font::Fonts;
use crate::layout::FloatId;

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
        if self.mode == Mode::ReadOnly || self.object_fixed(ObjectRef::Floating(f)) || !self.float_editable(f) {
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
        if !d.moved {
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
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        if let Some(&first) = poly.first() {
            poly.push(first);
        }
        n.poly = Some(Rc::from(poly));
        if !matches!(n.wrap.as_str(), "tight" | "through") {
            (n.wrap, n.behind) = ("tight".into(), false);
        }
        let out = self.set_float(n, "Edit Wrap Points", fonts);
        self.selection = Selection::Object(ObjectRef::Floating(f));
        out
    }
}
