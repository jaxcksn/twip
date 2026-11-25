//! Select Objects and anchor drags

use crate::font::Fonts;
use crate::layout::{FloatId, ParaId};

use super::Editor;
use super::commands::Outcome;
use super::input::Request;
use super::position::StoryId;
use super::selection::{ObjectRef, Selection};
use super::stories::reachable;

/// A rectangle being dragged in Select Objects, in its page's points.
#[derive(Clone, Copy, Debug)]
pub(super) struct Marquee {
    page: usize,
    from: [f64; 2],
    now: [f64; 2],
}

/// The selected float's anchor being dragged to another paragraph.
#[derive(Clone, Copy, Debug)]
pub(super) struct AnchorDrag {
    f: FloatId,
    page: usize,
    now: [f64; 2],
}

/// How far from the anchor's ring a press takes it, in pixels.
const ANCHOR_REACH: f64 = 7.0;

impl Editor {
    /// Turns Word's Select Objects on or off: while on, a drag from outside every object draws a rectangle that
    /// selects the floating objects wholly inside it, and a click on the text places no caret. Escape turns it off.
    pub fn set_select_objects(&mut self, on: bool) {
        self.select_objects = on;
        if !on && self.marquee.take().is_some() {
            self.requests.push(Request::ReleasePointer);
        }
    }

    /// Whether Select Objects is on.
    pub fn selecting_objects(&self) -> bool {
        self.select_objects
    }

