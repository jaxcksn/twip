//! The screen drawn with twip-graphics: each page's content recorded once into a picture ([`retained`]), and each
//! frame (the desk, the pages' shadows and paper, their pictures, the view's overlays, the rulers) recorded in device
//! pixels and then rendered on the CPU. It draws what [`super::Renderer::draw`] draws with Skia, item for item; the
//! colours, theme and appearance come from that renderer.

mod overlay;
pub mod retained;
pub mod ruler;
mod shape;
pub(crate) mod svg;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::Resources;
use twip_graphics::cpu::{self, Draw, Target};
use twip_graphics::font::{FontData, FontId};
use twip_graphics::image::{ImageData, ImageId};
use twip_graphics::kurbo::{self, Affine, PathEl, Shape};
use twip_graphics::paint::{BlendMode, Color, ImagePaint, Paint};
use twip_graphics::scene::{Clip, GlyphRun, Recorder, Synthesis};

use super::{ChromeFace, Frame, Renderer};
use crate::editing::view::View;
use crate::font::Face;
use crate::layout::fragment::{ImageKey, Item};
use crate::layout::traverse::paint;

/// The twip-graphics side of a presentation: the fonts and images registered with it, the CPU renderer, the retained
/// page pictures, and the frame being recorded. Its memory is kept between frames.
#[derive(Default)]
pub struct Graphics {
    res: Resources,
    cpu: cpu::Renderer,
    /// Registered faces, by the face file's address, index and instance (as the Skia typefaces are keyed).
    fonts: HashMap<(usize, u32, u32), Option<FontId>>,
    /// The first document face drawn: UI strings use it for characters the chrome faces lack.
    fallback: Option<Rc<Face>>,
    /// The document's pictures, by key; `None` for one that isn't an image twip-graphics reads.
    images: HashMap<ImageKey, Option<ImageId>>,
    /// 8×8 pattern fills' tiles: the bits, then the foreground and background colours.
    tiles: HashMap<([u8; 8], [u8; 4], [u8; 4]), Option<ImageId>>,
    paths: shape::Paths,
    pub retained: retained::Retained,
    /// This frame's drawing, in device pixels.
    frame: Recorder,
    /// The colour the frame starts from, when its drawing doesn't cover every pixel itself (it does once
    /// [`Graphics::draw`] has recorded the desk).
    clear: Option<Color>,
    /// Scratch: a UI string's glyphs, and one path's elements.
    glyphs: Vec<(u32, f64)>,
    els: Vec<PathEl>,
    /// Glyph runs carry their characters and clusters, so a PDF's text extracts as the document's.
    pub with_text: bool,
}

fn rect([x0, y0, x1, y1]: [f64; 4]) -> kurbo::Rect {
    kurbo::Rect::new(x0, y0, x1, y1)
}

/// A rect's outline, clockwise from its top left, as Skia strokes a rect.
fn rect_path(r: kurbo::Rect) -> [PathEl; 5] {
    [
        PathEl::MoveTo((r.x0, r.y0).into()),
        PathEl::LineTo((r.x1, r.y0).into()),
        PathEl::LineTo((r.x1, r.y1).into()),
        PathEl::LineTo((r.x0, r.y1).into()),
        PathEl::ClosePath,
    ]
}

/// A stroke `width` wide with Skia's paint defaults: butt caps, miter joins at limit 4.
fn pen(width: f64) -> kurbo::Stroke {
    kurbo::Stroke::new(width).with_caps(kurbo::Cap::Butt).with_join(kurbo::Join::Miter).with_miter_limit(4.0)
}

/// [`pen`], dashed `on`, `off` from the start.
fn dashed(width: f64, on: f64, off: f64) -> kurbo::Stroke {
    pen(width).with_dashes(0.0, [on, off])
}

fn aliased(c: Color) -> Paint<'static> {
    Paint { aa: false, ..Paint::from(c) }
}

/// A chrome face as Twip's [`Face`], made once per thread.
fn chrome_face(face: ChromeFace) -> Option<Rc<Face>> {
    thread_local! {
        static FACES: [Option<Rc<Face>>; 2] = [
            crate::font::shipped(super::CHROME_FONTS[0], "JetBrains Mono"),
            crate::font::shipped(super::CHROME_FONTS[1], "JetBrains Mono SemiBold"),
        ];
    }
    FACES.with(|f| f[face as usize].clone())
}

