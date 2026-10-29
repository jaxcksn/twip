//! The view's overlays: what editing shows over a page's content (the fade, placeholders and looks, marks,
//! regions, comment tints, the drop caret, the guide line, the selection, the caret). Drawn every frame, never
//! recorded with the page.

use skia_safe::{Canvas, Color, Data, Font, Image, Paint, Point, Rect};

use super::{Frame, Renderer};
use crate::editing::view::{MarkKind, SelectionView, View};

impl Renderer {
    /// Draws page `i`'s overlays; the canvas is in that page's points.
    pub(super) fn overlays(&mut self, canvas: &Canvas, view: &View, i: usize, f: &Frame) {
        let k = f.px_per_pt;
        let mut white = Paint::default();
        white.set_color(Color::WHITE);
        let palette = self.palette;
        let mut highlight = Paint::default();
        highlight.set_color(palette.selection);
        highlight.set_blend_mode(skia_safe::BlendMode::Multiply);
        let mut fade = Paint::default();
        fade.set_color(Color::from_argb(130, 255, 255, 255));
        let mut outline = Paint::default();
        outline.set_color(palette.object);
        outline.set_style(skia_safe::paint::Style::Stroke);
        outline.set_stroke_width(0.75);
        for (_, [x0, y0, x1, y1]) in view.fade.iter().filter(|(pg, _)| *pg == i) {
            canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &fade);
        }
        for ph in view.placeholders.iter().filter(|p| p.page == i) {
            self.placeholder(canvas, ph);
        }
        // Misspellings: a wavy red line along each word's bottom, 1 device pixel wide.
        let px = (k * f.dpr).max(0.01);
        let mut wave = Paint::default();
        wave.set_color(Color::from_rgb(230, 0, 0));
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
            line.set_color(Color::from_rgb(0, 120, 212));
            line.set_stroke_width((1.0 / px) as f32);
            line.set_path_effect(skia_safe::PathEffect::dash(&[(3.0 / px) as f32, (2.0 / px) as f32], 0.0));
            for (_, [x0, y0, x1, y1]) in g.rects.iter().filter(|(pg, _)| *pg == i) {
                canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &line);
            }
            if g.title_rect.0 == i {
                let [x0, y0, x1, y1] = g.title_rect.1;
                let mut tab = Paint::default();
                tab.set_color(Color::from_rgb(0, 120, 212));
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
        // Commented text: an amber tint over each thread's range, stronger for the active thread. A view
        // overlay, never printed.
        for c in &view.comments {
            let mut tint = Paint::default();
            tint.set_color(Color::from_argb(if c.active { 110 } else { 45 }, 255, 185, 0));
            for (_, [x0, y0, x1, y1]) in c.rects.iter().filter(|(pg, _)| *pg == i) {
                canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &tint);
            }
        }
        // Element marks: a tint over the element and a wavy line twice the spelling line's
        // weight, in the severity's colour, so they read apart from misspellings.
        for m in view.marks.iter().filter(|m| m.page == i) {
            let (r, g, b) = match m.severity {
                crate::editing::view::Severity::Error => (209, 52, 56),
                crate::editing::view::Severity::Warning => (193, 156, 0),
                crate::editing::view::Severity::Info => (0, 120, 212),
            };
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
        match &view.selection {
            SelectionView::Text(rects) | SelectionView::Cells(rects) => {
                for (_, [x0, y0, x1, y1]) in rects.iter().filter(|(pg, _)| *pg == i) {
                    canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &highlight);
                }
            }
            SelectionView::Object(rects, handles) => {
                // The outline and eight handles.
                for (_, [x0, y0, x1, y1]) in rects.iter().filter(|(pg, _)| *pg == i) {
                    canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &outline);
                    if !handles {
                        continue;
                    }
                    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
                    for (hx, hy) in [(*x0, *y0), (xm, *y0), (*x1, *y0), (*x1, ym), (*x1, *y1), (xm, *y1), (*x0, *y1), (*x0, ym)] {
                        let h = Rect::from_xywh(hx as f32 - 3.0, hy as f32 - 3.0, 6.0, 6.0);
                        canvas.draw_rect(h, &white);
                        canvas.draw_rect(h, &outline);
                    }
                }
            }
            SelectionView::None => {}
        }
        self.marks(canvas, view, i, f);
        // The caret: a bar at the insertion point, at least one device pixel wide, last so nothing covers it.
        if let Some(c) = view.caret.as_ref().filter(|c| f.caret && c.page == i && matches!(view.selection, SelectionView::None)) {
            let mut bar = Paint::default();
            bar.set_color(palette.caret);
            let width = f.dpr.round().max(1.0) / (k * f.dpr);
            canvas.draw_rect(Rect::new(c.x as f32, c.top as f32, (c.x + width) as f32, c.bottom as f32), &bar);
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
