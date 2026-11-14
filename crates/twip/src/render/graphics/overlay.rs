//! The view's overlays with twip-graphics: what editing shows over a page's content (field shading, the fade, marks,
//! regions, comment tints, the drop caret, the guide line, the selection, the caret).
//! Recorded every frame, never with the page.

use twip_graphics::kurbo::{self, Affine};
use twip_graphics::paint::{BlendMode, Color};
use twip_graphics::scene::{Clip, Layer, Recorder};

use super::{Graphics, aliased, chrome_face, dashed, measure, pen, rect, rect_path};
use crate::editing::view::{MarkKind, SelectionView, View};
use crate::layout::fragment::Item;
use crate::render::{ChromeFace, Frame, Renderer};

/// The points of a wavy line along `x0..x1` at `y`: a step `step` across, alternately `amp` down.
fn wave(x0: f64, x1: f64, y: f64, step: f64, amp: f64) -> impl Iterator<Item = (f64, f64)> {
    let n = (((x1 + step / 2.0) - x0) / step).floor().max(0.0) as usize;
    (0..=n).map(move |i| (x0 + i as f64 * step, if i % 2 == 1 { y + amp } else { y }))
}

impl Graphics {
    /// Page `i`'s overlays, in its points.
    pub(super) fn overlays(&mut self, rec: &mut Recorder, r: &Renderer, view: &View, images: &[Vec<u8>], i: usize, f: &Frame) {
        let k = f.px_per_pt;
        let (page, palette) = (r.page_theme(), r.palette());
        let ink = |c: u32| r.ink_for(false, c);
        // A light page: the tint multiplied under the text, which keeps its colour; a dark one darkens under multiply,
        // so the accent is laid over it.
        let highlight = aliased(palette.selection);
        let highlight = if page.is_dark() { highlight } else { twip_graphics::paint::Paint { blend: BlendMode::Multiply, ..highlight } };
        let fade = aliased(ink(0xFF_FFFF).with_alpha(130));
        let object = palette.object;
        let outline = pen(0.75);
        // Object chrome: 1 px lines, the page's colour behind handles.
        let hair = pen(1.0 / k);
        let paper = r.page();
        // Field shading: multiplied under the text on a light page, screened over a dark one.
        // shortcut: a highlight inside a field is tinted by the shading; drawing it beneath the page's items would
        // leave the highlight untouched.
        let (shade, blend) = if page.is_dark() { (Color::rgb(0x2C_2C2C), BlendMode::Screen) } else { (Color::rgb(0xD9_D9D9), BlendMode::Multiply) };
        let shade = twip_graphics::paint::Paint { blend, ..aliased(shade) };
        for (_, b) in view.field_shading.iter().filter(|(pg, _)| *pg == i) {
            rec.fill_rect(rect(*b), shade);
        }
        for (_, b) in view.fade.iter().filter(|(pg, _)| *pg == i) {
            rec.fill_rect(rect(*b), fade);
        }
        // Misspellings: a wavy red line along each word's bottom, 1 device pixel wide.
        let px = (k * f.dpr).max(0.01);
        let (step, amp) = (2.0 / px, 1.5 / px);
        let red = ink(0xE6_0000);
        for (_, [x0, _, x1, y1]) in view.spelling.iter().filter(|(pg, _)| *pg == i) {
            self.polyline(rec, wave(*x0, *x1, y1 - 1.5 * amp, step, amp), &pen(1.0 / px), red);
        }
        // Labelled regions (content controls, shown sub-templates): a dashed boundary and a title tab above its top left.
        let blue = ink(0x00_78D4);
        for g in &view.regions {
            for (_, b) in g.rects.iter().filter(|(pg, _)| *pg == i) {
                rec.stroke_path(&rect_path(rect(*b)), &dashed(1.0 / px, 3.0 / px, 2.0 / px), blue);
            }
            if g.title_rect.0 == i {
                let b = rect(g.title_rect.1);
                rec.fill_rect(b, aliased(blue));
                if let Some(face) = self.fallback.clone() {
                    rec.push_clip(Clip::Rect { rect: b, aa: true });
                    self.text(rec, &face, &g.title, b.x0 + 4.0, b.y1 - 4.0, 9.0, Color::WHITE);
                    rec.pop();
                }
            }
        }
        // The correction just made: a dashed accent underline, and its options button (16 x 12 px) under it.
        if let Some(c) = &view.correction {
            let accent = page.accent.solid;
            for (_, [x0, _, x1, y1]) in c.rects.iter().filter(|(pg, _)| *pg == i) {
                rec.line((*x0, y1 - 0.5).into(), (*x1, y1 - 0.5).into(), &dashed(0.75, 2.25, 1.5), accent);
            }
            if let Some((_, b)) = c.button.filter(|(pg, _)| *pg == i) {
                let b = rect(b);
                rec.fill_rect(b, aliased(paper));
                rec.stroke_path(&rect_path(b), &pen(0.75), accent);
                if let Some(face) = chrome_face(ChromeFace::Mono500) {
                    let w = measure(&face, "\u{25BE}", 6.0);
                    self.text(rec, &face, "\u{25BE}", (b.x0 + b.x1 - w) / 2.0, b.y1 - 2.25, 6.0, accent);
                }
            }
        }
        // Commented text: an amber tint over each thread's range, stronger for the active thread. A view overlay, never
        // printed.
        for c in &view.comments {
            let tint = aliased(ink(0xFF_B900).with_alpha(if c.active { 110 } else { 45 }));
            for (_, b) in c.rects.iter().filter(|(pg, _)| *pg == i) {
                rec.fill_rect(rect(*b), tint);
            }
        }
        // Element marks: a tint over the element and a wavy line twice the spelling line's weight, in the severity's
        // colour, so they read apart from misspellings.
        for m in view.marks.iter().filter(|m| m.page == i) {
            let c = ink(match m.severity {
                crate::editing::view::Severity::Error => 0xD1_3438,
                crate::editing::view::Severity::Warning => 0xC1_9C00,
                crate::editing::view::Severity::Info => 0x00_78D4,
            });
            let [x0, _, x1, y1] = m.rect;
            rec.fill_rect(rect(m.rect), aliased(c.with_alpha(28)));
            self.polyline(rec, wave(x0, x1, y1 - 1.5 * amp, step, amp), &pen(2.0 / px), c.with_alpha(255));
        }
        if let Some(c) = view.drop.as_ref().filter(|c| c.page == i) {
            rec.line((c.x, c.top).into(), (c.x, c.bottom).into(), &dashed(0.75, 1.5, 1.5), aliased(palette.drop));
        }
        if let Some((_, [x0, y0, x1, y1])) = view.guide.filter(|(pg, _)| *pg == i) {
            rec.line((x0, y0).into(), (x1, y1).into(), &outline, aliased(object));
        }
        // An object drag's ghost: the object's own drawing at half opacity, mapped into the box the drag puts it in.
        if let Some(g) = view.ghost.filter(|g| g.page == i)
            && let Some(pg) = view.snapshot.pages().get(g.source)
        {
            let ([a0, b0, a1, b1], [c0, d0, c1, d1]) = (g.from, g.to);
            let items = match g.float {
                Some(fl) => crate::layout::traverse::paint_float(pg, fl),
                None => crate::layout::traverse::paint_items(pg)
                    .into_iter()
                    .filter(|it| {
                        let q = match it {
                            Item::Image { rect, .. } => *rect,
                            Item::Shape(s) => s.rect,
                            _ => return false,
                        };
                        q[0] >= a0 - 0.5 && q[1] >= b0 - 0.5 && q[2] <= a1 + 0.5 && q[3] <= b1 + 0.5
                    })
                    .collect(),
            };
            rec.push_layer(&Layer { opacity: 0.5, ..Layer::default() });
            // Centre to centre, scaled along the frame's own (turned) sides.
            let turn = g.rot.to_radians();
            let to = Affine::translate(((c0 + c1) / 2.0, (d0 + d1) / 2.0))
                * Affine::rotate(turn)
                * Affine::scale_non_uniform((c1 - c0) / (a1 - a0).max(0.01), (d1 - d0) / (b1 - b0).max(0.01))
                * Affine::rotate(-turn)
                * Affine::translate((-(a0 + a1) / 2.0, -(b0 + b1) / 2.0));
            rec.push_transform(to);
            for it in &items {
                self.item(rec, r, false, it, images);
            }
            rec.pop();
            rec.pop();
        }
        // The selected float's anchor: an 8 px ring (1.5 px stroke) with a 1.5 x 6 px stem below it, 18 px left of its
        // paragraph's first line (at least 10 px from the page's edge), 8 px below the line's top.
        if let Some((_, [x, y])) = view.anchor.filter(|(pg, _)| *pg == i) {
            let u = 1.0 / k;
            let (cx, cy) = ((x - 18.0 * u).max(10.0 * u), y + 8.0 * u);
            self.draw_shape(rec, kurbo::Circle::new((cx, cy), 3.25 * u), Some(&pen(1.5 * u)), object);
            rec.fill_rect(kurbo::Rect::new(cx - 0.75 * u, cy + 4.0 * u, cx + 0.75 * u, cy + 10.0 * u), aliased(object));
        }
        match &view.selection {
            SelectionView::Text(rects) | SelectionView::Cells(rects) => {
                for (_, b) in rects.iter().filter(|(pg, _)| *pg == i) {
                    rec.fill_rect(rect(*b), highlight);
                }
            }
            SelectionView::Object(outlines, handles) => {
                // A 1 px outline and eight 8 px handles, filled with the page's colour, with a 1 px border, all turned
                // with the object.
                for o in outlines.iter().filter(|o| o.page == i) {
                    let [x0, y0, x1, y1] = o.rect;
                    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                    rec.push_transform(Affine::rotate_about(o.rot.to_radians(), kurbo::Point::new(xm, ym)));
                    rec.stroke_path(&rect_path(rect(o.rect)), &hair, object);
                    if *handles {
                        let h = 4.0 / k;
                        for (hx, hy) in [(x0, y0), (xm, y0), (x1, y0), (x1, ym), (x1, y1), (xm, y1), (x0, y1), (x0, ym)] {
                            let b = kurbo::Rect::new(hx - h, hy - h, hx + h, hy + h);
                            rec.fill_rect(b, paper);
                            rec.stroke_path(&rect_path(b), &hair, object);
                        }
                    }
                    rec.pop();
                }
            }
            SelectionView::None => {}
        }
        // A selected shape: its turned outline, its rotate knob (10 px, in the page's colour) on a 20 px stem from its
        // top, its adjust handles as yellow diamonds.
        for s in view.shapes.iter().filter(|s| s.page == i) {
            let [x0, y0, x1, y1] = s.rect;
            if s.rot != 0.0 {
                rec.push_transform(Affine::rotate_about(s.rot.to_radians(), kurbo::Point::new((x0 + x1) / 2.0, (y0 + y1) / 2.0)));
                rec.stroke_path(&rect_path(rect(s.rect)), &hair, object);
                rec.pop();
            }
            // The knob's centre is 24 px from the top; the stem runs 20 px of that.
            let stem = [s.top[0] + (s.rotate[0] - s.top[0]) * 20.0 / 24.0, s.top[1] + (s.rotate[1] - s.top[1]) * 20.0 / 24.0];
            rec.line((s.top[0], s.top[1]).into(), (stem[0], stem[1]).into(), &hair, object);
            let knob = kurbo::Circle::new((s.rotate[0], s.rotate[1]), 5.0 / k);
            self.draw_shape(rec, knob, None, paper);
            self.draw_shape(rec, knob, Some(&hair), object);
            let h = 4.0 / k;
            for &[x, y] in &s.adjust {
                let d = kurbo::BezPath::from_vec(vec![
                    kurbo::PathEl::MoveTo((x, y - h).into()),
                    kurbo::PathEl::LineTo((x + h, y).into()),
                    kurbo::PathEl::LineTo((x, y + h).into()),
                    kurbo::PathEl::LineTo((x - h, y).into()),
                    kurbo::PathEl::ClosePath,
                ]);
                rec.fill_path(d.elements(), aliased(Color::rgb(0xFF_D700)));
                rec.stroke_path(d.elements(), &hair, object);
            }
        }
        self.table_affordances(rec, r, view, i, f);
        self.object_feedback(rec, r, view, i, f);
        self.marks(rec, r, view, i, f);
        // The caret: a bar at the insertion point, at least one device pixel wide, last so nothing covers it.
        if let Some(c) = view.caret.as_ref().filter(|c| f.caret && c.page == i && matches!(view.selection, SelectionView::None)) {
            let width = f.dpr.round().max(1.0) / (k * f.dpr);
            rec.fill_rect(kurbo::Rect::new(c.x, c.top, c.x + width, c.bottom), aliased(r.caret()));
        }
    }