/// `text`'s width at `size` in `face`: the characters' advances, as Skia's `measure_str` adds them (no shaping).
fn measure(face: &Face, text: &str, size: f64) -> f64 {
    text.chars().map(|c| face.advance(face.glyph(c).unwrap_or(0))).sum::<f64>() * face.scale(size)
}

impl Graphics {
    /// Forgets the document's pictures (a new document).
    pub fn clear_images(&mut self) {
        self.images.clear();
    }

    /// Records a frame: the visible pages, then the view's overlays over them, in device pixels. What the host draws
    /// over it goes into [`Graphics::frame`]; [`Graphics::present`] renders it.
    pub fn draw(&mut self, r: &Renderer, view: &View, images: &[Vec<u8>], f: &Frame) {
        let mut rec = std::mem::take(&mut self.frame);
        let k = f.px_per_pt;
        // The desk: the viewport outside the papers, which cover the rest. Each paper is taken a device pixel smaller
        // than it is drawn, so however its edges round, no pixel is left unwritten: the paper draws over the overlap.
        let papers = view.pages_visible.clone().filter(|&i| view.snapshot.pages().get(i).is_some()).map(|i| {
            let [x, y, w, h] = view.stack.pages[i];
            let d = |v: f64, o: f64| (v * k + o) * f.dpr;
            [d(x, f.origin_x).floor() + 1.0, d(y, f.origin_y).floor() + 1.0, d(x + w, f.origin_x).ceil() - 1.0, d(y + h, f.origin_y).ceil() - 1.0]
        });
        desk(&mut rec, papers, f.width, f.height, r.background.unwrap_or(r.theme.tokens.desk));
        self.clear = None;
        let device = Affine::scale(f.dpr) * Affine::translate((f.origin_x, f.origin_y));
        let paper = aliased(r.ink_for(false, r.page_colour.unwrap_or(0xFF_FFFF)));
        let shadow_k = r.theme.tokens.shadow;
        let mut kept = std::mem::take(&mut self.retained);
        for i in view.pages_visible.clone() {
            let Some(page) = view.snapshot.pages().get(i) else { continue };
            let [x, y, w, h] = view.stack.pages[i];
            let p = kurbo::Rect::new(x * k, y * k, x * k + w * k, y * k + h * k);
            shadow(&mut rec, device.transform_rect_bbox(p), f.dpr, shadow_k);
            rec.push_transform(device);
            rec.fill_rect(p, paper);
            rec.push_transform(Affine::translate((p.x0, p.y0)) * Affine::scale(k));
            if r.retain && !r.direct {
                let inputs = retained::Inputs { gridlines: r.gridlines, appearance: r.appearance };
                let picture = kept.picture(page, inputs, |content| self.page(content, r, page, images));
                if let Some(picture) = picture {
                    rec.picture(&picture, Affine::IDENTITY);
                }
            } else {
                self.page(&mut rec, r, page, images);
            }
            self.overlays(&mut rec, r, view, images, i, f);
            rec.pop();
            rec.pop();
        }
        if r.retain {
            kept.trim();
        }
        self.retained = kept;
        self.frame = rec;
    }

    /// The markup pane over the frame: `r` (device pixels) in `c`.
    pub fn pane(&mut self, r: [f64; 4], c: Color) {
        self.frame.fill_rect(rect(r), aliased(c));
    }

    /// The balloons' leaders over everything: polylines in content CSS px (`f` scrolls and scales them), `width`
    /// device pixels wide, in `c`.
    pub fn leaders(&mut self, lines: &[Vec<crate::api::input::Point>], f: &Frame, width: f64, c: Color) {
        let mut rec = std::mem::take(&mut self.frame);
        let to = |q: &crate::api::input::Point| ((q.x + f.origin_x) * f.dpr, (q.y + f.origin_y) * f.dpr);
        for line in lines {
            self.polyline(&mut rec, line.iter().map(to), &pen(width), c);
        }
        self.frame = rec;
    }

    /// Tests: the colour the next frame starts from, for a frame that draws no pages.
    #[cfg(test)]
    pub(crate) fn clear_with(&mut self, c: Color) {
        self.clear = Some(c);
    }

    /// Draws the rulers over the frame ([`ruler::paint`]).
    pub fn ruler(&mut self, view: &crate::api::services::RulerView, theme: &super::theme::Theme, f: &ruler::RulerFrame) {
        let mut rec = std::mem::take(&mut self.frame);
        ruler::paint(self, &mut rec, view, theme, f);
        self.frame = rec;
    }

