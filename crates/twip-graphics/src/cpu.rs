//! The CPU backend: replays pictures into a caller's premultiplied RGBA8 or BGRA8 buffer.
//!
//! It is the reference renderer (golden tests compare against it, and the GPU backend is checked against it), the
//! fallback when no GPU is available, and the headless path (`frame_rgba`, page thumbnails, PNGs).
//!
//! Commands are painted in order, straight into the target: no tiles. Every draw is coverage × source → blend:
//!
//! - Coverage: a solid or shaded rect is covered analytically (exact area, in 1/256 pixel), text from cached glyph
//!   masks, everything else through the coverage rasteriser, all scaled by the clip (a fixed-point device rect, plus a
//!   coverage mask for path clips and rotated rect clips).
//! - Source: one premultiplied colour, or a shader (gradients, images, patterns, with colour filters; `shade.rs`).
//! - Blend: the kernels in `blend.rs`, every composite mode.
//!
//! A layer draws into an offscreen buffer of its own, which then goes through its filters (erode, blur) and is
//! composited with its opacity, colour filters and blend mode, through the clip. Its content is drawn up to the
//! filters' reach outside the clip, so a blur near the clip's edge pulls in what is just outside, as Skia sizes a
//! filtered layer.
//!
//! The same frame always gives the same bytes: on every SIMD level, and whether a picture is drawn nested or its
//! commands inline under the same transforms.

pub(crate) mod blend;
pub(crate) mod shade;

use std::fmt;

use fearless_simd::Level;

use crate::Resources;
use crate::cover::{Clip, Mask, apply_clip, factor, interior, max_scale, overlap, pixel_bounds, rect_path, rrect_path};
use crate::font::{GlyphCache, GlyphKey, Style, embolden_em, luminance_level};
use crate::fx::FastMap;
use crate::geom::{Point, Rect, Transform};
use crate::image::{ImageId, ImageRegistry, Mips};
use crate::paint::{BlendMode, Color, Extend, PremulColor, Sampling};
use crate::path::Verb;
use crate::raster::{IRect, ONE, Rasterizer, fixed};
use crate::scene::{BrushRec, ClipRec, Cmd, Filter, ImageRec, LayerRec, Lists, Picture, RunRec};
use blend::Solid;
use shade::{Kind, Shader};

/// The order of a pixel's four premultiplied 8-bit channels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Format {
    #[default]
    Rgba8,
    Bgra8,
}

/// A caller's pixel buffer: `height` rows of `width` premultiplied pixels, each row `stride` bytes after the last.
#[derive(Debug)]
pub struct Target<'a> {
    data: &'a mut [u8],
    width: u32,
    height: u32,
    stride: usize,
    format: Format,
    /// The device pixel the buffer's first pixel is (a layer's buffer covers part of the frame).
    origin: (i32, i32),
}

/// A buffer too small for the size it was given as, or a stride narrower than a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetError {
    pub len: usize,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let TargetError { len, width, height, stride } = self;
        write!(f, "a {len}-byte buffer can't hold {width}×{height} pixels at a stride of {stride} bytes")
    }
}

impl std::error::Error for TargetError {}

impl<'a> Target<'a> {
    pub fn new(data: &'a mut [u8], width: u32, height: u32, stride: usize, format: Format) -> Result<Target<'a>, TargetError> {
        let error = TargetError { len: data.len(), width, height, stride };
        let row = width as usize * 4;
        let needed = match height {
            0 => Some(0),
            h => stride.checked_mul(h as usize - 1).and_then(|n| n.checked_add(row)),
        };
        // Coordinates are i32 inside the renderer.
        let fits = width <= i32::MAX as u32 && height <= i32::MAX as u32;
        match needed {
            Some(n) if fits && stride >= row && n <= data.len() => Ok(Target { data, width, height, stride, format, origin: (0, 0) }),
            _ => Err(error),
        }
    }

    fn bounds(&self) -> IRect {
        let (x, y) = self.origin;
        IRect { x0: x, y0: y, x1: x + self.width as i32, y1: y + self.height as i32 }
    }

    /// `len` pixels of device row `y` from device column `x`, which the caller keeps inside the target.
    fn pixels(&mut self, y: i32, x: i32, len: usize) -> &mut [u8] {
        let at = (y - self.origin.1) as usize * self.stride + (x - self.origin.0) as usize * 4;
        &mut self.data[at..at + len * 4]
    }

    /// The buffer from device pixel (`x`, `y`) on, and its stride: a block whose rows the caller keeps inside.
    fn block(&mut self, y: i32, x: i32) -> (&mut [u8], usize) {
        let at = (y - self.origin.1) as usize * self.stride + (x - self.origin.0) as usize * 4;
        (&mut self.data[at..], self.stride)
    }

    fn bgra(&self) -> bool {
        self.format == Format::Bgra8
    }

    fn channels(&self, c: PremulColor) -> [u8; 4] {
        match self.format {
            Format::Rgba8 => [c.r, c.g, c.b, c.a],
            Format::Bgra8 => [c.b, c.g, c.r, c.a],
        }
    }
}

/// A picture placed in the frame: `transform` maps its local space to the target's pixels.
#[derive(Clone, Copy, Debug)]
pub struct Draw<'a> {
    pub picture: &'a Picture,
    pub transform: kurbo::Affine,
}

/// What a frame draws: the target cleared to `clear` (or left as it is), then `draws` in order.
#[derive(Clone, Copy, Debug, Default)]
pub struct Frame<'a> {
    pub clear: Option<Color>,
    pub draws: &'a [Draw<'a>],
}

/// What the last frame couldn't draw: image draws whose image didn't decode (or isn't registered), and glyphs whose
/// face or outline didn't read. Each draws nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub failed_images: u32,
    pub failed_glyphs: u32,
}

/// What a push saved, for its pop to restore.
#[derive(Clone, Copy, Debug)]
struct Saved {
    transform: Transform,
    clip: Clip,
    masks: usize,
}

/// A decoded image: its mip chain, or `None` when it didn't decode.
#[derive(Debug)]
struct Decoded {
    mips: Option<Mips>,
    used: u64,
}

/// A colour glyph's mask: premultiplied colour in the target's channel order, `width × height`, its top left at
/// (`left`, `top`) from the glyph origin's pixel.
#[derive(Debug, Default)]
pub(crate) struct ColourMask {
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    used: u64,
}

/// A colour mask is for a glyph mask key, the text colour (opaque: its alpha modulates the mask when drawn; the
/// foreground palette entry takes it) and the channel order.
type ColourKey = (GlyphKey, [u8; 4], bool);

/// Where a draw's colour comes from: one colour, or a shader (whose `Solid` carries only the blend and channel order).
#[derive(Clone, Copy)]
enum Source<'a> {
    Solid(Solid),
    Shade(&'a Shader<'a>, Solid),
}

