//! Object selection: what a click selects among pictures and text boxes, and what typing and deleting do to a
//! selected object.

use crate::docx::Inline;
use crate::font::Fonts;
use crate::layout::{FloatId, Unit};

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::position::{Position, StoryId};
use super::selection::{Granularity, ObjectRef, Selection};
use super::step::Step;
use super::stories::{inline_at, reachable};

impl Editor {
    /// The object a press selects, in paint order, among those of the story being edited: a floating picture anywhere
    /// on it, a text box on its border band (the border to 4 px inside; on the left edge beside a line, that line
    /// instead), an inline picture on its box. A float behind the text takes only presses where no line is over it.
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
            if !reachable(ctx, s.host) || px < x0 - one || px > x1 + one || py < y0 - one || py > y1 + one {
                continue;
            }
            // Behind the text, a line over it takes the press.
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
            if self.index.text_box(&self.doc, f).is_none() {
                return Some(Selection::Object(ObjectRef::Floating(f)));
            }
            let left = px - x0 < band;
            if !(left || x1 - px < band || py - y0 < band || y1 - py < band) {
                // Inside a text box: its text.
                return None;
            }
            if left
                && let Some(i) = geo.lines.iter().position(|l| l.story == si && py >= l.top && py < l.bottom)
                && let Some((a, b)) = self.line_span_of(&geo, i)
            {
                return Some(Selection::range(a, b, Granularity::Line));
            }
            return Some(Selection::Object(ObjectRef::Floating(f)));
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

    /// Before typing over a selected object: the caret goes before an inline picture (keeping it), or to a float's
    /// anchor paragraph's start (keeping the float).
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

    /// Delete or Backspace with an object selected: the object goes. An inline picture is deleted as the one
    /// unit it is; a float is taken out of its anchor paragraph.
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

/// A pointer drag on an object: moving a float, or resizing a float or an inline picture by one of its eight handles.
#[derive(Clone, Copy, Debug)]
pub(super) struct ObjectDrag {
    pub obj: ObjectRef,
    /// `None`: a move. `Some(k)`: handle `k`, clockwise from the top-left corner (0 top-left, 1 top, 2 top-right, 3
    /// right, 4 bottom-right, 5 bottom, 6 bottom-left, 7 left).
    pub handle: Option<usize>,
    /// Where the press was and where the pointer is, in document points.
    pub start: (f64, f64),
    pub now: (f64, f64),
    /// The float's box when the press was (page coordinates) and its page.
    pub rect: [f64; 4],
    pub page: usize,
    /// Ctrl: resize about the centre.
    pub ctrl: bool,
}

/// Picture Format commands.
#[derive(Clone, Debug, PartialEq)]
pub enum ObjectCmd {
    /// `square`, `tight`, `through`, `topAndBottom`, `behind` or `front` (a float only).
    Wrap(String),
    /// The anchor locked or unlocked (a float only).
    LockAnchor(bool),
    /// Width and height, in points.
    Size(f64, f64),
    /// The picture's own size (its pixels at its resolution).
    ResetSize,
    /// Alternative text.
    AltText(String),
}

/// The checkbox a click or Space toggled, and the caret it left after it: Space there toggles it again.
#[derive(Clone, Copy, Debug)]
pub(super) struct ToggledCheckbox {
    pub face: Position,
    pub after: Position,
}

/// Handle `k`'s side: -1 left or top, 1 right or bottom, 0 neither.
const HANDLES: [(f64, f64); 8] = [(-1.0, -1.0), (0.0, -1.0), (1.0, -1.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0), (-1.0, 1.0), (-1.0, 0.0)];

impl ObjectDrag {
    /// The box the drag shows now: moved by the pointer's travel, or resized (side handles stretch one
    /// dimension; a locked corner keeps the ratio, following the axis with the larger relative change), at least 1 pt.
    pub fn rect(&self, locked: bool) -> [f64; 4] {
        let (dx, dy) = (self.now.0 - self.start.0, self.now.1 - self.start.1);
        let [x0, y0, x1, y1] = self.rect;
        let Some(k) = self.handle else { return [x0 + dx, y0 + dy, x1 + dx, y1 + dy] };
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
    /// The resize handle of the selected object under a point (within 4 px): the object, its box (page
    /// coordinates), its page and the handle, clockwise from the top-left corner. Objects that can't change have none.
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
        // A handle's place along one axis: the box's low edge, its high edge or its middle.
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

    /// A press on a float's handle (while it is selected) or on an object; true when
    /// handled.
    pub(super) fn object_press(&mut self, x: f64, y: f64, ctrl: bool, fonts: &Fonts) -> bool {
        if let Some((obj, rect, page, k)) = self.handle_at(x, y, fonts) {
            self.object_drag = Some(ObjectDrag { obj, handle: Some(k), start: (x, y), now: (x, y), rect, page, ctrl });
            self.requests.push(Request::CapturePointer);
            return true;
        }
        let stack = self.stack();
        let Some((page, ..)) = stack.page_at(x, y) else { return false };
        self.ensure_page(page, fonts);
        let Some(geo) = self.pages.page(page, &self.index) else { return false };
        let Some(s) = self.object_hit(x, y, fonts) else { return false };
        self.set_selection(s);
        self.drag = None;
        self.activate_placeholder();
        if let Selection::Object(obj @ ObjectRef::Floating(f)) = s
            && !self.object_fixed(obj)
            && let Some(st) = geo.stories.iter().find(|t| t.float == Some(f))
        {
            self.object_drag = Some(ObjectDrag { obj, handle: None, start: (x, y), now: (x, y), rect: st.rect, page, ctrl });
            self.requests.push(Request::CapturePointer);
        }
        true
    }

    /// The release of an object drag: one step, or none when the pointer didn't move.
    pub(super) fn object_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.object_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        if d.now == d.start {
            return Some(Outcome::Unchanged("no drag"));
        }
        let label = if d.handle.is_some() { "Resize Object" } else { "Move Object" };
        Some(match d.obj {
            ObjectRef::Floating(f) => self.place_float(f, d.page, d.rect, d.rect(self.float_locked(f)), label, d.handle.is_none(), fonts),
            ObjectRef::Inline { para, offset } => {
                let [x0, y0, x1, y1] = d.rect(self.object_locked(d.obj));
                self.set_picture_size(para, offset, x1 - x0, y1 - y0, fonts)
            }
        })
    }