    /// Renders the recorded frame into `target`, over the desk colour, and starts the next one. What failed to draw (an
    /// image that didn't decode, a glyph that didn't read) is counted in [`Graphics::stats`].
    pub fn present(&mut self, target: &mut Target<'_>) {
        let picture = self.frame.finish();
        let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
        self.cpu.render(target, &self.res, &cpu::Frame { clear: self.clear, draws: &draws });
    }

    /// Hands the recorded frame (over the desk colour) and the resources it draws from to `draw`, as
    /// [`Graphics::present`] does to the CPU renderer, and starts the next one: the GPU screen's present.
    #[cfg_attr(not(twip_window), allow(dead_code, reason = "the GPU screen is the native windows'"))]
    pub fn finish<R>(&mut self, draw: impl FnOnce(&Resources, &cpu::Frame<'_>) -> R) -> R {
        let picture = self.frame.finish();
        let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
        draw(&self.res, &cpu::Frame { clear: self.clear, draws: &draws })
    }

    /// PDF export: one page item recorded into `rec`, in the page's points.
    pub fn record_item(&mut self, rec: &mut Recorder, r: &Renderer, item: &Item, images: &[Vec<u8>]) {
        self.item(rec, r, false, item, images);
    }

    /// The fonts and images recorded pictures draw with.
    pub fn resources(&self) -> &Resources {
        &self.res
    }

    /// Tools: `page`'s content as a picture in its points, recorded now (the oracle compares it with Skia's drawing).
    #[doc(hidden)]
    pub fn page_picture(&mut self, r: &Renderer, page: &crate::layout::fragment::PageFragment, images: &[Vec<u8>]) -> twip_graphics::scene::Picture {
        let mut rec = Recorder::new();
        self.page(&mut rec, r, page, images);
        rec.finish()
    }

    /// Tools: `picture` drawn into `target` under `transform`, over `clear`.
    #[doc(hidden)]
    pub fn render_picture(&mut self, target: &mut Target<'_>, picture: &twip_graphics::scene::Picture, transform: Affine, clear: Color) {
        let draws = [Draw { picture, transform }];
        self.cpu.render(target, &self.res, &cpu::Frame { clear: Some(clear), draws: &draws });
    }

    /// What the last frame couldn't draw.
    pub fn stats(&self) -> cpu::Stats {
        self.cpu.stats()
    }

    /// Records `page`'s content (in its points) into `rec`. Items are recorded where layout keeps them, under a
    /// translation to their place: no item is copied.
    fn page(&mut self, rec: &mut Recorder, r: &Renderer, page: &crate::layout::fragment::PageFragment, images: &[Vec<u8>]) {
        let mut at: Option<(f64, f64)> = None;
        paint(page, &mut |p| {
            if at != Some((p.dx, p.dy)) {
                if at.is_some() {
                    rec.pop();
                }
                rec.push_transform(Affine::translate((p.dx, p.dy)));
                at = Some((p.dx, p.dy));
            }
            self.item(rec, r, p.own_colours, p.item, images);
        });
        if at.is_some() {
            rec.pop();
        }
    }

    /// The registry's id for `face`, registered on first use.
    fn font(&mut self, face: &Rc<Face>) -> Option<FontId> {
        let key = (Arc::as_ptr(&face.data) as usize, face.index, face.instance);
        let res = &mut self.res;
        *self.fonts.entry(key).or_insert_with(|| res.fonts.add(Arc::clone(&face.data) as FontData, face.index, &face.variations).ok())
    }

    /// The document's picture `key`, registered on first use.
    fn image(&mut self, key: ImageKey, images: &[Vec<u8>]) -> Option<ImageId> {
        let res = &mut self.res;
        *self.images.entry(key).or_insert_with(|| {
            let bytes = key.index.and_then(|i| images.get(i))?;
            res.images.add(Arc::new(bytes.clone()) as ImageData).ok()
        })
    }

    /// Records `text` with its baseline's start at (`x`, `y`), `size` points, in `face`: each character's nominal glyph
    /// at the sum of the advances before it, as Skia's `draw_str` places them.
    #[allow(clippy::too_many_arguments, reason = "one string's place, face, size and colour")]
    fn text(&mut self, rec: &mut Recorder, face: &Rc<Face>, text: &str, x: f64, y: f64, size: f64, c: Color) {
        let Some(font) = self.font(face) else { return };
        let k = face.scale(size);
        let mut glyphs = std::mem::take(&mut self.glyphs);
        glyphs.clear();
        let mut at = x;
        for ch in text.chars() {
            let g = face.glyph(ch).unwrap_or(0);
            glyphs.push((g, at));
            at += face.advance(g) * k;
        }
        let run = GlyphRun { font, size, baseline: y, glyphs: &glyphs, synthesis: Synthesis::default() };
        rec.glyphs(&self.res.fonts, &run, c);
        self.glyphs = glyphs;
    }

    /// One page item, in the page's points.
    fn item(&mut self, rec: &mut Recorder, r: &Renderer, own: bool, item: &Item, images: &[Vec<u8>]) {
        let ink = |c: u32| r.ink_for(own, c);
        match item {
            Item::Rect { rect: rc, colour } => rec.fill_rect(rect(*rc), ink(*colour)),
            Item::Glyphs(g) => {
                if g.glyphs.is_empty() {
                    return;
                }
                let Some(font) = self.font(&g.face) else { return };
                if self.fallback.is_none() {
                    self.fallback = Some(Rc::clone(&g.face));
                }
                let synthesis = Synthesis { embolden: g.face.sim_bold, skew_x: if g.face.sim_italic { -0.2 } else { 0.0 } };
                let run = GlyphRun { font, size: g.size, baseline: g.baseline, glyphs: &g.glyphs, synthesis };
                if self.with_text {
                    let text: String = g.chars.iter().map(|c| c.0).collect();
                    let clusters = clusters(g).unwrap_or_default();
                    rec.glyphs_text(&self.res.fonts, &run, &text, &clusters, ink(g.colour.unwrap_or(0)));
                } else {
                    rec.glyphs(&self.res.fonts, &run, ink(g.colour.unwrap_or(0)));
                }
            }
            Item::Gridline { line: [x0, y0, x1, y1] } => {
                if r.gridlines {
                    rec.line((*x0, *y0).into(), (*x1, *y1).into(), &dashed(0.75, 2.25, 1.5), ink(0x9D_C3E6));
                }
            }
            Item::Shape(s) => self.shape(rec, s, images),
            Item::Turned(t) => {
                rec.push_transform(Affine::rotate_about(t.deg.to_radians(), kurbo::Point::new(t.centre[0], t.centre[1])));
                for i in &t.items {
                    self.item(rec, r, own, i, images);
                }
                rec.pop();
            }
            Item::Image { rect: rc, key } => {
                if let Some(bytes) = key.index.and_then(|i| images.get(i)).filter(|b| crate::docx::image::is_svg(b))
                    && svg::draw(rec, bytes, rect(*rc))
                {
                    return;
                }
                match self.image(*key, images) {
                    Some(id) => rec.image(id, None, rect(*rc), &ImagePaint::default()),
                    // A picture that isn't an image twip-graphics reads (EMF, WMF), or a placeholder without data: a
                    // light frame, a CSS pixel wide at 100%.
                    None => rec.stroke_path(&rect_path(rect(*rc)), &pen(0.75), aliased(ink(0xC8_C8C8))),
                }
            }
        }
    }

    /// Fills (`stroke: None`) or strokes a shape built here (a circle, a diamond), through the scratch buffer.
    fn draw_shape<'p>(&mut self, rec: &mut Recorder, shape: impl Shape, stroke: Option<&kurbo::Stroke>, paint: impl Into<Paint<'p>>) {
        let mut els = std::mem::take(&mut self.els);
        els.clear();
        els.extend(shape.path_elements(0.01));
        match stroke {
            Some(s) => rec.stroke_path(&els, s, paint),
            None => rec.fill_path(&els, paint),
        }
        self.els = els;
    }

