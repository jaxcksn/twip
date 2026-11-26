//! Object selection

use crate::api::value::{HorizontalReference, ObjectHorizontalAlignment, ObjectVerticalAlignment, ObjectWrap, VerticalReference, WrapSide};
use crate::docx::{FloatWrap, HorizontalAlign, HorizontalRelation, Inline, VerticalAlign, VerticalRelation, WrapText};
use crate::font::Fonts;
use crate::layout::{FloatId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::position::{Position, StoryId};
use super::selection::{Granularity, ObjectRef, Selection};
use super::snap::{SnapLine, Snapping};
use super::step::Step;
use super::stories::{inline_at, reachable};
use super::view::{Axis, Grid, GuideLine, Guides, Measure, Outline, Pt, Px, Tone};
use crate::docx::run_xml;
use crate::layout::vertical::Turn;

impl Editor {
    pub(super) fn object_hit(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Selection> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        let one = 0.75 / self.viewport.zoom.max(0.1);
        let band = 4.0 * one;
        for (si, s) in geo.stories.iter().enumerate().rev() {
            let Some(f) = s.float else { continue };
            let [x0, y0, x1, y1] = s.rect;
            let [lx, ly] = s.turn.back([px, py]);
            let in_box = lx >= x0 - one && lx <= x1 + one && ly >= y0 - one && ly <= y1 + one;
            let text_box = self.index.text_box(&self.doc, self.index.box_story(f)).is_some();
            // As in Word, a shape holding text (not a text box) is selected by a click anywhere on it, its text area
            // or not; a double-click, a click once it's selected or a click in the shape being edited goes to the text.
            if text_box && reachable(ctx, s.host) && !self.is_text_box_shape(f) {
                let object = Selection::Object(self.story_object(f));
                let editing = self.selection.focus().is_some_and(|p| p.story == s.story);
                let frame = match self.story_object(f) {
                    ObjectRef::Floating(g) => self.float_frame(g, page).map(|(r, _)| r),
                    ObjectRef::Inline { para, offset } => self.inline_box(&geo, para, offset),
                };
                // ponytail: the frame, not the outline: an ellipse's corners select it too; hit its geometry if that matters.
                let on = frame.is_some_and(|[a, b, c, d]| px >= a - one && px <= c + one && py >= b - one && py <= d + one);
                if on && !editing && self.press_clicks < 2 && self.selection != object {
                    return Some(object);
                }
            }
            if !reachable(ctx, s.host) || text_box && !in_box || !text_box && !self.shape_hit(page, f, px, py, 3.0 * one, in_box) {
                continue;
            }
            let text_over = || {
                geo.lines.iter().any(|l| {
                    let [x, y] = l.local([px, py]);
                    l.story != si
                        && y >= l.top
                        && y < l.bottom
                        && l.stops.first().zip(l.stops.last()).is_some_and(|(a, b)| x >= a.x && x <= b.x)
                        && reachable(ctx, geo.stories[l.story].host)
                })
            };
            if self.float_behind(f) && text_over() {
                continue;
            }
            if !text_box || self.float_of(f).is_some_and(|fl| fl.picture_control(&self.doc.controls).is_some()) {
                return Some(Selection::Object(ObjectRef::Floating(f)));
            }
            let left = lx - x0 < band;
            if !(left || x1 - lx < band || ly - y0 < band || y1 - ly < band) {
                return None;
            }
            if left
                && let Some(i) = geo.lines.iter().position(|l| l.story == si && l.holds(px, py))
                && let Some((a, b)) = self.line_span_of(&geo, i)
            {
                return Some(Selection::range(a, b, Granularity::Line));
            }
            return Some(Selection::Object(self.story_object(f)));
        }
        for l in geo.lines.iter().filter(|l| l.holds(px, py) && reachable(ctx, geo.stories[l.story].host)) {
            let x = l.local([px, py])[0];
            for w in l.stops.windows(2) {
                if w[1].para == w[0].para && w[1].unit.0 == w[0].unit.0 + 1 && x >= w[0].x && x < w[1].x {
                    let p = self.index.para(&self.doc, w[0].para)?;
                    if matches!(inline_at(p, w[0].unit), Some(Inline::Picture(..))) {
                        return Some(Selection::Object(ObjectRef::Inline { para: w[0].para, offset: w[0].unit }));
                    }
                }
            }
        }
        None
    }

    /// Whether float or inline box `f` is a text box (`txBox`) rather than a shape holding text.
    fn is_text_box_shape(&self, f: FloatId) -> bool {
        let shape = match self.story_object(f) {
            ObjectRef::Floating(f) => self.float_of(f).and_then(|fl| fl.shape.as_deref()),
            ObjectRef::Inline { para, offset } => match self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)) {
                Some(Inline::Picture(pic, _)) => pic.shape.as_deref(),
                _ => None,
            },
        };
        !matches!(shape.map(|s| &s.kind), Some(crate::drawing::ShapeKind::Sp(sp)) if !sp.text_box)
    }

    pub(super) fn story_object(&self, f: FloatId) -> ObjectRef {
        let found = self.index.float_anchor(f).and_then(|para| {
            let p = self.index.para(&self.doc, para)?;
            let mut at = 0u32;
            for i in &p.inlines {
                if let Inline::Picture(pic, _) = i
                    && pic.text.as_ref().is_some_and(|t| t.id == f.0)
                {
                    return Some(ObjectRef::Inline { para, offset: Unit(at) });
                }
                at += i.units() as u32;
            }
            None
        });
        found.unwrap_or(ObjectRef::Floating(f))
    }

    fn child_at(&self, page: usize, f: FloatId, x: f64, y: f64, slop: f64) -> Option<crate::drawing::ChildPath> {
        let snap = self.snapshot();
        let items = crate::layout::traverse::paint_float(snap.pages().get(page)?, f);
        items.iter().rev().find_map(|it| match it {
            crate::layout::fragment::Item::Shape(s) if !s.decoration && s.hit(x, y, slop) => Some(s.path.clone()),
            _ => None,
        })
    }

    pub(super) fn selected_child(&self) -> Option<&[u16]> {
        match (&self.object_child, self.selection) {
            (Some((f, c)), Selection::Object(ObjectRef::Floating(g))) if *f == g => Some(c),
            _ => None,
        }
    }

    pub fn selected_floats(&self) -> Vec<FloatId> {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Vec::new() };
        let mut v = vec![f];
        if let Some((p, also)) = &self.object_also
            && *p == f
        {
            v.extend(also.iter().copied().filter(|g| *g != f));
        }
        v
    }

    pub(super) fn object_toggle(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Selection::Object(ObjectRef::Floating(p)) = self.selection else { return false };
        let Some(Selection::Object(ObjectRef::Floating(f))) = self.object_hit(x, y, fonts) else { return false };
        let story = |e: &Self, f: FloatId| e.index.float_anchor(f).and_then(|a| e.index.para_slot(a)).map(|s| e.index.story(s.container));
        if f == p || story(self, f) != story(self, p) {
            return false;
        }
        let mut also = self.selected_floats().split_off(1);
        match also.iter().position(|g| *g == f) {
            Some(i) => {
                also.remove(i);
            }
            None => also.push(f),
        }
        self.object_also = Some((p, also));
        self.object_child = None;
        true
    }

    fn shape_hit(&self, page: usize, f: FloatId, x: f64, y: f64, slop: f64, in_box: bool) -> bool {
        let snap = self.snapshot();
        let Some(frag) = snap.pages().get(page) else { return in_box };
        let items = crate::layout::traverse::paint_float(frag, f);
        let mut shapes = items
            .iter()
            .filter_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s).filter(|s| !s.decoration) } else { None })
            .peekable();
        if shapes.peek().is_none() { in_box } else { shapes.any(|s| s.hit(x, y, slop)) }
    }

    pub(super) fn object_to_caret(&mut self) {
        let Selection::Object(o) = self.selection else { return };
        let (para, offset) = match o {
            ObjectRef::Inline { para, offset } => (para, offset),
            ObjectRef::Floating(f) => match self.index.float_anchor(f) {
                Some(a) => (a, Unit(0)),
                None => return,
            },
        };
        let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
        self.set_selection(Selection::Caret(Position::text(story, para, offset)));
    }

    pub(super) fn delete_object(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let Selection::Object(o) = self.selection else { return None };
        // A protected picture goes with its whole control in design mode (a floating one with its box), and stays
        // otherwise.
        if let Some(c) = self.picture_control_at(o).filter(|c| self.control_protected(*c)) {
            if self.mode != super::Mode::Design {
                return Some(Outcome::Unchanged("protected picture"));
            }
            if let ObjectRef::Floating(f) = o {
                return Some(self.remove_floats(&[f], fonts));
            }
            return Some(self.remove_element(&super::merge::api_id(crate::docx::ElementId::Control(c)), false, fonts));
        }
        match o {
            ObjectRef::Inline { para, offset } => {
                let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
                let a = Position::text(story, para, offset);
                self.selection = Selection::range(a, Position::text(story, para, Unit(offset.0 + 1)), Granularity::Char);
                None
            }
            ObjectRef::Floating(f) => match self.selected_child().map(<[u16]>::to_vec) {
                Some(path) => Some(self.remove_child(f, &path, fonts)),
                None => Some(self.remove_floats(&self.selected_floats(), fonts)),
            },
        }
    }

    /// Deletes floats `ids` in one transaction.
    fn remove_floats(&mut self, ids: &[FloatId], fonts: &Fonts) -> Outcome {
        let mut anchors: Vec<crate::layout::ParaId> = Vec::with_capacity(ids.len());
        for a in ids.iter().filter_map(|f| self.index.float_anchor(*f)) {
            if !anchors.contains(&a) {
                anchors.push(a);
            }
        }
        let Some(&first) = anchors.first() else { return Outcome::Unchanged("no anchor") };
        let story = self.index.para_slot(first).map_or(StoryId::Body, |s| self.index.story(s.container));
        let mut tx = self.begin("Delete");
        for anchor in anchors {
            let Some(p) = self.index.para(&self.doc, anchor) else { continue };
            let inlines: Vec<Inline> =
                p.inlines.iter().filter(|i| !matches!(i, Inline::Float(fl, _) if ids.contains(&FloatId(fl.id)))).cloned().collect();
            if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale object");
            }
        }
        self.selection = Selection::Caret(Position::text(story, first, Unit(0)));
        self.commit(tx, None, fonts)
    }

    /// Deletes the shape at `path` in group float `f`; a group it leaves empty goes with it.
    fn remove_child(&mut self, f: FloatId, path: &[u16], fonts: &Fonts) -> Outcome {
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        let only =
            |n: &crate::docx::Float, parent: &[u16]| n.shape.as_deref().and_then(|s| s.at(parent)?.group()).is_some_and(|g| g.children.len() <= 1);
        let Some((&leaf, leaf_parent)) = path.split_last() else { return self.remove_floats(&[f], fonts) };
        let leaves = n.shape.as_deref().and_then(|s| s.at(leaf_parent)?.group()).map_or(0, |g| g.children.len());
        if usize::from(leaf) >= leaves {
            return Outcome::Unchanged("no such shape");
        }
        let mut path = path;
        while let Some((_, parent)) = path.split_last()
            && only(&n, parent)
        {
            path = parent;
        }
        let Some((&i, parent)) = path.split_last() else { return self.remove_floats(&[f], fonts) };
        let Some(crate::drawing::ShapeKind::Group(g)) = n.shape.as_deref_mut().and_then(|s| s.at_mut(parent)).map(|s| &mut s.kind) else {
            return Outcome::Unchanged("not in a group");
        };
        if usize::from(i) >= g.children.len() {
            return Outcome::Unchanged("no such shape");
        }
        g.children.remove(usize::from(i));
        n.parts.retain(|p| !p.path.starts_with(path));
        for p in n.parts.iter_mut().filter(|p| p.path.len() > parent.len() && p.path.starts_with(parent) && p.path[parent.len()] > i) {
            p.path[parent.len()] -= 1;
        }
        super::shapes::refit(&mut n, path, true);
        self.object_child = None;
        self.set_float(n, "Delete", fonts)
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ObjectDrag {
    pub obj: ObjectRef,
    pub handle: Option<usize>,
    pub start: (f64, f64),
    pub now: (f64, f64),
    pub rect: [f64; 4],
    pub page: usize,
    pub ctrl: bool,
    pub shift: bool,
    pub live: bool,
    pub previewed: Option<crate::layout::ParaId>,
    pub snapping: Snapping,
    pub snap: [f64; 2],
    pub snapped: [Option<SnapLine>; 2],
    /// A turned float's rotation; `rect` is then its frame, unturned, which a quarter-turned float lays out swapped.
    pub rot: f64,
    pub swap: bool,
    /// The page under the pointer while moving, and the offset from `page`'s coordinates to its own.
    pub over: usize,
    pub offset: [f64; 2],
    /// Where the float's region is drawn as the drag began, before the live preview moves it.
    pub region: Turn,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectCmd {
    Wrap(ObjectWrap),
    LockAnchor(bool),
    /// A width and height, and the axes the command set: a set axis loses its relative size.
    Size(f64, f64, [bool; 2]),
    ResetSize,
    AltText(String, Option<bool>),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ToggledCheckbox {
    pub face: Position,
    pub after: Position,
}

/// A float's wrapping and whether it is behind the text, for `w` (inline handled before).
fn float_wrap(w: ObjectWrap) -> (FloatWrap, bool) {
    match w {
        ObjectWrap::Square => (FloatWrap::Square, false),
        ObjectWrap::Tight => (FloatWrap::Tight, false),
        ObjectWrap::Through => (FloatWrap::Through, false),
        ObjectWrap::TopAndBottom => (FloatWrap::TopAndBottom, false),
        ObjectWrap::BehindText => (FloatWrap::None, true),
        ObjectWrap::Inline | ObjectWrap::InFrontOfText => (FloatWrap::None, false),
    }
}

/// How far below its paragraph's top a picture floated from the paragraph's first line goes, pt (Word's 3175 EMU).
const MIN_BELOW_PARA: f64 = 0.25;

const HANDLES: [(f64, f64); 8] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];

impl ObjectDrag {
    fn new(obj: ObjectRef, handle: Option<usize>, at: (f64, f64), rect: [f64; 4], page: usize, ctrl: bool) -> ObjectDrag {
        ObjectDrag {
            obj,
            handle,
            start: at,
            now: at,
            rect,
            page,
            ctrl,
            shift: false,
            live: false,
            previewed: None,
            snapping: Snapping::On,
            snap: [0.0; 2],
            snapped: [None; 2],
            rot: 0.0,
            swap: false,
            over: page,
            offset: [0.0; 2],
            region: Turn::NONE,
        }
    }

    /// `r`, in `page`'s coordinates, in `over`'s.
    pub fn on_over(&self, r: [f64; 4]) -> [f64; 4] {
        let [sx, sy] = self.offset;
        [r[0] + sx, r[1] + sy, r[2] + sx, r[3] + sy]
    }

    /// `r`, a frame, as the box the float lays out in.
    pub fn placed(&self, r: [f64; 4]) -> [f64; 4] {
        if !self.swap {
            return r;
        }
        let [x0, y0, x1, y1] = r;
        let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
        [cx - hw, cy - hh, cx + hw, cy + hh]
    }

    pub fn rect(&self, locked: bool) -> [f64; 4] {
        let (mut dx, mut dy) = (self.now.0 - self.start.0, self.now.1 - self.start.1);
        let [x0, y0, x1, y1] = self.rect;
        let Some(k) = self.handle else {
            if self.shift {
                if dx.abs() >= dy.abs() { dy = 0.0 } else { dx = 0.0 }
            }
            let [sx, sy] = self.snap;
            return [x0 + dx + sx, y0 + dy + sy, x1 + dx + sx, y1 + dy + sy];
        };
        let locked = locked || self.shift;
        // A turned frame resizes along its own sides.
        (dx, dy) = turned((dx, dy), -self.rot);
        let (sx, sy) = HANDLES[k];
        let both = if self.ctrl { 2.0 } else { 1.0 };
        let (w0, h0) = (x1 - x0, y1 - y0);
        let (mut w, mut h) = ((w0 + sx * dx * both).max(1.0), (h0 + sy * dy * both).max(1.0));
        if locked && sx != 0.0 && sy != 0.0 && w0 > 0.0 && h0 > 0.0 {
            let (rw, rh) = (w / w0, h / h0);
            let r = if (rw - 1.0).abs() >= (rh - 1.0).abs() { rw } else { rh };
            (w, h) = ((w0 * r).max(1.0), (h0 * r).max(1.0));
        }
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let nx0 = if self.ctrl {
            cx - w / 2.0
        } else if sx < 0.0 {
            x1 - w
        } else {
            x0
        };
        let ny0 = if self.ctrl {
            cy - h / 2.0
        } else if sy < 0.0 {
            y1 - h
        } else {
            y0
        };
        if self.ctrl || self.rot == 0.0 {
            return [nx0, ny0, nx0 + w, ny0 + h];
        }
        // The opposite handle stays where it's drawn: the frame turns about its own centre, which moved.
        let fixed = |x0: f64, y0: f64, w: f64, h: f64| {
            let (fx, fy) = turned((-sx * w / 2.0, -sy * h / 2.0), self.rot);
            (x0 + w / 2.0 + fx, y0 + h / 2.0 + fy)
        };
        let ((ax, ay), (bx, by)) = (fixed(x0, y0, w0, h0), fixed(nx0, ny0, w, h));
        let (ox, oy) = (ax - bx, ay - by);
        [nx0 + ox, ny0 + oy, nx0 + w + ox, ny0 + h + oy]
    }
}

/// `(x, y)` turned `deg` clockwise about the origin.
pub(super) fn turned((x, y): (f64, f64), deg: f64) -> (f64, f64) {
    let (s, c) = deg.to_radians().sin_cos();
    (x * c - y * s, x * s + y * c)
}

impl Editor {
    pub(super) fn handle_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(ObjectRef, [f64; 4], f64, usize, usize)> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let one = 0.75 / self.viewport.zoom.max(0.1);
        let (obj, rect, rot) = match self.selection {
            Selection::Object(o @ ObjectRef::Floating(f)) => self.float_frame(f, page).map(|(r, rot)| (o, r, rot)),
            Selection::Object(o @ ObjectRef::Inline { para, offset }) => {
                self.inline_frame(&geo, para, offset).map(|(r, rot)| (o, r, rot + self.inline_turn(para, offset)))
            }
            _ => None,
        }
        .filter(|(o, ..)| !self.object_fixed(*o) && !self.gesture_kept(crate::api::command::CommandId::SetObjectSize))?;
        let [x0, y0, x1, y1] = rect;
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let (ux, uy) = turned((px - cx, py - cy), -rot);
        let (px, py) = (cx + ux, cy + uy);
        let at = |side: f64, lo: f64, hi: f64| match side {
            s if s < 0.0 => lo,
            s if s > 0.0 => hi,
            _ => (lo + hi) / 2.0,
        };
        let k = (0..8).find(|&k| {
            let (sx, sy) = HANDLES[k];
            (px - at(sx, x0, x1)).abs() <= 4.0 * one && (py - at(sy, y0, y1)).abs() <= 4.0 * one
        })?;
        Some((obj, rect, rot, page, k))
    }

    /// A float's frame on `page`, unturned, and its rotation.
    pub(super) fn float_frame(&mut self, f: FloatId, page: usize) -> Option<([f64; 4], f64)> {
        let rot = self.float_of(f)?.shape.as_ref().map_or(0.0, |s| s.xfrm.rot.degrees());
        Some((self.shape_root(f, page)?, rot))
    }

    pub(super) fn object_press(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) -> bool {
        if let Some((obj, rect, rot, page, k)) = self.handle_at(x, y, fonts) {
            let swap = matches!(obj, ObjectRef::Floating(f) if self.float_of(f).is_some_and(|fl| fl.rot_swap));
            let region = match obj {
                ObjectRef::Floating(f) => self.float_region(f, page),
                ObjectRef::Inline { .. } => Turn::NONE,
            };
            self.object_drag = Some(ObjectDrag { rot, swap, region, ..ObjectDrag::new(obj, Some(k), (x, y), rect, page, ctrl) });
            self.requests.push(Request::CapturePointer);
            return true;
        }
        let stack = self.stack();
        let Some((page, ..)) = stack.page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let Some(s) = self.object_hit(x, y, fonts) else { return false };
        let was = self.selection;
        self.set_selection(s);
        let (_, px, py) = stack.page_at(x, y).unwrap_or((page, x, y));
        self.object_also = None;
        self.object_child = match s {
            Selection::Object(ObjectRef::Floating(f)) if was == s => {
                let one = 0.75 / self.viewport.zoom.max(0.1);
                self.child_at(page, f, px, py, 3.0 * one).filter(|c| !c.is_empty()).map(|c| (f, c))
            }
            _ => None,
        };
        self.drag = None;
        if self.press_clicks <= 1 && !self.activation_suppressed && self.mode != super::Mode::Design && self.selected_picture_unfilled() {
            self.activate_picture_control();
            self.click_activated = self.selected_picture_control();
        }
        let kept = self.gesture_kept(crate::api::command::CommandId::SetObjectPosition);
        if let Selection::Object(obj @ ObjectRef::Floating(f)) = s
            && self.selected_child().is_some()
            && !self.object_fixed(obj)
            && !kept
        {
            self.child_move_press(f, page, px, py);
            return true;
        }
        if let Selection::Object(obj @ ObjectRef::Floating(f)) = s
            && !self.object_fixed(obj)
            && !kept
            && let Some((rect, rot)) = self.float_frame(f, page)
        {
            let swap = self.float_of(f).is_some_and(|fl| fl.rot_swap);
            let region = self.float_region(f, page);
            self.object_drag = Some(ObjectDrag { rot, swap, region, ..ObjectDrag::new(obj, None, (x, y), rect, page, ctrl) });
            self.build_snaps(page, Dragged::Float(f));
            self.requests.push(Request::CapturePointer);
        }
        if let Selection::Object(obj @ ObjectRef::Inline { para, offset }) = s
            && !self.object_fixed(obj)
            && self.mode != super::Mode::ReadOnly
        {
            let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
            let press = Position::text(story, para, offset);
            self.text_drag = Some(super::interaction::TextDrag { start: (x, y), press, moved: false, copy: ctrl, drop: None, menu: false });
            self.requests.push(Request::CapturePointer);
        }
        true
    }

    pub(super) fn object_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.object_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        let label = if d.handle.is_some() { "Resize Object" } else { "Move Object" };
        // The page it's dropped on, laid out after the live preview, holds its new anchor.
        if d.over != d.page {
            self.ensure_page(d.over, fonts);
        }
        let out = match d.obj {
            _ if d.now == d.start => Outcome::Unchanged("no drag"),
            ObjectRef::Floating(f) => match self.dropped_float(f, &d) {
                Some((moved, to)) => self.commit_float(f, moved, to, label, fonts),
                None => Outcome::Unchanged("no float"),
            },
            ObjectRef::Inline { para, offset } => {
                let [x0, y0, x1, y1] = d.rect(self.object_locked(d.obj));
                self.set_picture_size(para, offset, x1 - x0, y1 - y0, fonts)
            }
        };
        self.end_preview(d);
        Some(out)
    }

    pub(super) fn object_move(&mut self, x: f64, y: f64, mods: super::keymap::Mods, fonts: &Fonts) {
        let Some(mut d) = self.object_drag else { return };
        (d.now, d.shift) = ((x, y), mods.shift);
        d.snapping = if mods.alt { Snapping::Free } else { Snapping::On };
        if d.handle.is_none()
            && let ObjectRef::Floating(f) = d.obj
            && let over = self.page_under(&d)
            && over != d.over
        {
            let stack = self.stack();
            let (Some(a), Some(b)) = (stack.pages.get(d.page), stack.pages.get(over)) else { return };
            (d.over, d.offset) = (over, [a[0] - b[0], a[1] - b[1]]);
            self.build_snaps(over, Dragged::Float(f));
        }
        if d.handle.is_none() {
            (d.snap, d.snapped) = ([0.0; 2], [None; 2]);
            if d.snapping == Snapping::On {
                let (dx, dy) = (d.now.0 - d.start.0, d.now.1 - d.start.1);
                let free = if !d.shift {
                    [true, true]
                } else if dx.abs() >= dy.abs() {
                    [true, false]
                } else {
                    [false, true]
                };
                let (off, hit) = super::snap::snap(super::snap::turned_bounds(d.on_over(d.rect(false)), d.rot), &self.snaps, self.snap_threshold());
                for k in 0..2 {
                    if free[k] {
                        (d.snap[k], d.snapped[k]) = (off[k], hit[k]);
                    }
                }
            }
        }
        let ObjectRef::Floating(f) = d.obj else {
            self.object_drag = Some(d);
            return;
        };
        d.live |= self.live_layout && d.now != d.start;
        if d.live && d.over != d.page {
            self.ensure_page(d.over, fonts);
        }
        if d.live {
            let preview = self
                .dropped_float(f, &d)
                .and_then(|(float, dest)| Some(crate::layout::Preview { float, anchor: dest.or_else(|| self.index.float_anchor(f))? }));
            let at = preview.as_ref().map(|p| p.anchor);
            self.preview_layout(f, preview, d.previewed);
            d.previewed = at;
        }
        self.object_drag = Some(d);
    }

    /// The page a moving float goes to: the one its centre is over, unsnapped. It stays on the page it's on until
    /// its centre is past the gap beyond it, so it doesn't flit between pages along the gap.
    fn page_under(&self, d: &ObjectDrag) -> usize {
        let stack = self.stack();
        let (Some(home), Some(cur)) = (stack.pages.get(d.page), stack.pages.get(d.over)) else { return d.over };
        let [x0, y0, x1, y1] = ObjectDrag { snap: [0.0; 2], ..*d }.rect(false);
        let (cx, cy) = (home[0] + (x0 + x1) / 2.0, home[1] + (y0 + y1) / 2.0);
        let gap = super::view::PAGE_GAP;
        if cy >= cur[1] - gap && cy <= cur[1] + cur[3] + gap {
            return d.over;
        }
        stack.page_at(cx, cy).map_or(d.over, |(p, ..)| p)
    }

    pub(super) fn object_cancel(&mut self) -> bool {
        let Some(d) = self.object_drag.take() else { return false };
        self.end_preview(d);
        self.requests.push(Request::ReleasePointer);
        true
    }

    fn end_preview(&mut self, d: ObjectDrag) {
        if let (true, ObjectRef::Floating(f)) = (d.live, d.obj) {
            self.preview_layout(f, None, d.previewed);
        }
    }

    /// The object can't be moved or resized: a picture control's picture outside design mode, or a locked one's.
    pub(super) fn object_fixed(&self, o: ObjectRef) -> bool {
        self.picture_control_at(o)
            .and_then(|c| self.doc.controls.get(&c))
            .is_some_and(|k| self.mode != super::Mode::Design || k.props.lock != crate::docx::Lock::None)
    }

    pub(super) fn object_locked(&self, o: ObjectRef) -> bool {
        match o {
            ObjectRef::Floating(f) => self.float_locked(f),
            ObjectRef::Inline { para, offset } => matches!(
                self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)),
                Some(Inline::Picture(p, _)) if p.lock_aspect || p.image.is_some_and(|i| self.doc.controlled.contains_key(&i))
            ),
        }
    }

    pub(super) fn float_locked(&self, f: FloatId) -> bool {
        self.float_of(f).is_some_and(|fl| {
            fl.lock_aspect || fl.picture_control(&self.doc.controls).is_some() || fl.image.is_some_and(|k| self.doc.controlled.contains_key(&k))
        })
    }

    fn float_behind(&self, f: FloatId) -> bool {
        self.index
            .float_anchor(f)
            .and_then(|a| self.index.para(&self.doc, a))
            .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f && fl.behind)))
    }

    /// An inline picture's box on the page.
    pub(super) fn inline_box(&self, geo: &super::geometry::PageGeometry, para: crate::layout::ParaId, offset: Unit) -> Option<[f64; 4]> {
        geo.lines.iter().find_map(|l| l.pictures.iter().find(|(p, u, _)| *p == para && *u == offset).map(|(_, _, r)| l.turn.rect(*r)))
    }

    /// An inline picture's frame on the page, unturned, and its rotation: a vertical line draws it turned with it.
    pub(super) fn inline_frame(&self, geo: &super::geometry::PageGeometry, para: crate::layout::ParaId, offset: Unit) -> Option<([f64; 4], f64)> {
        geo.lines.iter().find_map(|l| {
            let &(_, _, [x0, y0, x1, y1]) = l.pictures.iter().find(|(p, u, _)| *p == para && *u == offset)?;
            let rot = l.turn.m[1].atan2(l.turn.m[0]).to_degrees();
            let [cx, cy] = l.turn.at([(x0 + x1) / 2.0, (y0 + y1) / 2.0]);
            let (w, h) = (x1 - x0, y1 - y0);
            Some(([cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0], rot))
        })
    }

    fn set_picture_size(&mut self, para: crate::layout::ParaId, offset: Unit, w: f64, h: f64, fonts: &Fonts) -> Outcome {
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let mut inlines = p.inlines.clone();
        let mut at = 0;
        let Some(pic) = inlines.iter_mut().find_map(|i| {
            let here = at;
            at += i.units();
            match i {
                Inline::Picture(pic, _) if here == offset.0 as usize => Some(pic),
                _ => None,
            }
        }) else {
            return Outcome::Unchanged("no picture");
        };
        let (w, h) = (((w.max(1.0)) * 20.0).round() / 20.0, ((h.max(1.0)) * 20.0).round() / 20.0);
        if (pic.w - w).abs() < 1e-9 && (pic.h - h).abs() < 1e-9 {
            return Outcome::Unchanged("same size");
        }
        let was = (pic.w, pic.h, pic.shape.as_deref().map_or(0.0, |s| s.xfrm.rot.degrees()));
        pic.w = w;
        pic.h = h;
        super::shapes::refit_turn(pic, was);
        let mut tx = self.begin("Resize Object");
        if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(ObjectRef::Inline { para, offset });
        self.commit(tx, None, fonts)
    }

    fn set_alt_text(&mut self, o: ObjectRef, text: &str, decorative: Option<bool>, fonts: &Fonts) -> Outcome {
        let text = if decorative == Some(true) { "" } else { text };
        let mark = |shape: &mut Option<Box<crate::drawing::Shape>>| match (decorative, shape) {
            (Some(on), Some(s)) if s.nv.decorative != on => {
                s.nv.decorative = on;
                true
            }
            _ => false,
        };
        let (para, unit) = match o {
            ObjectRef::Inline { para, offset } => (para, Some(offset)),
            ObjectRef::Floating(f) => match self.index.float_anchor(f) {
                Some(a) => (a, None),
                None => return Outcome::Unchanged("no anchor"),
            },
        };
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let mut inlines = p.inlines.clone();
        let mut at = 0u32;
        let mut done = false;
        for i in inlines.iter_mut() {
            let here = at;
            at += i.units() as u32;
            match i {
                Inline::Picture(pic, _) if Some(Unit(here)) == unit => {
                    done |= mark(&mut pic.shape) | (&*pic.alt != text);
                    pic.alt = text.into();
                }
                Inline::Float(fl, _) if unit.is_none() && ObjectRef::Floating(FloatId(fl.id)) == o => {
                    done |= mark(&mut fl.shape) | (&*fl.alt != text);
                    fl.alt = text.into();
                }
                _ => {}
            }
        }
        if !done {
            return Outcome::Unchanged("same alt text");
        }
        let mut tx = self.begin("Alt Text");
        if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(o);
        self.commit(tx, None, fonts)
    }

    pub(super) fn insert_picture(&mut self, data: Vec<u8>, fonts: &Fonts) -> Outcome {
        let (Some((pw, ph, dx, dy)), Some(_)) = (crate::docx::image::image_info(&data), crate::docx::image::image_type(&data)) else {
            return Outcome::Unchanged("not a picture");
        };
        let (mut w, mut h) = (pw * 72.0 / if dx > 1.0 { dx } else { 96.0 }, ph * 72.0 / if dy > 1.0 { dy } else { 96.0 });
        let container = self.caret().and_then(|p| p.para_offset()).and_then(|(p, _)| self.index.para_slot(p)).map(|s| s.container);
        let tw = container.map_or(468.0, |c| self.text_width(c));
        if w > tw && w > 0.0 {
            (w, h) = (tw, h * tw / w);
        }
        let (w, h) = (((w * 20.0).round() / 20.0).max(1.0), ((h * 20.0).round() / 20.0).max(1.0));
        let image = self.pending_image(data);
        let emu = crate::drawing::units::Emu::from_pt;
        let mut shape = crate::drawing::Shape::picture(image, emu(w), emu(h));
        shape.nv.name = self.new_object_name("Picture");
        let shape = Some(Box::new(shape));
        let image = Some(image);
        let out = self.insert_inline(
            |run| {
                Inline::Picture(
                    crate::docx::Picture {
                        w,
                        h,
                        image,
                        alt: "".into(),
                        lock_aspect: true,
                        effect: [0.0; 4],
                        shape: shape.clone(),
                        text: None,
                        src: Default::default(),
                    },
                    run,
                )
            },
            "Insert Picture",
            fonts,
        );
        if let (Outcome::Changed(_), Some((para, at))) = (&out, self.caret().and_then(|p| p.para_offset()))
            && at.0 > 0
        {
            self.set_selection(Selection::Object(ObjectRef::Inline { para, offset: Unit(at.0 - 1) }));
        }
        out
    }

    pub(super) fn insert_text_box(&mut self, fonts: &Fonts) -> Outcome {
        let Some(res) = self.doc.resolver.clone() else { return Outcome::Unchanged("no styles") };
        let cx = self.cx();
        let Some(caret) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        let Some((para, _)) = caret.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        if matches!(caret.story, StoryId::TextBox(_)) {
            return Outcome::Unchanged("text boxes don't nest");
        }
        if matches!(caret.story, StoryId::Footnote(_) | StoryId::Endnote(_)) {
            return Outcome::unavailable("floating objects don't go in notes");
        }
        if self.index.para(&self.doc, para).is_none() {
            return Outcome::Unchanged("no paragraph");
        }
        let Some(run) = self.run_at(&caret) else { return Outcome::Unchanged("no run") };
        let name = self.new_object_name("Text Box");
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let r = crate::docx::para_xml::from_xml(&res, None, None, &cx);
        let text_run = run_xml::plain(&res, &r.base);
        let pid = crate::docx::next_para_id();
        let inner = crate::docx::Paragraph {
            style: std::rc::Rc::new(r.style),
            mark: text_run,
            list: None,
            inlines: Vec::new(),
            src: None,
            id: pid,
            div: None,
            controls: Vec::new(),
        };
        let fid = crate::docx::next_para_id();
        let float = crate::docx::Float {
            id: fid,
            w: 144.0,
            auto_w: false,
            h: 72.0,
            image: None,
            alt: "".into(),
            lock_aspect: false,
            text_box: Some(crate::docx::TextBox {
                blocks: vec![crate::docx::Block::Para(inner)],
                insets: [7.2, 3.6, 7.2, 3.6],
                outline: 0.5,
                auto_fit: false,
                v_anchor: 0,
                flow: 0,
            }),
            h_relative: HorizontalRelation::Column,
            h_align: None,
            h_offset: 0.0,
            v_relative: VerticalRelation::Paragraph,
            v_align: None,
            v_offset: 0.0,
            wrap: FloatWrap::None,
            wrap_text: WrapText::BothSides,
            dist: [3.6, 3.6, 9.0, 9.0],
            effect: [0.0; 4],
            poly: None,
            poly_edited: false,
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: usize::MAX / 2,
            z: self.front_z(),
            src: None,
            locked: false,
            frame_src: None,
            in_note: false,
            parts: Vec::new(),
            // The shape the saved box reopens with: its white fill and outline draw now.
            shape: Some(Box::new(crate::drawing::Shape::new(
                crate::drawing::NewShape::TextBox,
                crate::drawing::units::Emu::from_pt(144.0),
                crate::drawing::units::Emu::from_pt(72.0),
                &name,
            ))),
            rot_swap: false,
            vml: false,
            relative: Default::default(),
        };
        let mut inlines = p.inlines.clone();
        inlines.insert(0, Inline::Float(Box::new(float), run));
        let mut tx = self.begin("Insert Text Box");
        if self.step(&mut tx, Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale caret");
        }
        self.selection = Selection::Caret(Position::text(StoryId::TextBox(FloatId(fid)), crate::layout::ParaId(pid), Unit(0)));
        self.commit(tx, None, fonts)
    }

    pub(super) fn object_focus(&mut self, back: Option<bool>) -> Outcome {
        fn walk(blocks: &[crate::docx::Block], out: &mut Vec<FloatId>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => {
                        out.extend(p.inlines.iter().filter_map(|i| if let Inline::Float(f, _) = i { Some(FloatId(f.id)) } else { None }))
                    }
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| r.cells.iter()).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        let mut floats = Vec::new();
        match self.editing() {
            s @ StoryId::HeaderFooter(_) => walk(self.index.blocks(&self.doc, super::index::Container::Story(s)).map_or(&[], |b| b), &mut floats),
            _ => self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut floats)),
        }
        if floats.is_empty() {
            return Outcome::Unchanged("no floating objects");
        }
        let now = match self.selection {
            Selection::Object(ObjectRef::Floating(f)) => floats.iter().position(|g| *g == f),
            _ => None,
        };
        let k = match (back, now) {
            (Some(true), Some(i)) => (i + floats.len() - 1) % floats.len(),
            (Some(false), Some(i)) => (i + 1) % floats.len(),
            _ => 0,
        };
        self.set_selection(Selection::Object(ObjectRef::Floating(floats[k])));
        Outcome::Moved
    }

    pub(super) fn object_cmd(&mut self, c: ObjectCmd, fonts: &Fonts) -> Outcome {
        let Selection::Object(o) = self.selection else { return Outcome::Unchanged("no object selected") };
        if let ObjectCmd::AltText(text, decorative) = &c {
            return self.set_alt_text(o, text, *decorative, fonts);
        }
        if matches!(c, ObjectCmd::Size(..) | ObjectCmd::ResetSize) && self.object_fixed(o) {
            return Outcome::Unchanged("fixed size");
        }
        // The image's own size, times the share of it a crop shows.
        let natural = |e: &Self, image: Option<usize>, shape: Option<&crate::drawing::Shape>| {
            let (w, h) = e.natural_size(image)?;
            let [l, t, r, b] = shape.and_then(|s| s.pic()).map_or([0.0; 4], |p| p.blip.src.map(crate::drawing::units::Pct::fraction));
            let shown = |a: f64, b: f64| (1.0 - a - b).max(0.001);
            Some((w * shown(l, r), h * shown(t, b)))
        };
        match o {
            ObjectRef::Inline { para, offset } => {
                let Some(Inline::Picture(pic, _)) = self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)) else {
                    return Outcome::Unchanged("no picture");
                };
                let (w, h) = match c {
                    ObjectCmd::Size(w, h, _) => (w, h),
                    ObjectCmd::ResetSize => match natural(self, pic.image, pic.shape.as_deref()) {
                        Some(s) => s,
                        None => return Outcome::Unchanged("no image size"),
                    },
                    ObjectCmd::Wrap(ObjectWrap::Inline) => return Outcome::Unchanged("already inline"),
                    ObjectCmd::Wrap(_) if self.object_fixed(o) => return Outcome::Unchanged("fixed picture"),
                    ObjectCmd::Wrap(w) => return self.float_inline(para, offset, w, fonts),
                    ObjectCmd::LockAnchor(_) => return Outcome::Unchanged("inline picture"),
                    ObjectCmd::AltText(..) => return Outcome::Unchanged("handled above"),
                };
                self.set_picture_size(para, offset, w, h, fonts)
            }
            ObjectRef::Floating(f) if c == ObjectCmd::Wrap(ObjectWrap::Inline) => self.inline_float(f, fonts),
            ObjectRef::Floating(f) => {
                let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
                let label = match &c {
                    &ObjectCmd::Wrap(w) => {
                        (n.wrap, n.behind) = float_wrap(w);
                        "Wrap Text"
                    }
                    &ObjectCmd::LockAnchor(on) => {
                        n.locked = on;
                        "Lock Anchor"
                    }
                    &ObjectCmd::Size(w, h, set) => {
                        (n.w, n.h) = (w.max(1.0), h.max(1.0));
                        for (k, set) in set.into_iter().enumerate() {
                            if set {
                                n.relative.size[k] = None;
                            }
                        }
                        "Size"
                    }
                    ObjectCmd::ResetSize => match natural(self, n.image, n.shape.as_deref()) {
                        Some((w, h)) => {
                            (n.w, n.h) = if n.rot_swap { (h, w) } else { (w, h) };
                            n.relative.size = [None, None];
                            "Reset Picture"
                        }
                        None => return Outcome::Unchanged("no image size"),
                    },
                    ObjectCmd::AltText(..) => return Outcome::Unchanged("handled above"),
                };
                (n.w, n.h) = ((n.w * 20.0).round() / 20.0, (n.h * 20.0).round() / 20.0);
                self.set_float(n, label, fonts)
            }
        }
    }

    /// An image's own size, points, at its resolution (96 dpi when it has none).
    fn natural_size(&self, image: Option<usize>) -> Option<(f64, f64)> {
        let (w, h, dx, dy) = image.and_then(|i| self.doc.images.get(i)).and_then(|b| crate::docx::image::image_info(b))?;
        Some((w * 72.0 / if dx > 1.0 { dx } else { 96.0 }, h * 72.0 / if dy > 1.0 { dy } else { 96.0 }))
    }

    /// The selected picture's own size, points.
    pub fn original_object_size(&self) -> Option<(f64, f64)> {
        self.natural_size(self.selected_image())
    }

    pub(super) fn float_of(&self, f: FloatId) -> Option<&crate::docx::Float> {
        let p = self.index.para(&self.doc, self.index.float_anchor(f)?)?;
        p.inlines.iter().find_map(|i| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some(&**fl),
            _ => None,
        })
    }

    pub(super) fn set_float(&mut self, n: crate::docx::Float, label: &'static str, fonts: &Fonts) -> Outcome {
        let f = FloatId(n.id);
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let mut inlines = p.inlines.clone();
        let Some(fl) = inlines.iter_mut().find_map(|i| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
            _ => None,
        }) else {
            return Outcome::Unchanged("no float");
        };
        let mut n = n;
        self.rewrap(fl, &mut n);
        if n == **fl {
            return Outcome::Unchanged("already so");
        }
        **fl = n;
        fit_picture_control(fl, &self.doc.controls);
        let mut tx = self.begin(label);
        if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(ObjectRef::Floating(f));
        self.commit(tx, None, fonts)
    }

    pub(super) fn set_object_position(&mut self, p: crate::api::command::ObjectPosition, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        let twips = |v: f64| (v * 20.0).trunc() / 20.0;
        let mut stay = [false; 2];
        if let Some(h) = p.horizontal {
            n.h_relative = match h.relative_to {
                HorizontalReference::Column => HorizontalRelation::Column,
                HorizontalReference::Margin => HorizontalRelation::Margin,
                HorizontalReference::Page => HorizontalRelation::Page,
                HorizontalReference::Character => HorizontalRelation::Character,
                HorizontalReference::LeftMargin => HorizontalRelation::LeftMargin,
                HorizontalReference::RightMargin => HorizontalRelation::RightMargin,
                HorizontalReference::InsideMargin => HorizontalRelation::InsideMargin,
                HorizontalReference::OutsideMargin => HorizontalRelation::OutsideMargin,
            };
            n.relative.pos[0] = h.percent.map(|v| (v * 10.0).round() as i64);
            (n.h_align, n.h_offset) = match (h.align, h.offset) {
                (Some(a), _) => (
                    Some(match a {
                        ObjectHorizontalAlignment::Left => HorizontalAlign::Left,
                        ObjectHorizontalAlignment::Center => HorizontalAlign::Center,
                        ObjectHorizontalAlignment::Right => HorizontalAlign::Right,
                        ObjectHorizontalAlignment::Inside => HorizontalAlign::Inside,
                        ObjectHorizontalAlignment::Outside => HorizontalAlign::Outside,
                    }),
                    n.h_offset,
                ),
                (None, Some(o)) => (None, twips(o.0)),
                (None, None) if h.percent.is_some() => (None, n.h_offset),
                (None, None) => {
                    stay[0] = true;
                    (None, 0.0)
                }
            };
        }
        if let Some(v) = p.vertical {
            n.v_relative = match v.relative_to {
                VerticalReference::Paragraph => VerticalRelation::Paragraph,
                VerticalReference::Line => VerticalRelation::Line,
                VerticalReference::Margin => VerticalRelation::Margin,
                VerticalReference::Page => VerticalRelation::Page,
                VerticalReference::TopMargin => VerticalRelation::TopMargin,
                VerticalReference::BottomMargin => VerticalRelation::BottomMargin,
                VerticalReference::InsideMargin => VerticalRelation::InsideMargin,
                VerticalReference::OutsideMargin => VerticalRelation::OutsideMargin,
            };
            n.relative.pos[1] = v.percent.map(|p| (p * 10.0).round() as i64);
            (n.v_align, n.v_offset) = match (v.align, v.offset) {
                (Some(a), _) => (
                    Some(match a {
                        ObjectVerticalAlignment::Top => VerticalAlign::Top,
                        ObjectVerticalAlignment::Center => VerticalAlign::Center,
                        ObjectVerticalAlignment::Bottom => VerticalAlign::Bottom,
                        ObjectVerticalAlignment::Inside => VerticalAlign::Inside,
                        ObjectVerticalAlignment::Outside => VerticalAlign::Outside,
                    }),
                    n.v_offset,
                ),
                (None, Some(o)) => (None, twips(o.0)),
                (None, None) if v.percent.is_some() => (None, n.v_offset),
                (None, None) => {
                    stay[1] = true;
                    (None, 0.0)
                }
            };
        }
        n.allow_overlap = p.allow_overlap.unwrap_or(n.allow_overlap);
        n.layout_in_cell = p.layout_in_cell.unwrap_or(n.layout_in_cell);
        let was = if stay.contains(&true) { self.float_box(f, fonts) } else { None };
        let Some((page, r)) = was else { return self.set_float(n, "Position", fonts) };
        self.begin_batch(Some("Position".into()));
        let changed = matches!(self.set_float(n, "Position", fonts), Outcome::Changed(_));
        if changed
            && let Some((on, now)) = self.float_box(f, fonts)
            && on == page
        {
            let (x, y) = (if stay[0] { r[0] } else { now[0] }, if stay[1] { r[1] } else { now[1] });
            self.place_float(f, page, now, [x, y, x + now[2] - now[0], y + now[3] - now[1]], "Position", fonts);
        }
        self.end_batch(true, fonts)
    }

    pub(super) fn set_object_wrap_options(&mut self, w: crate::api::command::ObjectWrapOptions, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        if let Some(s) = w.side {
            n.wrap_text = match s {
                WrapSide::BothSides => WrapText::BothSides,
                WrapSide::Left => WrapText::Left,
                WrapSide::Right => WrapText::Right,
                WrapSide::Largest => WrapText::Largest,
            };
        }
        for (k, v) in [w.top, w.bottom, w.left, w.right].into_iter().enumerate() {
            if let Some(v) = v {
                n.dist[k] = (v.0 * 20.0).round() / 20.0;
            }
        }
        self.set_float(n, "Wrap Options", fonts)
    }

    pub(super) fn object_placement(&self) -> Option<crate::api::state::ObjectPlacement> {
        use crate::api::value::{HorizontalPosition, Points, VerticalPosition};
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return None };
        let fl = self.float_of(f)?;
        let [top, bottom, left, right] = fl.dist.map(Points);
        Some(crate::api::state::ObjectPlacement {
            horizontal: HorizontalPosition {
                relative_to: match fl.h_relative {
                    HorizontalRelation::Column | HorizontalRelation::Unrecognised => HorizontalReference::Column,
                    HorizontalRelation::Margin => HorizontalReference::Margin,
                    HorizontalRelation::Page => HorizontalReference::Page,
                    HorizontalRelation::Character => HorizontalReference::Character,
                    HorizontalRelation::LeftMargin => HorizontalReference::LeftMargin,
                    HorizontalRelation::RightMargin => HorizontalReference::RightMargin,
                    HorizontalRelation::InsideMargin => HorizontalReference::InsideMargin,
                    HorizontalRelation::OutsideMargin => HorizontalReference::OutsideMargin,
                },
                align: fl.h_align.map(|a| match a {
                    HorizontalAlign::Left | HorizontalAlign::Unrecognised => ObjectHorizontalAlignment::Left,
                    HorizontalAlign::Center => ObjectHorizontalAlignment::Center,
                    HorizontalAlign::Right => ObjectHorizontalAlignment::Right,
                    HorizontalAlign::Inside => ObjectHorizontalAlignment::Inside,
                    HorizontalAlign::Outside => ObjectHorizontalAlignment::Outside,
                }),
                offset: (fl.h_align.is_none() && fl.relative.pos[0].is_none()).then_some(Points(fl.h_offset)),
                percent: fl.relative.pos[0].map(|t| t as f64 / 10.0),
            },
            vertical: VerticalPosition {
                relative_to: match fl.v_relative {
                    VerticalRelation::Paragraph | VerticalRelation::Unrecognised => VerticalReference::Paragraph,
                    VerticalRelation::Line => VerticalReference::Line,
                    VerticalRelation::Margin => VerticalReference::Margin,
                    VerticalRelation::Page => VerticalReference::Page,
                    VerticalRelation::TopMargin => VerticalReference::TopMargin,
                    VerticalRelation::BottomMargin => VerticalReference::BottomMargin,
                    VerticalRelation::InsideMargin => VerticalReference::InsideMargin,
                    VerticalRelation::OutsideMargin => VerticalReference::OutsideMargin,
                },
                align: fl.v_align.map(|a| match a {
                    VerticalAlign::Top => ObjectVerticalAlignment::Top,
                    VerticalAlign::Center => ObjectVerticalAlignment::Center,
                    VerticalAlign::Bottom => ObjectVerticalAlignment::Bottom,
                    VerticalAlign::Inside => ObjectVerticalAlignment::Inside,
                    VerticalAlign::Outside => ObjectVerticalAlignment::Outside,
                }),
                offset: (fl.v_align.is_none() && fl.relative.pos[1].is_none()).then_some(Points(fl.v_offset)),
                percent: fl.relative.pos[1].map(|t| t as f64 / 10.0),
            },
            allow_overlap: fl.allow_overlap,
            layout_in_cell: fl.layout_in_cell,
            wrap_side: match fl.wrap_text {
                WrapText::BothSides => WrapSide::BothSides,
                WrapText::Left => WrapSide::Left,
                WrapText::Right => WrapSide::Right,
                WrapText::Largest => WrapSide::Largest,
            },
            distance: crate::api::state::TextDistance { top, bottom, left, right },
        })
    }

    pub(super) fn nudge(&mut self, f: FloatId, dx: f64, dy: f64, fonts: &Fonts) -> Outcome {
        let Some((page, r)) = self.float_box(f, fonts) else { return Outcome::Unchanged("float not drawn") };
        self.place_float(f, page, r, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy], "Move Object", fonts)
    }

    pub(super) fn float_box(&mut self, f: FloatId, fonts: &Fonts) -> Option<(usize, [f64; 4])> {
        let n = self.pages.len();
        let near = self.near;
        (0..n).map(|d| if d % 2 == 0 { near + d / 2 } else { near.wrapping_sub(d / 2 + 1) }).filter(|&p| p < n).find_map(|p| {
            self.ensure_page(p, fonts);
            let geo = self.pages.page(p, &self.index)?;
            geo.stories.iter().find(|s| s.float == Some(f)).map(|s| (p, s.rect))
        })
    }

    fn float_inline(&mut self, para: crate::layout::ParaId, offset: Unit, wrap: ObjectWrap, fonts: &Fonts) -> Outcome {
        let Some(Inline::Picture(pic, run)) = self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)).cloned() else {
            return Outcome::Unchanged("no picture");
        };
        if self.index.para_slot(para).is_some_and(|s| matches!(self.index.story(s.container), StoryId::Footnote(_) | StoryId::Endnote(_))) {
            return Outcome::unavailable("floating objects don't go in notes");
        }
        let near = self.near;
        // Where it lies in line, kept below its paragraph's top as Word's conversion keeps it: at the top exactly, the
        // last line of the paragraph before wraps around it.
        // Boxes beside the lines, turned onto the page
        let was = self.pages.pages_of(para, near, &self.index).into_iter().find_map(|page| {
            let geo = self.pages.page(page, &self.index)?;
            let top = geo.lines_of(para).map(|(_, l)| l.top).fold(f64::INFINITY, f64::min);
            geo.lines.iter().find_map(|l| {
                let &(_, _, r) = l.pictures.iter().find(|(p, u, _)| *p == para && *u == offset)?;
                let [x0, y0, x1, y1] = l.region.inverse().rect(l.turn.rect(r));
                let dy = (top + MIN_BELOW_PARA - y0).max(0.0);
                Some((page, l.region.rect([x0, y0 + dy, x1, y1 + dy])))
            })
        });
        let (wrap, behind) = float_wrap(wrap);
        let id = crate::docx::next_para_id();
        // A picture that is all of a picture control floats with its control, in a box of its own.
        let control = self.whole_picture_control(para, offset);
        let boxed = control.and_then(|(c, _)| {
            let mut q = self.index.para(&self.doc, para)?.clone();
            q.controls.clear();
            q.inlines = vec![Inline::ControlStart(c), Inline::Picture(pic.clone(), run.clone()), Inline::ControlEnd(c)];
            let crate::docx::Block::Para(q) = super::clipboard::fresh(&crate::docx::Block::Para(q)) else { return None };
            Some(crate::docx::TextBox {
                blocks: vec![crate::docx::Block::Para(q)],
                insets: [0.0; 4],
                outline: 0.0,
                auto_fit: false,
                v_anchor: 0,
                flow: 0,
            })
        });
        let mut float = crate::docx::Float {
            id,
            w: pic.w,
            auto_w: false,
            h: pic.h,
            image: if boxed.is_some() { None } else { pic.image },
            alt: pic.alt.clone(),
            lock_aspect: pic.lock_aspect || boxed.is_some(),
            text_box: boxed.clone().or_else(|| pic.text.clone().map(|t| t.text_box)),
            h_relative: HorizontalRelation::Column,
            h_align: None,
            h_offset: 0.0,
            v_relative: VerticalRelation::Paragraph,
            v_align: None,
            v_offset: 0.0,
            wrap,
            wrap_text: WrapText::BothSides,
            dist: [0.0, 0.0, 9.0, 9.0],
            effect: pic.effect,
            poly: None,
            poly_edited: false,
            behind,
            allow_overlap: true,
            layout_in_cell: true,
            order: usize::MAX / 2,
            z: self.front_z(),
            src: None,
            locked: false,
            frame_src: None,
            in_note: false,
            parts: Vec::new(),
            shape: if boxed.is_some() { None } else { pic.shape.clone() },
            rot_swap: false,
            vml: false,
            relative: Default::default(),
        };
        if let Some(deg) = float.shape.as_ref().map(|s| s.xfrm.rot.normalized().degrees()) {
            super::shapes::rotate_to(&mut float, &[], deg, self.doc.settings.policy.mode);
        }
        let unwrapped = crate::docx::Float { wrap: FloatWrap::None, ..float.clone() };
        self.rewrap(&unwrapped, &mut float);
        self.begin_batch(Some("Wrap Text".into()));
        let mut tx = self.begin("Wrap Text");
        if let Some((c, inline)) = control {
            if !self.lift_picture_control(&mut tx, para, offset, c, inline, Some(Inline::Float(Box::new(float), run))) {
                self.abort(tx);
                self.end_batch(false, fonts);
                return Outcome::Unchanged("stale object");
            }
            let f = FloatId(id);
            self.selection = Selection::Object(ObjectRef::Floating(f));
            self.commit(tx, None, fonts);
            if let (Some((page, r)), Some((on, now))) = (was, self.float_box(f, fonts))
                && on == page
            {
                self.place_float(f, page, now, [r[0], r[1], r[0] + now[2] - now[0], r[1] + now[3] - now[1]], "Wrap Text", fonts);
            }
            return self.end_batch(true, fonts);
        }
        let removed = self.step(&mut tx, Step::Text { para, range: offset..Unit(offset.0 + 1), with: Vec::new(), edges: Default::default() });
        let inlines = self.index.para(&self.doc, para).map(|p| {
            let mut v = p.inlines.clone();
            v.insert(0, Inline::Float(Box::new(float), run));
            v
        });
        if removed.is_err() || inlines.is_none_or(|inlines| self.step(&mut tx, Step::Restyle { para, inlines }).is_err()) {
            self.abort(tx);
            self.end_batch(false, fonts);
            return Outcome::Unchanged("stale object");
        }
        let f = FloatId(id);
        self.selection = Selection::Object(ObjectRef::Floating(f));
        self.commit(tx, None, fonts);
        if let (Some((page, r)), Some((on, now))) = (was, self.float_box(f, fonts))
            && on == page
        {
            self.place_float(f, page, now, [r[0], r[1], r[0] + now[2] - now[0], r[1] + now[3] - now[1]], "Wrap Text", fonts);
        }
        self.end_batch(true, fonts)
    }

    fn inline_float(&mut self, f: FloatId, fonts: &Fonts) -> Outcome {
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let Some(k) = p.inlines.iter().position(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)) else {
            return Outcome::Unchanged("no float");
        };
        let Inline::Float(fl, run) = &p.inlines[k] else { return Outcome::Unchanged("no float") };
        // A floating picture control goes back in line as its control around its picture.
        let control = fl.picture_control(&self.doc.controls).and_then(|c| {
            let Some(crate::docx::Block::Para(q)) = fl.text_box.as_ref()?.blocks.first() else { return None };
            Some((c, q.inlines.iter().find(|i| matches!(i, Inline::Picture(..)))?.clone()))
        });
        if let Some((c, picture)) = control {
            let at = Unit(p.inlines[..k].iter().map(|i| i.units() as u32).sum());
            let mut inlines = p.inlines.clone();
            inlines.remove(k);
            let mut tx = self.begin("Wrap Text");
            let with = vec![Inline::ControlStart(c), picture, Inline::ControlEnd(c)];
            if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err()
                || self.step(&mut tx, Step::Text { para: anchor, range: at..at, with, edges: Default::default() }).is_err()
            {
                self.abort(tx);
                return Outcome::Unchanged("stale object");
            }
            self.selection = Selection::Object(ObjectRef::Inline { para: anchor, offset: at });
            return self.commit(tx, None, fonts);
        }
        if !fl.parts.is_empty() || fl.shape.is_none() && (fl.image.is_none() || fl.text_box.is_some()) {
            return Outcome::unsupported("only a picture or a shape goes in line with text");
        }
        let at = Unit(p.inlines[..k].iter().map(|i| i.units() as u32).sum());
        let text = fl.text_box.clone().map(|text_box| Box::new(crate::docx::InlineText { id: crate::docx::next_para_id(), text_box }));
        let (w, h) = if fl.rot_swap { (fl.h, fl.w) } else { (fl.w, fl.h) };
        let pic = crate::docx::Picture {
            w,
            h,
            image: fl.image,
            alt: fl.alt.clone(),
            lock_aspect: fl.lock_aspect,
            effect: fl.effect,
            shape: fl.shape.clone(),
            text,
            src: Default::default(),
        };
        let with = vec![Inline::Picture(pic, run.clone())];
        let mut tx = self.begin("Wrap Text");
        let inserted = self.step(&mut tx, Step::Text { para: anchor, range: at..at, with, edges: Default::default() });
        let inlines = self
            .index
            .para(&self.doc, anchor)
            .map(|p| p.inlines.iter().filter(|i| !matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)).cloned().collect::<Vec<_>>());
        if inserted.is_err() || inlines.is_none_or(|inlines| self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err()) {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(ObjectRef::Inline { para: anchor, offset: at });
        self.commit(tx, None, fonts)
    }

    /// Where a drag puts float `f`: moved on its own page, then onto the page under the pointer and re-anchored there.
    fn dropped_float(&mut self, f: FloatId, d: &ObjectDrag) -> Option<(crate::docx::Float, Option<crate::layout::ParaId>)> {
        let new = d.placed(d.rect(self.float_locked(f)));
        let mut fl = self.placed_float_in(f, d.page, d.region, d.placed(d.rect), new, [false; 2])?;
        if d.handle.is_some() {
            return Some((fl, None));
        }
        let anchor = self.index.float_anchor(f)?;
        // Off its anchor's page, an offset from the page or its margins is from the new page's.
        if d.over != d.page && !fl.locked {
            let twips = |v: f64| (v * 20.0).trunc() / 20.0;
            let offset = d.region.inverse().along(d.offset);
            fl.h_offset = twips(fl.h_offset + offset[0]);
            if !matches!(fl.v_relative, VerticalRelation::Paragraph | VerticalRelation::Line) {
                fl.v_offset = twips(fl.v_offset + offset[1]);
            }
        }
        let dest = self.reanchor(d.over, anchor, d.on_over(new), &mut fl);
        Some((fl, dest))
    }

    pub(super) fn place_float(&mut self, f: FloatId, page: usize, old: [f64; 4], new: [f64; 4], label: &'static str, fonts: &Fonts) -> Outcome {
        let Some(moved) = self.moved_float(f, page, old, new) else { return Outcome::Unchanged("no float") };
        self.commit_float(f, moved, None, label, fonts)
    }

    pub(super) fn commit_float(
        &mut self,
        f: FloatId,
        moved: crate::docx::Float,
        to: Option<crate::layout::ParaId>,
        label: &'static str,
        fonts: &Fonts,
    ) -> Outcome {
        // A frame is placed by its paragraphs' `w:framePr`, and stays with them.
        if self.float_of(f).is_some_and(crate::docx::Float::is_frame) {
            return self.commit_frame(f, &moved, label, fonts);
        }
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let mut inlines = p.inlines.clone();
        let Some(fl) = inlines.iter_mut().find_map(|i| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
            _ => None,
        }) else {
            return Outcome::Unchanged("no float");
        };
        let mut moved = moved;
        self.rewrap(fl, &mut moved);
        **fl = moved;
        fit_picture_control(fl, &self.doc.controls);
        let mut tx = self.begin(label);
        let ok = match to {
            None => self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_ok(),
            Some(dest) => {
                let k = inlines.iter().position(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f));
                let moved = k.map(|k| inlines.remove(k));
                let into = self.index.para(&self.doc, dest).map(|p| p.inlines.clone());
                match (moved, into) {
                    (Some(m), Some(mut v)) => {
                        v.insert(0, m);
                        self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_ok()
                            && self.step(&mut tx, Step::Restyle { para: dest, inlines: v }).is_ok()
                    }
                    _ => false,
                }
            }
        };
        if !ok {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Object(ObjectRef::Floating(f));
        self.commit(tx, None, fonts)
    }

    fn reanchor(&mut self, page: usize, anchor: crate::layout::ParaId, new: [f64; 4], fl: &mut crate::docx::Float) -> Option<crate::layout::ParaId> {
        if fl.locked {
            return None;
        }
        let (dest, top, region) = self.anchor_at(page, anchor, new).filter(|&(p, ..)| p != anchor)?;
        if matches!(fl.v_relative, VerticalRelation::Paragraph | VerticalRelation::Line) {
            fl.v_offset = ((region.inverse().rect(new)[1] - top) * 20.0).trunc() / 20.0;
            fl.v_align = None;
        }
        Some(dest)
    }

    /// The boxes layout places a float from on `page`, and where their origin lies on the page: the page, its
    /// margins and, for a float anchored in `anchor`, the anchor's column, or its cell when `in_cell` lays it out there.
    pub(super) fn float_frames(
        &mut self,
        page: usize,
        anchor: Option<crate::layout::ParaId>,
        in_cell: bool,
    ) -> Option<(crate::layout::float::Frames, [f64; 2])> {
        use crate::layout::page::{Facing, columns, margins_in};
        let snap = self.snapshot();
        let info = snap.info().get(page)?;
        // A body anchor's own section, which a continuous break can start on a page another section started
        let body = anchor.filter(|a| self.index.para_slot(*a).is_some_and(|s| self.index.story(s.container) == StoryId::Body));
        let sec = self.doc.sections.get(body.and_then(|a| self.para_section(a)).unwrap_or(info.section))?;
        let set = &self.doc.settings;
        let facing = Facing { mirror: set.mirror_margins, gutter_at_top: set.gutter_at_top, book_fold: set.book_fold };
        let (left, right, top) = margins_in(sec, self.doc.review.view, facing, info.number);
        let s = sec.section_in(self.doc.review.view);
        let width = s.page_w - left - right;
        let mut fr = crate::layout::float::Frames {
            page_w: s.page_w,
            page_h: s.page_h,
            margin: [left, right, top, s.bottom],
            column: (left, width),
            compat: set.compat(),
            cell: false,
            odd: info.number % 2 == 1,
        };
        let Some(anchor) = anchor else { return Some((fr, [0.0; 2])) };
        let geo = self.pages.page(page, &self.index)?;
        let line = geo.lines_of(anchor).map(|(_, l)| l).next();
        // In a cell: its box inside its borders, and its text
        let slot = self.index.para_slot(anchor)?;
        if in_cell
            && let super::index::Container::Cell { table, row, cell } = slot.container
            && let Some(c) = self.index.table(&self.doc, table).and_then(|t| t.rows.get(row)?.cells.get(cell))
            && let Some(g) = line.and_then(|l| geo.cells.iter().find(|g| g.chain == l.chain))
        {
            let [x0, y0, x1, _] = g.rect;
            let [mt, ml, _, mr] = c.margins;
            let w = x1 - x0;
            fr = crate::layout::float::Frames {
                page_w: w,
                page_h: 0.0,
                margin: [ml, mr, 0.0, 0.0],
                column: (ml, w - ml - mr),
                cell: true,
                odd: true,
                ..fr
            };
            return Some((fr, [x0, y0 + mt]));
        }
        // The body column the anchor is drawn in
        if matches!(slot.container, super::index::Container::Section(_)) && crate::layout::vertical::of_section(sec.text_flow).is_none() {
            let x = line.and_then(|l| l.stops.iter().find(|st| st.para == anchor)).map(|st| st.x);
            let area = crate::layout::space::BodyArea {
                left,
                width,
                top: crate::layout::units::Lu::round_half_up(top),
                bottom: crate::layout::units::Lu::round_half_up(s.page_h - s.bottom),
            };
            let cols = columns(s.columns, &area, top);
            let distance = |c: &crate::layout::space::ColumnSpace| x.map_or(0.0, |x| (c.x - x).max(x - c.x - c.width).max(0.0));
            if let Some(c) = cols.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))) {
                fr.column = (c.x, c.width);
            }
        }
        Some((fr, [0.0; 2]))
    }

    /// Where the frame float `f` is laid out in is drawn on page `page`.
    pub(super) fn float_region(&mut self, f: FloatId, page: usize) -> Turn {
        self.pages.page(page, &self.index).and_then(|g| g.stories.iter().find(|s| s.float == Some(f)).map(|s| s.region)).unwrap_or(Turn::NONE)
    }

    pub(super) fn moved_float(&mut self, f: FloatId, page: usize, old: [f64; 4], new: [f64; 4]) -> Option<crate::docx::Float> {
        self.placed_float(f, page, old, new, [false; 2])
    }

    /// [`Editor::moved_float`], each axis `unplace` names going to an offset from its reference however little it moves.
    pub(super) fn placed_float(&mut self, f: FloatId, page: usize, old: [f64; 4], new: [f64; 4], unplace: [bool; 2]) -> Option<crate::docx::Float> {
        let region = self.float_region(f, page);
        self.placed_float_in(f, page, region, old, new, unplace)
    }

    /// [`Self::placed_float`] with the float's region drawn turned by `region`.
    fn placed_float_in(
        &mut self,
        f: FloatId,
        page: usize,
        region: Turn,
        old: [f64; 4],
        new: [f64; 4],
        unplace: [bool; 2],
    ) -> Option<crate::docx::Float> {
        let back = region.inverse();
        let (old, new) = (back.rect(old), back.rect(new));
        let anchor = self.index.float_anchor(f)?;
        let anchor_top = self.pages.page(page, &self.index).and_then(|g| g.lines_of(anchor).map(|(_, l)| l.top).reduce(f64::min));
        let mut fl = self.float_of(f)?.clone();
        let twips = |v: f64| (v * 20.0).trunc() / 20.0;
        let (w, h) = (new[2] - new[0], new[3] - new[1]);
        let sized = [(w - (old[2] - old[0])).abs() > 1e-9, (h - (old[3] - old[1])).abs() > 1e-9];
        // An alignment its size moves (right, centre, inside, outside) goes to an offset when the size changes
        let moved = [
            unplace[0] || (new[0] - old[0]).abs() > 1e-9 || sized[0] && fl.h_align.is_some_and(|a| a != HorizontalAlign::Left),
            unplace[1] || (new[1] - old[1]).abs() > 1e-9 || sized[1] && fl.v_align.is_some_and(|a| a != VerticalAlign::Top),
        ];
        // An aligned or percentage place goes to an offset from its reference where the float now is.
        let placed = [fl.h_align.is_some() || fl.relative.pos[0].is_some(), fl.v_align.is_some() || fl.relative.pos[1].is_some()];
        let frames = if moved[0] && placed[0] || moved[1] && placed[1] {
            self.float_frames(page, Some(anchor), fl.in_cell(self.doc.settings.compat()))
        } else {
            None
        };
        // A character's x is only known to layout: the float keeps its place from it, moved by the drag.
        let from_char = frames.as_ref().filter(|_| moved[0] && placed[0] && fl.h_relative == HorizontalRelation::Character).map(|(fr, _)| {
            let x = |f: &crate::docx::Float| crate::layout::float::h_pos(f, fr);
            crate::layout::float::sized(&fl, fr).as_ref().map_or_else(|| x(&fl), x) + new[0] - old[0]
        });
        if sized.contains(&true) {
            fl.w = w;
            fl.h = h;
        }
        // A dragged size is absolute; the offsets follow once every field they read is final.
        for (k, sized) in sized.into_iter().enumerate() {
            if sized {
                fl.relative.size[k] = None;
            }
        }
        if moved[0] {
            (fl.h_align, fl.relative.pos[0]) = (None, None);
        }
        if moved[1] {
            (fl.v_align, fl.relative.pos[1]) = (None, None);
        }
        if moved[0] {
            let at = if placed[0] { frames.as_ref().map(|(fr, o)| (fr, o[0])) } else { None };
            let was = fl.h_offset;
            fl.h_offset = twips(match at {
                Some((fr, o)) => crate::layout::float::h_offset_at(&fl, fr, from_char.unwrap_or(new[0] - o)),
                None => new[0] - (old[0] - was),
            });
        }
        if moved[1] {
            let at = if placed[1] { frames.as_ref().map(|(fr, o)| (fr, o[1])) } else { None };
            let was = fl.v_offset;
            fl.v_offset = twips(match at {
                Some((fr, o)) => {
                    let top = anchor_top.map_or(fr.margin[2], |t| t - o);
                    crate::layout::float::v_offset_at(&fl, fr, new[1] - o, top, top)
                }
                None => new[1] - (old[1] - was),
            });
        }
        Some(fl)
    }

    /// The paragraph on page `page` a float with frame `r` on the page anchors to, moved from `anchor`: its top and the
    /// frame its lines are laid out in, which `r`'s top is compared in.
    pub(super) fn anchor_at(&mut self, page: usize, anchor: crate::layout::ParaId, r: [f64; 4]) -> Option<(crate::layout::ParaId, f64, Turn)> {
        let container = self.index.para_slot(anchor)?.container;
        let geo = self.pages.page(page, &self.index)?;
        // A body paragraph's float can go to the body of any section; one in a cell or another story stays in it.
        let same = |p: crate::layout::ParaId| {
            self.index.para_slot(p).is_some_and(|s| {
                s.container == container
                    || matches!((s.container, container), (super::index::Container::Section(_), super::index::Container::Section(_)))
            })
        };
        let lines: Vec<&super::geometry::GLine> = geo.lines.iter().filter(|l| !l.repeat && l.para().is_some_and(same)).collect();
        // A float lies on its anchor's first page: a paragraph continued from the page before can't hold one here, so
        // the paragraphs starting on the page do, above or below (when the page has any).
        let starts: Vec<&super::geometry::GLine> =
            lines.iter().copied().filter(|l| l.para().is_some_and(|p| l.first_of(p) == Some(Unit(0)))).collect();
        let lines = if starts.is_empty() { lines } else { starts };
        let region = lines.first().map_or(Turn::NONE, |l| l.region);
        let y = region.inverse().rect(r)[1];
        let l = lines
            .iter()
            .filter(|l| l.top <= y + 1e-6)
            .max_by(|a, b| a.top.total_cmp(&b.top))
            .or_else(|| lines.iter().min_by(|a, b| a.top.total_cmp(&b.top)))?;
        let para = l.para()?;
        let top = lines.iter().filter(|m| m.shows(para)).map(|m| m.top).fold(f64::MAX, f64::min);
        Some((para, top, region))
    }
}

