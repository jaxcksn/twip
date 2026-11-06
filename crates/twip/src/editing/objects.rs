//! Object selection

use crate::docx::Inline;
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
            let in_box = px >= x0 - one && px <= x1 + one && py >= y0 - one && py <= y1 + one;
            let text_box = self.index.text_box(&self.doc, self.index.box_story(f)).is_some();
            if !reachable(ctx, s.host) || text_box && !in_box || !text_box && !self.shape_hit(page, f, px, py, 3.0 * one, in_box) {
                continue;
            }
            let text_over = || {
                geo.lines.iter().any(|l| {
                    l.story != si
                        && py >= l.top
                        && py < l.bottom
                        && l.stops.first().zip(l.stops.last()).is_some_and(|(a, b)| px >= a.x && px <= b.x)
                        && reachable(ctx, geo.stories[l.story].host)
                })
            };
            if self.float_behind(f) && text_over() {
                continue;
            }
            if !text_box {
                return Some(Selection::Object(ObjectRef::Floating(f)));
            }
            let left = px - x0 < band;
            if !(left || x1 - px < band || py - y0 < band || y1 - py < band) {
                return None;
            }
            if left
                && let Some(i) = geo.lines.iter().position(|l| l.story == si && py >= l.top && py < l.bottom)
                && let Some((a, b)) = self.line_span_of(&geo, i)
            {
                return Some(Selection::range(a, b, Granularity::Line));
            }
            return Some(Selection::Object(self.story_object(f)));
        }
        for l in geo.lines.iter().filter(|l| py >= l.top && py < l.bottom && reachable(ctx, geo.stories[l.story].host)) {
            for w in l.stops.windows(2) {
                if w[1].para == w[0].para && w[1].unit.0 == w[0].unit.0 + 1 && px >= w[0].x && px < w[1].x {
                    let p = self.index.para(&self.doc, w[0].para)?;
                    if matches!(inline_at(p, w[0].unit), Some(Inline::Picture(..))) {
                        return Some(Selection::Object(ObjectRef::Inline { para: w[0].para, offset: w[0].unit }));
                    }
                }
            }
        }
        None
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
            crate::layout::fragment::Item::Shape(s) if s.hit(x, y, slop) => Some(s.path.clone()),
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
        let mut shapes = items.iter().filter_map(|it| if let crate::layout::fragment::Item::Shape(s) = it { Some(s) } else { None }).peekable();
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
        match o {
            ObjectRef::Inline { para, offset } => {
                let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
                let a = Position::text(story, para, offset);
                self.selection = Selection::range(a, Position::text(story, para, Unit(offset.0 + 1)), Granularity::Char);
                None
            }
            ObjectRef::Floating(f) => Some(self.remove_float(f, fonts)),
        }
    }

    fn remove_float(&mut self, f: FloatId, fonts: &Fonts) -> Outcome {
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let inlines: Vec<Inline> = p.inlines.iter().filter(|i| !matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f)).cloned().collect();
        let story = self.index.para_slot(anchor).map_or(StoryId::Body, |s| self.index.story(s.container));
        let mut tx = self.begin("Delete");
        if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale object");
        }
        self.selection = Selection::Caret(Position::text(story, anchor, Unit(0)));
        self.commit(tx, None, fonts)
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum ObjectCmd {
    Wrap(String),
    LockAnchor(bool),
    Size(f64, f64),
    ResetSize,
    AltText(String, Option<bool>),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ToggledCheckbox {
    pub face: Position,
    pub after: Position,
}

fn ooxml<T: serde::Serialize>(v: T) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(str::to_owned)).unwrap_or_default()
}

fn from_ooxml<T: serde::de::DeserializeOwned + Default>(s: &str) -> T {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or_default()
}

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
        }
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
        [nx0, ny0, nx0 + w, ny0 + h]
    }
}