    /// Strokes the open polyline through `points`, through the scratch buffer.
    fn polyline(&mut self, rec: &mut Recorder, points: impl IntoIterator<Item = (f64, f64)>, stroke: &kurbo::Stroke, c: Color) {
        let mut els = std::mem::take(&mut self.els);
        els.clear();
        for (i, p) in points.into_iter().enumerate() {
            els.push(if i == 0 { PathEl::MoveTo(p.into()) } else { PathEl::LineTo(p.into()) });
        }
        rec.stroke_path(&els, stroke, c);
        self.els = els;
    }
}

/// A page's shadow around its paper `p` (device pixels): four rounded rects, each grown and dropped a little more
/// and fainter. Only what shows around the paper is drawn (the paper is opaque and drawn over it), as Skia's
/// difference clip does, in whole-pixel pieces: the strips above and below the paper through the rounded rect, the
/// sides' rows by the corners likewise, and the sides' straight stretch between them as plain rects. Filling the
/// paper's area under it would cost four fills of the page a frame.
fn shadow(rec: &mut Recorder, p: kurbo::Rect, dpr: f64, k: f32) {
    // The paper's edges as its aliased fill rounds them.
    let snap = |v: f64| (v + 0.5).floor();
    let (px0, py0, px1, py1) = (snap(p.x0), snap(p.y0), snap(p.x1), snap(p.y1));
    for (inflate, drop, alpha) in [(4.0, 3.0, 6.0), (3.0, 2.5, 10.0), (2.0, 2.0, 14.0), (1.0, 1.0, 22.0)] {
        let c = Color::rgba(0, 0, 0, (alpha * k).min(255.0) as u8);
        let (grow, down, radius) = (inflate * dpr, drop * dpr, (inflate + 2.0) * dpr);
        let s = kurbo::Rect::new(p.x0 - grow, p.y0 - grow + down, p.x1 + grow, p.y1 + grow + down);
        let clipped = |rec: &mut Recorder, part: kurbo::Rect| {
            if part.width() > 0.0 && part.height() > 0.0 {
                rec.push_clip(Clip::Rect { rect: part, aa: false });
                rec.fill_rrect(s, radius, c);
                rec.pop();
            }
        };
        clipped(rec, kurbo::Rect::new(s.x0.floor(), s.y0.floor(), s.x1.ceil(), py0));
        clipped(rec, kurbo::Rect::new(s.x0.floor(), py1, s.x1.ceil(), s.y1.ceil()));
        // The sides: their rows within the corners' reach through the rounded rect, the rest as rects.
        let (a, b) = ((s.y0 + radius).ceil().clamp(py0, py1), (s.y1 - radius).floor().clamp(py0, py1));
        let (a, b) = if a < b { (a, b) } else { (py1, py1) };
        for (x0, x1) in [(s.x0.floor(), px0), (px1, s.x1.ceil())] {
            clipped(rec, kurbo::Rect::new(x0, py0, x1, a));
            clipped(rec, kurbo::Rect::new(x0, b, x1, py1));
        }
        if a < b {
            rec.fill_rect(kurbo::Rect::new(s.x0, a, px0, b), c);
            rec.fill_rect(kurbo::Rect::new(px1, a, s.x1, b), c);
        }
    }
}