impl Editor {
    fn checkbox_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        for l in geo.lines.iter().filter(|l| l.holds(px, py) && reachable(ctx, geo.stories[l.story].host)) {
            let x = l.local([px, py])[0];
            for w in l.stops.windows(2) {
                // A legacy check box's hidden code lies between its box's stops
                if w[1].para == w[0].para && w[1].unit.0 > w[0].unit.0 && x >= w[0].x && x < w[1].x {
                    let p = self.index.para(&self.doc, w[0].para)?;
                    let check_box = match inline_at(p, w[0].unit) {
                        Some(Inline::Sym { control: Some(c), .. }) => {
                            w[1].unit.0 == w[0].unit.0 + 1
                                && matches!(self.doc.controls.get(c).map(|c| &c.props.kind), Some(crate::docx::ControlKind::Checkbox { .. }))
                        }
                        Some(Inline::FieldBegin { form: Some(f), .. }) => matches!(f.data.kind, crate::docx::forms::FormKind::CheckBox { .. }),
                        _ => false,
                    };
                    if check_box {
                        return Some(Position::text(geo.stories[l.story].story, w[0].para, w[0].unit));
                    }
                }
            }
        }
        None
    }

    pub(super) fn checkbox_toggles_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        if matches!(self.mode, super::Mode::Design | super::Mode::ReadOnly) {
            return None;
        }
        let p = self.checkbox_at(x, y, fonts)?;
        let (para, u) = p.para_offset()?;
        match self.index.para(&self.doc, para).and_then(|q| inline_at(q, u))? {
            // A legacy check box toggles only where forms are filled in
            Inline::FieldBegin { form: Some(f), .. } => (f.data.enabled && self.fills_forms_at(&p)).then_some(p),
            Inline::Sym { control: Some(c), .. } => {
                if self.protection_refuses_at(crate::api::command::CommandId::SetCheckbox, &p, &p).is_some() {
                    return None;
                }
                self.doc.controls.get(c).is_some_and(|k| k.props.lock.editable()).then_some(p)
            }
            _ => None,
        }
    }

    pub(super) fn checkbox_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(p) = self.checkbox_toggles_at(x, y, fonts) else { return false };
        self.drag = None;
        self.toggle_checkbox(p, fonts);
        true
    }

    pub(super) fn selected_checkbox(&self) -> Option<Position> {
        if let Some(p) = self.selected_form_check_box() {
            return Some(p);
        }
        if let Selection::Caret(c) = self.selection
            && let Some(t) = self.toggled_checkbox.filter(|t| t.after == c)
        {
            return Some(t.face);
        }
        let (a, b) = self.selection.ordered(&self.index)?;
        let ((pa, ua), (pb, ub)) = (a.para_offset()?, b.para_offset()?);
        if pa != pb || ub.0 != ua.0 + 1 {
            return None;
        }
        let p = self.index.para(&self.doc, pa)?;
        match inline_at(p, ua) {
            Some(Inline::Sym { control: Some(c), .. })
                if matches!(self.doc.controls.get(c).map(|c| &c.props.kind), Some(crate::docx::ControlKind::Checkbox { .. })) =>
            {
                Some(a)
            }
            _ => None,
        }
    }

    pub(super) fn toggle_checkbox(&mut self, p: Position, fonts: &Fonts) -> Outcome {
        let Some((para, u)) = p.para_offset() else { return Outcome::Unchanged("no checkbox") };
        if let Some(Inline::FieldBegin { form: Some(_), .. }) = self.index.para(&self.doc, para).and_then(|q| inline_at(q, u)) {
            return self.toggle_form_check_box(p, fonts);
        }
        let Some(Inline::Sym { ch, font, run, control: Some(c) }) = self.index.para(&self.doc, para).and_then(|q| inline_at(q, u)).cloned() else {
            return Outcome::Unchanged("no checkbox");
        };
        let Some(crate::docx::ControlKind::Checkbox { checked, unchecked }) = self.doc.controls.get(&c).map(|c| c.props.kind.clone()) else {
            return Outcome::Unchanged("no checkbox");
        };
        if !self.editable(&p) {
            self.refused = Some("locked");
            self.events.push(super::input::Notice::Refused { sub_template: None, element: Some(crate::docx::ElementId::Control(c)) });
            return Outcome::Unchanged("locked");
        }
        let face = if ch == checked { unchecked } else { checked };
        let mut tx = self.begin("Checkbox");
        if self
            .step(
                &mut tx,
                Step::Text {
                    para,
                    range: u..Unit(u.0 + 1),
                    with: vec![Inline::Sym { ch: face, font, run, control: Some(c) }],
                    edges: Default::default(),
                },
            )
            .is_err()
        {
            self.abort(tx);
            return Outcome::Unchanged("stale checkbox");
        }
        let after = Position::text(p.story, para, Unit(u.0 + 1));
        self.selection = Selection::Caret(after);
        self.toggled_checkbox = Some(ToggledCheckbox { face: p, after });
        self.commit(tx, None, fonts)
    }
}