    /// Page `i`'s object drag feedback: the dot grid, dashed guides, the dotted anchor line, the original box dashed,
    /// dimension lines and chips. In page points; sizes are CSS px.
    fn object_feedback(&mut self, rec: &mut Recorder, r: &Renderer, view: &View, i: usize, f: &Frame) {
        use crate::editing::view::{Axis, Measure, Tone};
        let u = 1.0 / f.px_per_pt.max(0.01);
        let (t, accent) = (r.page_theme().tokens, r.palette().object);
        let line = |dash: Option<[f64; 2]>| match dash {
            Some([on, off]) => dashed(u, on * u, off * u),
            None => pen(u),
        };
        for (_, g) in view.guides.iter().filter(|(p, _)| *p == i) {
            if let (Some(grid), Some(page)) = (g.grid, view.snapshot.pages().get(i)) {
                let (step, dot) = (grid.step.0.max(1.0), t.ink3);
                let first = |o: f64| o - (o / step).floor() * step;
                let mut y = first(grid.origin[1].0);
                while y <= page.h {
                    let mut x = first(grid.origin[0].0);
                    while x <= page.w {
                        rec.fill_rect(kurbo::Rect::new(x - u / 2.0, y - u / 2.0, x + u / 2.0, y + u / 2.0), dot);
                        x += step;
                    }
                    y += step;
                }
            }
            let guide = line(Some([3.0, 3.0]));
            for l in &g.lines {
                let (a, b) = (l.span.0.0, l.span.1.0);
                match l.axis {
                    Axis::X => rec.line((l.at.0, a).into(), (l.at.0, b).into(), &guide, accent),
                    Axis::Y => rec.line((a, l.at.0).into(), (b, l.at.0).into(), &guide, accent),
                }
            }
            if let Some([x0, y0, x1, y1]) = g.anchor_link {
                rec.line((x0.0, y0.0).into(), (x1.0, y1.0).into(), &line(Some([1.0, 1.0])), accent);
            }
        }
        if let Some(o) = view.original.filter(|o| o.page == i) {
            let [x0, y0, x1, y1] = o.rect;
            rec.push_transform(Affine::rotate_about(o.rot.to_radians(), kurbo::Point::new((x0 + x1) / 2.0, (y0 + y1) / 2.0)));
            rec.stroke_path(&rect_path(rect(o.rect)), &line(Some([3.0, 3.0])), t.ink3);
            rec.pop();
        }
        let (Some(mono5), Some(mono6)) = (chrome_face(ChromeFace::Mono500), chrome_face(ChromeFace::Mono600)) else { return };
        let size = 10.0 * u;
        let tone = |tone: Tone| match tone {
            Tone::Accent => accent,
            Tone::Muted => t.ink2,
        };
        for (_, m) in view.measures.iter().filter(|(p, _)| *p == i) {
            match m {
                // A line with 7 px end ticks, its label over the middle on the paper.
                Measure::Dimension { from, to, label, tone: c } => {
                    let (c, (x0, y0), (x1, y1)) = (tone(*c), (from[0].0, from[1].0), (to[0].0, to[1].0));
                    rec.line((x0, y0).into(), (x1, y1).into(), &line(None), c);
                    for (x, y) in [(x0, y0), (x1, y1)] {
                        rec.line((x, y - 3.5 * u).into(), (x, y + 3.5 * u).into(), &line(None), c);
                    }
                    let w = measure(&mono6, label, size);
                    let (mx, top) = ((x0 + x1) / 2.0 - w / 2.0, y0 - 5.0 * u);
                    rec.fill_rect(kurbo::Rect::new(mx - 4.0 * u, top, mx + w + 4.0 * u, top + 10.0 * u), t.paper);
                    self.text(rec, &mono6, label, mx, top + 8.0 * u, size, c);
                }
                // The ink chip with paper text: 4 x 7 px padding, 2 px corners.
                Measure::Chip { at, label } => {
                    let (x, y) = (at[0].pt().0, at[1].pt().0);
                    let w = measure(&mono5, label, size);
                    let chip = kurbo::RoundedRect::new(x, y, x + w + 14.0 * u, y + 18.0 * u, 2.0 * u);
                    self.draw_shape(rec, chip, None, t.ink);
                    self.text(rec, &mono5, label, x + 7.0 * u, y + 12.5 * u, size, t.paper);
                }
                Measure::Status { at, label, tone: c } => self.text(rec, &mono6, label, at[0].0, at[1].0 + 8.0 * u, size, tone(*c)),
            }
        }
    }

