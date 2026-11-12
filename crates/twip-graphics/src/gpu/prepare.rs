//! A frame's preparation on the CPU: the pictures walked as the CPU backend walks them (transforms, clips, layers,
//! nested pictures, culling), each draw turned into instances in device pixels. Rects are covered analytically;
//! rounded rects, paths and glyphs by coverage in the atlas, rasterised and clipped here exactly as the CPU backend
//! computes it, so the GPU only multiplies coverage by the paint and blends. A paint the shader computes per pixel
//! (gradients through the CPU's 256-entry ramp, images from their mip chain, colour filters) carries its
//! device-to-paint transform and parameters in the instance.
//!
//! A layer draws into a texture of its own, over the region the CPU backend gives it, its filters run as passes over
//! that texture, and an instance composites it into its parent. The frame is a list of passes in the order they run:
//! a layer breaks its parent's pass, so a layer's texture is free again once it is composited, and the textures in
//! use are as many as layers nest deep.

use std::ops::Range;

use fearless_simd::Level;

use super::atlas::{Atlas, Slot};
use crate::Resources;
use crate::cover::{Clip, Mask, apply_clip, interior, max_scale, pixel_bounds, push_clip, rect_path, rrect_path};
use crate::cpu::shade::{Ramp, box_passes, gaussian, ramp};
use crate::cpu::{Frame, LayerRegion, Stats, layer_region, mip_index, skip_layer};
use crate::font::{GlyphCache, GlyphKey, Style, embolden_em, luminance_level};
use crate::fx::FastMap;
use crate::geom::{Point, Rect, Transform};
use crate::image::{ImageId, Mips};
use crate::paint::{BlendMode, ColorFilter, Extend, PremulColor, Sampling};
use crate::path::Verb;
use crate::raster::{IRect, ONE, Rasterizer, fixed};
use crate::scene::{BrushRec, Cmd, Filter, ImageRec, LayerRec, Lists, Picture, RunRec};

/// The atlas pages' size: what WebGL 2 guarantees for a texture side.
pub(super) const PAGE: u32 = 2048;

/// How an instance blends into the target, each a fixed-function blend on premultiplied colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Blend {
    /// Source-over.
    Over,
    /// Multiply, exactly, in two passes over the same instance: `(Dst, OneMinusSrcAlpha)`, which leaves the
    /// destination's alpha as it was, then `(OneMinusDstAlpha, One)`, which adds the source over the uncovered part.
    Multiply,
    /// The destination scaled by one minus the source's alpha: clear and destination-in, the shader choosing the alpha.
    Erase,
    /// Any other mode: the shader blends with a copy of the destination, as the CPU backend's general blend does, and
    /// replaces it. Each such instance draws alone, after its own copy.
    General,
}

/// A run of consecutive instances drawn with one blend, sampling one texture when they sample one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Batch {
    pub blend: Blend,
    pub source: Source,
    pub instances: Range<u32>,
}

/// The texture a batch samples: none, an image, or the texture of the layer at a nesting depth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Source {
    None,
    Image(ImageId, Sampler),
    Layer(usize),
}

/// A pass of the frame, in the order they run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Op {
    /// Batches drawn into the target (`slot` `None`) or a layer's texture (the layer's nesting depth), whose top left
    /// pixel is `region`'s; `clear` clears it first (to the frame's clear colour, or a layer's transparent).
    Draw { slot: Option<usize>, region: IRect, clear: bool, batches: Range<usize> },
    /// A filter step over a layer's `size` pixels, from its texture `read` (0 or 1) into the other.
    Filter { slot: usize, size: (u32, u32), read: usize, step: Step },
    /// `size` pixels of the target (`slot` `None`) or a layer's texture, from `from` (its pixels, not the device's),
    /// copied to the destination copy's top left, for the next instance's general blend.
    Copy { slot: Option<usize>, from: (u32, u32), size: (u32, u32) },
}

/// One filter step along rows or columns (see the filter shader).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Step {
    pub horizontal: bool,
    /// 0 a box, 1 a kernel, 2 a minimum.
    pub kind: u32,
    pub left: u32,
    pub right: u32,
    pub weights: [u32; 16],
}

/// How an image is sampled: filtering, and whether it repeats.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct Sampler {
    pub linear: bool,
    pub repeat: bool,
}

/// One instance's bytes, little-endian: quad (4 × f32), edges (4 × f32), colour (4 × u8, premultiplied), coverage kind
/// (u32), atlas page (u32), paint bits (u32), the device-to-paint transform and two paint values (8 × f32), the paint's
/// parameters (4 × f32), and its colour filters (u32: first row, then count in the top 8 bits).
pub(super) const INSTANCE: usize = 100;

/// A paint's kind, in the low bits of the paint word.
const SOLID: u32 = 0;
const LINEAR: u32 = 1;
const RADIAL: u32 = 2;
const SWEEP: u32 = 3;
const IMAGE: u32 = 4;
const LAYER: u32 = 5;
const COLOUR: u32 = 6;