    /// A picture that can't be resized or moved ([`crate::docx::Document::fixed`]): no handles, no drag, no nudge, no
    /// size change.
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

    /// Whether corner handles keep the object's ratio: a picture whose aspect ratio is locked; text boxes and
    /// unlocked pictures resize freely.
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

    /// A float drawn behind the text (`behindDoc`).
    fn float_behind(&self, f: FloatId) -> bool {
        self.index
            .float_anchor(f)
            .and_then(|a| self.index.para(&self.doc, a))
            .is_some_and(|p| p.inlines.iter().any(|i| matches!(i, Inline::Float(fl, _) if FloatId(fl.id) == f && fl.behind)))
    }

    /// An inline picture's box on a page: the rectangle layout drew it in.
    pub(super) fn inline_box(&self, geo: &super::geometry::PageGeometry, para: crate::layout::ParaId, offset: Unit) -> Option<[f64; 4]> {
        geo.lines.iter().find_map(|l| l.pictures.iter().find(|(p, u, _)| *p == para && *u == offset).map(|(_, _, r)| *r))
    }

    /// An inline picture at `offset` of `para` resized to `w` × `h` (at least 1 pt), one step.
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
        // Whole EMUs' worth of points is finer than anything shown; whole twips keep saving and reading back equal.
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