/// The scratch a draw writes through: the clip masks, the coverage row and the shaded colours.
struct Scratch<'a> {
    level: Level,
    masks: &'a [Mask],
    row: &'a mut Vec<u8>,
    colors: &'a mut Vec<[u8; 4]>,
    columns: &'a mut Vec<u32>,
}

impl Scratch<'_> {
    /// Blends `src` into the row of pixels from (`x`, `y`) at coverage `cov` (already clipped).
    fn paint(&mut self, target: &mut Target<'_>, y: i32, x: i32, cov: &[u8], src: Source<'_>) {
        let dst = target.pixels(y, x, cov.len());
        match src {
            Source::Solid(s) => blend::mask(self.level, dst, cov, s),
            Source::Shade(shader, s) => {
                self.colors.resize(cov.len(), [0; 4]);
                shader.row(self.level, y, x, self.colors, self.columns);
                blend::shaded(self.level, dst, cov, self.colors, s);
            }
        }
    }
}

/// The CPU renderer. It keeps its scratch memory and caches between frames, so a steady stream of frames allocates
/// nothing.
#[derive(Debug)]
pub struct Renderer {
    level: Level,
    raster: Rasterizer,
    stack: Vec<Saved>,
    /// A pool: the first `masks_used` are the open path clips'.
    masks: Vec<Mask>,
    masks_used: usize,
    /// Coverage for one row, and colours for one row.
    row: Vec<u8>,
    colors: Vec<[u8; 4]>,
    /// One row of image columns, blended vertically, for bilinear sampling.
    columns: Vec<u32>,
    /// Rects, rounded rects and glyph outlines drawn as paths.
    verbs: Vec<Verb>,
    points: Vec<Point>,
    glyphs: GlyphCache,
    /// The last text colour's luminance level: runs mostly share a colour.
    luminance: Option<(PremulColor, u8)>,
    colour_masks: FastMap<ColourKey, ColourMask>,
    colour_bytes: usize,
    images: FastMap<ImageId, Decoded>,
    image_bytes: usize,
    image_budget: usize,
    clock: u64,
    /// Layer buffers, kept for the next layer, and the filters' scratch.
    layers: Vec<Vec<u8>>,
    filter_scratch: Vec<u8>,
    stats: Stats,
}

impl Default for Renderer {
    fn default() -> Self {
        Renderer::new()
    }
}

impl Renderer {
    pub fn new() -> Renderer {
        Renderer::with_level(Level::new())
    }

    /// A renderer whose kernels run at `level`, for tests that compare SIMD levels.
    #[doc(hidden)]
    pub fn with_level(level: Level) -> Renderer {
        Renderer {
            level,
            raster: Rasterizer::new(level),
            stack: Vec::new(),
            masks: Vec::new(),
            masks_used: 0,
            row: Vec::new(),
            colors: Vec::new(),
            columns: Vec::new(),
            verbs: Vec::new(),
            points: Vec::new(),
            glyphs: GlyphCache::default(),
            luminance: None,
            colour_masks: FastMap::default(),
            colour_bytes: 0,
            images: FastMap::default(),
            image_bytes: 0,
            image_budget: 256 << 20,
            clock: 0,
            layers: Vec::new(),
            filter_scratch: Vec::new(),
            stats: Stats::default(),
        }
    }

    /// What the last frame couldn't draw.
    pub fn stats(&self) -> Stats {
        self.stats
    }

    pub fn render(&mut self, target: &mut Target<'_>, resources: &Resources, frame: &Frame<'_>) {
        self.clock += 1;
        self.stats = Stats::default();
        let full = target.bounds();
        if let Some(c) = frame.clear {
            let color = target.channels(c.premultiply());
            for y in full.y0..full.y1 {
                // Clearing replaces: a clear to transparent writes transparent pixels.
                target.pixels(y, full.x0, full.width() as usize).as_chunks_mut::<4>().0.fill(color);
            }
        }
        let clip = Clip { rect: [full.x0 * ONE, full.y0 * ONE, full.x1 * ONE, full.y1 * ONE], bounds: full, mask: None };
        for draw in frame.draws {
            self.picture(target, resources, draw.picture, Transform::from_kurbo(draw.transform), clip);
        }
        self.glyphs.trim();
        self.trim_images();
        self.trim_colour_masks();
    }

    fn picture(&mut self, target: &mut Target<'_>, res: &Resources, picture: &Picture, transform: Transform, clip: Clip) {
        let Some(bounds) = picture.local_bounds() else { return };
        // Glyph ink reaches up to a pixel past the bounds.
        let b = pixel_bounds(transform.map_rect(bounds));
        if (IRect { x0: b.x0 - 1, y0: b.y0 - 1, x1: b.x1 + 1, y1: b.y1 + 1 }).intersect(clip.bounds).is_empty() {
            return;
        }
        let mut i = 0;
        self.run(target, res, picture.lists(), &mut i, transform, clip, false);
    }

    /// Runs `lists`' commands from `*i`: to the end, or (`in_layer`) through the pop that ends the layer.
    #[allow(clippy::too_many_arguments, reason = "the walk's position and state stay explicit")]
    fn run(&mut self, target: &mut Target<'_>, res: &Resources, lists: &Lists, i: &mut usize, t: Transform, clip: Clip, in_layer: bool) {
        let base = self.stack.len();
        let (mut t, mut clip) = (t, clip);
        while let Some(&cmd) = lists.cmds.get(*i) {
            *i += 1;
            match cmd {
                Cmd::SolidRect { rect, color, aa, blend } => {
                    let src = Solid { color: target.channels(color), blend, bgra: target.bgra() };
                    self.fill_rect(target, rect, t, &clip, Source::Solid(src), aa);
                }
                Cmd::Rect { rect, paint } => {
                    self.with_paint(target, res, lists, paint, t, |r, target, src, aa| r.fill_rect(target, rect, t, &clip, src, aa))
                }
                Cmd::RRect { rrect, paint } => {
                    let (rect, radii) = lists.rrects[rrect as usize];
                    rrect_path(rect, radii, &mut self.verbs, &mut self.points);
                    self.with_paint(target, res, lists, paint, t, |r, target, src, aa| r.fill_scratch_path(target, t, &clip, src, aa));
                }
                Cmd::Path { path, paint } => {
                    let (verbs, points) = (&lists.verbs[path.verbs.range()], &lists.points[path.points.range()]);
                    self.with_paint(target, res, lists, paint, t, |r, target, src, aa| {
                        let mut s = Scratch { level: r.level, masks: &r.masks, row: &mut r.row, colors: &mut r.colors, columns: &mut r.columns };
                        fill_path(&mut r.raster, &mut s, target, verbs, points, t, &clip, src, aa);
                    });
                }
                Cmd::Glyphs { run, paint } => {
                    let run = &lists.runs[run as usize];
                    let color = match lists.paints[paint as usize].brush {
                        BrushRec::Solid(c) => Some(c),
                        _ => None,
                    };
                    self.with_paint(target, res, lists, paint, t, |r, target, src, _| r.glyphs(target, res, lists, run, color, src, t, &clip));
                }
                Cmd::Image { image } => self.image(target, res, &lists.images[image as usize], lists, t, &clip),
                Cmd::Picture { child, transform } => {
                    let child_transform = lists.transforms[transform as usize].then(t);
                    self.picture(target, res, &lists.children[child as usize], child_transform, clip);
                }
                Cmd::PushTransform { transform } => {
                    self.stack.push(Saved { transform: t, clip, masks: self.masks_used });
                    t = lists.transforms[transform as usize].then(t);
                }
                Cmd::PushClip { clip: index } => {
                    self.stack.push(Saved { transform: t, clip, masks: self.masks_used });
                    clip = self.push_clip(lists, &lists.clips[index as usize], t, clip);
                }
                Cmd::PushLayer { layer } => self.layer(target, res, lists, i, lists.layers[layer as usize], t, clip),
                Cmd::Pop => {
                    if self.stack.len() > base {
                        if let Some(saved) = self.stack.pop() {
                            (t, clip, self.masks_used) = (saved.transform, saved.clip, saved.masks);
                        }
                    } else if in_layer {
                        return;
                    }
                }
            }
        }
        // A finished picture is balanced; this only matters if it weren't.
        if let Some(saved) = self.stack.get(base) {
            self.masks_used = saved.masks;
        }
        self.stack.truncate(base);
    }