    /// The Select Objects rectangle being dragged: its page and its box (left, top, right, bottom), points.
    pub fn marquee_rect(&self) -> Option<(usize, [f64; 4])> {
        let m = self.marquee?;
        let ([ax, ay], [bx, by]) = (m.from, m.now);
        Some((m.page, [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)]))
    }

    /// A press in Select Objects off every object and handle: a rectangle starts (true).
    pub(super) fn marquee_press(&mut self, x: f64, y: f64, fonts: &Fonts) -> bool {
        if !self.select_objects
            || self.handle_at(x, y, fonts).is_some()
            || self.shape_handle_at(x, y).is_some()
            || self.object_hit(x, y, fonts).is_some()
        {
            return false;
        }
        let Some((page, px, py)) = self.stack().page_at(x, y) else { return false };
        self.marquee = Some(Marquee { page, from: [px, py], now: [px, py] });
        self.requests.push(Request::CapturePointer);
        true
    }

    pub(super) fn marquee_move(&mut self, x: f64, y: f64) -> bool {
        let Some(page) = self.marquee.map(|m| m.page) else { return false };
        let (ox, oy) = self.stack().to_doc(page, 0.0, 0.0).unwrap_or((0.0, 0.0));
        if let Some(m) = self.marquee.as_mut() {
            m.now = [x - ox, y - oy];
        }
        true
    }

    /// The rectangle let go: the floating objects of the story being edited drawn wholly inside it selected, the first
    /// in document order the selection and the rest with it; none inside leaves the selection as it was.
    pub(super) fn marquee_release(&mut self, fonts: &Fonts) -> bool {
        let Some((page, [x0, y0, x1, y1])) = self.marquee_rect() else { return false };
        self.marquee = None;
        self.requests.push(Request::ReleasePointer);
        self.ensure_page(page, fonts);
        let ctx = self.editing();
        let Some(geo) = self.pages.page(page, &self.index) else { return true };
        let inside: Vec<FloatId> = geo
            .stories
            .iter()
            .filter(|s| reachable(ctx, s.host) && s.float.is_some())
            .filter_map(|s| s.float.map(|f| (f, s.turn.rect(s.rect))))
            .filter(|(_, [a, b, c, d])| *a >= x0 && *b >= y0 && *c <= x1 && *d <= y1)
            .map(|(f, _)| f)
            .collect();
        // Floats only (an inline shape's story isn't one), one story's.
        let mut found: Vec<FloatId> = Vec::new();
        for f in inside {
            if matches!(self.story_object(f), ObjectRef::Floating(_)) && self.float_of(f).is_some() && !found.contains(&f) {
                found.push(f);
            }
        }
        let story = |e: &Self, f: FloatId| e.index.float_anchor(f).and_then(|a| e.index.para_slot(a)).map(|s| e.index.story(s.container));
        let Some(&first) = found.first() else { return true };
        let home = story(self, first);
        found.retain(|&f| story(self, f) == home);
        self.set_selection(Selection::Object(ObjectRef::Floating(first)));
        self.object_child = None;
        self.object_also = (found.len() > 1).then(|| (first, found[1..].to_vec()));
        true
    }

    /// Where the anchor of a float anchored in `para` shows on `page`: the left of the paragraph's lines there, at its
    /// first line's top, turned onto the page as that line is.
    pub(super) fn anchor_point(&mut self, page: usize, para: ParaId) -> Option<[f64; 2]> {
        let geo = self.pages.page(page, &self.index)?;
        let first = geo.lines.iter().find(|l| l.span_of(para).is_some())?;
        let x = geo.lines.iter().filter_map(|l| l.span_of(para).map(|(x0, _)| x0)).fold(f64::MAX, f64::min);
        Some(first.page(x, first.top))
    }

    /// The centre of the selected float's anchor ring when it is drawn on `page` (its anchor's first page), points.
    fn anchor_ring(&mut self, page: usize) -> Option<(FloatId, [f64; 2])> {
        let Selection::Object(ObjectRef::Floating(f)) = self.selection else { return None };
        if !(self.mark_options.all || self.mark_options.anchors) {
            return None;
        }
        let para = self.index.float_anchor(f)?;
        let [x, y] = self.anchor_point(page, para)?;
        let repeats = self.index.para_slot(para).is_some_and(|s| matches!(self.index.host(self.index.story(s.container)), StoryId::HeaderFooter(_)));
        // A header's anchor shows on the page nearest the view; another's on the first of its paragraph's pages, the
        // one the page before doesn't show (a paragraph's pages are one run).
        let shown = match repeats {
            true => self.pages.pages_of(para, self.near, &self.index).first() == Some(&page),
            false => !page.checked_sub(1).and_then(|p| self.pages.page(p, &self.index)).is_some_and(|g| g.shows_para(para)),
        };
        if !shown {
            return None;
        }
        let u = 0.75 / self.viewport.zoom.max(0.1);
        // As the overlay draws it: 18 px left of the anchor (10 px from the page's edge at least), 8 px down.
        Some((f, [(x - 18.0 * u).max(10.0 * u), y + 8.0 * u]))
    }

    /// The selected float whose anchor is under document point (`x`, `y`), and the page and page point there.
    pub(super) fn anchor_hit(&mut self, x: f64, y: f64) -> Option<(FloatId, usize, [f64; 2])> {
        let (page, px, py) = self.stack().page_at(x, y)?;
        let (f, [cx, cy]) = self.anchor_ring(page)?;
        // A frame stays with its paragraphs.
        if self.float_of(f).is_none_or(crate::docx::Float::is_frame) || !self.float_changeable(f) {
            return None;
        }
        let px_pt = 0.75 / self.viewport.zoom.max(0.1);
        let reach = ANCHOR_REACH * px_pt;
        // The ring and its stem below it.
        let on_it = (px - cx).abs() <= reach && py >= cy - reach && py <= cy + reach + 7.0 * px_pt;
        on_it.then_some((f, page, [px, py]))
    }

    /// A press on the selected float's anchor starts dragging it to another paragraph (true).
    pub(super) fn anchor_press(&mut self, x: f64, y: f64) -> bool {
        let Some((f, page, now)) = self.anchor_hit(x, y) else { return false };
        self.anchor_drag = Some(AnchorDrag { f, page, now });
        self.requests.push(Request::CapturePointer);
        true
    }

    pub(super) fn anchor_move(&mut self, x: f64, y: f64) -> bool {
        let Some(page) = self.anchor_drag.map(|d| d.page) else { return false };
        let (ox, oy) = self.stack().to_doc(page, 0.0, 0.0).unwrap_or((0.0, 0.0));
        if let Some(d) = self.anchor_drag.as_mut() {
            d.now = [x - ox, y - oy];
        }
        true
    }

    /// The paragraph an anchor dragged to `d.now` goes to, its top and the frame its lines are laid out in.
    fn anchor_target(&mut self, d: AnchorDrag) -> Option<(ParaId, f64, crate::layout::vertical::Turn)> {
        let anchor = self.index.float_anchor(d.f)?;
        let [x, y] = d.now;
        self.anchor_at(d.page, anchor, [x, y, x, y])
    }

    /// Where the dragged anchor would go: its page and where the paragraph's anchor shows there, for the overlay.
    pub(super) fn anchor_drag_view(&mut self) -> Option<(usize, [f64; 2])> {
        let d = self.anchor_drag?;
        let (para, ..) = self.anchor_target(d)?;
        Some((d.page, self.anchor_point(d.page, para)?))
    }

    /// The anchor let go: the float anchors in the paragraph there and stays where it is drawn (a place from its
    /// paragraph or line is from the new one's). One undo step ("Move Anchor").
    pub(super) fn anchor_release(&mut self, fonts: &Fonts) -> Option<Outcome> {
        let d = self.anchor_drag.take()?;
        self.requests.push(Request::ReleasePointer);
        let Some((dest, top, region)) = self.anchor_target(d) else { return Some(Outcome::Unchanged("no paragraph")) };
        let Some(anchor) = self.index.float_anchor(d.f) else { return Some(Outcome::Unchanged("no anchor")) };
        if dest == anchor {
            return Some(Outcome::Unchanged("same paragraph"));
        }
        let story = self.index.para_slot(dest).map_or(super::position::StoryId::Body, |s| self.index.story(s.container));
        if !self.float_changeable(d.f) || !self.editable(&super::position::Position::text(story, dest, crate::layout::Unit(0))) {
            return Some(Outcome::unavailable("the paragraph can't be changed"));
        }
        let Some((page, rect)) = self.float_box(d.f, fonts) else { return Some(Outcome::Unchanged("float not drawn")) };
        let Some(mut fl) = self.moved_float(d.f, page, rect, rect) else { return Some(Outcome::Unchanged("no float")) };
        let drawn = region.inverse().rect(rect);
        let frames = self.float_frames(d.page, Some(dest), fl.in_cell(self.doc.settings.compat()));
        // A place down from the paragraph (or its line) is from the new paragraph; one across from the column (or its
        // character) from the new paragraph's column. Both alignments go first: either changes where effect extents
        // fall on the other axis.
        let v = matches!(fl.v_relative.as_str(), "paragraph" | "line");
        let h = matches!(fl.h_relative, crate::docx::HorizontalRelation::Column | crate::docx::HorizontalRelation::Character) && frames.is_some();
        if v {
            (fl.v_align, fl.relative.pos[1]) = (None, None);
        }
        if h {
            (fl.h_align, fl.relative.pos[0]) = (None, None);
        }
        let twips = |p: f64| (p * 20.0).round() / 20.0;
        if v {
            fl.v_offset = twips(match &frames {
                Some((fr, _)) => crate::layout::float::v_offset_at(&fl, fr, drawn[1], top, top),
                None => drawn[1] - top,
            });
        }
        if h && let Some((fr, o)) = &frames {
            fl.h_offset = twips(crate::layout::float::h_offset_at(&fl, fr, drawn[0] - o[0]));
        }
        Some(self.commit_float(d.f, fl, Some(dest), "Move Anchor", fonts))
    }

    /// Drops a marquee or anchor drag (true when one was going).
    pub(super) fn marquee_cancel(&mut self) -> bool {
        if self.marquee.take().is_some() || self.anchor_drag.take().is_some() {
            self.requests.push(Request::ReleasePointer);
            return true;
        }
        false
    }
}