    /// Alt Text: the selected picture's alternative text, one step.
    fn set_alt_text(&mut self, o: ObjectRef, text: &str, fonts: &Fonts) -> Outcome {
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
                Inline::Picture(pic, _) if Some(Unit(here)) == unit && &*pic.alt != text => {
                    pic.alt = text.into();
                    done = true;
                }
                Inline::Float(fl, _) if unit.is_none() && ObjectRef::Floating(FloatId(fl.id)) == o && &*fl.alt != text => {
                    fl.alt = text.into();
                    done = true;
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

    /// Insert Picture: the image at the caret as an inline picture at its own size, scaled down to the text
    /// width when wider, and selected. One step; the image data stays with the document.
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
            |run| Inline::Picture(crate::docx::Picture { w, h, image, alt: "".into(), lock_aspect: true }, run),
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

    /// Insert Text Box: a 2 in × 1 in text box anchored at the caret's paragraph, in front of the text, its
    /// left at the caret's column and its top at the paragraph's; the caret goes into it. One step.
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
        // The box's paragraph: Normal, empty.
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
            behind: false,
            allow_overlap: true,
            layout_in_cell: true,
            // Above every float read (paint order).
            order: usize::MAX / 2,
            src: None,
            locked: false,
            frame_src: None,
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

    /// Ctrl+Alt+5 (`None`): the first floating object in the story being edited; Tab / Shift+Tab (`Some(back)`): the
    /// next or previous one, round. Esc then returns to the text (the float's anchor).
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

    /// Picture Format commands on the selected object: wrapping, the anchor lock, size, and the picture's own
    /// size back.
    pub(super) fn object_cmd(&mut self, c: ObjectCmd, fonts: &Fonts) -> Outcome {
        let Selection::Object(o) = self.selection else { return Outcome::Unchanged("no object selected") };
        if let ObjectCmd::AltText(text) = &c {
            return self.set_alt_text(o, text, fonts);
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
                    // shortcut: wrapping an inline picture would make it a float; not done.
                    ObjectCmd::Wrap(_) | ObjectCmd::LockAnchor(_) => return Outcome::Unchanged("inline picture"),
                    ObjectCmd::AltText(_) => return Outcome::Unchanged("handled above"),
                };
                self.set_picture_size(para, offset, w, h, fonts)
            }
            ObjectRef::Floating(f) => {
                let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
                let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
                let mut inlines = p.inlines.clone();
                let Some(fl) = inlines.iter_mut().find_map(|i| match i {
                    Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
                    _ => None,
                }) else {
                    return Outcome::Unchanged("no float");
                };
                let mut n = (**fl).clone();
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
                    ObjectCmd::AltText(_) => return Outcome::Unchanged("handled above"),
                };
                (n.w, n.h) = ((n.w * 20.0).round() / 20.0, (n.h * 20.0).round() / 20.0);
                if n == **fl {
                    return Outcome::Unchanged("already so");
                }
                **fl = n;
                let mut tx = self.begin(label);
                if self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_err() {
                    self.abort(tx);
                    return Outcome::Unchanged("stale object");
                }
                self.selection = Selection::Object(o);
                self.commit(tx, None, fonts)
            }
        }
    }

    /// An arrow key with a float selected moves it 0.75 pt, plain or with Ctrl. One step per press.
    pub(super) fn nudge(&mut self, f: FloatId, dx: f64, dy: f64, fonts: &Fonts) -> Outcome {
        let n = self.pages.len();
        let near = self.near;
        let found = (0..n).map(|d| if d % 2 == 0 { near + d / 2 } else { near.wrapping_sub(d / 2 + 1) }).filter(|&p| p < n).find_map(|p| {
            self.ensure_page(p, fonts);
            let geo = self.pages.page(p, &self.index)?;
            geo.stories.iter().find(|s| s.float == Some(f)).map(|s| (p, s.rect))
        });
        let Some((page, r)) = found else { return Outcome::Unchanged("float not drawn") };
        self.place_float(f, page, r, [r[0] + dx, r[1] + dy, r[2] + dx, r[3] + dy], "Move Object", false, fonts)
    }