    /// Calls `draw` with paint `index`'s source (a shader built for transform `t`) and anti-aliasing. A pattern whose
    /// image didn't decode draws nothing.
    fn with_paint(
        &mut self,
        target: &mut Target<'_>,
        res: &Resources,
        lists: &Lists,
        index: u32,
        t: Transform,
        draw: impl FnOnce(&mut Renderer, &mut Target<'_>, Source<'_>, bool),
    ) {
        let paint = lists.paints[index as usize];
        let solid = Solid { color: [0; 4], blend: paint.blend, bgra: target.bgra() };
        let stops = |span: crate::scene::Span| &lists.stops[span.range()];
        let gradient = |kind: Kind<'static>, stops_span, extend, transform: Transform| {
            let inv = transform.then(t).inverse()?;
            Some(Shader { kind, ramp: shade::ramp(stops(stops_span), solid.bgra), extend, inv, filters: &[], bgra: solid.bgra })
        };
        let shader = match paint.brush {
            BrushRec::Solid(c) => return draw(self, target, Source::Solid(Solid { color: target.channels(c), ..solid }), paint.aa),
            BrushRec::Linear { start, end, stops, extend, transform } => {
                let d = Point { x: end.x - start.x, y: end.y - start.y };
                let len2 = d.x * d.x + d.y * d.y;
                let inv_len2 = if len2 > 0.0 { 1.0 / len2 } else { 0.0 };
                gradient(Kind::Linear { p0: start, d, inv_len2 }, stops, extend, transform)
            }
            BrushRec::Radial { start, start_radius, end, end_radius, stops, extend, transform } => {
                let cd = Point { x: end.x - start.x, y: end.y - start.y };
                gradient(Kind::Radial { c0: start, r0: start_radius, cd, dr: end_radius - start_radius }, stops, extend, transform)
            }
            BrushRec::Sweep { center, start_angle, end_angle, stops, extend, transform } => {
                gradient(Kind::Sweep { c: center, start: start_angle, span: end_angle - start_angle }, stops, extend, transform)
            }
            BrushRec::Pattern { image, transform, sampling } => {
                // The shader borrows the decoded image while the draw borrows the renderer, so the image comes out of
                // the cache for the draw and goes back after.
                self.load(&res.images, image);
                let Some(decoded) = self.images.remove(&image) else { return };
                match (decoded.mips.as_ref(), transform.then(t).inverse()) {
                    (Some(mips), Some(inv)) => {
                        let (pixmap, scale) = mip_level(mips, inv, sampling);
                        let kind = Kind::Image { pixmap, scale, sampling, repeat: true };
                        let shader = Shader { kind, ramp: [[0; 4]; 256], extend: Extend::Repeat, inv, filters: &[], bgra: solid.bgra };
                        draw(self, target, Source::Shade(&shader, solid), paint.aa);
                    }
                    (None, _) => self.stats.failed_images += 1,
                    (Some(_), None) => {}
                }
                self.images.insert(image, decoded);
                return;
            }
        };
        if let Some(shader) = shader {
            draw(self, target, Source::Shade(&shader, solid), paint.aa);
        }
    }

    /// Decodes `id` into the cache if it isn't there.
    fn load(&mut self, images: &ImageRegistry, id: ImageId) {
        let clock = self.clock;
        if let Some(d) = self.images.get_mut(&id) {
            d.used = clock;
            return;
        }
        let mips = images.decode(id).ok().map(Mips::new);
        self.image_bytes += mips.as_ref().map_or(0, Mips::bytes);
        self.images.insert(id, Decoded { mips, used: clock });
    }

    /// Drops the least recently drawn images while the cache is over its budget (after a frame), down to half of it.
    fn trim_images(&mut self) {
        if self.image_bytes <= self.image_budget {
            return;
        }
        let mut by_age: Vec<(u64, ImageId)> = self.images.iter().map(|(k, d)| (d.used, *k)).collect();
        by_age.sort_unstable();
        for (_, id) in by_age {
            if self.image_bytes <= self.image_budget / 2 {
                break;
            }
            if let Some(d) = self.images.remove(&id) {
                self.image_bytes -= d.mips.as_ref().map_or(0, Mips::bytes);
            }
        }
    }

    /// Draws an image (or its source rect) into its destination rect, filtered by its colour filters.
    fn image(&mut self, target: &mut Target<'_>, res: &Resources, rec: &ImageRec, lists: &Lists, t: Transform, clip: &Clip) {
        self.load(&res.images, rec.image);
        let Some(decoded) = self.images.remove(&rec.image) else { return };
        let drawn = (|| {
            let mips = decoded.mips.as_ref()?;
            let base = &mips.levels[0];
            let src = rec.src.unwrap_or(Rect { x0: 0.0, y0: 0.0, x1: base.width as f32, y1: base.height as f32 });
            let (sw, sh) = (src.x1 - src.x0, src.y1 - src.y0);
            if sw <= 0.0 || sh <= 0.0 {
                return Some(());
            }
            let (sx, sy) = ((rec.dst.x1 - rec.dst.x0) / sw, (rec.dst.y1 - rec.dst.y0) / sh);
            let image_to_local = Transform { a: sx, b: 0.0, c: 0.0, d: sy, e: rec.dst.x0 - src.x0 * sx, f: rec.dst.y0 - src.y0 * sy };
            let inv = image_to_local.then(t).inverse()?;
            let (pixmap, scale) = mip_level(mips, inv, rec.sampling);
            let bgra = target.bgra();
            let filters = &lists.color_filters[rec.color_filters.range()];
            let kind = Kind::Image { pixmap, scale, sampling: rec.sampling, repeat: false };
            let shader = Shader { kind, ramp: [[0; 4]; 256], extend: Extend::Clamp, inv, filters, bgra };
            let solid = Solid { color: [0; 4], blend: BlendMode::SrcOver, bgra };
            self.fill_rect(target, rec.dst, t, clip, Source::Shade(&shader, solid), true);
            Some(())
        })();
        if drawn.is_none() && decoded.mips.is_none() {
            self.stats.failed_images += 1;
        }
        self.images.insert(rec.image, decoded);
    }

    fn fill_rect(&mut self, target: &mut Target<'_>, rect: Rect, t: Transform, clip: &Clip, src: Source<'_>, aa: bool) {
        if !t.is_scale_translate() {
            rect_path(rect, &mut self.verbs, &mut self.points);
            return self.fill_scratch_path(target, t, clip, src, aa);
        }
        let r = t.map_rect(rect);
        // Without anti-aliasing an edge rounds to the nearest pixel boundary, as Skia's aliased rects do.
        let q = |v: f32| if aa { fixed(v) } else { fixed((v + 0.5).floor()) };
        let [x0, y0, x1, y1] = [q(r.x0).max(clip.rect[0]), q(r.y0).max(clip.rect[1]), q(r.x1).min(clip.rect[2]), q(r.y1).min(clip.rect[3])];
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let px = IRect { x0: x0.div_euclid(ONE), y0: y0.div_euclid(ONE), x1: (x1 + ONE - 1).div_euclid(ONE), y1: (y1 + ONE - 1).div_euclid(ONE) }
            .intersect(clip.bounds);
        if px.is_empty() {
            return;
        }
        let mut s = Scratch { level: self.level, masks: &self.masks, row: &mut self.row, colors: &mut self.colors, columns: &mut self.columns };
        for y in px.y0..px.y1 {
            let cy = overlap(y0, y1, y);
            if let (None, Source::Solid(solid)) = (clip.mask, src) {
                let (left, right) = (px.x0, px.x1 - 1);
                blend::uniform(s.level, target.pixels(y, left, 1), factor(overlap(x0, x1, left), cy), solid);
                if right > left {
                    if right > left + 1 {
                        blend::uniform(s.level, target.pixels(y, left + 1, (right - left - 1) as usize), factor(ONE, cy), solid);
                    }
                    blend::uniform(s.level, target.pixels(y, right, 1), factor(overlap(x0, x1, right), cy), solid);
                }
                continue;
            }
            let len = px.width() as usize;
            let mut row = std::mem::take(s.row);
            row.clear();
            row.extend((px.x0..px.x1).map(|x| factor(overlap(x0, x1, x), cy)));
            if let Some(m) = clip.mask {
                blend::scale(s.level, &mut row, s.masks[m].row(y, px.x0, len));
            }
            s.paint(target, y, px.x0, &row, src);
            *s.row = row;
        }
    }

    /// Fills the path in `self.verbs` and `self.points`.
    fn fill_scratch_path(&mut self, target: &mut Target<'_>, t: Transform, clip: &Clip, src: Source<'_>, aa: bool) {
        let mut s = Scratch { level: self.level, masks: &self.masks, row: &mut self.row, colors: &mut self.colors, columns: &mut self.columns };
        fill_path(&mut self.raster, &mut s, target, &self.verbs, &self.points, t, clip, src, aa);
    }

    /// Draws a glyph run from cached masks, at any transform, or (when its em is over 256 pixels across, as Skia
    /// draws such text) from its outlines as paths. `color` is the run's solid colour, which picks the mask gamma
    /// (a shaded run takes the middle level).
    #[allow(clippy::too_many_arguments, reason = "a run, its colour and its frame state")]
    fn glyphs(
        &mut self,
        target: &mut Target<'_>,
        res: &Resources,
        lists: &Lists,
        run: &RunRec,
        color: Option<PremulColor>,
        src: Source<'_>,
        t: Transform,
        clip: &Clip,
    ) {
        let fonts = &res.fonts;
        let glyphs = &lists.glyphs[run.glyphs.range()];
        // The em-to-device matrix: the run's size, then the transform's linear part.
        let m = [t.a, t.b, t.c, t.d].map(|v| (v * run.size) as f64);
        let det = m[0] * m[3] - m[1] * m[2];
        if !(run.size > 0.0 && det != 0.0 && det.is_finite()) {
            return;
        }
        let style = Style { skew_x: run.synthesis.skew_x, bold: if run.synthesis.embolden { embolden_em(run.size) } else { 0.0 } };
        if max_scale(m) > 256.0 {
            let em = kurbo::Affine::new([run.size as f64, 0.0, 0.0, run.size as f64, 0.0, 0.0]);
            for g in glyphs {
                self.verbs.clear();
                self.points.clear();
                let map = kurbo::Affine::translate((g.x as f64, run.baseline as f64)) * em;
                match self.glyphs.outliner.glyph(fonts, run.font, g.id, style, map, &mut self.verbs, &mut self.points) {
                    Ok(true) => self.fill_scratch_path(target, t, clip, src, true),
                    Ok(false) => {}
                    Err(()) => self.stats.failed_glyphs += 1,
                }
            }
            return;
        }
        let lum = match (color, self.luminance) {
            (None, _) => 4,
            (Some(c), Some((last, lum))) if c == last => lum,
            (Some(c), _) => {
                let lum = luminance_level(c.r, c.g, c.b, c.a);
                self.luminance = Some((c, lum));
                lum
            }
        };
        let matrix = m.map(|v| (v * 64.0).round() as i32);
        let key = GlyphKey { font: run.font, glyph: 0, matrix, bin: [0, 0], skew: style.skew_x.to_bits(), bold: style.bold.to_bits(), lum };
        let strike = self.glyphs.strike(key);
        // Glyph origins land on quarter pixels; an axis-aligned run's baseline rounds to a whole pixel, as in Skia.
        let axis_aligned = t.b == 0.0 && t.c == 0.0;
        let inside = interior(clip);
        for g in glyphs {
            let o = t.map_point(Point { x: g.x, y: run.baseline });
            let qx = (o.x * 4.0).round() as i32;
            let qy = if axis_aligned { o.y.round() as i32 * 4 } else { (o.y * 4.0).round() as i32 };
            let bin = [qx.rem_euclid(4) as u8, qy.rem_euclid(4) as u8];
            let glyph_key = GlyphKey { glyph: g.id, bin, ..key };
            let mask = self.glyphs.mask_in(fonts, &mut self.raster, strike, g.id, bin);
            if mask.failed {
                self.stats.failed_glyphs += 1;
            }
            if mask.colour {
                let fg = color.unwrap_or(PremulColor { r: 0, g: 0, b: 0, a: 255 });
                self.colour_glyph(target, res, glyph_key, fg, src, (qx.div_euclid(4), qy.div_euclid(4)), clip);
                continue;
            }
            if mask.width == 0 {
                continue;
            }
            let (x0, y0) = (qx.div_euclid(4) + mask.left, qy.div_euclid(4) + mask.top);
            let r = IRect { x0, y0, x1: x0 + mask.width as i32, y1: y0 + mask.height as i32 };
            let vis = r.intersect(clip.bounds);
            if vis.is_empty() {
                continue;
            }
            let len = vis.width() as usize;
            let stride = mask.width as usize;
            let first = (vis.y0 - r.y0) as usize * stride + (vis.x0 - r.x0) as usize;
            // Inside the clip's fully covered pixels with no mask, a solid run's coverage goes straight to the kernel.
            if let (true, Source::Solid(solid)) = (clip.mask.is_none() && vis.intersect(inside) == vis, src) {
                let (dst, dst_stride) = target.block(vis.y0, vis.x0);
                blend::mask_block(self.level, dst, dst_stride, &mask.data[first..], stride, len, (vis.y1 - vis.y0) as usize, solid);
                continue;
            }
            let mut s = Scratch { level: self.level, masks: &self.masks, row: &mut self.row, colors: &mut self.colors, columns: &mut self.columns };
            for row in vis.y0..vis.y1 {
                let at = first + (row - vis.y0) as usize * stride;
                let mut cov = std::mem::take(s.row);
                cov.clear();
                cov.extend_from_slice(&mask.data[at..at + len]);
                apply_clip(s.level, clip, s.masks, row, vis.x0, &mut cov);
                s.paint(target, row, vis.x0, &cov, src);
                *s.row = cov;
            }
        }
    }

    /// Draws a colour glyph (COLR or bitmap) from its colour mask, made now if it isn't cached; `origin` is the glyph
    /// origin's pixel. The text colour's alpha modulates the mask.
    #[allow(clippy::too_many_arguments, reason = "a glyph, its colour and its frame state")]
    fn colour_glyph(
        &mut self,
        target: &mut Target<'_>,
        res: &Resources,
        key: GlyphKey,
        fg: PremulColor,
        src: Source<'_>,
        origin: (i32, i32),
        clip: &Clip,
    ) {
        let ck = self.cache_colour_mask(res, key, fg, target.format);
        let Some(mask) = self.colour_masks.get(&ck) else { return };
        if mask.width == 0 {
            return;
        }
        let (x0, y0) = (origin.0 + mask.left, origin.1 + mask.top);
        let r = IRect { x0, y0, x1: x0 + mask.width as i32, y1: y0 + mask.height as i32 };
        let vis = r.intersect(clip.bounds);
        let solid = match src {
            Source::Solid(s) | Source::Shade(_, s) => Solid { color: [0; 4], ..s },
        };
        let len = vis.width().max(0) as usize;
        for y in vis.y0..vis.y1 {
            let at = ((y - r.y0) as usize * mask.width as usize + (vis.x0 - r.x0) as usize) * 4;
            self.colors.clear();
            self.colors.extend(
                mask.data[at..at + len * 4].as_chunks::<4>().0.iter().map(
                    |&p| {
                        if fg.a == 255 { p } else { p.map(|v| crate::paint::mul_div255(v, fg.a)) }
                    },
                ),
            );
            self.row.clear();
            self.row.resize(len, 255);
            apply_clip(self.level, clip, &self.masks, y, vis.x0, &mut self.row);
            blend::shaded(self.level, target.pixels(y, vis.x0, len), &self.row, &self.colors, solid);
        }
    }

    /// Colour glyph `key`'s mask for text colour `fg` (its alpha left out: it modulates the mask when drawn), in
    /// `format`'s channel order, made now if it isn't cached.
    #[cfg(feature = "gpu")]
    pub(crate) fn colour_mask(&mut self, res: &Resources, key: GlyphKey, fg: PremulColor, format: Format) -> &ColourMask {
        let ck = self.cache_colour_mask(res, key, fg, format);
        &self.colour_masks[&ck]
    }

    /// Makes colour glyph `key`'s mask for `fg` and `format` if it isn't cached, marks it used this frame, and gives
    /// its key.
    fn cache_colour_mask(&mut self, res: &Resources, key: GlyphKey, fg: PremulColor, format: Format) -> ColourKey {
        let opaque = unpremultiply(fg).with_alpha(255);
        let ck: ColourKey = (key, [opaque.r, opaque.g, opaque.b, 255], format == Format::Bgra8);
        if !self.colour_masks.contains_key(&ck) {
            let mask = self.make_colour_mask(res, key, opaque, format).unwrap_or_default();
            self.colour_bytes += mask.data.len();
            self.colour_masks.insert(ck, mask);
        }
        let clock = self.clock;
        if let Some(mask) = self.colour_masks.get_mut(&ck) {
            mask.used = clock;
        }
        ck
    }

    /// Ends a frame for a renderer kept only for its colour masks: drops the least recently drawn over the budget,
    /// and starts the next frame's clock.
    #[cfg(feature = "gpu")]
    pub(crate) fn end_colour_frame(&mut self) {
        self.trim_colour_masks();
        self.clock += 1;
    }

    /// Draws colour glyph `key` (its picture, or its bitmap) into a new colour mask; `None` when it has neither.
    fn make_colour_mask(&mut self, res: &Resources, key: GlyphKey, fg: Color, format: Format) -> Option<ColourMask> {
        let [a, b, c, d] = key.matrix.map(|v| v as f64 / 64.0);
        let map = kurbo::Affine::new([a, b, c, d, key.bin[0] as f64 / 4.0, key.bin[1] as f64 / 4.0]);
        let picture = crate::colour::colr_picture(&res.fonts, key.font, key.glyph, fg);
        let bitmap = match picture {
            Some(_) => None,
            None => Some(crate::colour::bitmap(&res.fonts, key.font, key.glyph, max_scale([a, b, c, d]) as f32, fg)?),
        };
        let em = match (&picture, &bitmap) {
            (Some(p), _) => p.bounds()?,
            (None, Some((_, place))) => *place,
            (None, None) => return None,
        };
        let dev = map.transform_rect_bbox(em);
        let bounds = IRect { x0: dev.x0.floor() as i32, y0: dev.y0.floor() as i32, x1: dev.x1.ceil() as i32, y1: dev.y1.ceil() as i32 };
        let (w, h) = (bounds.width().max(0) as usize, (bounds.y1 - bounds.y0).max(0) as usize);
        // An em at most 256 pixels across paints far less than this; anything larger is a broken glyph.
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return None;
        }
        let mut data = vec![0u8; w * h * 4];
        {
            let mut t = Target { data: &mut data, width: w as u32, height: h as u32, stride: w * 4, format, origin: (bounds.x0, bounds.y0) };
            let clip = Clip { rect: [bounds.x0 * ONE, bounds.y0 * ONE, bounds.x1 * ONE, bounds.y1 * ONE], bounds, mask: None };
            let transform = Transform::from_kurbo(map);
            match (picture, bitmap) {
                (Some(p), _) => self.picture(&mut t, res, &p, transform, clip),
                (None, Some((pixels, place))) => {
                    // The bitmap as an image filling its placement, from the mip level for the glyph's size.
                    let mips = Mips::new(pixels);
                    let base = &mips.levels[0];
                    let (sx, sy) = ((place.width() / base.width as f64) as f32, (place.height() / base.height as f64) as f32);
                    let image_to_em = Transform { a: sx, b: 0.0, c: 0.0, d: sy, e: place.x0 as f32, f: place.y0 as f32 };
                    let inv = image_to_em.then(transform).inverse()?;
                    let (pixmap, scale) = mip_level(&mips, inv, Sampling::Linear);
                    let kind = Kind::Image { pixmap, scale, sampling: Sampling::Linear, repeat: false };
                    let bgra = format == Format::Bgra8;
                    let shader = Shader { kind, ramp: [[0; 4]; 256], extend: Extend::Clamp, inv, filters: &[], bgra };
                    let solid = Solid { color: [0; 4], blend: BlendMode::SrcOver, bgra };
                    self.fill_rect(&mut t, Rect::from_kurbo(place), transform, &clip, Source::Shade(&shader, solid), true);
                }
                (None, None) => return None,
            }
        }
        Some(ColourMask { left: bounds.x0, top: bounds.y0, width: w as u32, height: h as u32, data, used: 0 })
    }

    /// Drops the least recently drawn colour masks while they are over 16 MB (after a frame), down to half of it.
    fn trim_colour_masks(&mut self) {
        const BUDGET: usize = 16 << 20;
        if self.colour_bytes <= BUDGET {
            return;
        }
        let mut by_age: Vec<(u64, ColourKey)> = self.colour_masks.iter().map(|(k, m)| (m.used, *k)).collect();
        by_age.sort_unstable_by_key(|(used, _)| *used);
        for (_, key) in by_age {
            if self.colour_bytes <= BUDGET / 2 {
                break;
            }
            if let Some(m) = self.colour_masks.remove(&key) {
                self.colour_bytes -= m.data.len();
            }
        }
    }

    /// Draws a layer: its commands (from `*i`, through the pop that ends it) into a buffer of its own, then its
    /// filters, then the buffer composited through `clip`.
    #[allow(clippy::too_many_arguments, reason = "the walk's position and state stay explicit")]
    fn layer(&mut self, target: &mut Target<'_>, res: &Resources, lists: &Lists, i: &mut usize, rec: LayerRec, t: Transform, clip: Clip) {
        let Some(LayerRegion { region, rect, scale }) = layer_region(lists, &rec, t, &clip) else {
            skip_layer(lists, i);
            return;
        };
        let filters = &lists.filters[rec.filters.range()];
        let color_filters = &lists.color_filters[rec.color_filters.range()];
        let bgra = target.bgra();
        let out = region.intersect(clip.bounds);
        let (w, h) = (region.width() as usize, (region.y1 - region.y0) as usize);
        let mut buffer = self.layers.pop().unwrap_or_default();
        buffer.clear();
        buffer.resize(w * h * 4, 0);
        {
            let mut layer =
                Target { data: &mut buffer, width: w as u32, height: h as u32, stride: w * 4, format: target.format, origin: (region.x0, region.y0) };
            let inner = Clip { rect, bounds: region, mask: None };
            self.run(&mut layer, res, lists, i, t, inner, true);
        }
        for f in filters {
            match *f {
                Filter::Blur { sigma } => shade::blur(self.level, &mut buffer, w, h, sigma.abs() * scale, &mut self.filter_scratch),
                Filter::Erode { radius } => {
                    shade::erode(self.level, &mut buffer, w, h, (radius.abs() * scale).round() as usize, &mut self.filter_scratch)
                }
            }
        }
        let opacity = (rec.opacity * 255.0 + 0.5) as u8;
        let solid = Solid { color: [0; 4], blend: rec.blend, bgra };
        let len = out.width() as usize;
        for y in out.y0..out.y1 {
            let at = ((y - region.y0) as usize * w + (out.x0 - region.x0) as usize) * 4;
            self.colors.clear();
            self.colors.extend(buffer[at..at + len * 4].as_chunks::<4>().0.iter().map(|&p| {
                let p = if color_filters.is_empty() { p } else { shade::filter_in_order(p, color_filters, bgra) };
                if opacity == 255 { p } else { p.map(|c| crate::paint::mul_div255(c, opacity)) }
            }));
            self.row.clear();
            self.row.resize(len, 255);
            apply_clip(self.level, &clip, &self.masks, y, out.x0, &mut self.row);
            blend::shaded(self.level, target.pixels(y, out.x0, len), &self.row, &self.colors, solid);
        }
        self.layers.push(buffer);
    }

    fn push_clip(&mut self, lists: &Lists, rec: &ClipRec, t: Transform, clip: Clip) -> Clip {
        let scratch = (&mut self.verbs, &mut self.points);
        crate::cover::push_clip(&mut self.raster, self.level, (&mut self.masks, &mut self.masks_used), scratch, lists, rec, t, clip)
    }
}