/// Fills the parts of the `width` × `height` viewport (device pixels) outside `papers` with `c`, as whole-pixel rects:
/// band by band down the papers' edges, the gaps between the papers crossing each band.
fn desk(rec: &mut Recorder, papers: impl Iterator<Item = [f64; 4]>, width: f64, height: f64, c: Color) {
    let papers: Vec<[f64; 4]> = papers.filter(|p| p[2] > p[0] && p[3] > p[1]).collect();
    let mut ys: Vec<f64> = papers.iter().flat_map(|p| [p[1], p[3]]).chain([0.0, height]).map(|y| y.clamp(0.0, height)).collect();
    ys.sort_by(f64::total_cmp);
    ys.dedup();
    let paint = aliased(c);
    for band in ys.windows(2) {
        let (y0, y1) = (band[0], band[1]);
        let mut across: Vec<(f64, f64)> = papers.iter().filter(|p| p[1] <= y0 && p[3] >= y1).map(|p| (p[0], p[2])).collect();
        across.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = 0.0;
        for (a, b) in across {
            if a > x {
                rec.fill_rect(kurbo::Rect::new(x, y0, a.min(width), y1), paint);
            }
            x = x.max(b);
        }
        if x < width {
            rec.fill_rect(kurbo::Rect::new(x, y0, width, y1), paint);
        }
    }
}

/// Paint with a blend mode.
fn blended(c: Color, blend: BlendMode) -> Paint<'static> {
    Paint { blend, ..Paint::from(c) }
}

/// Each glyph's cluster (a UTF-8 offset into the run's characters). Characters of a piece share the piece's origin,
/// so the glyphs from one origin to the next are that piece's: one cluster per character when the counts match, else
/// one cluster for the piece.
pub(crate) fn clusters(r: &crate::layout::fragment::GlyphRun) -> Option<Vec<u32>> {
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
