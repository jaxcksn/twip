//! Shape editing

use crate::layout::FloatId;
use crate::layout::fragment::{Item, ShapeItem};

use super::Editor;
use super::selection::ObjectRef;
use super::snap::SnapLine;
use super::view::ShapeFrame;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum ShapeHandle {
    Rotate,
    Adjust(usize),
    Move,
}

fn turned(p: [f64; 2], c: [f64; 2], deg: f64) -> [f64; 2] {
    let (s, k) = deg.to_radians().sin_cos();
    let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
    [c[0] + dx * k - dy * s, c[1] + dx * s + dy * k]
}

impl Editor {
    fn selected_shape(&self, f: FloatId, page: usize) -> Option<std::rc::Rc<ShapeItem>> {
        let child = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        let snap = self.snapshot();
        let items = crate::layout::traverse::paint_float(snap.pages().get(page)?, f);
        items.into_iter().find_map(|it| match it {
            Item::Shape(s) if s.path == child => Some(s),
            _ => None,
        })
    }

    pub(super) fn shape_frames(&mut self, pages: std::ops::Range<usize>) -> Vec<ShapeFrame> {
        let floats = self.selected_floats();
        let [f] = floats[..] else { return Vec::new() };
        if self.object_fixed(ObjectRef::Floating(f))
            || self.object_drag.is_some_and(|d| d.now != d.start)
            || self.shape_drag.is_some()
            || self.float_of(f).is_none_or(|fl| fl.shape.is_none())
        {
            return Vec::new();
        }
        let up = KNOB * 0.75 / self.viewport.zoom.max(0.1);
        let mut out = Vec::new();
        for page in pages {
            let (rect, rot, adjust) = match self.selected_shape(f, page) {
                Some(s) => (s.rect, s.rot, s.geom.handles.iter().map(|h| s.to_page(h.at)).collect()),
                None if self.selected_child().is_none() => {
                    let Some(geo) = self.pages.page(page, &self.index) else { continue };
                    let Some(st) = geo.stories.iter().find(|s| s.float == Some(f)) else { continue };
                    (st.rect, 0.0, Vec::new())
                }
                None => continue,
            };
            let [x0, y0, x1, y1] = rect;
            let c = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
            let (top, rotate) = (turned([c[0], y0], c, rot), turned([c[0], y0 - up], c, rot));
            out.push(ShapeFrame { page, rect, rot, rotate, top, adjust });
        }
        out
    }

    pub(super) fn shape_handle_at(&mut self, x: f64, y: f64) -> Option<(FloatId, ShapeHandle, ShapeFrame)> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let f = *self.selected_floats().first()?;
        let reach = 4.0 * 0.75 / self.viewport.zoom.max(0.1);
        let near = |p: [f64; 2]| (p[0] - px).abs() <= reach && (p[1] - py).abs() <= reach;
        let frame = self.shape_frames(page..page + 1).into_iter().next()?;
        let h = match frame.adjust.iter().position(|p| near(*p)) {
            Some(i) => ShapeHandle::Adjust(i),
            None if near(frame.rotate) => ShapeHandle::Rotate,
            None => return None,
        };
        Some((f, h, frame))
    }
}

const KNOB: f64 = 24.0;

#[derive(Clone, Debug)]
pub(super) struct ShapeDrag {
    f: FloatId,
    root: [f64; 4],
    child: crate::drawing::ChildPath,
    pub handle: ShapeHandle,
    pub item: std::rc::Rc<ShapeItem>,
    start: [f64; 2],
    pub now: [f64; 2],
    pub page: usize,
    shift: bool,
    previewed: Option<crate::layout::ParaId>,
    pub snapped: [Option<SnapLine>; 2],
}

impl ShapeDrag {
    fn new(
        f: FloatId,
        root: [f64; 4],
        child: crate::drawing::ChildPath,
        handle: ShapeHandle,
        item: std::rc::Rc<ShapeItem>,
        at: [f64; 2],
        page: usize,
    ) -> ShapeDrag {
        ShapeDrag { f, root, child, handle, item, start: at, now: at, page, shift: false, previewed: None, snapped: [None; 2] }
    }

