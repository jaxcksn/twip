//! The renderer: paints snapshot pages and the editor's view overlays with Skia onto a host canvas.
//! It reads layout's paint traversal and `editing::view` only, and holds no
//! editing state: everything it draws for the editor comes from one [`View`], built from the snapshot it paints.

use std::collections::HashMap;

#[cfg(target_os = "emscripten")]
pub mod web;

use skia_safe::{Canvas, Color, Data, Font, FontMgr, Image, Paint, Point, RRect, Rect, Typeface, font::Edging};

use crate::editing::view::{SelectionView, View};
use crate::font::Face;
use crate::layout::fragment::{ImageKey, Item};
use crate::layout::traverse::paint;

/// Where and how a frame is drawn: the scroll origin (CSS px), the canvas size, device pixels per CSS pixel and CSS
/// pixels per point.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub origin_x: f64,
    pub origin_y: f64,
    pub width: f64,
    pub height: f64,
    pub dpr: f64,
    pub px_per_pt: f64,
}

/// Skia objects made from the layout's faces and the document's pictures, kept between frames.
#[derive(Default)]
pub struct Renderer {
    typefaces: HashMap<(usize, u32), Option<Typeface>>,
    images: HashMap<ImageKey, Option<Image>>,
    /// Placeholder icons as Skia paths, by the icon's address.
    icons: HashMap<usize, Vec<(skia_safe::Path, Option<u32>)>>,
    /// The host's look images, decoded once, by the bytes' address.
    look_images: HashMap<usize, Option<Image>>,
    pub background: Option<Color>,
    /// Table gridlines (cell edges without borders) show, on screen only.
    pub gridlines: bool,
    /// Glyph runs carry their characters (text blobs with clusters), so a PDF's text extracts as the document's.
    pub with_text: bool,
}

