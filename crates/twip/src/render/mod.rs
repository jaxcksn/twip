//! The renderer: paints snapshot pages and the editor's view overlays with Skia onto a host canvas.
//! It reads layout's paint traversal and `editing::view` only, and holds no
//! editing state: everything it draws for the editor comes from one [`View`], built from the snapshot it paints.

use std::collections::HashMap;

mod affordances;
pub mod appearance;
mod overlay;
pub mod retained;
pub mod ruler;
mod shape;
pub mod theme;
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
    /// Skia paths of the shapes drawn, by geometry.
    paths: shape::PathCache,
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
    /// The chrome's colours.
    pub theme: theme::Theme,
    /// Overlay colours the host set; they win over the theme's.
    pub overlay_colours: Option<Palette>,
    pub appearance: appearance::Appearance,
    pub page_colour: Option<u32>,
    own_colours: bool,
}

/// The colours of the overlays the host can set. By default the lines and marks are at least 3:1 against the page,
/// and the selection is the light blue of a system text selection, black text over it at least 4.5:1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    /// Blended under the content (multiply), so text over it keeps its contrast.
    pub selection: Color,
    pub object: Color,
    pub drop: Color,
    pub marks: Color,
    pub leaders: Color,
}

impl Default for Palette {
    fn default() -> Palette {
        theme::Theme::default().palette()
    }
}

/// The chrome's typefaces: subsets of JetBrains Mono (labels, numbers, hints and readouts) that ship with the engine
/// (`twip-tools chrome-fonts`), the same on every target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChromeFace {
    Mono500,
    Mono600,
}

/// A chrome typeface, made once per thread.
pub fn chrome_typeface(face: ChromeFace) -> Option<Typeface> {
    thread_local! {
        static FACES: [Option<Typeface>; 2] = [
            &include_bytes!("chrome_fonts/jetbrains-mono-500.ttf")[..],
            include_bytes!("chrome_fonts/jetbrains-mono-600.ttf"),
        ]
        .map(|b| FontMgr::new().new_from_data(Data::new_copy(b), 0));
    }
    FACES.with(|f| f[face as usize].clone())
}