fn amount(pt: f64, unit: crate::api::services::RulerUnit) -> String {
    use crate::api::services::RulerUnit as U;
    let per = match unit {
        U::Inches => 72.0,
        U::Centimeters => 72.0 / 2.54,
        U::Millimeters => 72.0 / 25.4,
        U::Points => 1.0,
        U::Picas => 12.0,
    };
    format!("{:.2}", pt / per)
}

fn unit_suffix(unit: crate::api::services::RulerUnit) -> &'static str {
    use crate::api::services::RulerUnit as U;
    match unit {
        U::Inches => " in",
        U::Centimeters => " cm",
        U::Millimeters => " mm",
        U::Points => " pt",
        U::Picas => " pi",
    }
}

pub(super) fn size_label(w: f64, h: f64, unit: crate::api::services::RulerUnit) -> String {
    format!("{} × {}{}", amount(w, unit), amount(h, unit), unit_suffix(unit))
}

pub(super) fn angle_label(deg: f64) -> String {
    let d = (deg.round() + 180.0).rem_euclid(360.0) - 180.0;
    format!("{}°", if d == -180.0 { 180.0 } else { d })
}

fn grid_step(unit: crate::api::services::RulerUnit) -> f64 {
    use crate::api::services::RulerUnit as U;
    match unit {
        U::Inches | U::Points | U::Picas => 18.0,
        U::Centimeters | U::Millimeters => 72.0 / 2.54 / 4.0,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Dragged {
    Float(FloatId),
    Table(crate::layout::TableId),
}

pub(super) type Feedback = (Vec<(usize, Guides)>, Vec<(usize, Measure)>, Option<Outline>);

impl Editor {
    pub(super) fn snap_threshold(&self) -> Pt {
        Pt(7.0 * 0.75 / self.viewport.zoom.max(0.1))
    }

    fn page_frame(&mut self, page: usize) -> Option<([f64; 2], [f64; 4])> {
        let (w, h) = self.snapshot().pages().get(page).map(|p| (p.w, p.h))?;
        let (fr, _) = self.float_frames(page, None, false)?;
        let [left, right, top, bottom] = fr.margin;
        Some(([w, h], [left, top, w - right, h - bottom]))
    }

    fn grid_from(&self, margins: [f64; 4], dragging: bool) -> Option<Grid> {
        use crate::api::services::ObjectGrid as G;
        let shows = match self.view_settings.object_grid {
            G::Off => false,
            G::WhileDragging => dragging,
            G::Always => true,
        };
        shows.then(|| Grid { origin: [Pt(margins[0]), Pt(margins[1])], step: Pt(grid_step(self.view_settings.unit)) })
    }

    pub(super) fn build_snaps(&mut self, page: usize, dragged: Dragged) {
        use crate::layout::fragment::Item;
        let Some((size, margins)) = self.page_frame(page) else { return };
        let Some(geo) = self.pages.page(page, &self.index) else { return };
        let mut tables: Vec<(crate::layout::TableId, [f64; 4])> = Vec::new();
        for r in &geo.rows {
            let [a0, b0, a1, b1] = r.rect;
            match tables.iter_mut().find(|t| t.0 == r.table) {
                Some((_, b)) => *b = [b[0].min(a0), b[1].min(b0), b[2].max(a1), b[3].max(b1)],
                None => tables.push((r.table, r.rect)),
            }
        }
        let snap = self.snapshot();
        let items = snap.pages().get(page).map(|p| crate::layout::traverse::paint_items(p)).unwrap_or_default();
        let shape_bounds = |f: u64| {
            items.iter().find_map(|it| match it {
                Item::Shape(s) if s.float == f && s.path.is_empty() && !s.decoration => Some(super::snap::turned_bounds(s.rect, s.rot)),
                _ => None,
            })
        };
        let floats: Vec<[f64; 4]> = geo.stories.iter().filter(|s| s.float.is_some()).map(|s| s.rect).collect();
        let mut objects: Vec<[f64; 4]> = geo
            .stories
            .iter()
            .filter_map(|s| s.float.filter(|f| Dragged::Float(*f) != dragged).map(|f| shape_bounds(f.0).unwrap_or(s.rect)))
            .collect();
        let in_float = |r: &[f64; 4]| floats.iter().any(|f| r[0] >= f[0] - 0.5 && r[1] >= f[1] - 0.5 && r[2] <= f[2] + 0.5 && r[3] <= f[3] + 0.5);
        for it in &items {
            match it {
                Item::Shape(s) if s.float == 0 && s.path.is_empty() && !s.decoration => objects.push(super::snap::turned_bounds(s.rect, s.rot)),
                Item::Image { rect, .. } if !in_float(rect) => objects.push(*rect),
                _ => {}
            }
        }
        let tables: Vec<[f64; 4]> = tables.into_iter().filter(|t| Dragged::Table(t.0) != dragged).map(|t| t.1).collect();
        let grid = self.grid_from(margins, true);
        self.snaps.fill(&super::snap::Sources { size, margins, tables: &tables, objects: &objects, grid });
    }

    pub(super) fn object_feedback(&mut self, visible: std::ops::Range<usize>, anchor: Option<(usize, [f64; 2])>) -> Feedback {
        use crate::api::services::MeasureDetail as D;
        let (mut guides, mut measures, mut original) = (Vec::new(), Vec::new(), None);
        let set = self.view_settings;
        let one = 0.75 / self.viewport.zoom.max(0.1);
        let chip = |at: [f64; 2], label: String| Measure::Chip { at: [Px::of(Pt(at[0] + 14.0 * one)), Px::of(Pt(at[1] + 18.0 * one))], label };
        let lines = |snaps: &super::snap::SnapLines, hit: [Option<SnapLine>; 2]| -> Vec<GuideLine> {
            let [w, h] = snaps.size;
            let along = [(Axis::X, (Pt(0.0), Pt(h))), (Axis::Y, (Pt(0.0), Pt(w)))];
            hit.iter()
                .zip(along)
                .filter_map(|(l, (axis, span))| l.filter(|l| l.kind != super::snap::SnapKind::Grid).map(|l| GuideLine { axis, at: l.at, span }))
                .collect()
        };
        if let Some(d) = self.object_drag.filter(|d| d.now != d.start) {
            let (ox, oy) = self.stack().to_doc(d.page, 0.0, 0.0).unwrap_or((0.0, 0.0));
            match d.handle {
                None => {
                    let [x0, y0, x1, y1] = super::snap::turned_bounds(d.on_over(d.rect(false)), d.rot);
                    let [ml, _, mr, _] = self.snaps.margins;
                    let anchor_link = anchor.filter(|(p, _)| *p == d.over).and_then(|(_, [ax, ay])| {
                        let (rx, ry) = ((ax - 18.0 * one).max(10.0 * one) + 6.0 * one, ay + 8.0 * one);
                        (x0 > rx).then_some([Pt(rx), Pt(ry), Pt(x0), Pt(ry)])
                    });
                    guides.push((d.over, Guides { lines: lines(&self.snaps, d.snapped), grid: self.snaps.grid, anchor_link }));
                    let (left, right) = (x0 - ml, mr - x1);
                    let equal = (left - right).abs() < 1.5 * one;
                    let shown = match set.detail {
                        D::Quiet => false,
                        D::Standard => equal || d.snapping == Snapping::Free,
                        D::Full => true,
                    };
                    if shown {
                        let tone = if equal { Tone::Accent } else { Tone::Muted };
                        let cy = Pt((y0 + y1) / 2.0);
                        let mut dim = |a: f64, b: f64| {
                            if b - a > 2.0 * one {
                                measures
                                    .push((d.over, Measure::Dimension { from: [Pt(a), cy], to: [Pt(b), cy], label: amount(b - a, set.unit), tone }));
                            }
                        };
                        dim(ml, x0);
                        dim(x1, mr);
                    }
                }
                Some(_) => {
                    let [x0, y0, x1, y1] = d.rect(self.object_locked(d.obj));
                    original = Some(Outline { page: d.page, rect: d.rect, rot: d.rot });
                    measures.push((d.page, chip([d.now.0 - ox, d.now.1 - oy], size_label(x1 - x0, y1 - y0, set.unit))));
                }
            }
        }
        if let Some(d) = &self.shape_drag {
            match d.handle {
                super::shapes::ShapeHandle::Rotate => {
                    original = Some(Outline { page: d.page, rect: d.item.rect, rot: d.item.rot });
                    measures.push((d.page, chip(d.now, angle_label(d.angle()))));
                }
                super::shapes::ShapeHandle::Move => {
                    guides.push((d.page, Guides { lines: lines(&self.snaps, d.snapped), grid: self.snaps.grid, anchor_link: None }));
                }
                super::shapes::ShapeHandle::Adjust(_) => {}
            }
        }
        if set.object_grid == crate::api::services::ObjectGrid::Always {
            for page in visible {
                if !guides.iter().any(|(p, _)| *p == page)
                    && let Some((_, margins)) = self.page_frame(page)
                {
                    guides.push((page, Guides { grid: self.grid_from(margins, false), ..Guides::default() }));
                }
            }
        }
        (guides, measures, original)
    }
}

/// A floating picture control's picture takes its box's size.
fn fit_picture_control(fl: &mut crate::docx::Float, controls: &std::collections::HashMap<crate::docx::ControlId, crate::docx::Control>) {
    if fl.picture_control(controls).is_none() {
        return;
    }
    let (w, h) = (fl.w, fl.h);
    if let Some(crate::docx::Block::Para(p)) = fl.text_box.as_mut().and_then(|tb| tb.blocks.first_mut()) {
        for i in &mut p.inlines {
            if let Inline::Picture(pic, _) = i {
                (pic.w, pic.h) = (w, h);
            }
        }
    }
}

#[cfg(test)]
mod decoration_tests {
    use super::*;

    /// Wavy underlines and check boxes are drawn as shapes in their lines: not objects a dragged one snaps to.
    #[test]
    fn decorations_are_not_snap_targets() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/render/underlines.docx");
        let mut e = Editor::open(&std::fs::read(path).expect("the fixture"), crate::editing::tests::fonts()).expect("opens");
        let snap = e.snapshot();
        let waves = crate::layout::traverse::paint_items(&snap.pages()[0])
            .iter()
            .filter(|i| matches!(i, crate::layout::fragment::Item::Shape(s) if s.decoration))
            .count();
        assert!(waves >= 8, "the fixture's waves are decoration shapes: {waves}");
        e.build_snaps(0, Dragged::Float(FloatId(u64::MAX)));
        let objects = |l: &[SnapLine]| {
            l.iter().filter(|l| matches!(l.kind, super::super::snap::SnapKind::ObjectEdge | super::super::snap::SnapKind::ObjectCentre)).count()
        };
        assert_eq!((objects(&e.snaps.x), objects(&e.snaps.y)), (0, 0));
    }
}