/// A premultiplied colour unpremultiplied (rounded).
fn unpremultiply(c: PremulColor) -> Color {
    if c.a == 0 {
        return Color::TRANSPARENT;
    }
    let un = |v: u8| ((v as u32 * 255 + c.a as u32 / 2) / c.a as u32).min(255) as u8;
    Color::rgba(un(c.r), un(c.g), un(c.b), c.a)
}

/// Where a layer draws: its pixels (`region`), its content's clip rect inside them (1/256 pixel), and the largest
/// factor its transform stretches lengths by (which scales its filters).
#[derive(Clone, Copy, Debug)]
pub(crate) struct LayerRegion {
    pub region: IRect,
    pub rect: [i32; 4],
    pub scale: f32,
}

/// Where layer `rec` under `t` and `clip` draws: within its filters' reach of the clip, inside its bounds, and (when
/// compositing a transparent pixel leaves the destination as it was) within that reach of its content. `None` when
/// it draws nothing, or would need a buffer over 2²⁸ pixels.
pub(crate) fn layer_region(lists: &Lists, rec: &LayerRec, t: Transform, clip: &Clip) -> Option<LayerRegion> {
    let scale = max_scale([t.a, t.b, t.c, t.d].map(|v| v as f64)) as f32;
    let reach: f32 = lists.filters[rec.filters.range()]
        .iter()
        .map(|f| match *f {
            Filter::Blur { sigma } => 3.0 * sigma.abs() * scale,
            Filter::Erode { radius } => radius.abs() * scale,
        })
        .sum();
    // The filters' reach, and the pixel glyph ink can reach past its bounds.
    let margin = reach.ceil() as i32 + 1;
    let mut rect = [clip.rect[0] - margin * ONE, clip.rect[1] - margin * ONE, clip.rect[2] + margin * ONE, clip.rect[3] + margin * ONE];
    let mut shrink = |r: Rect, by: i32| {
        let r = t.map_rect(r);
        let m = by * ONE;
        rect = [rect[0].max(fixed(r.x0) - m), rect[1].max(fixed(r.y0) - m), rect[2].min(fixed(r.x1) + m), rect[3].min(fixed(r.y1) + m)];
    };
    if let Some(b) = rec.bounds {
        shrink(b, 0);
    }
    // Where compositing a transparent pixel leaves the destination as it was, only what the layer draws (spread by its
    // filters) needs a buffer. Otherwise (a mode such as source-in, or a colour filter that colours transparent
    // pixels) the layer acts on all of its region. A transparent pixel stays transparent in either channel order.
    let color_filters = &lists.color_filters[rec.color_filters.range()];
    if keeps_destination(rec.blend) && shade::filter_in_order([0; 4], color_filters, false) == [0; 4] {
        shrink(rec.content?, margin);
    }
    let region = IRect {
        x0: rect[0].div_euclid(ONE),
        y0: rect[1].div_euclid(ONE),
        x1: (rect[2] + ONE - 1).div_euclid(ONE),
        y1: (rect[3] + ONE - 1).div_euclid(ONE),
    };
    let fits = region.width() as i64 * (region.y1 - region.y0) as i64 <= 1 << 28;
    (!region.is_empty() && !region.intersect(clip.bounds).is_empty() && fits).then_some(LayerRegion { region, rect, scale })
}