pub fn rgb(c: u32) -> Color {
    Color::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

impl Renderer {
    /// Forgets decoded pictures (a new document).
    pub fn clear_images(&mut self) {
        self.images.clear();
    }

    /// Draws `face`'s glyphs from `data` (a single-face font file) instead of its own file: PDF export's subsets.
    pub fn set_face_data(&mut self, face: &Face, data: &[u8]) {
        let key = (std::sync::Arc::as_ptr(&face.data) as usize, face.index);
        self.typefaces.insert(key, FontMgr::new().new_from_data(Data::new_copy(data), 0));
    }

    fn typeface(&mut self, face: &Face) -> Option<Typeface> {
        let key = (std::sync::Arc::as_ptr(&face.data) as usize, face.index);
        self.typefaces.entry(key).or_insert_with(|| FontMgr::new().new_from_data(Data::new_copy(&face.data), face.index)).clone()
    }

    /// The pages a frame shows.
    pub fn visible(view: &View, f: &Frame) -> std::ops::Range<usize> {
        let k = f.px_per_pt;
        let (top, bottom) = (-f.origin_y, -f.origin_y + f.height / f.dpr);
        let shown: Vec<usize> = (0..view.stack.pages.len())
            .filter(|&i| {
                let [_, y, _, h] = view.stack.pages[i];
                !((y + h) * k < top || y * k > bottom)
            })
            .collect();
        match (shown.first(), shown.last()) {
            (Some(&a), Some(&b)) => a..b + 1,
            _ => 0..0,
        }
    }

    /// Draws a frame: the visible pages, then the view's overlays over them.
    pub fn draw(&mut self, canvas: &Canvas, view: &View, images: &[Vec<u8>], f: &Frame) {
        canvas.clear(self.background.unwrap_or(Color::from_rgb(0xf3, 0xf3, 0xf3)));
        canvas.save();
        canvas.scale((f.dpr as f32, f.dpr as f32));
        canvas.translate((f.origin_x as f32, f.origin_y as f32));
        let k = f.px_per_pt;
        let mut shadow = Paint::default();
        shadow.set_anti_alias(true);
        let mut white = Paint::default();
        white.set_color(Color::WHITE);
        let mut highlight = Paint::default();
        highlight.set_color(Color::from_argb(90, 0, 120, 212));
        let mut fade = Paint::default();
        fade.set_color(Color::from_argb(130, 255, 255, 255));
        let mut outline = Paint::default();
        outline.set_color(Color::from_rgb(0x60, 0x60, 0x60));
        outline.set_style(skia_safe::paint::Style::Stroke);
        outline.set_stroke_width(0.75);
        for i in view.pages_visible.clone() {
            let Some(page) = view.snapshot.pages().get(i) else { continue };
            let [x, y, w, h] = view.stack.pages[i];
            let r = Rect::from_xywh((x * k) as f32, (y * k) as f32, (w * k) as f32, (h * k) as f32);
            for (inflate, drop, alpha) in [(4.0, 3.0, 6), (3.0, 2.5, 10), (2.0, 2.0, 14), (1.0, 1.0, 22)] {
                shadow.set_color(Color::from_argb(alpha, 0, 0, 0));
                let s = Rect::new(r.left - inflate, r.top - inflate + drop, r.right + inflate, r.bottom + inflate + drop);
                canvas.draw_rrect(RRect::new_rect_xy(s, inflate + 2.0, inflate + 2.0), &shadow);
            }
            canvas.draw_rect(r, &white);
            canvas.save();
            canvas.translate((r.left, r.top));
            canvas.scale((k as f32, k as f32));
            // Gridlines: dashed light blue cell edges, 1 device pixel, under the content (borders paint over them).
            if view.gridlines.iter().any(|(pg, _)| *pg == i) {
                let px = (k * f.dpr).max(0.01) as f32;
                let mut grid = Paint::default();
                grid.set_color(Color::from_rgb(0x9C, 0xC2, 0xE5));
                grid.set_style(skia_safe::paint::Style::Stroke);
                grid.set_stroke_width(1.0 / px);
                grid.set_path_effect(skia_safe::PathEffect::dash(&[3.0 / px, 2.0 / px], 0.0));
                for (_, [x0, y0, x1, y1]) in view.gridlines.iter().filter(|(pg, _)| *pg == i) {
                    canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &grid);
                }
            }
            paint(page, &mut |p| self.item(canvas, &p.absolute(), images));
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
            canvas.restore();
        }
        canvas.restore();
    }

    /// An unfilled placeholder the host's way: its box tinted, a border in the colour, the icon at its left and
    /// the label beside it. The canvas is in page points.
    fn placeholder(&mut self, canvas: &Canvas, ph: &crate::editing::view::PlaceholderView) {
        let [x0, y0, x1, y1] = ph.rect.map(|v| v as f32);
        let r = Rect::new(x0, y0, x1, y1);
        let (a, rr, g, b) = ((ph.color >> 24) as u8, (ph.color >> 16) as u8, (ph.color >> 8) as u8, ph.color as u8);
        let mut fill = Paint::default();
        fill.set_color(Color::from_argb(255, 255, 255, 255));
        canvas.draw_rect(r, &fill);
        fill.set_color(Color::from_argb(a, rr, g, b));
        canvas.draw_rect(r, &fill);
        let mut border = Paint::default();
        border.set_anti_alias(true);
        border.set_style(skia_safe::paint::Style::Stroke);
        border.set_stroke_width(0.75);
        border.set_color(Color::from_rgb(rr, g, b));
        canvas.draw_rect(r, &border);
        let h = (y1 - y0).max(1.0);
        let pad = (h * 0.12).clamp(1.0, 4.0);
        let mut x = x0 + pad;
        if let Some(icon) = &ph.icon {
            // Vector shapes, scaled into the box's height (a texture-free icon).
            let paths = self
                .icons
                .entry(std::rc::Rc::as_ptr(icon) as usize)
                .or_insert_with(|| icon.paths.iter().filter_map(|(d, f)| Some((skia_safe::Path::from_svg(d)?, *f))).collect());
            let [vx, vy, vw, vh] = icon.view_box;
            let s = ((h - 2.0 * pad) / vh.max(0.01)).min(((x1 - x0) * 0.4) / vw.max(0.01));
            let (w, ih) = (vw * s, vh * s);
            canvas.save();
            canvas.translate((x, y0 + (h - ih) / 2.0));
            canvas.scale((s, s));
            canvas.translate((-vx, -vy));
            let mut shape = Paint::default();
            shape.set_anti_alias(true);
            for (path, fill) in paths.iter() {
                shape.set_color(fill.map_or(Color::from_rgb(rr, g, b), Color::new));
                canvas.draw_path(path, &shape);
            }
            canvas.restore();
            x += w + pad;
        }
        if !ph.label.is_empty()
            && let Some(tf) = self.typefaces.values().flatten().next().cloned()
        {
            let size = (h * 0.45).clamp(4.0, 14.0);
            let font = Font::from_typeface(tf, size);
            let mut text = Paint::default();
            text.set_anti_alias(true);
            text.set_color(Color::from_rgb(rr / 2, g / 2, b / 2));
            canvas.save();
            canvas.clip_rect(r, None, true);
            canvas.draw_str(&ph.label, (x, y0 + h / 2.0 + size * 0.35), &font, &text);
            canvas.restore();
        }
    }

    /// Draws one page item; the canvas is in page points.
    pub fn draw_item(&mut self, canvas: &Canvas, item: &Item, images: &[Vec<u8>]) {
        self.item(canvas, item, images);
    }

    fn item(&mut self, canvas: &Canvas, item: &Item, images: &[Vec<u8>]) {
        let mut paint = Paint::default();
        paint.set_anti_alias(true);
        match item {
            Item::Rect { rect: [x0, y0, x1, y1], colour } => {
                paint.set_color(rgb(*colour));
                canvas.draw_rect(Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32), &paint);
            }
            Item::Glyphs(r) => {
                if r.glyphs.is_empty() {
                    return;
                }
                let Some(tf) = self.typeface(&r.face) else { return };
                let mut font = Font::from_typeface(tf, r.size as f32);
                font.set_subpixel(true).set_edging(Edging::AntiAlias).set_embolden(r.face.sim_bold);
                if r.face.sim_italic {
                    font.set_skew_x(-0.2);
                }
                paint.set_color(rgb(r.colour.unwrap_or(0)));
                if self.with_text && draw_text(canvas, r, &font, &paint) {
                    return;
                }
                let ids: Vec<u16> = r.glyphs.iter().map(|g| g.0 as u16).collect();
                let at: Vec<Point> = r.glyphs.iter().map(|g| Point::new(g.1 as f32, r.baseline as f32)).collect();
                canvas.draw_glyphs_at(&ids, at.as_slice(), Point::new(0.0, 0.0), &font, &paint);
            }
            Item::Gridline { line: [x0, y0, x1, y1] } => {
                if !self.gridlines {
                    return;
                }
                paint.set_color(Color::from_rgb(0x9d, 0xc3, 0xe6)).set_style(skia_safe::PaintStyle::Stroke).set_stroke_width(0.75);
                paint.set_path_effect(skia_safe::PathEffect::dash(&[2.25, 1.5], 0.0));
                canvas.draw_line((*x0 as f32, *y0 as f32), (*x1 as f32, *y1 as f32), &paint);
            }
            Item::Image { rect: [x0, y0, x1, y1], key } => {
                let dst = Rect::new(*x0 as f32, *y0 as f32, *x1 as f32, *y1 as f32);
                let image = self
                    .images
                    .entry(*key)
                    .or_insert_with(|| key.index.and_then(|i| images.get(i)).and_then(|b| Image::from_encoded(Data::new_copy(b))))
                    .clone();
                match image {
                    Some(img) => {
                        canvas.draw_image_rect(&img, None, dst, &paint);
                    }
                    None => {
                        // A picture Skia can't decode (EMF/WMF), or a placeholder without data: a light frame.
                        let mut frame = Paint::default();
                        frame.set_color(Color::from_rgb(0xc8, 0xc8, 0xc8)).set_style(skia_safe::PaintStyle::Stroke);
                        canvas.draw_rect(dst, &frame);
                    }
                }
            }
        }
    }
}