    pub fn angle(&self) -> f64 {
        let [x0, y0, x1, y1] = self.item.rect;
        let c = [(x0 + x1) / 2.0, (y0 + y1) / 2.0];
        let angle = |p: [f64; 2]| (p[1] - c[1]).atan2(p[0] - c[0]).to_degrees();
        let total = self.item.rot + angle(self.now) - angle(self.start);
        if self.shift {
            return (total / SNAP).round() * SNAP;
        }
        let right = (total / 90.0).round() * 90.0;
        if (total - right).abs() < SETTLE { right } else { total }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ShapeInsert {
    kind: crate::drawing::NewShape,
    start: Option<(usize, [f64; 2])>,
    now: [f64; 2],
    shift: bool,
}

fn rotate_to(n: &mut crate::docx::Float, child: &[u16], deg: f64, compat: i64) -> bool {
    let deg = deg.rem_euclid(360.0);
    let Some(s) = n.shape.as_deref_mut().and_then(|s| s.at_mut(child)) else { return false };
    s.xfrm.rot = crate::drawing::units::Angle((deg * 60000.0).round() as i32 % crate::drawing::units::Angle::FULL);
    if child.is_empty() {
        let quarter = (45.0..135.0).contains(&deg.rem_euclid(180.0)) && compat != 12;
        if quarter != n.rot_swap {
            let (w0, h0) = (n.w, n.h);
            (n.w, n.h) = (h0, w0);
            if n.h_align.is_none() {
                n.h_offset += (w0 - h0) / 2.0;
            }
            if n.v_align.is_none() {
                n.v_offset += (h0 - w0) / 2.0;
            }
            n.rot_swap = quarter;
        }
    }
    true
}

fn move_child(n: &mut crate::docx::Float, path: &[u16], root: [f64; 4], by: [f64; 2]) -> Option<()> {
    let (_, parent) = path.split_last()?;
    let tree = n.shape.as_deref()?;
    let (prect, rot, fh, fv) = tree.frame_at(root, parent)?;
    let [sx, sy] = tree.at(parent)?.group()?.scale(prect);
    let unturn = |deg: f64| {
        let (sin, cos) = (-deg).to_radians().sin_cos();
        (by[0] * cos - by[1] * sin, by[0] * sin + by[1] * cos)
    };
    let (mut ux, mut uy) = unturn(rot);
    if fh {
        ux = -ux;
    }
    if fv {
        uy = -uy;
    }
    let (rx, ry) = unturn(tree.xfrm.rot.normalized().degrees());
    let c = n.shape.as_deref_mut()?.at_mut(path)?;
    c.xfrm.off[0].0 += (ux / sx * 12700.0).round() as i64;
    c.xfrm.off[1].0 += (uy / sy * 12700.0).round() as i64;
    for p in n.parts.iter_mut().filter(|p| p.path.starts_with(path)) {
        p.rect = [p.rect[0] + rx, p.rect[1] + ry, p.rect[2] + rx, p.rect[3] + ry];
    }
    for k in (0..path.len()).rev() {
        let s = n.shape.as_deref_mut()?.at_mut(&path[..k])?;
        let crate::drawing::ShapeKind::Group(g) = &mut s.kind else { return None };
        let off = s.xfrm.off;
        let grow = g.fit_children(&mut s.xfrm);
        if k == 0 {
            s.xfrm.off = off;
            let pt = |e: i64| e as f64 / 12700.0;
            let (l, t) = (pt(grow[0]), pt(grow[1]));
            let (gw, gh) = (l + pt(grow[2]), t + pt(grow[3]));
            if n.rot_swap {
                (n.w, n.h) = (n.w + gh, n.h + gw);
            } else {
                (n.w, n.h) = (n.w + gw, n.h + gh);
            }
            if n.h_align.is_none() {
                n.h_offset -= l;
            }
            if n.v_align.is_none() {
                n.v_offset -= t;
            }
            for p in &mut n.parts {
                p.rect = [p.rect[0] + l, p.rect[1] + t, p.rect[2] + l, p.rect[3] + t];
            }
        }
    }
    Some(())
}

pub use crate::api::drawing::Arrange;

const SNAP: f64 = 15.0;
const SETTLE: f64 = 4.0;

impl Editor {
    pub(super) fn shape_press(&mut self, x: f64, y: f64) -> bool {
        let Some((f, handle, frame)) = self.shape_handle_at(x, y) else { return false };
        let Some((_, px, py)) = self.stack().page_at(x, y) else { return false };
        let child = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        let Some(item) = self.selected_shape(f, frame.page).or_else(|| self.group_item(f, frame.page)) else { return false };
        let Some(root) = self.shape_root(f, frame.page) else { return false };
        self.shape_drag = Some(ShapeDrag::new(f, root, child, handle, item, [px, py], frame.page));
        self.requests.push(super::input::Request::CapturePointer);
        true
    }

    pub(super) fn child_move_press(&mut self, f: FloatId, page: usize, px: f64, py: f64) -> bool {
        let Some(child) = self.selected_child().map(<[u16]>::to_vec) else { return false };
        let (Some(item), Some(root)) = (self.selected_shape(f, page), self.shape_root(f, page)) else { return false };
        self.shape_drag = Some(ShapeDrag::new(f, root, child, ShapeHandle::Move, item, [px, py], page));
        self.build_snaps(page, super::objects::Dragged::Float(f));
        self.requests.push(super::input::Request::CapturePointer);
        true
    }

    pub(super) fn shape_root(&mut self, f: FloatId, page: usize) -> Option<[f64; 4]> {
        let swap = self.float_of(f)?.rot_swap;
        let geo = self.pages.page(page, &self.index)?;
        let [x0, y0, x1, y1] = geo.stories.iter().find(|s| s.float == Some(f))?.rect;
        Some(if swap {
            let (cx, cy, hw, hh) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0, (y1 - y0) / 2.0, (x1 - x0) / 2.0);
            [cx - hw, cy - hh, cx + hw, cy + hh]
        } else {
            [x0, y0, x1, y1]
        })
    }

    fn group_item(&mut self, f: FloatId, page: usize) -> Option<std::rc::Rc<ShapeItem>> {
        let geo = self.pages.page(page, &self.index)?;
        let rect = geo.stories.iter().find(|s| s.float == Some(f))?.rect;
        let rot = self.float_of(f)?.shape.as_ref()?.xfrm.rot.degrees();
        let snap = self.snapshot();
        let any = crate::layout::traverse::paint_float(snap.pages().get(page)?, f).into_iter().find_map(|it| match it {
            Item::Shape(s) => Some(s),
            _ => None,
        })?;
        let mut s = any.moved(0.0, 0.0);
        (s.rect, s.rot, s.flip_h, s.flip_v) = (rect, rot, false, false);
        Some(std::rc::Rc::new(s))
    }

    fn dragged_float(&self, d: &ShapeDrag) -> Option<crate::docx::Float> {
        let mut n = self.float_of(d.f)?.clone();
        let compat = self.doc.settings.compat_mode;
        let s = n.shape.as_deref_mut()?.at_mut(&d.child)?;
        match d.handle {
            ShapeHandle::Rotate => {
                let own = s.xfrm.rot.degrees() + d.angle() - d.item.rot;
                rotate_to(&mut n, &d.child, own, compat);
            }
            ShapeHandle::Adjust(i) => {
                let [x0, y0, x1, y1] = d.item.rect;
                let emu = |v: f64| crate::drawing::units::Emu((v * 12700.0).round() as i64);
                let to = d.item.to_local(d.now);
                let sp = s.sp_mut()?;
                sp.geom = sp.geom.drag_handle(i, emu(x1 - x0), emu(y1 - y0), to)?;
            }
            ShapeHandle::Move => move_child(&mut n, &d.child, d.root, [d.now[0] - d.start[0], d.now[1] - d.start[1]])?,
        }
        Some(n)
    }

    pub(super) fn shape_move(&mut self, x: f64, y: f64, mods: super::keymap::Mods) {
        let Some(mut d) = self.shape_drag.take() else { return };
        let (ox, oy) = self.stack().to_doc(d.page, 0.0, 0.0).unwrap_or((0.0, 0.0));
        (d.now, d.shift, d.snapped) = ([x - ox, y - oy], mods.shift, [None; 2]);
        if d.handle == ShapeHandle::Move && !mods.alt {
            let [x0, y0, x1, y1] = super::snap::turned_bounds(d.item.rect, d.item.rot);
            let (dx, dy) = (d.now[0] - d.start[0], d.now[1] - d.start[1]);
            let (off, hit) = super::snap::snap([x0 + dx, y0 + dy, x1 + dx, y1 + dy], &self.snaps, self.snap_threshold());
            (d.now, d.snapped) = ([d.now[0] + off[0], d.now[1] + off[1]], hit);
        }
        let preview = self.dragged_float(&d).and_then(|float| Some(crate::layout::Preview { float, anchor: self.index.float_anchor(d.f)? }));
        let at = preview.as_ref().map(|p| p.anchor);
        self.preview_layout(d.f, preview, d.previewed);
        d.previewed = at;
        self.shape_drag = Some(d);
    }

    pub(super) fn shape_release(&mut self, fonts: &crate::font::Fonts) -> Option<super::commands::Outcome> {
        let d = self.shape_drag.take()?;
        self.requests.push(super::input::Request::ReleasePointer);
        let n = (d.now != d.start).then(|| self.dragged_float(&d)).flatten();
        self.preview_layout(d.f, None, d.previewed);
        let child = self.object_child.clone();
        let out = match n {
            Some(n) => self.set_float(
                n,
                match d.handle {
                    ShapeHandle::Rotate => "Rotate",
                    ShapeHandle::Adjust(_) => "Adjust Shape",
                    ShapeHandle::Move => "Move Object",
                },
                fonts,
            ),
            None => super::commands::Outcome::Unchanged("no drag"),
        };
        self.object_child = child;
        Some(out)
    }

    pub(super) fn shape_story_for_typing(&mut self, fonts: &crate::font::Fonts) -> bool {
        use crate::drawing::{ShapeKind, ShapeText};
        let floats = self.selected_floats();
        let (&[f], None) = (&floats[..], self.selected_child()) else { return false };
        let Some(mut n) = self.float_of(f).cloned() else { return false };
        if n.text_box.is_some() {
            return false;
        }
        let Some(shape) = n.shape.as_deref_mut() else { return false };
        let (w, h) = (shape.xfrm.ext[0], shape.xfrm.ext[1]);
        let ShapeKind::Sp(sp) = &mut shape.kind else { return false };
        let [l, t, r, b] = sp.geom.evaluate(w, h).text_rect;
        if r - l <= 0.0 || b - t <= 0.0 {
            return false;
        }
        let Some(res) = self.doc.resolver.clone() else { return false };
        let cx = self.cx();
        let style = res.para_from_xml(Some(r#"<w:pPr><w:jc w:val="center"/></w:pPr>"#), None, &cx);
        let mark = res.run_from_xml(&style.base, None, &cx);
        let pid = crate::docx::next_para_id();
        let para = crate::docx::Paragraph {
            style: std::rc::Rc::new(style.style),
            mark,
            list: None,
            inlines: Vec::new(),
            src: None,
            id: pid,
            div: None,
            controls: Vec::new(),
        };
        sp.text = ShapeText::Story;
        let pt = |e: crate::drawing::units::Emu| e.0 as f64 / 12700.0;
        let ins = sp.body.insets;
        n.text_box = Some(crate::docx::TextBox {
            blocks: vec![crate::docx::Block::Para(para)],
            insets: [pt(ins[0]), pt(ins[1]), pt(ins[2]), pt(ins[3])],
            outline: 0.0,
            auto_fit: matches!(sp.body.autofit, crate::drawing::text::AutoFit::Shape),
            v_anchor: match sp.body.anchor {
                crate::drawing::text::VAnchor::Centre => 1,
                crate::drawing::text::VAnchor::Bottom => 2,
                _ => 0,
            },
            flow: 0,
        });
        if !matches!(self.set_float(n, "Add Text", fonts), super::commands::Outcome::Changed(_)) {
            return false;
        }
        let at = super::position::Position::text(super::position::StoryId::TextBox(f), crate::layout::ParaId(pid), crate::layout::Unit(0));
        self.set_selection(super::selection::Selection::Caret(at));
        true
    }

    pub(super) fn insert_shape(
        &mut self,
        kind: crate::drawing::NewShape,
        at: Option<(usize, [f64; 4])>,
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        let line = matches!(kind, crate::drawing::NewShape::Line(_));
        self.insert_shape_in(kind, at, [72.0, if line { 0.0 } else { 72.0 }], fonts)
    }

    pub(super) fn insert_shape_sized(
        &mut self,
        kind: crate::drawing::NewShape,
        size: [f64; 2],
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        self.insert_shape_in(kind, None, size, fonts)
    }

    fn insert_shape_in(
        &mut self,
        kind: crate::drawing::NewShape,
        at: Option<(usize, [f64; 4])>,
        size: [f64; 2],
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        use super::commands::Outcome;
        use crate::drawing::{NewShape, Shape};
        if kind == NewShape::TextBox {
            return Outcome::unavailable("a text box is inserted by Insert Text Box");
        }
        self.object_to_caret();
        let Some(caret) = self.selection.focus() else { return Outcome::Unchanged("no caret") };
        let Some((caret_para, _)) = caret.para_offset() else { return Outcome::Unchanged("caret at a row end") };
        if matches!(caret.story, super::position::StoryId::TextBox(_)) {
            return Outcome::Unchanged("shapes don't go in text boxes");
        }
        let (para, rect) = match at {
            Some((page, r)) => match self.anchor_at(page, caret_para, r[1]) {
                Some((p, top)) => {
                    let section = self.snapshot().info().get(page).map_or(0, |i| i.section);
                    let left = self.doc.sections.get(section).map_or(72.0, |s| s.left);
                    (p, [r[0] - left, r[1] - top, r[2] - r[0], r[3] - r[1]])
                }
                None => return Outcome::Unchanged("no paragraph there"),
            },
            None => (caret_para, [0.0, 0.0, size[0], size[1]]),
        };
        let Some(p) = self.index.para(&self.doc, para) else { return Outcome::Unchanged("no paragraph") };
        let Some(run) = p.inlines.iter().find_map(|i| i.run().cloned()).or_else(|| self.run_at(&caret)) else { return Outcome::Unchanged("no run") };
        let emu = |v: f64| crate::drawing::units::Emu((v * 12700.0).round() as i64);
        let twips = |v: f64| (v * 20.0).trunc() / 20.0;
        let name = match kind {
            NewShape::Shape(_) => "Shape",
            NewShape::Line(_) => "Connector",
            NewShape::TextBox => "Text Box",
        };
        let fid = crate::docx::next_para_id();
        let float = crate::docx::Float {
            id: fid,
            w: rect[2],
            auto_w: false,
            h: rect[3],
            image: None,
            alt: "".into(),
            lock_aspect: false,
            text_box: None,
            h_relative: "column".into(),
            h_align: None,
            h_offset: twips(rect[0]),
            v_relative: "paragraph".into(),
            v_align: None,
            v_offset: twips(rect[1]),
            wrap: "none".into(),
            wrap_text: "bothSides".into(),
            dist: [0.0, 0.0, 9.0, 9.0],
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
            in_note: false,
            parts: Vec::new(),
            shape: Some(Box::new(Shape::new(kind, emu(rect[2]), emu(rect[3]), name))),
            rot_swap: false,
            vml: false,
            relative: Default::default(),
        };
        let mut inlines = p.inlines.clone();
        inlines.insert(0, crate::docx::Inline::Float(Box::new(float), run));
        let mut tx = self.begin("Insert Shape");
        if self.step(&mut tx, super::step::Step::Restyle { para, inlines }).is_err() {
            self.abort(tx);
            return Outcome::Unchanged("stale caret");
        }
        self.selection = super::selection::Selection::Object(ObjectRef::Floating(FloatId(fid)));
        self.commit(tx, None, fonts)
    }

    pub fn begin_shape_insert(&mut self, kind: crate::drawing::NewShape) {
        self.shape_insert = Some(ShapeInsert { kind, start: None, now: [0.0; 2], shift: false });
    }

    pub(super) fn shape_insert_pointer(&mut self, kind: super::input::PointerKind, x: f64, y: f64, shift: bool, fonts: &crate::font::Fonts) -> bool {
        use super::input::{PointerKind as K, Request};
        let Some(mut s) = self.shape_insert else { return false };
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return true };
        match kind {
            K::Down => {
                s.start = Some((page, [px, py]));
                s.now = [px, py];
                self.requests.push(Request::CapturePointer);
            }
            K::Move if s.start.is_some() => (s.now, s.shift) = ([px, py], shift),
            K::Up => {
                let Some((page, a)) = s.start else { return true };
                self.shape_insert = None;
                self.requests.push(Request::ReleasePointer);
                let line = matches!(s.kind, crate::drawing::NewShape::Line(_));
                let (ox, oy) = self.stack().to_doc(page, 0.0, 0.0).unwrap_or((0.0, 0.0));
                let b = [x - ox, y - oy];
                let (mut w, mut h) = (b[0] - a[0], b[1] - a[1]);
                if w.abs() < 2.0 && h.abs() < 2.0 {
                    (w, h) = (72.0, if line { 0.0 } else { 72.0 });
                } else if shift && !line {
                    let m = w.abs().max(h.abs());
                    (w, h) = (m * w.signum(), m * h.signum());
                }
                let r = [a[0].min(a[0] + w), a[1].min(a[1] + h), a[0].max(a[0] + w), a[1].max(a[1] + h)];
                self.insert_shape(s.kind, Some((page, r)), fonts);
                return true;
            }
            K::Cancel => {
                self.shape_insert = None;
                self.requests.push(Request::ReleasePointer);
                return true;
            }
            _ => {}
        }
        self.shape_insert = Some(s);
        true
    }

    pub(super) fn shape_cancel(&mut self) -> bool {
        let Some(d) = self.shape_drag.take() else { return false };
        self.preview_layout(d.f, None, d.previewed);
        self.requests.push(super::input::Request::ReleasePointer);
        true
    }
}

// Formatting operations
impl Editor {
    fn edit_floats(
        &mut self,
        label: &'static str,
        ids: &[FloatId],
        mut edit: impl FnMut(&mut crate::docx::Float, &[u16]) -> bool,
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        use super::commands::Outcome;
        if ids.is_empty() {
            return Outcome::unavailable("no floating object selected");
        }
        let (sel, child, also) = (self.selection, self.object_child.clone(), self.object_also.clone());
        let path = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        let mut tx = self.begin(label);
        let mut any = false;
        for &f in ids {
            let Some(anchor) = self.index.float_anchor(f) else { continue };
            let Some(p) = self.index.para(&self.doc, anchor) else { continue };
            let mut inlines = p.inlines.clone();
            let Some(fl) = inlines.iter_mut().find_map(|i| match i {
                crate::docx::Inline::Float(fl, _) if FloatId(fl.id) == f => Some(fl),
                _ => None,
            }) else {
                continue;
            };
            let mut n = (**fl).clone();
            let here = if ids.len() == 1 { &path[..] } else { &[] };
            if !edit(&mut n, here) || n == **fl {
                continue;
            }
            **fl = n;
            if self.step(&mut tx, super::step::Step::Restyle { para: anchor, inlines }).is_err() {
                self.abort(tx);
                return Outcome::Unchanged("stale object");
            }
            any = true;
        }
        if !any {
            self.abort(tx);
            return Outcome::Unchanged("already so");
        }
        self.selection = sel;
        let out = self.commit(tx, None, fonts);
        (self.object_child, self.object_also) = (child, also);
        out
    }

    fn edit_shapes(
        &mut self,
        label: &'static str,
        mut edit: impl FnMut(&mut crate::drawing::Shape) -> bool,
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        let ids = self.selected_floats();
        self.edit_floats(label, &ids, |n, path| n.shape.as_deref_mut().and_then(|s| s.at_mut(path)).is_some_and(&mut edit), fonts)
    }

    pub fn set_shape_preset(&mut self, preset: crate::drawing::geom::Preset, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        self.edit_shapes(
            "Change Shape",
            |s| s.sp_mut().map(|sp| sp.geom = crate::drawing::geom::Geometry::Preset { preset, adj: Vec::new() }).is_some(),
            fonts,
        )
    }

    pub fn set_shape_adjust(&mut self, index: usize, value: i64, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        self.edit_shapes(
            "Adjust Shape",
            |s| {
                let Some(sp) = s.sp_mut() else { return false };
                let mut vals = sp.geom.adjust_values();
                let Some(v) = vals.get_mut(index) else { return false };
                *v = value;
                sp.geom.with_values(&vals).map(|g| sp.geom = g).is_some()
            },
            fonts,
        )
    }

    pub fn set_shape_fill(&mut self, fill: Option<crate::drawing::paint::Fill>, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use crate::drawing::ShapeKind;
        self.edit_shapes(
            "Shape Fill",
            |s| match &mut s.kind {
                ShapeKind::Sp(sp) => {
                    sp.fill = fill.clone();
                    true
                }
                ShapeKind::Group(g) => {
                    g.fill = fill.clone();
                    true
                }
                _ => false,
            },
            fonts,
        )
    }

    pub fn set_shape_line(&mut self, line: Option<crate::drawing::paint::Line>, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use crate::drawing::ShapeKind;
        self.edit_shapes(
            "Shape Outline",
            |s| match &mut s.kind {
                ShapeKind::Sp(sp) => {
                    sp.line = line.clone();
                    true
                }
                ShapeKind::Pic(p) => {
                    p.line = line.clone();
                    true
                }
                _ => false,
            },
            fonts,
        )
    }

    pub fn set_shape_effects(&mut self, effects: Option<crate::drawing::paint::EffectList>, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use crate::drawing::ShapeKind;
        self.edit_shapes(
            "Shape Effects",
            |s| match &mut s.kind {
                ShapeKind::Sp(sp) => {
                    sp.effects = effects.clone();
                    true
                }
                ShapeKind::Pic(p) => {
                    p.effects = effects.clone();
                    true
                }
                _ => false,
            },
            fonts,
        )
    }

    pub fn set_shape_body(&mut self, body: crate::drawing::text::BodyProps, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use crate::drawing::text::{AutoFit, VAnchor};
        let ids = self.selected_floats();
        self.edit_floats(
            "Text Box Options",
            &ids,
            |n, path| {
                let Some(sp) = n.shape.as_deref_mut().and_then(|s| s.at_mut(path)).and_then(|s| s.sp_mut()) else { return false };
                sp.body = body.clone();
                if let Some(tb) = n.text_box.as_mut().filter(|_| path.is_empty()) {
                    let pt = |e: crate::drawing::units::Emu| e.0 as f64 / 12700.0;
                    tb.insets = body.insets.map(pt);
                    tb.v_anchor = match body.anchor {
                        VAnchor::Centre => 1,
                        VAnchor::Bottom => 2,
                        _ => 0,
                    };
                    tb.auto_fit = matches!(body.autofit, AutoFit::Shape);
                }
                true
            },
            fonts,
        )
    }

    pub fn set_object_rotation(&mut self, deg: f64, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        if !(0.0..=360.0).contains(&deg) {
            return super::commands::Outcome::unavailable("a rotation is 0 to 360 degrees");
        }
        let ids = self.selected_floats();
        let compat = self.doc.settings.compat_mode;
        self.edit_floats("Rotate", &ids, |n, path| rotate_to(n, path, deg, compat), fonts)
    }

    pub fn flip_object(&mut self, horizontal: bool, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        self.edit_shapes(
            "Flip",
            |s| {
                let x = &mut s.xfrm;
                if horizontal {
                    x.flip_h = !x.flip_h
                } else {
                    x.flip_v = !x.flip_v
                }
                true
            },
            fonts,
        )
    }

    pub fn arrange_object(&mut self, how: Arrange, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        let ids = self.selected_floats();
        let Some(&f) = ids.first() else { return super::commands::Outcome::unavailable("no floating object selected") };
        match how {
            Arrange::InFrontOfText | Arrange::BehindText => {
                let behind = how == Arrange::BehindText;
                return self.edit_floats("Arrange", &ids, |n, _| std::mem::replace(&mut n.behind, behind) != behind, fonts);
            }
            _ => {}
        }
        let mut all: Vec<(u32, FloatId)> = Vec::new();
        fn walk(blocks: &[crate::docx::Block], out: &mut Vec<(u32, FloatId)>) {
            for b in blocks {
                match b {
                    crate::docx::Block::Para(p) => out.extend(p.inlines.iter().filter_map(|i| match i {
                        crate::docx::Inline::Float(fl, _) => Some((fl.z, FloatId(fl.id))),
                        _ => None,
                    })),
                    crate::docx::Block::Table(t) => t.rows.iter().flat_map(|r| &r.cells).for_each(|c| walk(&c.blocks, out)),
                }
            }
        }
        self.doc.sections.iter().for_each(|s| walk(&s.blocks, &mut all));
        self.doc.stories.values().for_each(|b| walk(b, &mut all));
        all.sort();
        let Some(i) = all.iter().position(|(_, g)| *g == f) else { return super::commands::Outcome::Unchanged("no float") };
        let z = all[i].0;
        let (max, min) = (all.last().map_or(z, |a| a.0), all.first().map_or(z, |a| a.0));
        let mut set: Vec<(FloatId, u32)> = match how {
            Arrange::ToFront if all[i + 1..].iter().any(|a| a.1 != f) => vec![(f, max.saturating_add(1))],
            Arrange::ToBack if all[..i].iter().any(|a| a.1 != f) => vec![(f, min.saturating_sub(1))],
            Arrange::Forward => all.get(i + 1).map(|&(z2, g)| vec![(f, z2), (g, z)]).unwrap_or_default(),
            Arrange::Backward => i.checked_sub(1).map(|j| vec![(f, all[j].0), (all[j].1, z)]).unwrap_or_default(),
            _ => Vec::new(),
        };
        if let [(_, a), (_, b)] = &mut set[..]
            && a == b
        {
            *a = if how == Arrange::Forward { a.saturating_add(1) } else { a.saturating_sub(1) };
        }
        if set.is_empty() {
            return super::commands::Outcome::Unchanged("already there");
        }
        let floats: Vec<FloatId> = set.iter().map(|s| s.0).collect();
        self.edit_floats(
            "Arrange",
            &floats,
            |n, _| set.iter().find(|s| s.0 == FloatId(n.id)).is_some_and(|s| std::mem::replace(&mut n.z, s.1) != s.1),
            fonts,
        )
    }
}

// Group and ungroup
impl Editor {
    fn replace_floats(
        &mut self,
        label: &'static str,
        ids: &[FloatId],
        anchor: crate::layout::ParaId,
        new: Vec<(crate::docx::Float, [f64; 4])>,
        fonts: &crate::font::Fonts,
    ) -> super::commands::Outcome {
        use super::commands::Outcome;
        use crate::docx::Inline;
        let Some(run) = self.index.para(&self.doc, anchor).and_then(|p| {
            p.inlines.iter().find_map(|i| match i {
                Inline::Float(fl, run) if ids.contains(&FloatId(fl.id)) => Some(run.clone()),
                _ => None,
            })
        }) else {
            return Outcome::Unchanged("no anchor");
        };
        self.begin_batch(Some(label.into()));
        let mut tx = self.begin(label);
        let mut paras: Vec<crate::layout::ParaId> = ids.iter().filter_map(|f| self.index.float_anchor(*f)).collect();
        paras.dedup();
        for (k, para) in paras.iter().enumerate() {
            let Some(p) = self.index.para(&self.doc, *para) else { continue };
            let mut inlines: Vec<Inline> =
                p.inlines.iter().filter(|i| !matches!(i, Inline::Float(fl, _) if ids.contains(&FloatId(fl.id)))).cloned().collect();
            if *para == anchor {
                for (n, (fl, _)) in new.iter().enumerate() {
                    inlines.insert(n, Inline::Float(Box::new(fl.clone()), run.clone()));
                }
            }
            if self.step(&mut tx, super::step::Step::Restyle { para: *para, inlines }).is_err() {
                self.abort(tx);
                self.end_batch(false, fonts);
                return Outcome::Unchanged(if k == 0 { "stale object" } else { "stale anchor" });
            }
        }
        let made: Vec<FloatId> = new.iter().map(|(f, _)| FloatId(f.id)).collect();
        self.selection = super::selection::Selection::Object(ObjectRef::Floating(made[0]));
        self.commit(tx, None, fonts);
        for (fl, to) in &new {
            let f = FloatId(fl.id);
            if let Some((page, now)) = self.float_box(f, fonts) {
                self.place_float(f, page, now, *to, label, false, fonts);
            }
        }
        self.selection = super::selection::Selection::Object(ObjectRef::Floating(made[0]));
        self.object_child = None;
        self.object_also = (made.len() > 1).then(|| (made[0], made[1..].to_vec()));
        self.end_batch(true, fonts)
    }

    pub fn group_objects(&mut self, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use super::commands::Outcome;
        use crate::drawing::units::Emu;
        use crate::drawing::{GroupShape, NonVisual, Shape, ShapeKind, Spans, Xfrm};
        let ids = self.selected_floats();
        if ids.len() < 2 {
            return Outcome::unavailable("select two or more objects to group");
        }
        let mut members = Vec::new();
        for &f in &ids {
            let (Some(m), Some((page, r))) = (self.float_of(f).cloned(), self.float_box(f, fonts)) else { return Outcome::Unchanged("no float") };
            if m.shape.is_none() {
                return Outcome::unsupported("only shapes and pictures with shape properties group");
            }
            members.push((m, page, r));
        }
        if members.iter().any(|(_, p, _)| *p != members[0].1) {
            return Outcome::unavailable("objects on one page group");
        }
        let anchor = match self.index.float_anchor(ids[0]) {
            Some(a) => a,
            None => return Outcome::Unchanged("no anchor"),
        };
        let first = members[0].0.clone();
        members.sort_by_key(|(m, ..)| (m.z, m.order));
        let u = members
            .iter()
            .fold([f64::MAX, f64::MAX, f64::MIN, f64::MIN], |u, (_, _, r)| [u[0].min(r[0]), u[1].min(r[1]), u[2].max(r[2]), u[3].max(r[3])]);
        let emu = |v: f64| Emu((v * 12700.0).round() as i64);
        let (mut children, mut parts) = (Vec::new(), Vec::new());
        for (k, (m, _, r)) in members.iter().enumerate() {
            let mut s = m.shape.as_deref().cloned().expect("checked");
            let (w, h) = if m.rot_swap { (m.h, m.w) } else { (m.w, m.h) };
            let (cx, cy) = ((r[0] + r[2]) / 2.0, (r[1] + r[3]) / 2.0);
            let (bx, by) = (cx - w / 2.0 - u[0], cy - h / 2.0 - u[1]);
            s.xfrm.off = [emu(bx), emu(by)];
            s.xfrm.ext = [emu(w), emu(h)];
            s.nv.id = (crate::docx::next_para_id() & 0x7FFF_FFFF) as u32;
            if let Some(tb) = &m.text_box {
                parts.push(crate::docx::GroupPart { rect: [bx, by, bx + w, by + h], image: None, text_box: Some(tb.clone()), path: vec![k as u16] });
            }
            for p in &m.parts {
                let (dx, dy) = (r[0] - u[0], r[1] - u[1]);
                let mut path = vec![k as u16];
                path.extend(&p.path);
                parts.push(crate::docx::GroupPart { rect: [p.rect[0] + dx, p.rect[1] + dy, p.rect[2] + dx, p.rect[3] + dy], path, ..p.clone() });
            }
            children.push(s);
        }
        let (gw, gh) = (u[2] - u[0], u[3] - u[1]);
        let group = Shape {
            nv: NonVisual { name: "Group".into(), ..NonVisual::default() },
            xfrm: Xfrm { ext: [emu(gw), emu(gh)], ..Xfrm::default() },
            kind: ShapeKind::Group(GroupShape { child_off: [Emu(0), Emu(0)], child_ext: [emu(gw), emu(gh)], fill: None, children, canvas: false }),
            spans: Spans::default(),
        };
        let g = crate::docx::Float {
            id: crate::docx::next_para_id(),
            w: gw,
            h: gh,
            image: None,
            text_box: None,
            parts,
            shape: Some(Box::new(group)),
            rot_swap: false,
            vml: false,
            relative: Default::default(),
            src: None,
            frame_src: None,
            in_note: false,
            z: members.iter().map(|(m, ..)| m.z).max().unwrap_or(first.z),
            order: usize::MAX / 2,
            ..first
        };
        self.replace_floats("Group", &ids, anchor, vec![(g, u)], fonts)
    }

    pub fn ungroup_objects(&mut self, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use super::commands::Outcome;
        use crate::drawing::units::Emu;
        let ids = self.selected_floats();
        let [g] = ids[..] else { return Outcome::unavailable("select one group to ungroup") };
        let Some(m) = self.float_of(g).cloned() else { return Outcome::Unchanged("no float") };
        let Some(crate::drawing::ShapeKind::Group(gs)) = m.shape.as_deref().map(|s| s.kind.clone()) else {
            return Outcome::unavailable("not a group");
        };
        let Some(anchor) = self.index.float_anchor(g) else { return Outcome::Unchanged("no anchor") };
        let Some((_, r)) = self.float_box(g, fonts) else { return Outcome::Unchanged("not laid out") };
        let compat = self.doc.settings.compat_mode;
        let emu = |v: f64| Emu((v * 12700.0).round() as i64);
        let map = |v: i64, axis: usize| {
            let ext = gs.child_ext[axis].0.max(1) as f64;
            let (lo, size) = if axis == 0 { (r[0], r[2] - r[0]) } else { (r[1], r[3] - r[1]) };
            lo + (v - gs.child_off[axis].0) as f64 / ext * size
        };
        let mut new = Vec::new();
        for (k, c) in gs.children.iter().enumerate() {
            let rect = [
                map(c.xfrm.off[0].0, 0),
                map(c.xfrm.off[1].0, 1),
                map(c.xfrm.off[0].0 + c.xfrm.ext[0].0, 0),
                map(c.xfrm.off[1].0 + c.xfrm.ext[1].0, 1),
            ];
            let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
            let mut s = c.clone();
            s.xfrm.off = [Emu(0), Emu(0)];
            s.xfrm.ext = [emu(w), emu(h)];
            let k = k as u16;
            let text_box = m.parts.iter().find(|p| p.path == [k]).and_then(|p| p.text_box.clone());
            let (dx, dy) = (rect[0] - r[0], rect[1] - r[1]);
            let parts = m
                .parts
                .iter()
                .filter(|p| p.path.len() > 1 && p.path[0] == k)
                .map(|p| crate::docx::GroupPart {
                    rect: [p.rect[0] - dx, p.rect[1] - dy, p.rect[2] - dx, p.rect[3] - dy],
                    path: p.path[1..].to_vec(),
                    ..p.clone()
                })
                .collect();
            let deg = s.xfrm.rot.degrees();
            let mut n = crate::docx::Float {
                id: crate::docx::next_para_id(),
                w,
                h,
                image: s.pic().and_then(|p| p.blip.image),
                text_box,
                parts,
                shape: Some(Box::new(s)),
                rot_swap: false,
                vml: false,
                relative: Default::default(),
                src: None,
                frame_src: None,
                in_note: false,
                z: m.z,
                order: usize::MAX / 2,
                ..m.clone()
            };
            rotate_to(&mut n, &[], deg, compat);
            let (cx, cy) = ((rect[0] + rect[2]) / 2.0, (rect[1] + rect[3]) / 2.0);
            let to = [cx - n.w / 2.0, cy - n.h / 2.0, cx + n.w / 2.0, cy + n.h / 2.0];
            new.push((n, to));
        }
        if new.is_empty() {
            return Outcome::Unchanged("an empty group");
        }
        self.replace_floats("Ungroup", &[g], anchor, new, fonts)
    }
}

// API shape commands
impl Editor {
    pub(super) fn shape_command(&mut self, cmd: super::commands::Command, fonts: &crate::font::Fonts) -> super::commands::Outcome {
        use super::commands::{Command as C, Outcome};
        use crate::api::drawing::FlipAxis;
        use crate::drawing::{NewShape, ShapeKind};
        let bad = |_: String| Outcome::Unchanged("invalid value");
        match cmd {
            C::InsertShape(s) => {
                let line = s.preset.is_line();
                let kind = if line { NewShape::Line(s.preset.0) } else { NewShape::Shape(s.preset.0) };
                let w = s.width.map_or(72.0, |p| p.0);
                let h = s.height.map_or(if line { 0.0 } else { 72.0 }, |p| p.0);
                self.insert_shape_sized(kind, [w, h], fonts)
            }
            C::SetShapePreset(p) => self.set_shape_preset(p.0, fonts),
            C::SetShapeAdjust(a) => self.set_shape_adjust(a.index as usize, a.value, fonts),
            C::SetShapeFill(v) => {
                let mut images = Vec::new();
                let fill = match v.to_model(&mut |data| {
                    crate::docx::image::image_info(data).map(|_| {
                        images.push(data.to_vec());
                        0
                    })
                }) {
                    Ok(f) => f,
                    Err(e) => return bad(e),
                };
                let fill = match (fill, images.pop()) {
                    (Some(crate::drawing::paint::Fill::Blip(mut b)), Some(data)) => {
                        b.image = Some(self.pending_image(data));
                        Some(crate::drawing::paint::Fill::Blip(b))
                    }
                    (f, _) => f,
                };
                self.set_shape_fill(fill, fonts)
            }
            C::SetShapeLine(change) => {
                let line = self.selected_line();
                match change.apply(line.as_ref(), &mut |_| None) {
                    Ok(l) => self.set_shape_line(l, fonts),
                    Err(e) => bad(e),
                }
            }
            C::SetShapeEffects(change) => {
                let effects = self.selected_effects();
                match change.apply(effects.as_ref()) {
                    Ok(e) => self.set_shape_effects(Some(e), fonts),
                    Err(e) => bad(e),
                }
            }
            C::SetShapeText(change) => {
                let Some(body) = self.selected_sp().map(|s| s.body.clone()) else { return Outcome::unavailable("no shape selected") };
                match change.apply(&body) {
                    Ok(b) => self.set_shape_body(b, fonts),
                    Err(e) => bad(e),
                }
            }
            C::SetPictureAdjust(change) => {
                let mut err = None;
                let out = self.edit_shapes(
                    "Picture Adjustments",
                    |s| match &mut s.kind {
                        ShapeKind::Pic(p) => match change.apply(&p.blip.adjust) {
                            Ok(a) => {
                                p.blip.adjust = a;
                                true
                            }
                            Err(e) => {
                                err = Some(e);
                                false
                            }
                        },
                        _ => false,
                    },
                    fonts,
                );
                err.map_or(out, bad)
            }
            C::SetObjectRotation(d) => self.set_object_rotation(d, fonts),
            C::FlipObject(axis) => self.flip_object(axis == FlipAxis::Horizontal, fonts),
            C::ArrangeObject(a) => self.arrange_object(a, fonts),
            C::GroupObjects => self.group_objects(fonts),
            C::UngroupObjects => self.ungroup_objects(fonts),
            C::SelectGroupChild(order) => self.select_group_child(order == crate::api::value::Order::Next),
            _ => Outcome::Unchanged("not a shape command"),
        }
    }

    fn selected_sp(&self) -> Option<crate::drawing::SpShape> {
        let f = *self.selected_floats().first()?;
        let path = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        self.float_of(f)?.shape.as_deref()?.at(&path)?.sp().cloned()
    }

    fn selected_line(&self) -> Option<crate::drawing::paint::Line> {
        let f = *self.selected_floats().first()?;
        let path = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        match &self.float_of(f)?.shape.as_deref()?.at(&path)?.kind {
            crate::drawing::ShapeKind::Sp(s) => s.line.clone(),
            crate::drawing::ShapeKind::Pic(p) => p.line.clone(),
            _ => None,
        }
    }

    fn selected_effects(&self) -> Option<crate::drawing::paint::EffectList> {
        let f = *self.selected_floats().first()?;
        let path = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        match &self.float_of(f)?.shape.as_deref()?.at(&path)?.kind {
            crate::drawing::ShapeKind::Sp(s) => s.effects.clone(),
            crate::drawing::ShapeKind::Pic(p) => p.effects.clone(),
            _ => None,
        }
    }

    fn select_group_child(&mut self, next: bool) -> super::commands::Outcome {
        use super::commands::Outcome;
        let floats = self.selected_floats();
        let [f] = floats[..] else { return Outcome::unavailable("no group selected") };
        let Some(n) = self.float_of(f).and_then(|fl| fl.shape.as_deref()).and_then(|s| s.group()).map(|g| g.children.len()) else {
            return Outcome::unavailable("no group selected");
        };
        if n == 0 {
            return Outcome::Unchanged("an empty group");
        }
        let now = self.selected_child().and_then(|c| c.first().copied()).map(usize::from);
        let k = match (now, next) {
            (None, true) => 0,
            (None, false) => n - 1,
            (Some(i), true) => (i + 1) % n,
            (Some(i), false) => (i + n - 1) % n,
        };
        self.object_child = Some((f, vec![k as u16]));
        Outcome::Moved
    }
}

impl Editor {
    pub fn theme_colours(&self) -> Vec<crate::api::value::Color> {
        crate::api::drawing::ThemeColour::ALL
            .iter()
            .map(|t| {
                crate::api::value::Color(
                    crate::drawing::colour::Colour::scheme(t.slot()).resolve(&self.doc.theme, &self.doc.colour_map, None).to_rgb(),
                )
            })
            .collect()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShapeInfo {
    pub kind: Option<crate::api::state::ObjectKind>,
    pub state: Option<crate::api::drawing::ShapeState>,
    pub rotation: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub child_path: Vec<u16>,
    pub count: u16,
    pub floating: bool,
    pub sp: bool,
    pub pic: bool,
    pub group: bool,
}

impl Editor {
    pub fn shape_info(&self) -> Option<ShapeInfo> {
        use crate::api::drawing::{EffectsValue, FillState, FillValue, LineState, LineValue, ShapePreset, ShapeState, ShapeTextState};
        use crate::api::state::ObjectKind as K;
        use crate::drawing::ShapeKind;
        use crate::drawing::paint::style_entry;
        let floats = self.selected_floats();
        let (shape, floating, text_box) = match self.selection {
            super::selection::Selection::Object(ObjectRef::Floating(f)) => {
                let fl = self.float_of(f)?;
                (fl.shape.as_deref()?, true, fl.text_box.is_some())
            }
            super::selection::Selection::Object(ObjectRef::Inline { para, offset }) => {
                match self.index.para(&self.doc, para).and_then(|p| super::stories::inline_at(p, offset)) {
                    Some(crate::docx::Inline::Picture(p, _)) => (p.shape.as_deref()?, false, false),
                    _ => return None,
                }
            }
            _ => return None,
        };
        let child = self.selected_child().map(<[u16]>::to_vec).unwrap_or_default();
        let s = shape.at(&child)?;
        let theme = &self.doc.theme;
        let mut info = ShapeInfo {
            rotation: s.xfrm.rot.degrees(),
            flip_h: s.xfrm.flip_h,
            flip_v: s.xfrm.flip_v,
            count: floats.len().max(1) as u16,
            child_path: child,
            floating,
            ..ShapeInfo::default()
        };
        let style = |r: &Option<(u32, Option<crate::drawing::colour::Colour>)>| r.as_ref().map(|(i, c)| (*i, c.clone()));
        match &s.kind {
            ShapeKind::Sp(sp) => {
                info.sp = true;
                let line_preset = sp.geom.preset().map(ShapePreset).is_some_and(ShapePreset::is_line);
                info.kind = Some(if sp.text_box && (text_box || !info.child_path.is_empty()) {
                    K::TextBox
                } else if line_preset {
                    K::Line
                } else {
                    K::Shape
                });
                let refs = sp.style.as_ref();
                let fill = match &sp.fill {
                    Some(f) => FillState::Explicit(FillValue::of(f, None, &[])),
                    None => FillState::FromStyle(match refs.and_then(|r| style(&r.fill)) {
                        Some((i, c)) => {
                            style_entry(&theme.fills, Some(&theme.bg_fills), i).map_or(FillValue::None, |f| FillValue::of(f, c.as_ref(), &[]))
                        }
                        None => FillValue::None,
                    }),
                };
                let none = crate::drawing::paint::Line { fill: Some(crate::drawing::paint::Fill::None), ..Default::default() };
                let line = match &sp.line {
                    Some(l) => LineState::Explicit(LineValue::of(l, None, &[])),
                    None => LineState::FromStyle(match refs.and_then(|r| style(&r.line)) {
                        Some((i, c)) => LineValue::of(style_entry(&theme.lines, None, i).unwrap_or(&none), c.as_ref(), &[]),
                        None => LineValue::of(&none, None, &[]),
                    }),
                };
                let effects = match &sp.effects {
                    Some(e) => EffectsValue::of(e, None),
                    None => refs
                        .and_then(|r| style(&r.effect))
                        .and_then(|(i, c)| style_entry(&theme.effects, None, i).map(|e| EffectsValue::of(e, c.as_ref())))
                        .unwrap_or_default(),
                };
                info.state = Some(ShapeState {
                    preset: sp.geom.preset().map(ShapePreset),
                    adjust: sp.geom.adjust_values(),
                    fill,
                    line,
                    effects,
                    text: Some(ShapeTextState::of(&sp.body)),
                });
            }
            ShapeKind::Pic(p) => {
                info.pic = true;
                info.kind = Some(K::Picture);
                info.state = Some(ShapeState {
                    preset: p.geom.preset().map(ShapePreset),
                    adjust: p.geom.adjust_values(),
                    fill: FillState::Explicit(FillValue::of(&crate::drawing::paint::Fill::Blip(p.blip.clone()), None, &[])),
                    line: match &p.line {
                        Some(l) => LineState::Explicit(LineValue::of(l, None, &[])),
                        None => LineState::FromStyle(LineValue::of(
                            &crate::drawing::paint::Line { fill: Some(crate::drawing::paint::Fill::None), ..Default::default() },
                            None,
                            &[],
                        )),
                    },
                    effects: p.effects.as_ref().map(|e| EffectsValue::of(e, None)).unwrap_or_default(),
                    text: None,
                });
            }
            ShapeKind::Group(g) => {
                info.group = true;
                info.kind = Some(K::Group);
                info.state = Some(ShapeState {
                    preset: None,
                    adjust: Vec::new(),
                    fill: g.fill.as_ref().map_or(FillState::FromStyle(FillValue::None), |f| FillState::Explicit(FillValue::of(f, None, &[]))),
                    line: LineState::FromStyle(LineValue::of(
                        &crate::drawing::paint::Line { fill: Some(crate::drawing::paint::Fill::None), ..Default::default() },
                        None,
                        &[],
                    )),
                    effects: EffectsValue::default(),
                    text: None,
                });
            }
            ShapeKind::Raw(_) => {}
        }
        Some(info)
    }
}