/// Whether compositing a transparent source pixel with `mode` leaves the destination pixel as it was.
pub(crate) fn keeps_destination(mode: BlendMode) -> bool {
    !matches!(mode, BlendMode::Clear | BlendMode::Src | BlendMode::SrcIn | BlendMode::DstIn | BlendMode::SrcOut | BlendMode::DstAtop)
}

/// Moves `*i` past a layer's commands, through the pop that ends it.
pub(crate) fn skip_layer(lists: &Lists, i: &mut usize) {
    let mut depth = 0;
    while let Some(cmd) = lists.cmds.get(*i) {
        *i += 1;
        match cmd {
            Cmd::PushTransform { .. } | Cmd::PushClip { .. } | Cmd::PushLayer { .. } => depth += 1,
            Cmd::Pop if depth == 0 => return,
            Cmd::Pop => depth -= 1,
            _ => {}
        }
    }
}

/// The mip level to sample for a draw whose device-to-image transform is `inv` (no finer than an image pixel per
/// device pixel), and its scale from image pixels to level pixels in x and y. Nearest sampling reads the image itself.
fn mip_level(mips: &Mips, inv: Transform, sampling: Sampling) -> (&crate::image::Pixmap, (f32, f32)) {
    let base = &mips.levels[0];
    let p = &mips.levels[mip_index(mips.levels.len(), inv, sampling)];
    (p, (p.width as f32 / base.width as f32, p.height as f32 / base.height as f32))
}