/// Draws a glyph run as a text blob carrying its characters and clusters (Skia's PDF backend writes them as the
/// glyphs' Unicode, or ActualText where a cluster is not one character per glyph). False when the run's characters
/// can't be matched to its glyphs.
fn draw_text(canvas: &Canvas, r: &crate::layout::fragment::GlyphRun, font: &Font, paint: &Paint) -> bool {
    let Some(clusters) = clusters(r) else { return false };
    let text: String = r.chars.iter().map(|c| c.0).collect();
    let mut b = skia_safe::TextBlobBuilder::new();
    let (ids, xs, utf8, cl) = b.alloc_run_text_pos_h(font, r.glyphs.len(), r.baseline as f32, text.len(), None);
    for (k, &(g, x)) in r.glyphs.iter().enumerate() {
        ids[k] = g as u16;
        xs[k] = x as f32;
    }
    utf8.copy_from_slice(text.as_bytes());
    cl.copy_from_slice(&clusters);
    let Some(blob) = b.make() else { return false };
    canvas.draw_text_blob(&blob, (0, 0), paint);
    true
}

/// Each glyph's cluster (a UTF-8 offset into the run's characters). Characters of a piece share the piece's origin,
/// so the glyphs from one origin to the next are that piece's: one cluster per character when the counts match, else
/// one cluster for the piece.
fn clusters(r: &crate::layout::fragment::GlyphRun) -> Option<Vec<u32>> {
    if r.glyphs.is_empty() || r.chars.is_empty() {
        return None;
    }
    let mut pieces: Vec<(f64, usize, usize)> = Vec::new();
    for (i, &(_, x, _)) in r.chars.iter().enumerate() {
        match pieces.last_mut() {
            Some(p) if p.0 == x => p.2 += 1,
            Some(p) if p.0 > x => return None,
            _ => pieces.push((x, i, 1)),
        }
    }
    let mut byte = Vec::with_capacity(r.chars.len());
    let mut at = 0u32;
    for &(c, _, _) in r.chars.iter() {
        byte.push(at);
        at += c.len_utf8() as u32;
    }
    let piece_of: Vec<usize> = r.glyphs.iter().map(|g| pieces.partition_point(|p| p.0 <= g.1 + 1e-6).saturating_sub(1)).collect();
    if piece_of.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    let mut count = vec![0usize; pieces.len()];
    for &p in &piece_of {
        count[p] += 1;
    }
    let mut rank = vec![0usize; pieces.len()];
    Some(
        piece_of
            .iter()
            .map(|&p| {
                let (_, first, n) = pieces[p];
                let k = rank[p];
                rank[p] += 1;
                byte[if count[p] == n { first + k } else { first }]
            })
            .collect(),
    )
}