/// A paint as an instance carries it.
#[derive(Clone, Copy, Debug)]
struct Paint {
    color: [u8; 4],
    /// Kind (bits 0–3), extend (4–5), ramp row or mip level (16–31); [`push`](Prepare::push) adds the blend's output
    /// bits (8–15).
    bits: u32,
    /// Device pixels to the paint's space: a, b, c, d, e, f; then two values (an image level's scale, a conical
    /// gradient's start radius and radius change).
    xform: [f32; 8],
    params: [f32; 4],
    filters: u32,
    source: Source,
    blend: BlendMode,
    aa: bool,
}

impl Paint {
    fn solid(c: PremulColor, blend: BlendMode, aa: bool) -> Paint {
        Paint { color: [c.r, c.g, c.b, c.a], bits: SOLID, xform: [0.0; 8], params: [0.0; 4], filters: 0, source: Source::None, blend, aa }
    }
}

/// An image the renderer has (or will have, once uploaded): its levels' sizes. `None` when it didn't decode.
pub(super) type Known = FastMap<ImageId, Option<Vec<(u32, u32)>>>;

/// What a push saved, for its pop to restore.
#[derive(Clone, Copy, Debug)]
struct Saved {
    transform: Transform,
    clip: Clip,
    masks: usize,
}

/// A frame's instances and the state that builds them, kept between frames.
pub(super) struct Prepare {
    level: Level,
    raster: Rasterizer,
    glyphs: GlyphCache,
    stack: Vec<Saved>,
    masks: Vec<Mask>,
    masks_used: usize,
    verbs: Vec<Verb>,
    points: Vec<Point>,
    pub atlas: Atlas,
    /// Where each glyph mask no clip cuts is in the atlas this frame.
    placed: FastMap<GlyphKey, Option<Slot>>,
    /// Colour glyphs' masks (premultiplied RGBA), and where each glyph's (for a text colour) is this frame.
    pub colour: Atlas,
    placed_colour: FastMap<(GlyphKey, [u8; 4]), Option<Slot>>,
    /// The CPU renderer, which makes and keeps colour glyphs' masks.
    cpu: crate::cpu::Renderer,
    pub instances: Vec<u8>,
    pub batches: Vec<Batch>,
    pub ops: Vec<Op>,
    /// Where the batches of the pass being built start.
    open: usize,
    /// The largest layer drawn at each nesting depth this frame.
    pub slots: Vec<(u32, u32)>,
    /// How deep the layer being drawn nests (0 outside layers).
    depth: usize,
    /// The largest destination copy this frame.
    pub dst: (u32, u32),
    /// The device's largest texture side.
    max_texture: u32,
    /// This frame's gradient ramps, one row each.
    pub ramps: Vec<Ramp>,
    /// This frame's colour filters, one row each: a kind texel, then its values (see the shader).
    pub filters: Vec<[f32; FILTER_TEXELS * 4]>,
    /// Images decoded and waiting to be uploaded; the renderer takes them.
    pub pending: Vec<(ImageId, Mips)>,
    pub known: Known,
    pub stats: Stats,
}

/// A colour filter row's texels: the kind, then a matrix (its four rows of four, then the offsets) or a table (256
/// values, four to a texel).
pub(super) const FILTER_TEXELS: usize = 65;

impl Prepare {
    pub fn new(max_texture: u32) -> Prepare {
        let level = Level::new();
        Prepare {
            level,
            raster: Rasterizer::new(level),
            glyphs: GlyphCache::default(),
            stack: Vec::new(),
            masks: Vec::new(),
            masks_used: 0,
            verbs: Vec::new(),
            points: Vec::new(),
            atlas: Atlas::new(PAGE, 1),
            placed: FastMap::default(),
            colour: Atlas::new(PAGE, 4),
            placed_colour: FastMap::default(),
            cpu: crate::cpu::Renderer::with_level(level),
            instances: Vec::new(),
            batches: Vec::new(),
            ops: Vec::new(),
            open: 0,
            slots: Vec::new(),
            depth: 0,
            dst: (0, 0),
            max_texture,
            ramps: Vec::new(),
            filters: Vec::new(),
            pending: Vec::new(),
            known: Known::default(),
            stats: Stats::default(),
        }
    }

    /// Builds `frame`'s instances for a `width × height` target.
    pub fn frame(&mut self, res: &Resources, frame: &Frame<'_>, (width, height): (u32, u32)) {
        self.instances.clear();
        self.batches.clear();
        self.ops.clear();
        self.open = 0;
        self.slots.clear();
        self.depth = 0;
        self.dst = (0, 0);
        self.ramps.clear();
        self.filters.clear();
        self.atlas.clear();
        self.placed.clear();
        self.colour.clear();
        self.placed_colour.clear();
        self.stats = Stats::default();
        let full = IRect { x0: 0, y0: 0, x1: width as i32, y1: height as i32 };
        let clip = Clip { rect: [0, 0, full.x1 * ONE, full.y1 * ONE], bounds: full, mask: None };
        self.begin(None, full, frame.clear.is_some());
        for d in frame.draws {
            self.picture(res, d.picture, Transform::from_kurbo(d.transform), clip);
        }
        self.close();
        self.glyphs.trim();
        self.cpu.end_colour_frame();
    }

