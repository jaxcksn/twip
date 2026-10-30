//! The renderer: paints snapshot pages and the editor's view overlays with Skia onto a host canvas.
//! It reads layout's paint traversal and `editing::view` only, and holds no
//! editing state: everything it draws for the editor comes from one [`View`], built from the snapshot it paints.

use std::collections::HashMap;

mod overlay;
pub mod retained;
pub mod ruler;
#[cfg(target_os = "emscripten")]
pub mod web;
#[cfg(twip_dawn)]
pub mod window;

use skia_safe::font_arguments::{VariationPosition, variation_position::Coordinate};
use skia_safe::{Canvas, Color, Data, Font, FontArguments, FontMgr, FourByteTag, Image, Paint, Point, RRect, Rect, Typeface, font::Edging};

use crate::editing::view::View;
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
    /// Draw the caret (the view's, when it has one): the host's view is focused and the blink is in its visible phase.
    pub caret: bool,
}

/// Skia objects made from the layout's faces and the document's pictures, kept between frames.
#[derive(Default)]
pub struct Renderer {
    typefaces: HashMap<(usize, u32, u32), Option<Typeface>>,
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
    /// Keep each page's content and draw it again from that ([`retained`]): on screen, not for PDF export.
    pub retain: bool,
    pub retained: retained::Retained,
    /// Draw everything from scratch, past any retained page content: what a verified frame is compared with.
    pub direct: bool,
    pub palette: Palette,
}

/// The colours of the overlays the host can set. By default the lines and marks are at least 3:1 against the page,
/// and the selection is the light blue of a system text selection, black text over it at least 4.5:1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub caret: Color,
    /// Blended under the content (multiply), so text over it keeps its contrast.
    pub selection: Color,
    pub object: Color,
    pub drop: Color,
    pub marks: Color,
    pub leaders: Color,
}

impl Default for Palette {
    fn default() -> Palette {
        Palette {
            caret: Color::BLACK,
            selection: Color::from_rgb(0xB3, 0xD7, 0xFF),
            object: Color::from_rgb(0x60, 0x60, 0x60),
            drop: Color::from_rgb(0x40, 0x40, 0x40),
            marks: Color::from_rgb(0x2F, 0x6F, 0xB5),
            leaders: Color::from_rgb(0x76, 0x76, 0x76),
        }
    }
}

pub fn rgb(c: u32) -> Color {
    Color::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

impl Renderer {
    /// A renderer for the screen: it keeps each page's content ([`retained`]).
    pub fn retaining() -> Renderer {
        Renderer { retain: true, ..Renderer::default() }
    }

    /// Forgets decoded pictures (a new document).
    pub fn clear_images(&mut self) {
        self.images.clear();
    }

    /// Draws `face`'s glyphs from `data` (a single-face font file) instead of its own file: PDF export's subsets.
    pub fn set_face_data(&mut self, face: &Face, data: &[u8]) {
        let key = (std::sync::Arc::as_ptr(&face.data) as usize, face.index, face.instance);
        self.typefaces.insert(key, FontMgr::new().new_from_data(Data::new_copy(data), 0));
    }

    /// `face`'s typeface: a variable font's at the face's instance (FONT-17).
    fn typeface(&mut self, face: &Face) -> Option<Typeface> {
        let key = (std::sync::Arc::as_ptr(&face.data) as usize, face.index, face.instance);
        self.typefaces
            .entry(key)
            .or_insert_with(|| {
                let typeface = FontMgr::new().new_from_data(Data::new_copy(&face.data), face.index)?;
                if face.variations.is_empty() {
                    return Some(typeface);
                }
                let coordinates: Vec<Coordinate> = face
                    .variations
                    .iter()
                    .map(|&(t, value)| Coordinate { axis: FourByteTag::from_chars(t[0] as char, t[1] as char, t[2] as char, t[3] as char), value })
                    .collect();
                let arguments = FontArguments::new().set_variation_design_position(VariationPosition { coordinates: &coordinates });
                typeface.clone_with_arguments(&arguments).or(Some(typeface))
            })
            .clone()
    }

    /// The pages a frame shows.
    pub fn visible(stack: &crate::editing::view::PageStack, f: &Frame) -> std::ops::Range<usize> {
        let k = f.px_per_pt;
        let (top, bottom) = (-f.origin_y, -f.origin_y + f.height / f.dpr);
        let shown: Vec<usize> = (0..stack.pages.len())
            .filter(|&i| {
                let [_, y, _, h] = stack.pages[i];
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
        for i in view.pages_visible.clone() {
            let Some(page) = view.snapshot.pages().get(i) else { continue };
            let [x, y, w, h] = view.stack.pages[i];
            let r = Rect::from_xywh((x * k) as f32, (y * k) as f32, (w * k) as f32, (h * k) as f32);
            // The shadow only shows around the paper: clipping the paper out saves filling its whole area four times.
            canvas.save();
            canvas.clip_rect(r, skia_safe::ClipOp::Difference, false);
            for (inflate, drop, alpha) in [(4.0, 3.0, 6), (3.0, 2.5, 10), (2.0, 2.0, 14), (1.0, 1.0, 22)] {
                shadow.set_color(Color::from_argb(alpha, 0, 0, 0));
                let s = Rect::new(r.left - inflate, r.top - inflate + drop, r.right + inflate, r.bottom + inflate + drop);
                canvas.draw_rrect(RRect::new_rect_xy(s, inflate + 2.0, inflate + 2.0), &shadow);
            }
            canvas.restore();
            canvas.draw_rect(r, &white);
            canvas.save();
            canvas.translate((r.left, r.top));
            canvas.scale((k as f32, k as f32));
            if self.retain && !self.direct {
                let mut kept = std::mem::take(&mut self.retained);
                kept.draw(canvas, page, retained::Inputs { gridlines: self.gridlines }, (w, h), |c| {
                    paint(page, &mut |p| self.item(c, &p.absolute(), images));
                });
                self.retained = kept;
            } else {
                paint(page, &mut |p| self.item(canvas, &p.absolute(), images));
            }
            self.overlays(canvas, view, i, f);
            canvas.restore();
        }
        canvas.restore();
        if self.retain {
            self.retained.trim();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    #[test]
    fn the_default_overlay_colours_have_3_to_1_contrast_with_the_page_and_the_canvas() {
        // The selection is a light fill text sits on, not a line or mark: its check is black text's contrast over it
        // (`the_default_selection_keeps_black_text_readable`).
        let p = Palette::default();
        for (name, c) in [("caret", p.caret), ("object", p.object), ("drop", p.drop), ("marks", p.marks), ("leaders", p.leaders)] {
            for back in [Color::WHITE, Color::from_rgb(0xf3, 0xf3, 0xf3)] {
                let (a, b) = (luminance(c), luminance(back));
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(ratio >= 3.0, "{name} against {back:?}: {ratio:.2}");
            }
        }
    }
}
