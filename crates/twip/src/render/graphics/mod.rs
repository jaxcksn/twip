//! The screen drawn with twip-graphics: each page's content recorded once into a picture ([`retained`]), and each
//! frame (the desk, the pages' shadows and paper, their pictures, the view's overlays, the rulers) recorded in device
//! pixels and then rendered on the CPU. The colours, theme and appearance come from
//! [`super::Renderer`].

#[cfg(test)]
mod output;
mod overlay;
pub mod retained;
pub mod ruler;
mod shape;
pub(crate) mod svg;
#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

use twip_graphics::Resources;
use twip_graphics::cpu::{self, Draw, Target};
use twip_graphics::font::{FontData, FontId};
use twip_graphics::image::{ImageData, ImageId};
use twip_graphics::kurbo::{self, Affine, PathEl, Shape};
use twip_graphics::paint::{BlendMode, Color, ImagePaint, Paint};
use twip_graphics::scene::{Clip, GlyphRun, Picture, Recorder, Synthesis};

use super::{ChromeFace, Frame, Renderer};
use crate::editing::view::View;
use crate::font::Face;
use crate::layout::fragment::{GlyphFill, ImageKey, Item};
use crate::layout::ids::FloatId;
use crate::layout::traverse::{paint, paint_float};

/// The twip-graphics side of a presentation: the fonts and images registered with it, the CPU renderer, the retained
/// page pictures, and the frame being recorded. Its memory is kept between frames.
#[derive(Default)]
pub struct Graphics {
    res: Resources,
    cpu: cpu::Renderer,
    /// Registered faces, by the face file's address, index and instance.
    fonts: HashMap<(usize, u32, u32), Option<FontId>>,
    /// The first document face drawn: UI strings use it for characters the chrome faces lack.
    fallback: Option<Rc<Face>>,
    /// The document's pictures, by key; `None` for one that isn't an image twip-graphics reads.
    images: HashMap<ImageKey, Option<ImageId>>,
    /// The document's metafiles recorded as pictures in their frames' points, with the frames' sizes, by key; `None`
    /// for one that doesn't read. The bitmaps they draw are registered in `metafile_images`.
    metafiles: HashMap<ImageKey, Option<(Picture, kurbo::Size)>>,
    metafile_images: Vec<ImageId>,
    /// Tiled vector pictures' tiles recorded, by picture, tile size and crop (as bits); `None` for one that doesn't
    /// draw.
    vector_tiles: HashMap<(ImageKey, [u64; 6]), Option<Picture>>,
    /// Tiled vector pictures drawn as pixels, by picture, tile size and crop (as bits) and flip.
    vector_patterns: HashMap<(ImageKey, [u64; 6], u8), (ImageId, u32, u32)>,
    /// The faces metafile text draws with.
    lookup: Option<crate::font::Fonts>,
    /// 8×8 pattern fills' tiles: the bits, then the foreground and background colours.
    tiles: HashMap<([u8; 8], [u8; 4], [u8; 4]), Option<ImageId>>,
    /// Pictures with a colour changed (`a:clrChange`), by picture, the colour and what it becomes.
    recoloured: HashMap<(ImageId, [u8; 4], [u8; 4]), shape::Recoloured>,
    recoloured_bytes: u64,
    /// The frame being recorded, and the pictures with a changed colour drawn since a page recording began.
    frames: u64,
    recolour_log: Vec<ImageId>,
    /// Pictures decoded to change a colour.
    recolour_decodes: u64,
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

/// A shape whose line is document ink (a border, a check box) with that colour taken through the appearance, as a rect's is.
fn ink_shape(r: &Renderer, s: &crate::layout::fragment::ShapeItem) -> crate::layout::fragment::ShapeItem {
    use crate::drawing::paint::Paint;
    let map = |c: &mut crate::drawing::colour::Rgba| {
        let rgb = r.appearance.map(u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b));
        (c.r, c.g, c.b) = ((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
    };
    let mut mapped = s.clone();
    if let Paint::Solid(c) = &mut mapped.line.paint {
        map(c);
    }
    match &mut mapped.fill {
        Paint::Solid(c) => map(c),
        Paint::Hatch { fg, bg, .. } => {
            map(fg);
            map(bg);
        }
        _ => {}
    }
    mapped
}

fn rect([x0, y0, x1, y1]: [f64; 4]) -> kurbo::Rect {
    kurbo::Rect::new(x0, y0, x1, y1)
}

/// The most tiles a tiled vector picture places one by one.
const MAX_TILES: usize = 4096;

/// A rect's outline, clockwise from its top left.
fn rect_path(r: kurbo::Rect) -> [PathEl; 5] {
    [
        PathEl::MoveTo((r.x0, r.y0).into()),
        PathEl::LineTo((r.x1, r.y0).into()),
        PathEl::LineTo((r.x1, r.y1).into()),
        PathEl::LineTo((r.x0, r.y1).into()),
        PathEl::ClosePath,
    ]
}

/// A stroke `width` wide: butt caps, miter joins at limit 4.
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

/// `text`'s width at `size` in `face`: the characters' advances added up (no shaping).
fn measure(face: &Face, text: &str, size: f64) -> f64 {
    text.chars().map(|c| face.advance(face.glyph(c).unwrap_or(0))).sum::<f64>() * face.scale(size)
}

impl Graphics {
    /// Forgets the document's pictures (a new document), their bytes too: a long session opens many documents.
    pub fn clear_images(&mut self) {
        for id in self.images.drain().filter_map(|(_, id)| id).chain(self.recoloured.drain().filter_map(|(_, r)| r.id)) {
            self.res.images.remove(id);
        }
        self.recoloured_bytes = 0;
        self.forget_metafiles();
    }

    /// The faces metafile text draws with: `fonts`'. When they change, metafiles are recorded again.
    pub fn set_fonts(&mut self, fonts: &crate::font::Fonts) {
        if self.lookup.as_ref().map(crate::font::Fonts::revision) != Some(fonts.revision()) {
            self.lookup = Some(fonts.fork());
            self.forget_metafiles();
            self.retained.clear();
        }
    }

    fn forget_metafiles(&mut self) {
        self.metafiles.clear();
        self.vector_tiles.clear();
        self.vector_patterns.clear();
        for id in self.metafile_images.drain(..) {
            self.res.images.remove(id);
        }
    }

    /// The metafile `key` as a picture in its frame's points, recorded on first use, with the frame's size.
    fn metafile(&mut self, key: ImageKey, bytes: &[u8]) -> Option<(Picture, kurbo::Size)> {
        if let Some(m) = self.metafiles.get(&key) {
            return m.clone();
        }
        let size = crate::render::metafile::size(bytes);
        let mut rec = Recorder::new();
        let drawn = size.filter(|_| crate::render::metafile::play(&mut svg::Recording::new(&mut rec, Some(self)), bytes));
        let m = drawn.map(|size| (rec.finish(), size));
        self.metafiles.insert(key, m.clone());
        m
    }

    /// A vector picture tiled across `area` (the shape's box, in its space): the first tile at (`x`, `y`), `w` × `h`
    /// each, the crop cut from each, every other one mirrored as `flip` says. The tile is recorded once and placed
    /// tile by tile; past [`MAX_TILES`] tiles (each then under a sixty-fourth of the area across) it is drawn once,
    /// with its mirrored neighbours, into pixels at four a point, and repeated as a pattern.
    #[allow(clippy::too_many_arguments, reason = "the picture, the tiling and the area it covers")]
    fn vector_tiles(
        &mut self,
        rec: &mut Recorder,
        key: ImageKey,
        images: &[Vec<u8>],
        (x, y, w, h): (f64, f64, f64, f64),
        crop: [f64; 4],
        flip: crate::drawing::paint::TileFlip,
        area: kurbo::Rect,
    ) {
        use crate::drawing::paint::TileFlip;
        if !(w > 1e-6 && h > 1e-6 && w.is_finite() && h.is_finite()) {
            return;
        }
        let size = [w, h, crop[0], crop[1], crop[2], crop[3]].map(f64::to_bits);
        let tile = match self.vector_tiles.get(&(key, size)) {
            Some(t) => t.clone(),
            None => {
                let mut tile = Recorder::new();
                let drawn = self.vector(&mut tile, key, images, kurbo::Rect::new(0.0, 0.0, w, h), crop);
                let t = drawn.then(|| tile.finish());
                self.vector_tiles.insert((key, size), t.clone());
                t
            }
        };
        let Some(tile) = tile else { return };
        // Tile (i, j)'s transform: mirrored across its middle on the odd columns or rows `flip` names.
        let place = |i: i64, j: i64| {
            let (fx, fy) = match flip {
                TileFlip::X => (i % 2 != 0, false),
                TileFlip::Y => (false, j % 2 != 0),
                TileFlip::Xy => (i % 2 != 0, j % 2 != 0),
                TileFlip::None => (false, false),
            };
            let mirror =
                Affine::new([if fx { -1.0 } else { 1.0 }, 0.0, 0.0, if fy { -1.0 } else { 1.0 }, if fx { w } else { 0.0 }, if fy { h } else { 0.0 }]);
            Affine::translate((x + i as f64 * w, y + j as f64 * h)) * mirror
        };
        let (i0, i1) = (((area.x0 - x) / w).floor(), ((area.x1 - x) / w).ceil());
        let (j0, j1) = (((area.y0 - y) / h).floor(), ((area.y1 - y) / h).ceil());
        if (i1 - i0) * (j1 - j0) <= MAX_TILES as f64 {
            for j in j0 as i64..j1 as i64 {
                for i in i0 as i64..i1 as i64 {
                    rec.picture(&tile, place(i, j));
                }
            }
            return;
        }
        // ponytail: past MAX_TILES tiles the fill is pixels, a raster pattern in PDF too: placing each would make the
        // backends draw without bound (a tile can be a hundredth of a point). A mesh of tiles in one picture, placed
        // once per row, would keep it vector while bounding the commands, not the drawing.
        let cache = (key, size, flip as u8);
        if let Some(&pattern) = self.vector_patterns.get(&cache) {
            return Self::pattern_fill(rec, pattern, area, (x, y, w, h));
        }
        // Two tiles by two: one period of every flip.
        let k = (4.0f64).min(256.0 / w.max(h));
        let (pw, ph) = (((2.0 * w * k).ceil() as u32).max(1), ((2.0 * h * k).ceil() as u32).max(1));
        let mut mosaic = Recorder::new();
        mosaic.push_transform(Affine::scale(k));
        for (i, j) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
            mosaic.picture(&tile, place(i, j) * Affine::translate((-x, -y)));
        }
        mosaic.pop();
        let mosaic = mosaic.finish();
        let mut px = vec![0u8; pw as usize * ph as usize * 4];
        let Ok(mut target) = Target::new(&mut px, pw, ph, pw as usize * 4, twip_graphics::cpu::Format::Rgba8) else { return };
        let draws = [Draw { picture: &mosaic, transform: Affine::IDENTITY }];
        self.cpu.render(&mut target, &self.res, &cpu::Frame { clear: Some(Color::TRANSPARENT), draws: &draws });
        // The registry takes straight colour.
        for p in px.as_chunks_mut::<4>().0 {
            let a = u32::from(p[3]);
            if a != 0 && a != 255 {
                for c in &mut p[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
        let Ok(image) = self.res.images.add_pixels(pw, ph, px) else { return };
        self.metafile_images.push(image);
        self.vector_patterns.insert(cache, (image, pw, ph));
        Self::pattern_fill(rec, (image, pw, ph), area, (x, y, w, h));
    }

    /// `area` filled with `image` (`pw` × `ph` pixels), a two-by-two mosaic of `w` × `h` tiles, repeated from (`x`,
    /// `y`).
    fn pattern_fill(rec: &mut Recorder, (image, pw, ph): (ImageId, u32, u32), area: kurbo::Rect, (x, y, w, h): (f64, f64, f64, f64)) {
        let transform = Affine::translate((x, y)) * Affine::scale_non_uniform(2.0 * w / f64::from(pw), 2.0 * h / f64::from(ph));
        let paint = Paint::new(twip_graphics::paint::Brush::Pattern { image, transform, sampling: twip_graphics::paint::Sampling::Linear });
        rec.fill_rect(area, paint);
    }

    /// Tests: the metafile `bytes` drawn on the CPU over white into `w` × `h` pixels, its text in `fonts`: RGBA.
    #[cfg(test)]
    pub(crate) fn metafile_pixels(bytes: &[u8], w: u32, h: u32, fonts: Option<&crate::font::Fonts>) -> Option<Vec<[u8; 4]>> {
        let mut g = Graphics::default();
        if let Some(f) = fonts {
            g.set_fonts(f);
        }
        let mut rec = Recorder::new();
        let key = ImageKey { index: Some(0), generation: 0 };
        let images = [bytes.to_vec()];
        if !g.vector(&mut rec, key, &images, kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h)), [0.0; 4]) {
            return None;
        }
        let picture = rec.finish();
        let mut px = vec![0u8; w as usize * h as usize * 4];
        let mut target = Target::new(&mut px, w, h, w as usize * 4, twip_graphics::cpu::Format::Rgba8).ok()?;
        g.render_picture(&mut target, &picture, Affine::IDENTITY, Color::WHITE);
        Some(px.as_chunks::<4>().0.to_vec())
    }

    /// Draws the picture `key` as vectors when it is an SVG or a metafile: the part `crop` leaves (the fractions cut
    /// off its left, top, right and bottom) filling `dst`, clipped to it. False when it is neither, or doesn't read.
    fn vector(&mut self, rec: &mut Recorder, key: ImageKey, images: &[Vec<u8>], dst: kurbo::Rect, crop: [f64; 4]) -> bool {
        let Some(bytes) = key.index.and_then(|i| images.get(i)) else { return false };
        let Some(kind) = crate::docx::image::vector(bytes) else { return false };
        let (kw, kh) = (1.0 - crop[0] - crop[2], 1.0 - crop[1] - crop[3]);
        if !(kw > 1e-9 && kh > 1e-9) {
            return true;
        }
        // The whole picture's box, the cropped part of which is `dst`.
        let (fw, fh) = (dst.width() / kw, dst.height() / kh);
        let full = kurbo::Rect::new(dst.x0 - crop[0] * fw, dst.y0 - crop[1] * fh, dst.x0 - crop[0] * fw + fw, dst.y0 - crop[1] * fh + fh);
        match kind {
            crate::docx::image::Vector::Svg => {
                let cropped = crop != [0.0; 4];
                if cropped {
                    rec.push_clip(Clip::Rect { rect: dst, aa: true });
                }
                let drawn = svg::draw(rec, bytes, full);
                if cropped {
                    rec.pop();
                }
                drawn
            }
            crate::docx::image::Vector::Emf(_) | crate::docx::image::Vector::Wmf(_) => {
                let Some((picture, size)) = self.metafile(key, bytes) else { return false };
                // Clipped to the box only when it draws outside it: a clip edge on a fill's edge anti-aliases it twice.
                let frame = kurbo::Rect::new(0.0, 0.0, size.width, size.height).inflate(size.width * 1e-6, size.height * 1e-6);
                let inside = crop == [0.0; 4] && picture.bounds().is_some_and(|b| frame.contains_rect(b));
                if !inside {
                    rec.push_clip(Clip::Rect { rect: dst, aa: true });
                }
                rec.picture(&picture, Affine::translate((full.x0, full.y0)) * Affine::scale_non_uniform(fw / size.width, fh / size.height));
                if !inside {
                    rec.pop();
                }
                true
            }
        }
    }

    /// Records a frame: the visible pages, then the view's overlays over them, in device pixels. What the host draws
    /// over it goes into [`Graphics::frame`]; [`Graphics::present`] renders it.
    pub fn draw(&mut self, r: &Renderer, view: &View, images: &[Vec<u8>], f: &Frame) {
        self.trim_recoloured(shape::RECOLOUR_MAX_BYTES);
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
                let inputs = retained::Inputs { gridlines: r.gridlines, appearance: r.appearance, page_fill: r.page_fill_generation() };
                let picture = kept.picture(page, inputs, |content| {
                    self.recolour_log.clear();
                    self.page(content, r, page, images);
                    std::mem::take(&mut self.recolour_log)
                });
                rec.picture(&picture, Affine::IDENTITY);
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

    /// Hands the recorded frame (over the desk colour), the resources it draws from and the CPU renderer (for a frame
    /// the GPU can't draw) to `draw`, as [`Graphics::present`] does to the CPU renderer, and starts the next one: the
    /// GPU screen's present.
    #[cfg_attr(not(twip_gpu), allow(dead_code, reason = "no GPU screen in this build"))]
    pub fn finish<R>(&mut self, draw: impl FnOnce(&Resources, &cpu::Frame<'_>, &mut cpu::Renderer) -> R) -> R {
        let picture = self.frame.finish();
        let draws = [Draw { picture: &picture, transform: Affine::IDENTITY }];
        draw(&self.res, &cpu::Frame { clear: self.clear, draws: &draws }, &mut self.cpu)
    }

    /// PDF export: one page item recorded into `rec`, in the page's points.
    pub fn record_item(&mut self, rec: &mut Recorder, r: &Renderer, item: &Item, images: &[Vec<u8>]) {
        self.item(rec, r, false, item, images);
    }

    /// The fonts and images recorded pictures draw with.
    pub fn resources(&self) -> &Resources {
        &self.res
    }

    /// Tests: `page`'s content as a picture in its points, recorded now.
    #[doc(hidden)]
    pub fn page_picture(&mut self, r: &Renderer, page: &crate::layout::fragment::PageFragment, images: &[Vec<u8>]) -> twip_graphics::scene::Picture {
        let mut rec = Recorder::new();
        self.recolour_log.clear();
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
        self.page_fill(rec, r, [page.w, page.h], images);
        let mut at: Option<(f64, f64)> = None;
        paint(page, &mut |p| {
            if at != Some((p.dx, p.dy)) {
                if at.is_some() {
                    rec.pop();
                }
                rec.push_transform(Affine::translate((p.dx, p.dy)));
                at = Some((p.dx, p.dy));
            }
            // A shape with a reflection and text over it: the reflection holds the text.
            if let Item::Shape(s) = p.item
                && s.effects.reflection.is_some()
                && !s.picture
                && (s.float | s.text_float) != 0
            {
                let own = p.own_colours;
                let items: Vec<Item> = paint_float(page, FloatId(if s.float != 0 { s.float } else { s.text_float }))
                    .into_iter()
                    .filter(|i| matches!(i, Item::Glyphs(_) | Item::Turned(_)))
                    .collect();
                let (dx, dy) = (p.dx, p.dy);
                self.shape_with_text(
                    rec,
                    s,
                    images,
                    Some(&mut |g, rec, back| {
                        rec.push_transform(back * Affine::translate((-dx, -dy)));
                        for item in &items {
                            g.item(rec, r, own, item, images);
                        }
                        rec.pop();
                    }),
                );
                return;
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
    /// at the sum of the advances before it.
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
            Item::Rect { rect: rc, colour, snap: false } => rec.fill_rect(rect(*rc), ink(*colour)),
            Item::Rect { rect: rc, colour, snap: true } => rec.fill_rect(rect(*rc), aliased(ink(*colour))),
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
                let fill = match g.paint.as_deref() {
                    None => Some(g.colour.unwrap_or(0)),
                    Some(p) => {
                        // Effect copies aren't text: drawing with text (a PDF), they are paths.
                        for &(dx, dy, c) in p.under.iter() {
                            rec.push_transform(Affine::translate((dx, dy)));
                            if self.with_text {
                                rec.glyph_paths(&self.res.fonts, &run, ink(c));
                            } else {
                                rec.glyphs(&self.res.fonts, &run, ink(c));
                            }
                            rec.pop();
                        }
                        match p.fill {
                            GlyphFill::Solid(c) => Some(c),
                            GlyphFill::None => None,
                        }
                    }
                };
                if self.with_text {
                    let text: String = g.chars.iter().map(|c| c.0).collect();
                    let clusters = clusters(g).unwrap_or_default();
                    match fill {
                        Some(c) => rec.glyphs_text(&self.res.fonts, &run, &text, &clusters, ink(c)),
                        // Hollow text keeps its characters.
                        None => rec.invisible_text(&self.res.fonts, &run, &text, &clusters),
                    }
                } else if let Some(c) = fill {
                    rec.glyphs(&self.res.fonts, &run, ink(c));
                }
                if let Some((width, c)) = g.paint.as_deref().and_then(|p| p.outline) {
                    rec.stroke_glyphs(&self.res.fonts, &run, &kurbo::Stroke::new(width), ink(c));
                }
            }
            Item::Gridline { line: [x0, y0, x1, y1] } => {
                if r.gridlines {
                    rec.line((*x0, *y0).into(), (*x1, *y1).into(), &dashed(0.75, 2.25, 1.5), ink(0x9D_C3E6));
                }
            }
            Item::Shape(s) if s.ink && !own && !r.appearance.is_identity() => self.shape(rec, &ink_shape(r, s), images),
            Item::Shape(s) => self.shape(rec, s, images),
            Item::Turned(t) => {
                rec.push_transform(Affine::rotate_about(t.deg.to_radians(), kurbo::Point::new(t.centre[0], t.centre[1])));
                for i in &t.items {
                    self.item(rec, r, own, i, images);
                }
                rec.pop();
            }
            Item::Image { rect: rc, key, .. } => {
                if self.vector(rec, *key, images, rect(*rc), [0.0; 4]) {
                    return;
                }
                match self.image(*key, images) {
                    Some(id) => rec.image(id, None, rect(*rc), &ImagePaint::default()),
                    // A picture that isn't an image twip-graphics reads (WMF), or a placeholder without data: a light
                    // frame, a CSS pixel wide at 100%.
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
/// and fainter. Only what shows around the paper is drawn (the paper is opaque and drawn over it),
/// in whole-pixel pieces: the strips above and below the paper through the rounded rect, the
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

/// A run's pieces, `(origin, first character, characters)` (its characters share their piece's origin), and each
/// glyph's piece: the glyphs from one origin to the next are that piece's. `None` when the origins or the glyphs go
/// back.
pub(crate) fn glyph_pieces(r: &crate::layout::fragment::GlyphRun) -> Option<(Vec<(f64, usize, usize)>, Vec<usize>)> {
    let mut pieces: Vec<(f64, usize, usize)> = Vec::new();
    for (i, &(_, x, _)) in r.chars.iter().enumerate() {
        match pieces.last_mut() {
            Some(p) if p.0 == x => p.2 += 1,
            Some(p) if p.0 > x => return None,
            _ => pieces.push((x, i, 1)),
        }
    }
    let piece_of: Vec<usize> = r.glyphs.iter().map(|g| pieces.partition_point(|p| p.0 <= g.1 + 1e-6).saturating_sub(1)).collect();
    if piece_of.windows(2).any(|w| w[1] < w[0]) {
        return None;
    }
    Some((pieces, piece_of))
}

/// Each glyph's cluster (a UTF-8 offset into the run's characters): one cluster per character when a piece's glyph
/// and character counts match ([`glyph_pieces`]), else one cluster for the piece.
pub(crate) fn clusters(r: &crate::layout::fragment::GlyphRun) -> Option<Vec<u32>> {
    if r.glyphs.is_empty() || r.chars.is_empty() {
        return None;
    }
    let (pieces, piece_of) = glyph_pieces(r)?;
    let mut byte = Vec::with_capacity(r.chars.len());
    let mut at = 0u32;
    for &(c, _, _) in r.chars.iter() {
        byte.push(at);
        at += c.len_utf8() as u32;
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
mod ink_tests {
    use super::*;
    use crate::render::appearance::{Appearance, PageMode};

    #[test]
    fn a_stroked_border_takes_the_appearance_like_a_rect() {
        use crate::drawing::{
            colour::Rgba,
            geom::Geom,
            paint::{Paint, ResolvedEffects, ResolvedLine},
        };
        let geom = Geom { paths: vec![], text_rect: [0.0; 4], handles: vec![], cxn: vec![] };
        let line = ResolvedLine { width: 1.0, paint: Paint::Solid(Rgba { r: 0, g: 0, b: 0, a: 255 }), ..ResolvedLine::none() };
        let s = crate::layout::fragment::ShapeItem {
            rect: [0.0; 4],
            rot: 0.0,
            flip_h: false,
            flip_v: false,
            geom: std::rc::Rc::new(geom),
            fill: Paint::None,
            line,
            effects: ResolvedEffects::default(),
            behind: false,
            float: 0,
            path: Vec::new(),
            picture: false,
            ink: true,
            decoration: true,
            snap: false,
            clip_bottom: None,
            text_float: 0,
            of: Default::default(),
            text_path: None,
        };
        let r = Renderer { appearance: Appearance::Dark { page: PageMode::Dark }, ..Renderer::default() };
        let want = r.appearance.map(0x000000);
        assert_ne!(want, 0x000000, "black is lightened on the dark page");
        let Paint::Solid(c) = ink_shape(&r, &s).line.paint else { panic!() };
        assert_eq!(u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b), want);
    }
}
