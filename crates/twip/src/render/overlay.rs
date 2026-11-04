//! The view's overlays: what editing shows over a page's content (the fade, placeholders and looks, marks,
//! regions, comment tints, the drop caret, the guide line, the selection, the caret). Drawn every frame, never
//! recorded with the page.

use skia_safe::{Canvas, Color, Data, Font, Image, Paint, Point, Rect};

use super::{Frame, Renderer};
use crate::editing::view::{MarkKind, SelectionView, View};
use crate::layout::fragment::Item;

impl Renderer {
    /// Draws page `i`'s overlays; the canvas is in that page's points.
    pub(super) fn overlays(&mut self, canvas: &Canvas, view: &View, images: &[Vec<u8>], i: usize, f: &Frame) {
        let k = f.px_per_pt;
        let mut white = Paint::default();
        white.set_color(Color::WHITE);
        let palette = self.palette;
        let mut highlight = Paint::default();
        highlight.set_color(palette.selection);
        // A light page: the tint multiplied under the text, which keeps its colour; a dark one darkens under multiply,
        // so the accent is laid over it.
        if !self.theme.is_dark() {
            highlight.set_blend_mode(skia_safe::BlendMode::Multiply);
        }
        let mut fade = Paint::default();
        fade.set_color(self.ink(0xFF_FFFF).with_a(130));
        let mut outline = Paint::default();
        outline.set_color(palette.object);
        outline.set_style(skia_safe::paint::Style::Stroke);
        outline.set_stroke_width(0.75);
        // Object chrome: 1 px lines, the paper token behind handles.
        let mut hair = outline.clone();
        hair.set_anti_alias(true);
        hair.set_stroke_width((1.0 / k) as f32);
        let mut paper = Paint::default();
        paper.set_anti_alias(true);
        paper.set_color(self.theme.tokens.paper);
        for (_, [x0, y0, x1, y1]) in view.fade.iter().filter(|(pg, _)| *pg == i) {
            canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &fade);
        }
        for ph in view.placeholders.iter().filter(|p| p.page == i) {
            self.placeholder(canvas, ph);
        }
        // Misspellings: a wavy red line along each word's bottom, 1 device pixel wide.
        let px = (k * f.dpr).max(0.01);
        let mut wave = Paint::default();
        // DRAW-27: marks on the page take the page's colours.
        wave.set_color(self.ink(0xE6_0000));
        wave.set_style(skia_safe::paint::Style::Stroke);
        wave.set_anti_alias(true);
        wave.set_stroke_width((1.0 / px) as f32);
        let (step, amp) = (2.0 / px, 1.5 / px);
        for (_, [x0, _, x1, y1]) in view.spelling.iter().filter(|(pg, _)| *pg == i) {
            let y = y1 - 1.5 * amp;
            let mut pts = vec![Point::new(*x0 as f32, y as f32)];
            let mut x = x0 + step;
            while x <= x1 + step / 2.0 {
                pts.push(Point::new(x as f32, (if pts.len() % 2 == 1 { y + amp } else { y }) as f32));
                x += step;
            }
            canvas.draw_points(skia_safe::canvas::PointMode::Polygon, &pts, &wave);
        }
        // Design-mode looks: the host's image scaled into a picture control's box keeping its
        // aspect ratio, or a fill, label and icon drawn as a placeholder's are.
        for l in view.looks.iter().filter(|l| l.page == i) {
            let [x0, y0, x1, y1] = l.rect;
            if let Some(bytes) = &l.image {
                let key = std::rc::Rc::as_ptr(bytes) as usize;
                let img = self.look_images.entry(key).or_insert_with(|| Image::from_encoded(Data::new_copy(bytes))).clone();
                if let Some(img) = img {
                    let (iw, ih) = (img.width() as f64, img.height() as f64);
                    let s = ((x1 - x0) / iw).min((y1 - y0) / ih);
                    let (w, h) = (iw * s, ih * s);
                    let (cx, cy) = (x0 + (x1 - x0 - w) / 2.0, y0 + (y1 - y0 - h) / 2.0);
                    canvas.draw_image_rect(&img, None, Rect::new(cx as f32, cy as f32, (cx + w) as f32, (cy + h) as f32), &Paint::default());
                }
            } else {
                let ph = crate::editing::view::PlaceholderView {
                    page: i,
                    rect: l.rect,
                    color: l.fill.unwrap_or(0),
                    label: l.label.clone().unwrap_or_default(),
                    icon: l.icon.clone(),
                };
                self.placeholder(canvas, &ph);
            }
        }
        // Design-mode regions: a dashed boundary and a title tab above its top left.
        for g in &view.regions {
            let mut line = Paint::default();
            line.set_anti_alias(true);
            line.set_style(skia_safe::paint::Style::Stroke);
            line.set_color(self.ink(0x00_78D4));
            line.set_stroke_width((1.0 / px) as f32);
            line.set_path_effect(skia_safe::PathEffect::dash(&[(3.0 / px) as f32, (2.0 / px) as f32], 0.0));
            for (_, [x0, y0, x1, y1]) in g.rects.iter().filter(|(pg, _)| *pg == i) {
                canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &line);
            }
            if g.title_rect.0 == i {
                let [x0, y0, x1, y1] = g.title_rect.1;
                let mut tab = Paint::default();
                tab.set_color(self.ink(0x00_78D4));
                let r = Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
                canvas.draw_rect(r, &tab);
                if let Some(tf) = self.typefaces.values().flatten().next().cloned() {
                    let font = Font::from_typeface(tf, 9.0);
                    let mut text = Paint::default();
                    text.set_anti_alias(true);
                    text.set_color(Color::WHITE);
                    canvas.save();
                    canvas.clip_rect(r, None, true);
                    canvas.draw_str(&g.title, ((x0 + 4.0) as f32, (y1 - 4.0) as f32), &font, &text);
                    canvas.restore();
                }
            }
        }
        // The correction just made: a dashed accent underline, and its options button (16 x 12 px) under it.
        if let Some(c) = &view.correction {
            let mut dash = Paint::default();
            dash.set_anti_alias(true);
            dash.set_style(skia_safe::paint::Style::Stroke);
            dash.set_color(self.theme.accent.solid);
            dash.set_stroke_width(0.75);
            dash.set_path_effect(skia_safe::PathEffect::dash(&[2.25, 1.5], 0.0));
            for (_, [x0, _, x1, y1]) in c.rects.iter().filter(|(pg, _)| *pg == i) {
                canvas.draw_line((*x0 as f32, (*y1 - 0.5) as f32), (*x1 as f32, (*y1 - 0.5) as f32), &dash);
            }
            if let Some((_, [x0, y0, x1, y1])) = c.button.filter(|(pg, _)| *pg == i) {
                let r = Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
                let mut fill = Paint::default();
                fill.set_color(self.theme.tokens.paper);
                canvas.draw_rect(r, &fill);
                let mut edge = Paint::default();
                edge.set_anti_alias(true);
                edge.set_style(skia_safe::paint::Style::Stroke);
                edge.set_color(self.theme.accent.solid);
                edge.set_stroke_width(0.75);
                canvas.draw_rect(r, &edge);
                if let Some(tf) = super::chrome_typeface(super::ChromeFace::Mono500) {
                    let font = Font::from_typeface(tf, 6.0);
                    let mut ink = Paint::default();
                    ink.set_anti_alias(true);
                    ink.set_color(self.theme.accent.solid);
                    let (w, _) = font.measure_str("\u{25BE}", None);
                    canvas.draw_str("\u{25BE}", (((x0 + x1) as f32 - w) / 2.0, (y1 - 2.25) as f32), &font, &ink);
                }
            }
        }
        // Commented text: an amber tint over each thread's range, stronger for the active thread. A view
        // overlay, never printed.
        for c in &view.comments {
            let mut tint = Paint::default();
            tint.set_color(self.ink(0xFF_B900).with_a(if c.active { 110 } else { 45 }));
            for (_, [x0, y0, x1, y1]) in c.rects.iter().filter(|(pg, _)| *pg == i) {
                canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &tint);
            }
        }
        // Element marks: a tint over the element and a wavy line twice the spelling line's
        // weight, in the severity's colour, so they read apart from misspellings.
        for m in view.marks.iter().filter(|m| m.page == i) {
            let c = self.ink(match m.severity {
                crate::editing::view::Severity::Error => 0xD1_3438,
                crate::editing::view::Severity::Warning => 0xC1_9C00,
                crate::editing::view::Severity::Info => 0x00_78D4,
            });
            let (r, g, b) = (c.r(), c.g(), c.b());
            let [x0, y0, x1, y1] = m.rect;
            let mut tint = Paint::default();
            tint.set_color(Color::from_argb(28, r, g, b));
            canvas.draw_rect(Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32), &tint);
            let mut line = wave.clone();
            line.set_color(Color::from_rgb(r, g, b));
            line.set_stroke_width((2.0 / px) as f32);
            let y = y1 - 1.5 * amp;
            let mut pts = vec![Point::new(x0 as f32, y as f32)];
            let mut x = x0 + step;
            while x <= x1 + step / 2.0 {
                pts.push(Point::new(x as f32, (if pts.len() % 2 == 1 { y + amp } else { y }) as f32));
                x += step;
            }
            canvas.draw_points(skia_safe::canvas::PointMode::Polygon, &pts, &line);
        }
        if let Some(c) = view.drop.as_ref().filter(|c| c.page == i) {
            let mut dash = outline.clone();
            dash.set_color(palette.drop);
            dash.set_path_effect(skia_safe::PathEffect::dash(&[1.5, 1.5], 0.0));
            canvas.draw_line((c.x as f32, c.top as f32), (c.x as f32, c.bottom as f32), &dash);
        }
        if let Some((_, [x0, y0, x1, y1])) = view.guide.filter(|(pg, _)| *pg == i) {
            canvas.draw_line((x0 as f32, y0 as f32), (x1 as f32, y1 as f32), &outline);
        }
        // An object drag's ghost: the object's own drawing at half opacity, mapped into the box the drag puts it in.
        if let Some(g) = view.ghost.filter(|g| g.page == i)
            && let Some(page) = view.snapshot.pages().get(i)
        {
            let ([a0, b0, a1, b1], [c0, d0, c1, d1]) = (g.from, g.to);
            let items = match g.float {
                Some(fl) => crate::layout::traverse::paint_float(page, fl),
                None => crate::layout::traverse::paint_items(page)
                    .into_iter()
                    .filter(|it| {
                        let r = match it {
                            Item::Image { rect, .. } => *rect,
                            Item::Shape(s) => s.rect,
                            _ => return false,
                        };
                        r[0] >= a0 - 0.5 && r[1] >= b0 - 0.5 && r[2] <= a1 + 0.5 && r[3] <= b1 + 0.5
                    })
                    .collect(),
            };
            canvas.save_layer_alpha_f(None, 0.5);
            canvas.translate((c0 as f32, d0 as f32));
            canvas.scale((((c1 - c0) / (a1 - a0).max(0.01)) as f32, ((d1 - d0) / (b1 - b0).max(0.01)) as f32));
            canvas.translate((-a0 as f32, -b0 as f32));
            for it in &items {
                self.item(canvas, it, images);
            }
            canvas.restore();
        }
        // The selected float's anchor: an 8 px ring (1.5 px stroke) with a 1.5 x 6 px stem below it, 18 px left of
        // its paragraph's first line (at least 10 px from the page's edge), 8 px below the line's top.
        if let Some((_, [x, y])) = view.anchor.filter(|(pg, _)| *pg == i) {
            let u = 1.0 / k;
            let (cx, cy) = ((x - 18.0 * u).max(10.0 * u), y + 8.0 * u);
            let mut ring = outline.clone();
            ring.set_anti_alias(true);
            ring.set_stroke_width((1.5 * u) as f32);
            canvas.draw_circle((cx as f32, cy as f32), (3.25 * u) as f32, &ring);
            let mut stem = Paint::default();
            stem.set_color(palette.object);
            canvas.draw_rect(Rect::from_xywh((cx - 0.75 * u) as f32, (cy + 4.0 * u) as f32, (1.5 * u) as f32, (6.0 * u) as f32), &stem);
        }
        match &view.selection {
            SelectionView::Text(rects) | SelectionView::Cells(rects) => {
                for (_, [x0, y0, x1, y1]) in rects.iter().filter(|(pg, _)| *pg == i) {
                    canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &highlight);
                }
            }
            SelectionView::Object(rects, handles) => {
                // A 1 px outline and eight 8 px handles, paper filled with a 1 px border.
                for (_, [x0, y0, x1, y1]) in rects.iter().filter(|(pg, _)| *pg == i) {
                    canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &hair);
                    if !handles {
                        continue;
                    }
                    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                    let r = (4.0 / k) as f32;
                    for (hx, hy) in [(*x0, *y0), (xm, *y0), (*x1, *y0), (*x1, ym), (*x1, *y1), (xm, *y1), (*x0, *y1), (*x0, ym)] {
                        let h = Rect::from_xywh(hx as f32 - r, hy as f32 - r, 2.0 * r, 2.0 * r);
                        canvas.draw_rect(h, &paper);
                        canvas.draw_rect(h, &hair);
                    }
                }
            }
            SelectionView::None => {}
        }
        // A selected shape: its turned outline, its rotate knob (10 px, paper filled) on a 20 px stem from its top,
        // its adjust handles as yellow diamonds.
        for s in view.shapes.iter().filter(|s| s.page == i) {
            let [x0, y0, x1, y1] = s.rect;
            if s.rot != 0.0 {
                canvas.save();
                canvas.rotate(s.rot as f32, Some(((x0 + x1) as f32 / 2.0, (y0 + y1) as f32 / 2.0).into()));
                canvas.draw_rect(Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32), &hair);
                canvas.restore();
            }
            let pt = |p: [f64; 2]| skia_safe::Point::new(p[0] as f32, p[1] as f32);
            // The knob's centre is 24 px from the top; the stem runs 20 px of that.
            let stem = [s.top[0] + (s.rotate[0] - s.top[0]) * 20.0 / 24.0, s.top[1] + (s.rotate[1] - s.top[1]) * 20.0 / 24.0];
            canvas.draw_line(pt(s.top), pt(stem), &hair);
            let knob = (5.0 / k) as f32;
            canvas.draw_circle(pt(s.rotate), knob, &paper);
            canvas.draw_circle(pt(s.rotate), knob, &hair);
            let mut fill = white.clone();
            fill.set_color(skia_safe::Color::from_rgb(0xFF, 0xD7, 0x00));
            let r = (4.0 / k) as f32;
            for a in &s.adjust {
                let mut d = skia_safe::PathBuilder::new();
                let [x, y] = *a;
                d.move_to((x as f32, y as f32 - r))
                    .line_to((x as f32 + r, y as f32))
                    .line_to((x as f32, y as f32 + r))
                    .line_to((x as f32 - r, y as f32))
                    .close();
                let d = d.detach();
                canvas.draw_path(&d, &fill);
                canvas.draw_path(&d, &hair);
            }
        }
        self.table_affordances(canvas, view, i, f);
        self.object_feedback(canvas, view, i, f);
        self.marks(canvas, view, i, f);
        // The caret: a bar at the insertion point, at least one device pixel wide, last so nothing covers it.
        if let Some(c) = view.caret.as_ref().filter(|c| f.caret && c.page == i && matches!(view.selection, SelectionView::None)) {
            let mut bar = Paint::default();
            bar.set_color(palette.caret);
            let width = f.dpr.round().max(1.0) / (k * f.dpr);
            canvas.draw_rect(Rect::new(c.x as f32, c.top as f32, (c.x + width) as f32, c.bottom as f32), &bar);
        }
    }

    /// Page `i`'s object drag feedback: the dot grid, dashed guides, the dotted anchor line, the original box dashed,
    /// dimension lines and chips. The canvas is in page points; sizes are CSS px.
    fn object_feedback(&mut self, canvas: &Canvas, view: &View, i: usize, f: &Frame) {
        use crate::editing::view::{Axis, Measure, Tone};
        let u = 1.0 / f.px_per_pt.max(0.01);
        let (t, accent) = (self.theme.tokens, self.palette.object);
        let line = |c: Color, dash: Option<[f64; 2]>| {
            let mut p = Paint::default();
            p.set_anti_alias(true);
            p.set_style(skia_safe::paint::Style::Stroke);
            p.set_color(c);
            p.set_stroke_width(u as f32);
            if let Some([on, off]) = dash {
                p.set_path_effect(skia_safe::PathEffect::dash(&[(on * u) as f32, (off * u) as f32], 0.0));
            }
            p
        };
        let fill = |c: Color| {
            let mut p = Paint::default();
            p.set_anti_alias(true);
            p.set_color(c);
            p
        };
        for (_, g) in view.guides.iter().filter(|(p, _)| *p == i) {
            if let (Some(grid), Some(page)) = (g.grid, view.snapshot.pages().get(i)) {
                let (step, dot) = (grid.step.0.max(1.0), fill(t.ink3));
                let first = |o: f64| o - (o / step).floor() * step;
                let mut y = first(grid.origin[1].0);
                while y <= page.h {
                    let mut x = first(grid.origin[0].0);
                    while x <= page.w {
                        canvas.draw_rect(Rect::from_xywh((x - u / 2.0) as f32, (y - u / 2.0) as f32, u as f32, u as f32), &dot);
                        x += step;
                    }
                    y += step;
                }
            }
            let dashed = line(accent, Some([3.0, 3.0]));
            for l in &g.lines {
                let (a, b) = (l.span.0.0 as f32, l.span.1.0 as f32);
                match l.axis {
                    Axis::X => canvas.draw_line((l.at.0 as f32, a), (l.at.0 as f32, b), &dashed),
                    Axis::Y => canvas.draw_line((a, l.at.0 as f32), (b, l.at.0 as f32), &dashed),
                };
            }
            if let Some([x0, y0, x1, y1]) = g.anchor_link {
                canvas.draw_line((x0.0 as f32, y0.0 as f32), (x1.0 as f32, y1.0 as f32), &line(accent, Some([1.0, 1.0])));
            }
        }
        if let Some(o) = view.original.filter(|o| o.page == i) {
            let [x0, y0, x1, y1] = o.rect;
            canvas.save();
            canvas.rotate(o.rot as f32, Some((((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32).into()));
            canvas.draw_rect(Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32), &line(t.ink3, Some([3.0, 3.0])));
            canvas.restore();
        }
        let (Some(mono5), Some(mono6)) = (super::chrome_typeface(super::ChromeFace::Mono500), super::chrome_typeface(super::ChromeFace::Mono600))
        else {
            return;
        };
        let (chip_font, label_font) = (Font::from_typeface(mono5, (10.0 * u) as f32), Font::from_typeface(mono6, (10.0 * u) as f32));
        let tone = |t: Tone| match t {
            Tone::Accent => accent,
            Tone::Muted => self.theme.tokens.ink2,
        };
        for (_, m) in view.measures.iter().filter(|(p, _)| *p == i) {
            match m {
                // A line with 7 px end ticks, its label over the middle on the paper.
                Measure::Dimension { from, to, label, tone: c } => {
                    let (c, (x0, y0), (x1, y1)) = (tone(*c), (from[0].0, from[1].0), (to[0].0, to[1].0));
                    let ink = line(c, None);
                    canvas.draw_line((x0 as f32, y0 as f32), (x1 as f32, y1 as f32), &ink);
                    for (x, y) in [(x0, y0), (x1, y1)] {
                        canvas.draw_line((x as f32, (y - 3.5 * u) as f32), (x as f32, (y + 3.5 * u) as f32), &ink);
                    }
                    let w = f64::from(label_font.measure_str(label, None).0);
                    let (mx, top) = ((x0 + x1) / 2.0 - w / 2.0, y0 - 5.0 * u);
                    canvas.draw_rect(Rect::from_xywh((mx - 4.0 * u) as f32, top as f32, (w + 8.0 * u) as f32, (10.0 * u) as f32), &fill(t.paper));
                    canvas.draw_str(label, (mx as f32, (top + 8.0 * u) as f32), &label_font, &fill(c));
                }
                // The ink chip with paper text: 4 x 7 px padding, 2 px corners.
                Measure::Chip { at, label } => {
                    let (x, y) = (at[0].pt().0, at[1].pt().0);
                    let w = f64::from(chip_font.measure_str(label, None).0);
                    let r = Rect::from_xywh(x as f32, y as f32, (w + 14.0 * u) as f32, (18.0 * u) as f32);
                    canvas.draw_round_rect(r, (2.0 * u) as f32, (2.0 * u) as f32, &fill(t.ink));
                    canvas.draw_str(label, ((x + 7.0 * u) as f32, (y + 12.5 * u) as f32), &chip_font, &fill(t.paper));
                }
                Measure::Status { at, label, tone: c } => {
                    canvas.draw_str(label, (at[0].0 as f32, (at[1].0 + 8.0 * u) as f32), &label_font, &fill(tone(*c)));
                }
                // The ruler draws its own hints.
                Measure::Hint { .. } => {}
            }
        }
    }

    /// Page `i`'s table affordances: a resize's or move's new box (dashed, the accent's soft tint inside); the shown
    /// table's move handle (14 px, paper filled, a four-way arrow) 20 px above and left of it and its resize handle (7 px)
    /// at its lower right, both with a 1 px accent border; under the pointer, a "+" (14 px, accent filled) with a dashed
    /// line where the column or row goes, an accent arrow (10 × 7 px) at a column or row it selects, or a dashed line
    /// down a column border. The canvas is in page points; sizes are CSS px.
    fn table_affordances(&mut self, canvas: &Canvas, view: &View, i: usize, f: &Frame) {
        use crate::editing::view::TableMark;
        let u = 1.0 / f.px_per_pt.max(0.01);
        let (accent, solid, paper) = (self.theme.accent, self.palette.object, self.theme.tokens.paper);
        let paint = |c: Color, stroke: bool| {
            let mut p = Paint::default();
            p.set_anti_alias(true);
            p.set_color(c);
            if stroke {
                p.set_style(skia_safe::paint::Style::Stroke);
                p.set_stroke_width(u as f32);
            }
            p
        };
        let mut dashed = paint(solid, true);
        dashed.set_path_effect(skia_safe::PathEffect::dash(&[(3.0 * u) as f32, (3.0 * u) as f32], 0.0));
        let line = |a: (f64, f64), b: (f64, f64), p: &Paint| canvas.draw_line((a.0 as f32, a.1 as f32), (b.0 as f32, b.1 as f32), p);
        // A box `s` px square at a page point, paper filled with an accent border.
        let boxed = |x: f64, y: f64, s: f64| {
            let r = Rect::from_xywh(x as f32, y as f32, (s * u) as f32, (s * u) as f32);
            canvas.draw_rect(r, &paint(paper, false));
            canvas.draw_rect(r, &paint(solid, true));
        };
        // The "+" button: accent filled, its cross in the colour drawn on the accent.
        let plus = |x: f64, y: f64| {
            canvas.draw_rect(Rect::from_xywh(x as f32, y as f32, (14.0 * u) as f32, (14.0 * u) as f32), &paint(solid, false));
            let mut cross = paint(accent.on, true);
            cross.set_stroke_width((1.5 * u) as f32);
            let (cx, cy) = (x + 7.0 * u, y + 7.0 * u);
            line((cx - 3.5 * u, cy), (cx + 3.5 * u, cy), &cross);
            line((cx, cy - 3.5 * u), (cx, cy + 3.5 * u), &cross);
        };
        let triangle = |pts: [(f64, f64); 3]| {
            let mut b = skia_safe::PathBuilder::new();
            b.move_to((pts[0].0 as f32, pts[0].1 as f32))
                .line_to((pts[1].0 as f32, pts[1].1 as f32))
                .line_to((pts[2].0 as f32, pts[2].1 as f32))
                .close();
            canvas.draw_path(&b.detach(), &paint(solid, false));
        };
        if let Some((_, [x0, y0, x1, y1])) = view.table_preview.filter(|(p, _)| *p == i) {
            let r = Rect::new(x0 as f32, y0 as f32, x1 as f32, y1 as f32);
            canvas.draw_rect(r, &paint(accent.soft, false));
            canvas.draw_rect(r, &dashed);
        }
        let Some(t) = view.table.as_ref().filter(|t| t.page == i) else { return };
        match t.mark {
            Some(TableMark::InsertColumn { x, top, bottom }) => {
                _ = line((x, top - 4.0 * u), (x, bottom + 4.0 * u), &dashed);
                plus(x - 7.0 * u, top - 20.0 * u);
            }
            Some(TableMark::InsertRow { y, left, right }) => {
                line((left - 6.0 * u, y), (right, y), &dashed);
                plus(left - 22.0 * u, y - 7.0 * u);
            }
            Some(TableMark::SelectColumn { x0, x1, top }) => {
                let (cx, tip) = ((x0 + x1) / 2.0, top - 6.0 * u);
                triangle([(cx - 5.0 * u, tip - 7.0 * u), (cx + 5.0 * u, tip - 7.0 * u), (cx, tip)]);
            }
            Some(TableMark::SelectRow { y0, y1, left }) => {
                let (cy, tip) = ((y0 + y1) / 2.0, left - 6.0 * u);
                triangle([(tip - 7.0 * u, cy - 5.0 * u), (tip - 7.0 * u, cy + 5.0 * u), (tip, cy)]);
            }
            Some(TableMark::Border { x, top, bottom }) => _ = line((x, top - 4.0 * u), (x, bottom + 4.0 * u), &dashed),
            None => {}
        }
        if let Some([x, y]) = t.corner {
            let (bx, by) = (x - 20.0 * u, y - 20.0 * u);
            boxed(bx, by, 14.0);
            // The four-way arrow: a cross 8 px across with 2 px heads.
            let ink = paint(solid, true);
            let (cx, cy, a, h) = (bx + 7.0 * u, by + 7.0 * u, 4.0 * u, 2.0 * u);
            line((cx - a, cy), (cx + a, cy), &ink);
            line((cx, cy - a), (cx, cy + a), &ink);
            for (dx, dy) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let tip = (cx + dx * a, cy + dy * a);
                let back = (tip.0 - dx * h, tip.1 - dy * h);
                line(tip, (back.0 + dy * h, back.1 + dx * h), &ink);
                line(tip, (back.0 - dy * h, back.1 - dx * h), &ink);
            }
        }
        if let Some([x, y]) = t.end {
            boxed(x, y, 7.0);
        }
    }

    /// Page `i`'s formatting marks and text boundaries, in the palette's marks colour; the canvas is in page points.
    fn marks(&mut self, canvas: &Canvas, view: &View, i: usize, f: &Frame) {
        if view.formatting_marks.is_empty() && view.boundaries.is_empty() {
            return;
        }
        let px = (f.px_per_pt * f.dpr).max(0.01);
        let mut ink = Paint::default();
        ink.set_anti_alias(true);
        ink.set_color(self.palette.marks);
        let mut line = ink.clone();
        line.set_style(skia_safe::paint::Style::Stroke);
        line.set_stroke_width((1.0 / px) as f32);
        let typeface = self.typefaces.values().flatten().next().cloned();
        for m in view.formatting_marks.iter().filter(|m| m.page == i) {
            let [x0, top, x1, bottom] = m.at;
            let h = bottom - top;
            let mid = top + h * 0.55;
            match m.kind {
                MarkKind::Paragraph | MarkKind::LineBreak => {
                    if let Some(tf) = &typeface {
                        let font = Font::from_typeface(tf.clone(), (h * 0.7) as f32);
                        let glyph = if m.kind == MarkKind::Paragraph { "\u{b6}" } else { "\u{21b5}" };
                        canvas.draw_str(glyph, ((x0 + 0.5) as f32, (bottom - h * 0.22) as f32), &font, &ink);
                    }
                }
                MarkKind::Space => {
                    canvas.draw_circle((((x0 + x1) / 2.0) as f32, mid as f32), (h * 0.045).max(0.5) as f32, &ink);
                }
                MarkKind::Tab => {
                    let (a, b) = (x0 + 1.0, (x1 - 1.0).max(x0 + 2.0));
                    let head = (h * 0.15).min(b - a);
                    canvas.draw_line((a as f32, mid as f32), (b as f32, mid as f32), &line);
                    canvas.draw_line(((b - head) as f32, (mid - head) as f32), (b as f32, mid as f32), &line);
                    canvas.draw_line(((b - head) as f32, (mid + head) as f32), (b as f32, mid as f32), &line);
                }
                MarkKind::Break => {
                    let mut dash = line.clone();
                    dash.set_path_effect(skia_safe::PathEffect::dash(&[(3.0 / px) as f32, (2.0 / px) as f32], 0.0));
                    canvas.draw_line((x0 as f32, mid as f32), (x1 as f32, mid as f32), &dash);
                }
                MarkKind::Hidden => {
                    let mut dots = line.clone();
                    dots.set_path_effect(skia_safe::PathEffect::dash(&[(1.0 / px) as f32, (1.0 / px) as f32], 0.0));
                    let y = bottom - 1.0 / px;
                    canvas.draw_line((x0 as f32, y as f32), (x1 as f32, y as f32), &dots);
                }
            }
        }
        let mut edge = line.clone();
        edge.set_alpha(150);
        edge.set_path_effect(skia_safe::PathEffect::dash(&[(2.0 / px) as f32, (2.0 / px) as f32], 0.0));
        for (_, [x0, y0, x1, y1]) in view.boundaries.iter().filter(|(pg, _)| *pg == i) {
            canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &edge);
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::editing::tests::scenarios::Run;
    use crate::render::{Frame, Renderer};

    #[test]
    fn a_dragged_float_draws_its_ghost_where_the_drag_puts_it() {
        // The frame with the drag's ghost differs from the one without only inside the box the drag puts it in.
        let mut r = Run::open("FX-OBJ.docx", "ghost-pixels");
        r.e.live_layout = false;
        r.step(&json!({ "click": { "float": 1 } }));
        r.step(&json!({ "press": { "float": 1 } }));
        r.step(&json!({ "move_to": { "float": 1, "dy": -150 } }));
        let f = Frame { origin_x: 0.0, origin_y: 0.0, width: 640.0, height: 820.0, dpr: 1.0, px_per_pt: 1.0, caret: false };
        let mut view = r.e.view(0..1);
        let g = view.ghost.expect("a ghost");
        let images = r.e.document().images.clone();
        let pixels = |view: &crate::editing::view::View| {
            let mut s = skia_safe::surfaces::raster_n32_premul((640, 820)).expect("a raster surface");
            Renderer::default().draw(s.canvas(), view, &images, &f);
            let info = s.image_info();
            let mut px = vec![0u8; 640 * 820 * 4];
            assert!(s.read_pixels(&info, &mut px, 640 * 4, (0, 0)));
            px
        };
        let with = pixels(&view);
        view.ghost = None;
        let without = pixels(&view);
        let [ox, oy, ..] = view.stack.pages[0];
        let [x0, y0, x1, y1] = g.to;
        let (mut inside, mut outside) = (0, 0);
        for y in 0..820usize {
            for x in 0..640usize {
                let k = (y * 640 + x) * 4;
                if with[k..k + 4] != without[k..k + 4] {
                    let (px, py) = (x as f64 - ox, y as f64 - oy);
                    if px >= x0 - 2.0 && px <= x1 + 2.0 && py >= y0 - 2.0 && py <= y1 + 2.0 {
                        inside += 1;
                    } else {
                        outside += 1;
                    }
                }
            }
        }
        assert!(inside > 100 && outside == 0, "ghost pixels: {inside} inside {:?}, {outside} outside", g.to);
    }

    #[test]
    fn table_handles_draw_where_the_view_puts_them() {
        // A table at (100, 100)-(300, 200): the move handle 20 px above and left of it, the resize handle at its lower
        // right, a "+" above x 150 with its dashed line down the table, and a resize preview. At 1 px per point each
        // changes pixels in its own box, and only there.
        use crate::editing::view::{TableHandles, TableMark};
        let mut r = Run::open("FX-OBJ.docx", "table-pixels");
        let f = Frame { origin_x: 0.0, origin_y: 0.0, width: 640.0, height: 820.0, dpr: 1.0, px_per_pt: 1.0, caret: false };
        let images = r.e.document().images.clone();
        let pixels = |view: &crate::editing::view::View| {
            let mut s = skia_safe::surfaces::raster_n32_premul((640, 820)).expect("a raster surface");
            Renderer::default().draw(s.canvas(), view, &images, &f);
            let info = s.image_info();
            let mut px = vec![0u8; 640 * 820 * 4];
            assert!(s.read_pixels(&info, &mut px, 640 * 4, (0, 0)));
            px
        };
        let plain = r.e.view(0..1);
        let [ox, oy, ..] = plain.stack.pages[0];
        let changed = |view: &crate::editing::view::View| {
            let (a, b) = (pixels(&plain), pixels(view));
            (0..820usize)
                .flat_map(|y| (0..640usize).map(move |x| (x, y)))
                .filter(|&(x, y)| a[(y * 640 + x) * 4..][..4] != b[(y * 640 + x) * 4..][..4])
                .map(|(x, y)| (x as f64 - ox, y as f64 - oy))
                .collect::<Vec<_>>()
        };
        let inside = |(x, y): (f64, f64), [x0, y0, x1, y1]: [f64; 4]| x >= x0 - 1.5 && x <= x1 + 1.5 && y >= y0 - 1.5 && y <= y1 + 1.5;
        let mut view = plain.clone();
        let mark = TableMark::InsertColumn { x: 150.0, top: 100.0, bottom: 200.0 };
        view.table = Some(TableHandles { page: 0, corner: Some([100.0, 100.0]), end: Some([300.0, 200.0]), mark: Some(mark) });
        let boxes = [[80.0, 80.0, 94.0, 94.0], [300.0, 200.0, 307.0, 207.0], [143.0, 80.0, 157.0, 94.0], [150.0, 96.0, 150.0, 204.0]];
        let d = changed(&view);
        assert!(d.iter().all(|p| boxes.iter().any(|b| inside(*p, *b))), "{:?}", d.iter().find(|p| !boxes.iter().any(|b| inside(**p, *b))));
        assert!(boxes.iter().all(|b| d.iter().filter(|p| inside(**p, *b)).count() > 10), "each one draws");
        let mut preview = plain.clone();
        preview.table_preview = Some((0, [100.0, 300.0, 250.0, 350.0]));
        let d = changed(&preview);
        assert!(d.len() > 5000 && d.iter().all(|p| inside(*p, [100.0, 300.0, 250.0, 350.0])), "{} pixels", d.len());
    }

    #[test]
    fn guides_the_grid_and_chips_draw_where_the_view_puts_them() {
        // A guide down x 100, the grid, and a chip at (225, 225) pt: each changes pixels there, and only there.
        use crate::editing::view::{Axis, Grid, GuideLine, Guides, Measure, Pt, Px};
        let mut r = Run::open("FX-OBJ.docx", "guide-pixels");
        let f = Frame { origin_x: 0.0, origin_y: 0.0, width: 640.0, height: 820.0, dpr: 1.0, px_per_pt: 1.0, caret: false };
        let images = r.e.document().images.clone();
        let pixels = |view: &crate::editing::view::View| {
            let mut s = skia_safe::surfaces::raster_n32_premul((640, 820)).expect("a raster surface");
            Renderer::default().draw(s.canvas(), view, &images, &f);
            let info = s.image_info();
            let mut px = vec![0u8; 640 * 820 * 4];
            assert!(s.read_pixels(&info, &mut px, 640 * 4, (0, 0)));
            px
        };
        let plain = r.e.view(0..1);
        let [ox, oy, ..] = plain.stack.pages[0];
        let changed = |view: &crate::editing::view::View| {
            let (a, b) = (pixels(&plain), pixels(view));
            (0..820usize)
                .flat_map(|y| (0..640usize).map(move |x| (x, y)))
                .filter(|&(x, y)| a[(y * 640 + x) * 4..][..4] != b[(y * 640 + x) * 4..][..4])
                .map(|(x, y)| (x as f64 - ox, y as f64 - oy))
                .collect::<Vec<_>>()
        };
        let mut guide = plain.clone();
        guide.guides = vec![(0, Guides { lines: vec![GuideLine { axis: Axis::X, at: Pt(100.0), span: (Pt(0.0), Pt(700.0)) }], ..Guides::default() })];
        let d = changed(&guide);
        assert!(d.len() > 100 && d.iter().all(|(x, _)| (x - 100.0).abs() <= 1.0), "{} pixels, {:?}", d.len(), d.first());
        let mut grid = plain.clone();
        grid.guides = vec![(0, Guides { grid: Some(Grid { origin: [Pt(72.0), Pt(72.0)], step: Pt(18.0) }), ..Guides::default() })];
        assert!(changed(&grid).len() > 300, "the dots");
        let mut chip = plain.clone();
        chip.measures = vec![(0, Measure::Chip { at: [Px(300.0), Px(300.0)], label: "2.50 × 1.63 in".into() })];
        let d = changed(&chip);
        assert!(
            d.len() > 100 && d.iter().all(|(x, y)| (224.0..345.0).contains(x) && (224.0..244.0).contains(y)),
            "{} pixels, {:?}",
            d.len(),
            d.last()
        );
    }
}