    /// Page `i`'s table affordances: a resize's or move's new box (dashed, the accent's soft tint inside); the shown
    /// table's move handle (14 px, a four-way arrow) 20 px above and left of it and its resize handle (7 px) at its
    /// lower right, both with a 1 px accent border; under the pointer, a "+" (14 px, an accent border and cross) with
    /// a dashed line where the column or row goes, an accent arrow (10 × 7 px) at a column or row it selects, or a
    /// dashed line down a column border. In page points; sizes are CSS px.
    fn table_affordances(&mut self, rec: &mut Recorder, r: &Renderer, view: &View, i: usize, f: &Frame) {
        use crate::editing::view::TableMark;
        let u = 1.0 / f.px_per_pt.max(0.01);
        let (soft, solid) = (r.page_theme().accent.soft, r.palette().object);
        let (thin, dash) = (pen(u), dashed(u, 3.0 * u, 3.0 * u));
        let line = |rec: &mut Recorder, a: (f64, f64), b: (f64, f64), s: &kurbo::Stroke| rec.line(a.into(), b.into(), s, solid);
        // A box `s` px square at a page point: an accent border, the page showing through.
        let boxed =
            |rec: &mut Recorder, x: f64, y: f64, s: f64| rec.stroke_path(&rect_path(kurbo::Rect::new(x, y, x + s * u, y + s * u)), &thin, solid);
        // The "+" button: an accent border and cross, the page showing through.
        let plus = |rec: &mut Recorder, x: f64, y: f64| {
            boxed(rec, x, y, 14.0);
            let cross = pen(1.5 * u);
            let (cx, cy) = (x + 7.0 * u, y + 7.0 * u);
            line(rec, (cx - 3.5 * u, cy), (cx + 3.5 * u, cy), &cross);
            line(rec, (cx, cy - 3.5 * u), (cx, cy + 3.5 * u), &cross);
        };
        let triangle = |rec: &mut Recorder, pts: [(f64, f64); 3]| {
            let els = [
                kurbo::PathEl::MoveTo(pts[0].into()),
                kurbo::PathEl::LineTo(pts[1].into()),
                kurbo::PathEl::LineTo(pts[2].into()),
                kurbo::PathEl::ClosePath,
            ];
            rec.fill_path(&els, solid);
        };
        if let Some((_, b)) = view.table_preview.filter(|(p, _)| *p == i) {
            rec.fill_rect(rect(b), soft);
            rec.stroke_path(&rect_path(rect(b)), &dash, solid);
        }
        let Some(t) = view.table.as_ref().filter(|t| t.page == i) else { return };
        match t.mark {
            Some(TableMark::InsertColumn { x, top, bottom }) => {
                line(rec, (x, top - 4.0 * u), (x, bottom + 4.0 * u), &dash);
                plus(rec, x - 7.0 * u, top - 20.0 * u);
            }
            Some(TableMark::InsertRow { y, left, right }) => {
                line(rec, (left - 6.0 * u, y), (right, y), &dash);
                plus(rec, left - 22.0 * u, y - 7.0 * u);
            }
            Some(TableMark::SelectColumn { x0, x1, top }) => {
                let (cx, tip) = ((x0 + x1) / 2.0, top - 6.0 * u);
                triangle(rec, [(cx - 5.0 * u, tip - 7.0 * u), (cx + 5.0 * u, tip - 7.0 * u), (cx, tip)]);
            }
            Some(TableMark::SelectRow { y0, y1, left }) => {
                let (cy, tip) = ((y0 + y1) / 2.0, left - 6.0 * u);
                triangle(rec, [(tip - 7.0 * u, cy - 5.0 * u), (tip - 7.0 * u, cy + 5.0 * u), (tip, cy)]);
            }
            Some(TableMark::Border { x, top, bottom }) => line(rec, (x, top - 4.0 * u), (x, bottom + 4.0 * u), &dash),
            Some(TableMark::RowBorder { y, left, right }) => line(rec, (left - 4.0 * u, y), (right + 4.0 * u, y), &dash),
            None => {}
        }
        if let Some([x, y]) = t.corner {
            let (bx, by) = (x - 20.0 * u, y - 20.0 * u);
            boxed(rec, bx, by, 14.0);
            // The four-way arrow: a cross 8 px across with 2 px heads.
            let (cx, cy, a, h) = (bx + 7.0 * u, by + 7.0 * u, 4.0 * u, 2.0 * u);
            line(rec, (cx - a, cy), (cx + a, cy), &thin);
            line(rec, (cx, cy - a), (cx, cy + a), &thin);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let tip = (cx + dx * a, cy + dy * a);
                let back = (tip.0 - dx * h, tip.1 - dy * h);
                line(rec, tip, (back.0 + dy * h, back.1 + dx * h), &thin);
                line(rec, tip, (back.0 - dy * h, back.1 - dx * h), &thin);
            }
        }
        if let Some([x, y]) = t.end {
            boxed(rec, x, y, 7.0);
        }
    }

    /// Page `i`'s formatting marks and text boundaries, in the palette's marks colour, in page points.
    fn marks(&mut self, rec: &mut Recorder, r: &Renderer, view: &View, i: usize, f: &Frame) {
        if view.formatting_marks.is_empty() && view.boundaries.is_empty() {
            return;
        }
        let px = (f.px_per_pt * f.dpr).max(0.01);
        let ink = r.palette().marks;
        let line = pen(1.0 / px);
        let face = self.fallback.clone();
        for m in view.formatting_marks.iter().filter(|m| m.page == i) {
            let [x0, top, x1, bottom] = m.at;
            let h = bottom - top;
            let mid = top + h * 0.55;
            match m.kind {
                MarkKind::Paragraph | MarkKind::LineBreak => {
                    if let Some(face) = &face {
                        let glyph = if m.kind == MarkKind::Paragraph { "\u{b6}" } else { "\u{21b5}" };
                        self.text(rec, face, glyph, x0 + 0.5, bottom - h * 0.22, h * 0.7, ink);
                    }
                }
                MarkKind::Space => self.draw_shape(rec, kurbo::Circle::new(((x0 + x1) / 2.0, mid), (h * 0.045).max(0.5)), None, ink),
                MarkKind::Tab => {
                    let (a, b) = (x0 + 1.0, (x1 - 1.0).max(x0 + 2.0));
                    let head = (h * 0.15).min(b - a);
                    rec.line((a, mid).into(), (b, mid).into(), &line, ink);
                    rec.line((b - head, mid - head).into(), (b, mid).into(), &line, ink);
                    rec.line((b - head, mid + head).into(), (b, mid).into(), &line, ink);
                }
                MarkKind::Break => rec.line((x0, mid).into(), (x1, mid).into(), &dashed(1.0 / px, 3.0 / px, 2.0 / px), ink),
                MarkKind::Hidden => {
                    let y = bottom - 1.0 / px;
                    rec.line((x0, y).into(), (x1, y).into(), &dashed(1.0 / px, 1.0 / px, 1.0 / px), ink);
                }
            }
        }
        let edge = dashed(1.0 / px, 2.0 / px, 2.0 / px);
        for (_, b) in view.boundaries.iter().filter(|(pg, _)| *pg == i) {
            rec.stroke_path(&rect_path(rect(*b)), &edge, ink.with_alpha(150));
        }
    }
}