pub fn rgb(c: u32) -> Color {
    Color::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

impl Renderer {
    /// A renderer for the screen: it keeps each page's content ([`retained`]).
    pub fn retaining() -> Renderer {
        Renderer { retain: true, ..Renderer::default() }
    }

    /// Sets the chrome's theme; the overlay colours follow it unless the host set its own.
    pub fn set_theme(&mut self, theme: theme::Theme) {
        self.theme = theme;
    }

    /// The overlay colours: the host's ([`Renderer::overlay_colours`]), or the page theme's.
    pub fn palette(&self) -> Palette {
        self.overlay_colours.unwrap_or_else(|| self.page_theme().palette())
    }

    /// The caret's colour: black or white, whichever stands out more on the page as drawn. Neither the theme's
    /// colours nor the host's change it.
    pub fn caret(&self) -> Color {
        let page = self.page();
        if theme::contrast(Color::WHITE, page) > theme::contrast(Color::BLACK, page) { Color::WHITE } else { Color::BLACK }
    }

    /// The page's colour as drawn: what handles and buttons on the page are filled with, whatever the chrome's theme.
    pub(super) fn page(&self) -> Color {
        rgb(self.appearance.map(self.page_colour.unwrap_or(0xFF_FFFF)))
    }

    /// The theme of what's drawn over the page (the selection, handles, guides, measurements): the chrome's when its
    /// scheme matches the page, else the matching scheme's with the same accent, so a light page under dark chrome
    /// gets the light scheme's accent and readouts.
    pub fn page_theme(&self) -> theme::Theme {
        let dark = theme::luminance(self.page()) < 0.2;
        match self.theme.scheme {
            theme::Scheme::HighContrast(_) => self.theme,
            _ if dark == self.theme.is_dark() => self.theme,
            _ => theme::Theme::new(if dark { theme::Scheme::Dark } else { theme::Scheme::Light }, self.theme.accent.base),
        }
    }

    pub(super) fn ink(&self, c: u32) -> Color {
        rgb(if self.own_colours { c } else { self.appearance.map(c) })
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
        canvas.clear(self.background.unwrap_or(self.theme.tokens.desk));
        canvas.save();
        canvas.scale((f.dpr as f32, f.dpr as f32));
        canvas.translate((f.origin_x as f32, f.origin_y as f32));
        let k = f.px_per_pt;
        let mut shadow = Paint::default();
        shadow.set_anti_alias(true);
        // Page colour
        self.own_colours = false;
        let mut paper = Paint::default();
        paper.set_color(self.ink(self.page_colour.unwrap_or(0xFF_FFFF)));
        let shadow_k = self.theme.tokens.shadow;
        for i in view.pages_visible.clone() {
            let Some(page) = view.snapshot.pages().get(i) else { continue };
            let [x, y, w, h] = view.stack.pages[i];
            let r = Rect::from_xywh((x * k) as f32, (y * k) as f32, (w * k) as f32, (h * k) as f32);
            // The shadow only shows around the paper: clipping the paper out saves filling its whole area four times.
            canvas.save();
            canvas.clip_rect(r, skia_safe::ClipOp::Difference, false);
            for (inflate, drop, alpha) in [(4.0, 3.0, 6), (3.0, 2.5, 10), (2.0, 2.0, 14), (1.0, 1.0, 22)] {
                shadow.set_color(Color::from_argb((alpha as f32 * shadow_k).min(255.0) as u8, 0, 0, 0));
                let s = Rect::new(r.left - inflate, r.top - inflate + drop, r.right + inflate, r.bottom + inflate + drop);
                canvas.draw_rrect(RRect::new_rect_xy(s, inflate + 2.0, inflate + 2.0), &shadow);
            }
            canvas.restore();
            canvas.draw_rect(r, &paper);
            canvas.save();
            canvas.translate((r.left, r.top));
            canvas.scale((k as f32, k as f32));
            if self.retain && !self.direct {
                let mut kept = std::mem::take(&mut self.retained);
                let inputs = retained::Inputs { gridlines: self.gridlines, appearance: self.appearance };
                kept.draw(canvas, page, inputs, (w, h), |c| {
                    paint(page, &mut |p| {
                        self.own_colours = p.own_colours;
                        self.item(c, &p.absolute(), images)
                    });
                });
                self.retained = kept;
            } else {
                paint(page, &mut |p| {
                    self.own_colours = p.own_colours;
                    self.item(canvas, &p.absolute(), images)
                });
            }
            self.own_colours = false;
            self.overlays(canvas, view, images, i, f);
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
        fill.set_color(self.ink(0xFF_FFFF));
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
                paint.set_color(self.ink(*colour));
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
                paint.set_color(self.ink(r.colour.unwrap_or(0)));
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
                paint.set_color(self.ink(0x9D_C3E6)).set_style(skia_safe::PaintStyle::Stroke).set_stroke_width(0.75);
                paint.set_path_effect(skia_safe::PathEffect::dash(&[2.25, 1.5], 0.0));
                canvas.draw_line((*x0 as f32, *y0 as f32), (*x1 as f32, *y1 as f32), &paint);
            }
            Item::Shape(s) => self.shape(canvas, s, images),
            Item::Turned(t) => {
                canvas.save();
                canvas.rotate(t.deg as f32, Some(Point::new(t.centre[0] as f32, t.centre[1] as f32)));
                for i in &t.items {
                    self.item(canvas, i, images);
                }
                canvas.restore();
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
                        frame.set_color(self.ink(0xC8_C8C8)).set_style(skia_safe::PaintStyle::Stroke);
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
    fn marks_on_a_dark_page_hold_against_it() {
        let r = Renderer { appearance: appearance::Appearance::Dark { page: appearance::PageMode::Dark }, ..Renderer::default() };
        let page = r.ink(0xFF_FFFF);
        for c in [0xE6_0000, 0x00_78D4, 0xD1_3438, 0xC1_9C00] {
            assert!(theme::contrast(r.ink(c), page) >= 3.0, "{c:06X} on the dark page");
        }
    }

    #[test]
    fn the_editor_sizes_chrome_text_with_the_ui_faces_advance() {
        for face in [ChromeFace::Mono500, ChromeFace::Mono600] {
            let font = Font::from_typeface(chrome_typeface(face).expect("the face"), 100.0);
            for text in ["M", "i", "\u{e9}", "\u{17e}", "\u{2197}"] {
                let (w, _) = font.measure_str(text, None);
                assert!((f64::from(w) / 100.0 - crate::editing::view::MONO_ADVANCE).abs() < 1e-3, "{face:?} {text}: {w}");
            }
        }
    }

    #[test]
    fn the_caret_is_black_or_white_against_the_page() {
        let mut r = Renderer::default();
        let mut dark = theme::Theme::new(theme::Scheme::Dark, theme::AZURE);
        dark.tokens.ink = Color::from_rgb(0xE6, 0xED, 0xF3);
        r.set_theme(dark);
        assert_eq!(r.caret(), Color::BLACK, "on the white page, whatever the chrome's text colour");
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.caret(), Color::WHITE, "on the dark page");
        r.appearance = appearance::Appearance::Light;
        r.page_colour = Some(0x10_2030);
        assert_eq!(r.caret(), Color::WHITE, "on a dark page colour");
        r.overlay_colours = Some(Palette { marks: Color::RED, ..Palette::default() });
        assert_eq!(r.caret(), Color::WHITE, "whatever the host's overlay colours");
    }

    #[test]
    fn what_is_drawn_over_the_page_takes_the_pages_scheme() {
        let mut r = Renderer::default();
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        let light = theme::Tokens::of(theme::Scheme::Light);
        assert_eq!((r.page_theme().tokens, r.page()), (light, Color::WHITE), "a light page under dark chrome");
        assert_eq!(r.palette().object, theme::AZURE, "and the light scheme's accent");
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.page_theme(), r.theme, "the dark page: the chrome's own");
    }

    #[test]
    fn overlay_colours_the_host_set_outlast_a_theme_change() {
        let mut r = Renderer::default();
        let mine = Palette { marks: Color::RED, ..Palette::default() };
        r.overlay_colours = Some(mine);
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        assert_eq!(r.palette(), mine, "the host's colours stay");
        assert_eq!(r.theme.tokens.desk, theme::Tokens::of(theme::Scheme::Dark).desk, "the rest follows the theme");
        r.overlay_colours = None;
        r.set_theme(theme::Theme::new(theme::Scheme::Dark, theme::AZURE));
        r.appearance = appearance::Appearance::Dark { page: appearance::PageMode::Dark };
        assert_eq!(r.palette(), r.theme.palette());
    }

    #[test]
    fn the_chrome_faces_load_and_draw_their_symbols() {
        for face in [ChromeFace::Mono500, ChromeFace::Mono600] {
            let tf = chrome_typeface(face).unwrap_or_else(|| panic!("{face:?} loads"));
            for c in ['0', 'A', 'z', '″', '×', 'Δ', '→', '·', '−', '°', '▾'] {
                assert_ne!(tf.unichar_to_glyph(c as i32), 0, "{face:?} has {c}");
            }
        }
    }

    #[test]
    fn the_default_overlay_colours_have_3_to_1_contrast_with_the_page_and_the_canvas() {
        // The selection is a light fill text sits on, not a line or mark: its check is black text's contrast over it
        // (`the_default_selection_keeps_black_text_readable`).
        let p = Palette::default();
        for (name, c) in [("object", p.object), ("drop", p.drop), ("marks", p.marks), ("leaders", p.leaders)] {
            for back in [Color::WHITE, Color::from_rgb(0xf3, 0xf3, 0xf3)] {
                let (a, b) = (luminance(c), luminance(back));
                let ratio = (a.max(b) + 0.05) / (a.min(b) + 0.05);
                assert!(ratio >= 3.0, "{name} against {back:?}: {ratio:.2}");
            }
        }
    }
}