impl Editor {
    pub(super) fn handle_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<(ObjectRef, [f64; 4], usize, usize)> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let one = 0.75 / self.viewport.zoom.max(0.1);
        let (obj, rect) = match self.selection {
            Selection::Object(o @ ObjectRef::Floating(f)) => geo.stories.iter().find(|s| s.float == Some(f)).map(|s| (o, s.rect)),
            Selection::Object(o @ ObjectRef::Inline { para, offset }) => self.inline_box(&geo, para, offset).map(|r| (o, r)),
            _ => None,
        }
        .filter(|(o, _)| !self.object_fixed(*o))?;
        let [x0, y0, x1, y1] = rect;
        let at = |side: f64, lo: f64, hi: f64| match side {
            s if s < 0.0 => lo,
            s if s > 0.0 => hi,
            _ => (lo + hi) / 2.0,
        };
        let k = (0..8).find(|&k| {
            let (sx, sy) = HANDLES[k];
            (px - at(sx, x0, x1)).abs() <= 4.0 * one && (py - at(sy, y0, y1)).abs() <= 4.0 * one
        })?;
        Some((obj, rect, page, k))
    }

    pub(super) fn object_press(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) -> bool {
        if let Some((obj, rect, page, k)) = self.handle_at(x, y, fonts) {
            self.object_drag = Some(ObjectDrag::new(obj, Some(k), (x, y), rect, page, ctrl));
            self.requests.push(Request::CapturePointer);
            return true;
        }
        let stack = self.stack();
        let Some((page, ..)) = stack.page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let Some(geo) = self.pages.page(page, &self.index) else { return false };
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
        self.activate_placeholder();
        if let Selection::Object(obj @ ObjectRef::Floating(f)) = s
            && self.selected_child().is_some()
            && !self.object_fixed(obj)
        {
            self.child_move_press(f, page, px, py);
            return true;
        }
        if let Selection::Object(obj @ ObjectRef::Floating(f)) = s
            && !self.object_fixed(obj)
            && let Some(st) = geo.stories.iter().find(|t| t.float == Some(f))
        {
            self.object_drag = Some(ObjectDrag::new(obj, None, (x, y), st.rect, page, ctrl));
            self.build_snaps(page, Dragged::Float(f));
            self.requests.push(Request::CapturePointer);
        }
        if let Selection::Object(obj @ ObjectRef::Inline { para, offset }) = s
            && !self.object_fixed(obj)
            && self.mode != super::Mode::ReadOnly
        {
            let story = self.index.para_slot(para).map_or(StoryId::Body, |s| self.index.story(s.container));
            let press = Position::text(story, para, offset);
            self.text_drag = Some(super::interaction::TextDrag { start: (x, y), press, moved: false, copy: ctrl, drop: None });
            self.requests.push(Request::CapturePointer);
        }
        true
    }

    pub(super) fn object_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.object_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        let label = if d.handle.is_some() { "Resize Object" } else { "Move Object" };
        let out = match d.obj {
            _ if d.now == d.start => Outcome::Unchanged("no drag"),
            ObjectRef::Floating(f) => self.place_float(f, d.page, d.rect, d.rect(self.float_locked(f)), label, d.handle.is_none(), fonts),
            ObjectRef::Inline { para, offset } => {
                let [x0, y0, x1, y1] = d.rect(self.object_locked(d.obj));
                self.set_picture_size(para, offset, x1 - x0, y1 - y0, fonts)
            }
        };
        self.end_preview(d);
        Some(out)
    }

    pub(super) fn object_move(&mut self, x: f64, y: f64, mods: super::keymap::Mods) {
        let Some(mut d) = self.object_drag else { return };
        (d.now, d.shift) = ((x, y), mods.shift);
        d.snapping = if mods.alt { Snapping::Free } else { Snapping::On };
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
                let (off, hit) = super::snap::snap(d.rect(false), &self.snaps, self.snap_threshold());
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
        if d.live {
            let to = d.rect(self.float_locked(f));
            let preview = self.moved_float(f, d.page, d.rect, to).and_then(|mut float| {
                let anchor = self.index.float_anchor(f)?;
                let dest = if d.handle.is_none() { self.reanchor(d.page, anchor, to, &mut float) } else { None };
                Some(crate::layout::Preview { float, anchor: dest.unwrap_or(anchor) })
            });
            let at = preview.as_ref().map(|p| p.anchor);
            self.preview_layout(f, preview, d.previewed);
            d.previewed = at;
        }
        self.object_drag = Some(d);
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

    pub(super) fn object_fixed(&self, o: ObjectRef) -> bool {
        let image = match o {
            ObjectRef::Inline { para, offset } => match self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)) {
                Some(Inline::Picture(p, _)) => p.image,
                _ => None,
            },
            ObjectRef::Floating(f) => self.index.float_anchor(f).and_then(|a| self.index.para(&self.doc, a)).and_then(|p| {
                p.inlines.iter().find_map(|i| match i {
                    Inline::Float(fl, _) if FloatId(fl.id) == f => fl.image,
                    _ => None,
                })
            }),
        };
        image.is_some_and(|i| self.doc.fixed.contains(&i))
    }

    pub(super) fn object_locked(&self, o: ObjectRef) -> bool {
        match o {
            ObjectRef::Floating(f) => self.float_locked(f),
            ObjectRef::Inline { para, offset } => {
                matches!(self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)), Some(Inline::Picture(p, _)) if p.lock_aspect)
            }
        }
    }

    pub(super) fn float_locked(&self, f: FloatId) -> bool {
        self.index.text_box(&self.doc, f).is_none()
            && self
                .index
                .float_anchor(f)
                .and_then(|a| self.index.para(&self.doc, a))
                .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f && fl.lock_aspect)))
    }

    fn float_behind(&self, f: FloatId) -> bool {
        self.index
            .float_anchor(f)
            .and_then(|a| self.index.para(&self.doc, a))
            .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f && fl.behind)))
    }

    pub(super) fn inline_box(&self, geo: &super::geometry::PageGeometry, para: crate::layout::ParaId, offset: Unit) -> Option<[f64; 4]> {
        geo.lines.iter().find_map(|l| l.pictures.iter().find(|(p, u, _)| *p == para && *u == offset).map(|(_, _, r)| *r))
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
        pic.w = w;
        pic.h = h;
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
        let image = Some(self.pending_image(data));
        let out = self.insert_inline(
            |run| {
                Inline::Picture(
                    crate::docx::Picture { w, h, image, alt: "".into(), lock_aspect: true, effect: [0.0; 4], shape: None, text: None },
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
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let Some(run) = self.run_at(&caret) else { return Outcome::Unchanged("no run") };
        let r = res.para_from_xml(None, None, &cx);
        let text_run = res.run_from_xml(&r.base, None, &cx);
        let pid = crate::docx::next_para_id();
        let inner = crate::docx::Paragraph {
            style: std::rc::Rc::new(r.style),
            mark: text_run,
            list: None,
            inlines: Vec::new(),
            src: None,
            id: pid,
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
            }),
            h_relative: "column".into(),
            h_align: None,
            h_offset: 0.0,
            v_relative: "paragraph".into(),
            v_align: None,
            v_offset: 0.0,
            wrap: "none".into(),
            wrap_text: "bothSides".into(),
            dist: [3.6, 3.6, 9.0, 9.0],
            effect: [0.0; 4],
            poly: None,
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            order: usize::MAX / 2,
            z: u32::MAX,
            src: None,
            locked: false,
            frame_src: None,
            parts: Vec::new(),
            shape: None,
            rot_swap: false,
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
        let natural = |e: &Self, image: Option<usize>| {
            let (w, h, dx, dy) = image.and_then(|i| e.doc.images.get(i)).and_then(|b| crate::docx::image::image_info(b))?;
            Some((w * 72.0 / if dx > 1.0 { dx } else { 96.0 }, h * 72.0 / if dy > 1.0 { dy } else { 96.0 }))
        };
        match o {
            ObjectRef::Inline { para, offset } => {
                let Some(Inline::Picture(pic, _)) = self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)) else {
                    return Outcome::Unchanged("no picture");
                };
                let (w, h) = match c {
                    ObjectCmd::Size(w, h) => (w, h),
                    ObjectCmd::ResetSize => match natural(self, pic.image) {
                        Some(s) => s,
                        None => return Outcome::Unchanged("no image size"),
                    },
                    ObjectCmd::Wrap(w) if w == "inline" => return Outcome::Unchanged("already inline"),
                    ObjectCmd::Wrap(_) if self.object_fixed(o) => return Outcome::Unchanged("fixed picture"),
                    ObjectCmd::Wrap(w) => return self.float_inline(para, offset, &w, fonts),
                    ObjectCmd::LockAnchor(_) => return Outcome::Unchanged("inline picture"),
                    ObjectCmd::AltText(..) => return Outcome::Unchanged("handled above"),
                };
                self.set_picture_size(para, offset, w, h, fonts)
            }
            ObjectRef::Floating(f) if matches!(&c, ObjectCmd::Wrap(w) if w == "inline") => self.inline_float(f, fonts),
            ObjectRef::Floating(f) => {
                let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
                let label = match &c {
                    ObjectCmd::Wrap(w) => {
                        (n.wrap, n.behind) = match w.as_str() {
                            "behind" => ("none".into(), true),
                            "front" => ("none".into(), false),
                            other => (other.into(), false),
                        };
                        "Wrap Text"
                    }
                    &ObjectCmd::LockAnchor(on) => {
                        n.locked = on;
                        "Lock Anchor"
                    }
                    ObjectCmd::Size(w, h) => {
                        (n.w, n.h) = (w.max(1.0), h.max(1.0));
                        "Size"
                    }
                    ObjectCmd::ResetSize => match natural(self, n.image) {
                        Some((w, h)) => {
                            (n.w, n.h) = (w, h);
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
        if n == **fl {
            return Outcome::Unchanged("already so");
        }
        **fl = n;
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
            n.h_relative = ooxml(h.relative_to);
            (n.h_align, n.h_offset) = match (h.align, h.offset) {
                (Some(a), _) => (Some(ooxml(a)), n.h_offset),
                (None, Some(o)) => (None, twips(o.0)),
                (None, None) => {
                    stay[0] = true;
                    (None, 0.0)
                }
            };
        }
        if let Some(v) = p.vertical {
            n.v_relative = ooxml(v.relative_to);
            (n.v_align, n.v_offset) = match (v.align, v.offset) {
                (Some(a), _) => (Some(ooxml(a)), n.v_offset),
                (None, Some(o)) => (None, twips(o.0)),
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
            self.place_float(f, page, now, [x, y, x + now[2] - now[0], y + now[3] - now[1]], "Position", false, fonts);
        }
        self.end_batch(true, fonts)
    }

    pub(super) fn set_object_wrap_options(&mut self, w: crate::api::command::ObjectWrapOptions, fonts: &Fonts) -> Outcome {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return Outcome::unavailable("no floating object selected") };
        let Some(mut n) = self.float_of(f).cloned() else { return Outcome::Unchanged("no float") };
        if let Some(s) = w.side {
            n.wrap_text = ooxml(s);
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
                relative_to: from_ooxml(&fl.h_relative),
                align: fl.h_align.as_deref().map(from_ooxml),
                offset: fl.h_align.is_none().then_some(Points(fl.h_offset)),
            },
            vertical: VerticalPosition {
                relative_to: from_ooxml(&fl.v_relative),
                align: fl.v_align.as_deref().map(from_ooxml),
                offset: fl.v_align.is_none().then_some(Points(fl.v_offset)),
            },
            allow_overlap: fl.allow_overlap,
            layout_in_cell: fl.layout_in_cell,
            wrap_side: from_ooxml(&fl.wrap_text),
            distance: crate::api::state::TextDistance { top, bottom, left, right },
        })
    }

    pub(super) fn nudge(&mut self, f: FloatId, dx: f64, dy: f64, fonts: &Fonts) -> Outcome {
        let Some((page, r)) = self.float_box(f, fonts) else { return Outcome::Unchanged("float not drawn") };
        self.place_float(f, page, r, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy], "Move Object", false, fonts)
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

    fn float_inline(&mut self, para: crate::layout::ParaId, offset: Unit, wrap: &str, fonts: &Fonts) -> Outcome {
        let Some(Inline::Picture(pic, run)) = self.index.para(&self.doc, para).and_then(|p| inline_at(p, offset)).cloned() else {
            return Outcome::Unchanged("no picture");
        };
        let near = self.near;
        let was = self.pages.pages_of(para, near, &self.index).into_iter().find_map(|page| {
            let geo = self.pages.page(page, &self.index)?;
            self.inline_box(&geo, para, offset).map(|r| (page, r))
        });
        let (wrap, behind) = match wrap {
            "behind" => ("none", true),
            "front" => ("none", false),
            w => (w, false),
        };
        let id = crate::docx::next_para_id();
        let float = crate::docx::Float {
            id,
            w: pic.w,
            auto_w: false,
            h: pic.h,
            image: pic.image,
            alt: pic.alt.clone(),
            lock_aspect: pic.lock_aspect,
            text_box: pic.text.clone().map(|t| t.text_box),
            h_relative: "column".into(),
            h_align: None,
            h_offset: 0.0,
            v_relative: "paragraph".into(),
            v_align: None,
            v_offset: 0.0,
            wrap: wrap.into(),
            wrap_text: "bothSides".into(),
            dist: [0.0, 0.0, 9.0, 9.0],
            effect: [0.0; 4],
            poly: None,
            behind,
            allow_overlap: true,
            layout_in_cell: true,
            order: usize::MAX / 2,
            z: u32::MAX,
            src: None,
            locked: false,
            frame_src: None,
            parts: Vec::new(),
            shape: pic.shape.clone(),
            rot_swap: false,
        };
        self.begin_batch(Some("Wrap Text".into()));
        let mut tx = self.begin("Wrap Text");
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
            self.place_float(f, page, now, [r[0], r[1], r[0] + now[2] - now[0], r[1] + now[3] - now[1]], "Wrap Text", false, fonts);
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
        if !fl.parts.is_empty() || fl.shape.is_none() && (fl.image.is_none() || fl.text_box.is_some()) {
            return Outcome::unsupported("only a picture or a shape goes in line with text");
        }
        let at = Unit(p.inlines[..k].iter().map(|i| i.units() as u32).sum());
        let text = fl.text_box.clone().map(|text_box| Box::new(crate::docx::InlineText { id: crate::docx::next_para_id(), text_box }));
        let pic = crate::docx::Picture {
            w: fl.w,
            h: fl.h,
            image: fl.image,
            alt: fl.alt.clone(),
            lock_aspect: fl.lock_aspect,
            effect: [0.0; 4],
            shape: fl.shape.clone(),
            text,
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

    pub(super) fn place_float(
        &mut self,
        f: FloatId,
        page: usize,
        old: [f64; 4],
        new: [f64; 4],
        label: &'static str,
        reanchor: bool,
        fonts: &Fonts,
    ) -> Outcome {
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(moved) = self.moved_float(f, page, old, new) else { return Outcome::Unchanged("no float") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let mut inlines = p.inlines.clone();
        let Some(fl) = inlines.iter_mut().find_map(|i| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
            _ => None,
        }) else {
            return Outcome::Unchanged("no float");
        };
        **fl = moved;
        let to = if reanchor { self.reanchor(page, anchor, new, fl) } else { None };
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
        let (dest, top) = self.anchor_at(page, anchor, new[1]).filter(|&(p, _)| p != anchor)?;
        if matches!(fl.v_relative.as_str(), "paragraph" | "line") {
            fl.v_offset = ((new[1] - top) * 20.0).trunc() / 20.0;
            fl.v_align = None;
        }
        Some(dest)
    }

    fn moved_float(&mut self, f: FloatId, page: usize, old: [f64; 4], new: [f64; 4]) -> Option<crate::docx::Float> {
        let anchor = self.index.float_anchor(f)?;
        let section = self.snapshot().info().get(page).map_or(0, |i| i.section);
        let sec = self.doc.sections.get(section).cloned();
        let anchor_top = self.pages.page(page, &self.index).and_then(|g| g.lines_of(anchor).map(|(_, l)| l.top).reduce(f64::min));
        let mut fl = self.float_of(f)?.clone();
        let (ox, oy) = match &sec {
            Some(s) => (
                match fl.h_relative.as_str() {
                    _ if fl.h_align.is_none() => old[0] - fl.h_offset,
                    "page" | "leftMargin" | "insideMargin" | "outsideMargin" => 0.0,
                    "rightMargin" => s.page_w - s.right,
                    _ => s.left,
                },
                match fl.v_relative.as_str() {
                    _ if fl.v_align.is_none() => old[1] - fl.v_offset,
                    "page" | "topMargin" => 0.0,
                    "bottomMargin" => s.page_h - s.bottom,
                    "paragraph" | "line" => anchor_top.unwrap_or(s.top),
                    _ => s.top,
                },
            ),
            None => (old[0] - fl.h_offset, old[1] - fl.v_offset),
        };
        let twips = |v: f64| (v * 20.0).trunc() / 20.0;
        if (new[0] - old[0]).abs() > 1e-9 {
            fl.h_offset = twips(new[0] - ox);
            fl.h_align = None;
        }
        if (new[1] - old[1]).abs() > 1e-9 {
            fl.v_offset = twips(new[1] - oy);
            fl.v_align = None;
        }
        let (w, h) = (new[2] - new[0], new[3] - new[1]);
        if (w - (old[2] - old[0])).abs() > 1e-9 || (h - (old[3] - old[1])).abs() > 1e-9 {
            fl.w = w;
            fl.h = h;
        }
        Some(fl)
    }

    pub(super) fn anchor_at(&mut self, page: usize, anchor: crate::layout::ParaId, y: f64) -> Option<(crate::layout::ParaId, f64)> {
        let container = self.index.para_slot(anchor)?.container;
        let geo = self.pages.page(page, &self.index)?;
        let same = |p: crate::layout::ParaId| self.index.para_slot(p).is_some_and(|s| s.container == container);
        let lines: Vec<&super::geometry::GLine> = geo.lines.iter().filter(|l| !l.repeat && l.para().is_some_and(same)).collect();
        let l = lines
            .iter()
            .filter(|l| l.top <= y + 1e-6)
            .max_by(|a, b| a.top.total_cmp(&b.top))
            .or_else(|| lines.iter().min_by(|a, b| a.top.total_cmp(&b.top)))?;
        let para = l.para()?;
        let top = lines.iter().filter(|m| m.shows(para)).map(|m| m.top).fold(f64::MAX, f64::min);
        Some((para, top))
    }
}

impl Editor {
    fn checkbox_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        let stack = self.stack();
        let (page, px, py) = stack.page_at(x, y)?;
        self.ensure_page(page, fonts);
        let geo = self.pages.page(page, &self.index)?;
        let ctx = self.editing();
        for l in geo.lines.iter().filter(|l| py >= l.top && py < l.bottom && reachable(ctx, geo.stories[l.story].host)) {
            for w in l.stops.windows(2) {
                if w[1].para == w[0].para && w[1].unit.0 == w[0].unit.0 + 1 && px >= w[0].x && px < w[1].x {
                    let p = self.index.para(&self.doc, w[0].para)?;
                    if let Some(Inline::Sym { control: Some(c), .. }) = inline_at(p, w[0].unit)
                        && matches!(self.doc.controls.get(c).map(|c| &c.props.kind), Some(crate::docx::ControlKind::Checkbox { .. }))
                    {
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
        let Some(Inline::Sym { control: Some(c), .. }) = self.index.para(&self.doc, para).and_then(|q| inline_at(q, u)) else { return None };
        self.doc.controls.get(c).is_some_and(|k| k.props.lock.editable()).then_some(p)
    }

    pub(super) fn checkbox_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(p) = self.checkbox_toggles_at(x, y, fonts) else { return false };
        self.drag = None;
        self.toggle_checkbox(p, fonts);
        true
    }

    pub(super) fn selected_checkbox(&self) -> Option<Position> {
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
        let Some(Inline::Sym { ch, font, run, control: Some(c) }) = self.index.para(&self.doc, para).and_then(|q| inline_at(q, u)).cloned() else {
            return Outcome::Unchanged("no checkbox");
        };
        let Some(crate::docx::ControlKind::Checkbox { checked, unchecked }) = self.doc.controls.get(&c).map(|c| c.props.kind.clone()) else {
            return Outcome::Unchanged("no checkbox");
        };
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

    fn page_frame(&self, page: usize) -> Option<([f64; 2], [f64; 4])> {
        let snap = self.snapshot();
        let p = snap.pages().get(page)?;
        let s = self.doc.sections.get(snap.info().get(page).map_or(0, |i| i.section))?;
        let left = s.left + if self.doc.settings.gutter_at_top { 0.0 } else { s.gutter };
        Some(([p.w, p.h], [left, s.top, p.w - s.right, p.h - s.bottom]))
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
                Item::Shape(s) if s.float == f && s.path.is_empty() => Some(super::snap::turned_bounds(s.rect, s.rot)),
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
                Item::Shape(s) if s.float == 0 && s.path.is_empty() => objects.push(super::snap::turned_bounds(s.rect, s.rot)),
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
                    let [x0, y0, x1, y1] = d.rect(false);
                    let [ml, _, mr, _] = self.snaps.margins;
                    let anchor_link = anchor.filter(|(p, _)| *p == d.page).and_then(|(_, [ax, ay])| {
                        let (rx, ry) = ((ax - 18.0 * one).max(10.0 * one) + 6.0 * one, ay + 8.0 * one);
                        (x0 > rx).then_some([Pt(rx), Pt(ry), Pt(x0), Pt(ry)])
                    });
                    guides.push((d.page, Guides { lines: lines(&self.snaps, d.snapped), grid: self.snaps.grid, anchor_link }));
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
                                    .push((d.page, Measure::Dimension { from: [Pt(a), cy], to: [Pt(b), cy], label: amount(b - a, set.unit), tone }));
                            }
                        };
                        dim(ml, x0);
                        dim(x1, mr);
                    }
                }
                Some(_) => {
                    let [x0, y0, x1, y1] = d.rect(self.object_locked(d.obj));
                    original = Some(Outline { page: d.page, rect: d.rect, rot: 0.0 });
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