/// The mip level (of `levels`) an image is sampled from under `inv` (device to image pixels): the base when sampled
/// nearest, else the level whose pixels are nearest to the device's without being smaller.
pub(crate) fn mip_index(levels: usize, inv: Transform, sampling: Sampling) -> usize {
    if sampling == Sampling::Nearest {
        return 0;
    }
    // Image pixels per device pixel, the larger of the two directions.
    let k = (inv.a * inv.a + inv.b * inv.b).sqrt().max((inv.c * inv.c + inv.d * inv.d).sqrt());
    if k > 1.0 { (k.log2().floor() as usize).min(levels - 1) } else { 0 }
}

#[allow(clippy::too_many_arguments, reason = "the borrows of the renderer's fields stay apart")]
fn fill_path(
    raster: &mut Rasterizer,
    s: &mut Scratch<'_>,
    target: &mut Target<'_>,
    verbs: &[Verb],
    points: &[Point],
    t: Transform,
    clip: &Clip,
    src: Source<'_>,
    aa: bool,
) {
    raster.fill(verbs, points, t, clip.bounds, aa, |y, x, cov| {
        apply_clip(s.level, clip, s.masks, y, x, cov);
        s.paint(target, y, x, cov, src);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factor_rounds_area_to_255ths() {
        assert_eq!(factor(256, 256), 255);
        assert_eq!(factor(0, 256), 0);
        assert_eq!(factor(128, 256), 128);
        assert_eq!(factor(128, 128), 64);
    }

    #[test]
    fn a_steady_frame_allocates_nothing() {
        use crate::paint::{Brush, ImagePaint, Paint, Stop};
        use crate::scene::{Clip, Layer, Recorder};
        use kurbo::Shape;
        let mut resources = Resources::default();
        let tile = resources.images.add_pixels(2, 2, vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 128]).expect("pixels");
        let mut rec = Recorder::new();
        rec.fill_rect(kurbo::Rect::new(0.5, 0.5, 30.0, 20.0), Color::BLACK);
        rec.fill_rrect(kurbo::Rect::new(10.0, 10.0, 60.0, 40.0), 6.0, Color::WHITE);
        let stops = [Stop { offset: 0.0, color: Color::BLACK }, Stop { offset: 1.0, color: Color::WHITE }];
        let gradient = Brush::Linear {
            start: (0.0, 0.0).into(),
            end: (40.0, 0.0).into(),
            stops: &stops,
            extend: Extend::Mirror,
            transform: kurbo::Affine::IDENTITY,
        };
        rec.fill_rect(kurbo::Rect::new(0.0, 40.0, 60.0, 60.0), Paint::new(gradient));
        rec.image(tile, None, kurbo::Rect::new(40.0, 0.0, 60.0, 20.0), &ImagePaint::default());
        let circle = kurbo::Circle::new((32.0, 32.0), 20.0).to_path(0.01);
        rec.push_clip(Clip::Path(circle.elements()));
        rec.push_clip(Clip::Rect { rect: kurbo::Rect::new(5.5, 5.5, 50.0, 50.0), aa: true });
        rec.push_layer(&Layer { opacity: 0.5, filters: &[Filter::Blur { sigma: 2.0 }], ..Layer::default() });
        rec.fill_path(circle.elements(), Color::BLACK.with_alpha(100));
        rec.pop();
        rec.push_transform(kurbo::Affine::rotate(0.3));
        rec.fill_rect(kurbo::Rect::new(0.0, 0.0, 40.0, 10.0), Color::WHITE);
        let inner = rec.finish();
        rec.picture(&inner, kurbo::Affine::translate((3.0, 4.0)));
        let picture = rec.finish();
        let mut px = vec![0u8; 64 * 64 * 4];
        let mut renderer = Renderer::new();
        let draws = [Draw { picture: &picture, transform: kurbo::Affine::scale(1.5) }];
        let frame = Frame { clear: Some(Color::WHITE), draws: &draws };
        let mut render = || renderer.render(&mut Target::new(&mut px, 64, 64, 256, Format::Rgba8).expect("fits"), &resources, &frame);
        render();
        assert_eq!(allocation_counter::measure(render).count_total, 0);
    }

    #[test]
    fn layers_that_act_on_transparent_pixels_cover_their_whole_region() {
        use crate::paint::BlendMode;
        use crate::scene::{Layer, Recorder};
        let draw = |layer: &Layer<'_>| {
            let mut rec = Recorder::new();
            rec.push_layer(layer);
            rec.fill_rect(kurbo::Rect::new(2.0, 2.0, 4.0, 4.0), Color::BLACK);
            rec.pop();
            let picture = rec.finish();
            let mut px = vec![0u8; 8 * 8 * 4];
            let draws = [Draw { picture: &picture, transform: kurbo::Affine::IDENTITY }];
            let frame = Frame { clear: Some(Color::rgb(0xFF_0000)), draws: &draws };
            Renderer::new().render(&mut Target::new(&mut px, 8, 8, 32, Format::Rgba8).expect("fits"), &Resources::default(), &frame);
            px
        };
        let at = |px: &[u8], x: usize, y: usize| <[u8; 4]>::try_from(&px[(y * 8 + x) * 4..][..4]).expect("a pixel");
        // Source-over: the layer changes only what it draws.
        let over = draw(&Layer::default());
        assert_eq!((at(&over, 3, 3), at(&over, 6, 6)), ([0, 0, 0, 255], [255, 0, 0, 255]));
        // Destination-in keeps the destination only where the layer has drawn: everywhere else it clears.
        let dst_in = draw(&Layer { blend: BlendMode::DstIn, ..Layer::default() });
        assert_eq!((at(&dst_in, 3, 3), at(&dst_in, 6, 6), at(&dst_in, 0, 0)), ([255, 0, 0, 255], [0; 4], [0; 4]));
        // A colour filter that turns transparent pixels opaque blue colours the whole region, not just the content.
        let mut blue = [0.0; 20];
        (blue[14], blue[19]) = (1.0, 1.0);
        let filtered = draw(&Layer { color_filters: &[crate::paint::ColorFilter::Matrix(blue)], ..Layer::default() });
        assert_eq!((at(&filtered, 3, 3), at(&filtered, 7, 0)), ([0, 0, 255, 255], [0, 0, 255, 255]));
    }

    #[test]
    fn failed_images_are_counted_and_draw_nothing() {
        use crate::paint::ImagePaint;
        use crate::scene::Recorder;
        let mut resources = Resources::default();
        // An id the registry doesn't have stands for an image that didn't decode.
        let missing = crate::image::ImageId(41);
        let good = resources.images.add_pixels(1, 1, vec![0, 0, 0, 255]).expect("pixels");
        let mut rec = Recorder::new();
        rec.image(missing, None, kurbo::Rect::new(0.0, 0.0, 4.0, 4.0), &ImagePaint::default());
        rec.image(good, None, kurbo::Rect::new(4.0, 0.0, 8.0, 4.0), &ImagePaint::default());
        let picture = rec.finish();
        let mut px = vec![255u8; 8 * 4 * 4];
        let mut renderer = Renderer::new();
        let draws = [Draw { picture: &picture, transform: kurbo::Affine::IDENTITY }];
        renderer.render(&mut Target::new(&mut px, 8, 4, 32, Format::Rgba8).expect("fits"), &resources, &Frame { clear: None, draws: &draws });
        assert_eq!(renderer.stats(), Stats { failed_images: 1, failed_glyphs: 0 });
        assert_eq!(&px[..4], &[255; 4], "the missing image left the pixels alone");
        assert_eq!(&px[16..20], &[0, 0, 0, 255], "the good one drew");
    }

    #[test]
    fn malformed_images_and_fonts_are_counted() {
        use crate::font::FontData;
        use crate::scene::{GlyphRun, Recorder, Synthesis};
        let mut resources = Resources::default();
        // Images whose headers probe but whose pixel data is broken: they register, then fail to decode.
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 8, 8);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().expect("a header");
            writer.write_image_data(&(0..256).map(|i| (i * 7) as u8).collect::<Vec<_>>()).expect("pixels");
        }
        let gif = include_bytes!("../tests/images/palette.gif");
        // Part of the PNG's compressed pixels (in its one IDAT chunk, from byte 41) zeroed, so they no longer inflate; the
        // GIF cut short inside its image data.
        png[45..60].fill(0);
        let cut = [png, gif[..gif.len() - 100].to_vec()].map(|b| {
            let data: crate::image::ImageData = std::sync::Arc::new(b);
            resources.images.add(data).expect("the header probes")
        });
        // A face whose outlines are garbage: the tables parse, the glyphs don't.
        let mut bytes = include_bytes!("../tests/fonts/jetbrains-mono-500.ttf").to_vec();
        let glyf = skrifa::FontRef::new(&bytes)
            .expect("reads")
            .table_directory
            .table_records()
            .iter()
            .find(|r| r.tag() == skrifa::raw::types::Tag::new(b"glyf"))
            .map(|r| (r.offset() as usize, r.length() as usize))
            .expect("a glyf table");
        bytes[glyf.0..glyf.0 + glyf.1].fill(0xFF);
        let font = resources.fonts.add(std::sync::Arc::new(bytes) as FontData, 0, &[]).expect("the tables parse");
        let mut rec = Recorder::new();
        rec.fill_rect(kurbo::Rect::new(0.0, 0.0, 1.0, 1.0), Color::BLACK);
        for (i, id) in cut.into_iter().enumerate() {
            rec.image(id, None, kurbo::Rect::new(i as f64 * 8.0, 8.0, i as f64 * 8.0 + 8.0, 16.0), &crate::paint::ImagePaint::default());
        }
        let run = GlyphRun { font, size: 12.0, baseline: 12.0, glyphs: &[(36, 2.0)], synthesis: Synthesis::default() };
        rec.glyphs(&resources.fonts, &run, Color::BLACK);
        let picture = rec.finish();
        let mut px = vec![255u8; 16 * 16 * 4];
        let mut renderer = Renderer::new();
        let draws = [Draw { picture: &picture, transform: kurbo::Affine::IDENTITY }];
        renderer.render(&mut Target::new(&mut px, 16, 16, 64, Format::Rgba8).expect("fits"), &resources, &Frame { clear: None, draws: &draws });
        assert_eq!(renderer.stats(), Stats { failed_images: 2, failed_glyphs: 1 });
        assert!(px[16 * 8 * 4..].iter().all(|&v| v == 255), "nothing drew where the images and glyph failed");
    }

    #[test]
    fn targets_reject_short_buffers_and_narrow_strides() {
        let mut data = vec![0u8; 4 * 4 * 3];
        assert!(Target::new(&mut data, 4, 3, 16, Format::Rgba8).is_ok());
        assert!(Target::new(&mut data, 4, 3, 12, Format::Rgba8).is_err(), "stride narrower than a row");
        assert!(Target::new(&mut data, 4, 4, 16, Format::Rgba8).is_err(), "too short");
        assert!(Target::new(&mut data, 4, 0, 16, Format::Rgba8).is_ok());
        let mut padded = vec![0u8; 20 * 2 + 16];
        assert!(Target::new(&mut padded, 4, 3, 20, Format::Bgra8).is_ok(), "the last row needs no padding");
    }
}