    /// Starts a pass drawing into `slot`'s texture (the target when `None`), whose top left pixel is `region`'s.
    fn begin(&mut self, slot: Option<usize>, region: IRect, clear: bool) {
        self.close();
        let at = self.batches.len();
        self.ops.push(Op::Draw { slot, region, clear, batches: at..at });
        self.open = at;
    }

    /// Ends the pass being built: its batches are those since it began.
    fn close(&mut self) {
        let end = self.batches.len();
        if let Some(Op::Draw { batches, .. }) = self.ops.last_mut()
            && batches.start == self.open
        {
            batches.end = end;
        }
    }

    fn picture(&mut self, res: &Resources, picture: &Picture, t: Transform, clip: Clip) {
        let Some(bounds) = picture.local_bounds() else { return };
        // Glyph ink reaches up to a pixel past the bounds.
        let b = pixel_bounds(t.map_rect(bounds));
        if (IRect { x0: b.x0 - 1, y0: b.y0 - 1, x1: b.x1 + 1, y1: b.y1 + 1 }).intersect(clip.bounds).is_empty() {
            return;
        }
        let mut i = 0;
        self.run(res, picture.lists(), &mut i, t, clip, false);
    }

    /// Runs `lists`' commands from `*i`: to the end, or (`in_layer`) through the pop that ends the layer.
    fn run(&mut self, res: &Resources, lists: &Lists, i: &mut usize, t: Transform, clip: Clip, in_layer: bool) {
        let base = self.stack.len();
        let (mut t, mut clip) = (t, clip);
        while let Some(&cmd) = lists.cmds.get(*i) {
            *i += 1;
            match cmd {
                Cmd::SolidRect { rect, color, aa, blend } => self.rect(rect, t, &clip, &Paint::solid(color, blend, aa)),
                Cmd::Rect { rect, paint } => {
                    if let Some(p) = self.paint(res, lists, paint, t) {
                        self.rect(rect, t, &clip, &p);
                    }
                }
                Cmd::RRect { rrect, paint } => {
                    if let Some(p) = self.paint(res, lists, paint, t) {
                        let (rect, radii) = lists.rrects[rrect as usize];
                        let (mut verbs, mut points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
                        rrect_path(rect, radii, &mut verbs, &mut points);
                        self.path(&verbs, &points, t, &clip, &p);
                        (self.verbs, self.points) = (verbs, points);
                    }
                }
                Cmd::Path { path, paint } => {
                    if let Some(p) = self.paint(res, lists, paint, t) {
                        self.path(&lists.verbs[path.verbs.range()], &lists.points[path.points.range()], t, &clip, &p);
                    }
                }
                Cmd::Glyphs { run, paint } => {
                    let color = match lists.paints[paint as usize].brush {
                        BrushRec::Solid(c) => Some(c),
                        _ => None,
                    };
                    if let Some(p) = self.paint(res, lists, paint, t) {
                        self.glyphs(res, lists, &lists.runs[run as usize], color, &p, t, &clip);
                    }
                }
                Cmd::Image { image } => self.image(res, &lists.images[image as usize], lists, t, &clip),
                Cmd::Picture { child, transform } => {
                    let child_transform = lists.transforms[transform as usize].then(t);
                    self.picture(res, &lists.children[child as usize], child_transform, clip);
                }
                Cmd::PushTransform { transform } => {
                    self.stack.push(Saved { transform: t, clip, masks: self.masks_used });
                    t = lists.transforms[transform as usize].then(t);
                }
                Cmd::PushClip { clip: index } => {
                    self.stack.push(Saved { transform: t, clip, masks: self.masks_used });
                    let scratch = (&mut self.verbs, &mut self.points);
                    clip = push_clip(
                        &mut self.raster,
                        self.level,
                        (&mut self.masks, &mut self.masks_used),
                        scratch,
                        lists,
                        &lists.clips[index as usize],
                        t,
                        clip,
                    );
                }
                Cmd::PushLayer { layer } => self.layer(res, lists, i, lists.layers[layer as usize], t, clip),
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
        if let Some(saved) = self.stack.get(base) {
            self.masks_used = saved.masks;
        }
        self.stack.truncate(base);
    }

    /// A layer, as the CPU backend draws it: its commands (from `*i`, through the pop that ends it) into a texture over
    /// its region, its filters over that texture, then the texture composited through `clip` with the layer's colour
    /// filters, opacity and blend.
    fn layer(&mut self, res: &Resources, lists: &Lists, i: &mut usize, rec: LayerRec, t: Transform, clip: Clip) {
        let found = layer_region(lists, &rec, t, &clip).filter(|_| blending(rec.blend).is_some());
        let parent = match self.ops.last() {
            Some(&Op::Draw { slot, region, .. }) => Some((slot, region)),
            _ => None,
        };
        let (Some(LayerRegion { region, rect, scale }), Some((parent, parent_region))) = (found, parent) else {
            skip_layer(lists, i);
            return;
        };
        let size = (region.width() as u32, (region.y1 - region.y0) as u32);
        // shortcut: a layer wider or taller than the device's largest texture isn't drawn; drawing its region in tiles
        // lands later in the GPU backend.
        if size.0 > self.max_texture || size.1 > self.max_texture {
            skip_layer(lists, i);
            return;
        }
        let slot = self.depth;
        if self.slots.len() == slot {
            self.slots.push((0, 0));
        }
        let largest = &mut self.slots[slot];
        *largest = (largest.0.max(size.0), largest.1.max(size.1));
        self.depth += 1;
        self.begin(Some(slot), region, true);
        self.run(res, lists, i, t, Clip { rect, bounds: region, mask: None }, true);
        self.close();
        // Each filter's steps come in pairs (rows, then columns), so the result ends in the layer's first texture.
        let mut read = 0;
        let mut step = |ops: &mut Vec<Op>, step: Step| {
            ops.push(Op::Filter { slot, size, read, step });
            read ^= 1;
        };
        let along = |horizontal: bool, kind: u32, (left, right): (usize, usize), weights: [u32; 16]| Step {
            horizontal,
            kind,
            left: left as u32,
            right: right as u32,
            weights,
        };
        for f in &lists.filters[rec.filters.range()] {
            match *f {
                Filter::Blur { sigma } => {
                    let sigma = sigma.abs() * scale;
                    // Below this a blur changes nothing visible, and Skia skips it.
                    if sigma.is_nan() || sigma < 0.531_923 {
                        continue;
                    }
                    if sigma < 2.0 {
                        let (k, radius) = gaussian(sigma);
                        let mut weights = [0; 16];
                        weights[..k.len()].copy_from_slice(&k);
                        for horizontal in [true, false] {
                            step(&mut self.ops, along(horizontal, 1, (radius, radius), weights));
                        }
                        continue;
                    }
                    let passes = box_passes(sigma);
                    // Boxes of one pixel change nothing.
                    if passes[0] == (0, 0) {
                        continue;
                    }
                    for horizontal in [true, false] {
                        for reach in passes {
                            step(&mut self.ops, along(horizontal, 0, reach, [0; 16]));
                        }
                    }
                }
                Filter::Erode { radius } => {
                    let r = (radius.abs() * scale).round() as usize;
                    if r > 0 {
                        for horizontal in [true, false] {
                            step(&mut self.ops, along(horizontal, 2, (r, r), [0; 16]));
                        }
                    }
                }
            }
        }
        self.depth -= 1;
        self.begin(parent, parent_region, false);
        let color_filters = &lists.color_filters[rec.color_filters.range()];
        let first = self.filters.len() as u32;
        self.filters.extend(color_filters.iter().map(filter_row));
        let opacity = (rec.opacity * 255.0 + 0.5) as u8;
        let paint = Paint {
            color: [0; 4],
            bits: LAYER,
            xform: [0.0; 8],
            params: [region.x0 as f32, region.y0 as f32, f32::from(opacity), 0.0],
            filters: first | (color_filters.len() as u32) << 24,
            source: Source::Layer(slot),
            blend: rec.blend,
            aa: true,
        };
        let out = region.intersect(clip.bounds);
        let out = Rect { x0: out.x0 as f32, y0: out.y0 as f32, x1: out.x1 as f32, y1: out.y1 as f32 };
        self.rect(out, Transform::IDENTITY, &clip, &paint);
    }

    /// Paint `index` under `t`, as the CPU backend builds its shaders: `None` when it draws nothing (a transform that
    /// collapses space, an image that didn't decode).
    fn paint(&mut self, res: &Resources, lists: &Lists, index: u32, t: Transform) -> Option<Paint> {
        let p = lists.paints[index as usize];
        let base = Paint { blend: p.blend, aa: p.aa, ..Paint::solid(PremulColor::default(), p.blend, p.aa) };
        let gradient = |this: &mut Prepare, kind: u32, stops: crate::scene::Span, extend: Extend, transform: Transform, values: [f32; 2], params| {
            let inv = transform.then(t).inverse()?;
            let row = this.ramps.len() as u32;
            this.ramps.push(ramp(&lists.stops[stops.range()], false));
            let bits = kind | extend_bits(extend) << 4 | row << 16;
            Some(Paint { bits, xform: affine(inv, values), params, ..base })
        };
        match p.brush {
            BrushRec::Solid(c) => Some(Paint::solid(c, p.blend, p.aa)),
            BrushRec::Linear { start, end, stops, extend, transform } => {
                let d = Point { x: end.x - start.x, y: end.y - start.y };
                let len2 = d.x * d.x + d.y * d.y;
                let inv_len2 = if len2 > 0.0 { 1.0 / len2 } else { 0.0 };
                gradient(self, LINEAR, stops, extend, transform, [inv_len2, 0.0], [start.x, start.y, d.x, d.y])
            }
            BrushRec::Radial { start, start_radius, end, end_radius, stops, extend, transform } => {
                let params = [start.x, start.y, end.x - start.x, end.y - start.y];
                gradient(self, RADIAL, stops, extend, transform, [start_radius, end_radius - start_radius], params)
            }
            BrushRec::Sweep { center, start_angle, end_angle, stops, extend, transform } => {
                gradient(self, SWEEP, stops, extend, transform, [0.0, 0.0], [center.x, center.y, start_angle, end_angle - start_angle])
            }
            BrushRec::Pattern { image, transform, sampling } => {
                let inv = transform.then(t).inverse()?;
                self.image_paint(res, image, inv, sampling, true, &[], base)
            }
        }
    }

    /// An image's paint under `inv` (device to image pixels): its mip level, its sampler, and its colour filters.
    #[allow(clippy::too_many_arguments, reason = "the image, its mapping, and how it is sampled and filtered")]
    fn image_paint(
        &mut self,
        res: &Resources,
        id: ImageId,
        inv: Transform,
        sampling: Sampling,
        repeat: bool,
        filters: &[ColorFilter],
        base: Paint,
    ) -> Option<Paint> {
        let levels = self.levels(res, id)?;
        let level = mip_index(levels.len(), inv, sampling);
        let (base_w, base_h) = levels[0];
        let (w, h) = levels[level];
        let scale = [w as f32 / base_w as f32, h as f32 / base_h as f32];
        let first = self.filters.len() as u32;
        for f in filters {
            self.filters.push(filter_row(f));
        }
        let filters = first | (filters.len() as u32) << 24;
        let bits = IMAGE | (level as u32) << 16;
        let sampler = Sampler { linear: sampling == Sampling::Linear, repeat };
        Some(Paint { bits, xform: affine(inv, scale), params: [w as f32, h as f32, 0.0, 0.0], filters, source: Source::Image(id, sampler), ..base })
    }

    /// Image `id`'s levels' sizes, decoding it (to be uploaded with the frame) the first time; `None`, counted,
    /// when it doesn't decode.
    fn levels(&mut self, res: &Resources, id: ImageId) -> Option<Vec<(u32, u32)>> {
        let known = self.known.entry(id).or_insert_with(|| {
            let mips = Mips::new(res.images.decode(id).ok()?);
            let sizes = mips.levels.iter().map(|l| (l.width, l.height)).collect();
            self.pending.push((id, mips));
            Some(sizes)
        });
        let levels = known.clone();
        if levels.is_none() {
            self.stats.failed_images += 1;
        }
        levels
    }

    /// An image (or its source rect) drawn into its destination rect, filtered by its colour filters.
    fn image(&mut self, res: &Resources, rec: &ImageRec, lists: &Lists, t: Transform, clip: &Clip) {
        let Some(&(iw, ih)) = self.levels(res, rec.image).as_ref().and_then(|l| l.first()) else { return };
        let src = rec.src.unwrap_or(Rect { x0: 0.0, y0: 0.0, x1: iw as f32, y1: ih as f32 });
        let (sw, sh) = (src.x1 - src.x0, src.y1 - src.y0);
        if sw <= 0.0 || sh <= 0.0 {
            return;
        }
        let (sx, sy) = ((rec.dst.x1 - rec.dst.x0) / sw, (rec.dst.y1 - rec.dst.y0) / sh);
        let image_to_local = Transform { a: sx, b: 0.0, c: 0.0, d: sy, e: rec.dst.x0 - src.x0 * sx, f: rec.dst.y0 - src.y0 * sy };
        let Some(inv) = image_to_local.then(t).inverse() else { return };
        let filters = &lists.color_filters[rec.color_filters.range()];
        let base = Paint::solid(PremulColor::default(), BlendMode::SrcOver, true);
        if let Some(p) = self.image_paint(res, rec.image, inv, rec.sampling, false, filters, base) {
            self.rect(rec.dst, t, clip, &p);
        }
    }

    /// Appends an instance, extending the pass's last batch when it blends alike from the same texture (multiply
    /// instances draw one at a time: each one's two passes must both finish before the next instance's first).
    fn push(&mut self, quad: IRect, edges: [f32; 4], kind: u32, page: u32, paint: &Paint) {
        let Some((blend, output)) = blending(paint.blend) else { return };
        if blend == Blend::General {
            // The pass breaks for the copy of the destination under the quad, then goes on.
            let Some(&Op::Draw { slot, region, .. }) = self.ops.last() else { return };
            let size = (quad.width() as u32, (quad.y1 - quad.y0) as u32);
            self.close();
            self.ops.push(Op::Copy { slot, from: ((quad.x0 - region.x0) as u32, (quad.y0 - region.y0) as u32), size });
            self.begin(slot, region, false);
            self.dst = (self.dst.0.max(size.0), self.dst.1.max(size.1));
        }
        let n = (self.instances.len() / INSTANCE) as u32;
        let quad = [quad.x0, quad.y0, quad.x1, quad.y1].map(|v| v as f32);
        for v in quad.iter().chain(&edges) {
            self.instances.extend_from_slice(&v.to_le_bytes());
        }
        self.instances.extend_from_slice(&paint.color);
        for v in [kind, page, paint.bits | output << 8] {
            self.instances.extend_from_slice(&v.to_le_bytes());
        }
        for v in paint.xform.iter().chain(&paint.params) {
            self.instances.extend_from_slice(&v.to_le_bytes());
        }
        self.instances.extend_from_slice(&paint.filters.to_le_bytes());
        let in_pass = self.batches.len() > self.open;
        match self.batches.last_mut() {
            Some(b)
                if in_pass
                    && b.blend == blend
                    && !matches!(blend, Blend::Multiply | Blend::General)
                    && b.source == paint.source
                    && b.instances.end == n =>
            {
                b.instances.end = n + 1
            }
            _ => self.batches.push(Batch { blend, source: paint.source, instances: n..n + 1 }),
        }
    }

    /// A rect under `t`: analytic when it stays axis-aligned and no mask clips it, else as a path.
    fn rect(&mut self, rect: Rect, t: Transform, clip: &Clip, paint: &Paint) {
        if !t.is_scale_translate() || clip.mask.is_some() {
            let (mut verbs, mut points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
            rect_path(rect, &mut verbs, &mut points);
            self.path(&verbs, &points, t, clip, paint);
            (self.verbs, self.points) = (verbs, points);
            return;
        }
        let r = t.map_rect(rect);
        // Without anti-aliasing an edge rounds to the nearest pixel boundary, as Skia's aliased rects do.
        let q = |v: f32| if paint.aa { fixed(v) } else { fixed((v + 0.5).floor()) };
        let c = clip.rect;
        let [x0, y0, x1, y1] = [q(r.x0).max(c[0]), q(r.y0).max(c[1]), q(r.x1).min(c[2]), q(r.y1).min(c[3])];
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        let px = IRect { x0: x0.div_euclid(ONE), y0: y0.div_euclid(ONE), x1: (x1 + ONE - 1).div_euclid(ONE), y1: (y1 + ONE - 1).div_euclid(ONE) }
            .intersect(clip.bounds);
        if px.is_empty() {
            return;
        }
        let edges = [x0, y0, x1, y1].map(|v| v as f32 / ONE as f32);
        self.push(px, edges, 0, 0, paint);
    }

    /// A path's coverage (clipped as the CPU backend clips it) into the atlas, tile by tile, each tile one instance.
    fn path(&mut self, verbs: &[Verb], points: &[Point], t: Transform, clip: &Clip, paint: &Paint) {
        let Some(first) = points.first() else { return };
        let first = t.map_point(*first);
        let (x0, y0, x1, y1) = points
            .iter()
            .map(|&p| t.map_point(p))
            .fold((first.x, first.y, first.x, first.y), |(a, b, c, d), p| (a.min(p.x), b.min(p.y), c.max(p.x), d.max(p.y)));
        let bounds = pixel_bounds(Rect { x0, y0, x1, y1 }).intersect(clip.bounds);
        if bounds.is_empty() {
            return;
        }
        let tile = (PAGE / 2) as i32;
        for ty in (bounds.y0..bounds.y1).step_by(tile as usize) {
            for tx in (bounds.x0..bounds.x1).step_by(tile as usize) {
                let part = IRect { x0: tx, y0: ty, x1: (tx + tile).min(bounds.x1), y1: (ty + tile).min(bounds.y1) };
                let (w, h) = (part.width() as u32, (part.y1 - part.y0) as u32);
                let Some(slot) = self.atlas.alloc(w, h) else { continue };
                for row in 0..h {
                    self.atlas.row(slot, row, w).fill(0);
                }
                let Prepare { raster, atlas, masks, level, .. } = self;
                raster.fill(verbs, points, t, part, paint.aa, |y, x, cov| {
                    apply_clip(*level, clip, masks, y, x, cov);
                    let out = atlas.row(slot, (y - part.y0) as u32, w);
                    let at = (x - part.x0) as usize;
                    out[at..at + cov.len()].copy_from_slice(cov);
                });
                let edges = [part.x0 as f32, part.y0 as f32, slot.x as f32, slot.y as f32];
                self.push(part, edges, 1, slot.page, paint);
            }
        }
    }

    /// A glyph run from cached masks, as the CPU backend draws it: the em-to-device matrix, quarter-pixel bins, the
    /// baseline rounded for axis-aligned text, and a solid colour's mask gamma (a shaded run takes the middle level). A
    /// mask the clip cuts is copied with the clip's coverage applied; an uncut one is placed in the atlas once a frame
    /// and shared.
    #[allow(clippy::too_many_arguments, reason = "a run, its paint and its frame state")]
    fn glyphs(&mut self, res: &Resources, lists: &Lists, run: &RunRec, color: Option<PremulColor>, paint: &Paint, t: Transform, clip: &Clip) {
        let fonts = &res.fonts;
        let glyphs = &lists.glyphs[run.glyphs.range()];
        let m = [t.a, t.b, t.c, t.d].map(|v| (v * run.size) as f64);
        let det = m[0] * m[3] - m[1] * m[2];
        if !(run.size > 0.0 && det != 0.0 && det.is_finite()) {
            return;
        }
        let style = Style { skew_x: run.synthesis.skew_x, bold: if run.synthesis.embolden { embolden_em(run.size) } else { 0.0 } };
        if max_scale(m) > 256.0 {
            let em = kurbo::Affine::new([run.size as f64, 0.0, 0.0, run.size as f64, 0.0, 0.0]);
            for g in glyphs {
                let (mut verbs, mut points) = (std::mem::take(&mut self.verbs), std::mem::take(&mut self.points));
                verbs.clear();
                points.clear();
                let map = kurbo::Affine::translate((g.x as f64, run.baseline as f64)) * em;
                match self.glyphs.outliner.glyph(fonts, run.font, g.id, style, map, &mut verbs, &mut points) {
                    Ok(true) => self.path(&verbs, &points, t, clip, &Paint { aa: true, ..*paint }),
                    Ok(false) => {}
                    Err(()) => self.stats.failed_glyphs += 1,
                }
                (self.verbs, self.points) = (verbs, points);
            }
            return;
        }
        let lum = color.map_or(4, |c| luminance_level(c.r, c.g, c.b, c.a));
        let matrix = m.map(|v| (v * 64.0).round() as i32);
        let key = GlyphKey { font: run.font, glyph: 0, matrix, bin: [0, 0], skew: style.skew_x.to_bits(), bold: style.bold.to_bits(), lum };
        let axis_aligned = t.b == 0.0 && t.c == 0.0;
        let inside = interior(clip);
        for g in glyphs {
            let o = t.map_point(Point { x: g.x, y: run.baseline });
            let qx = (o.x * 4.0).round() as i32;
            let qy = if axis_aligned { o.y.round() as i32 * 4 } else { (o.y * 4.0).round() as i32 };
            let key = GlyphKey { glyph: g.id, bin: [qx.rem_euclid(4) as u8, qy.rem_euclid(4) as u8], ..key };
            let origin = (qx.div_euclid(4), qy.div_euclid(4));
            let mask = self.glyphs.mask(fonts, &mut self.raster, key);
            if mask.failed {
                self.stats.failed_glyphs += 1;
            }
            if mask.colour {
                self.colour_glyph(res, key, color.unwrap_or(PremulColor { r: 0, g: 0, b: 0, a: 255 }), paint.blend, origin, clip);
                continue;
            }
            if mask.width == 0 {
                continue;
            }
            let r = IRect {
                x0: origin.0 + mask.left,
                y0: origin.1 + mask.top,
                x1: origin.0 + mask.left + mask.width as i32,
                y1: origin.1 + mask.top + mask.height as i32,
            };
            let vis = r.intersect(clip.bounds);
            if vis.is_empty() {
                continue;
            }
            let uncut = clip.mask.is_none() && vis.intersect(inside) == vis;
            let slot = if uncut {
                let Prepare { placed, atlas, glyphs: cache, raster, .. } = self;
                let at = *placed.entry(key).or_insert_with(|| {
                    let mask = cache.mask(fonts, raster, key);
                    let (w, h) = (mask.width, mask.height);
                    let slot = atlas.alloc(w, h)?;
                    for row in 0..h {
                        let at = (row * w) as usize;
                        atlas.row(slot, row, w).copy_from_slice(&mask.data[at..at + w as usize]);
                    }
                    Some(slot)
                });
                at.map(|slot| Slot { page: slot.page, x: slot.x + (vis.x0 - r.x0) as u32, y: slot.y + (vis.y0 - r.y0) as u32 })
            } else {
                let (w, h) = (vis.width() as u32, (vis.y1 - vis.y0) as u32);
                let Prepare { atlas, glyphs: cache, raster, masks, level, .. } = self;
                let mask = cache.mask(fonts, raster, key);
                atlas.alloc(w, h).inspect(|&slot| {
                    for row in 0..h {
                        let y = vis.y0 + row as i32;
                        let at = (y - r.y0) as usize * mask.width as usize + (vis.x0 - r.x0) as usize;
                        let out = atlas.row(slot, row, w);
                        out.copy_from_slice(&mask.data[at..at + w as usize]);
                        apply_clip(*level, clip, masks, y, vis.x0, out);
                    }
                })
            };
            if let Some(slot) = slot {
                let edges = [vis.x0 as f32, vis.y0 as f32, slot.x as f32, slot.y as f32];
                self.push(vis, edges, 1, slot.page, paint);
            }
        }
    }
}

impl Prepare {
    /// A colour glyph (COLR or bitmap) from its colour mask, which the CPU renderer makes and keeps, placed in the
    /// colour atlas once a frame; `origin` is the glyph origin's pixel. As on the CPU, the text colour's alpha
    /// modulates the mask, and the clip's coverage covers it.
    fn colour_glyph(&mut self, res: &Resources, key: GlyphKey, fg: PremulColor, blend: BlendMode, origin: (i32, i32), clip: &Clip) {
        let Prepare { cpu, colour, placed_colour, .. } = self;
        let mask = cpu.colour_mask(res, key, fg, crate::cpu::Format::Rgba8);
        if mask.width == 0 {
            return;
        }
        let r = IRect { x0: origin.0 + mask.left, y0: origin.1 + mask.top, x1: 0, y1: 0 };
        let r = IRect { x1: r.x0 + mask.width as i32, y1: r.y0 + mask.height as i32, ..r };
        let vis = r.intersect(clip.bounds);
        if vis.is_empty() {
            return;
        }
        // shortcut: a colour mask larger than an atlas page isn't drawn; drawing it in tiles lands later in the GPU
        // backend.
        let slot = *placed_colour.entry((key, [fg.r, fg.g, fg.b, fg.a])).or_insert_with(|| {
            let slot = colour.alloc(mask.width, mask.height)?;
            let row = mask.width as usize * 4;
            for (y, src) in mask.data.chunks_exact(row).enumerate() {
                colour.row(slot, y as u32, mask.width).copy_from_slice(src);
            }
            Some(slot)
        });
        let Some(slot) = slot else { return };
        let paint = Paint {
            color: [0, 0, 0, fg.a],
            bits: COLOUR | slot.page << 16,
            xform: [0.0; 8],
            params: [r.x0 as f32, r.y0 as f32, slot.x as f32, slot.y as f32],
            filters: 0,
            source: Source::None,
            blend,
            aa: true,
        };
        let vis = Rect { x0: vis.x0 as f32, y0: vis.y0 as f32, x1: vis.x1 as f32, y1: vis.y1 as f32 };
        self.rect(vis, Transform::IDENTITY, clip, &paint);
    }
}

/// `inv` as the instance carries it, with two more values after it.
fn affine(inv: Transform, more: [f32; 2]) -> [f32; 8] {
    [inv.a, inv.b, inv.c, inv.d, inv.e, inv.f, more[0], more[1]]
}

fn extend_bits(e: Extend) -> u32 {
    match e {
        Extend::Clamp => 0,
        Extend::Repeat => 1,
        Extend::Mirror => 2,
    }
}

/// A colour filter as a row of the filter texture: texel 0 is the kind (0 a matrix, 1 a table); a matrix's rows' four
/// multipliers follow in texels 1–4 and its four offsets in texel 5; a table's 256 values follow four to a texel.
fn filter_row(f: &ColorFilter) -> [f32; FILTER_TEXELS * 4] {
    let mut row = [0.0; FILTER_TEXELS * 4];
    match f {
        ColorFilter::Matrix(m) => {
            for r in 0..4 {
                row[4 + r * 4..8 + r * 4].copy_from_slice(&m[r * 5..r * 5 + 4]);
                row[20 + r] = m[r * 5 + 4];
            }
        }
        ColorFilter::Table(t) => {
            row[0] = 1.0;
            for (i, &v) in t.iter().enumerate() {
                row[4 + i] = f32::from(v);
            }
        }
    }
    row
}

/// How `mode` blends, and the shader's output bits for it (bits 8–9: 0 the paint, 1 destination-in's erase, 2
/// clear's; bits 10–15: the mode, for a general blend): source-over and multiply blend the paint, destination-in
/// erases by one minus its alpha, clear by all, and the other modes blend in the shader. `None` for destination,
/// which leaves the destination as it was.
fn blending(mode: BlendMode) -> Option<(Blend, u32)> {
    match mode {
        BlendMode::SrcOver => Some((Blend::Over, 0)),
        BlendMode::Multiply => Some((Blend::Multiply, 0)),
        BlendMode::DstIn => Some((Blend::Erase, 1)),
        BlendMode::Clear => Some((Blend::Erase, 2)),
        BlendMode::Dst => None,
        mode => Some((Blend::General, (mode as u32) << 2)),
    }
}