    /// A float moved or resized from box `old` to `new` (page coordinates on `page`): its offsets keep their reference
    /// (page, margin, column or paragraph), an aligned position becoming an offset.
    fn place_float(&mut self, f: FloatId, page: usize, old: [f64; 4], new: [f64; 4], label: &'static str, reanchor: bool, fonts: &Fonts) -> Outcome {
        let Some(anchor) = self.index.float_anchor(f) else { return Outcome::Unchanged("no anchor") };
        let Some(p) = self.index.para(&self.doc, anchor) else { return Outcome::Unchanged("no anchor") };
        let section = self.snapshot().info().get(page).map_or(0, |i| i.section);
        let sec = self.doc.sections.get(section).cloned();
        let anchor_top = self.pages.page(page, &self.index).and_then(|g| g.lines_of(anchor).map(|(_, l)| l.top).reduce(f64::min));
        let mut inlines = p.inlines.clone();
        let Some(fl) = inlines.iter_mut().find_map(|i| match i {
            Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
            _ => None,
        }) else {
            return Outcome::Unchanged("no float");
        };
        // The reference's origin: from the offset when there is one, else from the reference's name.
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
        // Offsets in whole twips, as the reader truncates them (FLOAT-6), so saving and reading back keeps them.
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
        // A float dragged to a new place (not nudged) that isn't locked re-anchors to the paragraph at its new
        // top (in its anchor's container), its paragraph-relative offset measured from there.
        let to = if reanchor && !fl.locked { self.anchor_at(page, anchor, new[1]) } else { None }.filter(|&(p, _)| p != anchor);
        if let Some((_, top)) = to
            && matches!(fl.v_relative.as_str(), "paragraph" | "line")
        {
            fl.v_offset = twips(new[1] - top);
            fl.v_align = None;
        }
        let mut tx = self.begin(label);
        let ok = match to {
            None => self.step(&mut tx, Step::Restyle { para: anchor, inlines }).is_ok(),
            Some((dest, _)) => {
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

    /// The paragraph a float whose top is at `y` on `page` anchors to: the last line starting at or above `y` (else
    /// the first below) among the lines in the same container as `anchor`; with that paragraph's top on the page.
    fn anchor_at(&mut self, page: usize, anchor: crate::layout::ParaId, y: f64) -> Option<(crate::layout::ParaId, f64)> {
        let container = self.index.para_slot(anchor)?.container;
        let geo = self.pages.page(page, &self.index)?;
        let same = |p: crate::layout::ParaId| self.index.para_slot(p).is_some_and(|s| s.container == container);
        let lines: Vec<&super::geometry::GLine> = geo.lines.iter().filter(|l| !l.repeat && l.para().is_some_and(same)).collect();
        let l = lines
            .iter()
            .filter(|l| l.top <= y + 1e-6)
            .max_by(|a, b| a.top.total_cmp(&b.top))
            .or_else(|| lines.iter().min_by(|a, b| a.top.total_cmp(&b.top)))?;
        // A line showing several paragraphs (REV-6) anchors to the one it starts with.
        let para = l.para()?;
        let top = lines.iter().filter(|m| m.shows(para)).map(|m| m.top).fold(f64::MAX, f64::min);
        Some((para, top))
    }
}

impl Editor {
    /// A checkbox content control's face under a point: its paragraph and unit.
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

    /// The face of the checkbox a click at a point toggles: none in design mode, read only, or when the control's
    /// content is locked (the click is then a click on text).
    pub(super) fn checkbox_toggles_at(&mut self, x: f64, y: f64, fonts: &Fonts) -> Option<Position> {
        if matches!(self.mode, super::Mode::Design | super::Mode::ReadOnly) {
            return None;
        }
        let p = self.checkbox_at(x, y, fonts)?;
        let (para, u) = p.para_offset()?;
        let Some(Inline::Sym { control: Some(c), .. }) = self.index.para(&self.doc, para).and_then(|q| inline_at(q, u)) else { return None };
        self.doc.controls.get(c).is_some_and(|k| k.props.lock.editable()).then_some(p)
    }

    /// A click on a checkbox toggles it, the caret after it; true when it did.
    pub(super) fn checkbox_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        let Some(p) = self.checkbox_toggles_at(x, y, fonts) else { return false };
        self.drag = None;
        self.toggle_checkbox(p, fonts);
        true
    }

    /// The checkbox Space toggles: the one selected, or the one a click (or Space) just toggled while the caret is
    /// still where the toggle left it.
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

    /// Toggles the checkbox whose face is at `p`, the caret after it.
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
